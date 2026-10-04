//! The hook that stops a call, set through Lua's C API.
//!
//! mlua's own hook empties the stack of the frame it fires in before it
//! raises, and Lua runs the `__close` methods of that frame right then,
//! inside the hook, where every hook is off. One `<close>` variable whose
//! method spins would never stop. This hook raises from the frame as it
//! stands, so Lua closes those variables as the error unwinds, with the
//! hook on again, and a method that spins stops at the next look.
//!
//! Lua copies a thread's hook to every coroutine the thread makes, so the
//! one hook set on the main thread covers every coroutine too, and no
//! wrapped function has to move it.
//!
//! A coroutine the hook stopped dies with the hook off for good, since
//! Lua turns it back on only where a protected call catches the error.
//! Its `__close` methods would then run unhooked when something closes
//! it, so the wrapped `coroutine.close` asks [`stopped_dead`] first and
//! leaves such a coroutine as it is.
#![allow(unsafe_code)]

use std::ffi::{c_int, c_void, CStr};
use std::sync::Arc;

use mlua::{ffi, Lua, Value};

use crate::limits::{At, Limits, HOOK_EVERY, INTERNAL_CHUNK};

/// Where the registry keeps the address of the limits the hook reads.
static LIMITS_KEY: u8 = 0;

/// The value a stop raises: a light userdata at this address, which no
/// script can make. A dead coroutine that holds it as its error is one
/// the hook stopped.
static STOP_VALUE: u8 = 0;

fn address(of: &'static u8) -> *mut c_void {
    std::ptr::from_ref(of).cast_mut().cast()
}

/// Keeps the limits the registry points at alive as long as the state.
/// mlua drops app data only after it closes the state.
struct Held {
    _limits: Arc<Limits>,
}

/// Point the registry at `limits` and set the hook on the main thread.
pub(crate) fn install(lua: &Lua, limits: &Arc<Limits>) -> mlua::Result<()> {
    lua.set_app_data(Held {
        _limits: Arc::clone(limits),
    });
    let limits = Arc::as_ptr(limits).cast_mut().cast::<c_void>();
    // SAFETY: `exec_raw` hands over the state it runs on, the main
    // thread outside any callback, with room on its stack for the one
    // value pushed here, which `lua_rawsetp` pops. The address stays
    // valid while the state lives, since `Held` keeps the limits alive
    // until after mlua closes the state.
    unsafe {
        lua.exec_raw::<()>((), |state| {
            ffi::lua_pushlightuserdata(state, limits);
            ffi::lua_rawsetp(state, ffi::LUA_REGISTRYINDEX, address(&LIMITS_KEY));
            ffi::lua_sethook(state, Some(hook), ffi::LUA_MASKCOUNT, HOOK_EVERY as c_int);
        })
    }
}

/// The hook. Lua calls it every [`HOOK_EVERY`] instructions on any
/// thread of the state. It raises the stop when [`Limits::on_hook`] asks
/// for one, straight from the frame it fires in.
unsafe extern "C-unwind" fn hook(state: *mut ffi::lua_State, ar: *mut ffi::lua_Debug) {
    // SAFETY: Lua calls the hook with a live state and its debug record.
    // `should_stop` returns before the error, so no Rust value that
    // needs dropping lives across the jump `lua_error` makes. Pushing a
    // light userdata never allocates, and a hook always has room on
    // the stack for it.
    unsafe {
        if should_stop(state, ar) {
            ffi::lua_pushlightuserdata(state, address(&STOP_VALUE));
            ffi::lua_error(state);
        }
    }
}

/// True when the call running now must stop.
///
/// # Safety
/// `state` and `ar` are what Lua handed the hook.
unsafe fn should_stop(state: *mut ffi::lua_State, ar: *mut ffi::lua_Debug) -> bool {
    // SAFETY: the registry entry is the light userdata `install` put
    // there, which points at limits `Held` keeps alive. Reading it
    // pushes one value, and the pop takes it off again. The pop
    // lowers the top to where it stood when the hook began, above
    // every slot of the running function, so it closes no variable.
    let limits = unsafe {
        ffi::lua_rawgetp(state, ffi::LUA_REGISTRYINDEX, address(&LIMITS_KEY));
        let limits = ffi::lua_touserdata(state, -1).cast::<Limits>().cast_const();
        ffi::lua_pop(state, 1);
        limits.as_ref()
    };
    let Some(limits) = limits else {
        return false;
    };
    // SAFETY: `ar` is the hook's own record and `state` the thread it
    // fires on.
    limits.on_hook(|| unsafe { place_of_hook(state, ar).or_else(|| place_on_stack(state)) })
}

/// Where the hook fired, when that is in your Lua.
///
/// # Safety
/// `state` and `ar` are what Lua handed the hook.
unsafe fn place_of_hook(state: *mut ffi::lua_State, ar: *mut ffi::lua_Debug) -> Option<At> {
    // SAFETY: inside a hook `lua_getinfo` reads the function the hook
    // fired in through `ar`.
    unsafe {
        ffi::lua_getinfo(state, c"Sl".as_ptr(), ar);
        place_of(&*ar)
    }
}

/// The nearest place on the stack of `state` that stands in your Lua.
///
/// # Safety
/// `state` is a live thread that runs no other code meanwhile.
unsafe fn place_on_stack(state: *mut ffi::lua_State) -> Option<At> {
    (0..64).find_map(|level| {
        // SAFETY: a zeroed record is what `lua_getstack` fills, and
        // `lua_getinfo` reads the level it filled in.
        unsafe {
            let mut ar: ffi::lua_Debug = std::mem::zeroed();
            if ffi::lua_getstack(state, level, &mut ar) == 0 {
                return None;
            }
            ffi::lua_getinfo(state, c"Sl".as_ptr(), &mut ar);
            place_of(&ar)
        }
    })
}

/// The chunk and line a debug record names, unless it names Vosh's own
/// Lua or a C function.
///
/// # Safety
/// `ar` was filled by `lua_getinfo` with `S` and `l`.
unsafe fn place_of(ar: &ffi::lua_Debug) -> Option<At> {
    let line = u32::try_from(ar.currentline)
        .ok()
        .filter(|line| *line > 0)?;
    if ar.source.is_null() {
        return None;
    }
    // SAFETY: Lua keeps the chunk name as a string that ends in a NUL.
    let source = unsafe { CStr::from_ptr(ar.source) }
        .to_string_lossy()
        .into_owned();
    if source == INTERNAL_CHUNK {
        return None;
    }
    Some(At { source, line })
}

/// True when `value` is a coroutine the hook stopped: dead on an error,
/// and that error the stop.
pub(crate) fn stopped_dead(value: &Value) -> bool {
    let Value::Thread(thread) = value else {
        return false;
    };
    let state = thread.to_pointer().cast_mut().cast::<ffi::lua_State>();
    // SAFETY: a thread's pointer is its state, which lives while the
    // value does. A dead coroutine runs nothing, and its error is the
    // top value of its stack.
    unsafe {
        let status = ffi::lua_status(state);
        status != ffi::LUA_OK
            && status != ffi::LUA_YIELD
            && ffi::lua_gettop(state) > 0
            && ffi::lua_touserdata(state, -1) == address(&STOP_VALUE)
    }
}

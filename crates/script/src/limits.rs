//! The limits every call into Lua runs under. Lua runs on the session
//! loop, so a call that never returns holds back every game line after
//! it, and a flood of sends overflows the game's input buffer.
//!
//! A call gets [`TIME_BUDGET`] and [`CALL_MEMORY`] more than the state
//! held when it began, never past [`STATE_MEMORY`] in all, and may queue
//! [`ACTIONS_PER_CALL`] actions. Rust decides a stop, never the script.
//! A hook counts instructions and checks the clock every [`HOOK_EVERY`],
//! and a stop sets a flag that `pcall`, `xpcall` and the coroutine
//! functions rethrow while it holds, so no protected loop can catch the
//! stop and spin on.
//!
//! mlua drops a hook on any thread but the one it was set on, so a
//! coroutine would run free. The wrapped `coroutine.resume`,
//! `coroutine.close` and `coroutine.wrap` point the hook at the
//! coroutine while it runs and back at the thread that resumed it after.
//!
//! The hook cannot look inside one C function, so a Lua pattern that
//! backtracks for a long time in `string.find` runs past the budget.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use mlua::{Debug, Function, HookTriggers, Lua, MultiValue, Value, VmState};

/// How long one call may run.
pub(crate) const TIME_BUDGET: Duration = Duration::from_millis(100);
/// How many instructions run between two looks at the clock.
pub(crate) const HOOK_EVERY: u32 = 10_000;
/// One megabyte, the unit the stop lines count memory in.
pub(crate) const MB: usize = 1024 * 1024;
/// How much more memory one call may hold than the state held when it
/// began.
pub(crate) const CALL_MEMORY: usize = 32 * MB;
/// How much memory all the Lua together may hold.
pub(crate) const STATE_MEMORY: usize = 128 * MB;
/// How many actions one call may queue. The rest drop.
pub(crate) const ACTIONS_PER_CALL: usize = 100;

/// The chunk name of Vosh's own Lua, which a stop never points at.
pub(crate) const INTERNAL_CHUNK: &str = "=[vosh]";

/// The message Lua gives a failed allocation.
const MEMORY_MESSAGE: &str = "not enough memory";

/// Why Vosh stopped a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StopReason {
    /// It ran past [`TIME_BUDGET`].
    Time,
    /// It held [`CALL_MEMORY`] more than when it began.
    CallMemory,
    /// The state reached [`STATE_MEMORY`].
    StateMemory,
}

/// Where in your Lua a stop happened: the chunk name, as Lua keeps it,
/// and the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct At {
    pub(crate) source: String,
    pub(crate) line: u32,
}

/// A stop, and where it happened when Vosh could tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stop {
    pub(crate) reason: StopReason,
    pub(crate) at: Option<At>,
}

/// The clock, the memory limit and the stop flag of the call running
/// now. The hook and the wrapped functions share it.
pub(crate) struct Limits {
    /// Set once a call is stopped, and cleared when the next one
    /// begins. Read on every hook and every protected call.
    stopped: AtomicBool,
    /// The hook sits on a coroutine, or on a thread that resumed one,
    /// so the next call points it at the main thread again.
    hook_moved: AtomicBool,
    inner: Mutex<Inner>,
}

struct Inner {
    /// When the call running now runs out of time. Between calls it
    /// lies in the past, so any Lua that runs outside a call stops at
    /// the first look at the clock.
    deadline: Instant,
    /// The memory limit the call running now has.
    memory_limit: usize,
    stop: Option<Stop>,
}

impl Limits {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            stopped: AtomicBool::new(false),
            hook_moved: AtomicBool::new(false),
            inner: Mutex::new(Inner {
                deadline: Instant::now(),
                memory_limit: STATE_MEMORY,
                stop: None,
            }),
        })
    }

    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Start a call: clear the stop, start the clock, and let the call
    /// hold [`CALL_MEMORY`] more than the state holds now.
    pub(crate) fn begin(self: &Arc<Self>, lua: &Lua) {
        // Near the whole state's limit, garbage left by an earlier call
        // could pass for memory your scripts hold, so collect it first.
        if lua.used_memory().saturating_add(CALL_MEMORY) > STATE_MEMORY {
            let _ = lua.gc_collect();
        }
        let memory_limit = lua
            .used_memory()
            .saturating_add(CALL_MEMORY)
            .min(STATE_MEMORY);
        // A state made by mlua always has memory control.
        let _ = lua.set_memory_limit(memory_limit);
        {
            let mut inner = self.inner();
            inner.deadline = Instant::now() + TIME_BUDGET;
            inner.memory_limit = memory_limit;
            inner.stop = None;
        }
        self.stopped.store(false, Ordering::SeqCst);
        if self.hook_moved.swap(false, Ordering::SeqCst) {
            lua.set_hook(triggers(), hook(Arc::clone(self)));
        }
    }

    /// End a call. The memory limit goes back to [`STATE_MEMORY`] and
    /// the clock to the past. `memory_error` says the call failed on a
    /// memory error, which stops it even when no protected call saw it.
    /// Returns what stopped the call, if anything did.
    pub(crate) fn end(&self, lua: &Lua, memory_error: bool) -> Option<Stop> {
        let _ = lua.set_memory_limit(STATE_MEMORY);
        if memory_error {
            self.note_memory(None);
        }
        let mut inner = self.inner();
        inner.deadline = Instant::now();
        inner.memory_limit = STATE_MEMORY;
        inner.stop.take()
    }

    fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    /// Stop the call running now, unless something stopped it already.
    fn stop(&self, reason: StopReason, at: Option<At>) {
        let mut inner = self.inner();
        if inner.stop.is_none() {
            inner.stop = Some(Stop { reason, at });
        }
        self.stopped.store(true, Ordering::SeqCst);
    }

    /// Stop the call running now on a failed allocation. Under the whole
    /// state's limit that is [`StopReason::StateMemory`], and otherwise
    /// the call's own.
    fn note_memory(&self, at: Option<At>) {
        let reason = if self.inner().memory_limit >= STATE_MEMORY {
            StopReason::StateMemory
        } else {
            StopReason::CallMemory
        };
        self.stop(reason, at);
    }

    /// The hook: past the deadline it stops the call, and while a stop
    /// holds it raises again on every look.
    fn on_hook(&self, lua: &Lua, debug: &Debug<'_>) -> mlua::Result<VmState> {
        if self.is_stopped() {
            return Err(stopped());
        }
        if Instant::now() < self.inner().deadline {
            return Ok(VmState::Continue);
        }
        self.stop(StopReason::Time, at_of(debug).or_else(|| caller(lua)));
        Err(stopped())
    }

    /// What a wrapped `pcall`, `xpcall` or `coroutine.resume` hands back:
    /// its results as they are, or the stop raised again while one
    /// holds. A failed allocation the call caught stops the call too.
    fn settle(&self, lua: &Lua, results: MultiValue) -> mlua::Result<MultiValue> {
        if self.is_stopped() {
            return Err(stopped());
        }
        let caught = matches!(results.front(), Some(Value::Boolean(false)));
        if caught && results.get(1).is_some_and(is_memory_value) {
            self.note_memory(caller(lua));
            return Err(stopped());
        }
        Ok(results)
    }
}

/// How often the hook runs.
fn triggers() -> HookTriggers {
    HookTriggers::new().every_nth_instruction(HOOK_EVERY)
}

/// The hook function, for any thread.
fn hook(limits: Arc<Limits>) -> impl Fn(&Lua, Debug<'_>) -> mlua::Result<VmState> + Send + 'static {
    move |lua, debug| limits.on_hook(lua, &debug)
}

/// The error a stop raises. The script never sees its text, since the
/// stop ends the call and Vosh prints its own line.
fn stopped() -> mlua::Error {
    mlua::Error::RuntimeError("Vosh stopped this Lua".into())
}

/// True when `err` is or wraps a failed allocation.
pub(crate) fn is_memory_error(err: &mlua::Error) -> bool {
    match err {
        mlua::Error::MemoryError(_) => true,
        mlua::Error::CallbackError { cause, .. } | mlua::Error::WithContext { cause, .. } => {
            is_memory_error(cause)
        }
        _ => false,
    }
}

/// True when `value`, the error a protected call caught, is a failed
/// allocation.
fn is_memory_value(value: &Value) -> bool {
    match value {
        Value::String(s) => s.to_str().is_ok_and(|text| *text == *MEMORY_MESSAGE),
        Value::Error(err) => is_memory_error(err),
        _ => false,
    }
}

/// Where `debug` stands, when it stands in your Lua.
fn at_of(debug: &Debug<'_>) -> Option<At> {
    let line = u32::try_from(debug.curr_line())
        .ok()
        .filter(|line| *line > 0)?;
    let source = debug.source().source?.into_owned();
    if source == INTERNAL_CHUNK {
        return None;
    }
    Some(At { source, line })
}

/// The nearest place on the stack that stands in your Lua, past Vosh's
/// own functions and the C ones.
fn caller(lua: &Lua) -> Option<At> {
    (0..64).find_map(|level| lua.inspect_stack(level).as_ref().and_then(at_of))
}

/// The Lua that wraps the protected calls, the coroutine functions,
/// `setmetatable` and `print`. It keeps the originals as upvalues, and
/// with no `debug` library in the sandbox no script can reach them.
///
/// Lua runs two kinds of function with every hook off: a message
/// handler that handles an error the hook raised, and a `__gc` method.
/// So the wrapped `xpcall` skips your handler while a stop holds, and
/// `setmetatable` refuses a metatable with `__gc` in it.
const GUARDS: &str = r##"
local settle, hook_on, after_resume, is_stopped, log = ...
local raw_pcall, raw_xpcall, raw_setmetatable = pcall, xpcall, setmetatable
local error, rawget, select, type = error, rawget, select, type
local tostring, concat = tostring, table.concat
local co = coroutine
local raw_create, raw_resume = co.create, co.resume
local raw_status, raw_close = co.status, co.close

function pcall(f, ...)
  return settle(raw_pcall(f, ...))
end

function xpcall(f, handler, ...)
  if type(handler) ~= "function" then
    return settle(raw_xpcall(f, handler, ...))
  end
  local function handle(err)
    if is_stopped() then
      return err
    end
    return handler(err)
  end
  return settle(raw_xpcall(f, handle, ...))
end

function setmetatable(t, mt)
  if type(mt) == "table" and rawget(mt, "__gc") ~= nil then
    error("Vosh does not run __gc methods", 2)
  end
  return raw_setmetatable(t, mt)
end

local function resume(c, ...)
  hook_on(c)
  return after_resume(raw_resume(c, ...))
end
co.resume = resume

-- Closing a coroutine runs the __close methods it still holds, on the
-- coroutine itself.
local function close(c)
  hook_on(c)
  return after_resume(raw_close(c))
end
co.close = close

-- What coroutine.wrap does with what the coroutine gave back: the
-- values, or its error raised where the wrapped function was called.
local function finish(c, ok, ...)
  if ok then
    return ...
  end
  local err = ...
  if raw_status(c) == "dead" then
    close(c)
  end
  error(err, 2)
end

function co.wrap(f)
  local c = raw_create(f)
  return function(...)
    return finish(c, resume(c, ...))
  end
end

function print(...)
  local parts = {}
  for i = 1, select("#", ...) do
    parts[i] = tostring((select(i, ...)))
  end
  log(concat(parts, "\t"))
end
"##;

/// Wrap `pcall`, `xpcall`, `setmetatable`, `coroutine.resume`,
/// `coroutine.close`, `coroutine.wrap` and `print`, set the whole
/// state's memory limit, and set the hook. `log` takes the line a
/// `print` makes.
pub(crate) fn install(lua: &Lua, limits: &Arc<Limits>, log: Function) -> mlua::Result<()> {
    let settle = {
        let limits = Arc::clone(limits);
        lua.create_function(move |lua, results: MultiValue| limits.settle(lua, results))?
    };
    let hook_on = {
        let limits = Arc::clone(limits);
        lua.create_function(move |_, thread: Value| {
            if let Value::Thread(thread) = thread {
                limits.hook_moved.store(true, Ordering::SeqCst);
                thread.set_hook(triggers(), hook(Arc::clone(&limits)));
            }
            Ok(())
        })?
    };
    let after_resume = {
        let limits = Arc::clone(limits);
        lua.create_function(move |lua, results: MultiValue| {
            limits.hook_moved.store(true, Ordering::SeqCst);
            lua.current_thread()
                .set_hook(triggers(), hook(Arc::clone(&limits)));
            limits.settle(lua, results)
        })?
    };
    let is_stopped = {
        let limits = Arc::clone(limits);
        lua.create_function(move |_, ()| Ok(limits.is_stopped()))?
    };
    lua.load(GUARDS).set_name(INTERNAL_CHUNK).call::<()>((
        settle,
        hook_on,
        after_resume,
        is_stopped,
        log,
    ))?;
    lua.set_memory_limit(STATE_MEMORY)?;
    lua.set_hook(triggers(), hook(Arc::clone(limits)));
    Ok(())
}

//! The limits every call into Lua runs under. Lua runs on the session
//! loop, so a call that never returns holds back every game line after
//! it, and a flood of sends overflows the game's input buffer.
//!
//! A call gets [`TIME_BUDGET`] and [`CALL_MEMORY`] more than the state
//! held when it began, never past [`STATE_MEMORY`] in all, and may queue
//! [`ACTIONS_PER_CALL`] actions with [`CALL_BYTES`] of text among them.
//! The text Rust copies out of Lua counts toward neither memory limit,
//! so each piece of it has a size limit of its own. Rust decides a stop,
//! never the script.
//! A hook counts instructions and checks the clock every [`HOOK_EVERY`],
//! and a stop sets a flag that `pcall`, `xpcall` and the coroutine
//! functions rethrow while it holds, so no protected loop can catch the
//! stop and spin on. [`crate::hook`] sets the hook, on every coroutine
//! too.
//!
//! The hook cannot look inside one C function, so the wrapped library
//! functions that could run long inside one do their work where the
//! limits see it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use mlua::{Function, Lua, MultiValue, Value};

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
/// The most bytes one line Lua sends or puts through `mud.input` may
/// hold, the size of the game's input buffer.
pub(crate) const LINE_BYTES: usize = 1024;
/// The most bytes one echo, print or error line may hold.
pub(crate) const ECHO_BYTES: usize = 64 * 1024;
/// The most bytes a name, a pattern, an expansion or a value may hold.
pub(crate) const NAME_BYTES: usize = 4 * 1024;
/// The most text one call may queue in all.
pub(crate) const CALL_BYTES: usize = 256 * 1024;
/// The most blocks one pane shows. The rest drop.
pub(crate) const PANE_BLOCKS: usize = 200;
/// The most characters one piece of text in a pane block may hold. The
/// rest is cut.
pub(crate) const PANE_LINE_CHARS: usize = 500;

/// The chunk name of Vosh's own Lua, which a stop never points at.
pub(crate) const INTERNAL_CHUNK: &str = "=[vosh]";

/// The message Lua gives a failed allocation.
const MEMORY_MESSAGE: &str = "not enough memory";

/// Why Vosh stopped a call, and so the plugin or loose script that ran
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
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
    pub(crate) fn begin(&self, lua: &Lua) {
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

    /// What the hook asks: true when the call must stop. Past the
    /// deadline it stops the call at the place `place` gives, and while
    /// a stop holds it says so on every look.
    pub(crate) fn on_hook(&self, place: impl FnOnce() -> Option<At>) -> bool {
        if self.is_stopped() {
            return true;
        }
        if Instant::now() < self.inner().deadline {
            return false;
        }
        self.stop(StopReason::Time, place());
        true
    }

    /// True when the call running now is out of time, which stops it at
    /// the nearest place in your Lua, or was stopped already. Rust code
    /// that loops for Lua asks it now and then, as the hook would.
    pub(crate) fn out_of_time(&self, lua: &Lua) -> bool {
        self.on_hook(|| caller(lua))
    }

    /// How many more bytes the call running now may hold, which caps
    /// what Rust builds for it before it hands the result to Lua.
    pub(crate) fn memory_room(&self, lua: &Lua) -> usize {
        self.inner().memory_limit.saturating_sub(lua.used_memory())
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

/// The error a stop raises. The script never sees its text, since the
/// stop ends the call and Vosh prints its own line.
pub(crate) fn stopped() -> mlua::Error {
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
fn at_of(debug: &mlua::Debug<'_>) -> Option<At> {
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
pub(crate) fn caller(lua: &Lua) -> Option<At> {
    (0..64).find_map(|level| lua.inspect_stack(level).as_ref().and_then(at_of))
}

/// The Lua that wraps the protected calls, the coroutine functions,
/// `setmetatable` and `print`. It keeps the originals as upvalues, and
/// with no `debug` library in the sandbox no script can reach them.
///
/// Lua runs three kinds of function with every hook off: a message
/// handler that handles an error the hook raised, a `__gc` method, and
/// the `__close` methods of a coroutine the hook stopped once something
/// closes it. So the wrapped `xpcall` skips your handler while a stop
/// holds, `setmetatable` refuses a metatable with `__gc` in it, and the
/// wrapped `coroutine.close` leaves a coroutine the hook stopped alone.
const GUARDS: &str = r##"
local settle, stopped_dead, is_stopped, log = ...
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
  return settle(raw_resume(c, ...))
end
co.resume = resume

-- Closing a coroutine runs the __close methods it still holds, on the
-- coroutine itself, under the hook it took from the thread that made
-- it. One the hook stopped has its hook off, so it stays as it is.
local function close(c)
  if stopped_dead(c) then
    return false, "Vosh stopped this Lua"
  end
  return settle(raw_close(c))
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
/// `coroutine.close`, `coroutine.wrap` and `print`, and set the whole
/// state's memory limit. `log` takes the line a `print` makes. The hook
/// goes on last, from [`crate::hook::install`].
pub(crate) fn install(lua: &Lua, limits: &Arc<Limits>, log: Function) -> mlua::Result<()> {
    let settle = {
        let limits = Arc::clone(limits);
        lua.create_function(move |lua, results: MultiValue| limits.settle(lua, results))?
    };
    let stopped_dead =
        lua.create_function(|_, thread: Value| Ok(crate::hook::stopped_dead(&thread)))?;
    let is_stopped = {
        let limits = Arc::clone(limits);
        lua.create_function(move |_, ()| Ok(limits.is_stopped()))?
    };
    lua.load(GUARDS).set_name(INTERNAL_CHUNK).call::<()>((
        settle,
        stopped_dead,
        is_stopped,
        log,
    ))?;
    lua.set_memory_limit(STATE_MEMORY)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stop_note_on_a_plugin_page_names_these_limits() {
        // STOP_REASON in src/settings/scripts/pluginState.ts says why a
        // plugin is stopped over its editor.
        let page = include_str!("../../../src/settings/scripts/pluginState.ts");
        for reason in [
            format!("time: 'one call ran past {} ms',", TIME_BUDGET.as_millis()),
            format!(
                "call_memory: 'one call used more than {} MB',",
                CALL_MEMORY / MB
            ),
            format!(
                "state_memory: 'your scripts held more than {} MB',",
                STATE_MEMORY / MB
            ),
        ] {
            assert!(page.contains(&reason), "{reason}");
        }
    }
}

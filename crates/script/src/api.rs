//! Install the `mud.*` Lua API. The functions here are sync and side-effect
//! free at the Rust level; they queue [`Action`] values into the engine's
//! shared state so the session can apply them after the callback returns.

use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::time::Duration;

use mlua::{Function, Lua, Result as LuaResult, Table, Value};
use vosh_automation::vars::Scope;

use crate::actions::Action;
use crate::state::{Callback, EngineState, StateInner};

/// Counter for synthetic callback ids. The Lua engine stores the actual
/// callback function in its registry and we hand around the integer id so
/// the rest of the app does not need to grab Lua values.
static NEXT_CALLBACK_ID: AtomicI64 = AtomicI64::new(1);
static NEXT_TIMER_ID: AtomicU32 = AtomicU32::new(1);

pub(crate) fn alloc_callback_id() -> i64 {
    NEXT_CALLBACK_ID.fetch_add(1, Ordering::Relaxed)
}

fn alloc_timer_id() -> u32 {
    NEXT_TIMER_ID.fetch_add(1, Ordering::Relaxed)
}

/// Install the mud API as the `mud` global table scripts call into.
pub(crate) fn install(lua: &Lua) -> LuaResult<()> {
    let mud = lua.create_table()?;

    mud.set("send", lua.create_function(mud_send)?)?;
    mud.set("input", lua.create_function(mud_input)?)?;
    mud.set("echo", lua.create_function(mud_echo)?)?;
    mud.set("log", lua.create_function(mud_log)?)?;

    mud.set("alias", lua.create_function(mud_alias)?)?;
    mud.set("unalias", lua.create_function(mud_unalias)?)?;

    mud.set("var", lua.create_function(mud_var)?)?;
    mud.set("set_var", lua.create_function(mud_set_var)?)?;
    mud.set("set_profile_var", lua.create_function(mud_set_profile_var)?)?;
    mud.set("unset_var", lua.create_function(mud_unset_var)?)?;

    mud.set("set_prompt_var", lua.create_function(mud_set_prompt_var)?)?;
    mud.set(
        "unset_prompt_var",
        lua.create_function(mud_unset_prompt_var)?,
    )?;

    mud.set(
        "set_group_enabled",
        lua.create_function(mud_set_group_enabled)?,
    )?;

    mud.set("trigger", lua.create_function(mud_trigger)?)?;
    mud.set("untrigger", lua.create_function(mud_untrigger)?)?;

    mud.set("on_gmcp", lua.create_function(mud_on_gmcp)?)?;

    mud.set("timer", lua.create_function(mud_timer)?)?;
    mud.set("cancel_timer", lua.create_function(mud_cancel_timer)?)?;

    lua.globals().set("mud", mud)?;
    Ok(())
}

fn with_state<F, R>(lua: &Lua, f: F) -> LuaResult<R>
where
    F: FnOnce(&mut StateInner) -> LuaResult<R>,
{
    let cell = {
        let state = lua
            .app_data_ref::<EngineState>()
            .ok_or_else(|| mlua::Error::RuntimeError("script engine state missing".into()))?;
        state.cell.clone()
    };
    let mut guard = cell
        .lock()
        .map_err(|_| mlua::Error::RuntimeError("script engine state poisoned".into()))?;
    f(&mut guard)
}

/// Hold `key`, a function the call running now hands over, under `id`,
/// with the owner of that call.
fn hold(s: &mut StateInner, id: i64, key: mlua::RegistryKey) {
    let owner = s.owner();
    s.callbacks.insert(id, Callback { key, owner });
}

fn mud_send(lua: &Lua, text: String) -> LuaResult<()> {
    with_state(lua, |s| {
        s.queue(Action::Send(text));
        Ok(())
    })
}

fn mud_input(lua: &Lua, text: String) -> LuaResult<()> {
    with_state(lua, |s| {
        s.queue(Action::Input(text));
        Ok(())
    })
}

fn mud_echo(lua: &Lua, text: String) -> LuaResult<()> {
    with_state(lua, |s| {
        s.queue(Action::Echo(text));
        Ok(())
    })
}

/// `mud.log`, and the line `print` makes.
pub(crate) fn mud_log(lua: &Lua, text: String) -> LuaResult<()> {
    with_state(lua, |s| {
        s.queue(Action::Log(text));
        Ok(())
    })
}

fn mud_alias(lua: &Lua, (name, expansion): (String, String)) -> LuaResult<()> {
    with_state(lua, |s| {
        s.queue(Action::SetAlias { name, expansion });
        Ok(())
    })
}

fn mud_unalias(lua: &Lua, name: String) -> LuaResult<()> {
    with_state(lua, |s| {
        s.queue(Action::RemoveAlias(name));
        Ok(())
    })
}

fn mud_var(lua: &Lua, name: String) -> LuaResult<Option<String>> {
    // Variables live on the Profile, not in the Lua state. The session
    // pushes them into a snapshot table accessible to scripts via the
    // shared state. For a synchronous getter we read that snapshot.
    with_state(lua, |s| Ok(s.var_snapshot.get(&name).cloned()))
}

fn mud_set_var(lua: &Lua, (name, value): (String, String)) -> LuaResult<()> {
    with_state(lua, |s| {
        let queued = s.queue(Action::SetVar {
            scope: Scope::Session,
            name: name.clone(),
            value: value.clone(),
        });
        if queued {
            s.var_snapshot.insert(name, value);
        }
        Ok(())
    })
}

fn mud_set_profile_var(lua: &Lua, (name, value): (String, String)) -> LuaResult<()> {
    with_state(lua, |s| {
        let queued = s.queue(Action::SetVar {
            scope: Scope::Profile,
            name: name.clone(),
            value: value.clone(),
        });
        if queued {
            s.var_snapshot.insert(name, value);
        }
        Ok(())
    })
}

fn mud_unset_var(lua: &Lua, name: String) -> LuaResult<()> {
    with_state(lua, |s| {
        if s.queue(Action::RemoveVar(name.clone())) {
            s.var_snapshot.remove(&name);
        }
        Ok(())
    })
}

fn mud_set_prompt_var(lua: &Lua, (name, value): (String, String)) -> LuaResult<()> {
    with_state(lua, |s| {
        s.queue(Action::SetPromptVar { name, value });
        Ok(())
    })
}

fn mud_unset_prompt_var(lua: &Lua, name: String) -> LuaResult<()> {
    with_state(lua, |s| {
        s.queue(Action::RemovePromptVar(name));
        Ok(())
    })
}

fn mud_set_group_enabled(lua: &Lua, (name, enabled): (String, bool)) -> LuaResult<()> {
    with_state(lua, |s| {
        s.queue(Action::SetGroupEnabled { name, enabled });
        Ok(())
    })
}

fn mud_trigger(lua: &Lua, (name, pattern, callback): (String, String, Function)) -> LuaResult<()> {
    let key = lua.create_registry_value(callback)?;
    let id = alloc_callback_id();
    with_state(lua, |s| {
        hold(s, id, key);
        s.queue(Action::SetLuaTrigger {
            owner: s.owner(),
            name,
            pattern,
            callback_id: id,
        });
        Ok(())
    })
}

fn mud_untrigger(lua: &Lua, name: String) -> LuaResult<()> {
    with_state(lua, |s| {
        s.queue(Action::RemoveLuaTrigger {
            owner: s.owner(),
            name,
        });
        Ok(())
    })
}

fn mud_on_gmcp(lua: &Lua, (package, callback): (String, Function)) -> LuaResult<()> {
    let key = lua.create_registry_value(callback)?;
    let id = alloc_callback_id();
    with_state(lua, |s| {
        hold(s, id, key);
        s.queue(Action::SubscribeGmcp {
            package,
            callback_id: id,
        });
        Ok(())
    })
}

fn mud_timer(lua: &Lua, (secs, callback): (f64, Function)) -> LuaResult<u32> {
    let key = lua.create_registry_value(callback)?;
    let id = alloc_callback_id();
    let timer_id = alloc_timer_id();
    with_state(lua, |s| {
        hold(s, id, key);
        s.timer_callbacks.insert(timer_id, id);
        s.queue(Action::Timer {
            delay: Duration::from_secs_f64(secs.max(0.0)),
            callback_id: id,
            timer_id,
        });
        Ok(timer_id)
    })
}

fn mud_cancel_timer(lua: &Lua, timer_id: u32) -> LuaResult<()> {
    with_state(lua, |s| {
        // Free the callback now. The session drops the schedule when it
        // applies the cancel, and nothing else would ever free it. This
        // removes the key directly because `with_state` already holds the
        // lock that `ScriptEngine::drop_callback` takes.
        if let Some(callback_id) = s.timer_callbacks.remove(&timer_id) {
            s.callbacks.remove(&callback_id);
        }
        s.queue(Action::CancelTimer(timer_id));
        Ok(())
    })
}

/// Convert a `serde_json::Value` to a Lua value. Used when invoking GMCP
/// subscribers so scripts can read the JSON payload as native tables.
pub(crate) fn json_to_lua(lua: &Lua, value: &serde_json::Value) -> LuaResult<Value> {
    Ok(match value {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(b) => Value::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                Value::Number(f)
            } else {
                Value::Nil
            }
        }
        serde_json::Value::String(s) => Value::String(lua.create_string(s)?),
        serde_json::Value::Array(items) => {
            let t = lua.create_table()?;
            for (i, v) in items.iter().enumerate() {
                t.set(i + 1, json_to_lua(lua, v)?)?;
            }
            Value::Table(t)
        }
        serde_json::Value::Object(map) => {
            let t = lua.create_table()?;
            for (k, v) in map {
                t.set(k.as_str(), json_to_lua(lua, v)?)?;
            }
            Value::Table(t)
        }
    })
}

/// Apply the sandbox: remove globals that shell out, touch the filesystem,
/// read the environment, or load arbitrary code.
pub(crate) fn apply_sandbox(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();
    for name in ["dofile", "loadfile", "load", "loadstring", "require"] {
        globals.set(name, Value::Nil)?;
    }
    let io: Option<Table> = globals.get("io").ok();
    if io.is_some() {
        globals.set("io", Value::Nil)?;
    }
    let os: Option<Table> = globals.get("os").ok();
    if let Some(os) = os {
        for name in [
            "execute", "exit", "getenv", "remove", "rename", "tmpname", "setenv",
        ] {
            os.set(name, Value::Nil)?;
        }
    }
    let pkg: Option<Table> = globals.get("package").ok();
    if pkg.is_some() {
        globals.set("package", Value::Nil)?;
    }
    Ok(())
}

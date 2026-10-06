//! Install the `mud.*` Lua API. The functions here are sync and side-effect
//! free at the Rust level; they queue [`Action`] values into the engine's
//! shared state so the session can apply them after the callback returns.

use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::time::Duration;

use mlua::{FromLuaMulti, Function, IntoLuaMulti, Lua, Result as LuaResult, Table, Value};
use vosh_automation::alert::{AlertParts, Attention};
use vosh_automation::vars::Scope;

use crate::actions::Action;
use crate::limits::{ECHO_BYTES, LINE_BYTES, NAME_BYTES};
use crate::owner::Owner;
use crate::state::{Callback, EngineState, StateInner};

/// A piece of text Lua hands a `mud` function, copied out of Lua only
/// when it holds at most `cap` bytes, since the copy counts toward no
/// memory limit. None when it is longer.
pub(crate) fn capped(text: &mlua::String, cap: usize) -> LuaResult<Option<String>> {
    if text.as_bytes().len() > cap {
        return Ok(None);
    }
    Ok(Some(text.to_str()?.to_owned()))
}

/// Each of `texts` copied out of Lua when it fits its cap, or None when
/// any is longer, which drops what the call would have queued and notes
/// it, so the call ends with a line that says so.
fn all_capped<const N: usize>(
    lua: &Lua,
    texts: [(&mlua::String, usize); N],
) -> LuaResult<Option<[String; N]>> {
    let mut out: [String; N] = std::array::from_fn(|_| String::new());
    for (slot, (text, cap)) in out.iter_mut().zip(texts) {
        match capped(text, cap)? {
            Some(text) => *slot = text,
            None => {
                with_state(lua, |s| {
                    s.drop_long_text();
                    Ok(())
                })?;
                return Ok(None);
            }
        }
    }
    Ok(Some(out))
}

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

/// Install the mud API as the `mud` global table your `#lua` lines,
/// trigger and alias bodies and loose scripts call into.
pub(crate) fn install(lua: &Lua) -> LuaResult<()> {
    lua.globals().set("mud", mud_table(lua, None)?)
}

/// A new `mud` table. What its functions register belongs to `owner`,
/// a plugin's own table, or with None to the owner of the call that
/// runs, the shared table.
pub(crate) fn mud_table(lua: &Lua, owner: Option<Owner>) -> LuaResult<Table> {
    let mud = lua.create_table()?;

    mud.set("send", lua.create_function(mud_send)?)?;
    mud.set("input", owned(lua, owner.as_ref(), mud_input)?)?;
    mud.set("echo", lua.create_function(mud_echo)?)?;
    mud.set("log", lua.create_function(mud_log)?)?;

    mud.set("alias", owned(lua, owner.as_ref(), mud_alias)?)?;
    mud.set("unalias", owned(lua, owner.as_ref(), mud_unalias)?)?;

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

    mud.set("trigger", owned(lua, owner.as_ref(), mud_trigger)?)?;
    mud.set("untrigger", owned(lua, owner.as_ref(), mud_untrigger)?)?;

    mud.set("on_gmcp", owned(lua, owner.as_ref(), mud_on_gmcp)?)?;

    mud.set("alert", owned(lua, owner.as_ref(), mud_alert)?)?;
    mud.set("pane", owned(lua, owner.as_ref(), crate::pane::mud_pane)?)?;

    mud.set("timer", owned(lua, owner.as_ref(), mud_timer)?)?;
    mud.set(
        "cancel_timer",
        owned(lua, owner.as_ref(), mud_cancel_timer)?,
    )?;

    Ok(mud)
}

/// A mud function that registers for `owner`, or for the owner of the
/// call that runs when `owner` is None.
fn owned<A, R>(
    lua: &Lua,
    owner: Option<&Owner>,
    f: fn(&Lua, Option<&Owner>, A) -> LuaResult<R>,
) -> LuaResult<Function>
where
    A: FromLuaMulti + 'static,
    R: IntoLuaMulti + 'static,
{
    let owner = owner.cloned();
    lua.create_function(move |lua, args: A| f(lua, owner.as_ref(), args))
}

/// Who a registration belongs to: the owner of the `mud` table it came
/// through, or else the owner of the call that runs.
fn registrant(s: &StateInner, owner: Option<&Owner>) -> Owner {
    owner.cloned().unwrap_or_else(|| s.owner())
}

pub(crate) fn with_state<F, R>(lua: &Lua, f: F) -> LuaResult<R>
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
/// for `owner`.
fn hold(s: &mut StateInner, id: i64, key: mlua::RegistryKey, owner: Owner) {
    s.callbacks.insert(id, Callback { key, owner });
}

/// Queue the action `make` builds from `texts`, each within its cap.
fn queue_capped<const N: usize>(
    lua: &Lua,
    texts: [(&mlua::String, usize); N],
    make: impl FnOnce([String; N]) -> Action,
) -> LuaResult<bool> {
    let Some(texts) = all_capped(lua, texts)? else {
        return Ok(false);
    };
    with_state(lua, |s| Ok(s.queue(make(texts))))
}

fn mud_send(lua: &Lua, text: mlua::String) -> LuaResult<()> {
    queue_capped(lua, [(&text, LINE_BYTES)], |[text]| Action::Send(text))?;
    Ok(())
}

/// `mud.input`. The line goes with whose Lua asked for it, so a plugin's
/// line runs no slash command.
fn mud_input(lua: &Lua, owner: Option<&Owner>, text: mlua::String) -> LuaResult<()> {
    let Some([line]) = all_capped(lua, [(&text, LINE_BYTES)])? else {
        return Ok(());
    };
    with_state(lua, |s| {
        let owner = registrant(s, owner);
        s.queue(Action::Input { owner, line });
        Ok(())
    })
}

fn mud_echo(lua: &Lua, text: mlua::String) -> LuaResult<()> {
    queue_capped(lua, [(&text, ECHO_BYTES)], |[text]| Action::Echo(text))?;
    Ok(())
}

/// `mud.log`, and the line `print` makes. The line goes with the owner
/// of the call that printed it, so the Scripts page shows it with that
/// plugin.
pub(crate) fn mud_log(lua: &Lua, text: mlua::String) -> LuaResult<()> {
    let Some([text]) = all_capped(lua, [(&text, ECHO_BYTES)])? else {
        return Ok(());
    };
    with_state(lua, |s| {
        let owner = s.owner();
        s.queue(Action::Log { owner, text });
        Ok(())
    })
}

/// `mud.alias`. A plugin's alias lasts for the session and belongs to
/// the plugin, and any other is one you keep.
fn mud_alias(
    lua: &Lua,
    owner: Option<&Owner>,
    (name, expansion): (mlua::String, mlua::String),
) -> LuaResult<()> {
    let Some([name, expansion]) = all_capped(lua, [(&name, NAME_BYTES), (&expansion, NAME_BYTES)])?
    else {
        return Ok(());
    };
    with_state(lua, |s| {
        s.queue(match registrant(s, owner) {
            Owner::Plugin(plugin) => Action::SetPluginAlias {
                plugin,
                name,
                expansion,
            },
            _ => Action::SetAlias { name, expansion },
        });
        Ok(())
    })
}

/// `mud.unalias`. A plugin removes only an alias it made.
fn mud_unalias(lua: &Lua, owner: Option<&Owner>, name: mlua::String) -> LuaResult<()> {
    let Some([name]) = all_capped(lua, [(&name, NAME_BYTES)])? else {
        return Ok(());
    };
    with_state(lua, |s| {
        s.queue(match registrant(s, owner) {
            Owner::Plugin(plugin) => Action::RemovePluginAlias { plugin, name },
            _ => Action::RemoveAlias(name),
        });
        Ok(())
    })
}

fn mud_var(lua: &Lua, name: String) -> LuaResult<Option<String>> {
    // Variables live on the Profile, not in the Lua state. The session
    // pushes them into a snapshot table accessible to scripts via the
    // shared state. For a synchronous getter we read that snapshot.
    with_state(lua, |s| Ok(s.var_snapshot.get(&name).cloned()))
}

fn mud_set_var(lua: &Lua, (name, value): (mlua::String, mlua::String)) -> LuaResult<()> {
    let Some([name, value]) = all_capped(lua, [(&name, NAME_BYTES), (&value, NAME_BYTES)])? else {
        return Ok(());
    };
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

fn mud_set_profile_var(lua: &Lua, (name, value): (mlua::String, mlua::String)) -> LuaResult<()> {
    let Some([name, value]) = all_capped(lua, [(&name, NAME_BYTES), (&value, NAME_BYTES)])? else {
        return Ok(());
    };
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

fn mud_unset_var(lua: &Lua, name: mlua::String) -> LuaResult<()> {
    let Some([name]) = all_capped(lua, [(&name, NAME_BYTES)])? else {
        return Ok(());
    };
    with_state(lua, |s| {
        if s.queue(Action::RemoveVar(name.clone())) {
            s.var_snapshot.remove(&name);
        }
        Ok(())
    })
}

fn mud_set_prompt_var(lua: &Lua, (name, value): (mlua::String, mlua::String)) -> LuaResult<()> {
    queue_capped(
        lua,
        [(&name, NAME_BYTES), (&value, NAME_BYTES)],
        |[name, value]| Action::SetPromptVar { name, value },
    )?;
    Ok(())
}

fn mud_unset_prompt_var(lua: &Lua, name: mlua::String) -> LuaResult<()> {
    queue_capped(lua, [(&name, NAME_BYTES)], |[name]| {
        Action::RemovePromptVar(name)
    })?;
    Ok(())
}

/// `mud.alert(title, options)`. The alert rings as a trigger's alert
/// does, under the same focus rule and 10 second cap, and belongs to
/// whose Lua raised it, so turning a plugin off ends its alerts. It posts
/// a banner unless `options` says `banner = false`, and `options` may add
/// `sound`, `attention` (`once` or `until`), `background`, `words` and
/// the `text` a banner with words shows.
fn mud_alert(
    lua: &Lua,
    owner: Option<&Owner>,
    (title, options): (mlua::String, Option<Table>),
) -> LuaResult<()> {
    let get_text = |key: &str| -> LuaResult<Option<mlua::String>> {
        options.as_ref().map_or(Ok(None), |o| o.get(key))
    };
    let get_flag = |key: &str| -> LuaResult<Option<bool>> {
        options.as_ref().map_or(Ok(None), |o| o.get(key))
    };
    let (text, sound, attention) = (
        get_text("text")?,
        get_text("sound")?,
        get_text("attention")?,
    );
    let empty = lua.create_string("")?;
    let Some([title, text, sound, attention]) = all_capped(
        lua,
        [
            (&title, NAME_BYTES),
            (text.as_ref().unwrap_or(&empty), ECHO_BYTES),
            (sound.as_ref().unwrap_or(&empty), NAME_BYTES),
            (attention.as_ref().unwrap_or(&empty), NAME_BYTES),
        ],
    )?
    else {
        return Ok(());
    };
    let parts = AlertParts {
        banner: get_flag("banner")?.unwrap_or(true),
        sound: (!sound.is_empty()).then_some(sound),
        attention: match attention.as_str() {
            "once" => Some(Attention::Once),
            "until" => Some(Attention::Until),
            _ => None,
        },
        background: get_flag("background")?.unwrap_or(true),
        words: get_flag("words")?.unwrap_or(false),
    };
    with_state(lua, |s| {
        let owner = registrant(s, owner);
        s.queue(Action::Alert {
            owner,
            title,
            text: (!text.is_empty()).then_some(text),
            parts,
        });
        Ok(())
    })
}

fn mud_set_group_enabled(lua: &Lua, (name, enabled): (mlua::String, bool)) -> LuaResult<()> {
    queue_capped(lua, [(&name, NAME_BYTES)], |[name]| {
        Action::SetGroupEnabled { name, enabled }
    })?;
    Ok(())
}

fn mud_trigger(
    lua: &Lua,
    owner: Option<&Owner>,
    (name, pattern, callback): (mlua::String, mlua::String, Function),
) -> LuaResult<()> {
    let Some([name, pattern]) = all_capped(lua, [(&name, NAME_BYTES), (&pattern, NAME_BYTES)])?
    else {
        return Ok(());
    };
    let key = lua.create_registry_value(callback)?;
    let id = alloc_callback_id();
    with_state(lua, |s| {
        let owner = registrant(s, owner);
        hold(s, id, key, owner.clone());
        s.queue(Action::SetLuaTrigger {
            owner,
            name,
            pattern,
            callback_id: id,
        });
        Ok(())
    })
}

fn mud_untrigger(lua: &Lua, owner: Option<&Owner>, name: mlua::String) -> LuaResult<()> {
    let Some([name]) = all_capped(lua, [(&name, NAME_BYTES)])? else {
        return Ok(());
    };
    with_state(lua, |s| {
        s.queue(Action::RemoveLuaTrigger {
            owner: registrant(s, owner),
            name,
        });
        Ok(())
    })
}

fn mud_on_gmcp(
    lua: &Lua,
    owner: Option<&Owner>,
    (package, callback): (mlua::String, Function),
) -> LuaResult<()> {
    let Some([package]) = all_capped(lua, [(&package, NAME_BYTES)])? else {
        return Ok(());
    };
    let key = lua.create_registry_value(callback)?;
    let id = alloc_callback_id();
    with_state(lua, |s| {
        let owner = registrant(s, owner);
        hold(s, id, key, owner);
        s.queue(Action::SubscribeGmcp {
            package,
            callback_id: id,
        });
        Ok(())
    })
}

/// The longest a Lua timer waits. A longer delay waits this long.
pub(crate) const TIMER_MAX: Duration = Duration::from_secs(24 * 60 * 60);

/// How long a timer of `secs` seconds waits: none for a negative number,
/// and at most [`TIMER_MAX`]. A number that is not finite is an error,
/// since no wait could be that long.
fn timer_delay(secs: f64) -> LuaResult<Duration> {
    if !secs.is_finite() {
        return Err(mlua::Error::RuntimeError(
            "mud.timer needs a finite number of seconds".into(),
        ));
    }
    Ok(Duration::try_from_secs_f64(secs.max(0.0))
        .unwrap_or(TIMER_MAX)
        .min(TIMER_MAX))
}

fn mud_timer(
    lua: &Lua,
    owner: Option<&Owner>,
    (secs, callback): (f64, Function),
) -> LuaResult<u32> {
    let delay = timer_delay(secs)?;
    let key = lua.create_registry_value(callback)?;
    let id = alloc_callback_id();
    let timer_id = alloc_timer_id();
    with_state(lua, |s| {
        let owner = registrant(s, owner);
        hold(s, id, key, owner);
        s.timer_callbacks.insert(timer_id, id);
        s.queue(Action::Timer {
            delay,
            callback_id: id,
            timer_id,
        });
        Ok(timer_id)
    })
}

/// `mud.cancel_timer`. It cancels a timer only for Lua that shares names
/// with the timer's owner: a plugin or a loose script its own timers,
/// and your `#lua` lines and the Lua of your triggers and aliases each
/// other's, as they share trigger names. The engine frees the function
/// once the cancel drains, so a call Vosh stops cancels nothing.
fn mud_cancel_timer(lua: &Lua, owner: Option<&Owner>, timer_id: u32) -> LuaResult<()> {
    with_state(lua, |s| {
        let registrant = registrant(s, owner);
        let ours = s
            .timer_callbacks
            .get(&timer_id)
            .and_then(|id| s.callbacks.get(id))
            .is_some_and(|callback| callback.owner.shares_names_with(&registrant));
        if ours {
            s.queue(Action::CancelTimer(timer_id));
        }
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
/// read the environment, load arbitrary code, or change the C locale,
/// which `tostring` and the patterns of every script read and which the
/// whole process shares.
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
            "execute",
            "exit",
            "getenv",
            "remove",
            "rename",
            "tmpname",
            "setenv",
            "setlocale",
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

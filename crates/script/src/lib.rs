//! Embedded Lua scripting for mudclient. Phase 8.
//!
//! The engine wraps a sandboxed `mlua::Lua` and exposes a `mud.*` API to
//! Lua scripts. Side effects (sends, echoes, alias edits, trigger
//! registration, timer schedules) flow back as [`Action`] values that the
//! session loop applies after each Lua callback returns.

mod actions;
mod api;
mod state;

use std::collections::HashMap;

use mlua::{Function, Lua, Value};
use regex::Regex;
use thiserror::Error;

pub use actions::{Action, VarScope};
use state::EngineState;

/// One Lua-defined trigger: a regex matched against incoming MUD lines and
/// the registry id of the Lua callback to invoke when it fires.
struct LuaTrigger {
    name: String,
    pattern: String,
    regex: Regex,
    callback_id: i64,
    capture_names: Vec<Option<String>>,
    priority: i32,
    enabled: bool,
}

/// Public Lua-trigger record (mirrors `mudclient_trigger::Trigger` for the
/// listing UI but holds the Lua callback id rather than a structured
/// action).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaTriggerInfo {
    pub name: String,
    pub pattern: String,
    pub priority: i32,
    pub enabled: bool,
}

#[derive(Debug, Error)]
pub enum ScriptError {
    #[error("lua error: {0}")]
    Lua(#[from] mlua::Error),
    #[error("invalid regex `{pattern}`: {source}")]
    InvalidRegex {
        pattern: String,
        #[source]
        source: regex::Error,
    },
}

/// Result of running script code or dispatching an event. Carries the
/// queued [`Action`]s the caller should apply.
#[derive(Debug, Default)]
pub struct ScriptOutcome {
    pub actions: Vec<Action>,
}

pub struct ScriptEngine {
    lua: Lua,
    state: EngineState,
    triggers: Vec<LuaTrigger>,
    gmcp_subs: HashMap<String, Vec<i64>>,
    /// Map script source name to the loaded chunk for #script reload.
    loaded_scripts: HashMap<String, String>,
}

impl std::fmt::Debug for ScriptEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptEngine")
            .field("lua", &"<mlua::Lua>")
            .field("state", &"<EngineState>")
            .field("trigger_count", &self.triggers.len())
            .field("gmcp_subscriptions", &self.gmcp_subs.len())
            .field("loaded_scripts", &self.loaded_scripts.len())
            .finish()
    }
}

impl Default for ScriptEngine {
    fn default() -> Self {
        Self::new().expect("Lua initialization should not fail")
    }
}

impl ScriptEngine {
    pub fn new() -> Result<Self, ScriptError> {
        let lua = Lua::new();
        let state = EngineState::new();
        lua.set_app_data(state.clone());
        api::install(&lua)?;
        api::apply_sandbox(&lua)?;
        Ok(Self {
            lua,
            state,
            triggers: Vec::new(),
            gmcp_subs: HashMap::new(),
            loaded_scripts: HashMap::new(),
        })
    }

    /// Replace the synchronous variable snapshot Lua scripts read via
    /// `mud.var(name)`. Call this before `eval`, `match_line`, or
    /// `dispatch_gmcp` so scripts see fresh values.
    pub fn set_var_snapshot(&self, snapshot: HashMap<String, String>) {
        if let Ok(mut s) = self.state.cell.lock() {
            s.var_snapshot = snapshot;
        }
    }

    pub fn loaded_script_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.loaded_scripts.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn lua_triggers(&self) -> Vec<LuaTriggerInfo> {
        let mut out: Vec<LuaTriggerInfo> = self
            .triggers
            .iter()
            .map(|t| LuaTriggerInfo {
                name: t.name.clone(),
                pattern: t.pattern.clone(),
                priority: t.priority,
                enabled: t.enabled,
            })
            .collect();
        out.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.name.cmp(&b.name)));
        out
    }

    /// Evaluate a string of Lua. Used by `#lua <code>` and as the underlying
    /// load path for files.
    pub fn eval(&mut self, code: &str, chunk_name: &str) -> Result<ScriptOutcome, ScriptError> {
        let result: mlua::Result<()> = self.lua.load(code).set_name(chunk_name).exec();
        result?;
        Ok(self.drain())
    }

    /// Load a script as a named chunk so a later `reload` can re-execute
    /// it. The name is what the user typed.
    pub fn load_script(&mut self, name: &str, code: String) -> Result<ScriptOutcome, ScriptError> {
        let outcome = self.eval(&code, name)?;
        self.loaded_scripts.insert(name.to_string(), code);
        Ok(outcome)
    }

    /// Re-execute every loaded script. Triggers and subscriptions are NOT
    /// cleared automatically; scripts that want clean state should manage
    /// it themselves with `mud.untrigger` etc. Phase 9 will add proper
    /// hot-reload semantics.
    pub fn reload_scripts(&mut self) -> Result<ScriptOutcome, ScriptError> {
        let scripts: Vec<(String, String)> = self
            .loaded_scripts
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let mut acc = ScriptOutcome::default();
        for (name, code) in scripts {
            let result: mlua::Result<()> = self.lua.load(&code).set_name(&name).exec();
            result?;
            acc.actions.append(&mut self.drain().actions);
        }
        Ok(acc)
    }

    /// Run every Lua trigger against `line`. For each match the callback
    /// fires with a captures table.
    pub fn match_line(&mut self, line: &str) -> Result<ScriptOutcome, ScriptError> {
        let order: Vec<usize> = (0..self.triggers.len()).collect();
        let mut sorted: Vec<usize> = order;
        sorted.sort_by(|a, b| self.triggers[*b].priority.cmp(&self.triggers[*a].priority));
        for idx in sorted {
            let trigger = &self.triggers[idx];
            if !trigger.enabled {
                continue;
            }
            let regex = trigger.regex.clone();
            let callback_id = trigger.callback_id;
            let capture_names = trigger.capture_names.clone();
            for caps in regex.captures_iter(line) {
                let table = api::captures_to_lua(&self.lua, &caps, &capture_names)?;
                self.invoke_callback(callback_id, Value::Table(table))?;
            }
        }
        Ok(self.drain())
    }

    /// Fire every callback subscribed to `package` with the JSON `data`.
    pub fn dispatch_gmcp(
        &mut self,
        package: &str,
        data: &serde_json::Value,
    ) -> Result<ScriptOutcome, ScriptError> {
        let ids = match self.gmcp_subs.get(package) {
            Some(v) => v.clone(),
            None => return Ok(ScriptOutcome::default()),
        };
        for id in ids {
            let value = api::json_to_lua(&self.lua, data)?;
            self.invoke_callback(id, value)?;
        }
        Ok(self.drain())
    }

    /// Fire a one-shot timer callback by its callback id.
    pub fn fire_timer(&mut self, callback_id: i64) -> Result<ScriptOutcome, ScriptError> {
        self.invoke_callback(callback_id, Value::Nil)?;
        // Drop the callback after firing; it was a one-shot.
        self.drop_callback(callback_id);
        Ok(self.drain())
    }

    /// Drop a callback's registry key. Idempotent.
    pub fn drop_callback(&mut self, callback_id: i64) {
        if let Ok(mut s) = self.state.cell.lock() {
            s.callbacks.remove(&callback_id);
        }
    }

    fn invoke_callback(&self, callback_id: i64, arg: Value) -> mlua::Result<()> {
        let func: Option<Function> = {
            let s = self
                .state
                .cell
                .lock()
                .map_err(|_| mlua::Error::RuntimeError("script state poisoned".into()))?;
            match s.callbacks.get(&callback_id) {
                Some(key) => Some(self.lua.registry_value(key)?),
                None => None,
            }
        };
        if let Some(func) = func {
            let _: mlua::Result<()> = func.call(arg);
        }
        Ok(())
    }

    /// Drain queued actions, also installing any [`Action::SetLuaTrigger`]
    /// or [`Action::SubscribeGmcp`] into the engine's own bookkeeping
    /// before returning the rest to the caller.
    fn drain(&mut self) -> ScriptOutcome {
        let mut outcome = ScriptOutcome::default();
        let actions: Vec<Action> = match self.state.cell.lock() {
            Ok(mut s) => std::mem::take(&mut s.pending),
            Err(_) => Vec::new(),
        };
        for action in actions {
            match action {
                Action::SetLuaTrigger {
                    name,
                    pattern,
                    callback_id,
                    priority,
                } => match Regex::new(&pattern) {
                    Ok(regex) => {
                        let capture_names: Vec<Option<String>> = regex
                            .capture_names()
                            .map(|n| n.map(str::to_string))
                            .skip(1)
                            .collect();
                        let mut to_drop: Vec<i64> = Vec::new();
                        self.triggers.retain(|t| {
                            if t.name == name {
                                to_drop.push(t.callback_id);
                                false
                            } else {
                                true
                            }
                        });
                        for id in to_drop {
                            self.drop_callback_inline(id);
                        }
                        self.triggers.push(LuaTrigger {
                            name,
                            pattern,
                            regex,
                            callback_id,
                            capture_names,
                            priority,
                            enabled: true,
                        });
                    }
                    Err(e) => {
                        outcome.actions.push(Action::Log(format!(
                            "lua trigger `{name}` rejected: invalid regex {e}"
                        )));
                        self.drop_callback_inline(callback_id);
                    }
                },
                Action::RemoveLuaTrigger(name) => {
                    let mut to_drop: Vec<i64> = Vec::new();
                    self.triggers.retain(|t| {
                        if t.name == name {
                            to_drop.push(t.callback_id);
                            false
                        } else {
                            true
                        }
                    });
                    for id in to_drop {
                        self.drop_callback_inline(id);
                    }
                }
                Action::SubscribeGmcp {
                    package,
                    callback_id,
                } => {
                    self.gmcp_subs.entry(package).or_default().push(callback_id);
                }
                Action::DropCallback(id) => {
                    self.drop_callback_inline(id);
                }
                other => outcome.actions.push(other),
            }
        }
        outcome
    }

    fn drop_callback_inline(&self, callback_id: i64) {
        if let Ok(mut s) = self.state.cell.lock() {
            s.callbacks.remove(&callback_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(code: &str) -> Vec<Action> {
        let mut e = ScriptEngine::new().unwrap();
        e.eval(code, "test").unwrap().actions
    }

    #[test]
    fn send_queues_action() {
        let actions = run(r#"mud.send("look")"#);
        assert_eq!(actions, vec![Action::Send("look".into())]);
    }

    #[test]
    fn echo_queues_action() {
        let actions = run(r#"mud.echo("hi")"#);
        assert_eq!(actions, vec![Action::Echo("hi".into())]);
    }

    #[test]
    fn alias_queues_set_alias() {
        let actions = run(r#"mud.alias("greet", "wave;bow")"#);
        assert_eq!(
            actions,
            vec![Action::SetAlias {
                name: "greet".into(),
                expansion: "wave;bow".into()
            }]
        );
    }

    #[test]
    fn set_var_queues_action_and_updates_snapshot() {
        let mut e = ScriptEngine::new().unwrap();
        let actions = e.eval(r#"mud.set_var("hp", "100")"#, "t").unwrap().actions;
        assert!(matches!(
            actions[0],
            Action::SetVar {
                scope: VarScope::Session,
                ..
            }
        ));
        let actions2 = e
            .eval(r#"mud.echo(mud.var("hp") or "missing")"#, "t")
            .unwrap()
            .actions;
        assert_eq!(actions2, vec![Action::Echo("100".into())]);
    }

    #[test]
    fn trigger_fires_callback_on_match() {
        // Patterns are Rust regex (the regex crate), not Lua patterns. The
        // \w needs an extra backslash to survive the Lua string literal.
        let mut e = ScriptEngine::new().unwrap();
        e.eval(
            r#"
            mud.trigger("loot", "(\\w+) is DEAD", function(c)
                mud.send("loot " .. c[2])
            end)
            "#,
            "t",
        )
        .unwrap();
        let outcome = e.match_line("The goblin is DEAD!").unwrap();
        assert_eq!(outcome.actions, vec![Action::Send("loot goblin".into())]);
    }

    #[test]
    fn unsubscribe_trigger_stops_firing() {
        let mut e = ScriptEngine::new().unwrap();
        e.eval(
            r#"
            mud.trigger("t", "boom", function() mud.echo("hit") end)
            "#,
            "t",
        )
        .unwrap();
        assert_eq!(e.match_line("boom").unwrap().actions.len(), 1);
        e.eval(r#"mud.untrigger("t")"#, "t").unwrap();
        assert_eq!(e.match_line("boom").unwrap().actions.len(), 0);
    }

    #[test]
    fn gmcp_subscriber_receives_table() {
        let mut e = ScriptEngine::new().unwrap();
        e.eval(
            r#"
            mud.on_gmcp("Char.Vitals", function(d)
                mud.echo(tostring(d.hp) .. "/" .. tostring(d.maxhp))
            end)
            "#,
            "t",
        )
        .unwrap();
        let outcome = e
            .dispatch_gmcp("Char.Vitals", &serde_json::json!({"hp": 80, "maxhp": 100}))
            .unwrap();
        assert_eq!(outcome.actions, vec![Action::Echo("80/100".into())]);
    }

    #[test]
    fn timer_schedules_action() {
        let actions = run(r#"mud.timer(1.5, function() mud.echo("ping") end)"#);
        match &actions[0] {
            Action::Timer { delay, .. } => {
                assert_eq!(delay.as_millis(), 1500);
            }
            other => panic!("expected timer action, got {other:?}"),
        }
    }

    #[test]
    fn sandbox_blocks_dangerous_globals() {
        let mut e = ScriptEngine::new().unwrap();
        let result: Result<_, _> = e.eval("os.execute('ls')", "t");
        // os.execute is removed; calling nil errors.
        assert!(result.is_err());
        let result: Result<_, _> = e.eval("dofile('foo')", "t");
        assert!(result.is_err());
    }

    #[test]
    fn invalid_trigger_regex_logs_error() {
        let mut e = ScriptEngine::new().unwrap();
        let outcome = e
            .eval(r#"mud.trigger("bad", "[unclosed", function() end)"#, "t")
            .unwrap();
        assert!(matches!(outcome.actions[0], Action::Log(_)));
    }
}

//! Embedded Lua scripting for Vosh. Phase 8.
//!
//! The engine wraps a sandboxed `mlua::Lua` and exposes a `mud.*` API to
//! Lua scripts. Side effects (sends, echoes, alias edits, trigger
//! registration, timer schedules) flow back as [`Action`] values that the
//! session loop applies after each Lua callback returns.

mod actions;
mod api;
mod state;

use std::collections::{HashMap, HashSet};

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

/// Public Lua-trigger record (mirrors `vosh_trigger::Trigger` for the
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
}

/// What [`ScriptEngine::run_body`] puts before a body. It takes the
/// captures table the chunk is called with and ends without a line break,
/// so the body's lines keep their numbers.
const BODY_PREFIX: &str = "local captures = ...; ";

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
    /// GMCP subscriptions each loaded script owns, as package and
    /// callback id, so a later run can replace them.
    script_gmcp: HashMap<String, Vec<(String, i64)>>,
}

impl std::fmt::Debug for ScriptEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptEngine")
            .field("lua", &"<mlua::Lua>")
            .field("state", &"<EngineState>")
            .field("trigger_count", &self.triggers.len())
            .field("gmcp_subscriptions", &self.gmcp_subs.len())
            .field("loaded_scripts", &self.loaded_scripts.len())
            .field("script_gmcp", &self.script_gmcp.len())
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
            script_gmcp: HashMap::new(),
        })
    }

    /// True when the engine has at least one registered consumer:
    /// a Lua regex trigger, a GMCP subscription, or a loaded script.
    /// Callers on the per-line hot path use this to skip cloning the
    /// profile var map into the engine when nothing would observe
    /// the snapshot. Cheap (three `is_empty` checks on small
    /// collections).
    pub fn has_handlers(&self) -> bool {
        !self.triggers.is_empty() || !self.gmcp_subs.is_empty() || !self.loaded_scripts.is_empty()
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

    /// Run a body that a trigger's Script action or a script alias holds,
    /// with `captures` bound to a local `captures` table. Lua's
    /// `captures[1]` holds the first string, `captures[2]` the second,
    /// and so on.
    ///
    /// The captures reach Lua as a value, never as source text, so any
    /// bytes survive. The local sits on the body's first line, so an
    /// error names the line the body has it on. A `return` ends the body
    /// as it ends any Lua chunk, and whatever it returns is dropped.
    pub fn run_body(
        &mut self,
        body: &str,
        captures: &[String],
        chunk_name: &str,
    ) -> Result<ScriptOutcome, ScriptError> {
        let captures = self
            .lua
            .create_sequence_from(captures.iter().map(String::as_str))?;
        let mut source = String::with_capacity(BODY_PREFIX.len() + body.len());
        source.push_str(BODY_PREFIX);
        source.push_str(body);
        self.lua
            .load(source)
            .set_name(chunk_name)
            .call::<()>(captures)?;
        Ok(self.drain())
    }

    /// Load a script as a named chunk so a later `reload` can re-execute
    /// it. The name is what the user typed. Loading a name again runs it
    /// the way a reload does.
    pub fn load_script(&mut self, name: &str, code: String) -> Result<ScriptOutcome, ScriptError> {
        let outcome = self.run_script(name, &code)?;
        self.loaded_scripts.insert(name.to_string(), code);
        Ok(outcome)
    }

    /// Re-execute every loaded script. A trigger the script sets again
    /// replaces the one of that name. A GMCP package the script subscribes
    /// to again drops the handlers it made for that package before, and a
    /// package it leaves alone keeps them. Other state, like a timer the
    /// script schedules, is not cleared, so scripts that want clean state
    /// manage it themselves.
    pub fn reload_scripts(&mut self) -> Result<ScriptOutcome, ScriptError> {
        let scripts: Vec<(String, String)> = self
            .loaded_scripts
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let mut acc = ScriptOutcome::default();
        for (name, code) in scripts {
            acc.actions
                .append(&mut self.run_script(&name, &code)?.actions);
        }
        Ok(acc)
    }

    /// Run a loaded script's code. `mud.on_gmcp` has no name to replace a
    /// subscription by, so a run that succeeds and subscribes to a package
    /// drops the handlers the script made for that package on earlier
    /// runs. Without that each reload added one more handler. A package the
    /// run does not subscribe to keeps its handlers, so a script that sets
    /// a global to subscribe only once keeps the one it has. A run that
    /// fails keeps the old handlers and adds none.
    fn run_script(&mut self, name: &str, code: &str) -> Result<ScriptOutcome, ScriptError> {
        // Only this run's actions sit past `start`. An earlier entry that
        // failed can leave its own actions in front of them.
        let start = self.state.cell.lock().map_or(0, |s| s.pending.len());
        let result: mlua::Result<()> = self.lua.load(code).set_name(name).exec();
        if let Err(e) = result {
            self.discard_gmcp_subscriptions_since(start);
            return Err(e.into());
        }
        let made: Vec<(String, i64)> = match self.state.cell.lock() {
            Ok(s) => s
                .pending
                .get(start..)
                .unwrap_or_default()
                .iter()
                .filter_map(|action| match action {
                    Action::SubscribeGmcp {
                        package,
                        callback_id,
                    } => Some((package.clone(), *callback_id)),
                    _ => None,
                })
                .collect(),
            Err(_) => Vec::new(),
        };
        let outcome = self.drain();
        let renewed: HashSet<&str> = made.iter().map(|(package, _)| package.as_str()).collect();
        let (dropped, mut owned): (Vec<_>, Vec<_>) = self
            .script_gmcp
            .remove(name)
            .unwrap_or_default()
            .into_iter()
            .partition(|(package, _)| renewed.contains(package.as_str()));
        for (package, id) in dropped {
            if let Some(ids) = self.gmcp_subs.get_mut(&package) {
                ids.retain(|kept| *kept != id);
                if ids.is_empty() {
                    self.gmcp_subs.remove(&package);
                }
            }
            self.drop_callback_inline(id);
        }
        owned.extend(made);
        self.script_gmcp.insert(name.to_string(), owned);
        Ok(outcome)
    }

    /// Take back the GMCP subscriptions a failed script run queued past
    /// `start` and free their callbacks. Left queued, the next drain from
    /// any source would install them with no script to own them, so no
    /// later reload could drop them. The run's other actions stay queued
    /// as before. A trigger replaces the one of its name and a timer frees
    /// itself when it fires, so neither piles up the same way.
    fn discard_gmcp_subscriptions_since(&self, start: usize) {
        let Ok(mut s) = self.state.cell.lock() else {
            return;
        };
        let start = start.min(s.pending.len());
        let queued = s.pending.split_off(start);
        for action in queued {
            match action {
                Action::SubscribeGmcp { callback_id, .. } => {
                    s.callbacks.remove(&callback_id);
                }
                other => s.pending.push(other),
            }
        }
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
        // Drop the callback after firing; it was a one-shot. Forget its
        // timer id too, so a late `mud.cancel_timer` has nothing to free.
        if let Ok(mut s) = self.state.cell.lock() {
            s.timer_callbacks.retain(|_, id| *id != callback_id);
        }
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

    /// How many Lua callbacks the engine still holds a registry slot for.
    fn held_callbacks(e: &ScriptEngine) -> usize {
        e.state.cell.lock().unwrap().callbacks.len()
    }

    #[test]
    fn cancel_timer_frees_its_callback() {
        let mut e = ScriptEngine::new().unwrap();
        let scheduled = e
            .eval(
                r#"pending = mud.timer(5, function() mud.echo("late") end)"#,
                "t",
            )
            .unwrap()
            .actions;
        let Action::Timer {
            timer_id,
            callback_id,
            ..
        } = scheduled[0]
        else {
            panic!("expected timer action, got {:?}", scheduled[0]);
        };
        assert_eq!(held_callbacks(&e), 1);
        let cancelled = e.eval("mud.cancel_timer(pending)", "t").unwrap().actions;
        // The session still needs the cancel to drop the schedule.
        assert_eq!(cancelled, vec![Action::CancelTimer(timer_id)]);
        assert_eq!(held_callbacks(&e), 0);
        assert!(e.state.cell.lock().unwrap().timer_callbacks.is_empty());
        // A cancelled timer the session already took as due runs nothing.
        let leftover = &e.fire_timer(callback_id).unwrap().actions;
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn cancel_timer_leaves_other_timers_alone() {
        let mut e = ScriptEngine::new().unwrap();
        let scheduled = e
            .eval(
                r#"
                first = mud.timer(5, function() mud.echo("first") end)
                second = mud.timer(5, function() mud.echo("second") end)
                "#,
                "t",
            )
            .unwrap()
            .actions;
        let Action::Timer {
            callback_id: second,
            ..
        } = scheduled[1]
        else {
            panic!("expected timer action, got {:?}", scheduled[1]);
        };
        e.eval("mud.cancel_timer(first)", "t").unwrap();
        assert_eq!(held_callbacks(&e), 1);
        let fired = e.fire_timer(second).unwrap().actions;
        assert_eq!(fired, vec![Action::Echo("second".into())]);
        // A timer that fired leaves nothing behind for a late cancel.
        assert_eq!(held_callbacks(&e), 0);
        assert!(e.state.cell.lock().unwrap().timer_callbacks.is_empty());
    }

    #[test]
    fn firing_a_timer_leaves_other_timers_cancellable() {
        let mut e = ScriptEngine::new().unwrap();
        let scheduled = e
            .eval(
                r#"
                first = mud.timer(5, function() mud.echo("first") end)
                second = mud.timer(1, function() mud.echo("second") end)
                "#,
                "t",
            )
            .unwrap()
            .actions;
        let Action::Timer {
            callback_id: second,
            ..
        } = scheduled[1]
        else {
            panic!("expected timer action, got {:?}", scheduled[1]);
        };
        let fired = e.fire_timer(second).unwrap().actions;
        assert_eq!(fired, vec![Action::Echo("second".into())]);
        assert_eq!(held_callbacks(&e), 1);
        // The pending timer still frees its callback when cancelled.
        e.eval("mud.cancel_timer(first)", "t").unwrap();
        assert_eq!(held_callbacks(&e), 0);
        assert!(e.state.cell.lock().unwrap().timer_callbacks.is_empty());
    }

    #[test]
    fn reload_replaces_gmcp_subscriptions_instead_of_adding() {
        let mut e = ScriptEngine::new().unwrap();
        e.load_script(
            "vitals",
            r#"mud.on_gmcp("Char.Vitals", function(d) mud.echo("hp " .. d.hp) end)"#.into(),
        )
        .unwrap();
        e.reload_scripts().unwrap();
        e.reload_scripts().unwrap();
        let outcome = e
            .dispatch_gmcp("Char.Vitals", &serde_json::json!({"hp": 80}))
            .unwrap();
        assert_eq!(outcome.actions, vec![Action::Echo("hp 80".into())]);
        assert_eq!(held_callbacks(&e), 1);
    }

    #[test]
    fn loading_a_script_again_replaces_its_gmcp_subscriptions() {
        // A plugin reload loads the script again under the same name.
        let code = r#"mud.on_gmcp("Room.Info", function() mud.echo("room") end)"#;
        let mut e = ScriptEngine::new().unwrap();
        e.load_script("plugin:mapper", code.into()).unwrap();
        e.load_script("plugin:mapper", code.into()).unwrap();
        let outcome = e
            .dispatch_gmcp("Room.Info", &serde_json::json!({}))
            .unwrap();
        assert_eq!(outcome.actions, vec![Action::Echo("room".into())]);
        assert_eq!(held_callbacks(&e), 1);
    }

    #[test]
    fn reload_keeps_gmcp_subscriptions_the_script_did_not_make() {
        let mut e = ScriptEngine::new().unwrap();
        e.load_script(
            "vitals",
            r#"mud.on_gmcp("Char.Vitals", function() mud.echo("script") end)"#.into(),
        )
        .unwrap();
        e.load_script(
            "other",
            r#"mud.on_gmcp("Char.Vitals", function() mud.echo("other") end)"#.into(),
        )
        .unwrap();
        e.eval(
            r#"mud.on_gmcp("Char.Vitals", function() mud.echo("typed") end)"#,
            "t",
        )
        .unwrap();
        // A run that claimed handlers it did not make would drop them on
        // the next reload, so reload twice.
        e.reload_scripts().unwrap();
        e.reload_scripts().unwrap();
        let mut echoes = e
            .dispatch_gmcp("Char.Vitals", &serde_json::json!({}))
            .unwrap()
            .actions;
        echoes.sort_by_key(|a| format!("{a:?}"));
        assert_eq!(
            echoes,
            vec![
                Action::Echo("other".into()),
                Action::Echo("script".into()),
                Action::Echo("typed".into()),
            ]
        );
        assert_eq!(held_callbacks(&e), 3);
    }

    #[test]
    fn reload_keeps_a_gmcp_handler_the_script_guards_with_a_global() {
        // Lua globals survive a reload, so a script can subscribe once.
        let mut e = ScriptEngine::new().unwrap();
        e.load_script(
            "vitals",
            r#"
            if not hooked then
                mud.on_gmcp("Char.Vitals", function() mud.echo("guarded") end)
                hooked = true
            end
            mud.on_gmcp("Room.Info", function() mud.echo("room") end)
            "#
            .into(),
        )
        .unwrap();
        e.reload_scripts().unwrap();
        e.reload_scripts().unwrap();
        let data = serde_json::json!({});
        let vitals = e.dispatch_gmcp("Char.Vitals", &data).unwrap().actions;
        assert_eq!(vitals, vec![Action::Echo("guarded".into())]);
        let room = e.dispatch_gmcp("Room.Info", &data).unwrap().actions;
        assert_eq!(room, vec![Action::Echo("room".into())]);
        assert_eq!(held_callbacks(&e), 2);
    }

    #[test]
    fn failed_script_run_keeps_old_gmcp_handlers_and_adds_none() {
        let good = r#"mud.on_gmcp("Char.Vitals", function() mud.echo("v") end)"#;
        let bad = r#"
            mud.on_gmcp("Char.Vitals", function() mud.echo("v") end)
            error("typo")
        "#;
        let mut e = ScriptEngine::new().unwrap();
        e.load_script("vitals", good.into()).unwrap();
        assert!(e.load_script("vitals", bad.into()).is_err());
        // The handler from the good run still works, and only it.
        let vitals = serde_json::json!({});
        let outcome = e.dispatch_gmcp("Char.Vitals", &vitals).unwrap();
        assert_eq!(outcome.actions, vec![Action::Echo("v".into())]);
        assert_eq!(held_callbacks(&e), 1);
        // Fixing the script and reloading leaves exactly one handler.
        e.load_script("vitals", good.into()).unwrap();
        e.reload_scripts().unwrap();
        let outcome = e.dispatch_gmcp("Char.Vitals", &vitals).unwrap();
        assert_eq!(outcome.actions, vec![Action::Echo("v".into())]);
        assert_eq!(held_callbacks(&e), 1);
    }

    #[test]
    fn failed_first_script_load_leaves_no_gmcp_handler() {
        let mut e = ScriptEngine::new().unwrap();
        let bad = r#"
            mud.on_gmcp("Char.Vitals", function() mud.echo("v") end)
            mud.echo("before the typo")
            error("typo")
        "#;
        assert!(e.load_script("vitals", bad.into()).is_err());
        assert_eq!(held_callbacks(&e), 0);
        // The run's other actions still reach the next drain as before.
        let outcome = e
            .dispatch_gmcp("Char.Vitals", &serde_json::json!({}))
            .unwrap();
        let leftover = &outcome.actions;
        assert!(leftover.is_empty(), "{leftover:?}");
        let outcome = e.eval("", "t").unwrap();
        assert_eq!(
            outcome.actions,
            vec![Action::Echo("before the typo".into())]
        );
        let outcome = e
            .dispatch_gmcp("Char.Vitals", &serde_json::json!({}))
            .unwrap();
        let leftover = &outcome.actions;
        assert!(leftover.is_empty(), "{leftover:?}");
        let leftover = &e.loaded_script_names();
        assert!(leftover.is_empty(), "{leftover:?}");
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

    /// Run `code` as a body with `captures`, in a fresh engine.
    fn body(code: &str, captures: &[&str]) -> Result<Vec<Action>, ScriptError> {
        let captures: Vec<String> = captures.iter().map(|c| (*c).to_string()).collect();
        let mut e = ScriptEngine::new().unwrap();
        e.run_body(code, &captures, "body").map(|o| o.actions)
    }

    #[test]
    fn a_body_reads_its_captures_from_index_one() {
        let actions = body(
            "mud.send(captures[1] .. ' ' .. captures[2] .. ' ' .. #captures)",
            &["kick", "dragon"],
        )
        .unwrap();
        assert_eq!(actions, vec![Action::Send("kick dragon 2".into())]);
    }

    #[test]
    fn a_body_gets_each_capture_byte_for_byte() {
        // Quotes, backslashes, line breaks, a NUL before a digit and a
        // closing long bracket. As Lua source text the NUL and the 1
        // read back as one byte.
        let odd = "say \"hi\" \\ back\nslash\r\t\u{0}1 ]] end";
        let actions = body("mud.send(captures[1])", &[odd]).unwrap();
        assert_eq!(actions, vec![Action::Send(odd.into())]);
    }

    #[test]
    fn an_error_names_the_line_of_the_body_it_is_on() {
        let err = body("mud.echo('one')\nerror('boom')", &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains("[string \"body\"]:2: boom"), "{err}");
        let err = body("mud.echo('one')\nlocal = 1", &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains("[string \"body\"]:2:"), "{err}");
    }

    #[test]
    fn a_return_ends_the_body_and_what_it_returns_is_dropped() {
        let early = "mud.send('a')\nif not captures[1] then return end\nmud.send('b')";
        assert_eq!(body(early, &[]).unwrap(), vec![Action::Send("a".into())]);
        assert_eq!(
            body(early, &["go"]).unwrap(),
            vec![Action::Send("a".into()), Action::Send("b".into())]
        );
        let last = "mud.send('a')\nreturn 'dropped', 2";
        assert_eq!(body(last, &[]).unwrap(), vec![Action::Send("a".into())]);
        // A return with more after it in the same block is a syntax
        // error, as in any Lua chunk, so nothing runs.
        assert!(body("mud.send('a')\nreturn\nlocal after = 1", &[]).is_err());
    }

    #[test]
    fn captures_stay_local_to_the_body_and_its_closures() {
        let mut e = ScriptEngine::new().unwrap();
        let actions = e
            .run_body(
                "mud.timer(1, function() mud.send(captures[1]) end)",
                &["later".to_string()],
                "body",
            )
            .unwrap()
            .actions;
        let Action::Timer { callback_id, .. } = actions[0] else {
            panic!("expected a timer, got {actions:?}");
        };
        assert_eq!(
            e.fire_timer(callback_id).unwrap().actions,
            vec![Action::Send("later".into())]
        );
        assert_eq!(
            e.eval("mud.echo(tostring(captures))", "t").unwrap().actions,
            vec![Action::Echo("nil".into())]
        );
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

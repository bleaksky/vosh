//! Embedded Lua scripting for Vosh.
//!
//! The engine wraps a sandboxed `mlua::Lua` and exposes a `mud.*` API to
//! Lua scripts. Side effects (sends, echoes, alias edits, trigger
//! registration, timer schedules) flow back as [`Action`] values that the
//! session loop applies after each Lua callback returns.
//!
//! Every call into Lua runs under the limits in [`limits`] for an
//! [`Owner`]. A call that fails leaves an [`Action::Error`] line, and a
//! call Vosh stops sends nothing it queued and turns its owner off.

mod actions;
mod api;
mod limits;
mod owner;
mod report;
mod state;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use mlua::{Function, Lua, Table, Value};
use regex::Regex;
use thiserror::Error;

pub use actions::Action;
use limits::{Limits, Stop};
pub use owner::Owner;
use owner::Site;
use state::{CallInfo, EngineState};

/// One Lua-defined trigger: a regex matched against incoming MUD lines and
/// the registry id of the Lua callback to invoke when it fires.
struct LuaTrigger {
    name: String,
    pattern: String,
    regex: Regex,
    callback_id: i64,
    capture_names: Vec<Option<String>>,
}

/// One Lua trigger as the `#scripts` listing shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaTriggerInfo {
    pub name: String,
    pub pattern: String,
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
/// queued [`Action`]s the caller should apply, with an
/// [`Action::Error`] line after the actions of each call that failed.
#[derive(Debug, Default)]
pub struct ScriptOutcome {
    pub actions: Vec<Action>,
    /// A call ended in an error or a stop.
    pub failed: bool,
    /// Whose Lua Vosh stopped. The caller turns off a trigger or an
    /// alias named here, and the engine holds the rest off itself.
    pub stopped: Vec<Owner>,
}

impl ScriptOutcome {
    /// Add what `later` holds after what this outcome holds.
    pub fn append(&mut self, later: ScriptOutcome) {
        self.actions.extend(later.actions);
        self.failed |= later.failed;
        self.stopped.extend(later.stopped);
    }
}

/// A script `#script reload` runs again: its owner, its chunk name and
/// its code.
struct LoadedScript {
    owner: Owner,
    chunk: String,
    code: String,
}

/// What one call left behind before its actions drain.
struct Called {
    /// How many actions waited before the call began. The call's own sit
    /// past it.
    start: usize,
    stop: Option<Stop>,
    error: Option<mlua::Error>,
    /// The call queued more actions than one call may.
    dropped: bool,
}

/// One match of a Lua trigger, kept as text until its call makes the
/// captures table.
struct Hit {
    name: String,
    callback_id: i64,
    /// The whole match, then each numbered group.
    groups: Vec<Option<String>>,
    /// Each named group that matched.
    named: Vec<(String, String)>,
}

impl Hit {
    /// The captures table the trigger's function gets. Index 1 holds the
    /// whole match and the groups follow, and a named group shows under
    /// its name too.
    fn to_lua(&self, lua: &Lua) -> mlua::Result<Table> {
        let t = lua.create_table()?;
        for (i, group) in self.groups.iter().enumerate() {
            if let Some(text) = group {
                t.set(i + 1, text.as_str())?;
            }
        }
        for (name, text) in &self.named {
            t.set(name.as_str(), text.as_str())?;
        }
        Ok(t)
    }
}

pub struct ScriptEngine {
    lua: Lua,
    state: EngineState,
    limits: Arc<Limits>,
    triggers: Vec<LuaTrigger>,
    gmcp_subs: HashMap<String, Vec<i64>>,
    /// The scripts `#script reload` runs again, in the order they first
    /// loaded.
    loaded_scripts: Vec<LoadedScript>,
    /// GMCP subscriptions each loaded script owns, as package and
    /// callback id, so a later run can replace them.
    script_gmcp: HashMap<Owner, Vec<(String, i64)>>,
    /// The plugins and loose scripts Vosh stopped. A plugin stays off
    /// until it loads again, and a loose script until `#script reload`.
    stopped: HashSet<Owner>,
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
            .field("stopped", &self.stopped)
            .finish_non_exhaustive()
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
        let limits = Limits::new();
        limits::install(&lua, &limits, lua.create_function(api::mud_log)?)?;
        Ok(Self {
            lua,
            state,
            limits,
            triggers: Vec::new(),
            gmcp_subs: HashMap::new(),
            loaded_scripts: Vec::new(),
            script_gmcp: HashMap::new(),
            stopped: HashSet::new(),
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
        let mut names: Vec<String> = self
            .loaded_scripts
            .iter()
            .map(|script| script.owner.listed_name())
            .collect();
        names.sort();
        names
    }

    /// True while Vosh holds the plugin or loose script `owner` off
    /// after a stop.
    pub fn is_stopped(&self, owner: &Owner) -> bool {
        self.stopped.contains(owner)
    }

    pub fn lua_triggers(&self) -> Vec<LuaTriggerInfo> {
        let mut out: Vec<LuaTriggerInfo> = self
            .triggers
            .iter()
            .map(|t| LuaTriggerInfo {
                name: t.name.clone(),
                pattern: t.pattern.clone(),
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Evaluate a string of Lua you typed, as `#lua <code>` does, under
    /// the chunk name `chunk_name`.
    pub fn eval(&mut self, code: &str, chunk_name: &str) -> ScriptOutcome {
        let called = self.call(&Owner::Typed, |lua| {
            lua.load(code).set_name(chunk_name).exec()
        });
        self.finish(&Owner::Typed, &Site::Entry, called)
    }

    /// Run a body that a trigger's Script action or a script alias holds,
    /// for `owner`, with `captures` bound to a local `captures` table.
    /// Lua's `captures[1]` holds the first string, `captures[2]` the
    /// second, and so on.
    ///
    /// The captures reach Lua as a value, never as source text, so any
    /// bytes survive. The local sits on the body's first line, so an
    /// error names the line the body has it on, after the trigger or
    /// alias it belongs to. A `return` ends the body as it ends any Lua
    /// chunk, and whatever it returns is dropped.
    pub fn run_body(&mut self, owner: &Owner, body: &str, captures: &[String]) -> ScriptOutcome {
        let mut source = String::with_capacity(BODY_PREFIX.len() + body.len());
        source.push_str(BODY_PREFIX);
        source.push_str(body);
        let chunk = owner.body_chunk();
        let called = self.call(owner, |lua| {
            let captures = lua.create_sequence_from(captures.iter().map(String::as_str))?;
            lua.load(source).set_name(chunk).call::<()>(captures)
        });
        self.finish(owner, &Site::Entry, called)
    }

    /// Load a script as a named chunk so a later `reload` can re-execute
    /// it. `owner` names the plugin or the loose file, and `chunk` is the
    /// chunk name its errors name, like `@vitals_alert/main.lua`. Loading
    /// an owner again runs it the way a reload does, and turns it back on
    /// after a stop.
    pub fn load_script(&mut self, owner: Owner, chunk: &str, code: String) -> ScriptOutcome {
        self.stopped.remove(&owner);
        let outcome = self.run_script(&owner, chunk, &code);
        // A stopped script stays on the list, so `#script reload` can
        // bring it back. A script that failed keeps what it had.
        if !outcome.failed || self.stopped.contains(&owner) {
            let script = LoadedScript {
                owner,
                chunk: chunk.to_string(),
                code,
            };
            match self
                .loaded_scripts
                .iter_mut()
                .find(|loaded| loaded.owner == script.owner)
            {
                Some(loaded) => *loaded = script,
                None => self.loaded_scripts.push(script),
            }
        }
        outcome
    }

    /// Re-execute every loaded script in the order they loaded, and go on
    /// past one that fails. A loose script Vosh stopped runs again, and a
    /// plugin Vosh stopped stays off. A trigger the script sets again
    /// replaces the one of that name. A GMCP package the script
    /// subscribes to again drops the handlers it made for that package
    /// before, and a package it leaves alone keeps them. Other state, like
    /// a timer the script schedules, is not cleared, so scripts that want
    /// clean state manage it themselves.
    pub fn reload_scripts(&mut self) -> ScriptOutcome {
        let scripts: Vec<(Owner, String, String)> = self
            .loaded_scripts
            .iter()
            .filter(|script| {
                !(matches!(script.owner, Owner::Plugin(_)) && self.stopped.contains(&script.owner))
            })
            .map(|script| {
                (
                    script.owner.clone(),
                    script.chunk.clone(),
                    script.code.clone(),
                )
            })
            .collect();
        let mut acc = ScriptOutcome::default();
        for (owner, chunk, code) in scripts {
            self.stopped.remove(&owner);
            acc.append(self.run_script(&owner, &chunk, &code));
        }
        acc
    }

    /// Run a loaded script's code. `mud.on_gmcp` has no name to replace a
    /// subscription by, so a run that succeeds and subscribes to a package
    /// drops the handlers the script made for that package on earlier
    /// runs. Without that each reload added one more handler. A package the
    /// run does not subscribe to keeps its handlers, so a script that sets
    /// a global to subscribe only once keeps the one it has. A run that
    /// fails keeps the old handlers and adds none.
    fn run_script(&mut self, owner: &Owner, chunk: &str, code: &str) -> ScriptOutcome {
        let called = self.call(owner, |lua| lua.load(code).set_name(chunk).exec());
        if called.stop.is_some() {
            return self.finish(owner, &Site::Entry, called);
        }
        if called.error.is_some() {
            self.discard_gmcp_subscriptions_since(called.start);
            return self.finish(owner, &Site::Entry, called);
        }
        let made: Vec<(String, i64)> = match self.state.cell.lock() {
            Ok(s) => s
                .pending
                .get(called.start..)
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
        let outcome = self.finish(owner, &Site::Entry, called);
        let renewed: HashSet<&str> = made.iter().map(|(package, _)| package.as_str()).collect();
        let (dropped, mut owned): (Vec<_>, Vec<_>) = self
            .script_gmcp
            .remove(owner)
            .unwrap_or_default()
            .into_iter()
            .partition(|(package, _)| renewed.contains(package.as_str()));
        let dropped: Vec<i64> = dropped.into_iter().map(|(_, id)| id).collect();
        self.forget_callbacks(&dropped);
        owned.extend(made);
        self.script_gmcp.insert(owner.clone(), owned);
        outcome
    }

    /// Take back the GMCP subscriptions a failed script run queued past
    /// `start` and free their callbacks. Left queued, the drain would
    /// install them with no script to own them, so no later reload could
    /// drop them. The run's other actions stay queued as before. A
    /// trigger replaces the one of its name and a timer frees itself when
    /// it fires, so neither piles up the same way.
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

    /// Take back every action a stopped call queued past `start`, and
    /// free the functions it would have registered, so nothing it asked
    /// for happens.
    fn discard_since(&self, start: usize) {
        let Ok(mut s) = self.state.cell.lock() else {
            return;
        };
        let start = start.min(s.pending.len());
        for action in s.pending.split_off(start) {
            s.forget_registration(&action);
        }
    }

    /// Run every Lua trigger against `line`, in the order they were
    /// registered. For each match the callback fires with a captures
    /// table, as a call of its own.
    pub fn match_line(&mut self, line: &str) -> ScriptOutcome {
        let mut hits = Vec::new();
        for trigger in &self.triggers {
            for caps in trigger.regex.captures_iter(line) {
                let groups = caps
                    .iter()
                    .map(|m| m.map(|m| m.as_str().to_string()))
                    .collect();
                let named = trigger
                    .capture_names
                    .iter()
                    .enumerate()
                    .filter_map(|(i, name)| {
                        let name = name.as_ref()?;
                        Some((name.clone(), caps.get(i + 1)?.as_str().to_string()))
                    })
                    .collect();
                hits.push(Hit {
                    name: trigger.name.clone(),
                    callback_id: trigger.callback_id,
                    groups,
                    named,
                });
            }
        }
        let mut acc = ScriptOutcome::default();
        for hit in hits {
            let site = Site::LuaTrigger {
                name: hit.name.clone(),
                callback_id: hit.callback_id,
            };
            acc.append(self.run_callback(hit.callback_id, &site, |lua| {
                Ok(Value::Table(hit.to_lua(lua)?))
            }));
        }
        acc
    }

    /// Fire every callback subscribed to `package` with the JSON `data`,
    /// each as a call of its own.
    pub fn dispatch_gmcp(&mut self, package: &str, data: &serde_json::Value) -> ScriptOutcome {
        let ids = match self.gmcp_subs.get(package) {
            Some(v) => v.clone(),
            None => return ScriptOutcome::default(),
        };
        let mut acc = ScriptOutcome::default();
        for id in ids {
            let site = Site::Gmcp {
                package: package.to_string(),
                callback_id: id,
            };
            acc.append(self.run_callback(id, &site, |lua| api::json_to_lua(lua, data)));
        }
        acc
    }

    /// Fire a one-shot timer callback by its callback id.
    pub fn fire_timer(&mut self, callback_id: i64) -> ScriptOutcome {
        let site = Site::Timer { callback_id };
        let outcome = self.run_callback(callback_id, &site, |_| Ok(Value::Nil));
        // Drop the callback after firing; it was a one-shot. Forget its
        // timer id too, so a late `mud.cancel_timer` has nothing to free.
        if let Ok(mut s) = self.state.cell.lock() {
            s.timer_callbacks.retain(|_, id| *id != callback_id);
        }
        self.drop_callback(callback_id);
        outcome
    }

    /// Call the function Vosh holds as `callback_id` with the value `arg`
    /// makes, as a call of the function's owner. Nothing runs when an
    /// earlier stop or a cancel let the function go.
    fn run_callback(
        &mut self,
        callback_id: i64,
        site: &Site,
        arg: impl FnOnce(&Lua) -> mlua::Result<Value>,
    ) -> ScriptOutcome {
        let owner = match self.state.cell.lock() {
            Ok(s) => s.callbacks.get(&callback_id).map(|cb| cb.owner.clone()),
            Err(_) => None,
        };
        let Some(owner) = owner else {
            return ScriptOutcome::default();
        };
        let called = self.call(&owner, |lua| {
            let func: Option<Function> = {
                let s = self
                    .state
                    .cell
                    .lock()
                    .map_err(|_| mlua::Error::RuntimeError("script state poisoned".into()))?;
                match s.callbacks.get(&callback_id) {
                    Some(cb) => Some(lua.registry_value(&cb.key)?),
                    None => None,
                }
            };
            match func {
                Some(func) => func.call::<()>(arg(lua)?),
                None => Ok(()),
            }
        });
        self.finish(&owner, site, called)
    }

    /// Run `body` as one call of `owner`, under the limits. What the call
    /// queued waits in the pending list for [`Self::finish`].
    fn call(&self, owner: &Owner, body: impl FnOnce(&Lua) -> mlua::Result<()>) -> Called {
        let start = match self.state.cell.lock() {
            Ok(mut s) => {
                s.call = Some(CallInfo::new(owner.clone()));
                s.pending.len()
            }
            Err(_) => 0,
        };
        self.limits.begin(&self.lua);
        let result = body(&self.lua);
        let memory_error = matches!(&result, Err(err) if limits::is_memory_error(err));
        let stop = self.limits.end(&self.lua, memory_error);
        let dropped = match self.state.cell.lock() {
            Ok(mut s) => s.call.take().is_some_and(|call| call.dropped),
            Err(_) => false,
        };
        Called {
            start,
            stop,
            error: result.err(),
            dropped,
        }
    }

    /// Drain what one call queued, with a line for its error or its stop
    /// and one when it queued too much. A stopped call drops everything
    /// it queued and turns its owner off.
    fn finish(&mut self, owner: &Owner, site: &Site, called: Called) -> ScriptOutcome {
        if let Some(stop) = called.stop {
            self.discard_since(called.start);
            self.stop_owner(owner, site);
            let mut outcome = self.drain();
            outcome.actions.extend(
                report::stop_lines(owner, site, &stop)
                    .into_iter()
                    .map(Action::Error),
            );
            outcome.failed = true;
            outcome.stopped.push(owner.clone());
            return outcome;
        }
        let mut outcome = self.drain();
        if let Some(err) = called.error {
            outcome.actions.push(Action::Error(report::describe(&err)));
            outcome.failed = true;
        }
        if called.dropped {
            outcome
                .actions
                .push(Action::Error(report::cap_line(owner, site)));
        }
        outcome
    }

    /// Turn off what a stop of `owner` in `site` leaves off. A plugin
    /// and a loose script lose every function they handed over and stay
    /// stopped. A trigger or an alias loses its functions, and the caller
    /// turns it off. A function from a `#lua` line goes alone.
    fn stop_owner(&mut self, owner: &Owner, site: &Site) {
        match owner {
            Owner::Typed => {
                if let Some(id) = site.callback_id() {
                    self.forget_callbacks(&[id]);
                }
            }
            Owner::Plugin(_) | Owner::Script(_) => {
                self.stopped.insert(owner.clone());
                self.forget_owned(owner);
            }
            Owner::Trigger(_) | Owner::Alias(_) => self.forget_owned(owner),
        }
    }

    /// Let go of every function `owner` handed over.
    fn forget_owned(&mut self, owner: &Owner) {
        let ids: Vec<i64> = match self.state.cell.lock() {
            Ok(s) => s
                .callbacks
                .iter()
                .filter(|(_, cb)| cb.owner == *owner)
                .map(|(id, _)| *id)
                .collect(),
            Err(_) => Vec::new(),
        };
        self.forget_callbacks(&ids);
        self.script_gmcp.remove(owner);
    }

    /// Let go of the functions `ids` names, and of the Lua triggers, GMCP
    /// subscriptions and timers that would call them.
    fn forget_callbacks(&mut self, ids: &[i64]) {
        if ids.is_empty() {
            return;
        }
        if let Ok(mut s) = self.state.cell.lock() {
            for id in ids {
                s.callbacks.remove(id);
            }
            s.timer_callbacks.retain(|_, id| !ids.contains(id));
        }
        self.triggers.retain(|t| !ids.contains(&t.callback_id));
        for subs in self.gmcp_subs.values_mut() {
            subs.retain(|id| !ids.contains(id));
        }
        self.gmcp_subs.retain(|_, subs| !subs.is_empty());
        for owned in self.script_gmcp.values_mut() {
            owned.retain(|(_, id)| !ids.contains(id));
        }
    }

    /// Drop a callback's registry key. Idempotent.
    fn drop_callback(&self, callback_id: i64) {
        if let Ok(mut s) = self.state.cell.lock() {
            s.callbacks.remove(&callback_id);
        }
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
                            self.drop_callback(id);
                        }
                        self.triggers.push(LuaTrigger {
                            name,
                            pattern,
                            regex,
                            callback_id,
                            capture_names,
                        });
                    }
                    Err(e) => {
                        outcome.actions.push(Action::Log(format!(
                            "lua trigger `{name}` rejected: invalid regex {e}"
                        )));
                        self.drop_callback(callback_id);
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
                        self.drop_callback(id);
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
}

#[cfg(test)]
mod tests {
    use vosh_automation::vars::Scope;

    use super::*;

    fn run(code: &str) -> Vec<Action> {
        let mut e = ScriptEngine::new().unwrap();
        e.eval(code, "test").unwrap().actions
    }

    /// `unwrap` on an outcome checks that no call in it failed, as it
    /// checked the `Result` the engine returned before every error
    /// became a line.
    trait Succeeded {
        fn unwrap(self) -> ScriptOutcome;
    }

    impl Succeeded for ScriptOutcome {
        #[track_caller]
        fn unwrap(self) -> ScriptOutcome {
            assert!(!self.failed, "{:?}", self.actions);
            self
        }
    }

    /// The text of the first error line in `outcome`.
    fn error_line(outcome: &ScriptOutcome) -> String {
        outcome
            .actions
            .iter()
            .find_map(|action| match action {
                Action::Error(line) => Some(line.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no error line in {:?}", outcome.actions))
    }

    /// Load `code` as the loose script `name`.
    fn load(e: &mut ScriptEngine, name: &str, code: &str) -> ScriptOutcome {
        e.load_script(Owner::Script(name.into()), &format!("@{name}"), code.into())
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
                scope: Scope::Session,
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
        load(
            &mut e,
            "vitals.lua",
            r#"mud.on_gmcp("Char.Vitals", function(d) mud.echo("hp " .. d.hp) end)"#,
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
        let mapper = Owner::Plugin("mapper".into());
        e.load_script(mapper.clone(), "@mapper/main.lua", code.into())
            .unwrap();
        e.load_script(mapper, "@mapper/main.lua", code.into())
            .unwrap();
        let outcome = e
            .dispatch_gmcp("Room.Info", &serde_json::json!({}))
            .unwrap();
        assert_eq!(outcome.actions, vec![Action::Echo("room".into())]);
        assert_eq!(held_callbacks(&e), 1);
    }

    #[test]
    fn reload_keeps_gmcp_subscriptions_the_script_did_not_make() {
        let mut e = ScriptEngine::new().unwrap();
        load(
            &mut e,
            "vitals.lua",
            r#"mud.on_gmcp("Char.Vitals", function() mud.echo("script") end)"#,
        )
        .unwrap();
        load(
            &mut e,
            "other.lua",
            r#"mud.on_gmcp("Char.Vitals", function() mud.echo("other") end)"#,
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
        load(
            &mut e,
            "vitals.lua",
            r#"
            if not hooked then
                mud.on_gmcp("Char.Vitals", function() mud.echo("guarded") end)
                hooked = true
            end
            mud.on_gmcp("Room.Info", function() mud.echo("room") end)
            "#,
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
        load(&mut e, "vitals.lua", good).unwrap();
        assert!(load(&mut e, "vitals.lua", bad).failed);
        // The handler from the good run still works, and only it.
        let vitals = serde_json::json!({});
        let outcome = e.dispatch_gmcp("Char.Vitals", &vitals).unwrap();
        assert_eq!(outcome.actions, vec![Action::Echo("v".into())]);
        assert_eq!(held_callbacks(&e), 1);
        // Fixing the script and reloading leaves exactly one handler.
        load(&mut e, "vitals.lua", good).unwrap();
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
        let outcome = load(&mut e, "vitals.lua", bad);
        assert!(outcome.failed);
        assert_eq!(held_callbacks(&e), 0);
        // The run's other actions come back with its error line.
        assert_eq!(
            outcome.actions,
            vec![
                Action::Echo("before the typo".into()),
                Action::Error("vitals.lua:4: typo".into()),
            ]
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
        // Each is removed, so calling it calls nil.
        for code in [
            "os.execute('ls')",
            "dofile('foo')",
            "require('io')",
            "load('return 1')",
        ] {
            assert!(e.eval(code, "=#lua").failed, "{code}");
        }
        let gone = e
            .eval("mud.echo(tostring(io) .. tostring(package))", "=#lua")
            .unwrap();
        assert_eq!(gone.actions, vec![Action::Echo("nilnil".into())]);
    }

    /// Run `code` as the body of the trigger `body`, in a fresh engine.
    fn run_body(code: &str, captures: &[&str]) -> ScriptOutcome {
        let captures: Vec<String> = captures.iter().map(|c| (*c).to_string()).collect();
        let mut e = ScriptEngine::new().unwrap();
        e.run_body(&Owner::Trigger("body".into()), code, &captures)
    }

    /// Run `code` as a body with `captures`, in a fresh engine, and
    /// return its actions or its error line.
    fn body(code: &str, captures: &[&str]) -> Result<Vec<Action>, String> {
        let outcome = run_body(code, captures);
        if outcome.failed {
            Err(error_line(&outcome))
        } else {
            Ok(outcome.actions)
        }
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
        let err = body("mud.echo('one')\nerror('boom')", &[]).unwrap_err();
        assert_eq!(err, "trigger body:2: boom");
        let err = body("mud.echo('one')\nlocal = 1", &[]).unwrap_err();
        assert!(err.starts_with("trigger body:2:"), "{err}");
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
                &Owner::Alias("later".into()),
                "mud.timer(1, function() mud.send(captures[1]) end)",
                &["later".to_string()],
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

    /// The lines Vosh printed about the Lua, errors and stops alike.
    fn error_lines(outcome: &ScriptOutcome) -> Vec<String> {
        outcome
            .actions
            .iter()
            .filter_map(|action| match action {
                Action::Error(line) => Some(line.clone()),
                _ => None,
            })
            .collect()
    }

    /// Run `code` as a `#lua` line, which must stop on the time limit
    /// soon after 100 ms, and return what it left.
    #[track_caller]
    fn stops_in_time(e: &mut ScriptEngine, code: &str) -> ScriptOutcome {
        let started = std::time::Instant::now();
        let outcome = e.eval(code, "=#lua");
        let took = started.elapsed();
        assert!(took < std::time::Duration::from_secs(5), "{took:?}");
        assert!(took >= limits::TIME_BUDGET, "{took:?}");
        assert!(outcome.failed);
        assert_eq!(
            error_lines(&outcome),
            ["Vosh stopped your #lua line after 100 ms."]
        );
        outcome
    }

    #[test]
    fn a_loop_inside_coroutine_wrap_stops() {
        let mut e = ScriptEngine::new().unwrap();
        stops_in_time(
            &mut e,
            "local spin = coroutine.wrap(function() while true do end end) spin()",
        );
        // A coroutine that catches the stop itself rethrows it too.
        stops_in_time(
            &mut e,
            "coroutine.wrap(function() \
               while true do pcall(function() while true do end end) end \
             end)()",
        );
        // So does one coroutine inside another, through resume.
        stops_in_time(
            &mut e,
            "local inner = coroutine.create(function() while true do end end) \
             local outer = coroutine.create(function() \
               while true do coroutine.resume(inner) end \
             end) \
             while true do coroutine.resume(outer) end",
        );
    }

    #[test]
    fn a_loop_after_a_coroutine_yields_still_stops() {
        // The hook goes back to the thread that resumed the coroutine.
        let mut e = ScriptEngine::new().unwrap();
        stops_in_time(
            &mut e,
            "local step = coroutine.wrap(function() coroutine.yield() end) \
             step() while true do end",
        );
        // A coroutine left suspended by one call leaves the next call's
        // hook on the main thread.
        e.eval(
            "parked = coroutine.create(function() coroutine.yield() end) \
             coroutine.resume(parked)",
            "=#lua",
        )
        .unwrap();
        stops_in_time(&mut e, "while true do end");
        // A coroutine that yields and resumes keeps working.
        let counted = e
            .eval(
                "local count = coroutine.wrap(function() \
                   for i = 1, 3 do coroutine.yield(i) end \
                 end) \
                 mud.echo(count() .. count() .. count())",
                "=#lua",
            )
            .unwrap();
        assert_eq!(counted.actions, vec![Action::Echo("123".into())]);
    }

    #[test]
    fn a_loop_inside_pcall_inside_a_loop_stops() {
        let mut e = ScriptEngine::new().unwrap();
        stops_in_time(
            &mut e,
            "while true do pcall(function() while true do end end) end",
        );
        stops_in_time(
            &mut e,
            "while true do \
               xpcall(function() while true do end end, function(err) return err end) \
             end",
        );
        // A handler that spins is stopped too.
        stops_in_time(
            &mut e,
            "while true do \
               xpcall(error, function() while true do end end) \
             end",
        );
        // pcall still catches an ordinary error.
        let caught = e
            .eval(
                "local ok, err = pcall(error, 'boom', 0) mud.echo(tostring(ok) .. ' ' .. err)",
                "=#lua",
            )
            .unwrap();
        assert_eq!(caught.actions, vec![Action::Echo("false boom".into())]);
    }

    #[test]
    fn a_memory_grab_inside_pcall_stops() {
        let mut e = ScriptEngine::new().unwrap();
        let outcome = e.eval(
            "mud.send('look')\n\
             local ok = pcall(function()\n\
               local t = {}\n\
               for i = 1, 1e9 do t[i] = string.rep('x', 64) .. i end\n\
             end)\n\
             mud.send('survived ' .. tostring(ok))",
            "=#lua",
        );
        assert!(outcome.failed);
        assert_eq!(
            error_lines(&outcome),
            ["Vosh stopped your #lua line. One call used more than 32 MB."]
        );
        // A stopped call sends nothing it queued, the send before the
        // grab included.
        assert!(!outcome
            .actions
            .iter()
            .any(|action| matches!(action, Action::Send(_))));
        // The memory comes back once the garbage goes, and the next call
        // runs.
        e.eval("mud.echo('next')", "=#lua").unwrap();
    }

    #[test]
    fn a_state_over_128_mb_stops_whoever_reaches_it() {
        let mut e = ScriptEngine::new().unwrap();
        e.eval("held = {}", "=#lua").unwrap();
        // Each call holds 10 MB more, and building the string takes 20
        // MB for a moment, under the 32 MB one call may use.
        let grab = "held[#held + 1] = string.rep('x', 10 * 1024 * 1024)";
        let mut grabs = 0;
        let outcome = loop {
            let outcome = e.eval(grab, "=#lua");
            if outcome.failed {
                break outcome;
            }
            grabs += 1;
            assert!(grabs < 20, "the state never filled");
        };
        assert!(grabs >= 10, "{grabs}");
        assert_eq!(
            error_lines(&outcome),
            ["Vosh stopped your #lua line. Your scripts hold more than 128 MB."]
        );
        // Letting go of what it holds brings the state back.
        e.eval("held = {} collectgarbage()", "=#lua").unwrap();
        e.eval(grab, "=#lua").unwrap();
    }

    #[test]
    fn lua_that_runs_with_the_hook_off_cannot_spin() {
        let mut e = ScriptEngine::new().unwrap();
        // A __gc method runs with every hook off, so none may be set.
        let refused = e.eval("setmetatable({}, {__gc = function() end})", "=#lua");
        assert_eq!(
            error_lines(&refused),
            ["#lua:1: Vosh does not run __gc methods"]
        );
        e.eval(
            "local t = setmetatable({}, {__index = {hp = 80}}) mud.echo(t.hp)",
            "=#lua",
        )
        .unwrap();
        // Closing a coroutine runs its __close methods on the
        // coroutine, under the hook.
        stops_in_time(
            &mut e,
            "local c = coroutine.create(function() \
               local guard <close> = setmetatable({}, {__close = function() \
                 while true do end \
               end}) \
               coroutine.yield() \
             end) \
             coroutine.resume(c) \
             coroutine.close(c)",
        );
    }

    #[test]
    fn a_send_flood_keeps_the_first_100_and_says_so() {
        let mut e = ScriptEngine::new().unwrap();
        let outcome = e.eval("for i = 1, 1000 do mud.send('look') end", "=#lua");
        // The cap drops actions and stops nothing.
        assert!(!outcome.failed);
        let sends = outcome
            .actions
            .iter()
            .filter(|action| matches!(action, Action::Send(_)))
            .count();
        assert_eq!(sends, 100);
        assert_eq!(
            error_lines(&outcome),
            ["Your #lua line queued more than 100 actions in one call. Vosh dropped the rest."]
        );
        // A function a dropped action would have registered goes too.
        e.eval(
            "for i = 1, 150 do mud.timer(5, function() end) end",
            "=#lua",
        );
        assert_eq!(held_callbacks(&e), 100);
        // The next call starts a new count.
        let next = e.eval("mud.send('look')", "=#lua").unwrap();
        assert_eq!(next.actions, vec![Action::Send("look".into())]);
    }

    #[test]
    fn print_and_errors_become_lines() {
        let mut e = ScriptEngine::new().unwrap();
        let printed = e.eval("print('hp', 80, nil, true)", "=#lua").unwrap();
        assert_eq!(
            printed.actions,
            vec![Action::Log("hp\t80\tnil\ttrue".into())]
        );
        // A callback error no longer vanishes.
        e.eval(
            "mud.on_gmcp('Char.Vitals', function(d) mud.echo(d.hp .. d.hp_pct) end)",
            "=#lua",
        )
        .unwrap();
        let outcome = e.dispatch_gmcp("Char.Vitals", &serde_json::json!({"hp": 80}));
        assert!(outcome.failed);
        assert_eq!(
            error_lines(&outcome),
            ["#lua:1: attempt to concatenate a nil value (field 'hp_pct')"]
        );
        // An error from a mud function names the line that called it.
        let outcome = e.eval("mud.echo('one')\nmud.send()", "=#lua");
        assert_eq!(outcome.actions[0], Action::Echo("one".into()));
        let line = error_line(&outcome);
        assert!(line.starts_with("#lua:2: bad argument #1"), "{line}");
    }

    #[test]
    fn a_stopped_plugin_names_its_file_and_line_and_stays_off() {
        let mut e = ScriptEngine::new().unwrap();
        let wait_full = Owner::Plugin("wait_full".into());
        let code = "-- wait_full\n\
                    -- Stand up once your hit points are full.\n\
                    \n\
                    mud.on_gmcp(\"Char.Vitals\", function(data)\n\
                      while data.hp < data.maxhp do\n\
                      end\n\
                      mud.send(\"stand\")\n\
                    end)\n";
        e.load_script(wait_full.clone(), "@wait_full/main.lua", code.into())
            .unwrap();
        let vitals = serde_json::json!({"hp": 186, "maxhp": 1020});
        let outcome = e.dispatch_gmcp("Char.Vitals", &vitals);
        assert_eq!(
            error_lines(&outcome),
            [
                "Vosh stopped wait_full at main.lua line 5 after 100 ms.",
                "wait_full stays off until you save it under Scripts in Settings or restart Vosh.",
            ]
        );
        assert_eq!(outcome.stopped, std::slice::from_ref(&wait_full));
        assert!(e.is_stopped(&wait_full));
        // Its handler is gone, so the next packet runs nothing, and a
        // reload leaves it off.
        let leftover = &e.dispatch_gmcp("Char.Vitals", &vitals).actions;
        assert!(leftover.is_empty(), "{leftover:?}");
        let leftover = &e.reload_scripts().actions;
        assert!(leftover.is_empty(), "{leftover:?}");
        let leftover = &e.dispatch_gmcp("Char.Vitals", &vitals).actions;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(held_callbacks(&e), 0);
        // Loading it again turns it back on.
        e.load_script(wait_full.clone(), "@wait_full/main.lua", code.into())
            .unwrap();
        assert!(!e.is_stopped(&wait_full));
        let full = serde_json::json!({"hp": 1020, "maxhp": 1020});
        assert_eq!(
            e.dispatch_gmcp("Char.Vitals", &full).actions,
            vec![Action::Send("stand".into())]
        );
    }

    #[test]
    fn a_plugin_load_that_runs_away_stays_off() {
        let mut e = ScriptEngine::new().unwrap();
        let spin = Owner::Plugin("spin".into());
        let outcome = e.load_script(
            spin.clone(),
            "@spin/main.lua",
            "mud.send('look')\nwhile true do end".into(),
        );
        assert_eq!(
            error_lines(&outcome),
            [
                "Vosh stopped spin at main.lua line 2 after 100 ms.",
                "spin stays off until you save it under Scripts in Settings or restart Vosh.",
            ]
        );
        assert!(!outcome
            .actions
            .iter()
            .any(|action| matches!(action, Action::Send(_))));
        assert!(e.is_stopped(&spin));
    }

    #[test]
    fn a_stopped_loose_script_stays_off_until_reload() {
        let mut e = ScriptEngine::new().unwrap();
        let combat = Owner::Script("combat.lua".into());
        load(
            &mut e,
            "combat.lua",
            "mud.trigger('hunger', 'You are hungry', function() while true do end end)",
        )
        .unwrap();
        let outcome = e.match_line("You are hungry.");
        assert_eq!(
            error_lines(&outcome),
            ["Vosh stopped combat.lua after 100 ms. It stays off until #script reload."]
        );
        assert!(e.is_stopped(&combat));
        let leftover = &e.lua_triggers();
        assert!(leftover.is_empty(), "{leftover:?}");
        // A reload runs it again and brings its trigger back.
        e.reload_scripts().unwrap();
        assert!(!e.is_stopped(&combat));
        assert_eq!(e.lua_triggers().len(), 1);
        // A load that runs away stays listed, so a reload can bring it
        // back.
        let outcome = load(&mut e, "spin.lua", "while true do end");
        assert_eq!(
            error_lines(&outcome),
            ["Vosh stopped spin.lua after 100 ms. It stays off until #script reload."]
        );
        assert!(e.loaded_script_names().contains(&"spin.lua".to_string()));
    }

    #[test]
    fn a_body_that_runs_away_names_its_trigger_or_alias() {
        let mut e = ScriptEngine::new().unwrap();
        let tells = Owner::Trigger("tells".into());
        let outcome = e.run_body(&tells, "mud.send('look') while true do end", &[]);
        assert_eq!(
            error_lines(&outcome),
            ["Vosh stopped the Lua in trigger tells after 100 ms. tells stays off until you save it or restart Vosh."]
        );
        assert_eq!(outcome.stopped, [tells]);
        assert_eq!(outcome.actions.len(), 1);
        let heal = Owner::Alias("heal".into());
        let outcome = e.run_body(&heal, "while true do end", &[]);
        assert_eq!(
            error_lines(&outcome),
            ["Vosh stopped the Lua in alias heal after 100 ms. heal stays off until you save it or restart Vosh."]
        );
        assert_eq!(outcome.stopped, [heal]);
    }

    #[test]
    fn a_function_a_body_left_behind_stops_its_trigger() {
        let mut e = ScriptEngine::new().unwrap();
        let tells = Owner::Trigger("tells".into());
        let made = e
            .run_body(
                &tells,
                "mud.timer(0, function() while true do end end) \
                 mud.on_gmcp('Char.Vitals', function() end)",
                &[],
            )
            .unwrap();
        let Action::Timer { callback_id, .. } = made.actions[0] else {
            panic!("expected a timer, got {:?}", made.actions);
        };
        let outcome = e.fire_timer(callback_id);
        assert_eq!(outcome.stopped, [tells]);
        assert_eq!(
            error_lines(&outcome),
            ["Vosh stopped the Lua in trigger tells after 100 ms. tells stays off until you save it or restart Vosh."]
        );
        // Every function the trigger handed over went with it.
        assert_eq!(held_callbacks(&e), 0);
    }

    #[test]
    fn a_function_a_lua_line_left_behind_goes_alone() {
        let mut e = ScriptEngine::new().unwrap();
        e.eval(
            "mud.trigger('spin', 'The day has begun', function() while true do end end) \
             mud.trigger('day', 'The day has begun', function() mud.echo('day') end) \
             mud.on_gmcp('Room.Info', function() while true do end end)",
            "=#lua",
        )
        .unwrap();
        let outcome = e.match_line("The day has begun.");
        assert_eq!(
            error_lines(&outcome),
            ["Vosh stopped the Lua trigger spin after 100 ms. Vosh removed it."]
        );
        assert!(outcome.actions.contains(&Action::Echo("day".into())));
        assert_eq!(e.lua_triggers().len(), 1);
        let outcome = e.dispatch_gmcp("Room.Info", &serde_json::json!({}));
        assert_eq!(
            error_lines(&outcome),
            ["Vosh stopped a Room.Info handler after 100 ms. Vosh removed it."]
        );
        let leftover = &e.dispatch_gmcp("Room.Info", &serde_json::json!({})).actions;
        assert!(leftover.is_empty(), "{leftover:?}");
        let made = e
            .eval("mud.timer(0, function() while true do end end)", "=#lua")
            .unwrap();
        let Action::Timer { callback_id, .. } = made.actions[0] else {
            panic!("expected a timer, got {:?}", made.actions);
        };
        assert_eq!(
            error_lines(&e.fire_timer(callback_id)),
            ["Vosh stopped a timer after 100 ms."]
        );
        // The trigger left over still runs.
        assert_eq!(
            e.match_line("The day has begun.").actions,
            vec![Action::Echo("day".into())]
        );
    }
}

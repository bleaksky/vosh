//! Embedded Lua scripting for Vosh.
//!
//! The engine wraps a sandboxed `mlua::Lua` and exposes a `mud.*` API to
//! Lua scripts. Side effects (sends, echoes, alias edits, trigger
//! registration, timer schedules) flow back as [`Action`] values that the
//! session loop applies after each Lua callback returns.
//!
//! Every call into Lua runs under the limits in [`limits`] for an
//! [`Owner`]. A call that fails leaves an [`Action::Error`] line, and a
//! call Vosh stops sends nothing it queued and turns its owner off. On
//! top of that, each plugin and loose script has the time [`budget`]
//! gives it for one event, across all its handlers.

mod actions;
mod api;
mod budget;
mod env;
mod hook;
mod library;
mod limits;
mod owner;
mod pane;
mod pattern;
mod report;
mod state;
mod strings;
#[cfg(test)]
mod test_support;
#[cfg(any(test, feature = "testkit"))]
pub mod testkit;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mlua::{Function, Lua, Table, Value};
use regex::Regex;
use thiserror::Error;

pub use actions::{Action, PaneBlock, Place};
use budget::{Budget, Event};
use env::Envs;
pub use limits::StopReason;
use limits::{Limits, Stop};
pub use owner::Owner;
use owner::Site;
use state::{CallInfo, EngineState};

/// One Lua-defined trigger: a regex matched against incoming MUD lines and
/// the registry id of the Lua callback to invoke when it fires. Its name
/// is its own among the triggers its owner shares names with.
struct LuaTrigger {
    owner: Owner,
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
    /// Who registered it, as `Owner::listed_name` gives it. That names a
    /// plugin, a loose file, `#lua`, or the trigger or alias whose Lua
    /// made it.
    pub owner: String,
}

#[derive(Debug, Error)]
pub enum ScriptError {
    #[error("lua error: {0}")]
    Lua(#[from] mlua::Error),
}

/// The GMCP packages whose last packet Vosh never keeps for a new
/// handler. Each chat packet is one message and not a state, so a new
/// handler waits for the next one.
const UNKEPT_PACKAGES: &[&str] = &["Comm.Channel"];

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

/// What one round of Lua timers did.
#[derive(Debug, Default)]
pub struct FiredTimers {
    pub outcome: ScriptOutcome,
    /// The callback ids of the timers whose owner used its time for the
    /// round. They never ran, and the caller schedules them again for the
    /// next round.
    pub held: Vec<i64>,
}

/// What one call left behind before its actions drain.
struct Called {
    /// How many actions waited before the call began. The call's own sit
    /// past it.
    start: usize,
    /// How long the call's Lua ran.
    took: Duration,
    stop: Option<Stop>,
    error: Option<mlua::Error>,
    /// The call queued more actions than one call may.
    dropped: bool,
    /// The call queued more text than one call may.
    text_dropped: bool,
    /// The call gave a pane more blocks than one pane shows.
    blocks_dropped: bool,
}

/// What became of one handler in an event.
enum Handled {
    /// It ran, or its function had gone and nothing ran.
    Ran(ScriptOutcome),
    /// Its owner had used its time for the event, so it never started.
    /// The outcome holds the line that says so, the first time only.
    Skipped(ScriptOutcome),
}

impl Handled {
    fn outcome(self) -> ScriptOutcome {
        match self {
            Handled::Ran(outcome) | Handled::Skipped(outcome) => outcome,
        }
    }
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
    /// The plugins and loose scripts that loaded, in the order they first
    /// loaded, which `#script reload` follows.
    loaded_scripts: Vec<Owner>,
    /// The plugins and loose scripts Vosh stopped, with why. A plugin
    /// stays off until it loads again, and a loose script until `#script
    /// reload`.
    stopped: HashMap<Owner, StopReason>,
    /// The environment of each plugin that runs.
    envs: Envs,
    /// The last packet of each GMCP package this connection sent, which
    /// a new handler of the package gets at once. Chat packets stay out.
    packets: HashMap<String, serde_json::Value>,
    /// True while a new handler runs on the last packet of its package.
    replaying: bool,
    /// The time each plugin and loose script spent on the event its
    /// handlers answer now, while they answer one.
    budget: Option<Budget>,
}

impl std::fmt::Debug for ScriptEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptEngine")
            .field("lua", &"<mlua::Lua>")
            .field("state", &"<EngineState>")
            .field("trigger_count", &self.triggers.len())
            .field("gmcp_subscriptions", &self.gmcp_subs.len())
            .field("loaded_scripts", &self.loaded_scripts.len())
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
        api::apply_sandbox(&lua)?;
        let limits = Limits::new();
        limits::install(&lua, &limits, lua.create_function(api::mud_log)?)?;
        library::install(&lua)?;
        strings::install(&lua, &limits)?;
        // The plugins read the standard library as it stands now, so
        // before the shared `mud` table joins the globals.
        let envs = Envs::install(&lua)?;
        api::install(&lua)?;
        // The hook goes on last, since Lua that runs outside a call stops
        // at its first look.
        hook::install(&lua, &limits)?;
        Ok(Self {
            lua,
            state,
            limits,
            triggers: Vec::new(),
            gmcp_subs: HashMap::new(),
            loaded_scripts: Vec::new(),
            stopped: HashMap::new(),
            envs,
            packets: HashMap::new(),
            replaying: false,
            budget: None,
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
        let mut names: Vec<String> = self.loaded_scripts.iter().map(Owner::listed_name).collect();
        names.sort();
        names
    }

    /// True while Vosh holds the plugin or loose script `owner` off
    /// after a stop.
    pub fn is_stopped(&self, owner: &Owner) -> bool {
        self.stopped.contains_key(owner)
    }

    /// Why Vosh holds the plugin or loose script `owner` off, while it
    /// does, so the Scripts page can say what stopped it.
    pub fn stop_reason(&self, owner: &Owner) -> Option<StopReason> {
        self.stopped.get(owner).copied()
    }

    pub fn lua_triggers(&self) -> Vec<LuaTriggerInfo> {
        let mut out: Vec<LuaTriggerInfo> = self
            .triggers
            .iter()
            .map(|t| LuaTriggerInfo {
                name: t.name.clone(),
                pattern: t.pattern.clone(),
                owner: t.owner.listed_name(),
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.owner.cmp(&b.owner)));
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

    /// Evaluate a string of Lua you typed in the console of the plugin
    /// `name`, as one call of the plugin in its own environment, so it
    /// reads the plugin's globals and its `mud` table registers for the
    /// plugin. A plugin that is off or stopped has no environment, so
    /// nothing runs and a note says why.
    pub fn eval_in_plugin(&mut self, name: &str, code: &str) -> ScriptOutcome {
        let owner = Owner::Plugin(name.to_string());
        let Some(env) = self.envs.get(name) else {
            let state = if self.is_stopped(&owner) {
                "stopped"
            } else {
                "not running"
            };
            return ScriptOutcome {
                actions: vec![Action::Note {
                    text: format!("{name} is {state}, so the console ran nothing."),
                    owner,
                }],
                ..ScriptOutcome::default()
            };
        };
        // The line runs under the chunk name of a `#lua` line, so an
        // error names it as it names one.
        let chunk = Owner::Typed.body_chunk();
        let called = self.call(&owner, |lua| {
            lua.load(code).set_name(chunk).set_environment(env).exec()
        });
        self.finish(&owner, &Site::Entry, called)
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

    /// Load a plugin or a loose script, or load it again. `owner` names
    /// the plugin or the loose file, and `chunk` is the chunk name its
    /// errors name, like `@vitals_alert/main.lua`.
    ///
    /// A load that succeeds takes the place of all `owner` registered
    /// before, its Lua triggers, GMCP handlers and timers, and a plugin's
    /// aliases, so a load never doubles them. A load that fails registers
    /// nothing and leaves what `owner` had, and its other actions go
    /// ahead. A load Vosh stops leaves `owner` off with nothing
    /// registered. Loading turns an owner back on after a stop.
    pub fn load_script(&mut self, owner: Owner, chunk: &str, code: &str) -> ScriptOutcome {
        self.stopped.remove(&owner);
        let (outcome, ran) = self.run_script(&owner, chunk, code);
        // A stopped script stays on the list, so `#script reload` can
        // bring it back. A plugin that failed takes its place too, since
        // the profile turned it on and a reload should try it again once
        // you fix it. A loose script that failed keeps its place, or
        // never takes one, so a name you mistyped does not stick.
        let listed = ran || self.stopped.contains_key(&owner) || matches!(owner, Owner::Plugin(_));
        if listed && !self.loaded_scripts.contains(&owner) {
            self.loaded_scripts.push(owner);
        }
        outcome
    }

    /// Put the plugin `name` on the list `#script reload` reads, when Vosh
    /// could not read its files to load it, so a reload tries it again
    /// once you fix them, as it does a plugin whose Lua failed.
    pub fn list_unread_plugin(&mut self, name: &str) {
        let owner = Owner::Plugin(name.to_string());
        if !self.loaded_scripts.contains(&owner) {
            self.loaded_scripts.push(owner);
        }
    }

    /// The plugins and loose scripts `#script reload` reads again and
    /// loads, in the order they first loaded. A loose script Vosh stopped
    /// is among them, so a reload brings it back, and a plugin Vosh
    /// stopped is not, since it stays off until you save it or restart
    /// Vosh.
    pub fn reload_order(&self) -> Vec<Owner> {
        self.loaded_scripts
            .iter()
            .filter(|owner| {
                !(matches!(owner, Owner::Plugin(_)) && self.stopped.contains_key(owner))
            })
            .cloned()
            .collect()
    }

    /// The plugins that loaded, a stopped one among them, by name, in the
    /// order they first loaded.
    pub fn loaded_plugins(&self) -> Vec<String> {
        self.loaded_scripts
            .iter()
            .filter_map(|owner| match owner {
                Owner::Plugin(name) => Some(name.clone()),
                _ => None,
            })
            .collect()
    }

    /// Run a loaded script's code as one call of `owner`. A plugin runs
    /// in a new environment of its own, which takes the place of the one
    /// it had once the run succeeds, and a loose script runs in the
    /// global one. A run that succeeds lets go of all `owner` registered
    /// before it began, and one that fails takes back what it registered
    /// itself. Returns what the run asks for, and whether the code ran to
    /// its end, whatever a handler it made did with a packet after.
    fn run_script(&mut self, owner: &Owner, chunk: &str, code: &str) -> (ScriptOutcome, bool) {
        let before = self.owned_callbacks(owner);
        // A plugin's environment is made inside the call, by Lua that
        // runs under the call's own clock, so a failure to make it is the
        // load's error.
        let mut env: Option<Table> = None;
        let envs = &self.envs;
        let called = self.call(owner, |lua| {
            let chunk = lua.load(code).set_name(chunk);
            match owner {
                Owner::Plugin(name) => {
                    let made = envs.create(lua, name)?;
                    env = Some(made.clone());
                    chunk.set_environment(made).exec()
                }
                _ => chunk.exec(),
            }
        });
        if called.stop.is_some() {
            return (self.finish(owner, &Site::Entry, called), false);
        }
        if called.error.is_some() {
            self.discard_registrations_since(called.start);
            return (self.finish(owner, &Site::Entry, called), false);
        }
        let mut outcome = ScriptOutcome {
            actions: self.release(&before),
            ..ScriptOutcome::default()
        };
        if let Owner::Plugin(name) = owner {
            self.envs.set(name, env);
            // What the run made follows, in place of the old.
            outcome.actions.push(Action::DropPlugin(name.clone()));
        }
        outcome.append(self.finish(owner, &Site::Entry, called));
        (outcome, true)
    }

    /// Turn the plugin or loose script `owner` off: let go of the Lua
    /// triggers, GMCP handlers and timers it registered, and a plugin's
    /// aliases, and take it off the list `#script reload` runs. The
    /// variables it set and the groups it turned on or off stay as they
    /// are, and so do the aliases a loose script made, which you keep. A
    /// plugin Vosh stopped stays stopped, so a profile that turns it on
    /// again leaves it off until you save it or restart Vosh.
    pub fn unload(&mut self, owner: &Owner) -> ScriptOutcome {
        self.loaded_scripts.retain(|loaded| loaded != owner);
        if !matches!(owner, Owner::Plugin(_)) {
            self.stopped.remove(owner);
        }
        let owned = self.owned_callbacks(owner);
        let mut actions = self.release(&owned);
        if let Owner::Plugin(name) = owner {
            self.envs.set(name, None);
            actions.push(Action::DropPlugin(name.clone()));
        }
        ScriptOutcome {
            actions,
            ..ScriptOutcome::default()
        }
    }

    /// The functions `owner` handed over that Vosh still holds.
    fn owned_callbacks(&self, owner: &Owner) -> Vec<i64> {
        match self.state.cell.lock() {
            Ok(s) => s
                .callbacks
                .iter()
                .filter(|(_, cb)| cb.owner == *owner)
                .map(|(id, _)| *id)
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Take back the registrations a failed load queued past `start` and
    /// let go of their functions, so its owner keeps what it had. The
    /// load's other actions stay queued.
    fn discard_registrations_since(&self, start: usize) {
        let Ok(mut s) = self.state.cell.lock() else {
            return;
        };
        let start = start.min(s.pending.len());
        for action in s.pending.split_off(start) {
            if action.registers() {
                s.forget_registration(&action);
            } else {
                s.pending.push(action);
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
    /// table, as a call of its own. The line is one event, so a plugin or
    /// loose script that used its time for it skips the rest.
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
        if hits.is_empty() {
            return ScriptOutcome::default();
        }
        self.in_event(Event::Line, |engine| {
            let mut acc = ScriptOutcome::default();
            for hit in hits {
                let site = Site::LuaTrigger {
                    name: hit.name.clone(),
                    callback_id: hit.callback_id,
                };
                let handled = engine.run_callback(hit.callback_id, &site, |lua| {
                    Ok(Value::Table(hit.to_lua(lua)?))
                });
                acc.append(handled.outcome());
            }
            acc
        })
    }

    /// Fire every callback subscribed to `package` with the JSON `data`,
    /// each as a call of its own. The packet is one event, so a plugin or
    /// loose script that used its time for it skips the rest. Vosh keeps
    /// the packet as the last of its package, for the handlers made
    /// later, unless it is a chat packet.
    pub fn dispatch_gmcp(&mut self, package: &str, data: &serde_json::Value) -> ScriptOutcome {
        if !UNKEPT_PACKAGES.contains(&package) {
            self.packets.insert(package.to_string(), data.clone());
        }
        let ids = match self.gmcp_subs.get(package) {
            Some(v) => v.clone(),
            None => return ScriptOutcome::default(),
        };
        self.in_event(Event::Packet(package.to_string()), |engine| {
            let mut acc = ScriptOutcome::default();
            for id in ids {
                let site = Site::Gmcp {
                    package: package.to_string(),
                    callback_id: id,
                };
                let handled = engine.run_callback(id, &site, |lua| api::json_to_lua(lua, data));
                acc.append(handled.outcome());
            }
            acc
        })
    }

    /// Forget the last packet of every package, as the connection that
    /// sent them ends.
    pub fn forget_gmcp_packets(&mut self) {
        self.packets.clear();
    }

    /// Fire a one-shot timer callback by its callback id, as a round of
    /// its own.
    pub fn fire_timer(&mut self, callback_id: i64) -> ScriptOutcome {
        self.fire_timers(&[callback_id]).outcome
    }

    /// Fire the one-shot timers whose callback ids `callback_ids` holds,
    /// in that order, each as a call of its own. The round is one event,
    /// so a timer whose plugin or loose script used its time for the
    /// round never starts, and comes back in [`FiredTimers::held`] for
    /// the next round.
    pub fn fire_timers(&mut self, callback_ids: &[i64]) -> FiredTimers {
        self.in_event(Event::Timers, |engine| {
            let mut fired = FiredTimers::default();
            for &callback_id in callback_ids {
                let site = Site::Timer { callback_id };
                match engine.run_callback(callback_id, &site, |_| Ok(Value::Nil)) {
                    Handled::Ran(outcome) => {
                        fired.outcome.append(outcome);
                        // Drop the callback after firing, since it was a
                        // one-shot. Forget its timer id too, so a late
                        // `mud.cancel_timer` has nothing to free.
                        if let Ok(mut s) = engine.state.cell.lock() {
                            s.timer_callbacks.retain(|_, id| *id != callback_id);
                        }
                        engine.drop_callback(callback_id);
                    }
                    Handled::Skipped(outcome) => {
                        fired.outcome.append(outcome);
                        fired.held.push(callback_id);
                    }
                }
            }
            fired
        })
    }

    /// Run `handlers` as the handlers of one `event`, with the whole
    /// budget for each owner, and give back the budget of the event they
    /// ran inside, if any, once they end.
    fn in_event<T>(&mut self, event: Event, handlers: impl FnOnce(&mut Self) -> T) -> T {
        let outer = self.budget.replace(Budget::new(event));
        let out = handlers(self);
        self.budget = outer;
        out
    }

    /// Call the function Vosh holds as `callback_id` with the value `arg`
    /// makes, as a call of the function's owner, and charge the time it
    /// ran to the owner in the event running now. Nothing runs when an
    /// earlier stop or a cancel let the function go, or when the owner
    /// used its time for the event.
    fn run_callback(
        &mut self,
        callback_id: i64,
        site: &Site,
        arg: impl FnOnce(&Lua) -> mlua::Result<Value>,
    ) -> Handled {
        let owner = match self.state.cell.lock() {
            Ok(s) => s.callbacks.get(&callback_id).map(|cb| cb.owner.clone()),
            Err(_) => None,
        };
        let Some(owner) = owner else {
            return Handled::Ran(ScriptOutcome::default());
        };
        if let Some(budget) = self.budget.as_mut() {
            if !budget.allows(&owner) {
                let mut outcome = ScriptOutcome::default();
                if budget.skip(&owner) {
                    outcome.actions.push(Action::Error {
                        text: report::budget_line(&owner, &budget.event),
                        owner,
                        at: None,
                    });
                }
                return Handled::Skipped(outcome);
            }
        }
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
        if let Some(budget) = self.budget.as_mut() {
            budget.charge(&owner, called.took);
        }
        Handled::Ran(self.finish(&owner, site, called))
    }

    /// Run `body` as one call of `owner`, under the limits. What the call
    /// queued waits in the pending list for [`Self::finish`]. Every piece
    /// of Lua Vosh runs goes through here, since Lua that runs outside a
    /// call stops at the hook's first look.
    fn call(&self, owner: &Owner, body: impl FnOnce(&Lua) -> mlua::Result<()>) -> Called {
        let start = match self.state.cell.lock() {
            Ok(mut s) => {
                s.call = Some(CallInfo::new(owner.clone()));
                s.pending.len()
            }
            Err(_) => 0,
        };
        self.limits.begin(&self.lua);
        let began = Instant::now();
        let result = body(&self.lua);
        let took = began.elapsed();
        #[cfg(any(test, feature = "testkit"))]
        let took = took + testkit::take_napped(&self.lua);
        let memory_error = matches!(&result, Err(err) if limits::is_memory_error(err));
        let stop = self.limits.end(&self.lua, memory_error);
        let (dropped, text_dropped, blocks_dropped) = match self.state.cell.lock() {
            Ok(mut s) => s.call.take().map_or((false, false, false), |call| {
                (call.dropped, call.text_dropped, call.blocks_dropped)
            }),
            Err(_) => (false, false, false),
        };
        Called {
            start,
            took,
            stop,
            error: result.err(),
            dropped,
            text_dropped,
            blocks_dropped,
        }
    }

    /// Drain what one call queued, with a line for its error or its stop
    /// and one when it queued too much. A stopped call drops everything
    /// it queued and turns its owner off.
    fn finish(&mut self, owner: &Owner, site: &Site, called: Called) -> ScriptOutcome {
        if let Some(stop) = called.stop {
            self.discard_since(called.start);
            let released = self.stop_owner(owner, site, stop.reason);
            // The stop took back what the call registered, so nothing
            // new waits for a packet.
            let (mut outcome, _) = self.drain();
            outcome.actions.extend(released);
            outcome
                .actions
                .extend(report::stop_lines(owner, site, &stop));
            outcome.failed = true;
            outcome.stopped.push(owner.clone());
            return outcome;
        }
        let (mut outcome, fresh) = self.drain();
        if let Some(err) = called.error {
            outcome.actions.push(report::error(owner, &err));
            outcome.failed = true;
        }
        if called.dropped {
            outcome.actions.push(Action::Error {
                owner: owner.clone(),
                text: report::cap_line(owner, site),
                at: None,
            });
        }
        if called.text_dropped {
            outcome.actions.push(Action::Error {
                owner: owner.clone(),
                text: report::text_cap_line(owner, site),
                at: None,
            });
        }
        if called.blocks_dropped {
            outcome.actions.push(Action::Error {
                owner: owner.clone(),
                text: report::pane_cap_line(owner, site),
                at: None,
            });
        }
        outcome.append(self.replay(fresh));
        outcome
    }

    /// Hand each new handler in `fresh`, a package and a callback id, the
    /// last packet of its package, as a call of its own, so a handler
    /// made mid session starts from what the game last sent. A handler
    /// that a replayed call makes waits for the next packet, so a handler
    /// that makes another each time it runs cannot go round for ever. The
    /// replay is an event of its own, so a plugin or loose script that
    /// used its time for it leaves the rest of its new handlers to wait
    /// for the next packet.
    fn replay(&mut self, fresh: Vec<(String, i64)>) -> ScriptOutcome {
        if self.replaying || fresh.is_empty() {
            return ScriptOutcome::default();
        }
        self.replaying = true;
        let acc = self.in_event(Event::Replay, |engine| {
            let mut acc = ScriptOutcome::default();
            for (package, callback_id) in fresh {
                let Some(data) = engine.packets.get(&package).cloned() else {
                    continue;
                };
                let site = Site::Gmcp {
                    package,
                    callback_id,
                };
                let handled =
                    engine.run_callback(callback_id, &site, |lua| api::json_to_lua(lua, &data));
                acc.append(handled.outcome());
            }
            acc
        });
        self.replaying = false;
        acc
    }

    /// Turn off what a stop of `owner` in `site` for `reason` leaves
    /// off, and return the actions that tell the session. A plugin and a
    /// loose script lose every function they handed over and stay
    /// stopped, and a plugin its environment. A trigger or an alias loses
    /// its functions, and the caller turns it off. A function from a
    /// `#lua` line goes alone.
    fn stop_owner(&mut self, owner: &Owner, site: &Site, reason: StopReason) -> Vec<Action> {
        let ids = match owner {
            Owner::Typed => site.callback_id().into_iter().collect(),
            Owner::Plugin(_) | Owner::Script(_) => {
                self.stopped.insert(owner.clone(), reason);
                self.owned_callbacks(owner)
            }
            Owner::Trigger { .. } | Owner::Alias { .. } => self.owned_callbacks(owner),
        };
        let mut actions = self.release(&ids);
        if let Owner::Plugin(name) = owner {
            self.envs.set(name, None);
            actions.push(Action::DropPlugin(name.clone()));
        }
        actions
    }

    /// Let go of the functions `ids` names, and of the Lua triggers, GMCP
    /// subscriptions and timers that would call them. Returns a cancel
    /// for each timer that goes, so the session drops it too.
    fn release(&mut self, ids: &[i64]) -> Vec<Action> {
        if ids.is_empty() {
            return Vec::new();
        }
        let mut cancels = Vec::new();
        if let Ok(mut s) = self.state.cell.lock() {
            for id in ids {
                s.callbacks.remove(id);
            }
            s.timer_callbacks.retain(|timer_id, id| {
                let keep = !ids.contains(id);
                if !keep {
                    cancels.push(Action::CancelTimer(*timer_id));
                }
                keep
            });
        }
        cancels.sort_by_key(|action| match action {
            Action::CancelTimer(timer_id) => *timer_id,
            _ => 0,
        });
        self.triggers.retain(|t| !ids.contains(&t.callback_id));
        for subs in self.gmcp_subs.values_mut() {
            subs.retain(|id| !ids.contains(id));
        }
        self.gmcp_subs.retain(|_, subs| !subs.is_empty());
        cancels
    }

    /// Drop a callback's registry key. Idempotent.
    fn drop_callback(&self, callback_id: i64) {
        if let Ok(mut s) = self.state.cell.lock() {
            s.callbacks.remove(&callback_id);
        }
    }

    /// Take out the Lua trigger `name` whose names `owner` shares, and
    /// let go of its function.
    fn remove_lua_trigger(&mut self, owner: &Owner, name: &str) {
        let mut to_drop: Vec<i64> = Vec::new();
        self.triggers.retain(|t| {
            if t.name == name && t.owner.shares_names_with(owner) {
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

    /// Drain queued actions, also installing any [`Action::SetLuaTrigger`]
    /// or [`Action::SubscribeGmcp`] into the engine's own bookkeeping
    /// before returning the rest to the caller, with each new GMCP
    /// handler as its package and callback id.
    fn drain(&mut self) -> (ScriptOutcome, Vec<(String, i64)>) {
        let mut outcome = ScriptOutcome::default();
        let mut fresh = Vec::new();
        let actions: Vec<Action> = match self.state.cell.lock() {
            Ok(mut s) => std::mem::take(&mut s.pending),
            Err(_) => Vec::new(),
        };
        for action in actions {
            match action {
                Action::SetLuaTrigger {
                    owner,
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
                        self.remove_lua_trigger(&owner, &name);
                        self.triggers.push(LuaTrigger {
                            owner,
                            name,
                            pattern,
                            regex,
                            callback_id,
                            capture_names,
                        });
                    }
                    Err(e) => {
                        outcome.actions.push(Action::Error {
                            owner,
                            text: format!("lua trigger `{name}` rejected: invalid regex {e}"),
                            at: None,
                        });
                        self.drop_callback(callback_id);
                    }
                },
                Action::RemoveLuaTrigger { owner, name } => {
                    self.remove_lua_trigger(&owner, &name);
                }
                Action::SubscribeGmcp {
                    package,
                    callback_id,
                } => {
                    fresh.push((package.clone(), callback_id));
                    self.gmcp_subs.entry(package).or_default().push(callback_id);
                }
                // The function a cancelled timer would have called goes
                // now, and the session drops the schedule when it applies
                // the cancel.
                Action::CancelTimer(timer_id) => {
                    if let Ok(mut s) = self.state.cell.lock() {
                        if let Some(callback_id) = s.timer_callbacks.remove(&timer_id) {
                            s.callbacks.remove(&callback_id);
                        }
                    }
                    outcome.actions.push(Action::CancelTimer(timer_id));
                }
                other => outcome.actions.push(other),
            }
        }
        (outcome, fresh)
    }
}

#[cfg(test)]
mod tests {
    use vosh_automation::vars::Scope;

    use super::*;
    use crate::test_support::{error_lines, returns_in_time, said};

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
                Action::Error { text, .. } => Some(text.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no error line in {:?}", outcome.actions))
    }

    /// Load `code` as the loose script `name`.
    fn load(e: &mut ScriptEngine, name: &str, code: &str) -> ScriptOutcome {
        e.load_script(Owner::Script(name.into()), &format!("@{name}"), code)
    }

    /// Load each script `#script reload` runs again, in its order, with
    /// the code `files` holds for its name, the way the app reads each
    /// file again.
    fn reload(e: &mut ScriptEngine, files: &[(&str, &str)]) -> ScriptOutcome {
        let mut acc = ScriptOutcome::default();
        for owner in e.reload_order() {
            let (name, chunk) = match &owner {
                Owner::Script(name) => (name.clone(), format!("@{name}")),
                Owner::Plugin(name) => (name.clone(), format!("@{name}/main.lua")),
                other => panic!("{other:?} never reloads"),
            };
            let (_, code) = files
                .iter()
                .find(|(file, _)| *file == name)
                .unwrap_or_else(|| panic!("no file for {name}"));
            acc.append(e.load_script(owner, &chunk, code));
        }
        acc
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
    fn two_plugins_can_share_a_trigger_name() {
        let mut e = ScriptEngine::new().unwrap();
        // Each plugin drops its trigger on a packet of its own.
        for (name, package) in [("hunger", "Room.Info"), ("meals", "Char.Vitals")] {
            e.load_script(
                Owner::Plugin(name.into()),
                &format!("@{name}/main.lua"),
                &format!(
                    "mud.trigger('eat', 'You are hungry', function() mud.echo('{name}') end)\n\
                     mud.on_gmcp('{package}', function() mud.untrigger('eat') end)"
                ),
            )
            .unwrap();
        }
        let line = "You are hungry.";
        assert_eq!(
            e.match_line(line).unwrap().actions,
            vec![Action::Echo("hunger".into()), Action::Echo("meals".into())]
        );
        let listed: Vec<(String, String)> = e
            .lua_triggers()
            .into_iter()
            .map(|t| (t.name, t.owner))
            .collect();
        assert_eq!(
            listed,
            [
                ("eat".to_string(), "plugin:hunger".to_string()),
                ("eat".to_string(), "plugin:meals".to_string()),
            ]
        );
        // Your own Lua has names of its own, so its untrigger takes
        // neither, and its trigger of the same name joins them.
        e.eval(
            "mud.untrigger('eat') \
             mud.trigger('eat', 'You are hungry', function() mud.echo('typed') end)",
            "=#lua",
        )
        .unwrap();
        assert_eq!(e.match_line(line).unwrap().actions.len(), 3);
        // A plugin's untrigger takes its own alone.
        e.dispatch_gmcp("Char.Vitals", &serde_json::json!({}))
            .unwrap();
        assert_eq!(
            e.match_line(line).unwrap().actions,
            vec![Action::Echo("hunger".into()), Action::Echo("typed".into())]
        );
    }

    #[test]
    fn your_lua_lines_and_bodies_share_trigger_names() {
        let mut e = ScriptEngine::new().unwrap();
        e.eval(
            "mud.trigger('day', 'The day has begun', function() mud.echo('typed') end)",
            "=#lua",
        )
        .unwrap();
        e.run_body(
            &Owner::trigger("dawn"),
            "mud.trigger('day', 'The day has begun', function() mud.echo('body') end)",
            &[],
        )
        .unwrap();
        let line = "The day has begun.";
        assert_eq!(
            e.match_line(line).unwrap().actions,
            vec![Action::Echo("body".into())]
        );
        e.eval("mud.untrigger('day')", "=#lua").unwrap();
        let leftover = &e.lua_triggers();
        assert!(leftover.is_empty(), "{leftover:?}");
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
    fn a_new_handler_gets_the_last_packet_of_its_package_at_once() {
        let mut e = ScriptEngine::new().unwrap();
        // No packet yet, so a handler waits for the first.
        let waiting = e
            .eval(
                "mud.on_gmcp('Char.Status', function(d) mud.echo('status ' .. d.level) end)",
                "=#lua",
            )
            .unwrap();
        let leftover = &waiting.actions;
        assert!(leftover.is_empty(), "{leftover:?}");
        let status = serde_json::json!({"name": "Orla", "level": 50});
        let fired = e.dispatch_gmcp("Char.Status", &status).unwrap();
        assert_eq!(echoes(&fired), ["status 50"]);
        // A plugin that turns on later hears the same packet as it loads,
        // after what its load asked for.
        let loaded = plugin(
            &mut e,
            "levels",
            "mud.echo('loaded') \
             mud.on_gmcp('Char.Status', function(d) mud.echo('level ' .. d.level) end) \
             mud.on_gmcp('Room.Info', function() mud.echo('room') end)",
        )
        .unwrap();
        assert_eq!(echoes(&loaded), ["loaded", "level 50"]);
        // The packet stays the last until the next of its package.
        let later = e
            .eval(
                "mud.on_gmcp('Char.Status', function(d) mud.echo('again ' .. d.level) end)",
                "=#lua",
            )
            .unwrap();
        assert_eq!(echoes(&later), ["again 50"]);
        // A disconnect forgets the packets.
        e.forget_gmcp_packets();
        let leftover = &e
            .eval(
                "mud.on_gmcp('Char.Status', function() mud.echo('stale') end)",
                "=#lua",
            )
            .unwrap()
            .actions;
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn a_new_handler_never_gets_the_last_chat_packet() {
        // The tell fixture as the game sent it. Each chat packet is one
        // message and not a state, so Vosh keeps none for later.
        let fixture = include_str!("../../../fixtures/gmcp/aabahran/chat/tell.gmcp");
        let (package, body) = fixture.trim_end().split_once(' ').unwrap();
        assert_eq!(package, "Comm.Channel");
        let tell: serde_json::Value = serde_json::from_str(body).unwrap();
        let mut e = ScriptEngine::new().unwrap();
        e.dispatch_gmcp(package, &tell).unwrap();
        let handler = "mud.on_gmcp('Comm.Channel', function(d) \
                         mud.echo(d.channel .. ' from ' .. d.speaker) \
                       end)";
        let loaded = plugin(&mut e, "chat", handler).unwrap();
        let leftover = &echoes(&loaded);
        assert!(leftover.is_empty(), "{leftover:?}");
        let typed = e.eval(handler, "=#lua").unwrap();
        let leftover = &echoes(&typed);
        assert!(leftover.is_empty(), "{leftover:?}");
        // The next message reaches both.
        let fired = e.dispatch_gmcp(package, &tell).unwrap();
        assert_eq!(echoes(&fired), ["tell from Tolliver", "tell from Tolliver"]);
    }

    #[test]
    fn a_handler_that_makes_another_each_time_cannot_go_round() {
        let mut e = ScriptEngine::new().unwrap();
        e.dispatch_gmcp("Char.Vitals", &serde_json::json!({"hp": 80}))
            .unwrap();
        let outcome = e
            .eval(
                "local function again(d) \
                   mud.echo('hp ' .. d.hp) \
                   mud.on_gmcp('Char.Vitals', again) \
                 end \
                 mud.on_gmcp('Char.Vitals', again)",
                "=#lua",
            )
            .unwrap();
        // The new handler runs once, and the one it makes waits.
        assert_eq!(echoes(&outcome), ["hp 80"]);
        assert_eq!(held_callbacks(&e), 2);
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
    fn a_timer_waits_at_most_a_day_and_never_forever() {
        let mut e = ScriptEngine::new().unwrap();
        for secs in ["math.huge", "-math.huge", "0/0"] {
            let outcome = e.eval(&format!("mud.timer({secs}, function() end)"), "=#lua");
            assert_eq!(
                error_lines(&outcome),
                ["#lua:1: mud.timer needs a finite number of seconds"],
                "{secs}"
            );
        }
        assert_eq!(held_callbacks(&e), 0);
        let delays: Vec<std::time::Duration> = e
            .eval(
                "mud.timer(1e300, function() end) mud.timer(-5, function() end)",
                "=#lua",
            )
            .unwrap()
            .actions
            .into_iter()
            .filter_map(|action| match action {
                Action::Timer { delay, .. } => Some(delay),
                _ => None,
            })
            .collect();
        assert_eq!(delays, [api::TIMER_MAX, std::time::Duration::ZERO]);
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
    fn only_the_owner_of_a_timer_cancels_it() {
        let mut e = ScriptEngine::new().unwrap();
        plugin(
            &mut e,
            "keeper",
            "kept = mud.timer(60, function() mud.echo('kept') end) \
             function stop() mud.cancel_timer(kept) end",
        )
        .unwrap();
        let kept: u32 = e
            .eval("mud.echo(tostring(plugins.keeper.kept))", "=#lua")
            .unwrap()
            .actions
            .iter()
            .find_map(|a| match a {
                Action::Echo(id) => id.parse().ok(),
                _ => None,
            })
            .unwrap();
        // Neither another plugin nor your own Lua cancels it.
        let other = plugin(&mut e, "rival", &format!("mud.cancel_timer({kept})")).unwrap();
        assert!(!other.actions.contains(&Action::CancelTimer(kept)));
        let typed = e
            .eval(&format!("mud.cancel_timer({kept})"), "=#lua")
            .unwrap();
        assert!(typed.actions.is_empty(), "{:?}", typed.actions);
        assert_eq!(held_callbacks(&e), 1);
        // Your own Lua cancels the timers of a trigger body, since they
        // share names, and a call Vosh stops cancels nothing.
        let made = e
            .run_body(
                &Owner::trigger("tick"),
                "later = mud.timer(60, function() end)",
                &[],
            )
            .unwrap();
        let Action::Timer { timer_id, .. } = made.actions[0] else {
            panic!("expected a timer, got {:?}", made.actions);
        };
        let stopped = e.eval("mud.cancel_timer(later) while true do end", "=#lua");
        assert!(stopped.failed);
        assert!(!stopped.actions.contains(&Action::CancelTimer(timer_id)));
        assert_eq!(held_callbacks(&e), 2);
        let cancelled = e.eval("mud.cancel_timer(later)", "=#lua").unwrap();
        assert_eq!(cancelled.actions, [Action::CancelTimer(timer_id)]);
        assert_eq!(held_callbacks(&e), 1);
        // The plugin cancels its own, whoever calls its function.
        let own = e.eval("plugins.keeper.stop()", "=#lua").unwrap();
        assert_eq!(own.actions, [Action::CancelTimer(kept)]);
        assert_eq!(held_callbacks(&e), 0);
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
        let code = r#"mud.on_gmcp("Char.Vitals", function(d) mud.echo("hp " .. d.hp) end)"#;
        load(&mut e, "vitals.lua", code).unwrap();
        reload(&mut e, &[("vitals.lua", code)]).unwrap();
        reload(&mut e, &[("vitals.lua", code)]).unwrap();
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
        e.load_script(mapper.clone(), "@mapper/main.lua", code)
            .unwrap();
        e.load_script(mapper, "@mapper/main.lua", code).unwrap();
        let outcome = e
            .dispatch_gmcp("Room.Info", &serde_json::json!({}))
            .unwrap();
        assert_eq!(outcome.actions, vec![Action::Echo("room".into())]);
        assert_eq!(held_callbacks(&e), 1);
    }

    #[test]
    fn reload_keeps_gmcp_subscriptions_the_script_did_not_make() {
        let mut e = ScriptEngine::new().unwrap();
        let files = [
            (
                "vitals.lua",
                r#"mud.on_gmcp("Char.Vitals", function() mud.echo("script") end)"#,
            ),
            (
                "other.lua",
                r#"mud.on_gmcp("Char.Vitals", function() mud.echo("other") end)"#,
            ),
        ];
        for (name, code) in files {
            load(&mut e, name, code).unwrap();
        }
        e.eval(
            r#"mud.on_gmcp("Char.Vitals", function() mud.echo("typed") end)"#,
            "t",
        )
        .unwrap();
        // A run that claimed handlers it did not make would drop them on
        // the next reload, so reload twice.
        reload(&mut e, &files).unwrap();
        reload(&mut e, &files).unwrap();
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
    fn a_reload_takes_back_each_handler_the_script_made_before() {
        // Lua globals survive a reload, but the handlers the last run
        // made do not, so a handler a global guards goes and stays gone.
        let mut e = ScriptEngine::new().unwrap();
        let code = r#"
            if not hooked then
                mud.on_gmcp("Char.Vitals", function() mud.echo("guarded") end)
                hooked = true
            end
            mud.on_gmcp("Room.Info", function() mud.echo("room") end)
            "#;
        load(&mut e, "vitals.lua", code).unwrap();
        reload(&mut e, &[("vitals.lua", code)]).unwrap();
        reload(&mut e, &[("vitals.lua", code)]).unwrap();
        let data = serde_json::json!({});
        let vitals = &e.dispatch_gmcp("Char.Vitals", &data).unwrap().actions;
        assert!(vitals.is_empty(), "{vitals:?}");
        let room = e.dispatch_gmcp("Room.Info", &data).unwrap().actions;
        assert_eq!(room, vec![Action::Echo("room".into())]);
        assert_eq!(held_callbacks(&e), 1);
    }

    /// The timer ids an outcome cancels.
    fn cancels(outcome: &ScriptOutcome) -> Vec<u32> {
        outcome
            .actions
            .iter()
            .filter_map(|action| match action {
                Action::CancelTimer(id) => Some(*id),
                _ => None,
            })
            .collect()
    }

    /// The timer id each timer in an outcome starts with.
    fn timers(outcome: &ScriptOutcome) -> Vec<u32> {
        outcome
            .actions
            .iter()
            .filter_map(|action| match action {
                Action::Timer { timer_id, .. } => Some(*timer_id),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_reload_never_doubles_what_a_plugin_registers() {
        let code = "mud.trigger('hunger', 'You are hungry', function() mud.echo('eat') end)\n\
                    mud.on_gmcp('Char.Vitals', function() mud.echo('vitals') end)\n\
                    mud.timer(60, function() mud.echo('later') end)";
        let mut e = ScriptEngine::new().unwrap();
        let meals = Owner::Plugin("meals".into());
        let first = e
            .load_script(meals.clone(), "@meals/main.lua", code)
            .unwrap();
        let mut started = timers(&first);
        for _ in 0..3 {
            let again = e
                .load_script(meals.clone(), "@meals/main.lua", code)
                .unwrap();
            // Each load cancels the timer the last one started.
            assert_eq!(cancels(&again), started);
            started = timers(&again);
        }
        assert_eq!(
            e.match_line("You are hungry.").unwrap().actions,
            vec![Action::Echo("eat".into())]
        );
        assert_eq!(
            e.dispatch_gmcp("Char.Vitals", &serde_json::json!({}))
                .unwrap()
                .actions,
            vec![Action::Echo("vitals".into())]
        );
        assert_eq!(held_callbacks(&e), 3);
        assert_eq!(e.state.cell.lock().unwrap().timer_callbacks.len(), 1);
    }

    #[test]
    fn a_reload_drops_what_the_new_code_no_longer_registers() {
        let mut e = ScriptEngine::new().unwrap();
        let both = "mud.trigger('hunger', 'You are hungry', function() mud.echo('eat') end)\n\
                    mud.trigger('thirst', 'You are thirsty', function() mud.echo('drink') end)";
        load(&mut e, "meals.lua", both).unwrap();
        assert_eq!(e.lua_triggers().len(), 2);
        load(
            &mut e,
            "meals.lua",
            "mud.trigger('hunger', 'You are hungry', function() mud.echo('eat') end)",
        )
        .unwrap();
        let names: Vec<String> = e.lua_triggers().into_iter().map(|t| t.name).collect();
        assert_eq!(names, ["hunger"]);
        let leftover = &e.match_line("You are thirsty.").unwrap().actions;
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn a_reload_follows_the_order_scripts_first_loaded_in() {
        let mut e = ScriptEngine::new().unwrap();
        load(&mut e, "b.lua", "").unwrap();
        plugin(&mut e, "a", "").unwrap();
        load(&mut e, "a.lua", "").unwrap();
        // A load again keeps its place, and a first load that fails
        // takes none.
        load(&mut e, "b.lua", "").unwrap();
        assert!(load(&mut e, "typo.lua", "mud.ech('x')").failed);
        // A plugin Vosh stopped stays off, and a loose script comes back.
        assert!(plugin(&mut e, "spin", "while true do end").failed);
        assert!(load(&mut e, "spin.lua", "while true do end").failed);
        assert_eq!(
            e.reload_order(),
            [
                Owner::Script("b.lua".into()),
                Owner::Plugin("a".into()),
                Owner::Script("a.lua".into()),
                Owner::Script("spin.lua".into()),
            ]
        );
    }

    #[test]
    fn loaded_plugins_names_each_plugin_once_with_a_stopped_one() {
        let mut e = ScriptEngine::new().unwrap();
        plugin(&mut e, "meals", "").unwrap();
        load(&mut e, "combat.lua", "").unwrap();
        assert!(plugin(&mut e, "spin", "while true do end").failed);
        plugin(&mut e, "meals", "").unwrap();
        assert_eq!(e.loaded_plugins(), ["meals", "spin"]);
        e.unload(&Owner::Plugin("spin".into())).unwrap();
        assert_eq!(e.loaded_plugins(), ["meals"]);
    }

    #[test]
    fn a_failed_reload_keeps_the_triggers_and_timers_it_had() {
        let mut e = ScriptEngine::new().unwrap();
        let good = "mud.trigger('hunger', 'You are hungry', function() mud.echo('eat') end)\n\
                    mud.timer(60, function() end)";
        load(&mut e, "meals.lua", good).unwrap();
        let bad = "mud.trigger('hunger', 'You are hungry', function() mud.echo('new') end)\n\
                   mud.timer(60, function() end)\n\
                   error('typo')";
        let outcome = load(&mut e, "meals.lua", bad);
        assert!(outcome.failed);
        let leftover = &cancels(&outcome);
        assert!(leftover.is_empty(), "{leftover:?}");
        let leftover = &timers(&outcome);
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(
            e.match_line("You are hungry.").unwrap().actions,
            vec![Action::Echo("eat".into())]
        );
        assert_eq!(held_callbacks(&e), 2);
    }

    #[test]
    fn an_unload_takes_exactly_what_its_owner_registered() {
        let mut e = ScriptEngine::new().unwrap();
        let meals = Owner::Plugin("meals".into());
        let code = "mud.trigger('hunger', 'You are hungry', function() mud.echo('meals') end)\n\
                    mud.on_gmcp('Char.Vitals', function() mud.echo('meals') end)\n\
                    mud.on_gmcp('Room.Info', function() \
                      mud.timer(60, function() end) \
                      mud.set_var('fed', 'yes') \
                    end)";
        e.load_script(meals.clone(), "@meals/main.lua", code)
            .unwrap();
        // What its handler registers later is its own too.
        let later = e
            .dispatch_gmcp("Room.Info", &serde_json::json!({}))
            .unwrap();
        let started = timers(&later);
        load(
            &mut e,
            "hunger.lua",
            "mud.trigger('hunger', 'You are hungry', function() mud.echo('hunger') end)",
        )
        .unwrap();
        e.eval(
            "mud.on_gmcp('Char.Vitals', function() mud.echo('typed') end)",
            "=#lua",
        )
        .unwrap();
        let outcome = e.unload(&meals).unwrap();
        assert_eq!(cancels(&outcome), started);
        assert_eq!(e.loaded_script_names(), ["hunger.lua"]);
        assert_eq!(
            e.match_line("You are hungry.").unwrap().actions,
            vec![Action::Echo("hunger".into())]
        );
        assert_eq!(
            e.dispatch_gmcp("Char.Vitals", &serde_json::json!({}))
                .unwrap()
                .actions,
            vec![Action::Echo("typed".into())]
        );
        let leftover = &e
            .dispatch_gmcp("Room.Info", &serde_json::json!({}))
            .unwrap()
            .actions;
        assert!(leftover.is_empty(), "{leftover:?}");
        // The variable it set stays, since the session holds it.
        assert_eq!(
            e.eval("mud.echo(mud.var('fed'))", "=#lua").unwrap().actions,
            vec![Action::Echo("yes".into())]
        );
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
        reload(&mut e, &[("vitals.lua", good)]).unwrap();
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
                Action::Error {
                    owner: Owner::Script("vitals.lua".into()),
                    text: "vitals.lua:4: typo".into(),
                    at: Some(Place {
                        source: "vitals.lua".into(),
                        line: 4,
                    }),
                },
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

    /// Load `code` as the plugin `name`.
    fn plugin(e: &mut ScriptEngine, name: &str, code: &str) -> ScriptOutcome {
        e.load_script(
            Owner::Plugin(name.into()),
            &format!("@{name}/main.lua"),
            code,
        )
    }

    /// The echoes of an outcome.
    fn echoes(outcome: &ScriptOutcome) -> Vec<String> {
        outcome
            .actions
            .iter()
            .filter_map(|action| match action {
                Action::Echo(text) => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn each_plugin_keeps_its_own_globals() {
        let mut e = ScriptEngine::new().unwrap();
        for name in ["first", "second"] {
            plugin(
                &mut e,
                name,
                &format!(
                    "function draw() return '{name}' end\n\
                     mud.on_gmcp('Char.Vitals', function() mud.echo(draw()) end)"
                ),
            )
            .unwrap();
        }
        let drawn = e
            .dispatch_gmcp("Char.Vitals", &serde_json::json!({}))
            .unwrap();
        assert_eq!(echoes(&drawn), ["first", "second"]);
        // Neither reaches the globals your own Lua shares, nor they its.
        e.eval("secret = 'typed'", "=#lua").unwrap();
        let shared = e.eval("mud.echo(tostring(draw))", "=#lua").unwrap();
        assert_eq!(echoes(&shared), ["nil"]);
        let inside = plugin(&mut e, "third", "mud.echo(tostring(secret))").unwrap();
        assert_eq!(echoes(&inside), ["nil"]);
        // _G names the plugin's own globals.
        let own = plugin(&mut e, "fourth", "hp = 80 mud.echo(tostring(_G.hp))").unwrap();
        assert_eq!(echoes(&own), ["80"]);
    }

    #[test]
    fn a_plugin_cannot_change_mud_or_the_libraries_for_the_rest() {
        let mut e = ScriptEngine::new().unwrap();
        plugin(
            &mut e,
            "rude",
            "mud.send = function() end\n\
             mud.send('quiet')",
        )
        .unwrap();
        // Its own mud table changed, and nothing else did.
        let typed = e.eval("mud.send('look')", "=#lua").unwrap();
        assert_eq!(typed.actions, vec![Action::Send("look".into())]);
        let other = plugin(&mut e, "polite", "mud.send('look')").unwrap();
        assert!(other.actions.contains(&Action::Send("look".into())));
        // The libraries read as they are, and refuse a change.
        let read = plugin(
            &mut e,
            "reader",
            "local n = 0 for _ in pairs(string) do n = n + 1 end\n\
             mud.echo(string.upper('hp') .. ('mv'):upper() .. tostring(n > 5))\n\
             mud.echo(tostring(getmetatable('')) .. tostring(getmetatable(string)))",
        )
        .unwrap();
        assert_eq!(echoes(&read), ["HPMVtrue", "falsefalse"]);
        // The sandbox holds inside a plugin too.
        let sandboxed = plugin(
            &mut e,
            "sandboxed",
            "mud.echo(tostring(io) .. tostring(os.getenv) .. tostring(load))",
        )
        .unwrap();
        assert_eq!(echoes(&sandboxed), ["nilnilnil"]);
        let refused = plugin(&mut e, "writer", "string.upper = nil");
        assert_eq!(
            error_lines(&refused),
            ["writer/main.lua:1: string is read only in a plugin"]
        );
        // A plugin may still put its own table in place of one.
        let shadowed = plugin(
            &mut e,
            "shadow",
            "string = { upper = function() return 'mine' end } mud.echo(string.upper('x'))",
        )
        .unwrap();
        assert_eq!(echoes(&shadowed), ["mine"]);
        let typed = e.eval("mud.echo(string.upper('hp'))", "=#lua").unwrap();
        assert_eq!(echoes(&typed), ["HP"]);
        // Your own Lua reads the string metatable as stock Lua does.
        let typed = e
            .eval(
                "mud.echo(type(getmetatable('')) .. tostring(getmetatable('').__index == string))",
                "=#lua",
            )
            .unwrap();
        assert_eq!(echoes(&typed), ["tabletrue"]);
    }

    #[test]
    fn a_plugin_starts_from_fresh_globals_on_each_load() {
        let mut e = ScriptEngine::new().unwrap();
        let code = "loads = (loads or 0) + 1 mud.echo(tostring(loads))";
        for _ in 0..3 {
            assert_eq!(echoes(&plugin(&mut e, "counter", code).unwrap()), ["1"]);
        }
        // A loose script shares the globals, which a reload keeps.
        load(&mut e, "counter.lua", code).unwrap();
        assert_eq!(echoes(&load(&mut e, "counter.lua", code).unwrap()), ["2"]);
    }

    #[test]
    fn a_plugin_loads_wherever_the_hook_count_stands() {
        // The count of instructions to the hook's next look carries over
        // from the call before, so a loop of each length leaves it at
        // another place when the load begins.
        for spin in (0..limits::HOOK_EVERY).step_by(7) {
            let mut e = ScriptEngine::new().unwrap();
            e.eval(&format!("for i = 1, {spin} do end"), "=#lua")
                .unwrap();
            let loaded = plugin(&mut e, "helpers", "mud.echo('on')");
            assert_eq!(echoes(&loaded.unwrap()), ["on"], "{spin}");
        }
    }

    #[test]
    fn your_lua_reaches_a_plugin_through_plugins() {
        let mut e = ScriptEngine::new().unwrap();
        plugin(
            &mut e,
            "helpers",
            "function rescue(name) return 'rescue ' .. name end",
        )
        .unwrap();
        let typed = e
            .eval("mud.send(plugins.helpers.rescue('Orla'))", "=#lua")
            .unwrap();
        assert_eq!(typed.actions, vec![Action::Send("rescue Orla".into())]);
        let body = e
            .run_body(
                &Owner::trigger("guard"),
                "mud.send(plugins.helpers.rescue(captures[1]))",
                &["Maren".into()],
            )
            .unwrap();
        assert_eq!(body.actions, vec![Action::Send("rescue Maren".into())]);
        // It lists the plugin's own globals.
        let listed = e
            .eval(
                "local names = {} \
                 for name in pairs(plugins.helpers) do names[#names + 1] = name end \
                 table.sort(names) mud.echo(table.concat(names, ' '))",
                "=#lua",
            )
            .unwrap();
        assert_eq!(echoes(&listed), ["rescue"]);
        // A plugin sees neither the others nor this view.
        let inside = plugin(&mut e, "nosy", "mud.echo(tostring(plugins))").unwrap();
        assert_eq!(echoes(&inside), ["nil"]);
    }

    #[test]
    fn plugins_reads_only_and_follows_a_reload() {
        let mut e = ScriptEngine::new().unwrap();
        plugin(&mut e, "helpers", "level = 1").unwrap();
        e.eval("held = plugins.helpers", "=#lua").unwrap();
        for (code, line) in [
            (
                "plugins.helpers.level = 2",
                "#lua:1: plugins.helpers is read only",
            ),
            ("plugins.helpers = {}", "#lua:1: plugins is read only"),
        ] {
            assert_eq!(error_lines(&e.eval(code, "=#lua")), [line], "{code}");
        }
        plugin(&mut e, "helpers", "level = 3").unwrap();
        let read = e
            .eval(
                "mud.echo(tostring(held.level) .. tostring(plugins.missing))",
                "=#lua",
            )
            .unwrap();
        assert_eq!(echoes(&read), ["3nil"]);
        // Neither its environment nor its mud table comes out of the
        // view, so nothing reached through it writes into the plugin.
        let reached = e.eval(
            "mud.echo(tostring(plugins.helpers._G) .. tostring(plugins.helpers.mud)) \
             plugins.helpers._G.level = 99",
            "=#lua",
        );
        assert_eq!(echoes(&reached), ["nilnil"]);
        assert_eq!(
            error_lines(&reached),
            ["#lua:1: attempt to index a nil value (field '_G')"]
        );
        let read = e.eval("mud.echo(tostring(held.level))", "=#lua").unwrap();
        assert_eq!(echoes(&read), ["3"]);
        e.unload(&Owner::Plugin("helpers".into())).unwrap();
        let read = e
            .eval(
                "mud.echo(tostring(held.level) .. tostring(plugins.helpers))",
                "=#lua",
            )
            .unwrap();
        assert_eq!(echoes(&read), ["nilnil"]);
    }

    #[test]
    fn a_mud_input_line_goes_with_whose_lua_asked_for_it() {
        let mut e = ScriptEngine::new().unwrap();
        let inputs = |outcome: ScriptOutcome| -> Vec<Action> {
            let actions = outcome.unwrap().actions;
            actions
                .into_iter()
                .filter(|action| matches!(action, Action::Input { .. }))
                .collect()
        };
        assert_eq!(
            inputs(plugin(
                &mut e,
                "helpers",
                "mud.input('#lua x = 1') function go() mud.input('look') end"
            )),
            [Action::Input {
                owner: Owner::Plugin("helpers".into()),
                line: "#lua x = 1".into(),
            }]
        );
        // A plugin function your Lua calls asks for the plugin.
        assert_eq!(
            inputs(e.eval("plugins.helpers.go() mud.input('look')", "=#lua")),
            [
                Action::Input {
                    owner: Owner::Plugin("helpers".into()),
                    line: "look".into(),
                },
                Action::Input {
                    owner: Owner::Typed,
                    line: "look".into(),
                },
            ]
        );
    }

    #[test]
    fn what_a_plugin_function_registers_is_the_plugin_s_whoever_calls_it() {
        let mut e = ScriptEngine::new().unwrap();
        let watch = Owner::Plugin("watch".into());
        e.load_script(
            watch.clone(),
            "@watch/main.lua",
            "function watch(pattern) \
               mud.trigger('seen', pattern, function() mud.echo('seen') end) \
             end",
        )
        .unwrap();
        e.eval("plugins.watch.watch('You are hungry')", "=#lua")
            .unwrap();
        let listed: Vec<String> = e.lua_triggers().into_iter().map(|t| t.owner).collect();
        assert_eq!(listed, ["plugin:watch"]);
        // So turning the plugin off takes it.
        e.unload(&watch).unwrap();
        let leftover = &e.lua_triggers();
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn mud_alert_rings_for_whose_lua_raised_it_and_posts_a_banner_at_first() {
        use vosh_automation::alert::{AlertParts, Attention};
        assert_eq!(
            run("mud.alert('Health low')"),
            vec![Action::Alert {
                owner: Owner::Typed,
                title: "Health low".into(),
                text: None,
                parts: AlertParts {
                    banner: true,
                    ..AlertParts::default()
                },
            }]
        );
        let mut e = ScriptEngine::new().unwrap();
        let watch = Owner::Plugin("watch".into());
        let code = "mud.alert('Health low', {text = 'Tolliver bleeds', words = true, \
                    sound = 'low', attention = 'until', background = false, banner = false})";
        let loaded = e
            .load_script(watch.clone(), "@watch/main.lua", code)
            .unwrap();
        assert_eq!(
            loaded.actions,
            vec![
                Action::DropPlugin("watch".into()),
                Action::Alert {
                    owner: watch.clone(),
                    title: "Health low".into(),
                    text: Some("Tolliver bleeds".into()),
                    parts: AlertParts {
                        banner: false,
                        sound: Some("low".into()),
                        attention: Some(Attention::Until),
                        background: false,
                        words: true,
                    },
                },
            ]
        );
        assert_eq!(watch.tag(), "plugin:watch");
        // An attention it does not know asks for none.
        let actions = run("mud.alert('Health low', {attention = 'forever'})");
        assert!(
            matches!(&actions[..], [Action::Alert { parts, .. }] if parts.attention.is_none()),
            "{actions:?}"
        );
    }

    #[test]
    fn a_plugin_alias_is_the_plugin_s_and_goes_with_it() {
        let mut e = ScriptEngine::new().unwrap();
        let healer = Owner::Plugin("healer".into());
        let code = "mud.alias('hl', 'cast heal') mud.unalias('kk')";
        let loaded = e
            .load_script(healer.clone(), "@healer/main.lua", code)
            .unwrap();
        // A load drops the aliases the plugin made before its own run.
        assert_eq!(
            loaded.actions,
            vec![
                Action::DropPlugin("healer".into()),
                Action::SetPluginAlias {
                    plugin: "healer".into(),
                    name: "hl".into(),
                    expansion: "cast heal".into(),
                },
                Action::RemovePluginAlias {
                    plugin: "healer".into(),
                    name: "kk".into(),
                },
            ]
        );
        // A failed load keeps the aliases it had and makes none.
        let failed = e.load_script(
            healer.clone(),
            "@healer/main.lua",
            "mud.alias('hl', 'cast cure') error('typo')",
        );
        assert_eq!(error_lines(&failed), ["healer/main.lua:1: typo"]);
        assert_eq!(failed.actions.len(), 1);
        assert_eq!(
            e.unload(&healer).unwrap().actions,
            vec![Action::DropPlugin("healer".into())]
        );
        // Your own Lua and a loose script make aliases you keep.
        let typed = e.eval("mud.alias('hl', 'cast heal')", "=#lua").unwrap();
        let script = load(&mut e, "heal.lua", "mud.unalias('hl')").unwrap();
        assert_eq!(
            [typed.actions, script.actions].concat(),
            vec![
                Action::SetAlias {
                    name: "hl".into(),
                    expansion: "cast heal".into(),
                },
                Action::RemoveAlias("hl".into()),
            ]
        );
    }

    #[test]
    fn sandbox_blocks_dangerous_globals() {
        let mut e = ScriptEngine::new().unwrap();
        // Each is removed, so calling it calls nil.
        for code in [
            "os.execute('ls')",
            "dofile('foo')",
            "os.getenv('HOME')",
            "require('io')",
            "load('return 1')",
            "os.setlocale('fr_FR.UTF-8')",
        ] {
            assert!(e.eval(code, "=#lua").failed, "{code}");
        }
        let gone = e
            .eval(
                "mud.echo(tostring(io) .. tostring(package) .. tostring(os.getenv) \
                 .. tostring(os.setlocale) .. tostring(1.5))",
                "=#lua",
            )
            .unwrap();
        assert_eq!(gone.actions, vec![Action::Echo("nilnilnilnil1.5".into())]);
    }

    /// Run `code` as the body of the trigger `body`, in a fresh engine.
    fn run_body(code: &str, captures: &[&str]) -> ScriptOutcome {
        let captures: Vec<String> = captures.iter().map(|c| (*c).to_string()).collect();
        let mut e = ScriptEngine::new().unwrap();
        e.run_body(&Owner::trigger("body"), code, &captures)
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
                &Owner::alias("later"),
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
    fn invalid_trigger_regex_shows_as_an_error() {
        let mut e = ScriptEngine::new().unwrap();
        let outcome = e
            .eval(r#"mud.trigger("bad", "[unclosed", function() end)"#, "t")
            .unwrap();
        assert!(
            matches!(
                &outcome.actions[0],
                Action::Error { owner: Owner::Typed, text, at: None }
                    if text.starts_with("lua trigger `bad` rejected")
            ),
            "{:?}",
            outcome.actions
        );
        assert_eq!(held_callbacks(&e), 0);
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
        // The thread that resumed the coroutine keeps its own hook.
        let mut e = ScriptEngine::new().unwrap();
        stops_in_time(
            &mut e,
            "local step = coroutine.wrap(function() coroutine.yield() end) \
             step() while true do end",
        );
        // A coroutine left suspended by one call leaves the next call
        // under the hook.
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
        // Large steps reach 32 MB long before 100 ms, even in a debug
        // build with every test running at once.
        let outcome = e.eval(
            "mud.send('look')\n\
             local ok = pcall(function()\n\
               local t = {}\n\
               for i = 1, 1e9 do t[i] = string.rep('x', 65536) .. i end\n\
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
    fn garbage_near_the_cap_leaves_each_call_its_own_limit() {
        let mut e = ScriptEngine::new().unwrap();
        // 70 MB held, then 40 MB of garbage the collector, stopped,
        // leaves in place.
        e.eval("held = {} collectgarbage('stop')", "=#lua").unwrap();
        for _ in 0..7 {
            e.eval(
                "held[#held + 1] = string.rep('x', 10 * 1024 * 1024)",
                "=#lua",
            )
            .unwrap();
        }
        for _ in 0..4 {
            e.eval("local junk = string.rep('y', 10 * 1024 * 1024)", "=#lua")
                .unwrap();
        }
        // A call that holds 40 MB more is past its own 32 MB, whatever
        // the garbage made the state look like when it began.
        let outcome = e.eval(
            "for i = 1, 4 do held[#held + 1] = string.rep('z', 10 * 1024 * 1024) end",
            "=#lua",
        );
        assert_eq!(
            error_lines(&outcome),
            ["Vosh stopped your #lua line. One call used more than 32 MB."]
        );
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

    /// A `<close>` method that spins, as Lua source.
    const SPINNING_CLOSE: &str = "setmetatable({}, {__close = function() while true do end end})";

    #[test]
    fn a_close_method_that_spins_stops_on_every_path() {
        let loads = [
            // A plugin load, on the main thread.
            format!("local x <close> = {SPINNING_CLOSE} while true do end"),
            // In a coroutine.
            format!(
                "local co = coroutine.wrap(function() \
                   local x <close> = {SPINNING_CLOSE} while true do end \
                 end) co()"
            ),
            // The closing value of a generic for.
            format!(
                "for _ in function() return 1 end, nil, nil, {SPINNING_CLOSE} do \
                   while true do end \
                 end"
            ),
        ];
        for code in loads {
            let outcome = returns_in_time(move || {
                let mut e = ScriptEngine::new().unwrap();
                plugin(&mut e, "spin", &code)
            });
            assert_eq!(outcome.stopped, [Owner::Plugin("spin".into())]);
        }
        // A metatable that gains __close after setmetatable, in a body.
        let outcome = returns_in_time(|| {
            let mut e = ScriptEngine::new().unwrap();
            e.run_body(
                &Owner::trigger("late"),
                "local mt = {} local t = setmetatable({}, mt) \
                 mt.__close = function() while true do end end \
                 local x <close> = t while true do end",
                &[],
            )
        });
        assert_eq!(outcome.stopped, [Owner::trigger("late")]);
    }

    #[test]
    fn a_coroutine_a_stop_ended_never_runs_its_close_methods() {
        let (first, later) = returns_in_time(|| {
            let mut e = ScriptEngine::new().unwrap();
            let first = e.eval(
                &format!(
                    "co = coroutine.create(function() \
                       local x <close> = {SPINNING_CLOSE} while true do end \
                     end) coroutine.resume(co)"
                ),
                "=#lua",
            );
            let later = e.eval(
                "local ok, err = coroutine.close(co) mud.echo(tostring(ok) .. ' ' .. err)",
                "=#lua",
            );
            (first, later)
        });
        assert_eq!(
            error_lines(&first),
            ["Vosh stopped your #lua line after 100 ms."]
        );
        assert_eq!(
            later.unwrap().actions,
            vec![Action::Echo("false Vosh stopped this Lua".into())]
        );
        // A coroutine an ordinary error ended still closes.
        let mut e = ScriptEngine::new().unwrap();
        let closed = e
            .eval(
                "local co = coroutine.create(function() \
                   local x <close> = setmetatable({}, {__close = function() mud.echo('closed') end}) \
                   error('boom', 0) \
                 end) \
                 coroutine.resume(co) mud.echo(select(2, coroutine.close(co)))",
                "=#lua",
            )
            .unwrap();
        assert_eq!(echoes(&closed), ["closed", "boom"]);
    }

    #[test]
    fn closing_the_main_thread_from_a_coroutine_still_stops() {
        let outcome = returns_in_time(|| {
            let mut e = ScriptEngine::new().unwrap();
            plugin(
                &mut e,
                "closer",
                "local main = coroutine.running() \
                 local co = coroutine.wrap(function() \
                   pcall(coroutine.close, main) while true do end \
                 end) co()",
            )
        });
        assert_eq!(outcome.stopped, [Owner::Plugin("closer".into())]);
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

    /// How many bytes of text the actions of `outcome` hold.
    fn text_bytes(outcome: &ScriptOutcome) -> usize {
        outcome.actions.iter().map(Action::text_len).sum()
    }

    #[test]
    fn the_text_one_call_hands_over_stays_within_its_limits() {
        const TOO_MUCH: &str =
            "Your #lua line queued more text than one call may. Vosh dropped what went past the limit.";
        let mut e = ScriptEngine::new().unwrap();
        // Each piece past its own limit drops before Rust copies it.
        let outcome = e.eval(
            "local s = string.rep('x', 10 * 1024 * 1024) \
             for i = 1, 100 do mud.echo(s) end \
             mud.send(string.rep('y', 1025)) mud.input(string.rep('y', 1025)) \
             print(string.rep('z', 64 * 1024 + 1)) \
             mud.set_var('hp', string.rep('9', 4097)) \
             mud.alias(string.rep('a', 4097), 'look') \
             mud.trigger('t', string.rep('.', 4097), function() end) \
             mud.send(string.rep('y', 1024)) mud.echo('kept')",
            "=#lua",
        );
        assert!(!outcome.failed);
        assert_eq!(error_lines(&outcome), [TOO_MUCH]);
        let kept: Vec<&Action> = outcome
            .actions
            .iter()
            .filter(|a| !matches!(a, Action::Error { .. }))
            .collect();
        assert_eq!(
            kept,
            [
                &Action::Send("y".repeat(1024)),
                &Action::Echo("kept".into())
            ]
        );
        assert_eq!(held_callbacks(&e), 0);
        // Pieces that fit stop once the call holds 256 KB of them.
        let outcome = e.eval(
            "local s = string.rep('x', 60 * 1024) for i = 1, 100 do mud.echo(s) end",
            "=#lua",
        );
        assert_eq!(echoes(&outcome).len(), 4);
        assert!(text_bytes(&outcome) < 256 * 1024 + 200);
        assert_eq!(error_lines(&outcome), [TOO_MUCH]);
        // An error line ends where an echo would.
        let outcome = e.eval("error(string.rep('e', 1024 * 1024))", "=#lua");
        assert_eq!(error_line(&outcome).len(), 64 * 1024);
    }

    #[test]
    fn print_and_errors_become_lines() {
        let mut e = ScriptEngine::new().unwrap();
        let printed = e.eval("print('hp', 80, nil, true)", "=#lua").unwrap();
        assert_eq!(
            printed.actions,
            vec![Action::Log {
                owner: Owner::Typed,
                text: "hp\t80\tnil\ttrue".into()
            }]
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
        e.load_script(wait_full.clone(), "@wait_full/main.lua", code)
            .unwrap();
        let vitals = serde_json::json!({"hp": 186, "maxhp": 1020});
        let outcome = e.dispatch_gmcp("Char.Vitals", &vitals);
        // The stop is an error, and how long the plugin stays off a note.
        assert_eq!(
            said(&outcome),
            [
                (
                    "error",
                    wait_full.clone(),
                    "Vosh stopped wait_full at main.lua line 5 after 100 ms.".to_string()
                ),
                (
                    "note",
                    wait_full.clone(),
                    "wait_full stays off until you save it under Scripts in Settings or restart Vosh."
                        .to_string()
                ),
            ]
        );
        assert_eq!(outcome.stopped, std::slice::from_ref(&wait_full));
        assert!(e.is_stopped(&wait_full));
        assert_eq!(e.stop_reason(&wait_full), Some(StopReason::Time));
        // Its handler is gone, so the next packet runs nothing, and a
        // reload leaves it off.
        let leftover = &e.dispatch_gmcp("Char.Vitals", &vitals).actions;
        assert!(leftover.is_empty(), "{leftover:?}");
        let leftover = &reload(&mut e, &[]).actions;
        assert!(leftover.is_empty(), "{leftover:?}");
        let leftover = &e.dispatch_gmcp("Char.Vitals", &vitals).actions;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(held_callbacks(&e), 0);
        // Loading it again turns it back on, and its new handler runs at
        // once on the last packet, which says you are full now.
        let full = serde_json::json!({"hp": 1020, "maxhp": 1020});
        let leftover = &e.dispatch_gmcp("Char.Vitals", &full).actions;
        assert!(leftover.is_empty(), "{leftover:?}");
        let loaded = e
            .load_script(wait_full.clone(), "@wait_full/main.lua", code)
            .unwrap();
        assert!(!e.is_stopped(&wait_full));
        assert!(loaded.actions.contains(&Action::Send("stand".into())));
        assert_eq!(
            e.dispatch_gmcp("Char.Vitals", &full).actions,
            vec![Action::Send("stand".into())]
        );
    }

    #[test]
    fn a_stopped_plugin_stays_stopped_once_turned_off() {
        let mut e = ScriptEngine::new().unwrap();
        let spin = Owner::Plugin("spin".into());
        e.load_script(spin.clone(), "@spin/main.lua", "while true do end");
        assert!(e.is_stopped(&spin));
        e.unload(&spin).unwrap();
        assert!(e.is_stopped(&spin));
        let leftover = &e.loaded_plugins();
        assert!(leftover.is_empty(), "{leftover:?}");
        // A load, as a save makes, turns it back on.
        e.load_script(spin.clone(), "@spin/main.lua", "x = 1")
            .unwrap();
        assert!(!e.is_stopped(&spin));
    }

    #[test]
    fn a_plugin_that_failed_its_first_load_waits_for_a_reload() {
        let mut e = ScriptEngine::new().unwrap();
        let typo = Owner::Plugin("typo".into());
        let failed = e.load_script(typo.clone(), "@typo/main.lua", "mud.ech('x')");
        assert!(failed.failed);
        assert_eq!(e.reload_order(), std::slice::from_ref(&typo));
        // A loose script that failed takes no place.
        let failed = load(&mut e, "missing.lua", "error('typo')");
        assert!(failed.failed);
        assert_eq!(e.reload_order(), [typo]);
    }

    #[test]
    fn a_plugin_load_that_runs_away_stays_off() {
        let mut e = ScriptEngine::new().unwrap();
        let spin = Owner::Plugin("spin".into());
        let outcome = e.load_script(
            spin.clone(),
            "@spin/main.lua",
            "mud.send('look')\nwhile true do end",
        );
        assert_eq!(
            said(&outcome),
            [
                (
                    "error",
                    spin.clone(),
                    "Vosh stopped spin at main.lua line 2 after 100 ms.".to_string()
                ),
                (
                    "note",
                    spin.clone(),
                    "spin stays off until you save it under Scripts in Settings or restart Vosh."
                        .to_string()
                ),
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
        let code = "mud.trigger('hunger', 'You are hungry', function() while true do end end)";
        load(&mut e, "combat.lua", code).unwrap();
        let outcome = e.match_line("You are hungry.");
        assert_eq!(
            error_lines(&outcome),
            ["Vosh stopped combat.lua after 100 ms. It stays off until #script reload."]
        );
        assert!(e.is_stopped(&combat));
        let leftover = &e.lua_triggers();
        assert!(leftover.is_empty(), "{leftover:?}");
        // A reload runs it again and brings its trigger back.
        reload(&mut e, &[("combat.lua", code)]).unwrap();
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
        let tells = Owner::trigger("tells");
        let outcome = e.run_body(&tells, "mud.send('look') while true do end", &[]);
        assert_eq!(
            error_lines(&outcome),
            ["Vosh stopped the Lua in trigger tells after 100 ms. tells stays off until you save it or restart Vosh."]
        );
        assert_eq!(outcome.stopped, [tells]);
        assert_eq!(outcome.actions.len(), 1);
        let heal = Owner::alias("heal");
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
        let tells = Owner::trigger("tells");
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

    #[test]
    fn each_line_carries_the_owner_of_the_call_that_queued_it() {
        let mut e = ScriptEngine::new().unwrap();
        let meals = Owner::Plugin("meals".into());
        let loaded = plugin(
            &mut e,
            "meals",
            "print('meals is here')\n\
             function hungry() mud.log('You are hungry.') end\n\
             mud.on_gmcp('Char.Vitals', function(d) mud.log('hp ' .. d.hp) end)\n\
             mud.trigger('typo', 'The day has begun', function() mud.ech('x') end)",
        )
        .unwrap();
        assert_eq!(
            said(&loaded),
            [("print", meals.clone(), "meals is here".to_string())]
        );
        let vitals = e
            .dispatch_gmcp("Char.Vitals", &serde_json::json!({"hp": 1020}))
            .unwrap();
        assert_eq!(
            said(&vitals),
            [("print", meals.clone(), "hp 1020".to_string())]
        );
        // An error names the plugin and the place Lua names.
        assert_eq!(
            e.match_line("The day has begun.").actions,
            [Action::Error {
                owner: meals.clone(),
                text: "meals/main.lua:4: attempt to call a nil value (field 'ech')".into(),
                at: Some(Place {
                    source: "meals/main.lua".into(),
                    line: 4,
                }),
            }]
        );
        // Your own line is yours, the plugin function it calls included.
        let typed = e
            .eval("print('typed') plugins.meals.hungry()", "=#lua")
            .unwrap();
        assert_eq!(
            said(&typed),
            [
                ("print", Owner::Typed, "typed".to_string()),
                ("print", Owner::Typed, "You are hungry.".to_string()),
            ]
        );
        let tells = Owner::trigger("tells");
        let body = e
            .run_body(&tells, "print(captures[1])", &["Maren".into()])
            .unwrap();
        assert_eq!(said(&body), [("print", tells, "Maren".to_string())]);
        // So does a line about the action cap.
        let flood = Owner::Plugin("flood".into());
        let flooded = plugin(&mut e, "flood", "for i = 1, 150 do mud.send('look') end");
        assert_eq!(
            said(&flooded),
            [(
                "error",
                flood,
                "flood queued more than 100 actions in one call. Vosh dropped the rest."
                    .to_string()
            )]
        );
    }

    #[test]
    fn every_other_stop_prints_one_error_and_no_note() {
        let mut e = ScriptEngine::new().unwrap();
        let spin = "while true do end";
        let tells = Owner::trigger("tells");
        let heal = Owner::alias("heal");
        let combat = Owner::Script("combat.lua".into());
        for (owner, outcome) in [
            (tells.clone(), e.run_body(&tells, spin, &[])),
            (heal.clone(), e.run_body(&heal, spin, &[])),
            (combat.clone(), load(&mut e, "combat.lua", spin)),
            (Owner::Typed, e.eval(spin, "=#lua")),
        ] {
            let said = said(&outcome);
            assert_eq!(said.len(), 1, "{said:?}");
            assert_eq!((said[0].0, &said[0].1), ("error", &owner));
        }
    }

    #[test]
    fn the_engine_keeps_why_it_stopped_each_plugin() {
        let mut e = ScriptEngine::new().unwrap();
        let owner = |name: &str| Owner::Plugin(name.into());
        plugin(&mut e, "spin", "while true do end");
        assert_eq!(e.stop_reason(&owner("spin")), Some(StopReason::Time));
        plugin(
            &mut e,
            "grab",
            "local t = {} for i = 1, 1e9 do t[i] = string.rep('x', 65536) .. i end",
        );
        assert_eq!(e.stop_reason(&owner("grab")), Some(StopReason::CallMemory));
        // Each packet holds 10 MB more, under what one call may use,
        // until the whole state is full.
        plugin(
            &mut e,
            "hoard",
            "held = {} \
             mud.on_gmcp('Char.Vitals', function() \
               held[#held + 1] = string.rep('x', 10 * 1024 * 1024) \
             end)",
        )
        .unwrap();
        for _ in 0..20 {
            if e.dispatch_gmcp("Char.Vitals", &serde_json::json!({}))
                .failed
            {
                break;
            }
        }
        assert_eq!(
            e.stop_reason(&owner("hoard")),
            Some(StopReason::StateMemory)
        );
        // A plugin that runs has none, and a load clears it.
        plugin(&mut e, "spin", "x = 1").unwrap();
        assert_eq!(e.stop_reason(&owner("spin")), None);
        assert_eq!(e.stop_reason(&Owner::Typed), None);
    }

    #[test]
    fn the_console_of_a_plugin_runs_inside_it() {
        let mut e = ScriptEngine::new().unwrap();
        let vitals_alert = Owner::Plugin("vitals_alert".into());
        plugin(&mut e, "vitals_alert", "THRESHOLD = 0.3").unwrap();
        e.eval("THRESHOLD = 'mine'", "=#lua").unwrap();
        // It reads the plugin's globals, and your own Lua reads its own.
        let read = e
            .eval_in_plugin("vitals_alert", "print(THRESHOLD)")
            .unwrap();
        assert_eq!(
            said(&read),
            [("print", vitals_alert.clone(), "0.3".to_string())]
        );
        let mine = e.eval("print(THRESHOLD)", "=#lua").unwrap();
        assert_eq!(said(&mine), [("print", Owner::Typed, "mine".to_string())]);
        // What it registers is the plugin's.
        e.eval_in_plugin(
            "vitals_alert",
            "mud.trigger('day', 'The day has begun', function() print('day') end)",
        )
        .unwrap();
        let triggers = e.lua_triggers();
        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].owner, "plugin:vitals_alert");
        assert_eq!(
            said(&e.match_line("The day has begun.")),
            [("print", vitals_alert.clone(), "day".to_string())]
        );
        // An error names the console line, for the plugin.
        assert_eq!(
            e.eval_in_plugin("vitals_alert", "nope()").actions,
            [Action::Error {
                owner: vitals_alert.clone(),
                text: "#lua:1: attempt to call a nil value (global 'nope')".into(),
                at: Some(Place {
                    source: "#lua".into(),
                    line: 1,
                }),
            }]
        );
        // Off or stopped, it runs nothing and says why.
        e.unload(&vitals_alert).unwrap();
        let off = e.eval_in_plugin("vitals_alert", "print(THRESHOLD)");
        assert_eq!(
            said(&off),
            [(
                "note",
                vitals_alert,
                "vitals_alert is not running, so the console ran nothing.".to_string()
            )]
        );
        plugin(&mut e, "spin", "while true do end");
        let stopped = e.eval_in_plugin("spin", "print(1)");
        assert_eq!(
            said(&stopped),
            [(
                "note",
                Owner::Plugin("spin".into()),
                "spin is stopped, so the console ran nothing.".to_string()
            )]
        );
    }
}

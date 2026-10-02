//! Glue between [`vosh_script::ScriptEngine`] and the rest of the app.
//!
//! Lua callbacks return [`vosh_script::Action`] values; this module
//! applies them to the profile (vars, aliases, triggers), forwards
//! send/echo to the session, and tracks pending one-shot timers so the
//! session loop can fire them at the right time.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::time::Instant;
use vosh_automation::alias::Alias;
use vosh_automation::vars::{Scope, VariableStore};
use vosh_script::{Action, ScriptEngine, ScriptOutcome, VarScope};

use crate::list_events::{ListChanges, ListRevisions};
use crate::profile::Profile;

/// One pending one-shot Lua timer.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PendingTimer {
    pub deadline: Instant,
    pub callback_id: i64,
    pub timer_id: u32,
}

/// Shared list of pending Lua timers. Polled from the `io_loop` on each
/// 250 ms tick; expired entries fire their callbacks.
pub(crate) type SharedTimers = Arc<Mutex<Vec<PendingTimer>>>;

/// Refresh the script engine's view of session vars so `mud.var(name)`
/// returns up-to-date values. Skipped entirely when the engine has
/// no registered triggers / GMCP subs / loaded scripts — without
/// any consumer of `mud.var(name)`, cloning the var map per line
/// is wasted work. This is the common case for users who don't
/// write Lua, and matches what the engine's match/dispatch paths
/// would do anyway (no-op when nothing is registered).
pub(crate) fn snapshot_vars(script: &ScriptEngine, vars: &VariableStore) {
    if script.has_handlers() {
        refresh_vars(script, vars);
    }
}

/// Give Lua the current variables, so `mud.var(name)` reads them. Call
/// before Lua that runs for certain, such as the body of a script alias.
fn refresh_vars(script: &ScriptEngine, vars: &VariableStore) {
    let snapshot: std::collections::HashMap<String, String> = vars
        .iter()
        .map(|(k, v, _)| (k.to_string(), v.to_string()))
        .collect();
    script.set_var_snapshot(snapshot);
}

/// Run the Lua body of a script alias with the words typed after its
/// name, and apply what it asks of the profile. Lua reads the current
/// variables. A body that fails is logged and asks for nothing.
pub(crate) fn run_alias_body(
    profile: &mut Profile,
    call: &vosh_automation::alias::AliasScriptCall,
) -> ApplyResult {
    refresh_vars(&profile.script, &profile.vars);
    match profile
        .script
        .run_body(&call.body, &call.captures, "alias-script")
    {
        Ok(outcome) => apply_actions(profile, outcome),
        Err(err) => {
            tracing::warn!(error = %err, "alias script eval failed");
            ApplyResult::default()
        }
    }
}

/// Result of applying a [`ScriptOutcome`]: bytes to send, lines to echo,
/// timers to schedule. The session loop owns the actual stream and event
/// handle so it does the real IO.
#[derive(Debug, Default)]
pub(crate) struct ApplyResult {
    pub send_bytes: Vec<u8>,
    pub echoes: Vec<String>,
    /// Lines of input to feed back through the input pipeline.
    pub inputs: Vec<String>,
    /// True when any prompt var changed during apply — the session
    /// emits a `session://prompt-vars` snapshot to the frontend
    /// once per apply rather than once per individual set.
    pub prompt_vars_changed: bool,
    /// True when the apply touched DURABLE profile state (aliases,
    /// vars, group toggles) that should reach disk. Drives the
    /// debounced profile persist; ephemeral runtime state (prompt
    /// vars, timers, echoes) does not set it.
    pub durable_changed: bool,
    /// The trigger and alias lists this apply changed, like an alias a
    /// Lua `mud.alias` set. The caller tells the windows.
    pub lists: ListChanges,
    pub new_timers: Vec<PendingTimer>,
    pub cancel_timers: Vec<u32>,
}

impl ApplyResult {
    /// Add what `later` asks for after what this result asks for.
    pub(crate) fn append(&mut self, later: ApplyResult) {
        self.send_bytes.extend(later.send_bytes);
        self.echoes.extend(later.echoes);
        self.inputs.extend(later.inputs);
        self.prompt_vars_changed |= later.prompt_vars_changed;
        self.durable_changed |= later.durable_changed;
        self.lists = ListChanges {
            triggers: self.lists.triggers || later.lists.triggers,
            aliases: self.lists.aliases || later.lists.aliases,
            prompt: self.lists.prompt || later.lists.prompt,
            macro_groups: self.lists.macro_groups || later.lists.macro_groups,
        };
        self.new_timers.extend(later.new_timers);
        self.cancel_timers.extend(later.cancel_timers);
    }
}

/// Apply Lua-produced actions to the profile. The caller hands the
/// returned [`ApplyResult`] to `session::apply_script_result`, which
/// does the IO it lists on every path.
pub(crate) fn apply_actions(profile: &mut Profile, outcome: ScriptOutcome) -> ApplyResult {
    let mut result = ApplyResult::default();
    let lists_before = ListRevisions::of(profile);
    for action in outcome.actions {
        match action {
            Action::Send(line) => {
                result.send_bytes.extend_from_slice(line.as_bytes());
                result.send_bytes.extend_from_slice(b"\r\n");
            }
            Action::Input(line) => {
                result.inputs.push(line);
            }
            Action::Echo(line) => {
                result.echoes.push(line);
            }
            Action::Log(line) => {
                result.echoes.push(format!("[lua] {line}"));
            }
            Action::SetAlias { name, expansion } => {
                define_alias(profile, name, expansion);
                result.durable_changed = true;
            }
            Action::RemoveAlias(name) => {
                profile.aliases.remove(&name);
                result.durable_changed = true;
            }
            Action::SetVar { scope, name, value } => {
                let internal = scope_to_internal(scope);
                // Only profile-scoped vars are persisted; session vars
                // marking durable would reset the persist debounce on
                // every combat line for busy Lua triggers.
                if matches!(internal, Scope::Profile) {
                    result.durable_changed = true;
                }
                profile.vars.set(internal, name, value);
            }
            Action::RemoveVar(name) => {
                profile.vars.remove(&name);
                result.durable_changed = true;
            }
            Action::SetPromptVar { name, value } => {
                profile.prompt.vars.set_script(&name, &value);
                // Always flag as changed. The previous "only fire on
                // value change" semantics suppressed every emit after
                // the first one when the player was at full vitals
                // and the prompt repeated unchanged — which means the
                // frontend never re-rendered the custom prompt template
                // on subsequent prompts. The change-gate optimization
                // saved a few IPC events per second; the trade-off is
                // not worth losing the prompt-as-ready-indicator UX.
                result.prompt_vars_changed = true;
            }
            Action::RemovePromptVar(name) => {
                if profile.prompt.vars.remove_script(&name) {
                    result.prompt_vars_changed = true;
                }
            }
            Action::SetGroupEnabled { name, enabled } => {
                toggle_group(profile, &name, enabled);
                result.durable_changed = true;
            }
            Action::SetLuaTrigger { .. }
            | Action::RemoveLuaTrigger(_)
            | Action::SubscribeGmcp { .. } => {
                // The script engine consumes these in its own drain loop;
                // they should not reach here. Ignore defensively.
            }
            Action::Timer {
                delay,
                callback_id,
                timer_id,
            } => {
                result.new_timers.push(PendingTimer {
                    deadline: Instant::now() + delay,
                    callback_id,
                    timer_id,
                });
            }
            Action::CancelTimer(id) => {
                result.cancel_timers.push(id);
            }
        }
    }
    result.lists = ListChanges::since(lists_before, profile);
    result
}

/// Define the alias `name`, or replace the one of that name, the way
/// `mud.alias`, `#alias`, and `#endrec` do. A replaced alias stays in its group, so
/// the group still turns it on and off. In loadout mode that group is
/// what keeps a character's alias to that character, and a script that
/// set the alias again at launch used to put it in front of every other
/// character too.
pub(crate) fn define_alias(
    profile: &mut Profile,
    name: impl Into<String>,
    expansion: impl Into<String>,
) {
    let mut alias = Alias::new(name, expansion);
    alias.group = profile
        .aliases
        .get(&alias.name)
        .and_then(|old| old.group.clone());
    profile.aliases.set(alias);
}

fn scope_to_internal(scope: VarScope) -> Scope {
    match scope {
        VarScope::Profile => Scope::Profile,
        VarScope::Session => Scope::Session,
    }
}

/// Outcome of `toggle_group`. Reports which stores actually carried
/// at least one entry tagged with the requested group, so callers
/// (the `#group` slash command, the `mud.set_group_enabled` Lua API)
/// can echo something useful rather than silently no-op.
#[derive(Debug, Default)]
pub(crate) struct GroupToggleReport {
    pub aliases: bool,
    pub triggers: bool,
    pub macros: bool,
}

impl GroupToggleReport {
    pub(crate) fn touched(&self) -> bool {
        self.aliases || self.triggers || self.macros
    }
}

/// Flip a group's enabled state across triggers, aliases, and macros
/// in one shot. The group lives independently in each store, so this
/// only touches the stores that actually have a matching entry —
/// asking to disable a group that exists only in triggers won't
/// stamp an empty group name into the macro disabled set. In loadout
/// mode `name` is one of your folders, and each store turns on or off
/// every catalog group the profile's folder map names for it, see
/// [`crate::profile_config::GroupFolders`]. A macro group that turned
/// on or off moves [`Profile::macro_group_toggles`], so the windows
/// hear it through [`crate::list_events::ListChanges`].
pub(crate) fn toggle_group(profile: &mut Profile, name: &str, enabled: bool) -> GroupToggleReport {
    let mut report = GroupToggleReport::default();
    let folders = &profile.group_folders;
    let alias_groups = store_groups(profile.aliases.groups());
    for group in folder_groups(&folders.aliases, name) {
        if alias_groups.contains(group) {
            profile.aliases.set_group_enabled(group, enabled);
            report.aliases = true;
        }
    }
    let trigger_groups = store_groups(profile.triggers.groups());
    for group in folder_groups(&folders.triggers, name) {
        if trigger_groups.contains(group) {
            profile.triggers.set_group_enabled(group, enabled);
            report.triggers = true;
        }
    }
    let macro_groups = macro_groups(profile);
    for group in folder_groups(&folders.macros, name) {
        if macro_groups.contains(group) {
            let turned = if enabled {
                profile.disabled_macro_groups.remove(group)
            } else {
                profile.disabled_macro_groups.insert(group.to_string())
            };
            if turned {
                profile.macro_group_toggles = profile.macro_group_toggles.wrapping_add(1);
            }
            report.macros = true;
        }
    }
    report
}

/// Whether a folder is on in one store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GroupState {
    On,
    Off,
    /// Some of the catalog groups the folder stands for are on and some
    /// off, after a checkbox in Settings turned one of them alone.
    Mixed,
}

/// The state of the folder or group `name` in each store, triggers,
/// aliases, then macros, None where the store holds none of it. Follows
/// the folder map as [`toggle_group`] does.
pub(crate) fn group_states(profile: &Profile, name: &str) -> [Option<GroupState>; 3] {
    let folders = &profile.group_folders;
    let state = |groups: Vec<(String, bool)>, map: &BTreeMap<String, Vec<String>>| {
        let wanted = folder_groups(map, name);
        let found: Vec<bool> = groups
            .into_iter()
            .filter(|(g, _)| wanted.contains(&g.as_str()))
            .map(|(_, on)| on)
            .collect();
        match (found.iter().any(|on| *on), found.iter().any(|on| !*on)) {
            (false, false) => None,
            (true, false) => Some(GroupState::On),
            (false, true) => Some(GroupState::Off),
            (true, true) => Some(GroupState::Mixed),
        }
    };
    let macros = macro_groups(profile)
        .into_iter()
        .map(|g| {
            let on = !profile.disabled_macro_groups.contains(&g);
            (g, on)
        })
        .collect();
    [
        state(profile.triggers.groups(), &folders.triggers),
        state(profile.aliases.groups(), &folders.aliases),
        state(macros, &folders.macros),
    ]
}

/// The catalog groups `name` stands for in one store: the profile's
/// folder map entry for it, or else the group of that name.
fn folder_groups<'a>(folders: &'a BTreeMap<String, Vec<String>>, name: &'a str) -> Vec<&'a str> {
    match folders.get(name) {
        Some(groups) => groups.iter().map(String::as_str).collect(),
        None => vec![name],
    }
}

fn store_groups(groups: Vec<(String, bool)>) -> BTreeSet<String> {
    groups.into_iter().map(|(g, _)| g).collect()
}

/// Every group a macro of `profile` is in.
fn macro_groups(profile: &Profile) -> BTreeSet<String> {
    profile
        .macros
        .iter()
        .filter_map(|m| m.group.clone())
        .filter(|g| !g.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_alias(name: &str, expansion: &str) -> ScriptOutcome {
        ScriptOutcome {
            actions: vec![Action::SetAlias {
                name: name.into(),
                expansion: expansion.into(),
            }],
        }
    }

    #[test]
    fn a_lua_alias_replaces_one_in_its_group() {
        let mut p = Profile::default();
        let mut heal = Alias::new("hl", "cast heal");
        heal.group = Some("healing".into());
        p.aliases.set(heal);
        apply_actions(&mut p, set_alias("hl", "cast 'cure light'"));
        let hl = p.aliases.get("hl").unwrap();
        assert_eq!(hl.expansion, "cast 'cure light'");
        // It used to lose its group, so turning the group off no longer
        // turned it off, and in loadout mode it came on for every
        // character.
        assert_eq!(hl.group.as_deref(), Some("healing"));
    }

    /// A Healer after the shared catalog wizard. Its combat folder became
    /// the combat group it shares and the combat (Healer) group of its
    /// own. The default profile's combat (default) group is off for it.
    fn healer_after_the_wizard() -> Profile {
        let mut p = Profile::default();
        for (name, group) in [
            ("flee", "combat"),
            ("bash", "combat (Healer)"),
            ("kick", "combat (default)"),
        ] {
            let mut alias = Alias::new(name, name);
            alias.group = Some(group.into());
            p.aliases.set(alias);
        }
        p.aliases.set_disabled_groups(["combat (default)"]);
        p.group_folders.aliases.insert(
            "combat".into(),
            vec!["combat".into(), "combat (Healer)".into()],
        );
        p.group_folders.aliases.insert("loot".into(), Vec::new());
        p
    }

    fn aliases_on(p: &Profile) -> Vec<String> {
        p.aliases
            .list()
            .into_iter()
            .filter(|a| p.aliases.is_group_enabled(a.group.as_deref().unwrap_or("")))
            .map(|a| a.name.clone())
            .collect()
    }

    #[test]
    fn a_folder_turns_off_every_catalog_group_it_became() {
        let mut p = healer_after_the_wizard();
        assert_eq!(aliases_on(&p), ["bash", "flee"]);
        let report = toggle_group(&mut p, "combat", false);
        assert!(report.aliases);
        let leftover = &aliases_on(&p);
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(group_states(&p, "combat")[1], Some(GroupState::Off));
        // Turning it on again brings back the Healer's own, and never the
        // default profile's.
        toggle_group(&mut p, "combat", true);
        assert_eq!(aliases_on(&p), ["bash", "flee"]);
        assert_eq!(group_states(&p, "combat")[1], Some(GroupState::On));
    }

    #[test]
    fn a_folder_the_profile_never_had_turns_nothing_on() {
        let mut p = healer_after_the_wizard();
        let mut loot = Alias::new("loot", "get all");
        loot.group = Some("loot".into());
        p.aliases.set(loot);
        p.aliases.set_group_enabled("loot", false);
        assert!(!toggle_group(&mut p, "loot", true).touched());
        assert_eq!(aliases_on(&p), ["bash", "flee"]);
        assert_eq!(group_states(&p, "loot"), [None, None, None]);
    }

    #[test]
    fn a_lua_group_toggle_follows_the_folder() {
        let mut p = healer_after_the_wizard();
        let outcome = ScriptOutcome {
            actions: vec![Action::SetGroupEnabled {
                name: "combat".into(),
                enabled: false,
            }],
        };
        apply_actions(&mut p, outcome);
        let leftover = &aliases_on(&p);
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn a_new_lua_alias_has_no_group() {
        let mut p = Profile::default();
        apply_actions(&mut p, set_alias("hl", "cast heal"));
        assert_eq!(p.aliases.get("hl").unwrap().group, None);
    }
}

//! Glue between [`vosh_script::ScriptEngine`] and the rest of the app.
//!
//! Lua callbacks return [`vosh_script::Action`] values; this module
//! applies them to the profile (vars, aliases, triggers), forwards
//! send/echo to the session, and tracks pending one-shot timers so the
//! session loop can fire them at the right time. Every `[lua]` line it
//! prints joins the session's Output ring in [`output`].

pub(crate) mod output;
pub(crate) mod panes;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::time::Instant;
use vosh_automation::alias::Alias;
use vosh_automation::vars::Scope;
use vosh_automation::StopKey;
use vosh_script::{Action, Owner, Place, ScriptOutcome};

use self::output::{LuaKind, LuaLine};
use crate::app::events::{ListChanges, ListRevisions};
use crate::input::LineFrom;
use crate::profile::live::Profile;
use crate::profile::open::OpenProfile;
use crate::session::connection::Connection;

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
pub(crate) fn snapshot_vars(profile: &Profile, c: &Connection) {
    if c.script.has_handlers() {
        refresh_vars(profile, c);
    }
}

/// Give the session's Lua its variables over the profile's, so
/// `mud.var(name)` reads them. Call before Lua that runs for certain,
/// such as the body of a script alias, a `#lua` line or a load.
pub(crate) fn refresh_vars(profile: &Profile, c: &Connection) {
    let snapshot: std::collections::HashMap<String, String> = c
        .var_view(profile)
        .iter()
        .map(|(k, v, _)| (k.to_string(), v.to_string()))
        .collect();
    c.script.set_var_snapshot(snapshot);
}

/// Run the Lua body of a script alias with the words typed after its
/// name, and apply what it asks of the profile. Lua reads the current
/// variables. A body that fails asks for what it queued before the
/// error, and its error line prints. A body Vosh stopped asks for
/// nothing and turns its alias off, so a later step of the same line
/// that names the alias runs nothing.
pub(crate) fn run_alias_body(
    profile: &mut Profile,
    c: &mut Connection,
    call: &vosh_automation::ScriptCall,
) -> ApplyResult {
    if profile
        .aliases
        .is_stopped(call.group.as_deref(), &call.source, c.stop_key)
    {
        return ApplyResult::default();
    }
    refresh_vars(profile, c);
    let owner = Owner::Alias {
        name: call.source.clone(),
        group: call.group.clone(),
    };
    let outcome = c.script.run_body(&owner, &call.body, &call.captures);
    apply_actions(profile, c, outcome)
}

/// Run `code` from the console on the Scripts page in the session of
/// `c`: inside the plugin `plugin`, or else in the global environment as
/// a `#lua` line runs. The line you typed joins the Output ring first,
/// so what it prints follows it.
pub(crate) fn run_console(
    profile: &mut Profile,
    c: &mut Connection,
    code: &str,
    plugin: Option<&str>,
) -> ApplyResult {
    let owner = plugin.map_or(Owner::Typed, |name| Owner::Plugin(name.to_string()));
    let mut result = ApplyResult::default();
    result.keep_lua_lines(c, &owner, LuaKind::Input, code, None);
    refresh_vars(profile, c);
    let outcome = match plugin {
        Some(name) => c.script.eval_in_plugin(name, code),
        None => c.script.eval(code, "=#lua"),
    };
    result.append(apply_actions(profile, c, outcome));
    result
}

/// Turn off each trigger and alias whose Lua Vosh stopped in `outcome`,
/// in the session whose stops `key` names, until you save it again or
/// restart Vosh. The engine holds a stopped plugin or loose script off
/// itself.
pub(crate) fn turn_off_stopped(profile: &mut Profile, key: StopKey, outcome: &ScriptOutcome) {
    for owner in &outcome.stopped {
        match owner {
            Owner::Trigger(name) => profile.triggers.stop(name, key),
            Owner::Alias { name, group } => profile.aliases.stop(group.as_deref(), name, key),
            Owner::Plugin(_) | Owner::Script(_) | Owner::Typed => {}
        }
    }
}

/// The `[lua]` tag before each line Vosh prints about Lua, in the
/// theme's bright black, so the line reads as Vosh and not the game.
const LUA_TAG: &str = "\x1b[90m[lua]\x1b[0m";

/// The terminal lines for `text` from `print`, `mud.log` or a note of
/// Vosh's, one tagged line for each line of the text, in the default
/// color.
fn lua_lines(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split('\n')
        .map(|line| format!("{LUA_TAG} {}", line.trim_end_matches('\r')))
}

/// The terminal lines for a Lua error or a stop, tagged and in the
/// theme's red.
pub(crate) fn lua_error_lines(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split('\n')
        .map(|line| format!("{LUA_TAG} \x1b[31m{}\x1b[0m", line.trim_end_matches('\r')))
}

/// Result of applying a [`ScriptOutcome`]: bytes to send, lines to echo,
/// timers to schedule. The session loop owns the actual stream and event
/// handle so it does the real IO.
#[derive(Debug, Default)]
pub(crate) struct ApplyResult {
    pub send_bytes: Vec<u8>,
    pub echoes: Vec<String>,
    /// Lines of input to feed back through the input pipeline, each with
    /// whose Lua asked for it.
    pub inputs: Vec<(LineFrom, String)>,
    /// True when any prompt var changed during apply — the session
    /// emits a `session://prompt-vars` snapshot to the frontend
    /// once per apply rather than once per individual set.
    pub prompt_vars_changed: bool,
    /// True when the apply touched DURABLE profile state (aliases,
    /// vars, group toggles) that should reach disk. Drives the
    /// debounced profile persist; ephemeral runtime state (prompt
    /// vars, timers, echoes) does not set it.
    pub durable_changed: bool,
    /// The profile the Lua ran under, which a durable change marks to
    /// save. The step that held its lock names it through
    /// [`ApplyResult::ran_under`], since the session may play another
    /// profile by the time the result applies.
    pub profile: Option<Arc<OpenProfile>>,
    /// The trigger and alias lists this apply changed, like an alias a
    /// Lua `mud.alias` set. The caller tells the windows.
    pub lists: ListChanges,
    pub new_timers: Vec<PendingTimer>,
    pub cancel_timers: Vec<u32>,
    /// A `#walk` a line ran, for the walker in the session, after the
    /// bytes. Only the input pipeline sets it.
    pub walk: Option<crate::input::walk::WalkCommand>,
    /// The alerts the step raised, a trigger's, a preset's or Lua's,
    /// which ring once the locks let go, see [`crate::alert::ring`].
    pub alerts: Vec<crate::alert::Alert>,
    /// The Lua owners whose alerts end, each a plugin that turned off,
    /// stopped or loaded again, see [`crate::alert::end_owner`].
    pub ended: Vec<String>,
    /// The lines this apply added to the session's Output ring, which
    /// the Scripts page hears on `session://lua-output`.
    pub lua_lines: Vec<LuaLine>,
    /// Vosh stopped a plugin, so the Scripts page reads its list again,
    /// see `vosh://plugins-changed`.
    pub plugin_stopped: bool,
    /// A plugin drew, filled or lost a pane, so the session sends what
    /// changed on `session://lua-panes`, once per flush.
    pub panes_changed: bool,
}

impl ApplyResult {
    /// Add what `later` asks for after what this result asks for. A
    /// `#walk` in `later` takes over one in this result, as a second walk
    /// takes over the first.
    pub(crate) fn append(&mut self, later: ApplyResult) {
        self.send_bytes.extend(later.send_bytes);
        self.echoes.extend(later.echoes);
        self.inputs.extend(later.inputs);
        self.prompt_vars_changed |= later.prompt_vars_changed;
        self.durable_changed |= later.durable_changed;
        self.profile = later.profile.or(self.profile.take());
        self.lists = self.lists.or(later.lists);
        self.new_timers.extend(later.new_timers);
        self.cancel_timers.extend(later.cancel_timers);
        if later.walk.is_some() {
            self.walk = later.walk;
        }
        self.alerts.extend(later.alerts);
        self.ended.extend(later.ended);
        self.lua_lines.extend(later.lua_lines);
        self.plugin_stopped |= later.plugin_stopped;
        self.panes_changed |= later.panes_changed;
    }

    /// Keep the lines `text` prints under the `[lua]` tag about the Lua
    /// of `owner` in the Output ring of `c`, and among the lines this
    /// result tells the page.
    fn keep_lua_lines(
        &mut self,
        c: &mut Connection,
        owner: &Owner,
        kind: LuaKind,
        text: &str,
        at: Option<&Place>,
    ) {
        for line in LuaLine::lines(owner, kind, text, at, crate::session::now_ms()) {
            c.lua_output.push(line.clone());
            self.lua_lines.push(line);
        }
    }

    /// This result, whose Lua ran under `open` while the step held it.
    pub(crate) fn ran_under(mut self, open: &Arc<OpenProfile>) -> Self {
        self.profile = Some(Arc::clone(open));
        self
    }
}

/// Apply Lua-produced actions to the profile, and the prompt values a
/// script sets to the prompt engine on the connection. The caller hands
/// the returned [`ApplyResult`] to
/// `session::effects::apply_script_result`, which does the IO it lists
/// on every path.
pub(crate) fn apply_actions(
    profile: &mut Profile,
    c: &mut Connection,
    outcome: ScriptOutcome,
) -> ApplyResult {
    let mut result = ApplyResult::default();
    let lists_before = ListRevisions::of(profile, c);
    turn_off_stopped(profile, c.stop_key, &outcome);
    result.plugin_stopped = outcome
        .stopped
        .iter()
        .any(|owner| matches!(owner, Owner::Plugin(_)));
    for action in outcome.actions {
        match action {
            Action::Send(line) => {
                result.send_bytes.extend_from_slice(line.as_bytes());
                result.send_bytes.extend_from_slice(b"\r\n");
            }
            // The input pipeline decides what the line may run, from
            // whose Lua asked for it.
            Action::Input { owner, line } => result.inputs.push((LineFrom::lua(&owner), line)),
            Action::Echo(line) => {
                result.echoes.push(line);
            }
            Action::Log { owner, text } => {
                result.echoes.extend(lua_lines(&text));
                result.keep_lua_lines(c, &owner, LuaKind::Print, &text, None);
            }
            // A note, such as how long a stopped plugin stays off, reads
            // plain, as a print does.
            Action::Note { owner, text } => {
                result.echoes.extend(lua_lines(&text));
                result.keep_lua_lines(c, &owner, LuaKind::Note, &text, None);
            }
            Action::Error { owner, text, at } => {
                result.echoes.extend(lua_error_lines(&text));
                result.keep_lua_lines(c, &owner, LuaKind::Error, &text, at.as_ref());
            }
            Action::SetAlias { name, expansion } => {
                define_alias(profile, c.stop_key, name, expansion);
                result.durable_changed = true;
            }
            // The alias mud.alias would replace: the one that fires in
            // this session, else the first Settings lists.
            Action::RemoveAlias(name) => {
                if let Some(old) = profile.aliases.chosen(&name, c.stop_key).cloned() {
                    profile.aliases.remove(old.group.as_deref(), &old.name);
                }
                result.durable_changed = true;
            }
            // A plugin's aliases last in its session, so nothing saves.
            Action::SetPluginAlias {
                plugin,
                name,
                expansion,
            } => c.plugin_aliases.set(&plugin, name, expansion),
            Action::RemovePluginAlias { plugin, name } => {
                c.plugin_aliases.remove(&plugin, &name);
            }
            // The plugin turned off, stopped or loaded again, so its
            // aliases, its alerts and its panes end, and its panes empty
            // at once.
            Action::DropPlugin(plugin) => {
                c.plugin_aliases.remove_plugin(&plugin);
                result.panes_changed |= c.lua_panes.drop_plugin(&plugin);
                result.ended.push(vosh_script::Owner::Plugin(plugin).tag());
            }
            // Only profile-scoped vars are persisted; session vars
            // marking durable would reset the persist debounce on
            // every combat line for busy Lua triggers.
            Action::SetVar { scope, name, value } => match scope {
                Scope::Session => c.vars.set(name, value),
                Scope::Profile => {
                    profile.vars.set(name, value);
                    result.durable_changed = true;
                }
            },
            // Both scopes, so the profile half goes for every session.
            Action::RemoveVar(name) => {
                c.vars.remove(&name);
                profile.vars.remove(&name);
                result.durable_changed = true;
            }
            Action::SetPromptVar { name, value } => {
                c.prompt.vars.set_script(&name, &value);
                // Always flag as changed. A gate on change would send
                // nothing after the first prompt while you sit at full
                // vitals and the prompt repeats unchanged, so the page
                // would never draw the custom prompt again for the
                // prompts after it. The few events a second the gate
                // would save are not worth losing the prompt as a sign
                // that the game is ready.
                result.prompt_vars_changed = true;
            }
            Action::RemovePromptVar(name) => {
                if c.prompt.vars.remove_script(&name) {
                    result.prompt_vars_changed = true;
                }
            }
            Action::SetGroupEnabled { name, enabled } => {
                toggle_group(profile, &name, enabled);
                result.durable_changed = true;
            }
            Action::Pane { plugin, id, title } => {
                c.lua_panes.draw(plugin, id, title);
                result.panes_changed = true;
            }
            Action::PaneSet { plugin, id, blocks } => {
                result.panes_changed |= c.lua_panes.set(plugin, id, blocks);
            }
            Action::PaneMeta { plugin, id, text } => {
                result.panes_changed |= c.lua_panes.meta(plugin, id, text);
            }
            // The script engine consumes the Lua registrations in its own
            // drain loop, so they should not reach here.
            Action::SetLuaTrigger { .. }
            | Action::RemoveLuaTrigger { .. }
            | Action::SubscribeGmcp { .. } => {}
            Action::Timer {
                delay,
                callback_id,
                timer_id,
            } => {
                // Lua caps a delay at a day, and a deadline past what the
                // clock can hold never comes, so such a timer waits a day.
                let now = Instant::now();
                let deadline = now
                    .checked_add(delay)
                    .or_else(|| now.checked_add(std::time::Duration::from_secs(24 * 60 * 60)));
                if let Some(deadline) = deadline {
                    result.new_timers.push(PendingTimer {
                        deadline,
                        callback_id,
                        timer_id,
                    });
                }
            }
            Action::CancelTimer(id) => {
                result.cancel_timers.push(id);
            }
            Action::Alert {
                owner,
                title,
                text,
                parts,
            } => result
                .alerts
                .push(crate::alert::of_lua(&owner, title, text, parts)),
        }
    }
    result.lists = ListChanges::since(lists_before, profile, c);
    result
}

/// Define the alias `name`, or replace one of that name, the way
/// `mud.alias`, `#alias`, and `#endrec` do, and answer how a message
/// names it. When groups hold more than one alias of the name, the one
/// replaced is the one that fires in the session `key` names, else the
/// first Settings lists. A replaced alias stays in its group, so the
/// group still turns it on and off. In loadout mode that group is what
/// keeps a character's alias to that character, even when a script sets
/// the alias again at launch. A new alias goes in no group.
pub(crate) fn define_alias(
    profile: &mut Profile,
    key: StopKey,
    name: impl Into<String>,
    expansion: impl Into<String>,
) -> String {
    let mut alias = Alias::new(name, expansion);
    alias.name = alias.name.trim().to_string();
    alias.group = profile
        .aliases
        .chosen(&alias.name, key)
        .and_then(|old| old.group.clone());
    let label = alias.label();
    profile.aliases.set(alias);
    label
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
    pub timers: bool,
}

impl GroupToggleReport {
    pub(crate) fn touched(&self) -> bool {
        self.aliases || self.triggers || self.macros || self.timers
    }
}

/// Flip a group's enabled state across triggers, aliases, macros and
/// timers in one shot. The group lives independently in each store, so
/// this only touches the stores that actually have a matching entry —
/// asking to disable a group that exists only in triggers won't
/// stamp an empty group name into the macro disabled set. In loadout
/// mode `name` is one of your folders, and each store turns on or off
/// every catalog group the profile's folder map names for it, see
/// [`crate::profile::file::GroupFolders`]. Timers stay in the profile
/// file, so a timer group is always its own name. Each group turns
/// through [`set_list_group`], the call the switch on a group heading in
/// Settings makes, so the two agree.
pub(crate) fn toggle_group(profile: &mut Profile, name: &str, enabled: bool) -> GroupToggleReport {
    let mut report = GroupToggleReport::default();
    for list in GroupList::ALL {
        let present: BTreeSet<String> = list_groups(profile, list)
            .into_iter()
            .map(|(g, _)| g)
            .collect();
        for group in wanted_groups(profile, list, name) {
            if present.contains(&group) {
                set_list_group(profile, list, &group, enabled);
                match list {
                    GroupList::Triggers => report.triggers = true,
                    GroupList::Aliases => report.aliases = true,
                    GroupList::Macros => report.macros = true,
                    GroupList::Timers => report.timers = true,
                }
            }
        }
    }
    report
}

/// A list whose items sit in groups. The page names it as the
/// Automation list does, `triggers`, `aliases`, `macros` or `timers`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum GroupList {
    Triggers,
    Aliases,
    Macros,
    Timers,
}

impl GroupList {
    /// Every list, in the order `#group` reports them.
    pub(crate) const ALL: [GroupList; 4] = [
        GroupList::Triggers,
        GroupList::Aliases,
        GroupList::Macros,
        GroupList::Timers,
    ];

    /// Whether loadout mode's catalog holds this list, so the loadouts
    /// can decide its groups. Timers stay in the profile file.
    pub(crate) fn in_catalog(self) -> bool {
        self != GroupList::Timers
    }
}

/// Each group the items of `list` name, sorted and once each, paired
/// with whether it is on.
pub(crate) fn list_groups(profile: &Profile, list: GroupList) -> Vec<(String, bool)> {
    let with_state = |groups: BTreeSet<String>, off: &BTreeSet<String>| {
        groups
            .into_iter()
            .map(|g| {
                let on = !off.contains(&g);
                (g, on)
            })
            .collect()
    };
    match list {
        GroupList::Triggers => profile.triggers.groups(),
        GroupList::Aliases => profile.aliases.groups(),
        GroupList::Macros => with_state(macro_groups(profile), &profile.disabled_macro_groups),
        GroupList::Timers => with_state(timer_groups(profile), &profile.disabled_timer_groups),
    }
}

/// Turn the group `group` of `list` on or off by its own name, as the
/// switch on its heading in Settings does. Returns whether it turned. A
/// group that turned moves [`Profile::group_toggles`], and a macro group
/// [`Profile::macro_group_toggles`] as well, so the windows hear it
/// through [`crate::app::events::ListChanges`]. A macro group also lays
/// the hold of the preset macros again, see
/// [`crate::loadouts::presets::hold_profile_keys`].
pub(crate) fn set_list_group(
    profile: &mut Profile,
    list: GroupList,
    group: &str,
    enabled: bool,
) -> bool {
    if group.is_empty() {
        return false;
    }
    let set_in = |off: &mut BTreeSet<String>| {
        if enabled {
            off.remove(group)
        } else {
            off.insert(group.to_string())
        }
    };
    let turned = match list {
        GroupList::Triggers => profile.triggers.set_group_enabled(group, enabled),
        GroupList::Aliases => profile.aliases.set_group_enabled(group, enabled),
        GroupList::Macros => set_in(&mut profile.disabled_macro_groups),
        GroupList::Timers => set_in(&mut profile.disabled_timer_groups),
    };
    if turned {
        profile.group_toggles = profile.group_toggles.wrapping_add(1);
        if list == GroupList::Macros {
            profile.macro_group_toggles = profile.macro_group_toggles.wrapping_add(1);
            crate::loadouts::presets::hold_profile_keys(profile);
        }
    }
    turned
}

/// The groups of `list` the folder or group `name` stands for: the
/// profile's folder map entry for it, or else the group of that name.
fn wanted_groups(profile: &Profile, list: GroupList, name: &str) -> Vec<String> {
    let folders = &profile.group_folders;
    let map = match list {
        GroupList::Triggers => &folders.triggers,
        GroupList::Aliases => &folders.aliases,
        GroupList::Macros => &folders.macros,
        GroupList::Timers => return vec![name.to_string()],
    };
    folder_groups(map, name)
        .into_iter()
        .map(str::to_string)
        .collect()
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
/// aliases, macros, then timers, None where the store holds none of it.
/// Follows the folder map as [`toggle_group`] does.
pub(crate) fn group_states(profile: &Profile, name: &str) -> [Option<GroupState>; 4] {
    GroupList::ALL.map(|list| {
        let wanted = wanted_groups(profile, list, name);
        let found: Vec<bool> = list_groups(profile, list)
            .into_iter()
            .filter(|(g, _)| wanted.contains(g))
            .map(|(_, on)| on)
            .collect();
        match (found.iter().any(|on| *on), found.iter().any(|on| !*on)) {
            (false, false) => None,
            (true, false) => Some(GroupState::On),
            (false, true) => Some(GroupState::Off),
            (true, true) => Some(GroupState::Mixed),
        }
    })
}

/// The catalog groups `name` stands for in one store: the profile's
/// folder map entry for it, or else the group of that name.
fn folder_groups<'a>(folders: &'a BTreeMap<String, Vec<String>>, name: &'a str) -> Vec<&'a str> {
    match folders.get(name) {
        Some(groups) => groups.iter().map(String::as_str).collect(),
        None => vec![name],
    }
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

/// Every group an interval timer of `profile` is in.
pub(crate) fn timer_groups(profile: &Profile) -> BTreeSet<String> {
    profile
        .timers
        .iter()
        .filter_map(|t| t.group.clone())
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
            ..ScriptOutcome::default()
        }
    }

    #[test]
    fn a_lua_alias_replaces_one_in_its_group() {
        let mut p = Profile::default();
        let mut c = Connection::default();
        let mut heal = Alias::new("hl", "cast heal");
        heal.group = Some("healing".into());
        p.aliases.set(heal);
        apply_actions(&mut p, &mut c, set_alias("hl", "cast 'cure light'"));
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
        assert_eq!(group_states(&p, "loot"), [None, None, None, None]);
    }

    #[test]
    fn a_lua_group_toggle_follows_the_folder() {
        let mut p = healer_after_the_wizard();
        let outcome = ScriptOutcome {
            actions: vec![Action::SetGroupEnabled {
                name: "combat".into(),
                enabled: false,
            }],
            ..ScriptOutcome::default()
        };
        apply_actions(&mut p, &mut Connection::default(), outcome);
        let leftover = &aliases_on(&p);
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn a_timer_no_clock_can_hold_still_applies() {
        let mut p = Profile::default();
        let mut c = Connection::default();
        let outcome = ScriptOutcome {
            actions: vec![Action::Timer {
                delay: std::time::Duration::MAX,
                callback_id: 1,
                timer_id: 1,
            }],
            ..ScriptOutcome::default()
        };
        let apply = apply_actions(&mut p, &mut c, outcome);
        assert_eq!(apply.new_timers.len(), 1);
    }

    /// What `actions`, from the plugin `weather_pane`, do to `c`.
    fn apply_pane(c: &mut Connection, actions: Vec<Action>) -> ApplyResult {
        let outcome = ScriptOutcome {
            actions,
            ..ScriptOutcome::default()
        };
        apply_actions(&mut Profile::default(), c, outcome)
    }

    fn draw(id: &str) -> Action {
        Action::Pane {
            plugin: "weather_pane".into(),
            id: id.into(),
            title: "Weather".into(),
        }
    }

    fn row(id: &str, value: &str) -> Action {
        Action::PaneSet {
            plugin: "weather_pane".into(),
            id: id.into(),
            blocks: vec![vosh_script::PaneBlock::Row {
                label: "Sky".into(),
                value: value.into(),
            }],
        }
    }

    #[test]
    fn a_plugin_fills_its_pane_and_two_sets_send_it_once() {
        let mut c = Connection::default();
        let meta = Action::PaneMeta {
            plugin: "weather_pane".into(),
            id: "weather".into(),
            text: "Coastal North".into(),
        };
        let apply = apply_pane(
            &mut c,
            vec![
                draw("weather"),
                row("weather", "cloudy"),
                row("weather", "rainy"),
                meta,
            ],
        );
        assert!(apply.panes_changed);
        let sent = c.lua_panes.take_changes().expect("the pane");
        assert_eq!(sent.panes.len(), 1, "{sent:?}");
        assert_eq!(sent.removed, Vec::new());
        let pane = &sent.panes[0];
        assert_eq!(
            (pane.title.as_str(), pane.meta.as_str()),
            ("Weather", "Coastal North")
        );
        assert_eq!(
            pane.blocks,
            [panes::Block::Row {
                label: "Sky".into(),
                value: "rainy".into()
            }]
        );
        // A set for a pane the plugin never drew changes nothing.
        assert!(!apply_pane(&mut c, vec![row("worth", "rainy")]).panes_changed);
        assert_eq!(c.lua_panes.take_changes(), None);
    }

    #[test]
    fn a_dropped_plugin_empties_its_panes_at_once() {
        let mut c = Connection::default();
        apply_pane(&mut c, vec![draw("weather"), row("weather", "rainy")]);
        c.lua_panes.take_changes();
        let apply = apply_pane(&mut c, vec![Action::DropPlugin("weather_pane".into())]);
        assert!(apply.panes_changed);
        assert_eq!(c.lua_panes.all(), Vec::new());
        let sent = c.lua_panes.take_changes().expect("the removal");
        assert_eq!(sent.panes, Vec::new());
        assert_eq!(
            sent.removed,
            [panes::PaneId {
                plugin: "weather_pane".into(),
                id: "weather".into()
            }]
        );
        // A plugin that draws none changes nothing as it goes.
        assert!(!apply_pane(&mut c, vec![Action::DropPlugin("worth_pane".into())]).panes_changed);
    }

    #[test]
    fn a_reload_that_draws_the_pane_again_sends_it_and_no_removal() {
        let mut c = Connection::default();
        apply_pane(&mut c, vec![draw("weather"), row("weather", "rainy")]);
        c.lua_panes.take_changes();
        apply_pane(
            &mut c,
            vec![Action::DropPlugin("weather_pane".into()), draw("weather")],
        );
        let sent = c.lua_panes.take_changes().expect("the pane");
        assert_eq!(sent.removed, Vec::new());
        assert_eq!(sent.panes.len(), 1);
        assert_eq!(sent.panes[0].blocks, Vec::new());
    }

    #[test]
    fn a_new_lua_alias_has_no_group() {
        let mut p = Profile::default();
        let mut c = Connection::default();
        apply_actions(&mut p, &mut c, set_alias("hl", "cast heal"));
        assert_eq!(p.aliases.get("hl").unwrap().group, None);
    }
}

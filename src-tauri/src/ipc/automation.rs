//! The commands for your automation. Settings lists and saves your
//! triggers, aliases, macros and timers through them, adds and removes
//! what the presets add, and imports another client's file into the live
//! profile. The command line reads your macros through them too.
//!
//! Each command that reads or changes a profile takes the `profile` it
//! means, which a session must play, and acts on the selected session's
//! when it names none. Every window hears a change only while that
//! profile is the one in front.

use serde::de::value::{self, StrDeserializer};
use serde::Deserialize;
use tauri::{AppHandle, State};
use vosh_automation::trigger::Trigger;

use crate::app::events::{
    broadcast, broadcast_list_changes, ListChanges, ListRevisions, PresetsChanged, MACROS_CHANGED,
    PRESETS_CHANGED, TIMERS_CHANGED,
};
use crate::app::state::SharedState;
use crate::disk::save::{persist_profile, save_then_broadcast, SavePolicy};
use crate::import::{merge_triggers, ImportFormat};
use crate::loadouts::gating::{loadout_hold, LoadoutHold};
use crate::loadouts::presets::{
    delete_macro, import_macros, install_preset_macros, install_preset_triggers,
    remove_preset_macros, set_macro, switch_presets, PresetSwitch,
};
use crate::loadouts::set::LoadoutSet;
use crate::profile::live::{Macro, Profile, Timer};
use crate::script::{list_groups, set_list_group, GroupList};

#[tauri::command]
pub(crate) async fn triggers_list(
    state: State<'_, SharedState>,
    profile: Option<String>,
) -> Result<Vec<Trigger>, String> {
    let p = state.lock_named(profile).await?;
    Ok(p.triggers.list())
}

#[tauri::command]
pub(crate) async fn triggers_export(
    state: State<'_, SharedState>,
    profile: Option<String>,
) -> Result<String, String> {
    let p = state.lock_named(profile).await?;
    p.triggers.export_json().map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn triggers_import<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    json: String,
    profile: Option<String>,
) -> Result<usize, String> {
    let (open, count) = {
        let mut p = state.lock_named(profile).await?;
        let count = p.triggers.import_json(&json).map_err(|e| e.to_string())?;
        (p.open().clone(), count)
    };
    // The editor's save path lands here: persist, or the "saved" state
    // lives only in memory and vanishes on restart.
    let shared: SharedState = state.inner().clone();
    persist_profile(&shared, &open).await;
    broadcast_list_changes(&app, &open, ListChanges::TRIGGERS);
    Ok(count)
}

/// Dump every alias to a pretty JSON array. Mirrors `triggers_export`,
/// so the Aliases and Triggers editors in Settings load their lists the
/// same way, through automationRecords.ts and automationTriggers.ts.
#[tauri::command]
pub(crate) async fn aliases_export(
    state: State<'_, SharedState>,
    profile: Option<String>,
) -> Result<String, String> {
    let p = state.lock_named(profile).await?;
    aliases_json(&p.aliases)
}

/// The JSON `aliases_export` sends, sorted by name. The page reads it in
/// the palette and the Aliases editor, and its tests read
/// `fixtures/ipc/aliases_export.json`, which a test here holds to it.
fn aliases_json(store: &vosh_automation::alias::AliasStore) -> Result<String, String> {
    let aliases: Vec<vosh_automation::alias::Alias> = store.list().into_iter().cloned().collect();
    serde_json::to_string_pretty(&aliases).map_err(|e| e.to_string())
}

/// Replace the entire alias store with the JSON-decoded list. Returns
/// the count installed. Invalid JSON or wrong shape rejects without
/// touching the store.
#[tauri::command]
pub(crate) async fn aliases_import(
    app: AppHandle,
    state: State<'_, SharedState>,
    json: String,
    profile: Option<String>,
) -> Result<usize, String> {
    let parsed: Vec<vosh_automation::alias::Alias> =
        serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let count = parsed.len();
    let open = {
        let mut p = state.lock_named(profile).await?;
        p.aliases = imported_aliases(parsed, &p.aliases);
        p.open().clone()
    };
    // Same persistence rule as triggers_import: the editor saves here.
    let shared: SharedState = state.inner().clone();
    persist_profile(&shared, &open).await;
    broadcast_list_changes(&app, &open, ListChanges::ALIASES);
    Ok(count)
}

/// The store a Settings save of the alias list leaves, built from
/// `parsed` over `old`. The disabled-groups set is user state about
/// groups, not items, so it carries over, or every save would turn each
/// disabled group back on. A stopped alias stays off, under each key,
/// unless this save changed it, as the trigger import keeps its stops.
fn imported_aliases(
    parsed: Vec<vosh_automation::alias::Alias>,
    old: &vosh_automation::alias::AliasStore,
) -> vosh_automation::alias::AliasStore {
    let mut store = vosh_automation::alias::AliasStore::new();
    for alias in parsed {
        store.set(alias);
    }
    store.set_disabled_groups(old.disabled_groups());
    store.keep_stops_from(old);
    store
}

/// Snapshot of every keyboard macro binding. Used by the Settings
/// macros tab to render the existing list and by useMacroKeys.ts (via the
/// same payload) to seed its in-memory binding lookup before any
/// `vosh://macros-changed` event fires.
#[tauri::command]
pub(crate) async fn macros_list(
    state: State<'_, SharedState>,
    profile: Option<String>,
) -> Result<Vec<Macro>, String> {
    let p = state.lock_named(profile).await?;
    Ok(p.macros.clone())
}

/// Set or replace a binding of yours by key. Empty `command` is
/// rejected; callers that want to unbind should use `macros_delete`.
/// Re-binding a key of yours overwrites the prior command. `enabled`
/// turns the binding on or off without unbinding it. Absent keeps an
/// existing binding's state and makes a new binding on. With `preset`,
/// the call changes only the group of that preset's macro on `key`.
#[tauri::command]
pub(crate) async fn macros_set(
    app: AppHandle,
    state: State<'_, SharedState>,
    key: String,
    command: String,
    group: Option<String>,
    enabled: Option<bool>,
    preset: Option<String>,
    profile: Option<String>,
) -> Result<Vec<Macro>, String> {
    let (open, updated) = {
        let mut p = state.lock_named(profile).await?;
        set_macro(&mut p, &key, &command, group, enabled, preset.as_deref())?;
        (p.open().clone(), p.macros.clone())
    };
    save_then_broadcast(
        &app,
        &state,
        &open,
        SavePolicy::Now,
        MACROS_CHANGED,
        &updated,
    )
    .await;
    Ok(updated)
}

/// Remove your binding on `key`. No-op when you have none there.
#[tauri::command]
pub(crate) async fn macros_delete(
    app: AppHandle,
    state: State<'_, SharedState>,
    key: String,
    profile: Option<String>,
) -> Result<Vec<Macro>, String> {
    let (open, updated) = {
        let mut p = state.lock_named(profile).await?;
        delete_macro(&mut p, &key);
        (p.open().clone(), p.macros.clone())
    };
    save_then_broadcast(
        &app,
        &state,
        &open,
        SavePolicy::Now,
        MACROS_CHANGED,
        &updated,
    )
    .await;
    Ok(updated)
}

/// One entry in the macro groups list: name + whether the group is
/// currently enabled. The command line reads it to know which macro
/// keys fire.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct GroupState {
    pub name: String,
    pub enabled: bool,
}

#[tauri::command]
pub(crate) async fn macros_groups_list(
    state: State<'_, SharedState>,
    profile: Option<String>,
) -> Result<Vec<GroupState>, String> {
    let p = state.lock_named(profile).await?;
    let mut names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for m in &p.macros {
        if let Some(g) = &m.group {
            if !g.is_empty() {
                names.insert(g.clone());
            }
        }
    }
    Ok(names
        .into_iter()
        .map(|n| {
            let enabled = !p.disabled_macro_groups.contains(&n);
            GroupState { name: n, enabled }
        })
        .collect())
}

/// One group heading's switch, as Settings draws it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct GroupSwitchState {
    pub name: String,
    /// Whether the group is on now.
    pub enabled: bool,
    /// Set while the loadouts decide the group, so its note names the
    /// loadouts that decide. The switch still turns the group, and the
    /// next launch, profile switch or Loadouts save puts their state back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loadouts: Option<LoadoutHold>,
}

/// The switch of each group of `list`, sorted by name. `set` is loadout
/// mode's loadout set, None in per profile mode.
fn group_switches(p: &Profile, set: Option<&LoadoutSet>, list: GroupList) -> Vec<GroupSwitchState> {
    list_groups(p, list)
        .into_iter()
        .map(|(name, enabled)| {
            let loadouts = set
                .filter(|_| list.in_catalog())
                .and_then(|set| loadout_hold(set, &name));
            GroupSwitchState {
                name,
                enabled,
                loadouts,
            }
        })
        .collect()
}

/// Turn the group `group` of `list` on or off by its own name, as `#group`
/// turns each group it finds. The per list off lists are where the switch
/// lasts in both modes. A group the loadouts decide turns too, as `#group`
/// and Lua turn it, and the next launch, profile switch or Loadouts save
/// puts the loadouts state back.
fn switch_group(
    p: &mut Profile,
    list: GroupList,
    group: &str,
    enabled: bool,
) -> Result<(), String> {
    let group = group.trim();
    if !list_groups(p, list).iter().any(|(g, _)| g == group) {
        return Err(format!("Vosh has no group named “{group}” there now."));
    }
    set_list_group(p, list, group, enabled);
    Ok(())
}

/// The switch on each group heading of one Automation list.
#[tauri::command]
pub(crate) async fn groups_list(
    state: State<'_, SharedState>,
    list: GroupList,
    profile: Option<String>,
) -> Result<Vec<GroupSwitchState>, String> {
    let edited = state.edited_profile(profile)?;
    let set = state.loadout_set.lock().await;
    let p = edited.lock().await;
    let set = set.as_ref().map(|set| set.for_profile(p.name.as_deref()));
    Ok(group_switches(&p, set.as_deref(), list))
}

/// Turn a whole group of one list on or off, from the switch on its
/// heading in Settings, and answer every switch of that list. Saves the
/// profile, and every window hears that the group turned.
#[tauri::command]
pub(crate) async fn groups_set_enabled<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    list: GroupList,
    group: String,
    enabled: bool,
    profile: Option<String>,
) -> Result<Vec<GroupSwitchState>, String> {
    let edited = state.edited_profile(profile)?;
    let (open, switches, lists) = {
        let set = state.loadout_set.lock().await;
        let mut p = edited.lock().await;
        let set = set.as_ref().map(|set| set.for_profile(p.name.as_deref()));
        let before = ListRevisions::of_lists(&p);
        switch_group(&mut p, list, &group, enabled)?;
        (
            p.open().clone(),
            group_switches(&p, set.as_deref(), list),
            ListChanges::between(before, ListRevisions::of_lists(&p)),
        )
    };
    if lists.groups {
        let shared: SharedState = state.inner().clone();
        persist_profile(&shared, &open).await;
    }
    broadcast_list_changes(&app, &open, lists);
    Ok(switches)
}

/// List every interval timer, in stored order.
#[tauri::command]
pub(crate) async fn timers_list(
    state: State<'_, SharedState>,
    profile: Option<String>,
) -> Result<Vec<Timer>, String> {
    let p = state.lock_named(profile).await?;
    Ok(p.timers.clone())
}

/// Create or update an interval timer. A `None` id creates a new timer
/// (assigned the next free id); an existing id updates in place. The
/// interval is clamped to at least one second, and a blank group means
/// none. Returns the full list.
#[tauri::command]
pub(crate) async fn timers_set(
    app: AppHandle,
    state: State<'_, SharedState>,
    id: Option<u32>,
    name: String,
    interval_secs: u32,
    command: String,
    enabled: bool,
    group: Option<String>,
    profile: Option<String>,
) -> Result<Vec<Timer>, String> {
    let (open, updated) = {
        let mut p = state.lock_named(profile).await?;
        set_timer(&mut p, id, name, interval_secs, command, enabled, group)?;
        (p.open().clone(), p.timers.clone())
    };
    save_then_broadcast(
        &app,
        &state,
        &open,
        SavePolicy::Now,
        TIMERS_CHANGED,
        &updated,
    )
    .await;
    Ok(updated)
}

/// The part of [`timers_set`] that runs under the profile lock.
fn set_timer(
    p: &mut Profile,
    id: Option<u32>,
    name: String,
    interval_secs: u32,
    command: String,
    enabled: bool,
    group: Option<String>,
) -> Result<(), String> {
    let name = name.trim().to_string();
    let command = command.trim().to_string();
    if command.is_empty() {
        return Err("command cannot be empty".into());
    }
    let interval_secs = interval_secs.max(1);
    let group = group
        .map(|g| g.trim().to_string())
        .filter(|g| !g.is_empty());
    match id.and_then(|wanted| p.timers.iter_mut().find(|t| t.id == wanted)) {
        Some(existing) => {
            existing.name = name;
            existing.interval_secs = interval_secs;
            existing.command = command;
            existing.enabled = enabled;
            existing.group = group;
        }
        None => {
            let next_id = p.timers.iter().map(|t| t.id).max().unwrap_or(0) + 1;
            p.timers.push(Timer {
                id: next_id,
                name,
                interval_secs,
                command,
                enabled,
                group,
            });
        }
    }
    Ok(())
}

/// Remove a timer by id. No-op when the id is not present.
#[tauri::command]
pub(crate) async fn timers_delete(
    app: AppHandle,
    state: State<'_, SharedState>,
    id: u32,
    profile: Option<String>,
) -> Result<Vec<Timer>, String> {
    let (open, updated) = {
        let mut p = state.lock_named(profile).await?;
        p.timers.retain(|t| t.id != id);
        (p.open().clone(), p.timers.clone())
    };
    save_then_broadcast(
        &app,
        &state,
        &open,
        SavePolicy::Now,
        TIMERS_CHANGED,
        &updated,
    )
    .await;
    Ok(updated)
}

/// What a preset install did: the number of triggers and macros it
/// installed, and the names of the stored triggers of those presets it
/// took out because the presets no longer build them.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct PresetsInstalled {
    pub(crate) installed: usize,
    pub(crate) removed: Vec<String>,
}

/// Install the triggers and macros of the presets that are on. Each one
/// should already have its `preset` field set to the preset id, and each
/// preset comes whole, so a stored trigger of it the set does not name
/// comes out. This command validates and inserts them so the engine
/// starts matching and the keys start sending at once.
#[tauri::command]
pub(crate) async fn presets_install(
    app: AppHandle,
    state: State<'_, SharedState>,
    triggers: Vec<Trigger>,
    macros: Vec<Macro>,
    profile: Option<String>,
) -> Result<PresetsInstalled, String> {
    let (triggers_came, macros_came) = (!triggers.is_empty(), !macros.is_empty());
    let (open, done, macros) = {
        let mut p = state.lock_named(profile).await?;
        let installed = triggers.len() + macros.len();
        // The macros go first, since they refuse before they change
        // anything.
        install_preset_macros(&mut p, macros)?;
        let removed = install_preset_triggers(&mut p, triggers)?;
        let macros = macros_came.then(|| p.macros.clone());
        let done = PresetsInstalled { installed, removed };
        (p.open().clone(), done, macros)
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&shared, &open).await;
    if triggers_came {
        broadcast_list_changes(&app, &open, ListChanges::TRIGGERS);
    }
    if let Some(macros) = macros.filter(|_| state.in_front(&open)) {
        broadcast(&app, MACROS_CHANGED, &macros);
    }
    Ok(done)
}

/// Turn presets on and off in one step, for First Run's Get started and
/// the Presets page (First Run Q17, Presets Q10). Each preset `changes`
/// turns off loses its triggers and macros, and `triggers` and `macros`,
/// which the page built for the presets it turns on, install as
/// [`presets_install`] installs them. Then the switches land on the
/// `enabled_presets` list as it stands now, see [`switch_presets`], and
/// nothing else of the page's settings is written. Saves once and tells
/// every window.
#[tauri::command]
pub(crate) async fn presets_enabled_set<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    changes: Vec<PresetSwitch>,
    triggers: Vec<Trigger>,
    macros: Vec<Macro>,
    profile: Option<String>,
) -> Result<PresetsInstalled, String> {
    let (open, done, lists, macros) = {
        let mut p = state.lock_named(profile).await?;
        let lists_before = ListRevisions::of_lists(&p);
        let macros_before = p.macros.clone();
        let installed = triggers.len() + macros.len();
        install_preset_macros(&mut p, macros)?;
        for off in changes.iter().filter(|c| !c.on) {
            p.triggers.remove_by_preset(&off.id);
            remove_preset_macros(&mut p, &off.id);
        }
        let removed = install_preset_triggers(&mut p, triggers)?;
        p.ui.enabled_presets = switch_presets(&p.ui.enabled_presets, &changes);
        let lists = ListChanges::between(lists_before, ListRevisions::of_lists(&p));
        let macros = (p.macros != macros_before).then(|| p.macros.clone());
        let done = PresetsInstalled { installed, removed };
        (p.open().clone(), done, lists, macros)
    };
    persist_profile(state.inner(), &open).await;
    broadcast_list_changes(&app, &open, lists);
    if let Some(macros) = macros.filter(|_| state.in_front(&open)) {
        broadcast(&app, MACROS_CHANGED, &macros);
    }
    let profile = open.name();
    broadcast(&app, PRESETS_CHANGED, &PresetsChanged { profile });
    Ok(done)
}

/// Remove every trigger and macro tagged with the given preset id.
/// Returns the number removed.
#[tauri::command]
pub(crate) async fn presets_remove(
    app: AppHandle,
    state: State<'_, SharedState>,
    preset_id: String,
    profile: Option<String>,
) -> Result<usize, String> {
    let (open, triggers_removed, macros_removed, macros) = {
        let mut p = state.lock_named(profile).await?;
        let triggers_removed = p.triggers.remove_by_preset(&preset_id);
        let macros_removed = remove_preset_macros(&mut p, &preset_id);
        let macros = p.macros.clone();
        (p.open().clone(), triggers_removed, macros_removed, macros)
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&shared, &open).await;
    if triggers_removed > 0 {
        broadcast_list_changes(&app, &open, ListChanges::TRIGGERS);
    }
    if macros_removed > 0 && state.in_front(&open) {
        broadcast(&app, MACROS_CHANGED, &macros);
    }
    Ok(triggers_removed + macros_removed)
}

/// Detect which import format a file uses, based on content sniffing.
/// Frontend extension-checks first; this is the fallback. Returns
/// `null` when nothing recognized so the UI can ask the user.
#[tauri::command]
pub(crate) async fn import_detect(text: String) -> Result<Option<ImportFormat>, String> {
    Ok(crate::import::detect_format(&text))
}

#[derive(serde::Serialize)]
pub(crate) struct ImportSummary {
    pub aliases: usize,
    pub triggers: usize,
    pub macros: usize,
    pub vars: usize,
    pub unsupported: Vec<(String, String)>,
    pub unparsed: Vec<String>,
    pub rejected: Vec<String>,
    /// The triggers that take the name of a preset trigger, which stay
    /// out so the preset's keeps running.
    pub clashes: Vec<crate::import::vosh::Clash>,
}

/// Parse + apply an import file to the live profile. The format
/// string is an [`ImportFormat`] name such as `mudlet`; pass an
/// empty string to auto-detect. Aliases / triggers / macros / vars
/// merge into the existing stores (overwrite on name collision). A
/// trigger that takes a name of `preset_triggers`, the trigger names
/// of the page's preset library, joins the clash list instead, see
/// [`merge_triggers`]. Returns a summary so the UI can report what
/// landed and what did not.
#[tauri::command]
pub(crate) async fn import_apply<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    format: String,
    text: String,
    preset_triggers: Vec<String>,
    profile: Option<String>,
) -> Result<ImportSummary, String> {
    let fmt = if format.is_empty() {
        crate::import::detect_format(&text)
            .ok_or_else(|| "could not detect import format".to_string())?
    } else {
        ImportFormat::deserialize(StrDeserializer::<value::Error>::new(&format))
            .map_err(|_| format!("unknown import format: {format}"))?
    };
    let report = crate::import::parse(fmt, &text);
    let macros_changed = !report.macros.is_empty();
    let macros_snapshot: Vec<Macro>;
    let lists;
    let merged;
    let open = {
        let mut p = state.lock_named(profile).await?;
        let lists_before = ListRevisions::of_lists(&p);
        for alias in &report.aliases {
            p.aliases.set(alias.clone());
        }
        merged = merge_triggers(&mut p.triggers, &report.triggers, &preset_triggers);
        if macros_changed {
            import_macros(&mut p, &report.macros);
        }
        for (k, v) in &report.vars {
            p.vars.set(k.clone(), v.clone());
        }
        macros_snapshot = p.macros.clone();
        lists = ListChanges::between(lists_before, ListRevisions::of_lists(&p));
        p.open().clone()
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&shared, &open).await;
    if macros_changed && state.in_front(&open) {
        broadcast(&app, MACROS_CHANGED, &macros_snapshot);
    }
    broadcast_list_changes(&app, &open, lists);
    Ok(ImportSummary {
        aliases: report.aliases.len(),
        triggers: report.triggers.len() - merged.rejected.len() - merged.clashes.len(),
        macros: report.macros.len(),
        vars: report.vars.len(),
        unsupported: report.unsupported,
        unparsed: report.unparsed,
        rejected: merged.rejected,
        clashes: merged.clashes,
    })
}

#[cfg(test)]
mod tests {
    use super::{group_switches, switch_group, GroupSwitchState};
    use crate::loadouts::gating::LoadoutHold;
    use crate::loadouts::set::{Loadout, LoadoutSet};
    use crate::profile::live::{Macro, Profile, Timer};
    use crate::script::GroupList;

    /// A profile with a combat group in every list and a loot group of
    /// triggers.
    fn grouped() -> Profile {
        use vosh_automation::alias::Alias;
        use vosh_automation::trigger::{Trigger, TriggerAction};
        let mut p = Profile::default();
        for (name, group) in [("flee", "combat"), ("loot", "loot")] {
            p.triggers
                .set(Trigger {
                    group: Some(group.into()),
                    ..Trigger::new(name, "^x$", TriggerAction::Gag)
                })
                .unwrap();
        }
        let mut kick = Alias::new("kk", "kick");
        kick.group = Some("combat".into());
        p.aliases.set(kick);
        p.macros.push(Macro {
            key: "F1".into(),
            command: "bash".into(),
            group: Some("combat".into()),
            enabled: true,
            preset: None,
        });
        p.timers.push(Timer {
            id: 1,
            name: String::new(),
            interval_secs: 30,
            command: "rescue".into(),
            enabled: true,
            group: Some("combat".into()),
        });
        p
    }

    fn switch(name: &str, enabled: bool) -> GroupSwitchState {
        GroupSwitchState {
            name: name.into(),
            enabled,
            loadouts: None,
        }
    }

    #[test]
    fn an_alias_list_save_keeps_the_stops_of_each_alias_it_leaves_alone() {
        use super::imported_aliases;
        use vosh_automation::alias::{Alias, AliasStore};
        use vosh_automation::StopKey;
        let (one, two) = (StopKey(1), StopKey(2));
        let mut old = AliasStore::new();
        old.set(Alias::new("hl", "cast heal"));
        old.set(Alias::new("kk", "kick"));
        old.stop("hl", one);
        old.stop("kk", two);
        let saved = vec![Alias::new("hl", "cast heal"), Alias::new("kk", "kick %1")];
        let store = imported_aliases(saved, &old);
        assert!(store.is_stopped("hl", one));
        assert!(!store.is_stopped("hl", two));
        assert!(!store.is_stopped("kk", two));
    }

    #[test]
    fn each_list_answers_a_switch_for_each_of_its_groups() {
        let p = grouped();
        assert_eq!(
            group_switches(&p, None, GroupList::Triggers),
            [switch("combat", true), switch("loot", true)]
        );
        for list in [GroupList::Aliases, GroupList::Macros, GroupList::Timers] {
            assert_eq!(group_switches(&p, None, list), [switch("combat", true)]);
        }
    }

    #[test]
    fn a_switch_turns_one_list_and_agrees_with_group() {
        let mut p = grouped();
        for list in GroupList::ALL {
            switch_group(&mut p, list, "combat", false).unwrap();
            assert_eq!(
                group_switches(&p, None, list)[0],
                switch("combat", false),
                "{list:?}"
            );
        }
        // Each list turned alone, and #group reads what the switches set.
        let r = crate::input::process(&mut p, "#group combat");
        assert_eq!(
            r.echo[1..],
            [
                "  triggers: off",
                "  aliases : off",
                "  macros  : off",
                "  timers  : off"
            ]
        );
        assert!(!p.timer_fires(&p.timers[0]));
        // And a switch reads what #group set.
        crate::input::process(&mut p, "#group combat on");
        for list in GroupList::ALL {
            assert_eq!(group_switches(&p, None, list)[0], switch("combat", true));
        }
        // The trigger list's loot group never moved.
        assert!(p.triggers.is_group_enabled("loot"));
    }

    #[test]
    fn a_switch_keeps_off_through_a_save_of_the_list() {
        let mut p = grouped();
        switch_group(&mut p, GroupList::Triggers, "loot", false).unwrap();
        let json = p.triggers.export_json().unwrap();
        p.triggers.import_json(&json).unwrap();
        assert_eq!(
            group_switches(&p, None, GroupList::Triggers)[1],
            switch("loot", false)
        );
        let text = crate::profile::file::ProfileConfig::from_profile(&p)
            .to_toml()
            .unwrap();
        assert!(
            text.contains("disabled_trigger_groups = [\"loot\"]"),
            "{text}"
        );
    }

    #[test]
    fn a_switch_refuses_a_group_no_item_is_in() {
        let mut p = grouped();
        let err = switch_group(&mut p, GroupList::Aliases, "loot", false).unwrap_err();
        assert_eq!(err, "Vosh has no group named “loot” there now.");
        let leftover = &p.aliases.disabled_groups();
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    /// Loadout mode with Healer on, which lists the combat group.
    fn healer_on(dormant: bool) -> LoadoutSet {
        let mut healer = Loadout::empty("Healer");
        healer.enabled_groups = vec!["combat".into()];
        LoadoutSet {
            active: if dormant {
                Vec::new()
            } else {
                vec!["Healer".into()]
            },
            dormant,
            loadouts: vec![healer],
            ..Default::default()
        }
    }

    #[test]
    fn a_switch_turns_a_group_the_loadouts_decide_until_they_turn_it_back() {
        use crate::loadouts::gating::apply_effective_state;
        let held = |on, by: &[&str]| {
            Some(LoadoutHold {
                on,
                by: by.iter().map(|n| (*n).to_string()).collect(),
            })
        };
        let mut p = grouped();
        let set = healer_on(false);
        apply_effective_state(&set, &mut p);
        let switches = group_switches(&p, Some(&set), GroupList::Triggers);
        assert_eq!(switches[0].loadouts, held(true, &["Healer"]));
        assert_eq!(switches[1].loadouts, held(false, &["Healer"]));
        assert!(!switches[1].enabled);
        // The switch turns loot on under Healer, and the note stays.
        switch_group(&mut p, GroupList::Triggers, "loot", true).unwrap();
        let switches = group_switches(&p, Some(&set), GroupList::Triggers);
        assert!(switches[1].enabled);
        assert_eq!(switches[1].loadouts, held(false, &["Healer"]));
        // The next apply puts the loadouts state back.
        apply_effective_state(&set, &mut p);
        assert!(!group_switches(&p, Some(&set), GroupList::Triggers)[1].enabled);
        // Timers stay in the profile file, so no loadout decides them.
        let timers = group_switches(&p, Some(&set), GroupList::Timers);
        assert_eq!(timers, [switch("combat", true)]);
        switch_group(&mut p, GroupList::Timers, "combat", false).unwrap();
        assert!(!group_switches(&p, Some(&set), GroupList::Timers)[0].enabled);
    }

    #[test]
    fn a_switch_turns_an_alias_group_the_dormant_catalog_holds_off() {
        use crate::loadouts::gating::apply_effective_state;
        let held_off = Some(LoadoutHold {
            on: false,
            by: Vec::new(),
        });
        let mut p = grouped();
        let dormant = healer_on(true);
        apply_effective_state(&dormant, &mut p);
        let macros = group_switches(&p, Some(&dormant), GroupList::Macros);
        assert_eq!(macros[0].loadouts, held_off);
        assert!(!group_switches(&p, Some(&dormant), GroupList::Aliases)[0].enabled);
        switch_group(&mut p, GroupList::Aliases, "combat", true).unwrap();
        let switches = group_switches(&p, Some(&dormant), GroupList::Aliases);
        assert!(switches[0].enabled);
        assert_eq!(switches[0].loadouts, held_off);
        apply_effective_state(&dormant, &mut p);
        assert!(!group_switches(&p, Some(&dormant), GroupList::Aliases)[0].enabled);
    }

    #[test]
    fn on_a_plain_profile_a_switch_turns_triggers_aliases_and_timers() {
        let mut p = grouped();
        for list in [GroupList::Triggers, GroupList::Aliases, GroupList::Timers] {
            switch_group(&mut p, list, "combat", false).unwrap();
            assert_eq!(
                group_switches(&p, None, list)[0],
                switch("combat", false),
                "{list:?}"
            );
            switch_group(&mut p, list, "combat", true).unwrap();
            assert_eq!(
                group_switches(&p, None, list)[0],
                switch("combat", true),
                "{list:?}"
            );
        }
    }

    #[test]
    fn a_group_turned_by_group_shows_as_it_is_until_the_loadouts_turn_it_back() {
        use crate::loadouts::gating::apply_effective_state;
        let held_on = Some(LoadoutHold {
            on: true,
            by: vec!["Healer".into()],
        });
        let held_off = Some(LoadoutHold {
            on: false,
            by: Vec::new(),
        });
        // #group combat off while Healer holds combat on. The switch reads
        // the group as off, and the hold still says the loadouts turn it on.
        let mut p = grouped();
        let set = healer_on(false);
        apply_effective_state(&set, &mut p);
        crate::input::process(&mut p, "#group combat off");
        let switches = group_switches(&p, Some(&set), GroupList::Triggers);
        assert!(!switches[0].enabled);
        assert_eq!(switches[0].loadouts, held_on);
        // The next apply lays the loadouts over the group again.
        apply_effective_state(&set, &mut p);
        let switches = group_switches(&p, Some(&set), GroupList::Triggers);
        assert!(switches[0].enabled);
        assert_eq!(switches[0].loadouts, held_on);
        // #group combat on while the catalog is dormant, the other way.
        let dormant = healer_on(true);
        apply_effective_state(&dormant, &mut p);
        crate::input::process(&mut p, "#group combat on");
        let switches = group_switches(&p, Some(&dormant), GroupList::Aliases);
        assert!(switches[0].enabled);
        assert_eq!(switches[0].loadouts, held_off);
        apply_effective_state(&dormant, &mut p);
        assert!(!group_switches(&p, Some(&dormant), GroupList::Aliases)[0].enabled);
    }

    #[test]
    fn with_no_opinion_from_the_loadouts_the_switch_is_yours() {
        let mut p = grouped();
        let set = LoadoutSet {
            active: vec!["Quiet".into()],
            dormant: false,
            loadouts: vec![Loadout::empty("Quiet")],
            ..Default::default()
        };
        assert_eq!(
            group_switches(&p, Some(&set), GroupList::Aliases),
            [switch("combat", true)]
        );
        switch_group(&mut p, GroupList::Aliases, "combat", false).unwrap();
        assert!(!p.aliases.is_group_enabled("combat"));
    }

    #[test]
    fn the_page_reads_a_switch_and_its_hold_by_these_names() {
        let mut p = grouped();
        let set = healer_on(false);
        let sent = serde_json::to_value(group_switches(&p, Some(&set), GroupList::Macros)).unwrap();
        assert_eq!(
            sent,
            serde_json::json!([
                { "name": "combat", "enabled": true, "loadouts": { "on": true, "by": ["Healer"] } }
            ])
        );
        switch_group(&mut p, GroupList::Timers, "combat", false).unwrap();
        let sent = serde_json::to_value(group_switches(&p, None, GroupList::Timers)).unwrap();
        assert_eq!(
            sent,
            serde_json::json!([{ "name": "combat", "enabled": false }])
        );
        let list: GroupList = serde_json::from_value(serde_json::json!("timers")).unwrap();
        assert_eq!(list, GroupList::Timers);
    }

    #[test]
    fn a_switch_answers_every_switch_of_its_list() {
        use std::sync::Arc;

        use tauri::test::{mock_builder, mock_context, noop_assets};
        use tauri::Manager;

        use crate::app::state::{AppState, SharedState};

        let app = mock_builder().build(mock_context(noop_assets())).unwrap();
        app.manage::<SharedState>(Arc::new(AppState::default()));
        let state: SharedState = app.state::<SharedState>().inner().clone();
        tauri::async_runtime::block_on(async {
            *state.selected_profile().await = grouped();
            let answer = super::groups_set_enabled(
                app.handle().clone(),
                app.state::<SharedState>(),
                GroupList::Macros,
                " combat ".into(),
                false,
                None,
            )
            .await
            .unwrap();
            assert_eq!(answer, [switch("combat", false)]);
            let p = state.selected_profile().await;
            assert!(p.disabled_macro_groups.contains("combat"));
            drop(p);
            let listed = super::groups_list(app.state::<SharedState>(), GroupList::Macros, None)
                .await
                .unwrap();
            assert_eq!(listed, answer);
        });
    }

    #[test]
    fn a_timer_keeps_its_group_trimmed_and_a_blank_one_as_none() {
        let mut p = crate::profile::live::Profile::default();
        let set = |p: &mut _, id, group: &str| {
            super::set_timer(
                p,
                id,
                "drink".into(),
                60,
                "drink water".into(),
                true,
                Some(group.into()),
            )
        };
        set(&mut p, None, "  upkeep ").unwrap();
        assert_eq!(p.timers[0].group.as_deref(), Some("upkeep"));
        let id = p.timers[0].id;
        set(&mut p, Some(id), "  ").unwrap();
        assert_eq!(p.timers[0].group, None);
        assert_eq!(p.timers.len(), 1);
    }

    #[test]
    fn aliases_export_sends_the_shared_alias_fixture() {
        // The page tests read this file as the reply to aliases_export,
        // so renaming a field fails here instead of leaving the page to
        // read nothing.
        use vosh_automation::alias::{Alias, AliasStore};
        let mut store = AliasStore::new();
        store.set(Alias::new("rec", "recall"));
        store.set(Alias::new("k", "kill %1"));
        store.set(Alias::new("cs", "cast %1").with_group("magic"));
        store.set(Alias::new("lk", "kill %1").with_script("mud.send(\"look\")"));
        store.set(
            Alias::new("lt", "look")
                .with_script("mud.send(\"look \" .. captures[1])\nmud.echo(\"looked\")"),
        );
        let mut off = Alias::new("off", "say off");
        off.enabled = false;
        store.set(off);
        let json = super::aliases_json(&store).unwrap();
        // The file ends in a newline, and the reply does not.
        let fixture = include_str!("../../../fixtures/ipc/aliases_export.json");
        assert_eq!(
            fixture.strip_suffix('\n'),
            Some(json.as_str()),
            "fixtures/ipc/aliases_export.json no longer matches aliases_export"
        );
    }

    #[test]
    fn import_answers_the_format_names_and_errors_the_page_reads() {
        // The Import page sends back the name import_detect answers, and
        // importErrorMessage in automationRecords.ts turns the two
        // import_apply errors into sentences by their text.
        use std::sync::Arc;

        use tauri::test::{mock_builder, mock_context, noop_assets};
        use tauri::Manager;

        use crate::app::state::{AppState, SharedState};

        let gmud = "alias [g] [get $1.gold]\n";
        let samples = [
            (
                "mushclient",
                r#"<?xml version="1.0"?><muclient><world></world></muclient>"#,
            ),
            (
                "mudlet",
                r#"<?xml version="1.0"?><MudletPackage version="1.001"></MudletPackage>"#,
            ),
            ("gmud", gmud),
            (
                "cmud",
                "<?xml version=\"1.0\"?>\n<cmud>\n<window/>\n</cmud>\n",
            ),
        ];
        let app = mock_builder().build(mock_context(noop_assets())).unwrap();
        app.manage::<SharedState>(Arc::new(AppState::default()));
        tauri::async_runtime::block_on(async {
            for (name, text) in samples {
                let detected = super::import_detect(text.to_string()).await.unwrap();
                assert_eq!(
                    serde_json::to_value(detected).unwrap(),
                    serde_json::json!(name)
                );
            }
            // Both errors come back before the profile is touched, so no
            // save writes outside the test. A name it does not know fails
            // even when the file itself would be detected.
            let bogus = super::import_apply(
                app.handle().clone(),
                app.state::<SharedState>(),
                "bogus".to_string(),
                gmud.to_string(),
                Vec::new(),
                None,
            )
            .await;
            assert_eq!(bogus.err().as_deref(), Some("unknown import format: bogus"));
            let undetected = super::import_apply(
                app.handle().clone(),
                app.state::<SharedState>(),
                String::new(),
                "look\n".to_string(),
                Vec::new(),
                None,
            )
            .await;
            assert_eq!(
                undetected.err().as_deref(),
                Some("could not detect import format")
            );
        });
    }

    #[tokio::test]
    async fn a_switch_turns_presets_on_and_off_saves_the_list_alone_and_tells_every_window() {
        use std::sync::{Arc, Mutex};

        use tauri::test::{mock_builder, mock_context, noop_assets};
        use tauri::{Listener, Manager};
        use vosh_automation::trigger::{Trigger, TriggerAction};

        use super::{presets_enabled_set, PresetsInstalled};
        use crate::app::events::PRESETS_CHANGED;
        use crate::app::state::{AppState, SharedState};
        use crate::loadouts::presets::PresetSwitch;

        let app = mock_builder().build(mock_context(noop_assets())).unwrap();
        app.manage::<SharedState>(Arc::new(AppState::default()));
        let heard = Arc::new(Mutex::new(Vec::new()));
        let into = heard.clone();
        app.listen_any(PRESETS_CHANGED, move |event| {
            into.lock().unwrap().push(event.payload().to_string());
        });
        let trigger = |preset: &str, name: &str| Trigger {
            preset: Some(preset.into()),
            ..Trigger::new(name, "You quaff", TriggerAction::Gag)
        };
        let state = app.state::<SharedState>();
        {
            let mut p = state.selected_profile().await;
            p.ui.enabled_presets = vec![
                "sent_tells".into(),
                "later_preset".into(),
                "potion_labels".into(),
            ];
            p.ui.theme = "vellum".into();
            p.triggers
                .set(trigger("potion_labels", "potion.quaff"))
                .unwrap();
        }
        let switch = |id: &str, on| PresetSwitch { id: id.into(), on };
        let done = presets_enabled_set(
            app.handle().clone(),
            app.state(),
            vec![switch("potion_labels", false), switch("herb_labels", true)],
            vec![trigger("herb_labels", "herb.eat")],
            Vec::new(),
            None,
        )
        .await;
        assert_eq!(
            done,
            Ok(PresetsInstalled {
                installed: 1,
                removed: Vec::new()
            })
        );
        {
            let p = state.selected_profile().await;
            assert_eq!(
                p.ui.enabled_presets,
                ["sent_tells", "later_preset", "herb_labels"]
            );
            assert_eq!(p.ui.theme, "vellum", "the rest of the settings stay");
            let names: Vec<String> = p.triggers.list().into_iter().map(|t| t.name).collect();
            assert_eq!(names, ["herb.eat"]);
        }
        let off = ["sent_tells", "later_preset", "herb_labels"].map(|id| switch(id, false));
        let done = presets_enabled_set(
            app.handle().clone(),
            app.state(),
            off.to_vec(),
            Vec::new(),
            Vec::new(),
            None,
        )
        .await;
        assert_eq!(done.map(|d| d.installed), Ok(0));
        let p = state.selected_profile().await;
        assert_eq!(p.ui.enabled_presets, ["none"]);
        assert!(p.triggers.is_empty());
        assert_eq!(heard.lock().unwrap().len(), 2);
    }
}

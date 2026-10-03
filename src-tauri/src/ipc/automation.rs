//! The commands for your automation. Settings lists and saves your
//! triggers, aliases, macros and timers through them, adds and removes
//! the preset triggers, and imports another client's file into the live
//! profile. The command line reads your macros through them too.

use serde::de::value::{self, StrDeserializer};
use serde::Deserialize;
use tauri::{AppHandle, State};
use vosh_automation::trigger::Trigger;

use crate::app::events::{
    broadcast, broadcast_list_changes, ListChanges, ListRevisions, MACROS_CHANGED, TIMERS_CHANGED,
};
use crate::app::state::SharedState;
use crate::disk::save::{persist_profile, save_then_broadcast, SavePolicy};
use crate::import::ImportFormat;
use crate::profile::live::{Macro, Profile, Timer};

#[tauri::command]
pub(crate) async fn triggers_list(state: State<'_, SharedState>) -> Result<Vec<Trigger>, String> {
    let p = state.profile.lock().await;
    Ok(p.triggers.list())
}

#[tauri::command]
pub(crate) async fn triggers_export(state: State<'_, SharedState>) -> Result<String, String> {
    let p = state.profile.lock().await;
    p.triggers.export_json().map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn triggers_import(
    app: AppHandle,
    state: State<'_, SharedState>,
    json: String,
) -> Result<usize, String> {
    let count = {
        let mut p = state.profile.lock().await;
        p.triggers.import_json(&json).map_err(|e| e.to_string())?
    };
    // The editor's save path lands here: persist, or the "saved" state
    // lives only in memory and vanishes on restart.
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast_list_changes(&app, ListChanges::TRIGGERS);
    Ok(count)
}

/// Dump every alias to a pretty JSON array. Mirrors `triggers_export`,
/// so the Aliases and Triggers editors in Settings load their lists the
/// same way, through automationRecords.ts and automationTriggers.ts.
#[tauri::command]
pub(crate) async fn aliases_export(state: State<'_, SharedState>) -> Result<String, String> {
    let p = state.profile.lock().await;
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
) -> Result<usize, String> {
    let parsed: Vec<vosh_automation::alias::Alias> =
        serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let count = parsed.len();
    {
        let mut p = state.profile.lock().await;
        let mut store = vosh_automation::alias::AliasStore::new();
        for alias in parsed {
            store.set(alias);
        }
        // The disabled-groups set is user state about GROUPS, not items;
        // replacing the store without carrying it over silently
        // re-enabled every disabled group on each editor save.
        store.set_disabled_groups(p.aliases.disabled_groups());
        p.aliases = store;
    }
    // Same persistence rule as triggers_import: the editor saves here.
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast_list_changes(&app, ListChanges::ALIASES);
    Ok(count)
}

/// Snapshot of every keyboard macro binding. Used by the Settings
/// macros tab to render the existing list and by Input.tsx (via the
/// same payload) to seed its in-memory binding lookup before any
/// `vosh://macros-changed` event fires.
#[tauri::command]
pub(crate) async fn macros_list(state: State<'_, SharedState>) -> Result<Vec<Macro>, String> {
    let p = state.profile.lock().await;
    Ok(p.macros.clone())
}

/// Set or replace a binding by key. Empty `command` is rejected;
/// callers that want to unbind should use `macros_delete`.
/// Re-binding an existing key overwrites the prior command. `enabled`
/// turns the binding on or off without unbinding it. Absent keeps an
/// existing binding's state and makes a new binding on.
#[tauri::command]
pub(crate) async fn macros_set(
    app: AppHandle,
    state: State<'_, SharedState>,
    key: String,
    command: String,
    group: Option<String>,
    enabled: Option<bool>,
) -> Result<Vec<Macro>, String> {
    let key = key.trim().to_string();
    let command = command.trim().to_string();
    if key.is_empty() {
        return Err("key cannot be empty".into());
    }
    if command.is_empty() {
        return Err("command cannot be empty".into());
    }
    // Normalize the group: empty / whitespace-only -> None so the
    // wire format does not persist an empty group string.
    let group = group
        .map(|g| g.trim().to_string())
        .filter(|g| !g.is_empty());
    let updated = {
        let mut p = state.profile.lock().await;
        if let Some(existing) = p.macros.iter_mut().find(|m| m.key == key) {
            existing.command = command;
            existing.group = group;
            if let Some(enabled) = enabled {
                existing.enabled = enabled;
            }
        } else {
            p.macros.push(Macro {
                key,
                command,
                group,
                enabled: enabled.unwrap_or(true),
            });
        }
        p.macros.clone()
    };
    save_then_broadcast(&app, &state, SavePolicy::Now, MACROS_CHANGED, &updated).await;
    Ok(updated)
}

/// Remove a binding by key. No-op when the key is not bound.
#[tauri::command]
pub(crate) async fn macros_delete(
    app: AppHandle,
    state: State<'_, SharedState>,
    key: String,
) -> Result<Vec<Macro>, String> {
    let updated = {
        let mut p = state.profile.lock().await;
        p.macros.retain(|m| m.key != key);
        p.macros.clone()
    };
    save_then_broadcast(&app, &state, SavePolicy::Now, MACROS_CHANGED, &updated).await;
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
) -> Result<Vec<GroupState>, String> {
    let p = state.profile.lock().await;
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

/// List every interval timer, in stored order.
#[tauri::command]
pub(crate) async fn timers_list(state: State<'_, SharedState>) -> Result<Vec<Timer>, String> {
    let p = state.profile.lock().await;
    Ok(p.timers.clone())
}

/// Create or update an interval timer. A `None` id creates a new timer
/// (assigned the next free id); an existing id updates in place. The
/// interval is clamped to at least one second. Returns the full list.
#[tauri::command]
pub(crate) async fn timers_set(
    app: AppHandle,
    state: State<'_, SharedState>,
    id: Option<u32>,
    name: String,
    interval_secs: u32,
    command: String,
    enabled: bool,
) -> Result<Vec<Timer>, String> {
    let name = name.trim().to_string();
    let command = command.trim().to_string();
    if command.is_empty() {
        return Err("command cannot be empty".into());
    }
    let interval_secs = interval_secs.max(1);
    let updated = {
        let mut p = state.profile.lock().await;
        match id.and_then(|wanted| p.timers.iter_mut().find(|t| t.id == wanted)) {
            Some(existing) => {
                existing.name = name;
                existing.interval_secs = interval_secs;
                existing.command = command;
                existing.enabled = enabled;
            }
            None => {
                let next_id = p.timers.iter().map(|t| t.id).max().unwrap_or(0) + 1;
                p.timers.push(Timer {
                    id: next_id,
                    name,
                    interval_secs,
                    command,
                    enabled,
                });
            }
        }
        p.timers.clone()
    };
    save_then_broadcast(&app, &state, SavePolicy::Now, TIMERS_CHANGED, &updated).await;
    Ok(updated)
}

/// Remove a timer by id. No-op when the id is not present.
#[tauri::command]
pub(crate) async fn timers_delete(
    app: AppHandle,
    state: State<'_, SharedState>,
    id: u32,
) -> Result<Vec<Timer>, String> {
    let updated = {
        let mut p = state.profile.lock().await;
        p.timers.retain(|t| t.id != id);
        p.timers.clone()
    };
    save_then_broadcast(&app, &state, SavePolicy::Now, TIMERS_CHANGED, &updated).await;
    Ok(updated)
}

/// Bulk-install a set of preset triggers. Each trigger should already
/// have its `preset` field set to the preset id; this command
/// validates and inserts them so the engine starts matching
/// immediately. Returns the number installed.
#[tauri::command]
pub(crate) async fn presets_install(
    app: AppHandle,
    state: State<'_, SharedState>,
    triggers: Vec<Trigger>,
) -> Result<usize, String> {
    let installed = {
        let mut p = state.profile.lock().await;
        install_preset_triggers(&mut p, triggers)?
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    if installed > 0 {
        broadcast_list_changes(&app, ListChanges::TRIGGERS);
    }
    Ok(installed)
}

/// The body of [`presets_install`] over the live profile `p`, so a test
/// can run the preset install launch runs. Returns the number installed.
pub(crate) fn install_preset_triggers(
    p: &mut Profile,
    triggers: Vec<Trigger>,
) -> Result<usize, String> {
    let mut installed = 0usize;
    for mut t in triggers {
        // The startup re-install overwrites same-named presets so
        // pattern/template updates land, but the group is the user's
        // organization: carry it over so putting a preset into a group
        // survives relaunch.
        if t.group.is_none() {
            if let Some(existing) = p.triggers.get(&t.name) {
                t.group.clone_from(&existing.group);
            }
        }
        p.triggers.set(t).map_err(|e| e.to_string())?;
        installed += 1;
    }
    Ok(installed)
}

/// Remove every trigger tagged with the given preset id. Returns the
/// number removed.
#[tauri::command]
pub(crate) async fn presets_remove(
    app: AppHandle,
    state: State<'_, SharedState>,
    preset_id: String,
) -> Result<usize, String> {
    let removed = {
        let mut p = state.profile.lock().await;
        p.triggers.remove_by_preset(&preset_id)
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    if removed > 0 {
        broadcast_list_changes(&app, ListChanges::TRIGGERS);
    }
    Ok(removed)
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
}

/// Parse + apply an import file to the live profile. The format
/// string is an [`ImportFormat`] name such as `mudlet`; pass an
/// empty string to auto-detect. Aliases / triggers / macros / vars
/// merge into the existing stores (overwrite on name collision).
/// Returns a summary so the UI can report what landed and what
/// did not.
#[tauri::command]
pub(crate) async fn import_apply<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    format: String,
    text: String,
) -> Result<ImportSummary, String> {
    let fmt = if format.is_empty() {
        crate::import::detect_format(&text)
            .ok_or_else(|| "could not detect import format".to_string())?
    } else {
        ImportFormat::deserialize(StrDeserializer::<value::Error>::new(&format))
            .map_err(|_| format!("unknown import format: {format}"))?
    };
    let report = crate::import::parse(fmt, &text);
    let mut rejected: Vec<String> = Vec::new();
    let mut macros_changed = false;
    let macros_snapshot: Vec<Macro>;
    let lists;
    {
        let mut p = state.profile.lock().await;
        let lists_before = ListRevisions::of(&p);
        for alias in &report.aliases {
            p.aliases.set(alias.clone());
        }
        for trigger in &report.triggers {
            if let Err(e) = p.triggers.set(trigger.clone()) {
                rejected.push(format!("trigger `{}` rejected: {e}", trigger.name));
            }
        }
        for m in &report.macros {
            if let Some(existing) = p.macros.iter_mut().find(|x| x.key == m.key) {
                existing.command.clone_from(&m.command);
            } else {
                p.macros.push(m.clone());
            }
            macros_changed = true;
        }
        for (k, v) in &report.vars {
            p.vars
                .set(vosh_automation::vars::Scope::Profile, k.clone(), v.clone());
        }
        macros_snapshot = p.macros.clone();
        lists = ListChanges::since(lists_before, &p);
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    if macros_changed {
        broadcast(&app, MACROS_CHANGED, &macros_snapshot);
    }
    broadcast_list_changes(&app, lists);
    Ok(ImportSummary {
        aliases: report.aliases.len(),
        triggers: report.triggers.len() - rejected.len(),
        macros: report.macros.len(),
        vars: report.vars.len(),
        unsupported: report.unsupported,
        unparsed: report.unparsed,
        rejected,
    })
}

#[cfg(test)]
mod tests {
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
            )
            .await;
            assert_eq!(bogus.err().as_deref(), Some("unknown import format: bogus"));
            let undetected = super::import_apply(
                app.handle().clone(),
                app.state::<SharedState>(),
                String::new(),
                "look\n".to_string(),
            )
            .await;
            assert_eq!(
                undetected.err().as_deref(),
                Some("could not detect import format")
            );
        });
    }
}

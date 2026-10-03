//! The commands for loadout mode. Settings reads your loadouts and which
//! of them are on, and the Loadouts editor turns them on and off.

use tauri::{AppHandle, Emitter, Manager, State};
use tracing::warn;

use crate::app::events::{broadcast, LOADOUTS_CHANGED, MACRO_GROUPS_CHANGED};
use crate::app::state::SharedState;
use crate::disk::save::mark_profile_dirty;

/// One loadout as the frontend cares about it: the user-visible
/// identifying fields, the `enabled_groups` list (chips for the picker),
/// and the auto-match block.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct LoadoutSummary {
    pub name: String,
    pub description: Option<String>,
    pub enabled_groups: Vec<String>,
    pub auto_match: Option<crate::profile::login_match::AutoMatch>,
}

/// Shape returned by [`loadouts_get_state`]. Carries the active list,
/// the full loadout summaries, and a `path_b_active` flag so the
/// frontend can decide whether to render the Loadouts tab at all.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct LoadoutsState {
    pub path_b_active: bool,
    pub active: Vec<String>,
    pub loadouts: Vec<LoadoutSummary>,
}

/// Snapshot the current Path B loadout state for the Settings UI.
/// In legacy mode returns `path_b_active: false` plus empty lists so
/// the frontend can hide the Loadouts tab. In Path B mode the
/// active list and every loadout's summary come from the
/// `state.loadout_set` mutex.
#[tauri::command]
pub(crate) async fn loadouts_get_state(
    state: State<'_, SharedState>,
) -> Result<LoadoutsState, String> {
    let guard = state.loadout_set.lock().await;
    let Some(set) = guard.as_ref() else {
        return Ok(LoadoutsState {
            path_b_active: false,
            active: Vec::new(),
            loadouts: Vec::new(),
        });
    };
    let summaries: Vec<LoadoutSummary> = set
        .loadouts
        .iter()
        .map(|l| LoadoutSummary {
            name: l.name.clone(),
            description: l.description.clone(),
            enabled_groups: l.enabled_groups.clone(),
            auto_match: l.auto_match.clone(),
        })
        .collect();
    Ok(LoadoutsState {
        path_b_active: true,
        active: set.active.clone(),
        loadouts: summaries,
    })
}

/// Replace the active-loadouts list and reapply group state: the
/// union rule while loadouts are active, full dormancy when the user
/// deactivates everything. Persists the loadout set to disk and emits
/// a state-changed event so other windows, such as the Loadouts editor
/// in Settings, see the update.
#[tauri::command]
pub(crate) async fn loadouts_set_active(app: AppHandle, active: Vec<String>) -> Result<(), String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    set_active_loadouts(&app, &app_data, active).await?;
    // The recomputed (or dormant) disabled lists live in the profile
    // snapshot on disk; queue a persist so a crash before the exit
    // flush cannot leave loadouts.toml and per-profile state
    // disagreeing. Also clears any stale persist suppression — this is
    // a durable change the user asked for.
    mark_profile_dirty(&app);
    let _ = app.emit(LOADOUTS_CHANGED, &());
    Ok(())
}

/// The part of [`loadouts_set_active`] that runs under the loadout and
/// profile locks: take the new active list, lay the group state it
/// imposes over the live profile, and save loadouts.toml in `app_data`.
/// The command looks up the app data folder and queues the profile
/// save, so a test can run this against a mock app and a scratch folder.
/// When the switch turned a macro group on or off, every window hears it
/// once the locks are released, since the command line keeps its own map
/// of the macro keys that fire.
pub(crate) async fn set_active_loadouts<R: tauri::Runtime>(
    app: &AppHandle<R>,
    app_data: &std::path::Path,
    active: Vec<String>,
) -> Result<(), String> {
    let state: SharedState = app.state::<SharedState>().inner().clone();
    let macro_groups_changed = {
        let mut guard = state.loadout_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err("Path B not active".into());
        };
        // Filter to known loadout names. A stale name (e.g. from a
        // future-truncated payload) is silently dropped rather than
        // returning an error.
        set.active = active
            .into_iter()
            .filter(|n| set.loadouts.iter().any(|l| &l.name == n))
            .collect();
        // Deactivate-all is the documented kill switch ("Activate none
        // to keep the catalog dormant"). Recorded as an explicit flag:
        // an empty active list on its own is ambiguous with "loadouts
        // have no opinion", and the other apply points (startup,
        // profile switch) must be able to re-impose dormancy.
        set.dormant = set.active.is_empty();
        let snapshot = set.clone();
        let mut p = state.profile.lock().await;
        let macro_groups_before = p.disabled_macro_groups.clone();
        crate::loadout_store::apply_effective_state(&snapshot, &mut p);
        if let Err(e) = crate::loadout_store::save_loadout_set(app_data, &snapshot) {
            warn!(error = %e, "loadouts.toml save failed");
        }
        p.disabled_macro_groups != macro_groups_before
    };
    if macro_groups_changed {
        broadcast(app, MACRO_GROUPS_CHANGED, &"");
    }
    Ok(())
}

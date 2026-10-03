//! The commands for loadout mode. Settings reads your loadouts and which
//! of them are on, and the Loadouts editor turns them on and off.

use tauri::{AppHandle, Emitter, Manager, State};

use crate::app::events::LOADOUTS_CHANGED;
use crate::app::state::SharedState;
use crate::disk::save::mark_profile_dirty;
use crate::loadouts::set::set_active_loadouts;

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

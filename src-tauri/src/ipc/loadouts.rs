//! The commands for loadout mode. Settings reads your loadouts and which
//! of them are on, and the Loadouts editor turns them on and off. Each
//! profile keeps its own stack of loadouts that are on, so
//! both commands take the `profile` they mean, which a session must play,
//! and act on the selected session's when they name none.

use tauri::{AppHandle, Emitter, Manager, State};

use crate::app::events::LOADOUTS_CHANGED;
use crate::app::state::SharedState;
use crate::disk::save::mark_profile_dirty;
use crate::loadouts::set::set_active_loadouts;

/// One loadout as the frontend cares about it: the user-visible
/// identifying fields and the `enabled_groups` list (chips for the
/// picker).
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct LoadoutSummary {
    pub name: String,
    pub description: Option<String>,
    pub enabled_groups: Vec<String>,
}

/// Shape returned by [`loadouts_get_state`]. Carries the active list,
/// the full loadout summaries, and a `loadout_mode` flag so the
/// frontend can decide whether to render the Loadouts tab at all.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct LoadoutsState {
    /// The page reads this under the name it had before the app called
    /// it loadout mode.
    #[serde(rename = "path_b_active")]
    pub loadout_mode: bool,
    pub active: Vec<String>,
    pub loadouts: Vec<LoadoutSummary>,
}

/// Snapshot the current loadout state for the Settings UI. In per
/// profile mode returns `loadout_mode: false` plus empty lists so the
/// frontend can hide the Loadouts tab. In loadout mode every loadout's
/// summary comes from the `state.loadout_set` mutex, with the active list
/// of the profile.
#[tauri::command]
pub(crate) async fn loadouts_get_state(
    state: State<'_, SharedState>,
    profile: Option<String>,
) -> Result<LoadoutsState, String> {
    let profile = state.edited_profile(profile)?.name();
    let guard = state.loadout_set.lock().await;
    let Some(set) = guard.as_ref() else {
        return Ok(LoadoutsState {
            loadout_mode: false,
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
        })
        .collect();
    Ok(LoadoutsState {
        loadout_mode: true,
        active: set.for_profile(profile.as_deref()).active.clone(),
        loadouts: summaries,
    })
}

/// Replace the active-loadouts list of the profile and reapply group
/// state: the union rule while loadouts are active, full dormancy when
/// the user deactivates everything. Persists the loadout set to disk and,
/// while the profile is in front, emits a state-changed event so other
/// windows, such as the Loadouts editor in Settings, see the update.
#[tauri::command]
pub(crate) async fn loadouts_set_active(
    app: AppHandle,
    active: Vec<String>,
    profile: Option<String>,
) -> Result<(), String> {
    let open = set_active_loadouts(&app, active, profile).await?;
    // The recomputed (or dormant) disabled lists live in the profile
    // snapshot on disk; queue a persist so a crash before the exit
    // flush cannot leave loadouts.toml and per-profile state
    // disagreeing. Also clears any stale persist suppression — this is
    // a durable change the user asked for.
    mark_profile_dirty(&app, &open);
    if app.state::<SharedState>().in_front(&open) {
        let _ = app.emit(LOADOUTS_CHANGED, &());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::LoadoutsState;

    #[test]
    fn the_page_reads_loadout_mode_under_its_old_name() {
        let state = LoadoutsState {
            loadout_mode: true,
            active: Vec::new(),
            loadouts: Vec::new(),
        };
        let sent = serde_json::to_value(state).unwrap();
        assert_eq!(sent["path_b_active"], true);
        assert!(sent.get("loadout_mode").is_none());
    }
}

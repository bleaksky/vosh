//! The commands for the tick timer. The tick on the status line reads
//! the live configuration, and the timers editor in Settings saves a new
//! one. The tick settings belong to a profile, and every session on it
//! keeps its own count.

use tauri::{AppHandle, State};

use crate::app::events::TICK_CONFIG_CHANGED;
use crate::app::state::SharedState;
use crate::disk::save::{save_then_broadcast, SavePolicy};
use crate::sessions::SessionId;
use crate::tick::{apply_tick_config, follow_in_other_sessions, Daylight, TickConfig, TickRuntime};

/// Read the tick configuration of the profile `profile` names, or of the
/// selected session's.
#[tauri::command]
pub(crate) async fn tick_get_config(
    state: State<'_, SharedState>,
    profile: Option<String>,
) -> Result<TickConfig, String> {
    let p = state.lock_named(profile).await?;
    Ok(p.tick.config.clone())
}

/// Apply a new tick configuration through [`apply_tick_config`], which
/// changes every field or none, to the profile `profile` names, or the
/// selected session's, and every count on it follows.
/// Persists the profile and broadcasts `vosh://tick-config-changed`, while
/// the profile is in front, only after the whole configuration applied.
#[tauri::command]
pub(crate) async fn tick_set_config<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    config: TickConfig,
    profile: Option<String>,
) -> Result<TickConfig, String> {
    // The sessions, the selected one first, come from the map before the
    // profile lock.
    let selected = state.selected_session();
    let mut sessions = state.other_sessions(selected.id);
    sessions.insert(0, selected);
    let (lead, open, before, snapshot) = {
        let mut p = state.lock_named(profile).await?;
        let before = p.tick.config.clone();
        // The selected session's count takes the settings when it plays
        // the profile, as it does for a #tick line typed there, and else
        // the first count on the profile. The others follow below.
        let lead = p.players(&sessions).next().cloned();
        let now = tokio::time::Instant::now();
        let snapshot = match &lead {
            Some(session) => {
                let mut c = session.connection.lock();
                apply_tick_config(&mut p.tick, &mut c.tick, &config, now)?
            }
            // No session read before the lock plays the profile now, as a
            // close or a switch went first, so the settings land on the
            // profile alone.
            None => apply_tick_config(&mut p.tick, &mut TickRuntime::default(), &config, now)?,
        };
        (lead, p.open().clone(), before, snapshot)
    };
    if let Some(lead) = lead {
        follow_in_other_sessions(&state, lead.id, &open, &before).await;
    }
    save_then_broadcast(
        &app,
        &state,
        &open,
        SavePolicy::Now,
        TICK_CONFIG_CHANGED,
        &snapshot,
    )
    .await;
    Ok(snapshot)
}

/// Whether the sun is up in the game of `session`, or the selected one,
/// as its latest World.Time said, through a drop too: `day`, `night`, or
/// null before the first. A window that opens reads it, since each turn
/// goes out once on `vosh://daylight-changed`.
#[tauri::command]
pub(crate) async fn daylight_get(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<Option<Daylight>, String> {
    let session = state.session(session)?;
    let daylight = session.connection.lock().tick.daylight;
    Ok(daylight)
}

//! The commands for the tick timer. The tick on the status line reads
//! the live configuration, and the timers editor in Settings saves a new
//! one.

use tauri::{AppHandle, State};

use crate::app::events::{broadcast, TICK_CONFIG_CHANGED};
use crate::app::state::SharedState;
use crate::disk::save::persist_profile;
use crate::tick::{apply_tick_config, tick_config_payload, TickConfigPayload};

/// Read the live tick configuration.
#[tauri::command]
pub(crate) async fn tick_get_config(
    state: State<'_, SharedState>,
) -> Result<TickConfigPayload, String> {
    let p = state.profile.lock().await;
    Ok(tick_config_payload(&p.tick.config))
}

/// Apply a new tick configuration through [`apply_tick_config`], which
/// changes every field or none. Persists the active profile and
/// broadcasts `vosh://tick-config-changed` only after the whole
/// configuration applied.
#[tauri::command]
pub(crate) async fn tick_set_config(
    app: AppHandle,
    state: State<'_, SharedState>,
    config: TickConfigPayload,
) -> Result<TickConfigPayload, String> {
    let snapshot = {
        let mut p = state.profile.lock().await;
        apply_tick_config(&mut p.tick, &config, tokio::time::Instant::now())?
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast(&app, TICK_CONFIG_CHANGED, &snapshot);
    Ok(snapshot)
}

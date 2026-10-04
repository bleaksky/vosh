//! The commands for the tick timer. The tick on the status line reads
//! the live configuration, and the timers editor in Settings saves a new
//! one.

use tauri::{AppHandle, State};

use crate::app::events::TICK_CONFIG_CHANGED;
use crate::app::state::SharedState;
use crate::disk::save::{save_then_broadcast, SavePolicy};
use crate::tick::{apply_tick_config, TickConfig};

/// Read the live tick configuration.
#[tauri::command]
pub(crate) async fn tick_get_config(state: State<'_, SharedState>) -> Result<TickConfig, String> {
    let p = state.selected_session().lock_profile().await;
    Ok(p.tick.config.clone())
}

/// Apply a new tick configuration through [`apply_tick_config`], which
/// changes every field or none. Persists the active profile and
/// broadcasts `vosh://tick-config-changed` only after the whole
/// configuration applied.
#[tauri::command]
pub(crate) async fn tick_set_config(
    app: AppHandle,
    state: State<'_, SharedState>,
    config: TickConfig,
) -> Result<TickConfig, String> {
    let session = state.selected_session();
    let (open, snapshot) = {
        let mut p = session.lock_profile().await;
        let mut c = session.connection.lock();
        let snapshot = apply_tick_config(
            &mut p.tick,
            &mut c.tick,
            &config,
            tokio::time::Instant::now(),
        )?;
        (p.open().clone(), snapshot)
    };
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

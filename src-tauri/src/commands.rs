//! Tauri commands invoked by the frontend.

use std::sync::Arc;

use tauri::{AppHandle, State};
use tokio::sync::Mutex;

use crate::session::{self, SessionHandle};

/// Application-wide state. Phase 1 carries a single optional session. Phase 5
/// turns this into a map keyed by session id.
#[derive(Default)]
pub(crate) struct AppState {
    pub(crate) session: Mutex<Option<SessionHandle>>,
}

pub(crate) type SharedState = Arc<AppState>;

#[tauri::command]
pub(crate) async fn session_connect(
    app: AppHandle,
    state: State<'_, SharedState>,
    host: String,
    port: u16,
    tls: bool,
) -> Result<(), String> {
    let mut current = state.session.lock().await;
    if let Some(handle) = current.take() {
        handle.shutdown().await;
    }

    let handle = session::spawn(app, host, port, tls)
        .await
        .map_err(|e| e.to_string())?;
    *current = Some(handle);
    Ok(())
}

#[tauri::command]
pub(crate) async fn session_send(
    state: State<'_, SharedState>,
    bytes: Vec<u8>,
) -> Result<(), String> {
    let current = state.session.lock().await;
    let Some(handle) = current.as_ref() else {
        return Err("not connected".to_string());
    };
    if !handle.send(bytes) {
        return Err("session task gone".to_string());
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn session_disconnect(state: State<'_, SharedState>) -> Result<(), String> {
    let mut current = state.session.lock().await;
    if let Some(handle) = current.take() {
        handle.shutdown().await;
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

//! Tauri commands invoked by the frontend.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use crate::input;
use crate::profile::Profile;
use crate::session::{self, OutputPayload, SessionHandle};

/// Application-wide state. Phase 1 carries a single optional session and one
/// profile. Phase 5 widens this to a session map; Phase 9 widens to multiple
/// profiles.
#[derive(Default)]
pub(crate) struct AppState {
    pub(crate) session: Mutex<Option<SessionHandle>>,
    pub(crate) profile: Mutex<Profile>,
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

    // Clear session-scoped variables on reconnect; profile-scoped survive.
    state.profile.lock().await.vars.clear_session();

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
pub(crate) async fn session_send_input(
    app: AppHandle,
    state: State<'_, SharedState>,
    line: String,
) -> Result<(), String> {
    let result = {
        let mut profile = state.profile.lock().await;
        input::process(&mut profile, &line)
    };

    if !result.echo.is_empty() {
        let mut buf = Vec::new();
        for line in &result.echo {
            buf.extend_from_slice(b"\r\n");
            buf.extend_from_slice(line.as_bytes());
        }
        buf.extend_from_slice(b"\r\n");
        let _ = app.emit("session://output", OutputPayload { bytes: buf });
    }

    if result.bytes.is_empty() {
        return Ok(());
    }

    let current = state.session.lock().await;
    let Some(handle) = current.as_ref() else {
        let _ = app.emit(
            "session://output",
            OutputPayload {
                bytes: b"\r\n[not connected]\r\n".to_vec(),
            },
        );
        return Ok(());
    };
    if !handle.send(result.bytes) {
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

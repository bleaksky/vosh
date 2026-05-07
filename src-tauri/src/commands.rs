//! Tauri commands invoked by the frontend.

use std::sync::Arc;

use mudclient_map::Room;
use mudclient_trigger::Trigger;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use crate::input;
use crate::map_state::SharedMap;
use crate::profile::Profile;
use crate::script_state::SharedTimers;
use crate::session::{self, OutputPayload, SessionHandle};

/// Application-wide state. Phase 1 carries a single optional session and one
/// profile. Phase 5 widens this to a session map; Phase 9 widens to multiple
/// profiles.
#[derive(Default)]
pub(crate) struct AppState {
    pub(crate) session: Mutex<Option<SessionHandle>>,
    pub(crate) profile: Arc<Mutex<Profile>>,
    pub(crate) map: SharedMap,
    pub(crate) script_timers: SharedTimers,
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
    // Take any existing handle out under a brief lock and drop the lock
    // before doing the long-running connect. This lets `session_disconnect`
    // run concurrently to cancel a hung connect attempt.
    let old = {
        let mut current = state.session.lock().await;
        current.take()
    };
    if let Some(handle) = old {
        handle.shutdown().await;
    }

    // Clear session-scoped variables on reconnect; profile-scoped survive.
    state.profile.lock().await.vars.clear_session();

    let handle = session::spawn(
        app.clone(),
        host,
        port,
        tls,
        state.profile.clone(),
        state.map.clone(),
        state.script_timers.clone(),
    )
    .await
    .map_err(|e| {
        // Surface the disconnected state so the UI does not stay stuck on
        // "connecting...". The frontend listens for session://state.
        let _ = app.emit(
            "session://state",
            crate::session::StatePayload::Disconnected {
                reason: Some(e.to_string()),
            },
        );
        e.to_string()
    })?;

    let mut current = state.session.lock().await;
    if let Some(prev) = current.take() {
        // A concurrent connect raced us. Shut down our old handle.
        prev.shutdown().await;
    }
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

    // Push the cursor to a fresh line before the MUD's response arrives.
    // Many MUDs send their prompt without a trailing newline and follow up
    // with the next room name on the same line; injecting a CRLF here
    // gives that output its own row in the terminal.
    let _ = app.emit(
        "session://output",
        OutputPayload {
            bytes: b"\r\n".to_vec(),
        },
    );

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
    state: State<'_, SharedState>,
    json: String,
) -> Result<usize, String> {
    let mut p = state.profile.lock().await;
    p.triggers.import_json(&json).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[derive(serde::Serialize)]
pub(crate) struct AreaSnapshot {
    pub current_room_id: Option<i64>,
    pub area: String,
    pub rooms: Vec<Room>,
    pub exits: Vec<mudclient_map::Exit>,
}

#[tauri::command]
pub(crate) async fn map_area_snapshot(
    state: State<'_, SharedState>,
) -> Result<Option<AreaSnapshot>, String> {
    let guard = state.map.lock().await;
    let Some(map) = guard.as_ref() else {
        return Ok(None);
    };
    let Some(current_id) = map.current_room_id else {
        return Ok(None);
    };
    let Some(current) = map.store.get_room(current_id).map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let area = current.area.clone();
    let rooms = map.store.list_area(&area).map_err(|e| e.to_string())?;
    let exits = map.store.exits_in_area(&area).map_err(|e| e.to_string())?;
    Ok(Some(AreaSnapshot {
        current_room_id: Some(current_id),
        area,
        rooms,
        exits,
    }))
}

#[tauri::command]
pub(crate) async fn map_walk_to(
    state: State<'_, SharedState>,
    target_id: i64,
) -> Result<(), String> {
    let path = {
        let guard = state.map.lock().await;
        let Some(map) = guard.as_ref() else {
            return Err("map not ready".to_string());
        };
        let Some(current) = map.current_room_id else {
            return Err("not in a known room".to_string());
        };
        map.store
            .find_path(current, target_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "no known path".to_string())?
    };
    if path.is_empty() {
        return Ok(());
    }
    let mut bytes = Vec::with_capacity(path.iter().map(|s| s.len() + 2).sum());
    for dir in path {
        bytes.extend_from_slice(dir.as_bytes());
        bytes.extend_from_slice(b"\r\n");
    }
    let session = state.session.lock().await;
    let Some(handle) = session.as_ref() else {
        return Err("not connected".to_string());
    };
    if !handle.send(bytes) {
        return Err("session task gone".to_string());
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn map_set_note(
    state: State<'_, SharedState>,
    room_id: i64,
    notes: String,
) -> Result<(), String> {
    let mut guard = state.map.lock().await;
    let Some(map) = guard.as_mut() else {
        return Err("map not ready".to_string());
    };
    map.store
        .set_note(room_id, &notes)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn map_set_avoid(
    state: State<'_, SharedState>,
    room_id: i64,
    avoid: bool,
) -> Result<(), String> {
    let mut guard = state.map.lock().await;
    let Some(map) = guard.as_mut() else {
        return Err("map not ready".to_string());
    };
    map.store
        .set_avoid(room_id, avoid)
        .map_err(|e| e.to_string())
}

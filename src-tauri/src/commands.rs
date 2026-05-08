//! Tauri commands invoked by the frontend.

use std::sync::Arc;

use mudclient_log::{SearchHit, SearchOptions, SessionRow};
use mudclient_map::Room;
use mudclient_trigger::Trigger;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex;
use tracing::warn;

use crate::input;
use crate::log_state::{SharedLogStore, SharedScrollback};
use crate::map_state::SharedMap;
use crate::plugins::{PluginRecord, SharedPluginManager};
use crate::profile::Profile;
use crate::profile_config::ProfileConfig;
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
    pub(crate) logs: SharedLogStore,
    pub(crate) scrollback: SharedScrollback,
    pub(crate) plugins: SharedPluginManager,
}

pub(crate) type SharedState = Arc<AppState>;

/// Snapshot the live profile and write it to `<app_data_dir>/profile.toml`.
/// Failures are logged but not surfaced — callers don't want a UI toggle
/// to fail because the disk is full mid-flight, and the in-memory state
/// is still correct for the rest of the session.
async fn persist_profile(app: &AppHandle, state: &SharedState) {
    let Ok(dir) = app.path().app_data_dir() else {
        return;
    };
    let path = dir.join("profile.toml");
    let snapshot = {
        let p = state.profile.lock().await;
        ProfileConfig::from_profile(&p)
    };
    if let Err(e) = snapshot.save(&path) {
        warn!(error = %e, path = %path.display(), "auto-save profile failed");
    }
}

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

    let scrollback_path = tauri::Manager::path(&app)
        .app_data_dir()
        .ok()
        .map(|dir| crate::log_state::scrollback_path(&dir));

    let handle = session::spawn(
        app.clone(),
        host,
        port,
        tls,
        state.profile.clone(),
        state.map.clone(),
        state.script_timers.clone(),
        state.logs.clone(),
        state.scrollback.clone(),
        scrollback_path,
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
            buf.extend_from_slice(line.as_bytes());
            buf.extend_from_slice(b"\r\n");
        }
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

#[tauri::command]
pub(crate) async fn profile_export(state: State<'_, SharedState>) -> Result<String, String> {
    let p = state.profile.lock().await;
    let snapshot = ProfileConfig::from_profile(&p);
    snapshot.to_toml().map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn profile_import(
    state: State<'_, SharedState>,
    toml: String,
) -> Result<Vec<String>, String> {
    let snapshot = ProfileConfig::from_toml(&toml).map_err(|e| e.to_string())?;
    let mut p = state.profile.lock().await;
    Ok(snapshot.apply_to(&mut p))
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

#[tauri::command]
pub(crate) async fn logs_list_sessions(
    state: State<'_, SharedState>,
    limit: usize,
) -> Result<Vec<SessionRow>, String> {
    let guard = state.logs.lock().await;
    let Some(store) = guard.as_ref() else {
        return Ok(Vec::new());
    };
    store.list_sessions(limit).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn logs_search(
    state: State<'_, SharedState>,
    pattern: String,
    case_sensitive: bool,
    max_results: usize,
    session_id: Option<i64>,
) -> Result<Vec<SearchHit>, String> {
    let guard = state.logs.lock().await;
    let Some(store) = guard.as_ref() else {
        return Ok(Vec::new());
    };
    let opts = SearchOptions {
        case_sensitive,
        max_results,
        session_id,
    };
    store.search(&pattern, &opts).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn logs_export(
    state: State<'_, SharedState>,
    session_id: i64,
    with_ansi: bool,
) -> Result<String, String> {
    let guard = state.logs.lock().await;
    let Some(store) = guard.as_ref() else {
        return Err("log store not ready".to_string());
    };
    store
        .export_session(session_id, with_ansi)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn scrollback_load(state: State<'_, SharedState>) -> Result<Vec<u8>, String> {
    let sb = state.scrollback.lock().await;
    Ok(sb.dump())
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct UiConfigPayload {
    pub theme: String,
    pub auto_update: bool,
    pub font_family: String,
    pub font_size: u32,
    pub tracked_affects: Vec<String>,
    pub dock_layout: String,
}

#[tauri::command]
pub(crate) async fn ui_get_config(
    state: State<'_, SharedState>,
) -> Result<UiConfigPayload, String> {
    let p = state.profile.lock().await;
    Ok(UiConfigPayload {
        theme: p.ui.theme.clone(),
        auto_update: p.ui.auto_update,
        font_family: p.ui.font_family.clone(),
        font_size: p.ui.font_size,
        tracked_affects: p.ui.tracked_affects.clone(),
        dock_layout: p.ui.dock_layout.clone(),
    })
}

#[tauri::command]
pub(crate) async fn ui_set_dock_layout(
    app: AppHandle,
    state: State<'_, SharedState>,
    layout: String,
) -> Result<(), String> {
    {
        let mut p = state.profile.lock().await;
        p.ui.dock_layout = layout;
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    Ok(())
}

#[tauri::command]
pub(crate) async fn ui_set_config(
    app: AppHandle,
    state: State<'_, SharedState>,
    theme: String,
    auto_update: bool,
    font_family: String,
    font_size: u32,
    tracked_affects: Vec<String>,
) -> Result<(), String> {
    {
        let mut p = state.profile.lock().await;
        p.ui.theme = theme;
        p.ui.auto_update = auto_update;
        p.ui.font_family = font_family;
        p.ui.font_size = font_size.clamp(6, 64);
        p.ui.tracked_affects = tracked_affects
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    Ok(())
}

#[derive(serde::Serialize)]
pub(crate) struct UpdateCheckResult {
    pub available: bool,
    pub version: Option<String>,
    pub notes: Option<String>,
}

#[derive(serde::Serialize)]
pub(crate) struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub entry: String,
    pub dir: String,
    pub enabled: bool,
}

impl From<&PluginRecord> for PluginInfo {
    fn from(p: &PluginRecord) -> Self {
        Self {
            name: p.manifest.name.clone(),
            version: p.manifest.version.clone(),
            description: p.manifest.description.clone(),
            author: p.manifest.author.clone(),
            entry: p.manifest.entry.clone(),
            dir: p.dir.display().to_string(),
            enabled: p.enabled,
        }
    }
}

#[tauri::command]
pub(crate) async fn plugins_list(state: State<'_, SharedState>) -> Result<Vec<PluginInfo>, String> {
    let mut mgr = state.plugins.lock().await;
    mgr.discover().map_err(|e| e.to_string())?;
    Ok(mgr.list().iter().map(PluginInfo::from).collect())
}

#[tauri::command]
pub(crate) async fn plugins_set_enabled(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
    enabled: bool,
) -> Result<bool, String> {
    let body = if enabled {
        let mgr = state.plugins.lock().await;
        if !mgr.list().iter().any(|p| p.manifest.name == name) {
            return Err(format!("plugin `{name}` not found"));
        }
        Some(mgr.read_entry(&name).map_err(|e| e.to_string())?)
    } else {
        None
    };

    {
        let mut mgr = state.plugins.lock().await;
        if !mgr.mark_enabled(&name, enabled) {
            return Err(format!("plugin `{name}` not found"));
        }
        let mut p = state.profile.lock().await;
        p.plugins.enabled = mgr.enabled_names();
    }

    if let Some(code) = body {
        let mut p = state.profile.lock().await;
        crate::script_state::snapshot_vars(&p.script, &p.vars);
        let outcome = p
            .script
            .load_script(&format!("plugin:{name}"), code)
            .map_err(|e| e.to_string())?;
        let _ = crate::script_state::apply_actions(&mut p, outcome);
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    Ok(enabled)
}

#[tauri::command]
pub(crate) async fn plugins_reload(
    state: State<'_, SharedState>,
    name: String,
) -> Result<(), String> {
    let code = {
        let mgr = state.plugins.lock().await;
        mgr.read_entry(&name).map_err(|e| e.to_string())?
    };
    let mut p = state.profile.lock().await;
    crate::script_state::snapshot_vars(&p.script, &p.vars);
    let outcome = p
        .script
        .load_script(&format!("plugin:{name}"), code)
        .map_err(|e| e.to_string())?;
    let _ = crate::script_state::apply_actions(&mut p, outcome);
    Ok(())
}

#[tauri::command]
pub(crate) async fn updater_check(app: AppHandle) -> Result<UpdateCheckResult, String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => Ok(UpdateCheckResult {
            available: true,
            version: Some(update.version.clone()),
            notes: update.body.clone(),
        }),
        Ok(None) => Ok(UpdateCheckResult {
            available: false,
            version: None,
            notes: None,
        }),
        Err(e) => Err(e.to_string()),
    }
}

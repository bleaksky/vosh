//! Tauri commands invoked by the frontend.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::Mutex;
use tracing::warn;
use vosh_log::{SearchHit, SearchOptions, SessionRow};
use vosh_map::Room;
use vosh_trigger::Trigger;

use crate::input;
use crate::log_state::{SharedLogStore, SharedScrollback};

/// Send an event to every webview window. `AppHandle::emit` routes via
/// the global listener pool, which has been observed to skip late-attached
/// listeners in sibling webviews (the main window misses settings-window
/// updates). Iterating the live window map and emitting to each one
/// guarantees delivery to both the main and settings webviews.
fn broadcast<S: serde::Serialize + Clone>(app: &AppHandle, event: &str, payload: &S) {
    for win in app.webview_windows().values() {
        if let Err(e) = win.emit(event, payload.clone()) {
            warn!(error = %e, window = %win.label(), event, "broadcast failed");
        }
    }
}
use crate::map_state::SharedMap;
use crate::plugins::{PluginRecord, SharedPluginManager};
use crate::profile::{Macro, Profile};
use crate::profile_config::{strip_global_fields, DockEntryPersist, GlobalConfig, ProfileConfig};
use crate::script_state::SharedTimers;
use crate::session::{self, OutputPayload, SessionHandle, TargetPayload};

/// Application-wide state. Phase 1 carries a single optional session and one
/// profile. Phase 5 widens this to a session map; Phase 9 widens to multiple
/// profiles.
pub(crate) struct AppState {
    pub(crate) session: Mutex<Option<SessionHandle>>,
    pub(crate) profile: Arc<Mutex<Profile>>,
    pub(crate) map: SharedMap,
    pub(crate) script_timers: SharedTimers,
    pub(crate) logs: SharedLogStore,
    pub(crate) scrollback: SharedScrollback,
    pub(crate) plugins: SharedPluginManager,
    /// Catalog of named profiles. Loaded (or migrated from the legacy
    /// single-file layout) once at startup; commands mutate it under
    /// this mutex.
    pub(crate) profile_set: Arc<Mutex<Option<crate::profile_set::ProfileSet>>>,
    /// Last terminal size reported by the frontend, kept across the
    /// no-session window so a fresh `session_connect` can seed the
    /// telnet `Negotiator` with the real (cols, rows) instead of the
    /// 80×24 default. Without this cache the server's first NAWS
    /// reply carried 80 cols and wrapped early output (login banner,
    /// `who`, motd) until the user nudged the window. Stored under a
    /// std Mutex because the critical section is two integer copies
    /// — async overhead is not worth it.
    pub(crate) window_size: std::sync::Mutex<(u16, u16)>,
    /// Live connection target (host, port). Set when
    /// `session_connect` succeeds, cleared on disconnect. Read by the
    /// Char.Status-driven auto-switch path so the resolver knows
    /// which connection's profile to pick. Stored under a std mutex
    /// because the work inside the lock is just a clone.
    pub(crate) current_connection: std::sync::Mutex<Option<(String, u16)>>,
    /// Last character name observed via Char.Status or Char.Name on
    /// the current session. Cleared on disconnect. Used to suppress
    /// duplicate resolver calls when the MUD re-sends Char.Status on
    /// every vitals update.
    pub(crate) current_character: std::sync::Mutex<Option<String>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            session: Mutex::new(None),
            profile: Arc::new(Mutex::new(Profile::default())),
            map: SharedMap::default(),
            script_timers: SharedTimers::default(),
            logs: SharedLogStore::default(),
            scrollback: SharedScrollback::default(),
            plugins: SharedPluginManager::default(),
            profile_set: Arc::new(Mutex::new(None)),
            // Default matches `Negotiator::default()` so any code path
            // that bypasses `session_set_window_size` (e.g. an early
            // automated connect from a script) still gets a sensible
            // baseline.
            window_size: std::sync::Mutex::new((80, 24)),
            current_connection: std::sync::Mutex::new(None),
            current_character: std::sync::Mutex::new(None),
        }
    }
}

pub(crate) type SharedState = Arc<AppState>;

/// Snapshot the live profile and write it to the active profile's file
/// under `<app_data_dir>/profiles/<active>.toml`. Failures are logged
/// but not surfaced — callers don't want a UI toggle to fail because
/// the disk is full mid-flight, and the in-memory state is still
/// correct for the rest of the session.
async fn persist_profile(app: &AppHandle, state: &SharedState) {
    // Resolve the active profile's path + global path via ProfileSet
    // if it's been loaded; fall back to the legacy single-file path
    // if ProfileSet is somehow missing (shouldn't happen post-
    // startup; defensive path for very early calls before setup()
    // finishes).
    let (per_profile_path, global_path, scope) = {
        let guard = state.profile_set.lock().await;
        if let Some(set) = guard.as_ref() {
            (
                Some(set.active_path()),
                Some(set.global_path()),
                Some(*set.scope()),
            )
        } else {
            let Ok(dir) = app.path().app_data_dir() else {
                return;
            };
            (Some(dir.join("profile.toml")), None, None)
        }
    };

    let (mut per_profile_snapshot, global_snapshot) = {
        let p = state.profile.lock().await;
        let scope = scope.unwrap_or_default();
        (
            ProfileConfig::from_profile(&p),
            GlobalConfig::from_profile(&p, &scope),
        )
    };

    // Strip the global-scoped fields out of the per-profile snapshot
    // so they don't get duplicated. Honors the per-category scope
    // map (categories marked Profile-scoped stay in the per-profile
    // file).
    if let Some(scope) = scope.as_ref() {
        strip_global_fields(&mut per_profile_snapshot, scope);
    }

    if let Some(p) = per_profile_path.as_ref() {
        if let Err(e) = per_profile_snapshot.save(p) {
            warn!(error = %e, path = %p.display(), "auto-save per-profile failed");
        }
    }
    if let Some(g) = global_path.as_ref() {
        if let Err(e) = global_snapshot.save(g) {
            warn!(error = %e, path = %g.display(), "auto-save global failed");
        }
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

    // Remember the live connection target so the Char.Status-driven
    // auto-switch path can re-resolve against it once the MUD tells us
    // who we logged in as. Cleared in `session_disconnect`. Reset the
    // last-known character at the same time so a reconnect to a
    // different account triggers a fresh resolve.
    if let Ok(mut g) = state.current_connection.lock() {
        *g = Some((host.clone(), port));
    }
    if let Ok(mut g) = state.current_character.lock() {
        *g = None;
    }

    let scrollback_path = tauri::Manager::path(&app)
        .app_data_dir()
        .ok()
        .map(|dir| crate::log_state::scrollback_path(&dir));

    // Seed the negotiator with the most recently reported terminal
    // size so the initial `DO NAWS` reply during the handshake
    // carries the correct cols/rows. The default of (80, 24) is
    // applied only when the frontend never called
    // `session_set_window_size` before this connect.
    let initial_size = state.window_size.lock().map_or((80, 24), |g| *g);

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
        initial_size,
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
    let (result, target_after) = {
        let mut profile = state.profile.lock().await;
        let before_name = profile.target.name.clone();
        let before_idx = profile.target.room_idx;
        let before_keys = profile.target.quick_keys.clone();
        let result = input::process(&mut profile, &line);
        let after_name = profile.target.name.clone();
        let after_idx = profile.target.room_idx;
        let after_keys = profile.target.quick_keys.clone();
        let changed =
            before_name != after_name || before_idx != after_idx || before_keys != after_keys;
        let payload = if changed {
            Some(TargetPayload {
                name: after_name,
                room_idx: after_idx,
                quick_keys: after_keys,
            })
        } else {
            None
        };
        (result, payload)
    };

    if let Some(payload) = target_after {
        let _ = app.emit("session://target", payload);
    }

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
    if let Ok(mut g) = state.current_connection.lock() {
        *g = None;
    }
    if let Ok(mut g) = state.current_character.lock() {
        *g = None;
    }
    Ok(())
}

/// Inform the session of a new terminal size. The backend updates the
/// telnet negotiator and, when NAWS has already been negotiated with
/// the server, pushes a NAWS subnegotiation so the MUD re-wraps its
/// output at the new column count. No-op when not connected.
#[tauri::command]
pub(crate) async fn session_set_window_size(
    state: State<'_, SharedState>,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    // Always cache the size — even when no session exists, so the
    // next `session_connect` can seed the negotiator with the real
    // dimensions instead of the 80×24 default. Without this the
    // server's first NAWS reply (during the early handshake) would
    // carry the wrong size and wrap early output until the next
    // user-driven resize triggered a fresh subneg.
    if let Ok(mut guard) = state.window_size.lock() {
        *guard = (cols, rows);
    }
    let current = state.session.lock().await;
    if let Some(handle) = current.as_ref() {
        if !handle.set_window_size(cols, rows) {
            return Err("session task gone".into());
        }
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn triggers_list(state: State<'_, SharedState>) -> Result<Vec<Trigger>, String> {
    let p = state.profile.lock().await;
    Ok(p.triggers.list())
}

/// Snapshot of the current target state + configured quick-keys.
/// Frontend uses this to seed the `TargetBar` on mount before any
/// `session://target` events fire.
#[tauri::command]
pub(crate) async fn target_get(state: State<'_, SharedState>) -> Result<TargetPayload, String> {
    let p = state.profile.lock().await;
    Ok(TargetPayload {
        name: p.target.name.clone(),
        room_idx: p.target.room_idx,
        quick_keys: p.target.quick_keys.clone(),
    })
}

/// Snapshot of every keyboard macro binding. Used by the Settings
/// macros tab to render the existing list and by Input.tsx (via the
/// same payload) to seed its in-memory binding lookup before any
/// `vosh://macros-changed` event fires.
/// Detect which import format a file uses, based on content sniffing.
/// Frontend extension-checks first; this is the fallback. Returns
/// `null` when nothing recognized so the UI can ask the user.
#[tauri::command]
pub(crate) async fn import_detect(text: String) -> Result<Option<String>, String> {
    Ok(crate::import::detect_format(&text).map(|f| match f {
        crate::import::ImportFormat::Mushclient => "mushclient".to_string(),
        crate::import::ImportFormat::Mudlet => "mudlet".to_string(),
        crate::import::ImportFormat::Gmud => "gmud".to_string(),
        crate::import::ImportFormat::Cmud => "cmud".to_string(),
    }))
}

#[derive(serde::Serialize)]
pub(crate) struct ImportSummary {
    pub aliases: usize,
    pub triggers: usize,
    pub macros: usize,
    pub vars: usize,
    pub unsupported: Vec<(String, String)>,
    pub unparsed: Vec<String>,
    pub rejected: Vec<String>,
}

/// Parse + apply an import file to the live profile. The format
/// string is one of `mushclient` / `mudlet` / `gmud`; pass an
/// empty string to auto-detect. Aliases / triggers / macros / vars
/// merge into the existing stores (overwrite on name collision).
/// Returns a summary so the UI can report what landed and what
/// did not.
#[tauri::command]
pub(crate) async fn import_apply(
    app: AppHandle,
    state: State<'_, SharedState>,
    format: String,
    text: String,
) -> Result<ImportSummary, String> {
    let fmt = match format.as_str() {
        "mushclient" => crate::import::ImportFormat::Mushclient,
        "mudlet" => crate::import::ImportFormat::Mudlet,
        "gmud" => crate::import::ImportFormat::Gmud,
        "cmud" => crate::import::ImportFormat::Cmud,
        "" => crate::import::detect_format(&text)
            .ok_or_else(|| "could not detect import format".to_string())?,
        other => return Err(format!("unknown import format: {other}")),
    };
    let report = crate::import::parse(fmt, &text);
    let mut rejected: Vec<String> = Vec::new();
    let mut macros_changed = false;
    let macros_snapshot: Vec<Macro>;
    {
        let mut p = state.profile.lock().await;
        for alias in &report.aliases {
            p.aliases.set(alias.clone());
        }
        for trigger in &report.triggers {
            if let Err(e) = p.triggers.set(trigger.clone()) {
                rejected.push(format!("trigger `{}` rejected: {e}", trigger.name));
            }
        }
        for m in &report.macros {
            if let Some(existing) = p.macros.iter_mut().find(|x| x.key == m.key) {
                existing.command.clone_from(&m.command);
            } else {
                p.macros.push(m.clone());
            }
            macros_changed = true;
        }
        for (k, v) in &report.vars {
            p.vars.set(vosh_vars::Scope::Profile, k.clone(), v.clone());
        }
        macros_snapshot = p.macros.clone();
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    if macros_changed {
        broadcast(&app, "vosh://macros-changed", &macros_snapshot);
    }
    Ok(ImportSummary {
        aliases: report.aliases.len(),
        triggers: report.triggers.len() - rejected.len(),
        macros: report.macros.len(),
        vars: report.vars.len(),
        unsupported: report.unsupported,
        unparsed: report.unparsed,
        rejected,
    })
}

#[tauri::command]
pub(crate) async fn macros_list(state: State<'_, SharedState>) -> Result<Vec<Macro>, String> {
    let p = state.profile.lock().await;
    Ok(p.macros.clone())
}

/// Set or replace a binding by key. Empty `command` is rejected;
/// callers that want to unbind should use `macros_delete`.
/// Re-binding an existing key overwrites the prior command.
#[tauri::command]
pub(crate) async fn macros_set(
    app: AppHandle,
    state: State<'_, SharedState>,
    key: String,
    command: String,
    group: Option<String>,
) -> Result<Vec<Macro>, String> {
    let key = key.trim().to_string();
    let command = command.trim().to_string();
    if key.is_empty() {
        return Err("key cannot be empty".into());
    }
    if command.is_empty() {
        return Err("command cannot be empty".into());
    }
    // Normalize the group: empty / whitespace-only -> None so the
    // wire format does not persist an empty group string.
    let group = group
        .map(|g| g.trim().to_string())
        .filter(|g| !g.is_empty());
    let updated = {
        let mut p = state.profile.lock().await;
        if let Some(existing) = p.macros.iter_mut().find(|m| m.key == key) {
            existing.command = command;
            existing.group = group;
        } else {
            p.macros.push(Macro {
                key,
                command,
                group,
            });
        }
        p.macros.clone()
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast(&app, "vosh://macros-changed", &updated);
    Ok(updated)
}

/// Remove a binding by key. No-op when the key is not bound.
#[tauri::command]
pub(crate) async fn macros_delete(
    app: AppHandle,
    state: State<'_, SharedState>,
    key: String,
) -> Result<Vec<Macro>, String> {
    let updated = {
        let mut p = state.profile.lock().await;
        p.macros.retain(|m| m.key != key);
        p.macros.clone()
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast(&app, "vosh://macros-changed", &updated);
    Ok(updated)
}

/// One entry in a groups-list response: name + whether the group is
/// currently enabled. Used by every per-type groups list endpoint so
/// the frontend can render the toggle UI from a single shape.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct GroupState {
    pub name: String,
    pub enabled: bool,
}

#[tauri::command]
pub(crate) async fn aliases_groups_list(
    state: State<'_, SharedState>,
) -> Result<Vec<GroupState>, String> {
    let p = state.profile.lock().await;
    Ok(p.aliases
        .groups()
        .into_iter()
        .map(|(name, enabled)| GroupState { name, enabled })
        .collect())
}

#[tauri::command]
pub(crate) async fn aliases_set_group_enabled(
    app: AppHandle,
    state: State<'_, SharedState>,
    group: String,
    enabled: bool,
) -> Result<(), String> {
    {
        let mut p = state.profile.lock().await;
        p.aliases.set_group_enabled(group.trim(), enabled);
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast(&app, "vosh://alias-groups-changed", &group);
    Ok(())
}

#[tauri::command]
pub(crate) async fn triggers_groups_list(
    state: State<'_, SharedState>,
) -> Result<Vec<GroupState>, String> {
    let p = state.profile.lock().await;
    Ok(p.triggers
        .groups()
        .into_iter()
        .map(|(name, enabled)| GroupState { name, enabled })
        .collect())
}

#[tauri::command]
pub(crate) async fn triggers_set_group_enabled(
    app: AppHandle,
    state: State<'_, SharedState>,
    group: String,
    enabled: bool,
) -> Result<(), String> {
    {
        let mut p = state.profile.lock().await;
        p.triggers.set_group_enabled(group.trim(), enabled);
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast(&app, "vosh://trigger-groups-changed", &group);
    Ok(())
}

#[tauri::command]
pub(crate) async fn macros_groups_list(
    state: State<'_, SharedState>,
) -> Result<Vec<GroupState>, String> {
    let p = state.profile.lock().await;
    let mut names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for m in &p.macros {
        if let Some(g) = &m.group {
            if !g.is_empty() {
                names.insert(g.clone());
            }
        }
    }
    Ok(names
        .into_iter()
        .map(|n| {
            let enabled = !p.disabled_macro_groups.contains(&n);
            GroupState { name: n, enabled }
        })
        .collect())
}

#[tauri::command]
pub(crate) async fn macros_set_group_enabled(
    app: AppHandle,
    state: State<'_, SharedState>,
    group: String,
    enabled: bool,
) -> Result<(), String> {
    let group = group.trim().to_string();
    if group.is_empty() {
        return Ok(());
    }
    {
        let mut p = state.profile.lock().await;
        if enabled {
            p.disabled_macro_groups.remove(&group);
        } else {
            p.disabled_macro_groups.insert(group.clone());
        }
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast(&app, "vosh://macro-groups-changed", &group);
    Ok(())
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

/// Dump every alias to a pretty JSON array. Mirrors `triggers_export`
/// so the settings window can treat triggers and aliases with the
/// same `JsonTab` component.
#[tauri::command]
pub(crate) async fn aliases_export(state: State<'_, SharedState>) -> Result<String, String> {
    let p = state.profile.lock().await;
    let aliases: Vec<vosh_alias::Alias> = p.aliases.list().into_iter().cloned().collect();
    serde_json::to_string_pretty(&aliases).map_err(|e| e.to_string())
}

/// Replace the entire alias store with the JSON-decoded list. Returns
/// the count installed. Invalid JSON or wrong shape rejects without
/// touching the store.
#[tauri::command]
pub(crate) async fn aliases_import(
    state: State<'_, SharedState>,
    json: String,
) -> Result<usize, String> {
    let parsed: Vec<vosh_alias::Alias> = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let count = parsed.len();
    let mut p = state.profile.lock().await;
    let mut store = vosh_alias::AliasStore::new();
    for alias in parsed {
        store.set(alias);
    }
    p.aliases = store;
    Ok(count)
}

#[tauri::command]
pub(crate) fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Read the persistent dock layout. Returns the same shape as the
/// frontend `DockEntry` (id + zone) so the layout editor can render
/// from it directly.
#[tauri::command]
pub(crate) async fn dock_layout_get(
    state: State<'_, SharedState>,
) -> Result<Vec<DockEntryPersist>, String> {
    let p = state.profile.lock().await;
    Ok(p.ui.dock_layout.clone())
}

/// Replace the persistent dock layout. Persists to profile.toml and
/// broadcasts `vosh://dock-layout-changed` so other open windows
/// (specifically the main window) can re-apply without a relaunch.
#[tauri::command]
pub(crate) async fn dock_layout_set(
    app: AppHandle,
    state: State<'_, SharedState>,
    entries: Vec<DockEntryPersist>,
) -> Result<(), String> {
    {
        let mut p = state.profile.lock().await;
        p.ui.dock_layout.clone_from(&entries);
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    if let Err(e) = app.emit("vosh://dock-layout-changed", &entries) {
        warn!(error = %e, "failed to broadcast dock-layout-changed");
    }
    Ok(())
}

/// Open (or focus, if already open) the standalone settings window.
/// The settings window is a separate webview pointed at the same
/// frontend bundle with `?view=settings`, so the React entry can
/// branch and render the `SettingsApp` instead of the main `App`.
/// Both windows share the same Rust backend state.
#[tauri::command]
pub(crate) async fn open_settings_window(app: AppHandle) -> Result<(), String> {
    if let Some(existing) = app.get_webview_window("settings") {
        existing.show().map_err(|e| e.to_string())?;
        existing.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, "settings", WebviewUrl::App("index.html?view=settings".into()))
        .title("Vosh · settings")
        .inner_size(780.0, 640.0)
        .min_inner_size(520.0, 420.0)
        .resizable(true)
        // Frameless + transparent so the React side can draw the same
        // rounded Ghostty-style chrome the main window uses.
        .decorations(false)
        .transparent(true)
        // Stay hidden until the React app calls show() on first render
        // so the user never sees the unstyled default state.
        .visible(false)
        // Disable Tauri's OS file-drop handler. When enabled it
        // intercepts HTML5 drag-and-drop inside the webview, which
        // can break overlay drag interactions.
        .disable_drag_drop_handler()
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
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

// ============================================================
// Named profile collection (multi-profile support, Stage 1).
//
// Persistent layout under <app_data_dir>:
//     profiles.toml        — index (active + entries)
//     profiles/<name>.toml — per-profile snapshot
// AppState.profile_set holds the live ProfileSet behind a Mutex.
// ============================================================

#[derive(serde::Serialize)]
pub(crate) struct ProfilesListPayload {
    pub active: String,
    pub profiles: Vec<crate::profile_set::ProfileEntry>,
}

#[tauri::command]
pub(crate) async fn profiles_list(
    state: State<'_, SharedState>,
) -> Result<ProfilesListPayload, String> {
    let guard = state.profile_set.lock().await;
    let Some(set) = guard.as_ref() else {
        return Err("profile set not initialized".into());
    };
    Ok(ProfilesListPayload {
        active: set.active_name().to_string(),
        profiles: set.list().to_vec(),
    })
}

#[tauri::command]
pub(crate) async fn profile_create(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
) -> Result<(), String> {
    {
        let mut guard = state.profile_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err("profile set not initialized".into());
        };
        set.create(&name).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("vosh://profiles-changed", &name);
    Ok(())
}

#[tauri::command]
pub(crate) async fn profile_delete(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
) -> Result<(), String> {
    {
        let mut guard = state.profile_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err("profile set not initialized".into());
        };
        set.delete(&name).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("vosh://profiles-changed", &name);
    Ok(())
}

#[tauri::command]
pub(crate) async fn profile_rename(
    app: AppHandle,
    state: State<'_, SharedState>,
    old: String,
    new: String,
) -> Result<(), String> {
    {
        let mut guard = state.profile_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err("profile set not initialized".into());
        };
        set.rename(&old, &new).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("vosh://profiles-changed", &new);
    Ok(())
}

#[tauri::command]
pub(crate) async fn profile_duplicate(
    app: AppHandle,
    state: State<'_, SharedState>,
    source: String,
    new: String,
) -> Result<(), String> {
    {
        let mut guard = state.profile_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err("profile set not initialized".into());
        };
        set.duplicate(&source, &new).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("vosh://profiles-changed", &new);
    Ok(())
}

/// Read the per-category scope map. Frontend uses this to render
/// the toggle row in the Profiles tab.
#[tauri::command]
pub(crate) async fn profile_get_scope(
    state: State<'_, SharedState>,
) -> Result<crate::profile_set::ScopeConfig, String> {
    let guard = state.profile_set.lock().await;
    let Some(set) = guard.as_ref() else {
        return Err("profile set not initialized".into());
    };
    Ok(*set.scope())
}

/// Update the per-category scope map. After the index is updated,
/// persist the active profile so values move to the correct file
/// (a category flipped Global -> Profile lands in the per-profile
/// file on next save; Profile -> Global lands in global.toml).
#[tauri::command]
pub(crate) async fn profile_set_scope(
    app: AppHandle,
    state: State<'_, SharedState>,
    scope: crate::profile_set::ScopeConfig,
) -> Result<(), String> {
    {
        let mut guard = state.profile_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err("profile set not initialized".into());
        };
        set.set_scope(scope).map_err(|e| e.to_string())?;
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    let _ = app.emit("vosh://profiles-changed", "scope");
    Ok(())
}

#[tauri::command]
pub(crate) async fn profile_set_metadata(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
    description: Option<String>,
    auto_match: Option<crate::profile_set::AutoMatch>,
) -> Result<(), String> {
    {
        let mut guard = state.profile_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err("profile set not initialized".into());
        };
        set.set_metadata(&name, description, auto_match)
            .map_err(|e| e.to_string())?;
    }
    let _ = app.emit("vosh://profiles-changed", &name);
    Ok(())
}

/// Given a connect target, find the first profile whose `auto_match`
/// claims it. Returns the profile name or null. The frontend calls
/// this right before invoking `session_connect` so a matching
/// profile can be switched to ahead of the connection.
#[tauri::command]
pub(crate) async fn profile_resolve_match(
    state: State<'_, SharedState>,
    host: String,
    port: u16,
    character: Option<String>,
) -> Result<Option<String>, String> {
    let guard = state.profile_set.lock().await;
    let Some(set) = guard.as_ref() else {
        return Ok(None);
    };
    Ok(set.resolve_match(&host, port, character.as_deref()))
}

/// Shared body for switching the active profile. The
/// `profile_switch` Tauri command and the Char.Status auto-switch
/// path in `handle_char_known_for_auto_switch` both call this so the
/// persist + flip + reload sequence stays identical.
pub(crate) async fn apply_profile_switch(
    app: &AppHandle,
    state: &SharedState,
    name: &str,
) -> Result<(), String> {
    // Step 1: snapshot + write the CURRENT active profile so user
    // changes since the last persist are not lost on switch.
    persist_profile(app, state).await;

    // Step 2: flip the active pointer in the index.
    let (new_path, global_path) = {
        let mut guard = state.profile_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err("profile set not initialized".into());
        };
        set.switch(name).map_err(|e| e.to_string())?;
        (set.active_path(), set.global_path())
    };

    // Step 3: load per-profile file (or seed defaults) and then
    // overlay global.toml so theme/font/keep-last/auto-update/
    // dock_layout survive the switch.
    let per_profile = if new_path.exists() {
        Some(ProfileConfig::load(&new_path).map_err(|e| e.to_string())?)
    } else {
        None
    };
    let global = if global_path.exists() {
        Some(GlobalConfig::load(&global_path).map_err(|e| e.to_string())?)
    } else {
        None
    };
    {
        let mut p = state.profile.lock().await;
        match per_profile {
            Some(snap) => {
                snap.apply_to(&mut p);
            }
            None => {
                let default = ProfileConfig::default();
                default.apply_to(&mut p);
            }
        }
        if let Some(g) = global {
            g.apply_to(&mut p);
        }
    }

    let _ = app.emit("vosh://profile-switched", name);
    Ok(())
}

#[tauri::command]
pub(crate) async fn profile_switch(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
) -> Result<(), String> {
    let shared: SharedState = state.inner().clone();
    apply_profile_switch(&app, &shared, &name).await
}

/// Called by the session GMCP handler when Char.Status or Char.Name
/// reports a character name. Suppresses duplicate observations so the
/// resolver does not re-run on every Char.Status tick, then resolves
/// (host, port, character) against the profile set. When the resolved
/// profile differs from the currently-active one, swap to it and
/// announce on the terminal so the user knows the active profile
/// changed.
pub(crate) async fn handle_char_known_for_auto_switch(
    app: &AppHandle,
    state: &SharedState,
    character: &str,
) {
    let trimmed = character.trim();
    if trimmed.is_empty() {
        return;
    }
    // Short-circuit on duplicate observations. Char.Status is sent on
    // every vitals update, so without this gate the resolver would
    // run every tick.
    let should_resolve = {
        let Ok(mut guard) = state.current_character.lock() else {
            return;
        };
        if guard.as_deref() == Some(trimmed) {
            false
        } else {
            *guard = Some(trimmed.to_string());
            true
        }
    };
    if !should_resolve {
        return;
    }
    let Some((host, port)) = state.current_connection.lock().ok().and_then(|g| g.clone()) else {
        return;
    };
    let target = {
        let guard = state.profile_set.lock().await;
        let Some(set) = guard.as_ref() else {
            return;
        };
        let resolved = set.resolve_match(&host, port, Some(trimmed));
        match resolved {
            Some(name) if name != set.active_name() => Some(name),
            _ => None,
        }
    };
    let Some(new_name) = target else {
        return;
    };
    if let Err(e) = apply_profile_switch(app, state, &new_name).await {
        warn!(error = %e, "auto profile switch failed");
        return;
    }
    // Yellow announce line on the terminal mirrors the [vosh] system-
    // message style used by tick warnings.
    let line = format!("\r\n\x1b[33m[vosh] auto-switched profile to {new_name}\x1b[0m\r\n");
    let _ = app.emit(
        "session://output",
        OutputPayload {
            bytes: line.into_bytes(),
        },
    );
}

#[derive(serde::Serialize)]
pub(crate) struct AreaSnapshot {
    pub current_room_id: Option<i64>,
    pub area: String,
    pub rooms: Vec<Room>,
    pub exits: Vec<vosh_map::Exit>,
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
    pub tracked_affects: Vec<crate::profile_config::TrackedAffect>,
    pub enabled_presets: Vec<String>,
    pub keep_last_command: bool,
    pub theme_terminal_colors: bool,
    pub custom_themes: Vec<crate::profile_config::CustomTheme>,
    pub split_divider_color: Option<String>,
    pub side_panels_fill_height: bool,
    pub paste_line_delay_ms: u32,
    pub vitals: crate::profile_config::VitalsConfig,
    pub moons_position: String,
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
        enabled_presets: p.ui.enabled_presets.clone(),
        keep_last_command: p.ui.keep_last_command,
        theme_terminal_colors: p.ui.theme_terminal_colors,
        custom_themes: p.ui.custom_themes.clone(),
        split_divider_color: p.ui.split_divider_color.clone(),
        side_panels_fill_height: p.ui.side_panels_fill_height,
        paste_line_delay_ms: p.ui.paste_line_delay_ms,
        vitals: p.ui.vitals.clone(),
        moons_position: p.ui.moons_position.clone(),
    })
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn ui_set_config(
    app: AppHandle,
    state: State<'_, SharedState>,
    theme: String,
    auto_update: bool,
    font_family: String,
    font_size: u32,
    tracked_affects: Vec<crate::profile_config::TrackedAffect>,
    enabled_presets: Vec<String>,
    keep_last_command: bool,
    theme_terminal_colors: bool,
    custom_themes: Vec<crate::profile_config::CustomTheme>,
    split_divider_color: Option<String>,
    side_panels_fill_height: bool,
    paste_line_delay_ms: u32,
    vitals: crate::profile_config::VitalsConfig,
    moons_position: String,
) -> Result<(), String> {
    {
        let mut p = state.profile.lock().await;
        p.ui.theme = theme;
        p.ui.auto_update = auto_update;
        p.ui.font_family = font_family;
        p.ui.font_size = font_size.clamp(6, 64);
        // Trim each entry's name + label; drop rows whose name is
        // empty after trimming (no point tracking "" — it'll never
        // match a real affect). An empty label is normalized back to
        // None so the affects bar falls through to the name.
        p.ui.tracked_affects = tracked_affects
            .into_iter()
            .map(|t| crate::profile_config::TrackedAffect {
                name: t.name.trim().to_string(),
                label: t
                    .label
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
            })
            .filter(|t| !t.name.is_empty())
            .collect();
        p.ui.enabled_presets = enabled_presets
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        p.ui.enabled_presets.sort();
        p.ui.enabled_presets.dedup();
        p.ui.keep_last_command = keep_last_command;
        p.ui.theme_terminal_colors = theme_terminal_colors;
        p.ui.custom_themes = custom_themes;
        // Empty strings get normalized to None so the picker can clear
        // back to the theme default by submitting "".
        p.ui.split_divider_color = split_divider_color.and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        p.ui.side_panels_fill_height = side_panels_fill_height;
        // Clamp to a sane range so a malformed input cannot freeze the
        // paste indicator (0–10s per line is plenty).
        p.ui.paste_line_delay_ms = paste_line_delay_ms.min(10_000);
        // Normalize vitals glyphs + width. Empty glyph strings would
        // render zero-width bars; collapse to the default in that case
        // so the user cannot accidentally hide the bar via a typo.
        // Also coerce unknown layout / percent_color values back to
        // the defaults so a hand-edited profile.toml typo does not
        // break the panel render.
        let mut v = vitals;
        if v.bar_filled.is_empty() {
            v.bar_filled = "▰".to_string();
        }
        if v.bar_empty.is_empty() {
            v.bar_empty = "▱".to_string();
        }
        v.bar_width = v.bar_width.clamp(4, 60);
        if v.layout != "stacked" && v.layout != "inline" {
            v.layout = "stacked".to_string();
        }
        if v.percent_color != "fill" && v.percent_color != "gradient" {
            v.percent_color = "fill".to_string();
        }
        if v.bar_style != "solid" && v.bar_style != "track" {
            // "ramped" was an earlier mode that used Unicode 1/8-step
            // partial-block glyphs. It rendered ugly in most fonts so
            // the option got dropped; coerce any leftover config back
            // to "solid".
            v.bar_style = "solid".to_string();
        }
        p.ui.vitals = v;
        // Coerce an unknown moons_position value back to "right-edge"
        // so a hand-edited profile.toml typo cannot leave the status
        // bar rendering moons in an unrecognized slot.
        p.ui.moons_position = match moons_position.as_str() {
            "before-time" | "after-time" | "right-edge" => moons_position,
            _ => "right-edge".to_string(),
        };
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    Ok(())
}

/// Bulk-install a set of preset triggers. Each trigger should already
/// have its `preset` field set to the preset id; this command
/// validates and inserts them so the engine starts matching
/// immediately. Returns the number installed.
#[tauri::command]
pub(crate) async fn presets_install(
    app: AppHandle,
    state: State<'_, SharedState>,
    triggers: Vec<Trigger>,
) -> Result<usize, String> {
    let mut installed = 0usize;
    {
        let mut p = state.profile.lock().await;
        for t in triggers {
            p.triggers.set(t).map_err(|e| e.to_string())?;
            installed += 1;
        }
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    Ok(installed)
}

/// Remove every trigger tagged with the given preset id. Returns the
/// number removed.
#[tauri::command]
pub(crate) async fn presets_remove(
    app: AppHandle,
    state: State<'_, SharedState>,
    preset_id: String,
) -> Result<usize, String> {
    let removed = {
        let mut p = state.profile.lock().await;
        p.triggers.remove_by_preset(&preset_id)
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    Ok(removed)
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

/// Download + install the pending update and restart the app. Errors
/// surface to the frontend; the relaunch is a hard exit so any UI
/// confirmation has to happen before this call returns.
#[tauri::command]
pub(crate) async fn updater_install_and_relaunch(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no update available".to_string())?;
    // Progress callbacks are no-ops at this stage; can be wired to
    // session://event later for a download progress bar.
    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    app.restart();
}

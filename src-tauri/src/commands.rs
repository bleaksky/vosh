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
use crate::script_state;
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
    /// Path B authoring catalog. `Some` when the app started up with
    /// `catalog.toml` present (Path B mode); `None` in legacy per-
    /// profile mode. Mutated alongside the live `Profile` so on-disk
    /// state stays in step with in-memory edits.
    pub(crate) global_catalog: Arc<Mutex<Option<crate::loadout::GlobalCatalog>>>,
    /// Path B loadout collection. Same `Some`/`None` semantics as
    /// `global_catalog`. The active subset drives which catalog groups
    /// the runtime gates on (see [`crate::loadout_store::apply_loadout_state`]).
    pub(crate) loadout_set: Arc<Mutex<Option<crate::loadout::LoadoutSet>>>,
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
            global_catalog: Arc::new(Mutex::new(None)),
            loadout_set: Arc::new(Mutex::new(None)),
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
    // Path B branch. When `state.global_catalog` is `Some`, the user is
    // post-migration: authored items live in catalog.toml and the live
    // Profile is the cache. Write the live aliases / triggers / macros
    // back to the catalog plus the loadout set; the per-profile branch
    // below is skipped entirely (legacy profiles are in profiles/legacy/
    // and never reopened from a Path B session).
    if state.global_catalog.lock().await.is_some() {
        persist_path_b(app, state).await;
        return;
    }

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

/// Path B persistence. Snapshots the live `Profile`'s authored items
/// into `catalog.toml` and the in-memory `LoadoutSet` into
/// `loadouts.toml`, both via the same atomic-write-with-backup
/// pipeline the per-profile branch uses. Falls through to the legacy
/// `global.toml` write so theme / font / `dock_layout` edits land on
/// the same path in both modes.
async fn persist_path_b(app: &AppHandle, state: &SharedState) {
    let Ok(dir) = app.path().app_data_dir() else {
        return;
    };

    // Catalog. Pull aliases / triggers / macros directly from the live
    // Profile. The catalog is the authoritative source in Path B mode
    // so an overwrite here is correct — anything the user typed via
    // #alias / Settings made it into Profile and now into the file.
    let (catalog, global_snapshot, scope) = {
        let p = state.profile.lock().await;
        let aliases: Vec<vosh_alias::Alias> = p.aliases.list().into_iter().cloned().collect();
        let triggers: Vec<Trigger> = p.triggers.list();
        let macros: Vec<Macro> = p.macros.clone();
        let catalog = crate::loadout::GlobalCatalog {
            aliases,
            triggers,
            macros,
        };
        let scope = state
            .profile_set
            .lock()
            .await
            .as_ref()
            .map(|s| *s.scope())
            .unwrap_or_default();
        let global = GlobalConfig::from_profile(&p, &scope);
        (catalog, global, scope)
    };
    // `scope` is consumed below when stripping global-scoped fields out
    // of the per-profile snapshot. Holding the binding here so the
    // catalog/loadout writes can run without holding the profile-set
    // lock alongside them.

    // Snapshot the current LoadoutSet from state. We do not mutate the
    // active list here; that gets driven by future loadout_set_active
    // commands. Just persist whatever the runtime currently holds.
    let set_snapshot = state.loadout_set.lock().await.clone();

    if let Err(e) = crate::loadout_store::save_global_catalog(&dir, &catalog) {
        warn!(error = %e, "Path B catalog auto-save failed");
    }
    if let Some(set) = set_snapshot {
        if let Err(e) = crate::loadout_store::save_loadout_set(&dir, &set) {
            warn!(error = %e, "Path B loadout set auto-save failed");
        }
    }

    let (global_path, per_profile_path) = {
        let guard = state.profile_set.lock().await;
        match guard.as_ref() {
            Some(set) => (Some(set.global_path()), Some(set.active_path())),
            None => (None, None),
        }
    };
    if let Some(g) = global_path {
        if let Err(e) = global_snapshot.save(&g) {
            warn!(error = %e, path = %g.display(), "Path B global auto-save failed");
        }
    }
    // Per-profile snapshot. Before this, Path B only wrote catalog /
    // loadouts / global, which meant every UI field outside the five
    // scope-controlled ones (tracked_affects, theme_terminal_colors,
    // vitals config, custom themes, paste pacing, moons position,
    // chip style, side-panels fill, split-divider color, enabled
    // presets, dock layout when its scope is profile, ...) silently
    // dropped on every quit. Writing a per-profile file lets these
    // persist per-loadout the same way legacy mode does. The catalog
    // stays authoritative for aliases / triggers / macros, so we
    // blank those out of the per-profile snapshot before saving —
    // otherwise switching to an older loadout would resurrect that
    // loadout's stale alias copy and override fresher catalog edits.
    if let Some(p) = per_profile_path {
        let mut per_profile_snapshot = {
            let live = state.profile.lock().await;
            ProfileConfig::from_profile(&live)
        };
        per_profile_snapshot.aliases.clear();
        per_profile_snapshot.triggers.clear();
        per_profile_snapshot.disabled_alias_groups.clear();
        per_profile_snapshot.disabled_trigger_groups.clear();
        per_profile_snapshot.disabled_macro_groups.clear();
        strip_global_fields(&mut per_profile_snapshot, &scope);
        if let Err(e) = per_profile_snapshot.save(&p) {
            warn!(
                error = %e,
                path = %p.display(),
                "Path B per-profile auto-save failed",
            );
        }
    }
    // Mirror the new catalog into `state.global_catalog` so subsequent
    // reads see the latest write without going back to disk.
    *state.global_catalog.lock().await = Some(catalog);
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
    let (mut result, target_after, script_apply) = {
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
        // Run any Lua bodies queued by script-bodied aliases that
        // fired during expansion. The actions they produce (sends,
        // echoes, var sets, etc.) fold into the same ScriptOutcome
        // pipeline that the Lua-registered triggers use, and the
        // resulting ApplyResult is appended to this input's
        // bytes / echo so the user sees one coherent response.
        let script_apply = if result.scripts.is_empty() {
            None
        } else {
            let mut combined = vosh_script::ScriptOutcome::default();
            for call in &result.scripts {
                match script_state::eval_with_captures(
                    &mut profile.script,
                    &call.body,
                    &call.captures,
                    "alias-script",
                ) {
                    Ok(o) => combined.actions.extend(o.actions),
                    Err(err) => {
                        tracing::warn!(error = %err, "alias script eval failed");
                    }
                }
            }
            Some(script_state::apply_actions(&mut profile, combined))
        };
        (result, payload, script_apply)
    };

    if let Some(apply) = script_apply {
        // Lua actions append AFTER the alias's template output (which
        // is currently empty for script-bodied aliases since the
        // body owns the response). Echoes go through the same local
        // echo path as the alias's own echoes; send bytes append to
        // the wire payload.
        result.echo.extend(apply.echoes);
        result.bytes.extend(apply.send_bytes);
    }

    if let Some(payload) = target_after {
        let _ = app.emit("session://target", payload);
    }

    if !result.echo.is_empty() {
        let mut buf = Vec::new();
        for line in &result.echo {
            buf.extend_from_slice(line.as_bytes());
            buf.extend_from_slice(b"\r\n");
        }
        let _ = app.emit("session://output", OutputPayload::from_bytes(&buf));
    }

    if result.bytes.is_empty() {
        return Ok(());
    }

    let current = state.session.lock().await;
    let Some(handle) = current.as_ref() else {
        let _ = app.emit(
            "session://output",
            OutputPayload::from_bytes(b"\r\n[not connected]\r\n"),
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

/// Tier 3 native renderer (macOS). The frontend reports the terminal
/// pane's screen rectangle (CSS pixels, top-left origin, relative to the
/// window) and device pixel ratio so the native wgpu surface can track
/// it. NSView/Metal must be touched on the main thread, so the work is
/// dispatched there. A no-op on other platforms and when the surface is
/// not installed.
#[tauri::command]
pub(crate) fn native_surface_set_bounds(
    app: AppHandle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    dpr: f64,
) {
    #[cfg(target_os = "macos")]
    {
        let _ = app.run_on_main_thread(move || {
            crate::native_surface::set_bounds(x, y, width, height, dpr);
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (&app, x, y, width, height, dpr);
    }
}

/// Tier 3 native renderer (macOS): copy the current selection to the
/// clipboard. Used by the Cmd+C / Ctrl+C path; a no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_copy() {
    #[cfg(target_os = "macos")]
    crate::native_surface::request_copy();
}

/// Parse a `#rrggbb` (or `rrggbb`) hex color.
#[cfg(target_os = "macos")]
fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim().trim_start_matches('#');
    if s.len() < 6 {
        return None;
    }
    Some((
        u8::from_str_radix(&s[0..2], 16).ok()?,
        u8::from_str_radix(&s[2..4], 16).ok()?,
        u8::from_str_radix(&s[4..6], 16).ok()?,
    ))
}

/// Tier 3 native renderer (macOS): set the surface theme colors so the
/// background, foreground, and selection follow the active Vosh theme.
/// Colors are `#rrggbb`. A no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_set_theme(
    background: String,
    foreground: String,
    selection: String,
    ansi: Vec<String>,
) {
    #[cfg(target_os = "macos")]
    {
        if let (Some(bg), Some(fg), Some(sel)) = (
            parse_hex(&background),
            parse_hex(&foreground),
            parse_hex(&selection),
        ) {
            crate::cell_render::set_theme(bg, fg, sel);
            let palette: Vec<(u8, u8, u8)> = ansi.iter().filter_map(|s| parse_hex(s)).collect();
            if palette.len() == 16 {
                crate::cell_render::set_palette(&palette);
            }
            crate::native_surface::request_redraw();
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (background, foreground, selection, ansi);
    }
}

/// Tier 3 native renderer (macOS): echo locally-sent input into the grid so
/// the user sees their own commands (xterm gets the same bytes via
/// onLocalEcho). `text` is the already-styled echo line. A no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_echo(text: String) {
    #[cfg(target_os = "macos")]
    {
        crate::term_grid::feed_bytes(text.as_bytes());
        crate::native_surface::request_redraw();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = text;
    }
}

/// Tier 3 native renderer (macOS): rebuild the surface atlas at a new font
/// family and size (CSS px) so it matches the configured Vosh font. A no-op
/// elsewhere.
#[tauri::command]
pub(crate) fn native_surface_set_font(family: String, size: u32) {
    #[cfg(target_os = "macos")]
    crate::native_surface::request_set_font(family, size);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (family, size);
    }
}

/// Tier 3 native renderer (macOS): keyboard scroll. `kind` is "pageup",
/// "pagedown", or "bottom". Scrolls the grid and repaints; a no-op
/// elsewhere.
#[tauri::command]
pub(crate) fn native_surface_scroll(kind: String) {
    #[cfg(target_os = "macos")]
    {
        match kind.as_str() {
            "pageup" => crate::term_grid::scroll_page(true),
            "pagedown" => crate::term_grid::scroll_page(false),
            "bottom" => crate::term_grid::scroll_to_bottom(),
            _ => {}
        }
        crate::native_surface::request_redraw();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = kind;
    }
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
/// Re-apply Path B's catalog + active loadout set onto the live
/// profile. Used after a per-profile load on profile-switch to make
/// sure the catalog (the authoritative source for aliases / triggers /
/// macros in Path B mode) wins over whatever the per-profile file
/// carried — the per-profile file's aliases are blanked out by
/// `persist_path_b`, but the loadout's `enabled_groups` + tick still
/// need to be replayed against the freshly-loaded profile state.
async fn apply_path_b_overlays(state: &SharedState) {
    let catalog = match state.global_catalog.lock().await.as_ref() {
        Some(c) => c.clone(),
        None => return,
    };
    let set = state.loadout_set.lock().await.clone();
    let mut p = state.profile.lock().await;
    let mut aliases = vosh_alias::AliasStore::new();
    for a in &catalog.aliases {
        aliases.set(a.clone());
    }
    p.aliases = aliases;
    let mut triggers = vosh_trigger::TriggerStore::new();
    for t in &catalog.triggers {
        if let Err(e) = triggers.set(t.clone()) {
            warn!(error = %e, "catalog trigger rejected during profile switch");
        }
    }
    p.triggers = triggers;
    p.macros.clone_from(&catalog.macros);
    if let Some(set) = set.as_ref() {
        crate::loadout_store::apply_loadout_state(set, &mut p);
    }
}

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

    // Path B catalog re-overlay. The per-profile file in Path B mode
    // is written with blank aliases / triggers / macros (catalog is
    // authoritative), so after the apply_to above the live profile
    // has empty stores. Re-pour the catalog onto the live profile and
    // re-apply the loadout state so the freshly-switched profile
    // starts from the right place.
    apply_path_b_overlays(state).await;

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
        OutputPayload::from_bytes(line.as_bytes()),
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
    pub input_echo_color: Option<String>,
    pub side_panels_fill_height: bool,
    pub paste_line_delay_ms: u32,
    pub spellcheck_prompt: bool,
    pub prompt_template_enabled: bool,
    pub prompt_template: String,
    pub vitals: crate::profile_config::VitalsConfig,
    pub moons_position: String,
    pub chip_style: String,
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
        input_echo_color: p.ui.input_echo_color.clone(),
        side_panels_fill_height: p.ui.side_panels_fill_height,
        paste_line_delay_ms: p.ui.paste_line_delay_ms,
        spellcheck_prompt: p.ui.spellcheck_prompt,
        prompt_template_enabled: p.ui.prompt_template_enabled,
        prompt_template: p.ui.prompt_template.clone(),
        vitals: p.ui.vitals.clone(),
        moons_position: p.ui.moons_position.clone(),
        chip_style: p.ui.chip_style.clone(),
    })
}

#[tauri::command]
pub(crate) async fn ui_set_config(
    app: AppHandle,
    state: State<'_, SharedState>,
    config: UiConfigPayload,
) -> Result<(), String> {
    // Destructure the single IPC payload into the locals the
    // validation block below consumes. Get and set now share one
    // snake_case shape (`UiConfigPayload`); `dock_layout` stays out of
    // it because it is managed separately via dock_layout_get/set.
    let UiConfigPayload {
        theme,
        auto_update,
        font_family,
        font_size,
        tracked_affects,
        enabled_presets,
        keep_last_command,
        theme_terminal_colors,
        custom_themes,
        split_divider_color,
        input_echo_color,
        side_panels_fill_height,
        paste_line_delay_ms,
        spellcheck_prompt,
        prompt_template_enabled,
        prompt_template,
        vitals,
        moons_position,
        chip_style,
    } = config;
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
        p.ui.input_echo_color = input_echo_color.and_then(|s| {
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
        p.ui.spellcheck_prompt = spellcheck_prompt;
        p.ui.prompt_template_enabled = prompt_template_enabled;
        p.ui.prompt_template = prompt_template;
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
        // Legacy `bar_style: "spark"` migrates to solid + history
        // layout. The spark mode was reframed as a layout that wraps
        // any bar style with a braille trend grid below.
        if v.bar_style == "spark" {
            v.bar_style = "solid".to_string();
            v.bar_layout = "with_history".to_string();
        }
        if v.bar_style != "solid" && v.bar_style != "track" && v.bar_style != "ramped" {
            v.bar_style = "solid".to_string();
        }
        if v.bar_layout != "plain" && v.bar_layout != "with_history" {
            v.bar_layout = "plain".to_string();
        }
        p.ui.vitals = v;
        // Coerce an unknown moons_position value back to "right-edge"
        // so a hand-edited profile.toml typo cannot leave the status
        // bar rendering moons in an unrecognized slot.
        p.ui.moons_position = match moons_position.as_str() {
            "before-time" | "after-time" | "right-edge" => moons_position,
            _ => "right-edge".to_string(),
        };
        // Same coercion for chip_style — an unknown variant from a
        // hand-edited profile.toml falls back to the default rather
        // than letting the frontend render a chip with no style.
        p.ui.chip_style = match chip_style.as_str() {
            "value_only" | "caption_value" | "icon_value" => chip_style,
            _ => "value_only".to_string(),
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

/// Read-only Path B migration preview. Walks the current profile set,
/// loads each per-profile [`ProfileConfig`] off disk, and runs the
/// analyzer in [`crate::migration`]. Returns the full plan: every
/// auto-resolved item, every conflict (one entry per name with two or
/// more diverging variants), and the per-source-profile loadouts the
/// migration would generate. Nothing is written to disk; the wizard
/// uses this for the preview pane only. The companion
/// [`migration_apply`] command commits the plan once the user picks
/// winners for any conflicts.
#[tauri::command]
pub(crate) async fn migration_analyze(
    state: State<'_, SharedState>,
) -> Result<crate::migration::MigrationPlan, String> {
    let guard = state.profile_set.lock().await;
    let Some(set) = guard.as_ref() else {
        return Err("profile set not initialized".into());
    };
    let mut sources: Vec<(String, ProfileConfig)> = Vec::with_capacity(set.list().len());
    for entry in set.list() {
        let path = set.profile_path(&entry.name);
        let cfg = if path.exists() {
            ProfileConfig::load(&path).map_err(|e| e.to_string())?
        } else {
            ProfileConfig::default()
        };
        sources.push((entry.name.clone(), cfg));
    }
    Ok(crate::migration::analyze_profiles(&sources))
}

/// One conflict resolution from the wizard. Identifies a single
/// conflicted item (kind + name) and the source profile whose variant
/// should win. Resolutions not present in the list fall back to the
/// first variant in the conflict (the analyzer iterates source
/// profiles in index order, so this is deterministic).
#[derive(Debug, serde::Deserialize)]
pub(crate) struct ConflictResolution {
    pub kind: crate::migration::ItemKind,
    pub name: String,
    pub source_profile: String,
}

/// Commit the Path B migration. Re-runs the analyzer, applies the
/// user's per-conflict resolutions (or the first-variant default for
/// any missing resolution), writes `catalog.toml` + `loadouts.toml`,
/// moves every existing per-profile file into `profiles/legacy/`, and
/// restarts the app so the startup hook picks up Path B mode. The
/// previously-active profile name (from the index) becomes the sole
/// initial active loadout so the user's first post-restart session
/// keeps the same authoring set live.
#[tauri::command]
pub(crate) async fn migration_apply(
    app: AppHandle,
    state: State<'_, SharedState>,
    resolutions: Vec<ConflictResolution>,
) -> Result<(), String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;

    let previously_active = {
        let guard = state.profile_set.lock().await;
        guard.as_ref().map(|s| s.active_name().to_string())
    };

    // Re-load sources from disk — the analyze call has to walk the
    // same set the user just previewed, but a few seconds may have
    // passed and we want the fresh snapshot rather than caching across
    // commands.
    let sources = {
        let guard = state.profile_set.lock().await;
        let Some(set) = guard.as_ref() else {
            return Err("profile set not initialized".into());
        };
        let mut sources = Vec::with_capacity(set.list().len());
        for entry in set.list() {
            let path = set.profile_path(&entry.name);
            let cfg = if path.exists() {
                ProfileConfig::load(&path).map_err(|e| e.to_string())?
            } else {
                ProfileConfig::default()
            };
            sources.push((entry.name.clone(), cfg));
        }
        sources
    };

    let plan = crate::migration::analyze_profiles(&sources);
    let mut catalog = plan.auto_resolved;
    for conflict in &plan.conflicts {
        let chosen_source = resolutions
            .iter()
            .find(|r| r.kind == conflict.kind && r.name == conflict.name)
            .map_or_else(
                || conflict.variants[0].source_profile.as_str(),
                |r| r.source_profile.as_str(),
            );
        let chosen = conflict
            .variants
            .iter()
            .find(|v| v.source_profile == chosen_source)
            .ok_or_else(|| {
                format!(
                    "resolution for `{}` points to unknown source `{}`",
                    conflict.name, chosen_source
                )
            })?;
        match &chosen.item {
            crate::migration::ItemPayload::Alias { item } => catalog.aliases.push(item.clone()),
            crate::migration::ItemPayload::Trigger { item } => catalog.triggers.push(item.clone()),
            crate::migration::ItemPayload::Macro { item } => catalog.macros.push(item.clone()),
        }
    }

    let mut loadout_set = crate::loadout::LoadoutSet {
        loadouts: plan.loadouts,
        active: Vec::new(),
    };
    if let Some(name) = previously_active {
        if loadout_set.loadouts.iter().any(|l| l.name == name) {
            loadout_set.active.push(name);
        }
    }

    crate::loadout_store::save_global_catalog(&app_data, &catalog).map_err(|e| e.to_string())?;
    crate::loadout_store::save_loadout_set(&app_data, &loadout_set).map_err(|e| e.to_string())?;

    // Move profiles/<name>.toml into profiles/legacy/. Keep the index
    // file in place — it does not interfere with Path B mode and gives
    // a recoverable rollback if the user reverts. Each move uses
    // rename; cross-device or in-use cases fall through to copy-then-
    // delete via the standard library's fallback.
    let profiles_dir = app_data.join("profiles");
    let legacy_dir = profiles_dir.join("legacy");
    if let Err(e) = std::fs::create_dir_all(&legacy_dir) {
        return Err(format!(
            "failed to create legacy dir at {}: {e}",
            legacy_dir.display()
        ));
    }
    for (name, _) in &sources {
        let src = profiles_dir.join(format!("{name}.toml"));
        if !src.exists() {
            continue;
        }
        let dst = legacy_dir.join(format!("{name}.toml"));
        if let Err(e) = std::fs::rename(&src, &dst) {
            // Cross-device fallback: copy then remove.
            std::fs::copy(&src, &dst).map_err(|copy_err| {
                format!(
                    "failed to move {} to {}: {e}; copy fallback also failed: {copy_err}",
                    src.display(),
                    dst.display(),
                )
            })?;
            let _ = std::fs::remove_file(&src);
        }
    }

    // Returning Ok rather than calling `app.restart()` here. Restart
    // is fragile in dev mode: it tears down the binary out from under
    // the `tauri dev` watcher and leaves the next process trying to
    // load a frontend whose Vite dev server may have been killed
    // with the parent, ending in a hidden window with no JS reveal.
    // The frontend shows a "migration complete, please relaunch"
    // banner and offers an explicit [Quit Vosh] button (handled
    // separately by app_quit) that cleanly exits the process. The
    // user re-opens Vosh and the Path B startup hook picks the new
    // catalog up. Path B mode is durable on disk either way.
    let _ = app.emit("vosh://migration-applied", &());
    Ok(())
}

/// Cleanly exit the app. Surfaces a "quit" event first so any window
/// can flush state, then calls `app.exit(0)`. Used by the post-
/// migration prompt to take the user out of the legacy-mode session
/// in one click; on relaunch the Path B startup hook picks up the
/// new catalog.
#[tauri::command]
pub(crate) async fn app_quit(app: AppHandle) -> Result<(), String> {
    app.exit(0);
    Ok(())
}

/// One loadout as the frontend cares about it: the user-visible
/// identifying fields, the `enabled_groups` list (chips for the picker),
/// and the auto-match block.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct LoadoutSummary {
    pub name: String,
    pub description: Option<String>,
    pub enabled_groups: Vec<String>,
    pub auto_match: Option<crate::profile_set::AutoMatch>,
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

/// Replace the active-loadouts list and recompute every store's
/// `disabled_groups` from the new union. Persists the loadout set
/// to disk and emits a state-changed event so other windows (e.g. a
/// future `TopBar` checklist) see the update.
#[tauri::command]
pub(crate) async fn loadouts_set_active(
    app: AppHandle,
    state: State<'_, SharedState>,
    active: Vec<String>,
) -> Result<(), String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    {
        let mut guard = state.loadout_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err("Path B not active".into());
        };
        // Filter to known loadout names. A stale name (e.g. from a
        // future-truncated payload) is silently dropped rather than
        // returning an error.
        set.active = active
            .into_iter()
            .filter(|n| set.loadouts.iter().any(|l| &l.name == n))
            .collect();
        let snapshot = set.clone();
        let mut p = state.profile.lock().await;
        crate::loadout_store::apply_loadout_state(&snapshot, &mut p);
        if let Err(e) = crate::loadout_store::save_loadout_set(&app_data, &snapshot) {
            warn!(error = %e, "loadouts.toml save failed");
        }
    }
    let _ = app.emit("vosh://loadouts-changed", &());
    Ok(())
}

/// Snapshot of the per-session tick timer config. Mirrors
/// `tick::TickConfig` with `Duration` flattened to a `u64` of seconds
/// so the frontend can edit it cleanly. Reset pattern, auto-fire
/// command, warning timer / message / color are all optional — empty
/// means the feature is off.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct TickConfigPayload {
    pub enabled: bool,
    pub interval_secs: u64,
    pub auto_fire: Option<String>,
    pub sound: bool,
    pub reset_pattern: Option<String>,
    pub warn_at_secs: Option<u64>,
    pub warn_message: Option<String>,
    pub warn_color: Option<String>,
}

/// Read the live tick configuration.
#[tauri::command]
pub(crate) async fn tick_get_config(
    state: State<'_, SharedState>,
) -> Result<TickConfigPayload, String> {
    let p = state.profile.lock().await;
    let cfg = &p.tick.config;
    Ok(TickConfigPayload {
        enabled: cfg.enabled,
        interval_secs: cfg.interval.as_secs(),
        auto_fire: cfg.auto_fire.clone(),
        sound: cfg.sound,
        reset_pattern: cfg.reset_pattern.clone(),
        warn_at_secs: cfg.warn_at_secs,
        warn_message: cfg.warn_message.clone(),
        warn_color: cfg.warn_color.clone(),
    })
}

/// Apply a new tick configuration. Routes interval changes through
/// `TickRuntime::set_interval` so the next-fire deadline rebuilds,
/// and reset-pattern changes through `set_reset_pattern` so the
/// regex re-compiles (an invalid regex returns the underlying error
/// to the caller). Other fields are direct assignments. Persists the
/// active profile and broadcasts `vosh://tick-config-changed`.
#[tauri::command]
pub(crate) async fn tick_set_config(
    app: AppHandle,
    state: State<'_, SharedState>,
    config: TickConfigPayload,
) -> Result<TickConfigPayload, String> {
    // Normalize string options: empty / whitespace-only -> None so the
    // persisted state does not carry an empty placeholder.
    let auto_fire = config
        .auto_fire
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let reset_pattern = config
        .reset_pattern
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let warn_message = config
        .warn_message
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let warn_color = config
        .warn_color
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let snapshot = {
        let mut p = state.profile.lock().await;
        let now = tokio::time::Instant::now();
        if config.enabled {
            if !p.tick.config.enabled {
                p.tick.enable(now);
            }
            p.tick.set_interval(config.interval_secs, now);
        } else {
            p.tick.disable();
            // Still record the interval so the user can flip enabled
            // back on without re-typing it.
            p.tick.config.interval = std::time::Duration::from_secs(config.interval_secs.max(1));
        }
        p.tick
            .set_reset_pattern(reset_pattern.clone())
            .map_err(|e| format!("invalid reset pattern: {e}"))?;
        p.tick.config.auto_fire.clone_from(&auto_fire);
        p.tick.config.sound = config.sound;
        p.tick.config.warn_at_secs = config.warn_at_secs.filter(|s| *s > 0);
        p.tick.config.warn_message.clone_from(&warn_message);
        p.tick.config.warn_color.clone_from(&warn_color);

        TickConfigPayload {
            enabled: p.tick.config.enabled,
            interval_secs: p.tick.config.interval.as_secs(),
            auto_fire: auto_fire.clone(),
            sound: p.tick.config.sound,
            reset_pattern: reset_pattern.clone(),
            warn_at_secs: p.tick.config.warn_at_secs,
            warn_message: warn_message.clone(),
            warn_color: warn_color.clone(),
        }
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast(&app, "vosh://tick-config-changed", &snapshot);
    Ok(snapshot)
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

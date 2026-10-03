//! Tauri commands invoked by the frontend.

use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tracing::warn;
use vosh_automation::trigger::Trigger;
use vosh_log::{SearchOptions, SearchPage, SessionRow};

use crate::app::events::{
    self, broadcast, broadcast_list_changes, pane_layout_envelope, AffectsDisplay, ListChanges,
    ListRevisions, PaneLayoutEnvelope, AFFECTS_DISPLAY_CHANGED, CHAT_COLORS_CHANGED,
    CUSTOM_THEMES_CHANGED, HELP_OPEN, LOADOUTS_CHANGED, MACROS_CHANGED, MACRO_GROUPS_CHANGED,
    PANE_LAYOUT_CHANGED, PROFILES_CHANGED, TICK_CONFIG_CHANGED, TIMERS_CHANGED,
    TRACKED_AFFECTS_CHANGED,
};
use crate::app::state::{
    note_ui_config_replaced, panes_generation, ui_config_generation, AppState, SharedState,
    AUTO_PERSIST_SUPPRESSED, MIGRATION_RELAUNCH_PENDING, PROFILES_NOT_LOADED,
};
use crate::disk::save::{
    mark_profile_dirty, persist_profile, persist_profile_locked, persist_state,
    schedule_profile_persist, settle_line_effects, PERSIST_LOCK,
};
use crate::input;
use crate::loadouts::wizard::apply::{
    analyze_migration, announce_migration_applied, apply_migration, ConflictResolution,
};

use crate::profile::switch::{apply_profile_switch, read_shared_layer};
use crate::profile::{Macro, Profile, Timer};
use crate::profile_config::{
    hand_out_shared, share_custom_themes, GlobalConfig, HeldCustomThemes, PaneLayoutPersist,
};
use crate::script_state::ApplyResult;
use crate::session::{self, TargetPayload};
use crate::tick::{apply_tick_config, tick_config_payload, TickConfigPayload};

/// What launch has to tell you, for the main window to show once in the
/// terminal and as a toast.
#[tauri::command]
pub(crate) fn launch_notices_take(state: State<'_, SharedState>) -> Vec<String> {
    state.take_launch_notices()
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
    // The old session cleared the list as it ended. A new connection
    // starts with none until the MUD sends its own.
    state.last_affects.clear();
    crate::affect_full::connect(&app, state.inner());

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
    let target = (host.clone(), port);

    let spawned = session::spawn(
        app.clone(),
        host,
        port,
        tls,
        state.profile.clone(),
        state.script_timers.clone(),
        state.logs.clone(),
        state.scrollback.clone(),
        scrollback_path,
        initial_size,
    )
    .await;
    let handle = match spawned {
        Ok(handle) => handle,
        Err(e) => {
            // Surface the disconnected state so the UI does not stay stuck
            // on "connecting...". The frontend listens for session://state.
            let _ = app.emit(
                events::STATE,
                crate::session::StatePayload::Disconnected {
                    reason: Some(e.to_string()),
                },
            );
            // Nothing reached the target, so nobody is logged in there.
            // A connect that raced this one keeps its own target.
            if let Ok(mut g) = state.current_connection.lock() {
                if g.as_ref() == Some(&target) {
                    *g = None;
                }
            }
            crate::characters::broadcast_session_identity(&app, state.inner()).await;
            return Err(e.to_string());
        }
    };

    {
        let mut current = state.session.lock().await;
        if let Some(prev) = current.take() {
            // A concurrent connect raced us. Shut down our old handle.
            prev.shutdown().await;
        }
        *current = Some(handle);
    }
    crate::characters::broadcast_session_identity(&app, state.inner()).await;
    Ok(())
}

/// What the terminal prints when you send a line with no connection.
const NOT_CONNECTED: &[u8] = b"\r\n[not connected]\r\n";

/// Print the lines a typed line echoes, such as a slash command's
/// reply, one to a row. They go through [`session::emit_output`] like
/// every other terminal write, so the native renderer shows them too.
pub(crate) fn echo_lines<R: tauri::Runtime>(app: &AppHandle<R>, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    let mut buf = Vec::new();
    for line in lines {
        buf.extend_from_slice(line.as_bytes());
        buf.extend_from_slice(b"\r\n");
    }
    session::emit_output(app, buf);
}

#[tauri::command]
pub(crate) async fn session_send_input<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    line: String,
) -> Result<(), String> {
    // `#help <words>` opens Help on those words. The topics live in the
    // page, so the main window searches them, opens Help on the best
    // match, or says in the terminal that none matched.
    if let Some(words) = crate::input::help_query(&line) {
        return app
            .emit_to("main", HELP_OPEN, words)
            .map_err(|e| e.to_string());
    }
    // `#logs` works on the log store, not the profile, and can take a
    // while on a large log, so it runs on its own task and echoes when
    // done.
    if let Some(command) = crate::input::logs_command(&line) {
        crate::forget_passwords::start(&app, command);
        return Ok(());
    }
    // `#profile reset` and `#profile load` replace the live profile
    // wholesale, panes and tracked affects included. Path B turns them
    // into echoes, so there they change nothing.
    let mut effects = crate::input::LineEffects::default();
    // The profile file they read holds none of the shared settings, so
    // global.toml goes back over the result the way a switch lays it.
    let shared_layer = if crate::input::may_replace_profile(&line) {
        read_shared_layer(state.inner()).await
    } else {
        None
    };
    let (apply, target_after, look_changed) = {
        let mut profile = state.profile.lock().await;
        let lists_before = ListRevisions::of(&profile);
        let look_before = prompt_look(&profile);
        let before_name = profile.target.name.clone();
        let before_idx = profile.target.room_idx;
        let before_keys = profile.target.quick_keys.clone();
        let ran = match &shared_layer {
            Some(layer) => layer.keep_across(&mut profile, |p| input::run_line(p, &line)),
            None => input::run_line(&mut profile, &line),
        };
        // Only a reset, or a load that read its file, replaced the
        // profile. A load that failed leaves it for the saves to write.
        effects.note_ran(&line, &ran);
        if ran.replaced {
            note_ui_config_replaced();
        }
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
        // The line's own bytes and echo lines, with what the Lua bodies
        // of its script aliases send among them, then all else the Lua it
        // ran asks for. #trigger, #alias, and the Lua they run change the
        // lists an open Settings page shows, so the result carries every
        // list the line changed.
        let mut apply = session::line_script_result(ran);
        apply.lists = ListChanges::since(lists_before, &profile);
        let look_changed = prompt_look(&profile) != look_before;
        (apply, payload, look_changed)
    };
    // `#prompt draw` and `#prompt show` change the prompt on screen at
    // once, and `#prompt default` draws the new design there.
    if look_changed {
        request_prompt_repaint(state.inner()).await;
    }

    settle_line_effects(&app, effects).await;

    if let Some(payload) = target_after {
        let _ = app.emit(events::TARGET, payload);
    }

    deliver_script_result(&app, state.inner(), apply).await
}

/// Apply a script result outside the session loop, the way every path
/// applies one, then print its echo lines on the terminal and send its
/// bytes to the game. With no connection the terminal says so.
async fn deliver_script_result<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    apply: ApplyResult,
) -> Result<(), String> {
    let (bytes, echoes) =
        session::collect_script_result(app, &state.profile, &state.script_timers, apply).await;
    echo_lines(app, &echoes);

    if bytes.is_empty() {
        return Ok(());
    }

    let mut current = state.session.lock().await;
    if let Some(handle) = current.as_ref() {
        if handle.send(bytes) {
            return Ok(());
        }
        // The game closed the connection and the session ended, but its
        // handle stayed here. A send fails only once the session loop has
        // returned, so its teardown is done and nothing needs to wait on
        // it. Take the handle out, so this line and every one after it
        // finds no connection, as after a disconnect.
        *current = None;
    }
    session::emit_output(app, NOT_CONNECTED.to_vec());
    Ok(())
}

/// What decides how your prompt looks on screen: the switch, the design
/// and where it shows. A line that changes any of them repaints it.
pub(crate) fn prompt_look(p: &crate::profile::Profile) -> (bool, String, vosh_prompt::PromptShow) {
    let config = p.prompt.config();
    (config.draw, config.template.clone(), config.show)
}

/// Send a line typed into the masked password field, the one the input
/// row shows while the server holds echo. The line goes to the server
/// exactly as typed and skips the input pipeline, so no alias, variable,
/// macro recording, Lua alias body, or `#` command sees it and nothing
/// of it echoes to the terminal. The session log keeps `> (hidden)` in
/// its place.
#[tauri::command]
pub(crate) async fn session_send_masked<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    line: String,
) -> Result<(), String> {
    let current = state.session.lock().await;
    let Some(handle) = current.as_ref() else {
        session::emit_output(&app, NOT_CONNECTED.to_vec());
        return Ok(());
    };
    if !handle.send_masked(crate::hidden_input::masked_line_bytes(&line)) {
        return Err("session task gone".to_string());
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn session_disconnect(
    app: AppHandle,
    state: State<'_, SharedState>,
) -> Result<(), String> {
    {
        let mut current = state.session.lock().await;
        if let Some(handle) = current.take() {
            handle.shutdown().await;
        }
    }
    if let Ok(mut g) = state.current_connection.lock() {
        *g = None;
    }
    if let Ok(mut g) = state.current_character.lock() {
        *g = None;
    }
    crate::characters::broadcast_session_identity(&app, state.inner()).await;
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
    let lists;
    {
        let mut p = state.profile.lock().await;
        let lists_before = ListRevisions::of(&p);
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
            p.vars
                .set(vosh_automation::vars::Scope::Profile, k.clone(), v.clone());
        }
        macros_snapshot = p.macros.clone();
        lists = ListChanges::since(lists_before, &p);
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    if macros_changed {
        broadcast(&app, MACROS_CHANGED, &macros_snapshot);
    }
    broadcast_list_changes(&app, lists);
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

/// Snapshot of every keyboard macro binding. Used by the Settings
/// macros tab to render the existing list and by Input.tsx (via the
/// same payload) to seed its in-memory binding lookup before any
/// `vosh://macros-changed` event fires.
#[tauri::command]
pub(crate) async fn macros_list(state: State<'_, SharedState>) -> Result<Vec<Macro>, String> {
    let p = state.profile.lock().await;
    Ok(p.macros.clone())
}

/// Set or replace a binding by key. Empty `command` is rejected;
/// callers that want to unbind should use `macros_delete`.
/// Re-binding an existing key overwrites the prior command. `enabled`
/// turns the binding on or off without unbinding it. Absent keeps an
/// existing binding's state and makes a new binding on.
#[tauri::command]
pub(crate) async fn macros_set(
    app: AppHandle,
    state: State<'_, SharedState>,
    key: String,
    command: String,
    group: Option<String>,
    enabled: Option<bool>,
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
            if let Some(enabled) = enabled {
                existing.enabled = enabled;
            }
        } else {
            p.macros.push(Macro {
                key,
                command,
                group,
                enabled: enabled.unwrap_or(true),
            });
        }
        p.macros.clone()
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast(&app, MACROS_CHANGED, &updated);
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
    broadcast(&app, MACROS_CHANGED, &updated);
    Ok(updated)
}

/// List every interval timer, in stored order.
#[tauri::command]
pub(crate) async fn timers_list(state: State<'_, SharedState>) -> Result<Vec<Timer>, String> {
    let p = state.profile.lock().await;
    Ok(p.timers.clone())
}

/// Create or update an interval timer. A `None` id creates a new timer
/// (assigned the next free id); an existing id updates in place. The
/// interval is clamped to at least one second. Returns the full list.
#[tauri::command]
pub(crate) async fn timers_set(
    app: AppHandle,
    state: State<'_, SharedState>,
    id: Option<u32>,
    name: String,
    interval_secs: u32,
    command: String,
    enabled: bool,
) -> Result<Vec<Timer>, String> {
    let name = name.trim().to_string();
    let command = command.trim().to_string();
    if command.is_empty() {
        return Err("command cannot be empty".into());
    }
    let interval_secs = interval_secs.max(1);
    let updated = {
        let mut p = state.profile.lock().await;
        match id.and_then(|wanted| p.timers.iter_mut().find(|t| t.id == wanted)) {
            Some(existing) => {
                existing.name = name;
                existing.interval_secs = interval_secs;
                existing.command = command;
                existing.enabled = enabled;
            }
            None => {
                let next_id = p.timers.iter().map(|t| t.id).max().unwrap_or(0) + 1;
                p.timers.push(Timer {
                    id: next_id,
                    name,
                    interval_secs,
                    command,
                    enabled,
                });
            }
        }
        p.timers.clone()
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast(&app, TIMERS_CHANGED, &updated);
    Ok(updated)
}

/// Remove a timer by id. No-op when the id is not present.
#[tauri::command]
pub(crate) async fn timers_delete(
    app: AppHandle,
    state: State<'_, SharedState>,
    id: u32,
) -> Result<Vec<Timer>, String> {
    let updated = {
        let mut p = state.profile.lock().await;
        p.timers.retain(|t| t.id != id);
        p.timers.clone()
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast(&app, TIMERS_CHANGED, &updated);
    Ok(updated)
}

/// One entry in the macro groups list: name + whether the group is
/// currently enabled. The command line reads it to know which macro
/// keys fire.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct GroupState {
    pub name: String,
    pub enabled: bool,
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
pub(crate) async fn triggers_export(state: State<'_, SharedState>) -> Result<String, String> {
    let p = state.profile.lock().await;
    p.triggers.export_json().map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn triggers_import(
    app: AppHandle,
    state: State<'_, SharedState>,
    json: String,
) -> Result<usize, String> {
    let count = {
        let mut p = state.profile.lock().await;
        p.triggers.import_json(&json).map_err(|e| e.to_string())?
    };
    // The editor's save path lands here: persist, or the "saved" state
    // lives only in memory and vanishes on restart.
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast_list_changes(&app, ListChanges::TRIGGERS);
    Ok(count)
}

/// Dump every alias to a pretty JSON array. Mirrors `triggers_export`,
/// so the Aliases and Triggers editors in Settings load their lists the
/// same way, through automationRecords.ts and automationTriggers.ts.
#[tauri::command]
pub(crate) async fn aliases_export(state: State<'_, SharedState>) -> Result<String, String> {
    let p = state.profile.lock().await;
    aliases_json(&p.aliases)
}

/// The JSON `aliases_export` sends, sorted by name. The page reads it in
/// the palette and the Aliases editor, and its tests read
/// `fixtures/ipc/aliases_export.json`, which a test here holds to it.
fn aliases_json(store: &vosh_automation::alias::AliasStore) -> Result<String, String> {
    let aliases: Vec<vosh_automation::alias::Alias> = store.list().into_iter().cloned().collect();
    serde_json::to_string_pretty(&aliases).map_err(|e| e.to_string())
}

/// Replace the entire alias store with the JSON-decoded list. Returns
/// the count installed. Invalid JSON or wrong shape rejects without
/// touching the store.
#[tauri::command]
pub(crate) async fn aliases_import(
    app: AppHandle,
    state: State<'_, SharedState>,
    json: String,
) -> Result<usize, String> {
    let parsed: Vec<vosh_automation::alias::Alias> =
        serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let count = parsed.len();
    {
        let mut p = state.profile.lock().await;
        let mut store = vosh_automation::alias::AliasStore::new();
        for alias in parsed {
            store.set(alias);
        }
        // The disabled-groups set is user state about GROUPS, not items;
        // replacing the store without carrying it over silently
        // re-enabled every disabled group on each editor save.
        store.set_disabled_groups(p.aliases.disabled_groups());
        p.aliases = store;
    }
    // Same persistence rule as triggers_import: the editor saves here.
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast_list_changes(&app, ListChanges::ALIASES);
    Ok(count)
}

/// Tier 3 native renderer (macOS). The frontend reports the terminal
/// pane's screen rectangle (CSS pixels, top-left origin, relative to the
/// window) and device pixel ratio so the native wgpu surface can track
/// it. NSView/Metal must be touched on the main thread, so the work is
/// dispatched there. A no-op on other platforms and when the surface is
/// not installed. `lent` is the rows at the pane's bottom the pinned
/// prompt band borrows while your prompt takes more than one row: the
/// grid gives them up, and the game keeps the size it was told. A page
/// that sends none lends none.
#[tauri::command]
pub(crate) fn native_surface_set_bounds(
    app: AppHandle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    dpr: f64,
    lent: Option<u32>,
) {
    #[cfg(native_surface)]
    {
        let lent = lent.unwrap_or(0);
        let _ = app.run_on_main_thread(move || {
            crate::native_surface::set_bounds(x, y, width, height, dpr, lent);
        });
    }
    #[cfg(not(native_surface))]
    {
        let _ = (&app, x, y, width, height, dpr, lent);
    }
}

/// Tier 3 native renderer, underlay mode (macOS). The webview sits above
/// the surface and receives every click, so the page forwards pointer
/// events over the terminal here. `x` and `y` are CSS px from the pane's
/// top-left corner. `kind` is "down", "drag", "up", "move", "leave", or
/// "middle". `open` carries the Cmd modifier for opening links. The work
/// runs on the main thread, which the renderer requires. A no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_pointer(app: AppHandle, kind: String, x: f64, y: f64, open: bool) {
    #[cfg(native_surface)]
    {
        let _ = app.run_on_main_thread(move || {
            crate::native_surface::forward_pointer(&kind, x, y, open);
        });
    }
    #[cfg(not(native_surface))]
    {
        let _ = (&app, kind, x, y, open);
    }
}

/// Tier 3 native renderer: true once the surface installed and its GPU came
/// up. The page leaves the terminal pane transparent only after this, so a
/// failed install falls back to xterm. False elsewhere.
#[tauri::command]
pub(crate) fn native_surface_ready() -> bool {
    #[cfg(native_surface)]
    {
        crate::native_surface::is_ready()
    }
    #[cfg(not(native_surface))]
    {
        false
    }
}

/// Tier 3 native renderer, underlay mode (macOS): a wheel delta forwarded
/// from the page. Positive reveals older lines. A no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_wheel(app: AppHandle, delta_y: f64) {
    #[cfg(native_surface)]
    {
        let _ = app.run_on_main_thread(move || {
            crate::native_surface::forward_wheel(delta_y);
        });
    }
    #[cfg(not(native_surface))]
    {
        let _ = (&app, delta_y);
    }
}

/// Tier 3 native renderer (macOS): copy the current selection to the
/// clipboard. Used by the Cmd+C / Ctrl+C path; a no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_copy() {
    #[cfg(native_surface)]
    crate::native_surface::request_copy();
}

/// Tier 3 native renderer: select everything in the grid, scrollback
/// included, for the terminal menu's Select all and Cmd+A on an empty
/// command line. Repaints; a no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_select_all() {
    #[cfg(native_surface)]
    {
        crate::term_grid::select_all();
        crate::native_surface::request_redraw();
    }
}

/// Parse a `#rrggbb` (or `rrggbb`) hex color.
#[cfg(native_surface)]
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
    #[cfg(native_surface)]
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
    #[cfg(not(native_surface))]
    {
        let _ = (background, foreground, selection, ansi);
    }
}

/// Tier 3 native renderer: apply the split divider color setting to the
/// surface renderer (hex or `rgb()`/`rgba()`; None restores the default).
#[tauri::command]
pub(crate) fn native_surface_set_divider_color(color: Option<String>) {
    #[cfg(native_surface)]
    {
        let parsed = color
            .as_deref()
            .and_then(crate::cell_render::parse_css_color);
        crate::cell_render::set_divider_color(parsed);
        crate::native_surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    {
        let _ = color;
    }
}

/// Tier 3 native renderer: the chrome colors the page derives with its
/// theme tokens, as CSS colors (hex, or `rgb()`/`rgba()` with alpha). The
/// split divider, the selection, every find match, the current match, a
/// hovered link, the scrollbar thumb, and the selected row fill a lifted
/// prompt's band takes. `appearance` is the theme's, and a light one gives
/// the band its inset ring. Each call replaces the whole set, and a
/// missing or unreadable color falls back to one derived from the terminal
/// palette. The divider setting still wins over `divider`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) fn native_surface_set_tokens(
    divider: Option<String>,
    selection: Option<String>,
    find_match: Option<String>,
    current_match: Option<String>,
    link: Option<String>,
    scrollbar: Option<String>,
    selrow: Option<String>,
    appearance: Option<String>,
) {
    #[cfg(native_surface)]
    {
        let parse = |v: Option<String>| v.as_deref().and_then(crate::cell_render::parse_css_color);
        crate::cell_render::set_tokens(crate::cell_render::ChromeTokens {
            divider: parse(divider),
            selection: parse(selection),
            find_match: parse(find_match),
            current_match: parse(current_match),
            link: parse(link),
            scrollbar: parse(scrollbar),
            selrow: parse(selrow),
            light: appearance.as_deref() == Some("light"),
        });
        crate::native_surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    {
        let _ = (
            divider,
            selection,
            find_match,
            current_match,
            link,
            scrollbar,
            selrow,
            appearance,
        );
    }
}

/// Tier 3 native renderer: draw a band under each lifted prompt while your
/// prompt shows lifted. The grid tags a lift's cells either way.
#[tauri::command]
pub(crate) fn native_surface_set_prompt_bands(on: bool) {
    #[cfg(native_surface)]
    {
        crate::cell_render::set_prompt_bands(on);
        crate::native_surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    {
        let _ = on;
    }
}

/// Tier 3 native renderer: widen the band under the open row by `px` CSS
/// px, so it holds the prompt card's line break mark and caret past the
/// row's last glyph. 0 while the card is closed.
#[tauri::command]
pub(crate) fn native_surface_set_prompt_reach(px: f64) {
    #[cfg(native_surface)]
    {
        #[allow(clippy::cast_possible_truncation)]
        crate::cell_render::set_prompt_reach(px as f32);
        crate::native_surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    {
        let _ = px;
    }
}

/// Tier 3 native renderer (macOS): toggle drawing bright (ANSI 8-15) colored
/// text with the bold font weight. A no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_set_bright_bold(on: bool) {
    #[cfg(native_surface)]
    {
        crate::cell_render::set_bright_bold(on);
        crate::native_surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    {
        let _ = on;
    }
}

/// Tier 3 native renderer: turn blinking text on or off, from the Blinking
/// text setting and the system's reduce motion setting. Off, every
/// blinking cell draws steady. A no-op without the native surface.
#[tauri::command]
pub(crate) fn native_surface_set_blink_text(on: bool) {
    #[cfg(native_surface)]
    {
        crate::cell_render::set_blink_text(on);
        crate::native_surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    {
        let _ = on;
    }
}

/// Tier 3 native renderer (macOS): report xterm's device cell size so the
/// surface grid matches the webview's spacing exactly instead of deriving it
/// from font metrics. `char_height` is xterm's device glyph box, which it
/// centers in a cell taller than the box, so the surface can put its
/// baseline in the same place at every line height. A no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_set_cell_metrics(width: u32, height: u32, char_height: Option<u32>) {
    #[cfg(native_surface)]
    crate::native_surface::set_cell_metrics(width, height, char_height.unwrap_or(0));
    #[cfg(not(native_surface))]
    {
        let _ = (width, height, char_height);
    }
}

/// Tier 3 native renderer (macOS): hide or show the surface so a DOM overlay
/// (dropdown, menu, modal) that would be occluded by the opaque surface
/// shows through. xterm renders the same content behind it. A no-op
/// elsewhere.
#[tauri::command]
pub(crate) fn native_surface_set_visible(visible: bool) {
    #[cfg(native_surface)]
    crate::native_surface::set_visible(visible);
    #[cfg(not(native_surface))]
    {
        let _ = visible;
    }
}

/// Write text the webview drew itself, such as your typed echo or an
/// error notice. The native grid takes it too, as it takes every session
/// write, so it keeps the same content as xterm whichever renderer shows,
/// and the session closes the open row, since that text now follows it.
/// The webview calls it for every such write, on either renderer.
///
/// `after` is the newest output of the prompt stage xterm took before the
/// text, while xterm shows. While the native grid shows it is null, and
/// the grid names its own as it takes the text. The session can hear of
/// the text after it sent later output, since your echo and your line
/// reach it by two calls, and that output stays open.
#[tauri::command]
pub(crate) async fn terminal_local_write(
    state: State<'_, SharedState>,
    text: String,
    after: Option<u64>,
) -> Result<(), String> {
    #[cfg(native_surface)]
    let taken = {
        let taken = crate::term_grid::feed_local(text.as_bytes());
        crate::native_surface::request_redraw();
        taken
    };
    // With no grid, text whose renderer named nothing lands after
    // everything.
    #[cfg(not(native_surface))]
    let taken = {
        let _ = &text;
        u64::MAX
    };
    if let Some(handle) = state.session.lock().await.as_ref() {
        let _ = handle.local_write(after.unwrap_or(taken));
    }
    Ok(())
}

/// You started or stopped selecting text or reading back in xterm. While
/// you do, a clock piece in your design does not repaint your prompt in
/// the text, so the row under your selection or above your reading never
/// moves (decision 6). The native grid holds its own selection and scroll,
/// which the session reads itself.
#[tauri::command]
pub(crate) fn terminal_reader_busy(state: State<'_, SharedState>, busy: bool) {
    state
        .reader_busy
        .store(busy, std::sync::atomic::Ordering::Release);
}

/// Where the native grid's cursor sits and where the open region starts,
/// so the webview can map a pointer to a piece of your prompt while the
/// native renderer draws the terminal (section 6). Lines count from the
/// top of the live screen. Null before the grid exists. xterm reads its
/// own buffer and marker instead.
#[cfg(native_surface)]
#[tauri::command]
pub(crate) fn terminal_cursor() -> Option<crate::term_grid::CursorReport> {
    crate::term_grid::cursor_report()
}

/// No native grid on this build, so there is nothing to report.
#[cfg(not(native_surface))]
#[tauri::command]
pub(crate) fn terminal_cursor() -> Option<()> {
    None
}

/// The native grid's live screen as text, row by row, so the prompt card
/// can find the line the game sent while the profile reads no prompt and
/// no row is open. Null before the grid exists. xterm reads its own
/// buffer instead.
#[cfg(native_surface)]
#[tauri::command]
pub(crate) fn terminal_screen_rows() -> Option<crate::term_grid::ScreenRows> {
    crate::term_grid::screen_rows()
}

/// No native grid on this build, so there is nothing to read.
#[cfg(not(native_surface))]
#[tauri::command]
pub(crate) fn terminal_screen_rows() -> Option<()> {
    None
}

/// Tier 3 native renderer (macOS): search the grid and step to the next (or
/// previous) match, scrolling it into view and highlighting all matches.
/// Returns `[current, total]` (1-based; `[0, 0]` when no match). A no-op
/// returning `[0, 0]` elsewhere.
#[tauri::command]
pub(crate) fn native_surface_find(
    query: String,
    regex: bool,
    case_sensitive: bool,
    whole_word: bool,
    forward: bool,
) -> (usize, usize) {
    #[cfg(native_surface)]
    {
        let result = crate::term_grid::find_run(&query, regex, case_sensitive, whole_word, forward);
        crate::native_surface::request_redraw();
        result
    }
    #[cfg(not(native_surface))]
    {
        let _ = (query, regex, case_sensitive, whole_word, forward);
        (0, 0)
    }
}

/// Tier 3 native renderer (macOS): clear the find highlight. A no-op
/// elsewhere.
#[tauri::command]
pub(crate) fn native_surface_find_clear() {
    #[cfg(native_surface)]
    {
        crate::term_grid::find_clear();
        crate::native_surface::request_redraw();
    }
}

/// Tier 3 native renderer (macOS): rebuild the surface atlas at a new font
/// list and size (CSS px) so it matches the configured Vosh font. `family`
/// is the CSS font list xterm draws with. A no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_set_font(family: String, size: u32) {
    #[cfg(native_surface)]
    crate::native_surface::request_set_font(family, size);
    #[cfg(not(native_surface))]
    {
        let _ = (family, size);
    }
}

/// Tier 3 native renderer (macOS): keyboard scroll. `kind` is "pageup",
/// "pagedown", "bottom", or "toggle". Toggle opens or closes the split
/// the way a middle click does: scrolled back it snaps to the live
/// tail, at the tail it pages up into scrollback. Scrolls the grid and
/// repaints; a no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_scroll(kind: String) {
    #[cfg(native_surface)]
    {
        match kind.as_str() {
            "pageup" => crate::term_grid::scroll_page(true),
            "pagedown" => crate::term_grid::scroll_page(false),
            "bottom" => crate::term_grid::scroll_to_bottom(),
            "toggle" => {
                let (offset, _) = crate::term_grid::scroll_metrics();
                if offset > 0 {
                    crate::term_grid::scroll_to_bottom();
                } else {
                    crate::term_grid::scroll_page(true);
                }
            }
            _ => {}
        }
        crate::native_surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    {
        let _ = kind;
    }
}

/// Read the active profile's pane layout. A profile that has never
/// saved one gets a tree migrated from its dock layout (or the
/// default), with nothing written to disk until the first edit.
#[tauri::command]
pub(crate) async fn pane_layout_get(
    state: State<'_, SharedState>,
) -> Result<PaneLayoutEnvelope, String> {
    let p = state.profile.lock().await;
    Ok(pane_layout_envelope(&p))
}

/// Replace the active profile's pane layout and broadcast the
/// sanitized tree as `vosh://pane-layout-changed` to every window.
/// Splitter drags land here several times a second even after the
/// frontend debounce, so the disk write goes through the debounced
/// `mark_profile_dirty` rather than rotating a backup per drag step.
/// A profile switch or quit flushes it right away.
///
/// `generation` is the one the edited tree was read at. A write made
/// against a profile that has since been swapped out is refused and
/// returns false, and the caller reads the current tree again. An
/// untagged write (a tree that never came from the backend) applies.
#[tauri::command]
pub(crate) async fn pane_layout_set(
    app: AppHandle,
    state: State<'_, SharedState>,
    layout: PaneLayoutPersist,
    generation: Option<u64>,
) -> Result<bool, String> {
    let mut layout = layout;
    layout.sanitize();
    let current = {
        let mut p = state.profile.lock().await;
        let current = panes_generation();
        if generation.is_some_and(|g| g != current) {
            return Ok(false);
        }
        p.ui.panes = Some(layout.clone());
        current
    };
    // A layout tweak after `#profile reset` must not save the blanked
    // profile, so this schedules without clearing the suppression.
    schedule_profile_persist(&app);
    broadcast(
        &app,
        PANE_LAYOUT_CHANGED,
        &PaneLayoutEnvelope {
            layout,
            generation: Some(current),
        },
    );
    Ok(true)
}

/// A window beside the main one that loads the same bundle with its own
/// `?view=`, like Settings and Help. Each opens hidden on the theme's
/// ground and shows itself once its page has painted your theme.
struct AuxWindow {
    /// The window label, which the capabilities and the menu name.
    label: &'static str,
    /// The page the bundle renders, `index.html?view=...`.
    url: &'static str,
    title: &'static str,
    /// The default size, the approved boards' window.
    size: (f64, f64),
    /// The smallest size whose layout still fits.
    min_size: (f64, f64),
}

/// Settings, at the approved boards' 880×600. Under 820×560 its two
/// column layouts no longer fit.
const SETTINGS_WINDOW: AuxWindow = AuxWindow {
    label: "settings",
    url: "index.html?view=settings",
    title: "Settings",
    size: (880.0, 600.0),
    min_size: (820.0, 560.0),
};

/// Help, at the approved Help boards' 1040×700. Under 860 wide the
/// article no longer keeps its measure beside the 280 px sidebar.
const HELP_WINDOW: AuxWindow = AuxWindow {
    label: "help",
    url: "index.html?view=help",
    title: "Help",
    size: (1040.0, 700.0),
    min_size: (860.0, 560.0),
};

/// The logical size a window should take when the window state plugin
/// restored it at `restored`, or None when it already fits. A side under
/// the minimum, saved by an older and smaller window, goes back to the
/// default. The system does not apply the minimum to a size set from
/// code, so this has to.
fn window_fit(window: &AuxWindow, restored: (f64, f64)) -> Option<(f64, f64)> {
    let (width, height) = restored;
    let (min_width, min_height) = window.min_size;
    if width >= min_width && height >= min_height {
        return None;
    }
    Some((
        if width < min_width {
            window.size.0
        } else {
            width
        },
        if height < min_height {
            window.size.1
        } else {
            height
        },
    ))
}

/// Whether opening a window again brings the open one forward now. A
/// window neither on screen nor minimized is still loading. Its page
/// shows it once it has painted your theme, and showing it sooner would
/// put a frame without your theme on screen.
fn shows_on_reopen(visible: bool, minimized: bool) -> bool {
    visible || minimized
}

/// How long a window may stay hidden after an open before the backend
/// shows it anyway. The page shows it well before this, within its own
/// 500 ms fallback once it runs. This covers a page that never gets
/// that far, so the window always opens.
const SHOW_BACKSTOP: std::time::Duration = std::time::Duration::from_secs(2);

/// Show `window` after [`SHOW_BACKSTOP`] if its page has not shown it by
/// then.
fn show_backstop(window: tauri::WebviewWindow) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SHOW_BACKSTOP).await;
        if !window.is_visible().unwrap_or(true) && !window.is_minimized().unwrap_or(false) {
            warn!(
                window = window.label(),
                "the page never showed its window, showing it now"
            );
            let _ = window.show();
            let _ = window.set_focus();
        }
    });
}

/// Open `spec`, or bring it forward when it is already open. The window
/// is a separate webview on the same frontend bundle, and its `?view=`
/// tells the React entry which page to render. Every window shares the
/// one Rust backend state.
fn open_aux_window(app: &AppHandle, spec: &AuxWindow) -> Result<(), String> {
    if let Some(existing) = app.get_webview_window(spec.label) {
        let visible = existing.is_visible().unwrap_or(true);
        let minimized = existing.is_minimized().unwrap_or(false);
        if shows_on_reopen(visible, minimized) {
            existing.show().map_err(|e| e.to_string())?;
            existing.set_focus().map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    let builder = WebviewWindowBuilder::new(app, spec.label, WebviewUrl::App(spec.url.into()))
        .title(spec.title)
        .inner_size(spec.size.0, spec.size.1)
        .min_inner_size(spec.min_size.0, spec.min_size.1)
        .resizable(true)
        .transparent(true)
        // Stay hidden until the page has painted your theme and shows
        // the window itself, so the first frame is never the dark
        // stylesheet defaults.
        .visible(false)
        // Disable Tauri's OS file-drop handler. When enabled it
        // intercepts HTML5 drag-and-drop inside the webview, which
        // can break overlay drag interactions.
        .disable_drag_drop_handler();
    // Open on the theme's appearance, which the last theme paint
    // reported, and on macOS on its ground as well, so even a frame the
    // page has not painted yet is in your theme. Windows and Linux keep
    // the window clear (window_backdrop explains why). Before any paint
    // the window keeps the defaults.
    let builder = match crate::window_backdrop::current() {
        Some(backdrop) => backdrop.dress(builder),
        None => builder,
    };
    // macOS gives the window the main window's titled frame: native
    // traffic lights over the sidebar at the same centers, a hidden
    // title, and the system's corners and rim. Windows and Linux stay
    // frameless, and the page draws its own window controls.
    #[cfg(target_os = "macos")]
    let builder = builder
        .decorations(true)
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true);
    #[cfg(not(target_os = "macos"))]
    let builder = builder.decorations(false);
    let window = builder.build().map_err(|e| e.to_string())?;
    show_backstop(window.clone());
    if let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) {
        let current = size.to_logical::<f64>(scale);
        if let Some((width, height)) = window_fit(spec, (current.width, current.height)) {
            let _ = window.set_size(tauri::LogicalSize::new(width, height));
        }
    }
    Ok(())
}

/// Open (or focus, if already open) the standalone settings window,
/// where the React entry renders `SettingsApp`.
#[tauri::command]
pub(crate) async fn open_settings_window(app: AppHandle) -> Result<(), String> {
    open_aux_window(&app, &SETTINGS_WINDOW)
}

/// Open (or focus, if already open) the Help window, where the React
/// entry renders `HelpApp`. The page that asked leaves the topic or the
/// search it should land on (src/lib/helpLink.ts).
#[tauri::command]
pub(crate) async fn open_help_window(app: AppHandle) -> Result<(), String> {
    open_aux_window(&app, &HELP_WINDOW)
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
        return Err(PROFILES_NOT_LOADED.into());
    };
    Ok(ProfilesListPayload {
        active: set.active_name().to_string(),
        profiles: set.list().to_vec(),
    })
}

/// Write the live profile to its file before a copy of `source` reads
/// that file, when `source` is the live profile. Call with
/// [`PERSIST_LOCK`] held across this and the copy, so the copy reads
/// what the flush wrote and no persist rewrites the source mid copy.
async fn flush_before_copy(shared: &SharedState, app_data: Option<&std::path::Path>, source: &str) {
    let copying_live = shared
        .profile_set
        .lock()
        .await
        .as_ref()
        .is_some_and(|set| set.active_name() == source);
    // The live profile can run two seconds ahead of its file. After
    // `#profile reset` or `load` it is deliberately diverged, and the
    // copy takes the file as it stands.
    if copying_live && !AUTO_PERSIST_SUPPRESSED.load(std::sync::atomic::Ordering::Acquire) {
        persist_state(shared, app_data).await;
    }
}

/// Create a profile with `auto_match` as its login claim, starting as a
/// copy of `copy_from` when given. Returns the new entry and does not
/// switch. The claim takes nothing from other profiles, so a caller
/// that wants the character for itself follows with
/// `profile_set_login`.
#[tauri::command]
pub(crate) async fn profile_create(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
    copy_from: Option<String>,
    auto_match: Option<crate::profile_set::AutoMatch>,
) -> Result<crate::profile_set::ProfileEntry, String> {
    let app_data = app.path().app_data_dir().ok();
    let entry = create_profile(
        state.inner(),
        app_data.as_deref(),
        &name,
        copy_from.as_deref(),
        auto_match,
        &MIGRATION_RELAUNCH_PENDING,
    )
    .await?;
    broadcast(&app, PROFILES_CHANGED, &entry.name);
    Ok(entry)
}

/// The body of [`profile_create`] over the app data folder `app_data`,
/// with `relaunch_pending` in place of [`MIGRATION_RELAUNCH_PENDING`], so
/// a test can run it after the wizard.
pub(crate) async fn create_profile(
    state: &SharedState,
    app_data: Option<&std::path::Path>,
    name: &str,
    copy_from: Option<&str>,
    auto_match: Option<crate::profile_set::AutoMatch>,
    relaunch_pending: &std::sync::atomic::AtomicBool,
) -> Result<crate::profile_set::ProfileEntry, String> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    if let Some(source) = copy_from {
        // Read under the lock, which the wizard holds until it sets the
        // flag.
        if relaunch_pending.load(std::sync::atomic::Ordering::Acquire) {
            return Err(COPY_MIGRATION_PENDING.into());
        }
        flush_before_copy(state, app_data, source).await;
    }
    let mut guard = state.profile_set.lock().await;
    let Some(set) = guard.as_mut() else {
        return Err(PROFILES_NOT_LOADED.into());
    };
    set.create_from(name, copy_from, auto_match)
        .map_err(|e| e.to_string())
}

/// Why a profile cannot be renamed between `migration_apply` and the
/// relaunch that finishes it, or while launch could not finish a wizard
/// run. The next launch writes each profile file the run names under the
/// name it had, and the renamed file would keep what the move took out.
const RENAME_MIGRATION_PENDING: &str =
    "Quit Vosh and open it again to finish the move to loadouts, then rename the profile.";

/// Why a profile cannot be copied in the same window. The copy would take
/// a file the next launch has yet to finish, or the live profile a copy of
/// it saves first, which still holds the items the move took out.
const COPY_MIGRATION_PENDING: &str =
    "Quit Vosh and open it again to finish the move to loadouts, then copy the profile.";

#[tauri::command]
pub(crate) async fn profile_delete(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
) -> Result<(), String> {
    {
        let _persist_guard = PERSIST_LOCK.lock().await;
        let mut guard = state.profile_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err(PROFILES_NOT_LOADED.into());
        };
        set.delete(&name).map_err(|e| e.to_string())?;
    }
    broadcast(&app, PROFILES_CHANGED, &name);
    Ok(())
}

#[tauri::command]
pub(crate) async fn profile_rename(
    app: AppHandle,
    state: State<'_, SharedState>,
    old: String,
    new: String,
) -> Result<(), String> {
    rename_profile(state.inner(), &old, &new, &MIGRATION_RELAUNCH_PENDING).await?;
    broadcast(&app, PROFILES_CHANGED, &new);
    Ok(())
}

/// The body of [`profile_rename`], with `relaunch_pending` in place of
/// [`MIGRATION_RELAUNCH_PENDING`], so a test can run it after the wizard.
pub(crate) async fn rename_profile(
    state: &SharedState,
    old: &str,
    new: &str,
    relaunch_pending: &std::sync::atomic::AtomicBool,
) -> Result<(), String> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    // Read under the lock, which the wizard holds until it sets the flag.
    if relaunch_pending.load(std::sync::atomic::Ordering::Acquire) {
        return Err(RENAME_MIGRATION_PENDING.into());
    }
    let live = {
        let mut guard = state.profile_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err(PROFILES_NOT_LOADED.into());
        };
        let renames_live = set.active_name() == old;
        set.rename(old, new).map_err(|e| e.to_string())?;
        if renames_live {
            state.note_active_profile(set.active_name());
        }
        renames_live.then(|| crate::profile_set::display_name(set.active_name()))
    };
    // The custom prompt draws the live profile's new name.
    if let Some(name) = live {
        state.profile.lock().await.display_name = Some(name);
    }
    Ok(())
}

/// Copy `source` under a new name without its login claim. Duplicating
/// the live profile writes it first, so the copy holds your latest
/// changes.
#[tauri::command]
pub(crate) async fn profile_duplicate(
    app: AppHandle,
    state: State<'_, SharedState>,
    source: String,
    new: String,
) -> Result<(), String> {
    let app_data = app.path().app_data_dir().ok();
    duplicate_profile(
        state.inner(),
        app_data.as_deref(),
        &source,
        &new,
        &MIGRATION_RELAUNCH_PENDING,
    )
    .await?;
    broadcast(&app, PROFILES_CHANGED, &new);
    Ok(())
}

/// The body of [`profile_duplicate`] over the app data folder `app_data`,
/// with `relaunch_pending` in place of [`MIGRATION_RELAUNCH_PENDING`], so
/// a test can run it after the wizard.
pub(crate) async fn duplicate_profile(
    state: &SharedState,
    app_data: Option<&std::path::Path>,
    source: &str,
    new: &str,
    relaunch_pending: &std::sync::atomic::AtomicBool,
) -> Result<(), String> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    // Read under the lock, which the wizard holds until it sets the flag.
    if relaunch_pending.load(std::sync::atomic::Ordering::Acquire) {
        return Err(COPY_MIGRATION_PENDING.into());
    }
    flush_before_copy(state, app_data, source).await;
    let mut guard = state.profile_set.lock().await;
    let Some(set) = guard.as_mut() else {
        return Err(PROFILES_NOT_LOADED.into());
    };
    set.duplicate(source, new).map_err(|e| e.to_string())
}

/// Read the per-category scope map. Frontend uses this to render
/// the toggle row in the Profiles tab.
#[tauri::command]
pub(crate) async fn profile_get_scope(
    state: State<'_, SharedState>,
) -> Result<crate::profile_set::ScopeConfig, String> {
    let guard = state.profile_set.lock().await;
    let Some(set) = guard.as_ref() else {
        return Err(PROFILES_NOT_LOADED.into());
    };
    Ok(*set.scope())
}

/// Update the per-category scope map. After the index is updated,
/// persist the active profile so values move to the correct file
/// (a category flipped Global -> Profile lands in the per-profile
/// file on next save; Profile -> Global lands in global.toml).
///
/// Turning the theme category global also folds the custom themes the
/// other profile files hold into the shared list and clears them from
/// those files, or a switch to one of those profiles would lose them.
/// When the list grows, `vosh://custom-themes-changed` carries it to
/// every window.
///
/// Turning a category per profile first copies the shared values into
/// every other profile file that holds none of its own, since the save
/// drops them from global.toml.
#[tauri::command]
pub(crate) async fn profile_set_scope(
    app: AppHandle,
    state: State<'_, SharedState>,
    scope: crate::profile_set::ScopeConfig,
) -> Result<(), String> {
    // Held from the scope change through the persist, so no other
    // profile file write lands between the moves and the save.
    let persist_guard = PERSIST_LOCK.lock().await;
    let shared: SharedState = state.inner().clone();
    let gained = change_scope_locked(&shared, scope).await?;
    persist_profile_locked(&app, &shared).await;
    drop(persist_guard);
    if let Some(list) = gained {
        broadcast(&app, CUSTOM_THEMES_CHANGED, &list);
    }
    broadcast(&app, PROFILES_CHANGED, &"scope");
    Ok(())
}

/// Why a category cannot stop being shared between `migration_apply`
/// and the relaunch that finishes it. The shared values would have to
/// reach profile files that nothing may write in that window.
const SCOPE_MIGRATION_PENDING: &str =
    "Restart Vosh to finish the move to loadouts, then turn this off.";

/// Why the shared categories cannot change while Vosh holds a file it
/// could not read at launch. The live profile holds the defaults where
/// that file's settings belong, and a change would hand those defaults
/// to the other profiles or share them with every character.
fn scope_refusal_for_unread(set: &crate::profile_set::ProfileSet) -> Option<String> {
    use crate::profile_config::is_unread;
    if is_unread(&set.global_path()) {
        return Some(
            "Vosh could not read global.toml, so it will not change which settings every \
             character shares. Fix the file and restart Vosh."
                .to_string(),
        );
    }
    if is_unread(&set.active_path()) {
        return Some(format!(
            "Vosh could not read the {} profile file, so it will not change which settings \
             every character shares. Fix the file or switch to another profile.",
            crate::profile_set::display_name(set.active_name())
        ));
    }
    None
}

/// The body of [`profile_set_scope`] up to its save. Call with
/// [`PERSIST_LOCK`] held. Returns the live custom themes when turning the
/// theme category global added to them.
pub(crate) async fn change_scope_locked(
    state: &SharedState,
    scope: crate::profile_set::ScopeConfig,
) -> Result<Option<Vec<crate::profile_config::CustomTheme>>, String> {
    use crate::profile_set::Scope;
    let migration_pending = MIGRATION_RELAUNCH_PENDING.load(std::sync::atomic::Ordering::Acquire);
    let before = {
        let guard = state.profile_set.lock().await;
        let set = guard.as_ref().ok_or(PROFILES_NOT_LOADED)?;
        if let Some(refusal) = scope_refusal_for_unread(set) {
            return Err(refusal);
        }
        *set.scope()
    };
    // Every other profile file holds the defaults for a shared category,
    // and the save below drops the category from global.toml, so each
    // file takes the shared values first or that profile opens with the
    // defaults. The live profile holds the shared values.
    if let Some(stopped) = before.stopped_sharing(&scope) {
        if migration_pending {
            return Err(SCOPE_MIGRATION_PENDING.into());
        }
        let values = {
            let p = state.profile.lock().await;
            GlobalConfig::from_profile(&p, &stopped)
        };
        let guard = state.profile_set.lock().await;
        let set = guard.as_ref().ok_or(PROFILES_NOT_LOADED)?;
        hand_out_shared(set, &values)?;
    }
    let (held, global_path) = {
        let mut guard = state.profile_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err(PROFILES_NOT_LOADED.into());
        };
        let theme_was_global = matches!(set.scope().theme, Scope::Global);
        set.set_scope(scope).map_err(|e| e.to_string())?;
        // Nothing may write profile files while a migration relaunch is
        // pending. The next launch moves the themes instead.
        let theme_turned_global =
            !theme_was_global && matches!(scope.theme, Scope::Global) && !migration_pending;
        let held =
            theme_turned_global.then(|| HeldCustomThemes::find(set, Some(set.active_name())));
        (held, set.global_path())
    };
    let mut gained = None;
    if let Some(held) = held {
        let mut p = state.profile.lock().await;
        match share_custom_themes(held, &scope, &global_path, &mut p) {
            Ok(true) => gained = Some(p.ui.custom_themes.clone()),
            Ok(false) => {}
            Err(e) => warn!(error = %e, "custom themes stayed in their profile files"),
        }
    }
    Ok(gained)
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

#[tauri::command]
pub(crate) async fn profile_switch(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
) -> Result<(), String> {
    let shared: SharedState = state.inner().clone();
    apply_profile_switch(&app, &shared, &name).await
}

/// Run `read` on the log store's read connection, or on the writer when
/// the read connection did not open. None when neither is open.
async fn read_logs<T>(state: &AppState, read: impl FnOnce(&vosh_log::LogStore) -> T) -> Option<T> {
    {
        let guard = state.log_reader.lock().await;
        if let Some(store) = guard.as_ref() {
            return Some(read(store));
        }
    }
    let guard = state.logs.lock().await;
    guard.as_ref().map(read)
}

#[tauri::command]
pub(crate) async fn logs_list_sessions(
    state: State<'_, SharedState>,
    limit: usize,
    hide_local: Option<bool>,
) -> Result<Vec<SessionRow>, String> {
    read_logs(&state, |store| {
        store.list_sessions(limit, hide_local.unwrap_or(false))
    })
    .await
    .unwrap_or_else(|| Ok(Vec::new()))
    .map_err(|e| e.to_string())
}

/// One page of the Settings log view: the newest `max_results` matches
/// older than `before_line_id`, oldest first, and with `with_total` the
/// number of lines in that scope that match. The view leaves out
/// sessions to this machine with `hide_local`. A pattern the regex
/// engine cannot read comes back as an error starting `regex:`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn logs_search_page(
    state: State<'_, SharedState>,
    pattern: String,
    case_sensitive: bool,
    max_results: usize,
    session_id: Option<i64>,
    before_line_id: Option<i64>,
    hide_local: bool,
    with_total: bool,
) -> Result<SearchPage, String> {
    let opts = SearchOptions {
        case_sensitive,
        max_results,
        session_id,
        before_line_id,
        hide_local,
    };
    read_logs(&state, |store| {
        store.search_page(&pattern, &opts, with_total)
    })
    .await
    .unwrap_or_else(|| {
        Ok(SearchPage {
            hits: Vec::new(),
            total: with_total.then_some(0),
        })
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn logs_export(
    state: State<'_, SharedState>,
    session_id: i64,
    with_ansi: bool,
) -> Result<String, String> {
    read_logs(&state, |store| store.export_session(session_id, with_ansi))
        .await
        .ok_or_else(|| "log store not ready".to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn scrollback_load(
    state: State<'_, SharedState>,
    feed_native: bool,
) -> Result<ScrollbackLoad, String> {
    let sb = state.scrollback.lock().await;
    // With the run of repeated lines the screen ends on marked, so a pane
    // that loads it during the run rewrites the count in place.
    let bytes = sb.dump_live();
    // The native grid is fed only live output, so the persisted scrollback
    // would be missing there. The live pane asks us to seed it, and only
    // the first ask per process lands. A reloaded page asks again while
    // the grid still holds everything. The seed is claimed even when the
    // scrollback is empty, since the grid then gets every line live.
    #[cfg(native_surface)]
    let seeded_native = feed_native && crate::term_grid::claim_seed() && !bytes.is_empty();
    #[cfg(native_surface)]
    if seeded_native {
        crate::term_grid::feed_local(&bytes);
        crate::native_surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    let seeded_native = {
        let _ = feed_native;
        false
    };
    Ok(ScrollbackLoad {
        bytes,
        seeded_native,
    })
}

/// The persisted scrollback for a mounting terminal, and whether this call
/// also wrote it into the native grid. The page mirrors its restored banner
/// into the grid only when the seed landed here, so a reloaded page, whose
/// grid already holds the history and the first banner, adds no second one.
#[derive(Debug, serde::Serialize)]
pub(crate) struct ScrollbackLoad {
    pub bytes: Vec<u8>,
    pub seeded_native: bool,
}

/// The Settings payload. Every field falls back to the default a fresh
/// profile has, so a page that leaves one out still saves (D12).
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct UiConfigPayload {
    pub theme: String,
    pub follow_system_appearance: bool,
    pub light_theme: String,
    pub dark_theme: String,
    pub auto_update: bool,
    pub font_family: String,
    pub font_size: u32,
    pub terminal_line_height: String,
    pub tracked_affects: Vec<crate::profile_config::TrackedAffect>,
    pub enabled_presets: Vec<String>,
    pub keep_last_command: bool,
    pub theme_terminal_colors: Option<bool>,
    pub bright_bold: bool,
    /// None until you choose.
    #[serde(default)]
    pub blink_text: Option<bool>,
    pub readable_highlights: bool,
    pub collapse_repeats: bool,
    pub terminal_base_ansi: Option<Vec<String>>,
    pub custom_themes: Vec<crate::profile_config::CustomTheme>,
    pub split_divider_color: Option<String>,
    pub input_echo_color: Option<String>,
    pub echo_macros: bool,
    pub input_echo_caret: bool,
    pub paste_line_delay_ms: u32,
    pub spellcheck_prompt: bool,
    pub input_cursor_style: String,
    pub vitals_density: String,
    pub vitals_values: String,
    pub vitals_meter: String,
    pub vitals_warn_thirds: bool,
    pub vitals_hide_when_pinned: bool,
    pub chip_style: String,
    pub tick_count: String,
    pub affects_style: String,
    pub affects_marker: String,
    pub affects_tint: bool,
    #[serde(deserialize_with = "crate::profile_config::deserialize_affects_running_out_hours")]
    pub affects_running_out_hours: u32,
    #[serde(deserialize_with = "crate::profile_config::deserialize_affects_almost_gone_hours")]
    pub affects_almost_gone_hours: u32,
    /// The [`ui_config_generation`] this copy was read at. Never reaches
    /// disk. A save without one (a config that never came from the
    /// backend) applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<u64>,
}

impl Default for UiConfigPayload {
    fn default() -> Self {
        Self::from_ui(&crate::profile_config::UiConfig::default())
    }
}

impl UiConfigPayload {
    /// The snapshot `ui_get_config` hands the frontend.
    pub(crate) fn from_ui(ui: &crate::profile_config::UiConfig) -> Self {
        Self {
            theme: ui.theme.clone(),
            follow_system_appearance: ui.follow_system_appearance,
            light_theme: ui.light_theme.clone(),
            dark_theme: ui.dark_theme.clone(),
            auto_update: ui.auto_update,
            font_family: ui.font_family.clone(),
            font_size: ui.font_size,
            terminal_line_height: ui.terminal_line_height.clone(),
            tracked_affects: ui.tracked_affects.clone(),
            enabled_presets: ui.enabled_presets.clone(),
            keep_last_command: ui.keep_last_command,
            theme_terminal_colors: ui.theme_terminal_colors,
            bright_bold: ui.bright_bold,
            blink_text: ui.blink_text,
            readable_highlights: ui.readable_highlights,
            collapse_repeats: ui.collapse_repeats,
            terminal_base_ansi: ui.terminal_base_ansi.clone(),
            custom_themes: ui.custom_themes.clone(),
            split_divider_color: ui.split_divider_color.clone(),
            input_echo_color: ui.input_echo_color.clone(),
            echo_macros: ui.echo_macros,
            input_echo_caret: ui.input_echo_caret,
            paste_line_delay_ms: ui.paste_line_delay_ms,
            spellcheck_prompt: ui.spellcheck_prompt,
            input_cursor_style: ui.input_cursor_style.clone(),
            vitals_density: ui.vitals_density.clone(),
            vitals_values: ui.vitals_values.clone(),
            vitals_meter: ui.vitals_meter.clone(),
            vitals_warn_thirds: ui.vitals_warn_thirds,
            vitals_hide_when_pinned: ui.vitals_hide_when_pinned,
            chip_style: ui.chip_style.clone(),
            tick_count: ui.tick_count.clone(),
            affects_style: ui.affects_style.clone(),
            affects_marker: ui.affects_marker.clone(),
            affects_tint: ui.affects_tint,
            affects_running_out_hours: ui.affects_running_out_hours,
            affects_almost_gone_hours: ui.affects_almost_gone_hours,
            generation: None,
        }
    }

    /// Write every field onto the live UI config, normalizing as it
    /// goes. `ui_set_config` calls this, and each Settings tab saves the
    /// whole snapshot, so a field left out here would reset on the next
    /// save from any tab. `dock_layout` stays out on purpose, since only
    /// the conversion from the old dock to panes reads it. So do the old
    /// `vitals`, `moons_position` and `side_panels_fill_height`, which
    /// nothing reads and every save writes back as loaded (D12, D14).
    pub(crate) fn apply_to(self, ui: &mut crate::profile_config::UiConfig) {
        let UiConfigPayload {
            theme,
            follow_system_appearance,
            light_theme,
            dark_theme,
            auto_update,
            font_family,
            font_size,
            terminal_line_height,
            tracked_affects,
            enabled_presets,
            keep_last_command,
            theme_terminal_colors,
            bright_bold,
            blink_text,
            readable_highlights,
            collapse_repeats,
            terminal_base_ansi,
            custom_themes,
            split_divider_color,
            input_echo_color,
            echo_macros,
            input_echo_caret,
            paste_line_delay_ms,
            spellcheck_prompt,
            input_cursor_style,
            vitals_density,
            vitals_values,
            vitals_meter,
            vitals_warn_thirds,
            vitals_hide_when_pinned,
            chip_style,
            tick_count,
            affects_style,
            affects_marker,
            affects_tint,
            affects_running_out_hours,
            affects_almost_gone_hours,
            generation: _,
        } = self;
        ui.theme = theme;
        ui.follow_system_appearance = follow_system_appearance;
        // An empty light theme falls back to Vellum. An empty dark theme
        // stays empty, which the frontend reads as the current theme.
        ui.light_theme = match light_theme.trim() {
            "" => "vellum".to_string(),
            id => id.to_string(),
        };
        ui.dark_theme = dark_theme.trim().to_string();
        ui.auto_update = auto_update;
        ui.font_family = font_family;
        ui.font_size = font_size.clamp(6, 64);
        ui.terminal_line_height =
            crate::profile_config::coerce_terminal_line_height(terminal_line_height);
        ui.tracked_affects = crate::profile_config::normalize_tracked_affects(tracked_affects);
        ui.enabled_presets = enabled_presets
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        ui.enabled_presets.sort();
        ui.enabled_presets.dedup();
        ui.keep_last_command = keep_last_command;
        ui.theme_terminal_colors = theme_terminal_colors;
        ui.bright_bold = bright_bold;
        ui.blink_text = blink_text;
        ui.readable_highlights = readable_highlights;
        ui.collapse_repeats = collapse_repeats;
        ui.terminal_base_ansi = terminal_base_ansi;
        ui.custom_themes = custom_themes;
        // Empty strings get normalized to None so the picker can clear
        // back to the theme default by submitting "".
        ui.split_divider_color = split_divider_color.and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        ui.input_echo_color = input_echo_color.and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        ui.echo_macros = echo_macros;
        ui.input_echo_caret = input_echo_caret;
        // Clamp to a sane range so a malformed input cannot freeze the
        // paste indicator (0–10s per line is plenty).
        ui.paste_line_delay_ms = paste_line_delay_ms.min(10_000);
        ui.spellcheck_prompt = spellcheck_prompt;
        // Coerce an unknown caret shape (hand-edited profile.toml, or a
        // value from a newer build) back to the default so the input
        // row always paints something.
        ui.input_cursor_style = match input_cursor_style.as_str() {
            "block_outline" | "half_block" | "underline" | "underline_thick" | "pipe"
            | "pipe_thick" => input_cursor_style,
            _ => "block".to_string(),
        };
        ui.vitals_density = crate::profile_config::coerce_vitals_density(vitals_density);
        ui.vitals_values = crate::profile_config::coerce_vitals_values(vitals_values);
        ui.vitals_meter = crate::profile_config::coerce_vitals_meter(vitals_meter);
        ui.vitals_warn_thirds = vitals_warn_thirds;
        ui.vitals_hide_when_pinned = vitals_hide_when_pinned;
        // Same coercion for chip_style — an unknown variant from a
        // hand-edited profile.toml falls back to the default rather
        // than letting the frontend render a chip with no style.
        ui.chip_style = match chip_style.as_str() {
            "value_only" | "caption_value" | "icon_value" => chip_style,
            _ => "value_only".to_string(),
        };
        ui.tick_count = crate::profile_config::coerce_tick_count(tick_count);
        ui.affects_style = crate::profile_config::coerce_affects_style(affects_style);
        ui.affects_marker = crate::profile_config::coerce_affects_marker(affects_marker);
        ui.affects_tint = affects_tint;
        (ui.affects_running_out_hours, ui.affects_almost_gone_hours) =
            crate::profile_config::coerce_affects_thresholds(
                affects_running_out_hours,
                affects_almost_gone_hours,
            );
    }
}

/// The live UI config and the [`ui_config_generation`] it was read at.
#[tauri::command]
pub(crate) async fn ui_get_config(
    state: State<'_, SharedState>,
) -> Result<UiConfigPayload, String> {
    let p = state.profile.lock().await;
    Ok(ui_config_of(&p, ui_config_generation()))
}

/// What `ui_get_config` hands the webview for the live profile `p`, read
/// at `generation`. Your prompt is not in it: the prompt section reads and
/// writes the `[prompt]` table through the prompt commands, and `[ui]`
/// keeps only a copy of its switch and design for an older build (D20).
fn ui_config_of(p: &crate::profile::Profile, generation: u64) -> UiConfigPayload {
    let mut payload = UiConfigPayload::from_ui(&p.ui);
    payload.generation = Some(generation);
    payload
}

/// Save the whole UI config. A copy read before the live config was last
/// replaced is refused and returns false, and the caller reads the
/// config again.
#[tauri::command]
pub(crate) async fn ui_set_config(
    app: AppHandle,
    state: State<'_, SharedState>,
    config: UiConfigPayload,
) -> Result<bool, String> {
    let applied = {
        let mut p = state.profile.lock().await;
        apply_ui_config(&mut p.ui, config, ui_config_generation())
    };
    if !applied {
        return Ok(false);
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    Ok(true)
}

/// Ask the session to repaint the open row as the `[prompt]` table now
/// says. Nothing happens with no connection, or when no drawn prompt is
/// the last thing on screen.
pub(crate) async fn request_prompt_repaint(state: &SharedState) {
    if let Some(handle) = state.session.lock().await.as_ref() {
        let _ = handle.prompt_repaint();
    }
}

/// Write a whole config save onto `ui`, unless it was read at a
/// generation other than `current`. Returns whether it applied.
fn apply_ui_config(
    ui: &mut crate::profile_config::UiConfig,
    config: UiConfigPayload,
    current: u64,
) -> bool {
    if config.generation.is_some_and(|g| g != current) {
        return false;
    }
    config.apply_to(ui);
    true
}

/// Which values the game hides, as every open window last heard it on
/// `session://hidden`. The session reports each change once, so a window
/// that opens or reloads while the game hides something, Settings among
/// them, reads the state here. Nothing is hidden with no connection.
#[tauri::command]
pub(crate) async fn hidden_get(
    state: State<'_, SharedState>,
) -> Result<vosh_prompt::values::Hidden, String> {
    Ok(reported_hidden(state.inner()).await)
}

/// The body of [`hidden_get`].
async fn reported_hidden(state: &SharedState) -> vosh_prompt::values::Hidden {
    state.profile.lock().await.prompt.vars.reported()
}

/// Where your prompt shows, with what the Settings row and the main
/// window need beside it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct PromptShowState {
    /// `text`, `lifted` or `pinned`, from `[prompt] show`.
    pub show: String,
    /// The profile has a capture that reads a prompt. Without one Vosh
    /// finds no prompt to lift or pin.
    pub capture: bool,
    /// Draw your prompt is on, for the palette's row.
    pub draw: bool,
    /// The game sent Char.Prompt this session.
    pub game_sent: bool,
    /// The rows the band above the command line keeps while your prompt
    /// shows pinned, the most any prompt the capture reads can take.
    pub zone: usize,
    /// You turned prompts off in the game.
    pub prompts_off: bool,
}

/// Where the active profile's prompt shows, and whether it reads one.
/// Every window reads it again on `vosh://prompt-config-changed`.
#[tauri::command]
pub(crate) async fn prompt_show_get(
    state: State<'_, SharedState>,
) -> Result<PromptShowState, String> {
    Ok(prompt_show_state(&*state.profile.lock().await))
}

/// The body of [`prompt_show_get`].
fn prompt_show_state(p: &crate::profile::Profile) -> PromptShowState {
    PromptShowState {
        show: p.prompt.show().name().to_string(),
        capture: p.prompt.stage.has_recognizer(),
        draw: p.prompt.config().draw,
        game_sent: p.prompt.vars.gmcp().prompt_seen(),
        zone: p.prompt.zone(),
        prompts_off: p.prompt.prompts_off(),
    }
}

/// The triggers that hid your prompt this session while the profile
/// reads no prompt, so Vosh drew nothing in its place. The session names
/// each one once on `session://prompt-gag-without-reader`, so a window
/// that opens later, Settings among them, reads the list here. Empty
/// with no connection.
#[tauri::command]
pub(crate) async fn prompt_gags_without_reader(
    state: State<'_, SharedState>,
) -> Result<Vec<String>, String> {
    let p = state.profile.lock().await;
    Ok(p.prompt
        .stage
        .gags_without_reader()
        .map(str::to_string)
        .collect())
}

/// Replace a profile's tracked affects without touching the rest of
/// its UI config, so an editor outside Settings cannot write a stale
/// snapshot over other fields. Returns the normalized list.
///
/// With no `profile`, or the active one, the live profile takes the
/// list, persists, and broadcasts it as `vosh://tracked-affects-changed`
/// to every window. An inactive `profile` has its file rewritten
/// instead, and only `vosh://profile-changed` goes out, since the
/// tracked affects event would hand another profile's list to the main
/// window's store.
#[tauri::command]
pub(crate) async fn tracked_affects_set(
    app: AppHandle,
    state: State<'_, SharedState>,
    list: Vec<crate::profile_config::TrackedAffect>,
    profile: Option<String>,
) -> Result<Vec<crate::profile_config::TrackedAffect>, String> {
    let list = crate::profile_config::normalize_tracked_affects(list);
    let shared: SharedState = state.inner().clone();
    if let Some(name) = profile.as_deref() {
        let written = crate::characters::edit_inactive_profile(&shared, name, |_, config| {
            config.ui.tracked_affects.clone_from(&list);
        })
        .await?;
        if written.is_some() {
            crate::characters::broadcast_profile_changed(&app, name);
            return Ok(list);
        }
    }
    {
        let mut p = state.profile.lock().await;
        p.ui.tracked_affects.clone_from(&list);
    }
    persist_profile(&app, &shared).await;
    broadcast(&app, TRACKED_AFFECTS_CHANGED, &list);
    if let Some(active) = crate::characters::active_name(&shared).await {
        crate::characters::broadcast_profile_changed(&app, &active);
    }
    Ok(list)
}

/// Replace the active profile's theme choice without touching the rest
/// of the UI config. The main window's palette picks a theme while the
/// Settings window may hold its own full snapshot, so a whole config
/// write from one would overwrite the other's newer fields. The caller
/// applies and broadcasts the theme itself. While follow system
/// appearance is on, a pick fills the light or dark slot instead, so the
/// caller also sends the pair.
#[tauri::command]
pub(crate) async fn ui_set_theme(
    app: AppHandle,
    state: State<'_, SharedState>,
    theme: String,
    light_theme: Option<String>,
    dark_theme: Option<String>,
) -> Result<(), String> {
    {
        let mut p = state.profile.lock().await;
        if !apply_theme_pick(&mut p.ui, theme, light_theme, dark_theme) {
            return Ok(());
        }
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    Ok(())
}

/// Write a theme pick onto the live UI config. A missing or blank pair
/// entry leaves that slot alone. Returns whether anything changed, so an
/// unchanged pick skips the save.
fn apply_theme_pick(
    ui: &mut crate::profile_config::UiConfig,
    theme: String,
    light_theme: Option<String>,
    dark_theme: Option<String>,
) -> bool {
    let mut changed = false;
    let mut set = |slot: &mut String, value: String| {
        if !value.is_empty() && *slot != value {
            *slot = value;
            changed = true;
        }
    };
    set(&mut ui.theme, theme);
    if let Some(v) = light_theme {
        set(&mut ui.light_theme, v);
    }
    if let Some(v) = dark_theme {
        set(&mut ui.dark_theme, v);
    }
    changed
}

/// What one affects display pick changes. Each field left out stays as
/// it is.
#[derive(Debug, Default)]
pub(crate) struct AffectsDisplayPick {
    pub(crate) style: Option<String>,
    pub(crate) marker: Option<String>,
    pub(crate) tint: Option<bool>,
    pub(crate) running_out: Option<u32>,
    pub(crate) almost_gone: Option<u32>,
}

/// Change how the Affects pane draws without touching the rest of the
/// UI config, for the picks in the pane's own menu. The main window
/// holds no whole config to save, and Settings may hold one with newer
/// fields, so a whole config write from either would put stale values
/// back. Only what is given changes. Nothing is saved or sent when the
/// pick changes nothing.
#[tauri::command]
pub(crate) async fn ui_set_affects_display(
    app: AppHandle,
    state: State<'_, SharedState>,
    style: Option<String>,
    marker: Option<String>,
    tint: Option<bool>,
    running_out: Option<u32>,
    almost_gone: Option<u32>,
) -> Result<(), String> {
    let pick = AffectsDisplayPick {
        style,
        marker,
        tint,
        running_out,
        almost_gone,
    };
    let changed = {
        let mut p = state.profile.lock().await;
        apply_affects_display(&mut p.ui, pick)
    };
    let Some(display) = changed else {
        return Ok(());
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast(&app, AFFECTS_DISPLAY_CHANGED, &display);
    Ok(())
}

/// Write an affects display pick onto the live UI config, coercing an
/// unknown style or marker to the default and the hours to 0 to 99,
/// almost gone never over running out. Returns the new display when
/// anything changed, so an unchanged pick saves and sends nothing.
fn apply_affects_display(
    ui: &mut crate::profile_config::UiConfig,
    pick: AffectsDisplayPick,
) -> Option<AffectsDisplay> {
    let before = AffectsDisplay::of(ui);
    if let Some(style) = pick.style {
        ui.affects_style = crate::profile_config::coerce_affects_style(style);
    }
    if let Some(marker) = pick.marker {
        ui.affects_marker = crate::profile_config::coerce_affects_marker(marker);
    }
    if let Some(tint) = pick.tint {
        ui.affects_tint = tint;
    }
    if let Some(hours) = pick.running_out {
        ui.affects_running_out_hours = hours;
    }
    if let Some(hours) = pick.almost_gone {
        ui.affects_almost_gone_hours = hours;
    }
    (ui.affects_running_out_hours, ui.affects_almost_gone_hours) =
        crate::profile_config::coerce_affects_thresholds(
            ui.affects_running_out_hours,
            ui.affects_almost_gone_hours,
        );
    let after = AffectsDisplay::of(ui);
    (after != before).then_some(after)
}

/// The 16 ANSI slots a chat channel can take, in the frontend's names.
const CHAT_COLOR_SLOTS: [&str; 16] = [
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
    "brightBlack",
    "brightRed",
    "brightGreen",
    "brightYellow",
    "brightBlue",
    "brightMagenta",
    "brightCyan",
    "brightWhite",
];

/// The chat pane's channel colors for the live profile.
#[tauri::command]
pub(crate) async fn ui_get_chat_colors(
    state: State<'_, SharedState>,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    let p = state.profile.lock().await;
    Ok(p.ui.chat_colors.clone())
}

/// Recolor one chat channel from the pane menu, or give it back its
/// default with no color. Like the affects display picks, it touches
/// nothing else in the UI config. Nothing is saved or sent when the
/// pick changes nothing.
#[tauri::command]
pub(crate) async fn ui_set_chat_color(
    app: AppHandle,
    state: State<'_, SharedState>,
    channel: String,
    color: Option<String>,
) -> Result<(), String> {
    let changed = {
        let mut p = state.profile.lock().await;
        apply_chat_color(&mut p.ui, channel, color)
    };
    send_chat_colors(&app, state.inner(), changed).await;
    Ok(())
}

/// Give every chat channel its default color again.
#[tauri::command]
pub(crate) async fn ui_reset_chat_colors(
    app: AppHandle,
    state: State<'_, SharedState>,
) -> Result<(), String> {
    let changed = {
        let mut p = state.profile.lock().await;
        reset_chat_colors(&mut p.ui)
    };
    send_chat_colors(&app, state.inner(), changed).await;
    Ok(())
}

/// Save the profile and tell every window, when a chat color moved.
async fn send_chat_colors(
    app: &AppHandle,
    state: &SharedState,
    changed: Option<std::collections::BTreeMap<String, String>>,
) {
    let Some(colors) = changed else {
        return;
    };
    let shared: SharedState = state.clone();
    persist_profile(app, &shared).await;
    broadcast(app, CHAT_COLORS_CHANGED, &colors);
}

/// Write a chat color pick onto the live UI config. The channel matches
/// in lowercase. A color that is not one of the 16 slots clears the
/// channel back to its default. Returns the new table when anything
/// changed.
fn apply_chat_color(
    ui: &mut crate::profile_config::UiConfig,
    channel: String,
    color: Option<String>,
) -> Option<std::collections::BTreeMap<String, String>> {
    let channel = channel.trim().to_lowercase();
    if channel.is_empty() {
        return None;
    }
    let color = color.filter(|c| CHAT_COLOR_SLOTS.contains(&c.as_str()));
    let changed = match color {
        Some(color) => ui.chat_colors.insert(channel, color.clone()).as_ref() != Some(&color),
        None => ui.chat_colors.remove(&channel).is_some(),
    };
    changed.then(|| ui.chat_colors.clone())
}

/// Clear every chat color. Returns the empty table when there was any.
fn reset_chat_colors(
    ui: &mut crate::profile_config::UiConfig,
) -> Option<std::collections::BTreeMap<String, String>> {
    if ui.chat_colors.is_empty() {
        return None;
    }
    ui.chat_colors.clear();
    Some(std::collections::BTreeMap::new())
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
    let installed = {
        let mut p = state.profile.lock().await;
        install_preset_triggers(&mut p, triggers)?
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    if installed > 0 {
        broadcast_list_changes(&app, ListChanges::TRIGGERS);
    }
    Ok(installed)
}

/// The body of [`presets_install`] over the live profile `p`, so a test
/// can run the preset install launch runs. Returns the number installed.
pub(crate) fn install_preset_triggers(
    p: &mut Profile,
    triggers: Vec<Trigger>,
) -> Result<usize, String> {
    let mut installed = 0usize;
    for mut t in triggers {
        // The startup re-install overwrites same-named presets so
        // pattern/template updates land, but the group is the user's
        // organization: carry it over so putting a preset into a group
        // survives relaunch.
        if t.group.is_none() {
            if let Some(existing) = p.triggers.get(&t.name) {
                t.group.clone_from(&existing.group);
            }
        }
        p.triggers.set(t).map_err(|e| e.to_string())?;
        installed += 1;
    }
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
    if removed > 0 {
        broadcast_list_changes(&app, ListChanges::TRIGGERS);
    }
    Ok(removed)
}

#[derive(serde::Serialize)]
pub(crate) struct UpdateCheckResult {
    pub available: bool,
    pub version: Option<String>,
    pub notes: Option<String>,
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
/// loads each per-profile [`ProfileConfig`] off disk, the active one as
/// the save apply runs first would write it, and runs the analyzer in
/// [`crate::migration`]. Returns the full plan: every
/// auto-resolved item, every conflict (one entry per name with two or
/// more diverging variants), and the per-source-profile loadouts the
/// migration would generate. Nothing is written to disk; the wizard
/// uses this for the preview pane only. The companion
/// [`migration_apply`] command commits the plan once the user picks
/// winners for any conflicts. Refused while a profile file did not read
/// at launch, while an earlier run is unfinished, while catalog.toml or
/// loadouts.toml is on disk, while profiles/legacy holds copies from
/// an earlier run, or in a session that runs in loadout mode, see
/// [`migration_refusal`].
///
/// [`ProfileConfig`]: crate::profile_config::ProfileConfig
/// [`migration_refusal`]: crate::loadouts::wizard::apply::migration_refusal
#[tauri::command]
pub(crate) async fn migration_analyze(
    app: AppHandle,
    state: State<'_, SharedState>,
    library: Vec<String>,
) -> Result<crate::migration::MigrationPlan, String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let library: Vec<&str> = library.iter().map(String::as_str).collect();
    analyze_migration(&state, &app_data, &library).await
}

/// Commit the Path B migration. Saves the live profile, re-runs the
/// analyzer, applies the user's per-conflict resolutions (or the
/// default version for any missing resolution), copies every
/// existing per-profile file into
/// `profiles/legacy/`, writes `catalog.toml` + `loadouts.toml`, takes the
/// aliases, triggers, and macros out of each profile file, which keeps
/// every other setting, and asks for a relaunch so the startup hook
/// picks up Path B mode. No loadout is on at first, so the group
/// checkboxes in each profile file decide what is on for that profile,
/// at launch and at every switch, the way each profile had it. A write
/// that fails puts back every file the run changed, so you stay in per
/// profile mode and can run it again. A run that stops partway, or
/// cannot put every file back, finishes at the next launch from the
/// journal it saved first. Refused while a profile file did not read at
/// launch, while an earlier run is unfinished, while catalog.toml or
/// loadouts.toml is on disk, while profiles/legacy holds copies from
/// an earlier run, or in a session that runs in loadout mode, see
/// [`migration_refusal`].
///
/// [`migration_refusal`]: crate::loadouts::wizard::apply::migration_refusal
#[tauri::command]
pub(crate) async fn migration_apply(
    app: AppHandle,
    state: State<'_, SharedState>,
    resolutions: Vec<ConflictResolution>,
    library: Vec<String>,
) -> Result<(), String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let library: Vec<&str> = library.iter().map(String::as_str).collect();
    apply_migration(&state, &app_data, &resolutions, &library, || {
        // Path B is now on disk but the live session still holds the
        // pre-migration profile. Block every persist until the relaunch
        // loads the catalog, and flip the input layer into Path B mode
        // so the legacy #profile trio stops writing files.
        MIGRATION_RELAUNCH_PENDING.store(true, std::sync::atomic::Ordering::Release);
        crate::input::PATH_B_ACTIVE.store(true, std::sync::atomic::Ordering::Release);
    })
    .await?;

    // Returning Ok rather than calling `app.restart()` here. Restart
    // is fragile in dev mode: it tears down the binary out from under
    // the `tauri dev` watcher and leaves the next process trying to
    // load a frontend whose Vite dev server may have been killed
    // with the parent, ending in a hidden window with no JS reveal.
    // The frontend shows a "migration complete, please relaunch"
    // banner and offers an explicit [Quit Vosh] button (handled
    // separately by app_quit) that cleanly exits the process. The
    // user re-opens Vosh and the Path B startup hook picks the new
    // catalog up. Path B mode is durable on disk either way. The main
    // window hears the event and says that nothing saves until then, in
    // the terminal and in a toast that stays up.
    announce_migration_applied(&app);
    Ok(())
}

/// Cleanly exit the app. Surfaces a "quit" event first so any window
/// can flush state, then calls `app.exit(0)`. Used by the post-
/// migration prompt to take the user out of the legacy-mode session
/// in one click; on relaunch the Path B startup hook picks up the
/// new catalog.
#[tauri::command]
pub(crate) async fn app_quit(app: AppHandle) -> Result<(), String> {
    // No explicit persist here: `app.exit` raises `RunEvent::ExitRequested`,
    // whose handler asks the windows for their pending writes and then
    // flushes the profile exactly once (with a timeout), see exit_flush.rs.
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

/// Replace the active-loadouts list and reapply group state: the
/// union rule while loadouts are active, full dormancy when the user
/// deactivates everything. Persists the loadout set to disk and emits
/// a state-changed event so other windows, such as the Loadouts editor
/// in Settings, see the update.
#[tauri::command]
pub(crate) async fn loadouts_set_active(app: AppHandle, active: Vec<String>) -> Result<(), String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    set_active_loadouts(&app, &app_data, active).await?;
    // The recomputed (or dormant) disabled lists live in the profile
    // snapshot on disk; queue a persist so a crash before the exit
    // flush cannot leave loadouts.toml and per-profile state
    // disagreeing. Also clears any stale persist suppression — this is
    // a durable change the user asked for.
    mark_profile_dirty(&app);
    let _ = app.emit(LOADOUTS_CHANGED, &());
    Ok(())
}

/// The part of [`loadouts_set_active`] that runs under the loadout and
/// profile locks: take the new active list, lay the group state it
/// imposes over the live profile, and save loadouts.toml in `app_data`.
/// The command looks up the app data folder and queues the profile
/// save, so a test can run this against a mock app and a scratch folder.
/// When the switch turned a macro group on or off, every window hears it
/// once the locks are released, since the command line keeps its own map
/// of the macro keys that fire.
pub(crate) async fn set_active_loadouts<R: tauri::Runtime>(
    app: &AppHandle<R>,
    app_data: &std::path::Path,
    active: Vec<String>,
) -> Result<(), String> {
    let state: SharedState = app.state::<SharedState>().inner().clone();
    let macro_groups_changed = {
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
        // Deactivate-all is the documented kill switch ("Activate none
        // to keep the catalog dormant"). Recorded as an explicit flag:
        // an empty active list on its own is ambiguous with "loadouts
        // have no opinion", and the other apply points (startup,
        // profile switch) must be able to re-impose dormancy.
        set.dormant = set.active.is_empty();
        let snapshot = set.clone();
        let mut p = state.profile.lock().await;
        let macro_groups_before = p.disabled_macro_groups.clone();
        crate::loadout_store::apply_effective_state(&snapshot, &mut p);
        if let Err(e) = crate::loadout_store::save_loadout_set(app_data, &snapshot) {
            warn!(error = %e, "loadouts.toml save failed");
        }
        p.disabled_macro_groups != macro_groups_before
    };
    if macro_groups_changed {
        broadcast(app, MACRO_GROUPS_CHANGED, &"");
    }
    Ok(())
}

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

#[cfg(test)]
mod tests {
    use super::{
        shows_on_reopen, window_fit, ScrollbackLoad, UiConfigPayload, HELP_WINDOW, SETTINGS_WINDOW,
    };
    use crate::profile_config::{ProfileConfig, UiConfig};

    /// Send `ui` the way Settings does: out through `ui_get_config`,
    /// across the JSON bridge, and back through `ui_set_config` onto a
    /// fresh config.
    fn through_payload(ui: &UiConfig) -> UiConfig {
        let json = serde_json::to_string(&UiConfigPayload::from_ui(ui)).unwrap();
        let payload: UiConfigPayload = serde_json::from_str(&json).unwrap();
        let mut out = UiConfig::default();
        payload.apply_to(&mut out);
        out
    }

    #[test]
    fn ui_defaults_match_the_ones_the_page_fills() {
        // normalizeUiConfig in src/lib/session.ts reads the same file and
        // fills a missing field with each of these values.
        let text = include_str!("../../fixtures/ui-config/defaults.json");
        let fixture: serde_json::Value = serde_json::from_str(text).unwrap();
        let passed: Vec<&str> = fixture["passed_through"]
            .as_array()
            .unwrap()
            .iter()
            .map(|key| key.as_str().unwrap())
            .collect();
        // No [ui] table, an empty one, and an empty vitals table each take
        // their defaults by another path.
        for (from, ui) in [
            ("no [ui]", ProfileConfig::from_toml("").unwrap().ui),
            ("[ui]", ProfileConfig::from_toml("[ui]\n").unwrap().ui),
            (
                "[ui.vitals]",
                ProfileConfig::from_toml("[ui.vitals]\n").unwrap().ui,
            ),
            ("UiConfig::default", UiConfig::default()),
        ] {
            let mut sent = serde_json::to_value(UiConfigPayload::from_ui(&ui)).unwrap();
            let fields = sent.as_object_mut().unwrap();
            for key in &passed {
                assert!(fields.remove(*key).is_some(), "{from}: {key}");
            }
            assert_eq!(sent, fixture["defaults"], "{from}");
        }
    }

    /// Save `ui` to a profile file and read it back.
    fn through_toml(ui: &UiConfig) -> UiConfig {
        let config = ProfileConfig {
            ui: ui.clone(),
            ..ProfileConfig::default()
        };
        ProfileConfig::from_toml(&config.to_toml().unwrap())
            .unwrap()
            .ui
    }

    #[test]
    fn blinking_text_keeps_your_choice_and_none_until_you_make_one() {
        // None is no choice, which the page reads from the system's reduce
        // motion setting, so it never reaches the file as a value.
        let mut ui = UiConfig::default();
        assert_eq!(through_payload(&ui).blink_text, None);
        assert_eq!(through_toml(&ui).blink_text, None);
        let written = ProfileConfig::default().to_toml().unwrap();
        assert!(!written.contains("blink_text"), "{written}");
        for choice in [true, false] {
            ui.blink_text = Some(choice);
            assert_eq!(through_payload(&ui).blink_text, Some(choice));
            assert_eq!(through_toml(&ui).blink_text, Some(choice));
        }
    }

    /// A whole config save read at `generation`, holding `ui`.
    fn save_of(ui: &UiConfig, generation: Option<u64>) -> UiConfigPayload {
        let mut payload = UiConfigPayload::from_ui(ui);
        payload.generation = generation;
        payload
    }

    #[test]
    fn a_save_read_before_a_replace_leaves_the_new_profile_alone() {
        // The loaded profile counts up with the value alone.
        let mut live = UiConfig {
            tick_count: "up".into(),
            chip_style: "value_only".into(),
            ..UiConfig::default()
        };
        // Settings read the old profile at generation 3, and you moved
        // the font size after #profile load took it to 4.
        let old = UiConfig {
            tick_count: "down".into(),
            chip_style: "icon_value".into(),
            font_size: 16,
            ..UiConfig::default()
        };
        assert!(!super::apply_ui_config(
            &mut live,
            save_of(&old, Some(3)),
            4
        ));
        assert_eq!(live.tick_count, "up");
        assert_eq!(live.chip_style, "value_only");
        assert_eq!(live.font_size, UiConfig::default().font_size);
    }

    #[test]
    fn a_save_read_since_the_last_replace_applies() {
        let mut live = UiConfig::default();
        let edited = UiConfig {
            font_size: 16,
            tick_count: "up".into(),
            ..UiConfig::default()
        };
        assert!(super::apply_ui_config(
            &mut live,
            save_of(&edited, Some(4)),
            4
        ));
        assert_eq!(live.font_size, 16);
        assert_eq!(live.tick_count, "up");
        // A config that never came from the backend carries none.
        let mut live = UiConfig::default();
        assert!(super::apply_ui_config(&mut live, save_of(&edited, None), 4));
        assert_eq!(live.font_size, 16);
    }

    /// A live profile whose `[prompt]` table holds more than Settings
    /// shows: a capture and an earlier design.
    fn prompt_profile() -> crate::profile::Profile {
        let mut p = crate::profile::Profile::default();
        p.set_prompt_config(vosh_prompt::PromptConfig {
            draw: true,
            template: "%hp".into(),
            previous_templates: vec!["%mana".into()],
            capture: vosh_prompt::CaptureConfig::Regex(vosh_prompt::config::RegexCapture {
                lines: vec![r"\[(?<hp>\d+)hp\]".into()],
                ..vosh_prompt::config::RegexCapture::default()
            }),
            ..vosh_prompt::PromptConfig::default()
        });
        p
    }

    #[test]
    fn a_settings_payload_that_leaves_fields_out_still_reads() {
        let mut json = serde_json::to_value(UiConfigPayload::default()).unwrap();
        let fields = json.as_object_mut().unwrap();
        fields.remove("font_size");
        fields.remove("theme");
        fields.insert("font_family".into(), "Iosevka".into());
        let payload: UiConfigPayload = serde_json::from_value(json).unwrap();
        let defaults = crate::profile_config::UiConfig::default();
        assert_eq!(payload.font_family, "Iosevka");
        assert_eq!(payload.font_size, defaults.font_size);
        assert_eq!(payload.theme, defaults.theme);
        let empty: UiConfigPayload = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.font_size, defaults.font_size);
    }

    #[test]
    fn the_settings_payload_carries_nothing_of_your_prompt() {
        let p = prompt_profile();
        let json = serde_json::to_value(super::ui_config_of(&p, 5)).unwrap();
        let keys = json.as_object().unwrap();
        for key in ["prompt_template_enabled", "prompt_template", "prompt_show"] {
            assert!(!keys.contains_key(key), "{key} reaches Settings");
        }
        assert_eq!(json["generation"], 5);
    }

    #[test]
    fn a_settings_save_leaves_the_prompt_table_and_its_copy_alone() {
        let mut p = prompt_profile();
        let mut config = p.prompt.config().clone();
        config.show = vosh_prompt::PromptShow::Pinned;
        p.set_prompt_config(config);
        let table = p.prompt.config().clone();
        let mut save = super::ui_config_of(&p, 4);
        save.font_size = 16;
        assert!(super::apply_ui_config(&mut p.ui, save, 4));
        assert_eq!(p.ui.font_size, 16);
        assert_eq!(*p.prompt.config(), table);

        // A window from before the prompt section still sends the three
        // fields. They are read past and change nothing.
        let mut json = serde_json::to_value(super::ui_config_of(&p, 4)).unwrap();
        let fields = json.as_object_mut().unwrap();
        fields.insert("prompt_template_enabled".into(), false.into());
        fields.insert("prompt_template".into(), "stale".into());
        fields.insert("prompt_show".into(), "text".into());
        let old: UiConfigPayload = serde_json::from_value(json).unwrap();
        assert!(super::apply_ui_config(&mut p.ui, old, 4));
        assert_eq!(*p.prompt.config(), table);

        // The file keeps the table, and [ui] its copy of the switch and
        // the design for an older build.
        let file = crate::profile_config::ProfileConfig::from_profile(&p);
        assert_eq!(file.prompt_config(), table);
        assert!(file.ui.prompt_template_enabled);
        assert_eq!(file.ui.prompt_template, "%hp");
    }

    #[test]
    fn the_prompt_show_state_says_where_it_shows_and_whether_a_capture_reads_it() {
        let mut p = crate::profile::Profile::default();
        assert_eq!(
            super::prompt_show_state(&p),
            super::PromptShowState {
                show: "text".into(),
                capture: false,
                draw: false,
                game_sent: false,
                zone: 1,
                prompts_off: false,
            }
        );
        p = prompt_profile();
        let mut config = p.prompt.config().clone();
        config.show = vosh_prompt::PromptShow::Lifted;
        p.set_prompt_config(config);
        p.prompt.connect(true);
        p.prompt.observe(
            "Char.Prompt",
            serde_json::json!({"enabled": true, "prompt": "<%hhp> ", "fprompt": ""}),
            chrono::Local::now().fixed_offset(),
        );
        let state = super::prompt_show_state(&p);
        assert_eq!(state.show, "lifted");
        assert!(state.capture);
        assert_eq!(state.draw, p.prompt.config().draw);
        assert!(state.game_sent);
        assert_eq!(state.zone, 1);
        assert!(!state.prompts_off);
        p.prompt.observe(
            "Char.Prompt",
            serde_json::json!({"enabled": false, "prompt": "<%hhp> ", "fprompt": ""}),
            chrono::Local::now().fixed_offset(),
        );
        assert!(super::prompt_show_state(&p).prompts_off);
    }

    #[test]
    fn the_generation_travels_with_the_config_but_stays_optional() {
        let json = serde_json::to_value(save_of(&UiConfig::default(), Some(7))).unwrap();
        assert_eq!(json["generation"], serde_json::json!(7));
        let json = serde_json::to_value(save_of(&UiConfig::default(), None)).unwrap();
        assert!(json.get("generation").is_none());
        let back: UiConfigPayload = serde_json::from_value(json).unwrap();
        assert_eq!(back.generation, None);
    }

    #[test]
    fn follow_system_appearance_round_trips() {
        let ui = UiConfig {
            follow_system_appearance: true,
            ..UiConfig::default()
        };
        assert!(through_payload(&ui).follow_system_appearance);
        assert!(through_toml(&ui).follow_system_appearance);
        assert!(!through_payload(&UiConfig::default()).follow_system_appearance);
    }

    #[test]
    fn light_theme_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.light_theme, "vellum");
        ui.light_theme = "classic-vivid".into();
        assert_eq!(through_payload(&ui).light_theme, "classic-vivid");
        assert_eq!(through_toml(&ui).light_theme, "classic-vivid");
        // A blank pick saves as the default light theme.
        ui.light_theme = "  ".into();
        assert_eq!(through_payload(&ui).light_theme, "vellum");
    }

    #[test]
    fn dark_theme_round_trips() {
        let mut ui = UiConfig::default();
        // Unset until the first save, so the frontend can seed it from
        // the current theme.
        assert_eq!(ui.dark_theme, "");
        assert_eq!(through_payload(&ui).dark_theme, "");
        ui.dark_theme = "nord".into();
        assert_eq!(through_payload(&ui).dark_theme, "nord");
        assert_eq!(through_toml(&ui).dark_theme, "nord");
    }

    #[test]
    fn terminal_line_height_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.terminal_line_height, "default");
        for id in ["compact", "default", "loose"] {
            ui.terminal_line_height = id.into();
            assert_eq!(through_payload(&ui).terminal_line_height, id);
            assert_eq!(through_toml(&ui).terminal_line_height, id);
        }
        ui.terminal_line_height = "roomy".into();
        assert_eq!(through_payload(&ui).terminal_line_height, "default");
    }

    #[test]
    fn a_theme_pick_writes_only_what_it_names() {
        let mut ui = UiConfig::default();
        assert!(super::apply_theme_pick(&mut ui, "nord".into(), None, None));
        assert_eq!(ui.theme, "nord");
        assert_eq!(ui.light_theme, "vellum");
        assert_eq!(ui.dark_theme, "");

        // A pick while following the system fills the dark slot.
        assert!(super::apply_theme_pick(
            &mut ui,
            "nord".into(),
            Some("vellum".into()),
            Some("tokyo-night".into()),
        ));
        assert_eq!(ui.theme, "nord");
        assert_eq!(ui.dark_theme, "tokyo-night");

        // The same pick again changes nothing, and a blank slot is left alone.
        assert!(!super::apply_theme_pick(
            &mut ui,
            "nord".into(),
            Some(String::new()),
            Some("tokyo-night".into()),
        ));
        assert_eq!(ui.light_theme, "vellum");
    }

    #[test]
    fn vitals_density_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.vitals_density, "rows");
        for id in ["rows", "line"] {
            ui.vitals_density = id.into();
            assert_eq!(through_payload(&ui).vitals_density, id);
            assert_eq!(through_toml(&ui).vitals_density, id);
        }
        // An unknown density saves as rows.
        ui.vitals_density = "grid".into();
        assert_eq!(through_payload(&ui).vitals_density, "rows");
    }

    #[test]
    fn vitals_values_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.vitals_values, "current-max");
        for id in ["current-max", "current", "percent"] {
            ui.vitals_values = id.into();
            assert_eq!(through_payload(&ui).vitals_values, id);
            assert_eq!(through_toml(&ui).vitals_values, id);
        }
        // An unknown form saves as current and max.
        ui.vitals_values = "both".into();
        assert_eq!(through_payload(&ui).vitals_values, "current-max");
    }

    #[test]
    fn vitals_meter_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.vitals_meter, "line");
        for id in ["line", "bar", "none"] {
            ui.vitals_meter = id.into();
            assert_eq!(through_payload(&ui).vitals_meter, id);
            assert_eq!(through_toml(&ui).vitals_meter, id);
        }
        // An unknown meter saves as the line.
        ui.vitals_meter = "gauge".into();
        assert_eq!(through_payload(&ui).vitals_meter, "line");
    }

    #[test]
    fn vitals_warn_thirds_round_trips() {
        let mut ui = UiConfig::default();
        assert!(!ui.vitals_warn_thirds);
        assert!(!through_payload(&ui).vitals_warn_thirds);
        ui.vitals_warn_thirds = true;
        assert!(through_payload(&ui).vitals_warn_thirds);
        assert!(through_toml(&ui).vitals_warn_thirds);
    }

    #[test]
    fn readable_highlights_round_trips() {
        let mut ui = UiConfig::default();
        assert!(ui.readable_highlights);
        assert!(through_payload(&ui).readable_highlights);
        assert!(through_toml(&ui).readable_highlights);
        ui.readable_highlights = false;
        assert!(!through_payload(&ui).readable_highlights);
        assert!(!through_toml(&ui).readable_highlights);
    }

    #[test]
    fn readable_highlights_is_written_only_while_off() {
        let mut config = ProfileConfig::default();
        let on = config.to_toml().unwrap();
        assert!(!on.contains("readable_highlights"), "{on}");
        config.ui.readable_highlights = false;
        let off = config.to_toml().unwrap();
        assert!(off.contains("readable_highlights = false"), "{off}");
        // A file from before the switch reads it on.
        let old = ProfileConfig::from_toml("[ui]\ntheme = \"vellum\"\n").unwrap();
        assert!(old.ui.readable_highlights);
    }

    #[test]
    fn collapse_repeats_round_trips() {
        let mut ui = UiConfig::default();
        assert!(!ui.collapse_repeats);
        assert!(!through_payload(&ui).collapse_repeats);
        assert!(!through_toml(&ui).collapse_repeats);
        ui.collapse_repeats = true;
        assert!(through_payload(&ui).collapse_repeats);
        assert!(through_toml(&ui).collapse_repeats);
    }

    #[test]
    fn collapse_repeats_is_written_only_while_on() {
        let mut config = ProfileConfig::default();
        let off = config.to_toml().unwrap();
        assert!(!off.contains("collapse_repeats"), "{off}");
        config.ui.collapse_repeats = true;
        let on = config.to_toml().unwrap();
        assert!(on.contains("collapse_repeats = true"), "{on}");
        // A file from before the switch reads it off.
        let old = ProfileConfig::from_toml("[ui]\ntheme = \"vellum\"\n").unwrap();
        assert!(!old.ui.collapse_repeats);
    }

    #[test]
    fn vitals_hide_when_pinned_round_trips() {
        let mut ui = UiConfig::default();
        assert!(ui.vitals_hide_when_pinned);
        assert!(through_payload(&ui).vitals_hide_when_pinned);
        ui.vitals_hide_when_pinned = false;
        assert!(!through_payload(&ui).vitals_hide_when_pinned);
        assert!(!through_toml(&ui).vitals_hide_when_pinned);
    }

    #[test]
    fn tick_count_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.tick_count, "up");
        for id in ["up", "down", "down_past_zero"] {
            ui.tick_count = id.into();
            assert_eq!(through_payload(&ui).tick_count, id);
            assert_eq!(through_toml(&ui).tick_count, id);
        }
        // An unknown direction saves as counting up.
        ui.tick_count = "sideways".into();
        assert_eq!(through_payload(&ui).tick_count, "up");
    }

    #[test]
    fn a_profile_without_the_tick_count_counts_up() {
        let ui = ProfileConfig::from_toml("[ui]\ntheme = \"nord\"\n")
            .unwrap()
            .ui;
        assert_eq!(ui.tick_count, "up");
    }

    #[test]
    fn the_tick_count_stays_with_each_character() {
        // Like the tick and time style, the count is not one of the
        // settings you can keep the same for every character, so the
        // shared file never holds it and each profile file does.
        let ui = UiConfig {
            tick_count: "down".into(),
            ..UiConfig::default()
        };
        let toml = ProfileConfig {
            ui,
            ..ProfileConfig::default()
        }
        .to_toml()
        .unwrap();
        assert!(toml.contains("tick_count = \"down\""));
    }

    #[test]
    fn affects_style_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.affects_style, "timers");
        for id in ["timers", "countdown", "chips", "chips_drain"] {
            ui.affects_style = id.into();
            assert_eq!(through_payload(&ui).affects_style, id);
            assert_eq!(through_toml(&ui).affects_style, id);
        }
        // An unknown layout saves as Timers first.
        ui.affects_style = "grid".into();
        assert_eq!(through_payload(&ui).affects_style, "timers");
    }

    #[test]
    fn affects_marker_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.affects_marker, "dot");
        for id in ["dot", "square", "plus_minus", "none"] {
            ui.affects_marker = id.into();
            assert_eq!(through_payload(&ui).affects_marker, id);
            assert_eq!(through_toml(&ui).affects_marker, id);
        }
        // An unknown mark saves as the dot.
        ui.affects_marker = "check".into();
        assert_eq!(through_payload(&ui).affects_marker, "dot");
    }

    #[test]
    fn affects_tint_round_trips() {
        let mut ui = UiConfig::default();
        assert!(!ui.affects_tint);
        assert!(!through_payload(&ui).affects_tint);
        ui.affects_tint = true;
        assert!(through_payload(&ui).affects_tint);
        assert!(through_toml(&ui).affects_tint);
    }

    #[test]
    fn a_profile_without_the_affects_display_loads_the_defaults() {
        let ui = ProfileConfig::from_toml("[ui]\ntheme = \"nord\"\n")
            .unwrap()
            .ui;
        assert_eq!(ui.affects_style, "timers");
        assert_eq!(ui.affects_marker, "dot");
        assert!(!ui.affects_tint);
    }

    #[test]
    fn the_affects_display_stays_with_each_character() {
        let ui = UiConfig {
            affects_style: "chips".into(),
            affects_marker: "square".into(),
            affects_tint: true,
            ..UiConfig::default()
        };
        let toml = ProfileConfig {
            ui,
            ..ProfileConfig::default()
        }
        .to_toml()
        .unwrap();
        assert!(toml.contains("affects_style = \"chips\""));
        assert!(toml.contains("affects_marker = \"square\""));
        assert!(toml.contains("affects_tint = true"));
    }

    #[test]
    fn an_affects_display_pick_writes_only_what_it_names() {
        let mut ui = UiConfig {
            affects_marker: "square".into(),
            affects_tint: true,
            ..UiConfig::default()
        };
        let pick = |style: Option<&str>, marker: Option<&str>, tint: Option<bool>| {
            super::AffectsDisplayPick {
                style: style.map(Into::into),
                marker: marker.map(Into::into),
                tint,
                ..super::AffectsDisplayPick::default()
            }
        };
        let display = super::apply_affects_display(&mut ui, pick(Some("countdown"), None, None))
            .expect("a new style changes the display");
        assert_eq!(display.style, "countdown");
        assert_eq!(display.marker, "square");
        assert!(display.tint);
        assert_eq!(ui.affects_style, "countdown");
        assert_eq!(ui.affects_marker, "square");
        assert!(ui.affects_tint);

        // The same pick again changes nothing, so nothing is saved or sent.
        assert_eq!(
            super::apply_affects_display(&mut ui, pick(Some("countdown"), None, Some(true))),
            None
        );

        // An unknown value coerces to the default before it compares.
        let display =
            super::apply_affects_display(&mut ui, pick(None, Some("sparkle"), Some(false)))
                .expect("the marker and the tint change");
        assert_eq!(display.marker, "dot");
        assert!(!display.tint);
        assert_eq!(ui.affects_style, "countdown");
        assert_eq!(
            super::apply_affects_display(&mut ui, pick(None, Some("dot"), None)),
            None
        );
        // No pick touched the hours.
        assert_eq!(display.running_out, 2);
        assert_eq!(display.almost_gone, 1);
    }

    #[test]
    fn affects_thresholds_round_trip() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.affects_running_out_hours, 2);
        assert_eq!(ui.affects_almost_gone_hours, 1);
        for (running_out, almost_gone) in [(2, 1), (5, 2), (0, 0), (3, 3), (99, 0), (99, 99)] {
            ui.affects_running_out_hours = running_out;
            ui.affects_almost_gone_hours = almost_gone;
            for read in [through_payload(&ui), through_toml(&ui)] {
                assert_eq!(read.affects_running_out_hours, running_out);
                assert_eq!(read.affects_almost_gone_hours, almost_gone);
            }
        }
    }

    #[test]
    fn a_save_holds_the_affects_thresholds_to_whole_hours_in_order() {
        // Almost gone never goes over running out, which wins.
        let mut ui = UiConfig {
            affects_running_out_hours: 3,
            affects_almost_gone_hours: 7,
            ..UiConfig::default()
        };
        let saved = through_payload(&ui);
        assert_eq!(saved.affects_running_out_hours, 3);
        assert_eq!(saved.affects_almost_gone_hours, 3);
        // Neither goes past 99.
        ui.affects_running_out_hours = 500;
        ui.affects_almost_gone_hours = 120;
        let saved = through_payload(&ui);
        assert_eq!(saved.affects_running_out_hours, 99);
        assert_eq!(saved.affects_almost_gone_hours, 99);
    }

    #[test]
    fn a_page_that_sends_odd_affects_thresholds_still_saves() {
        let read = |json: &str| {
            let payload: UiConfigPayload = serde_json::from_str(json).unwrap();
            let mut ui = UiConfig::default();
            payload.apply_to(&mut ui);
            (ui.affects_running_out_hours, ui.affects_almost_gone_hours)
        };
        assert_eq!(read("{}"), (2, 1));
        assert_eq!(
            read(r#"{"affects_running_out_hours": 4.6, "affects_almost_gone_hours": -3}"#),
            (5, 0)
        );
        assert_eq!(
            read(r#"{"affects_running_out_hours": "6", "affects_almost_gone_hours": null}"#),
            (6, 1)
        );
    }

    #[test]
    fn a_profile_without_the_affects_thresholds_reads_two_and_one_and_writes_nothing_new() {
        let file = "[ui]\ntheme = \"nord\"\naffects_style = \"chips\"\n";
        let config = ProfileConfig::from_toml(file).unwrap();
        assert_eq!(config.ui.affects_running_out_hours, 2);
        assert_eq!(config.ui.affects_almost_gone_hours, 1);
        let toml = config.to_toml().unwrap();
        assert!(!toml.contains("affects_running_out_hours"));
        assert!(!toml.contains("affects_almost_gone_hours"));
    }

    #[test]
    fn the_affects_thresholds_stay_with_each_character_once_you_change_them() {
        let ui = UiConfig {
            affects_running_out_hours: 5,
            affects_almost_gone_hours: 2,
            ..UiConfig::default()
        };
        let toml = ProfileConfig {
            ui,
            ..ProfileConfig::default()
        }
        .to_toml()
        .unwrap();
        assert!(toml.contains("affects_running_out_hours = 5"));
        assert!(toml.contains("affects_almost_gone_hours = 2"));
        // Changing one writes that one alone.
        let ui = UiConfig {
            affects_running_out_hours: 4,
            ..UiConfig::default()
        };
        let toml = ProfileConfig {
            ui,
            ..ProfileConfig::default()
        }
        .to_toml()
        .unwrap();
        assert!(toml.contains("affects_running_out_hours = 4"));
        assert!(!toml.contains("affects_almost_gone_hours"));
    }

    #[test]
    fn a_hand_edited_affects_threshold_never_stops_a_profile_loading() {
        let read = |lines: &str| {
            let ui = ProfileConfig::from_toml(&format!("[ui]\n{lines}\n"))
                .unwrap()
                .ui;
            (ui.affects_running_out_hours, ui.affects_almost_gone_hours)
        };
        assert_eq!(
            read("affects_running_out_hours = 6\naffects_almost_gone_hours = 3"),
            (6, 3)
        );
        // Out of range clamps to 0 to 99.
        assert_eq!(
            read("affects_running_out_hours = 400\naffects_almost_gone_hours = -2"),
            (99, 0)
        );
        // A decimal rounds, and a string that holds a number reads as one.
        assert_eq!(
            read("affects_running_out_hours = 3.6\naffects_almost_gone_hours = \" 2 \""),
            (4, 2)
        );
        // Anything else reads as the default.
        assert_eq!(
            read("affects_running_out_hours = \"soon\"\naffects_almost_gone_hours = true"),
            (2, 1)
        );
        assert_eq!(
            read("affects_running_out_hours = [3]\naffects_almost_gone_hours = { at = 1 }"),
            (2, 1)
        );
        // Almost gone over running out reads as running out.
        assert_eq!(
            read("affects_running_out_hours = 1\naffects_almost_gone_hours = 4"),
            (1, 1)
        );
    }

    #[test]
    fn an_affects_threshold_pick_keeps_almost_gone_under_running_out() {
        let mut ui = UiConfig::default();
        let display = super::apply_affects_display(
            &mut ui,
            super::AffectsDisplayPick {
                running_out: Some(5),
                almost_gone: Some(2),
                ..super::AffectsDisplayPick::default()
            },
        )
        .expect("the hours change");
        assert_eq!((display.running_out, display.almost_gone), (5, 2));
        assert_eq!(display.style, "timers");
        // Running out under almost gone pulls almost gone down with it.
        let display = super::apply_affects_display(
            &mut ui,
            super::AffectsDisplayPick {
                running_out: Some(1),
                ..super::AffectsDisplayPick::default()
            },
        )
        .expect("the hours change");
        assert_eq!((display.running_out, display.almost_gone), (1, 1));
        // Almost gone over running out stops at it, which changes nothing.
        assert_eq!(
            super::apply_affects_display(
                &mut ui,
                super::AffectsDisplayPick {
                    almost_gone: Some(9),
                    ..super::AffectsDisplayPick::default()
                },
            ),
            None
        );
        assert_eq!(
            (ui.affects_running_out_hours, ui.affects_almost_gone_hours),
            (1, 1)
        );
    }

    #[test]
    fn chat_colors_stay_with_each_character_and_out_of_the_whole_config_save() {
        let mut ui = UiConfig::default();
        assert!(ui.chat_colors.is_empty());
        // A profile with no recolor writes no table.
        let plain = ProfileConfig {
            ui: ui.clone(),
            ..ProfileConfig::default()
        }
        .to_toml()
        .unwrap();
        assert!(!plain.contains("chat_colors"));

        ui.chat_colors.insert("say".into(), "brightBlue".into());
        assert_eq!(
            through_toml(&ui).chat_colors.get("say").map(String::as_str),
            Some("brightBlue")
        );
        // A whole config save from Settings carries no chat colors, so it
        // never writes an old copy back over a pick from the pane menu.
        let json = serde_json::to_value(UiConfigPayload::from_ui(&ui)).unwrap();
        assert!(json.get("chat_colors").is_none());
        let mut live = ui.clone();
        UiConfigPayload::from_ui(&UiConfig::default()).apply_to(&mut live);
        assert_eq!(
            live.chat_colors.get("say").map(String::as_str),
            Some("brightBlue")
        );
    }

    #[test]
    fn a_chat_color_pick_writes_only_its_channel() {
        let mut ui = UiConfig::default();
        let colors = super::apply_chat_color(&mut ui, " Say ".into(), Some("brightBlue".into()))
            .expect("a new color changes the table");
        assert_eq!(colors.get("say").map(String::as_str), Some("brightBlue"));
        assert_eq!(colors.len(), 1);

        // The same pick again changes nothing, so nothing is saved or sent.
        assert_eq!(
            super::apply_chat_color(&mut ui, "say".into(), Some("brightBlue".into())),
            None
        );

        let colors = super::apply_chat_color(&mut ui, "tell".into(), Some("red".into()))
            .expect("another channel joins");
        assert_eq!(colors.len(), 2);

        // Default, or a color that is not one of the 16, clears the channel.
        let colors = super::apply_chat_color(&mut ui, "say".into(), None)
            .expect("default clears the channel");
        assert!(!colors.contains_key("say"));
        let colors = super::apply_chat_color(&mut ui, "tell".into(), Some("sparkle".into()))
            .expect("an unknown color clears the channel");
        assert!(colors.is_empty());
        assert_eq!(super::apply_chat_color(&mut ui, "tell".into(), None), None);

        // A blank channel names nothing.
        assert_eq!(
            super::apply_chat_color(&mut ui, "  ".into(), Some("red".into())),
            None
        );
    }

    #[test]
    fn reset_all_clears_every_chat_color_once() {
        let mut ui = UiConfig::default();
        assert_eq!(super::reset_chat_colors(&mut ui), None);
        ui.chat_colors.insert("say".into(), "blue".into());
        ui.chat_colors.insert("yell".into(), "red".into());
        assert_eq!(
            super::reset_chat_colors(&mut ui),
            Some(std::collections::BTreeMap::new())
        );
        assert!(ui.chat_colors.is_empty());
    }

    #[test]
    fn a_profile_without_the_vitals_options_loads_the_defaults() {
        let ui = ProfileConfig::from_toml("[ui]\ntheme = \"nord\"\n")
            .unwrap()
            .ui;
        assert_eq!(ui.vitals_values, "current-max");
        assert_eq!(ui.vitals_meter, "line");
        assert!(!ui.vitals_warn_thirds);
        assert!(ui.vitals_hide_when_pinned);
    }

    #[test]
    fn a_profile_without_the_vitals_density_loads_rows() {
        let ui = ProfileConfig::from_toml("[ui]\ntheme = \"nord\"\n")
            .unwrap()
            .ui;
        assert_eq!(ui.vitals_density, "rows");
    }

    #[test]
    fn a_profile_without_the_appearance_fields_loads_the_defaults() {
        let ui = ProfileConfig::from_toml("[ui]\ntheme = \"nord\"\n")
            .unwrap()
            .ui;
        assert_eq!(ui.theme, "nord");
        assert!(!ui.follow_system_appearance);
        assert_eq!(ui.light_theme, "vellum");
        assert_eq!(ui.dark_theme, "");
        assert_eq!(ui.terminal_line_height, "default");
    }

    #[test]
    fn aliases_export_sends_the_shared_alias_fixture() {
        // The page tests read this file as the reply to aliases_export,
        // so renaming a field fails here instead of leaving the page to
        // read nothing.
        use vosh_automation::alias::{Alias, AliasStore};
        let mut store = AliasStore::new();
        store.set(Alias::new("rec", "recall"));
        store.set(Alias::new("k", "kill %1"));
        store.set(Alias::new("cs", "cast %1").with_group("magic"));
        store.set(Alias::new("lk", "kill %1").with_script("mud.send(\"look\")"));
        store.set(
            Alias::new("lt", "look")
                .with_script("mud.send(\"look \" .. captures[1])\nmud.echo(\"looked\")"),
        );
        let mut off = Alias::new("off", "say off");
        off.enabled = false;
        store.set(off);
        let json = super::aliases_json(&store).unwrap();
        // The file ends in a newline, and the reply does not.
        let fixture = include_str!("../../fixtures/ipc/aliases_export.json");
        assert_eq!(
            fixture.strip_suffix('\n'),
            Some(json.as_str()),
            "fixtures/ipc/aliases_export.json no longer matches aliases_export"
        );
    }

    #[tokio::test]
    async fn hidden_get_answers_what_the_session_last_reported() {
        let state: super::SharedState = std::sync::Arc::new(super::AppState::default());
        let nothing = serde_json::json!({
            "vitals": false, "tank": false, "opponent": false, "affects": false, "group": false,
        });
        let json = |h| serde_json::to_value(h).unwrap();
        assert_eq!(json(super::reported_hidden(&state).await), nothing);
        {
            let mut p = state.profile.lock().await;
            p.prompt.connect(true);
            let at = chrono::Local::now().fixed_offset();
            // The older build names the song and sends the true values.
            p.prompt.vars.observe(
                "Char.Affects",
                serde_json::json!({"affects":[{"name":"lamented tears","kind":"song","duration":3}]}),
                at,
            );
            p.prompt.vars.observe(
                "Char.Vitals",
                serde_json::json!({"hp":850,"maxhp":900,"mana":760,"maxmana":820,"move":250,"maxmove":250}),
                at,
            );
        }
        // Worked out, but the session has not reported it yet.
        assert_eq!(json(super::reported_hidden(&state).await), nothing);
        let reported = state.profile.lock().await.prompt.vars.take_hidden_change();
        assert!(reported.is_some_and(|h| h.vitals() && h.affects && h.group));
        assert_eq!(
            super::reported_hidden(&state).await,
            reported.expect("a report")
        );
    }

    #[test]
    fn settings_window_keeps_a_size_that_fits() {
        let fit = |size| window_fit(&SETTINGS_WINDOW, size);
        assert_eq!(fit((880.0, 600.0)), None);
        assert_eq!(fit((820.0, 560.0)), None);
        assert_eq!(fit((1200.0, 900.0)), None);
    }

    #[test]
    fn reopening_a_window_leaves_a_loading_one_to_its_page() {
        // On screen, or minimized: bring it forward now.
        assert!(shows_on_reopen(true, false));
        assert!(shows_on_reopen(false, true));
        assert!(shows_on_reopen(true, true));
        // Neither: the page has not painted your theme yet, and shows
        // the window itself once it has.
        assert!(!shows_on_reopen(false, false));
    }

    #[test]
    fn settings_window_grows_a_side_left_under_the_minimum() {
        let fit = |size| window_fit(&SETTINGS_WINDOW, size);
        // The old Settings window opened at 780×640.
        assert_eq!(fit((780.0, 640.0)), Some((880.0, 640.0)));
        assert_eq!(fit((900.0, 420.0)), Some((900.0, 600.0)));
        assert_eq!(fit((520.0, 420.0)), Some((880.0, 600.0)));
    }

    #[test]
    fn help_opens_at_the_board_size_on_its_own_page() {
        assert_eq!(HELP_WINDOW.label, "help");
        assert_eq!(HELP_WINDOW.url, "index.html?view=help");
        assert_eq!(HELP_WINDOW.size, (1040.0, 700.0));
        let fit = |size| window_fit(&HELP_WINDOW, size);
        assert_eq!(fit((1040.0, 700.0)), None);
        assert_eq!(fit((860.0, 560.0)), None);
        // A side under the minimum goes back to the board size.
        assert_eq!(fit((700.0, 800.0)), Some((1040.0, 800.0)));
        assert_eq!(fit((900.0, 400.0)), Some((900.0, 700.0)));
    }

    #[test]
    fn scrollback_load_names_the_fields_the_page_reads() {
        let load = ScrollbackLoad {
            bytes: vec![104, 105],
            seeded_native: true,
        };
        assert_eq!(
            serde_json::to_value(&load).unwrap(),
            serde_json::json!({ "bytes": [104, 105], "seeded_native": true })
        );
    }

    mod scope {
        use std::sync::Arc;

        use super::super::{change_scope_locked, AppState, SharedState, PERSIST_LOCK};
        use crate::profile::Profile;
        use crate::profile_config::{
            strip_global_fields, CustomTheme, GlobalConfig, ProfileConfig, TrackedAffect, UiConfig,
        };
        use crate::profile_set::tests::james_like_set;
        use crate::profile_set::{ProfileSet, Scope, ScopeConfig, DEFAULT_PROFILE_NAME};

        fn theme(id: &str, background: &str) -> CustomTheme {
            CustomTheme {
                id: id.into(),
                label: id.into(),
                xterm: [("background".to_string(), background.to_string())]
                    .into_iter()
                    .collect(),
                ..CustomTheme::default()
            }
        }

        fn ids(themes: &[CustomTheme]) -> Vec<&str> {
            themes.iter().map(|t| t.id.as_str()).collect()
        }

        /// The live profile with every shared setting off its default.
        fn shared_profile() -> Profile {
            let mut profile = Profile::default();
            profile.ui.theme = "night-ink".into();
            profile.ui.follow_system_appearance = true;
            profile.ui.light_theme = "classic-vivid".into();
            profile.ui.dark_theme = "night-ink".into();
            profile.ui.custom_themes = vec![theme("night-ink", "#000000")];
            profile.ui.font_family = "Iosevka".into();
            profile.ui.font_size = 16;
            profile.ui.terminal_line_height = "loose".into();
            profile.ui.keep_last_command = true;
            profile.ui.auto_update = true;
            profile
        }

        /// Mirror `persist_profile` for the active profile.
        fn persist(set: &ProfileSet, profile: &Profile) {
            let mut snapshot = ProfileConfig::from_profile(profile);
            strip_global_fields(&mut snapshot, set.scope());
            snapshot.save(&set.active_path()).unwrap();
            GlobalConfig::from_profile(profile, set.scope())
                .save(&set.global_path())
                .unwrap();
        }

        /// Mirror a switch. The active profile file loads first, then the
        /// shared part of global.toml over it.
        fn load(set: &ProfileSet) -> Profile {
            let mut profile = Profile::default();
            let path = set.active_path();
            if path.exists() {
                ProfileConfig::load(&path).unwrap().apply_to(&mut profile);
            }
            if let Some(global) =
                GlobalConfig::load_shared(&set.global_path(), set.scope()).unwrap()
            {
                global.apply_to(&mut profile);
            }
            profile
        }

        fn file(set: &ProfileSet, name: &str) -> ProfileConfig {
            ProfileConfig::load(&set.profile_path(name)).unwrap()
        }

        fn per_profile() -> ScopeConfig {
            ScopeConfig {
                theme: Scope::Profile,
                font: Scope::Profile,
                keep_last_command: Scope::Profile,
                auto_update: Scope::Profile,
                ..ScopeConfig::default()
            }
        }

        /// Default is live and shares everything. Healer saved its file
        /// while everything was shared, so it holds the defaults. Test-Prompt
        /// saved its own theme, font, and custom theme before they were
        /// shared, under the id the live custom theme holds.
        async fn three_profiles(dir: &std::path::Path) -> SharedState {
            let set = james_like_set(dir);
            let live = shared_profile();
            persist(&set, &live);

            let mut healer = ProfileConfig::default();
            healer.ui.tracked_affects = vec![TrackedAffect {
                name: "Fly".into(),
                label: None,
            }];
            healer.save(&set.profile_path("Healer")).unwrap();

            let mut prompt = ProfileConfig::default();
            prompt.ui.theme = "night-ink".into();
            prompt.ui.custom_themes = vec![theme("night-ink", "#ffffff")];
            prompt.ui.font_size = 13;
            prompt.save(&set.profile_path("Test-Prompt")).unwrap();

            let state: SharedState = Arc::new(AppState::default());
            *state.profile.lock().await = live;
            *state.profile_set.lock().await = Some(set);
            state
        }

        /// Mirror `profile_set_scope`. The persist that follows the change
        /// runs under the same lock.
        async fn set_scope(state: &SharedState, scope: ScopeConfig) {
            let _persist_guard = PERSIST_LOCK.lock().await;
            change_scope_locked(state, scope).await.unwrap();
            let live = state.profile.lock().await;
            let guard = state.profile_set.lock().await;
            persist(guard.as_ref().unwrap(), &live);
        }

        #[tokio::test]
        async fn turning_sharing_off_hands_the_shared_settings_to_every_profile() {
            let dir = tempfile::tempdir().unwrap();
            let state = three_profiles(dir.path()).await;
            set_scope(&state, per_profile()).await;

            let mut guard = state.profile_set.lock().await;
            let set = guard.as_mut().unwrap();
            // global.toml no longer holds the categories you turned off.
            let global = GlobalConfig::load(&set.global_path()).unwrap();
            assert!(global.theme.is_none());
            assert!(global.custom_themes.is_none());
            assert!(global.font_size.is_none());
            assert!(global.keep_last_command.is_none());
            assert!(global.auto_update.is_none());

            // Healer held none of its own, so it takes every shared value
            // and keeps what it owns.
            let healer = file(set, "Healer").ui;
            assert_eq!(healer.theme, "night-ink");
            assert!(healer.follow_system_appearance);
            assert_eq!(healer.light_theme, "classic-vivid");
            assert_eq!(healer.dark_theme, "night-ink");
            assert_eq!(ids(&healer.custom_themes), ["night-ink"]);
            assert_eq!(healer.font_family, "Iosevka");
            assert_eq!(healer.font_size, 16);
            assert_eq!(healer.terminal_line_height, "loose");
            assert!(healer.keep_last_command);
            assert!(healer.auto_update);
            assert_eq!(healer.tracked_affects.len(), 1);

            // Test-Prompt keeps its own theme and font, and its own custom
            // theme moves to a fresh id beside the shared one.
            let prompt = file(set, "Test-Prompt").ui;
            assert_eq!(ids(&prompt.custom_themes), ["night-ink", "night-ink-2"]);
            assert_eq!(prompt.custom_themes[1], {
                let mut own = theme("night-ink-2", "#ffffff");
                own.label = "night-ink (Test-Prompt)".into();
                own
            });
            assert_eq!(prompt.theme, "night-ink-2");
            assert_eq!(prompt.font_size, 13);
            assert_eq!(prompt.font_family, UiConfig::default().font_family);
            assert!(prompt.keep_last_command);

            // A switch to Healer shows what it showed while shared.
            set.switch("Healer").unwrap();
            let healer = load(set);
            assert_eq!(healer.ui.theme, "night-ink");
            assert_eq!(healer.ui.font_size, 16);
            assert!(healer.ui.keep_last_command);
            // The live profile kept its values in its own file.
            set.switch(DEFAULT_PROFILE_NAME).unwrap();
            let live = load(set);
            assert_eq!(live.ui.theme, "night-ink");
            assert_eq!(live.ui.font_size, 16);
        }

        #[tokio::test]
        async fn sharing_again_after_turning_it_off_keeps_every_theme() {
            let dir = tempfile::tempdir().unwrap();
            let state = three_profiles(dir.path()).await;
            set_scope(&state, per_profile()).await;
            set_scope(&state, ScopeConfig::default()).await;

            let live = state.profile.lock().await;
            assert_eq!(ids(&live.ui.custom_themes), ["night-ink", "night-ink-2"]);
            let guard = state.profile_set.lock().await;
            let set = guard.as_ref().unwrap();
            let global = GlobalConfig::load(&set.global_path()).unwrap();
            assert_eq!(
                ids(&global.custom_themes.unwrap()),
                ["night-ink", "night-ink-2"]
            );
            // Test-Prompt still points at its own theme for the next time
            // you turn sharing off.
            assert_eq!(file(set, "Test-Prompt").ui.theme, "night-ink-2");
        }

        #[tokio::test]
        async fn a_profile_that_never_saved_takes_the_shared_settings() {
            let dir = tempfile::tempdir().unwrap();
            let state = three_profiles(dir.path()).await;
            state
                .profile_set
                .lock()
                .await
                .as_mut()
                .unwrap()
                .create("Bard")
                .unwrap();
            set_scope(
                &state,
                ScopeConfig {
                    font: Scope::Profile,
                    ..ScopeConfig::default()
                },
            )
            .await;

            let guard = state.profile_set.lock().await;
            let set = guard.as_ref().unwrap();
            let bard = file(set, "Bard").ui;
            assert_eq!(bard.font_size, 16);
            assert_eq!(bard.terminal_line_height, "loose");
            // The theme is still shared, so the file keeps the defaults.
            let leftover = &bard.custom_themes;
            assert!(leftover.is_empty(), "{leftover:?}");
        }

        #[tokio::test]
        async fn a_file_vosh_cannot_read_keeps_the_settings_shared() {
            let dir = tempfile::tempdir().unwrap();
            let state = three_profiles(dir.path()).await;
            let (healer_path, global_path) = {
                let guard = state.profile_set.lock().await;
                let set = guard.as_ref().unwrap();
                std::fs::write(set.profile_path("Test-Prompt"), "theme = [").unwrap();
                (set.profile_path("Healer"), set.global_path())
            };
            let healer_before = std::fs::read_to_string(&healer_path).unwrap();
            let global_before = std::fs::read_to_string(&global_path).unwrap();

            let refused = {
                let _persist_guard = PERSIST_LOCK.lock().await;
                change_scope_locked(&state, per_profile()).await
            };

            let message = refused.unwrap_err();
            assert_eq!(
                message,
                "Vosh could not read the Test-Prompt profile file, so these settings stay the same for every character."
            );
            let guard = state.profile_set.lock().await;
            assert_eq!(guard.as_ref().unwrap().scope().theme, Scope::Global);
            assert_eq!(
                std::fs::read_to_string(&healer_path).unwrap(),
                healer_before
            );
            assert_eq!(
                std::fs::read_to_string(&global_path).unwrap(),
                global_before
            );
        }

        #[test]
        fn stopped_sharing_names_only_the_categories_turned_off() {
            let shared = ScopeConfig::default();
            assert!(shared.stopped_sharing(&shared).is_none());
            let stopped = shared.stopped_sharing(&per_profile()).unwrap();
            assert_eq!(stopped.theme, Scope::Global);
            assert_eq!(stopped.font, Scope::Global);
            assert_eq!(stopped.dock_layout, Scope::Profile);
            assert!(per_profile().stopped_sharing(&shared).is_none());
        }

        #[tokio::test]
        async fn turning_sharing_on_writes_no_other_profile_file() {
            let dir = tempfile::tempdir().unwrap();
            let state = three_profiles(dir.path()).await;
            let healer_before = {
                let guard = state.profile_set.lock().await;
                std::fs::read_to_string(guard.as_ref().unwrap().profile_path("Healer")).unwrap()
            };
            set_scope(&state, ScopeConfig::default()).await;
            let guard = state.profile_set.lock().await;
            let healer_after =
                std::fs::read_to_string(guard.as_ref().unwrap().profile_path("Healer")).unwrap();
            assert_eq!(healer_after, healer_before);
        }
    }
}

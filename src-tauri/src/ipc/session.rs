//! The commands for your connection to the game. The page connects and
//! disconnects through them, sends the lines you type, plain or masked,
//! tells the game the size of the terminal, and reads the target you track.

use tauri::{AppHandle, Emitter, State};

use crate::app::events::{self, ListChanges, ListRevisions, HELP_OPEN};
use crate::app::state::{note_ui_config_replaced, SharedState};
use crate::disk::save::settle_line_effects;
use crate::input;
use crate::profile::switch::read_shared_layer;
use crate::prompt::{prompt_look, request_prompt_repaint};
use crate::script::ApplyResult;
use crate::session::{self, TargetPayload};

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
        .map(|dir| crate::logs::scrollback_path(&dir));

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
        crate::logs::forget_passwords::start(&app, command);
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
    if !handle.send_masked(crate::session::echo::masked_line_bytes(&line)) {
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

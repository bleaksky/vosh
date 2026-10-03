//! The commands for your connection to the game. The page connects and
//! disconnects through them, sends the lines you type, plain or masked,
//! tells the game the size of the terminal, and reads the target you track.

use tauri::{AppHandle, Emitter, State};

use crate::app::events;
use crate::app::state::SharedState;
use crate::input;
use crate::output;
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
    let known_host = crate::profile_set::is_forsaken_lands(&host);

    let spawned = session::spawn(
        app.clone(),
        host,
        port,
        tls,
        known_host,
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

#[tauri::command]
pub(crate) async fn session_send_input<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    line: String,
) -> Result<(), String> {
    input::run_typed_line(&app, state.inner(), &line).await
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
        output::emit_output(&app, input::NOT_CONNECTED.to_vec());
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

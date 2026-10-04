//! The commands for your sessions and their connections to the game.
//! The page opens and selects a session, connects and disconnects
//! through them, sends the lines you type, plain or masked, stops a walk
//! on Esc, tells the game the size of the terminal, and reads the target
//! you track. Each acts on the session it names, or on the selected
//! session when it names none.

use tauri::{AppHandle, State};

use crate::app::state::SharedState;
use crate::input;
use crate::output;
use crate::session::TargetPayload;
use crate::sessions::SessionId;

/// Open a session after the others, with nothing connected, and return
/// its id. It plays the live profile, so its connection takes the
/// profile's tick settings and `[prompt]` table, as the first session's
/// does at launch.
#[tauri::command]
pub(crate) async fn session_open(state: State<'_, SharedState>) -> Result<SessionId, String> {
    let session = state.open_session();
    let mut p = state.profile.lock().await;
    let tick_before = p.tick.config.clone();
    let mut c = session.connection.lock();
    crate::profile::switch::hand_to_connection(&mut p, &mut c, &tick_before);
    Ok(session.id)
}

/// Select the session `session` names. The commands that name no session
/// act on it from then on.
#[tauri::command]
pub(crate) fn session_select(
    state: State<'_, SharedState>,
    session: SessionId,
) -> Result<(), String> {
    state.select_session(session)
}

#[tauri::command]
pub(crate) async fn session_connect(
    app: AppHandle,
    state: State<'_, SharedState>,
    host: String,
    port: u16,
    tls: bool,
    session: Option<SessionId>,
) -> Result<(), String> {
    let session = state.session(session)?;
    crate::session::connect(&app, state.inner(), &session, host, port, tls).await
}

#[tauri::command]
pub(crate) async fn session_send_input<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    line: String,
    session: Option<SessionId>,
) -> Result<(), String> {
    let session = state.session(session)?;
    input::run_typed_line(&app, state.inner(), &session, &line).await
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
    session: Option<SessionId>,
) -> Result<(), String> {
    let session = state.session(session)?;
    let current = session.slot.lock().await;
    let Some(handle) = current.as_ref() else {
        output::emit_output(&app, &session, input::NOT_CONNECTED.to_vec());
        return Ok(());
    };
    if !handle.send_masked(crate::session::echo::masked_line_bytes(&line)) {
        return Err("session task gone".to_string());
    }
    Ok(())
}

/// Stop the walk under way, as Esc in the command line does. It says
/// nothing when you are not walking or not connected.
#[tauri::command]
pub(crate) async fn session_walk_stop(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<(), String> {
    let session = state.session(session)?;
    if let Some(handle) = session.slot.lock().await.as_ref() {
        let _ = handle.walk(crate::input::walk::WalkCommand::Stop {
            key: true,
            rest: Vec::new(),
        });
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn session_disconnect(
    app: AppHandle,
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<(), String> {
    let session = state.session(session)?;
    crate::session::disconnect(&app, state.inner(), &session).await;
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
    session: Option<SessionId>,
) -> Result<(), String> {
    let session = state.session(session)?;
    // Always cache the size — even when no session exists, so the
    // next `session_connect` can seed the negotiator with the real
    // dimensions instead of the 80×24 default. Without this the
    // server's first NAWS reply (during the early handshake) would
    // carry the wrong size and wrap early output until the next
    // user-driven resize triggered a fresh subneg.
    if let Ok(mut guard) = session.window_size.lock() {
        *guard = (cols, rows);
    }
    let current = session.slot.lock().await;
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
pub(crate) async fn target_get(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<TargetPayload, String> {
    let session = state.session(session)?;
    let c = session.connection.lock();
    Ok(TargetPayload::of(&c))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use tauri::test::{mock_builder, mock_context, noop_assets};
    use tauri::Manager;

    use crate::app::state::{AppState, SharedState};

    #[tokio::test]
    async fn your_target_answers_while_the_profile_is_busy() {
        let app = mock_builder().build(mock_context(noop_assets())).unwrap();
        app.manage::<SharedState>(Arc::new(AppState::default()));
        let state: SharedState = app.state::<SharedState>().inner().clone();
        let session = state.selected_session();
        session.connection.lock().target.name = Some("goblin".into());

        // Another task holds the profile, as the session loop does while
        // it runs a line.
        let (held_tx, held) = tokio::sync::oneshot::channel();
        let (release, release_rx) = tokio::sync::oneshot::channel::<()>();
        let profile = state.profile.clone();
        let holder = tokio::spawn(async move {
            let _p = profile.lock().await;
            let _ = held_tx.send(());
            let _ = release_rx.await;
        });
        held.await.unwrap();

        let answer = tokio::time::timeout(
            Duration::from_secs(1),
            super::target_get(app.state::<SharedState>(), None),
        )
        .await
        .expect("the target answers without the profile")
        .unwrap();
        assert_eq!(answer.name.as_deref(), Some("goblin"));

        release.send(()).unwrap();
        holder.await.unwrap();
    }

    #[tokio::test]
    async fn a_new_session_takes_the_prompt_table_of_the_profile_it_plays() {
        let app = mock_builder().build(mock_context(noop_assets())).unwrap();
        app.manage::<SharedState>(Arc::new(AppState::default()));
        let state: SharedState = app.state::<SharedState>().inner().clone();
        {
            let mut p = state.profile.lock().await;
            p.prompt.draw = true;
            p.prompt.template = "<%hp>".into();
        }

        let id = super::session_open(app.state::<SharedState>())
            .await
            .unwrap();
        let session = state.session(Some(id)).unwrap();
        let p = state.profile.lock().await;
        let c = session.connection.lock();
        assert_eq!(*c.prompt.config(), p.prompt);
        assert_eq!(p.prompt.template, "<%hp>");
    }
}

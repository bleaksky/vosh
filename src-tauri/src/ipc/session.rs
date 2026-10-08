//! The commands for your sessions and their connections to the game.
//! The page lists, opens, selects, renames, moves and closes sessions,
//! keeps where each one dials, connects and disconnects through them,
//! sends the lines you type, plain, masked or raw into the game's editor,
//! walks the path you click on
//! the map, stops a walk on Esc, tells
//! the game the size of the terminal, and reads the target you track.
//! Each acts on the session it names, or on the selected session when it
//! names none. Every window hears the rows again after a step that
//! changes what one shows, see [`crate::sessions::broadcast_sessions`].

use tauri::{AppHandle, Manager, State};

use crate::app::state::SharedState;
use crate::disk::save::{persist_state, PERSIST_LOCK};
use crate::input;
use crate::output;
use crate::profile::set::save_sessions;
use crate::session::TargetPayload;
use crate::sessions::{broadcast_sessions, Address, SessionId, SessionRow};

/// Open a session after the others, with nothing connected, and return
/// its id. It plays `profile`, which it joins when another session plays
/// it and otherwise opens from its files, or with no `profile` the one
/// the selected session plays. Its connection takes the profile's tick
/// settings and `[prompt]` table, and its Lua engine loads the plugins
/// the profile turns on, as the first session's does at launch.
/// profiles.toml keeps it in the list a launch restores.
#[tauri::command]
pub(crate) async fn session_open<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    profile: Option<String>,
) -> Result<SessionId, String> {
    let session = {
        // No other step opens or closes a profile until the session plays
        // it.
        let _persist_guard = PERSIST_LOCK.lock().await;
        let open = match profile {
            Some(name) => crate::profile::switch::open_or_join(state.inner(), &name).await?,
            None => state.selected_session().profile(),
        };
        let session = state.open_session(open);
        save_sessions(state.inner()).await;
        broadcast_sessions(&app, state.inner());
        session
    };
    crate::app::launch::start_on_profile(&app, state.inner(), &session).await;
    Ok(session.id)
}

/// Select the session `session` names. The commands that name no session
/// act on it from then on, and its native grid shows. A session launch
/// restored opens its profile and reads its scrollback the first time,
/// see [`crate::app::launch::open_restored`]. profiles.toml then keeps it
/// as the selected one, and names the profile it plays as the active one.
#[tauri::command]
pub(crate) async fn session_select<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    session: SessionId,
) -> Result<(), String> {
    crate::app::launch::select_session(&app, state.inner(), session).await
}

/// Close the session `session` names. Its connection ends as on
/// Disconnect, and its grid, its scrollback file and the Lua stops it
/// made go. With it go its
/// connection's state, its Lua engine with the aliases its plugins made,
/// its recording and its snoop window. Its profile saves, unless
/// `#profile reset` or `#profile load` holds it, and closes when no other
/// session plays it.
/// A session that was selected hands the selection on, see
/// [`crate::sessions::Sessions::close`], and every window hears what the
/// next one brings to the front, see
/// [`crate::app::launch::show_selection`]. profiles.toml leaves it out of
/// the list a launch restores. Vosh never closes the only session,
/// since closing it closes the window.
#[tauri::command]
pub(crate) async fn session_close<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    session: SessionId,
) -> Result<(), String> {
    // Under the save lock, so a switch that found its session the last
    // on a profile still finds it so when it closes that profile.
    let (closed, front) = {
        let _persist_guard = PERSIST_LOCK.lock().await;
        let selected = state.selected_session();
        let closed = state.close_session(session)?;
        // A selected session that closes hands the selection on.
        (closed, (selected.id == session).then(|| selected.profile()))
    };
    crate::session::disconnect(&app, state.inner(), &closed).await;
    if let Some(window) = app.get_webview_window(&crate::app::windows::snoop_label(closed.id)) {
        let _ = window.close();
    }
    if let Some(app_data) = state.app_data.get() {
        let _ = std::fs::remove_file(crate::disk::paths::scrollback_path(app_data, closed.id));
    }
    let persist_guard = PERSIST_LOCK.lock().await;
    let open = closed.profile();
    {
        let mut p = open.lock().await;
        let key = closed.id.stop_key();
        p.triggers.forget_stops(key);
        p.aliases.forget_stops(key);
    }
    if !open.held() {
        persist_state(state.inner(), &open).await;
    }
    if state.close_unplayed(&open) {
        crate::profile::switch::leave_file(state.inner(), &open).await;
    }
    #[cfg(any(native_surface, test))]
    crate::native::grid::forget(closed.id);
    save_sessions(state.inner()).await;
    broadcast_sessions(&app, state.inner());
    drop(persist_guard);
    if let Some(front) = front {
        crate::app::launch::show_selection(&app, state.inner(), &front).await;
    }
    Ok(())
}

/// Give the session `session` names the name `name`, which its row and
/// the lines other sessions print read in place of its character. With
/// no name, or a blank one, the session reads its character again.
/// profiles.toml keeps the name for the next launch.
#[tauri::command]
pub(crate) async fn session_rename<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    session: SessionId,
    name: Option<String>,
) -> Result<(), String> {
    state.session(Some(session))?.rename(name.as_deref());
    let _persist_guard = PERSIST_LOCK.lock().await;
    save_sessions(state.inner()).await;
    broadcast_sessions(&app, state.inner());
    Ok(())
}

/// Move the session `session` names to the place `to` in the list, or to
/// its end when `to` lies past it, as a drag of its row does. The
/// selection stays, and profiles.toml keeps the order for the next
/// launch (Q18).
#[tauri::command]
pub(crate) async fn session_move<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    session: SessionId,
    to: usize,
) -> Result<(), String> {
    // Under the save lock, so no step opens, closes or moves a session
    // between the move and the save.
    let _persist_guard = PERSIST_LOCK.lock().await;
    state.move_session(session, to)?;
    save_sessions(state.inner()).await;
    broadcast_sessions(&app, state.inner());
    Ok(())
}

/// What a session's place to dial says without a host or a port.
const NO_ADDRESS: &str = "Give the session a host and a port to dial.";

/// Keep `host` on `port`, over TLS when `tls` says so, as where the
/// session `session` names dials, without dialing, as the session form
/// saves it. Each session keeps its own (board 7 and Q12). Its row names
/// that world from then on, and profiles.toml keeps it for the next
/// launch while it keeps the list, see
/// [`crate::profile::set::SessionEntry::list`]. A blank host or port 0
/// is refused.
#[tauri::command]
pub(crate) async fn session_set_address<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    session: SessionId,
    host: String,
    port: u16,
    tls: bool,
) -> Result<(), String> {
    let host = host.trim();
    if host.is_empty() || port == 0 {
        return Err(NO_ADDRESS.into());
    }
    let session = state.session(Some(session))?;
    *session
        .address
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Address {
        host: host.to_string(),
        port,
        tls,
    });
    let _persist_guard = PERSIST_LOCK.lock().await;
    save_sessions(state.inner()).await;
    broadcast_sessions(&app, state.inner());
    Ok(())
}

/// Every session in the order the window lists them.
#[tauri::command]
pub(crate) fn sessions_list(state: State<'_, SharedState>) -> Vec<SessionRow> {
    state.session_rows()
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

/// Send a line you type into the game's line editor while it holds a
/// text Vosh can name, exactly as typed (Description Editor Q3). No
/// alias, variable, `#` command or semicolon split sees it, and its
/// leading spaces stay, since every line there is part of your text. It
/// goes to the log as typed.
#[tauri::command]
pub(crate) async fn session_send_raw<R: tauri::Runtime>(
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
    if !handle.send(format!("{line}\r\n").into_bytes()) {
        return Err("session task gone".to_string());
    }
    Ok(())
}

/// Walk the path you clicked on the map: `steps` as a `#walk` string,
/// planned from room `start`, with the room each step should reach in
/// `rooms`. A walk under way gives way once its step in flight lands,
/// and the walker drops a path planned from a room you have since left.
/// It says nothing when you are not connected.
#[tauri::command]
pub(crate) async fn session_walk_route(
    state: State<'_, SharedState>,
    steps: String,
    start: i64,
    rooms: Vec<i64>,
    session: Option<SessionId>,
) -> Result<(), String> {
    use crate::input::walk::{parse_steps, Route, WalkCommand, WalkPlan};
    let steps = parse_steps(&steps).map_err(|e| e.to_string())?;
    if rooms.len() != steps.len() {
        return Err(format!(
            "The path has {} steps but names {} rooms.",
            steps.len(),
            rooms.len()
        ));
    }
    let session = state.session(session)?;
    if let Some(handle) = session.slot.lock().await.as_ref() {
        let _ = handle.walk(WalkCommand::Start {
            plan: WalkPlan {
                steps,
                route: Some(Route { start, rooms }),
            },
            rest: Vec::new(),
        });
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

/// What a session that waits to redial says when it does not.
const NOT_REDIALING: &str = "Vosh is not waiting to reconnect this session.";

/// Dial at once in the series of redials `session` runs after a drop,
/// Reconnect now on the notice. Its count goes on from the try it dials.
#[tauri::command]
pub(crate) async fn session_reconnect_now(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<(), String> {
    let session = state.session(session)?;
    if session.redial_now() {
        Ok(())
    } else {
        Err(NOT_REDIALING.into())
    }
}

/// End the series of redials `session` runs after a drop, Cancel on the
/// notice.
#[tauri::command]
pub(crate) async fn session_reconnect_cancel<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<(), String> {
    let session = state.session(session)?;
    crate::session::reconnect::cancel(&app, &session).await;
    Ok(())
}

/// Whether `profile`, or the profile the selected session plays when it
/// names none, dials again after the link drops while you play (Alerts
/// Q14). On at first.
#[tauri::command]
pub(crate) async fn reconnect_get(
    state: State<'_, SharedState>,
    profile: Option<String>,
) -> Result<bool, String> {
    Ok(state.lock_named(profile).await?.reconnect.is_on())
}

/// Turn Reconnect when the link drops on or off for `profile`, or for the
/// profile the selected session plays when it names none, and save. A
/// series that runs goes on.
#[tauri::command]
pub(crate) async fn reconnect_set<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    on: bool,
    profile: Option<String>,
) -> Result<(), String> {
    let open = {
        let mut p = state.lock_named(profile).await?;
        p.reconnect = crate::profile::file::OnSwitch(on);
        p.open().clone()
    };
    crate::disk::save::save_by(&app, &state, &open, crate::disk::save::SavePolicy::Now).await;
    Ok(())
}

/// Inform the session of a new terminal size. The backend updates the
/// telnet negotiator and, when NAWS has already been negotiated with
/// the server, pushes a NAWS subnegotiation so the MUD re-wraps its
/// output at the new column count. No-op when not connected. While
/// another session's grid shows, the session's native grid takes the
/// size too, since no frame sizes it.
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
    #[cfg(any(native_surface, test))]
    crate::native::grid::size_hidden(session.id, usize::from(cols), usize::from(rows));
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
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use serde_json::{json, Value};
    use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
    use tauri::{App, Listener, Manager};

    use crate::app::state::{AppState, SharedState};
    use crate::sessions::SessionId;

    /// A mock app that holds a fresh state.
    fn app() -> App<MockRuntime> {
        let app = mock_builder().build(mock_context(noop_assets())).unwrap();
        app.manage::<SharedState>(Arc::new(AppState::default()));
        app
    }

    /// Every list of rows the app sends from now on.
    fn hear_rows(app: &App<MockRuntime>) -> Arc<Mutex<Vec<Value>>> {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let keep = heard.clone();
        app.listen_any(crate::app::events::SESSIONS_CHANGED, move |e| {
            let rows = serde_json::from_str(e.payload()).expect("a JSON payload");
            keep.lock().expect("the rows").push(rows);
        });
        heard
    }

    /// The id, name and selection of each row the app sent last, and how
    /// many lists it sent so far.
    fn last_rows(heard: &Mutex<Vec<Value>>) -> (Value, usize) {
        let heard = heard.lock().expect("the rows");
        let last = heard.last().and_then(Value::as_array).map(|rows| {
            rows.iter()
                .map(|row| json!([row["id"], row["name"], row["selected"]]))
                .collect()
        });
        (last.unwrap_or_default(), heard.len())
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn every_window_hears_the_rows_after_each_change_to_the_list() {
        // A selection shows the session's grid, which other tests read.
        let _grid = crate::native::grid::lock_shared_grid_for_test();
        let app = app();
        let heard = hear_rows(&app);
        let one = app.state::<SharedState>().selected_session().id;
        let mut sent = 0;
        // Each step sends the rows as they then stand.
        let mut after = |rows: Value| {
            let (last, count) = last_rows(&heard);
            assert!(count > sent, "no rows went out");
            sent = count;
            assert_eq!(last, rows);
        };

        let two = super::session_open(app.handle().clone(), app.state(), None)
            .await
            .unwrap();
        after(json!([[one, null, true], [two, null, false]]));
        super::session_select(app.handle().clone(), app.state(), two)
            .await
            .unwrap();
        after(json!([[one, null, false], [two, null, true]]));
        super::session_rename(app.handle().clone(), app.state(), two, Some("Alt".into()))
            .await
            .unwrap();
        after(json!([[one, null, false], [two, "Alt", true]]));
        super::session_move(app.handle().clone(), app.state(), two, 0)
            .await
            .unwrap();
        after(json!([[two, "Alt", true], [one, null, false]]));
        let host = " play.theforsakenlands.com ".to_string();
        super::session_set_address(app.handle().clone(), app.state(), two, host, 1825, true)
            .await
            .unwrap();
        let rows = heard.lock().unwrap().last().cloned().unwrap_or_default();
        assert_eq!(
            (&rows[0]["host"], &rows[0]["port"], &rows[0]["tls"]),
            (
                &json!("play.theforsakenlands.com"),
                &json!(1825),
                &json!(true)
            )
        );
        after(json!([[two, "Alt", true], [one, null, false]]));
        super::session_close(app.handle().clone(), app.state(), two)
            .await
            .unwrap();
        after(json!([[one, null, true]]));
        // A step that fails changes no row and sends none.
        let gone = super::session_select(app.handle().clone(), app.state(), SessionId::numbered(9));
        assert!(gone.await.is_err());
        assert_eq!(last_rows(&heard).1, sent);
    }

    #[tokio::test]
    async fn a_session_needs_a_host_and_a_port_to_dial() {
        let app = app();
        let state: SharedState = app.state::<SharedState>().inner().clone();
        let one = state.selected_session();
        let set = |session, host: &str, port| {
            let host = host.to_string();
            super::session_set_address(
                app.handle().clone(),
                app.state(),
                session,
                host,
                port,
                false,
            )
        };
        for (host, port) in [(" ", 1848), ("play.theforsakenlands.com", 0)] {
            let refused = set(one.id, host, port).await;
            assert_eq!(refused, Err(super::NO_ADDRESS.to_string()));
        }
        assert!(one.address.lock().unwrap().is_none());
        let gone = set(SessionId::numbered(9), "play.theforsakenlands.com", 1848).await;
        assert_eq!(gone, Err(crate::sessions::NO_SUCH_SESSION.to_string()));
    }

    #[tokio::test]
    async fn your_target_answers_while_the_profile_is_busy() {
        let app = app();
        let state: SharedState = app.state::<SharedState>().inner().clone();
        let session = state.selected_session();
        session.connection.lock().target.name = Some("goblin".into());

        // Another task holds the profile, as the session loop does while
        // it runs a line.
        let (held_tx, held) = tokio::sync::oneshot::channel();
        let (release, release_rx) = tokio::sync::oneshot::channel::<()>();
        let session = state.selected_session();
        let holder = tokio::spawn(async move {
            let _p = session.lock_profile().await;
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
        let app = app();
        let state: SharedState = app.state::<SharedState>().inner().clone();
        {
            let mut p = state.selected_profile().await;
            p.prompt.draw = true;
            p.prompt.template = "<%hp>".into();
        }

        let id = super::session_open(app.handle().clone(), app.state::<SharedState>(), None)
            .await
            .unwrap();
        let session = state.session(Some(id)).unwrap();
        let p = state.selected_profile().await;
        let c = session.connection.lock();
        assert_eq!(*c.prompt.config(), p.prompt);
        assert_eq!(p.prompt.template, "<%hp>");
    }
}

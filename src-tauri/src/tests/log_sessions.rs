//! Log sessions. A connection writes the session log unless its
//! profile turns Log sessions off, and until you choose, a connection to
//! this computer writes none. Each test runs the real session loop with
//! the mock runtime against a game on a local port.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri::Manager;
use tokio::net::TcpListener;

use crate::app::state::{AppState, SharedState};

/// Connect the selected session of `state` to a game on a local port
/// and end the connection once the game took it, so its log row, if it
/// opened one, is closed.
async fn connect_once(state: &SharedState) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a local port");
    let port = listener.local_addr().expect("an address").port();
    let app = mock_builder()
        .build(mock_context(noop_assets()))
        .expect("a mock app");
    app.manage::<SharedState>(state.clone());
    let accept = tokio::spawn(async move { listener.accept().await.expect("a client").0 });
    let handle = crate::session::spawn(
        app.handle().clone(),
        state,
        &state.selected_session(),
        "127.0.0.1".into(),
        port,
        false,
        false,
        None,
        (100, 40),
    )
    .await
    .expect("the game answers");
    let _socket = accept.await.expect("the accept task");
    handle.shutdown().await;
}

/// How many logs the store holds.
async fn logs(state: &SharedState) -> usize {
    let guard = state.logs.lock().await;
    let store = guard.as_ref().expect("the log");
    store
        .list_sessions(0, &vosh_log::Scope::default())
        .expect("the logs")
        .len()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_connection_to_this_computer_logs_only_once_you_turn_it_on() {
    let dir = tempfile::tempdir().expect("a temporary folder");
    let state: SharedState = Arc::new(AppState::default());
    state.log_this_computer.store(false, Ordering::Release);
    *state.logs.lock().await =
        Some(vosh_log::LogStore::open(&dir.path().join("logs.sqlite")).expect("the log"));

    connect_once(&state).await;
    assert_eq!(logs(&state).await, 0, "no log until you choose");
    assert_eq!(state.selected_session().logs(), Vec::<i64>::new());

    state.selected_profile().await.ui.log_sessions = Some(true);
    connect_once(&state).await;
    assert_eq!(logs(&state).await, 1, "your choice wins");
    assert_eq!(state.selected_session().logs().len(), 1);

    state.selected_profile().await.ui.log_sessions = Some(false);
    connect_once(&state).await;
    assert_eq!(logs(&state).await, 1, "off writes no log");
}

//! Scrollback size from the field you pick to the ring each
//! session keeps: a new size trims every session on the profile, a
//! session takes its profile's size as it connects, and a launch reads
//! as much of the scrollback file as the size keeps.

use std::sync::Arc;

use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, Manager};
use tokio::net::TcpListener;

use crate::app::state::{AppState, SharedState};
use crate::profile::live::Profile;
use crate::profile::set::ProfileSet;
use crate::sessions::Session;

/// A mock app that holds `state`.
fn app_for(state: &SharedState) -> App<MockRuntime> {
    let app = mock_builder()
        .build(mock_context(noop_assets()))
        .expect("a mock app");
    app.manage::<SharedState>(state.clone());
    app
}

/// Keep `n` numbered lines in the ring of `session`.
async fn fill(session: &Session, n: usize) {
    let mut ring = session.scrollback.lock().await;
    for i in 0..n {
        ring.push(format!("Tolliver says, 'line {i}.'").into_bytes());
    }
}

/// How many lines the ring of `session` keeps, and its newest one.
async fn kept(session: &Session) -> (usize, String) {
    let ring = session.scrollback.lock().await;
    let last = ring.lines().last().unwrap_or_default();
    (
        ring.lines().count(),
        String::from_utf8(last.to_vec()).expect("text"),
    )
}

#[tokio::test]
async fn a_new_scrollback_size_trims_every_session_on_its_profile_and_no_other() {
    let dir = tempfile::tempdir().expect("a temporary folder");
    let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).expect("a set");
    set.create("Orla").expect("Orla");
    set.create("Maren").expect("Maren");
    let state: SharedState = Arc::new(AppState::default());
    state.set_profiles(set).await;
    let orla = state.add_open_profile("Orla", Profile::default());
    let maren = state.add_open_profile("Maren", Profile::default());
    let sessions = [
        state.open_session(orla.clone()),
        state.open_session(orla.clone()),
        state.open_session(maren),
    ];
    for session in &sessions {
        fill(session, 1_500).await;
    }
    let app = app_for(&state);

    // The page sends Scrollback size as Settings' General tab does.
    let field = serde_json::from_value(serde_json::json!({
        "field": "scrollback_lines",
        "value": 1_000,
    }))
    .expect("a field");
    crate::ipc::ui_config::ui_set_fields(
        app.handle().clone(),
        app.state(),
        vec![field],
        Some("Orla".into()),
    )
    .await
    .expect("the size saves");

    assert_eq!(orla.lock().await.ui.scrollback_lines, 1_000);
    let newest = "Tolliver says, 'line 1499.'".to_string();
    assert_eq!(kept(&sessions[0]).await, (1_000, newest.clone()));
    assert_eq!(kept(&sessions[1]).await, (1_000, newest.clone()));
    assert_eq!(
        kept(&sessions[2]).await,
        (1_500, newest),
        "Maren keeps hers"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_takes_its_profile_scrollback_size_as_it_connects() {
    let state: SharedState = Arc::new(AppState::default());
    let session = state.selected_session();
    fill(&session, 1_500).await;
    // A size the profile took while the session was not on it, such as
    // from its file.
    state.selected_profile().await.ui.scrollback_lines = 1_000;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a local port");
    let port = listener.local_addr().expect("an address").port();
    let accept = tokio::spawn(async move { listener.accept().await.expect("a client").0 });
    let app = app_for(&state);

    let handle = crate::session::spawn(
        app.handle().clone(),
        &state,
        &session,
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
    assert_eq!(
        kept(&session).await,
        (1_000, "Tolliver says, 'line 1499.'".into())
    );
    handle.shutdown().await;
}

#[tokio::test]
async fn a_launch_reads_as_much_scrollback_as_the_profile_keeps() {
    let dir = tempfile::tempdir().expect("a temporary folder");
    let root = dir.path();
    let lines: Vec<String> = (0..25_000)
        .map(|i| format!("Maren says, 'line {i}.'\r\n"))
        .collect();
    let newest = "Maren says, 'line 24999.'".to_string();

    // At 25,000 lines the whole file comes back.
    let state: SharedState = Arc::new(AppState::default());
    let session = state.selected_session();
    std::fs::write(
        crate::disk::paths::scrollback_path(root, session.id),
        lines.concat(),
    )
    .expect("the scrollback");
    state.selected_profile().await.ui.scrollback_lines = 25_000;
    let app = app_for(&state);
    crate::app::launch::start_selected(app.handle(), &state, root).await;
    assert_eq!(kept(&session).await, (25_000, newest.clone()));

    // At the 10,000 lines of a new profile only the newest come back.
    let state: SharedState = Arc::new(AppState::default());
    let session = state.selected_session();
    let app = app_for(&state);
    crate::app::launch::start_selected(app.handle(), &state, root).await;
    assert_eq!(kept(&session).await, (10_000, newest));
}

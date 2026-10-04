//! Two sessions on the one profile, each against a fake game of its own,
//! through the harness of the fake MUD tests. Each test holds a rule
//! that keeps one session's connection apart from the other's.
//!
//! Every session output also feeds the shared native grid, so each test
//! holds its guard to keep the others off it. No task of a session takes
//! it.

use serde_json::json;
use tauri::Manager;
use vosh_prompt::testkit::{Build, Options};

use super::fake_mud::harness::{FakeServer, Harness};
use crate::profile::set::DEFAULT_PROFILE_NAME;
use crate::sessions::SessionId;

/// What clients sent `server`, as text.
fn sent(server: &FakeServer) -> String {
    String::from_utf8_lossy(&server.received.lock().expect("the bytes")).into_owned()
}

/// Whether a row on the terminal of `session` shows `text`.
fn shows(h: &Harness, session: SessionId, text: &str) -> bool {
    h.screen_of(session).iter().any(|row| row.contains(text))
}

/// The harness with a second session, the first session logged in to the
/// first game and the second to the second.
async fn two_sessions_on_two_games() -> (Harness, SessionId, SessionId) {
    let h = Harness::new(Options::new(Build::New)).await;
    let (one, two) = (h.first, h.open_session().await);
    h.connect_to(one, &h.servers[0]).await;
    h.connect_to(two, &h.servers[1]).await;
    let welcome = "Welcome to the fake Aabahran, Tester.";
    h.until("both logins", |h| {
        shows(h, one, welcome) && shows(h, two, welcome)
    })
    .await;
    (h, one, two)
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_sessions_on_one_profile_play_two_games_apart() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_on_two_games().await;
    h.type_in(one, "spam 3").await;
    h.type_in(two, "spam 4").await;
    h.until("the spam of each game", |h| {
        shows(h, one, "Line 3 of 3 of the spam.") && shows(h, two, "Line 4 of 4 of the spam.")
    })
    .await;

    // Each game heard only the line typed in its own session.
    let (first, second) = (sent(&h.servers[0]), sent(&h.servers[1]));
    assert!(
        first.contains("spam 3") && !first.contains("spam 4"),
        "{first:?}"
    );
    assert!(
        second.contains("spam 4") && !second.contains("spam 3"),
        "{second:?}"
    );
    // Each terminal shows only its own session's echo and output.
    assert!(!shows(&h, one, "spam 4") && !shows(&h, one, "of 4 of the spam"));
    assert!(!shows(&h, two, "spam 3") && !shows(&h, two, "of 3 of the spam"));
    // Neither login moved the profile both sessions play.
    let active = h
        .state
        .profile_set
        .lock()
        .await
        .as_ref()
        .expect("the set")
        .active_name()
        .to_string();
    assert_eq!(active, DEFAULT_PROFILE_NAME);

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_target_in_one_session_leaves_the_others_empty() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_on_two_games().await;
    h.type_in(one, "tar goblin").await;
    h.until("the target in the first session", |h| {
        shows(h, one, "target: goblin")
    })
    .await;

    let target = crate::ipc::session::target_get(h.app.state(), Some(one))
        .await
        .expect("the first target");
    assert_eq!(target.name.as_deref(), Some("goblin"));
    let target = crate::ipc::session::target_get(h.app.state(), Some(two))
        .await
        .expect("the second target");
    assert_eq!(target.name, None);
    assert_eq!(h.events_of(two, "session://target").len(), 0);
    assert!(!shows(&h, two, "goblin"));

    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_disconnect_in_one_session_leaves_the_other_connected_with_its_target() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_sessions_on_two_games().await;
    for (id, name) in [(one, "goblin"), (two, "orc")] {
        // The fake game sends no Room.Chars, so the list goes in place
        // the way the session takes one.
        let session = h.state.session(Some(id)).expect("the session");
        {
            let mut p = h.state.profile.lock().await;
            crate::input::target::set_room_chars(
                &mut session.connection.lock(),
                &mut p.vars,
                crate::input::target::read_room_chars(&[
                    json!({"name": format!("a {name}"), "npc": true}),
                ]),
            );
        }
        h.type_in(id, &format!("tar {name}")).await;
    }
    let names = |h: &Harness, id| {
        h.events_of(id, "session://target")
            .last()
            .map(|t| t["name"].clone())
    };
    h.until("both targets", |h| {
        names(h, one) == Some(json!("goblin")) && names(h, two) == Some(json!("orc"))
    })
    .await;

    h.disconnect_session(two).await;
    h.until("the second target to clear", |h| {
        names(h, two) == Some(json!(null))
    })
    .await;

    // Only the second session's task ended, and only its connection
    // dropped its target and room list.
    let (first, second) = (
        h.state.session(Some(one)).expect("the first session"),
        h.state.session(Some(two)).expect("the second session"),
    );
    assert!(first.slot.lock().await.is_some());
    assert!(second.slot.lock().await.is_none());
    {
        let c = first.connection.lock();
        assert_eq!(c.target.name.as_deref(), Some("goblin"));
        assert_eq!(c.room_chars.len(), 1);
    }
    {
        let c = second.connection.lock();
        assert_eq!(c.target.name, None);
        assert_eq!(c.room_chars.len(), 0);
    }
    assert_eq!(names(&h, one), Some(json!("goblin")));
    assert!(h
        .events_of(one, "session://state")
        .iter()
        .all(|state| state["kind"] != "disconnected"));

    // The first session still plays its game.
    h.type_in(one, "spam 2").await;
    h.until("the first game to answer", |h| {
        shows(h, one, "Line 2 of 2 of the spam.")
    })
    .await;
    assert!(sent(&h.servers[0]).contains("spam 2"));
    h.finish(grid).await;
}

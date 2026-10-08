//! Auto reconnect through the real session against the fake game. Each
//! redial waits on a clock the test holds, so a test reads every wait the
//! series asks for and ends it at once, while each try dials the fake
//! game for real.

use std::time::Duration;

use serde_json::{json, Value as Json};
use tauri::Manager;
use tokio::sync::{mpsc, oneshot};
use vosh_prompt::testkit::{gmcp, Build, Options};

use super::fake_mud::harness::Harness;
use crate::sessions::SessionId;

/// The fake game with `name` logging in.
fn playing_as(name: &str) -> Options {
    Options {
        name: name.into(),
        ..Options::new(Build::New)
    }
}

/// The clock the redials of `h` wait on: each wait the series asks for,
/// with the session and the sender that ends it.
struct Clock(mpsc::UnboundedReceiver<(SessionId, Duration, oneshot::Sender<()>)>);

impl Clock {
    fn hold(h: &Harness) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        *h.state.redial_clock.lock().expect("the clock") = Some(tx);
        Self(rx)
    }

    /// The next wait a series asks for, within five seconds.
    async fn next(&mut self) -> (SessionId, Duration, oneshot::Sender<()>) {
        tokio::time::timeout(Duration::from_secs(5), self.0.recv())
            .await
            .expect("a wait came")
            .expect("the clock is held")
    }

    /// No series asks for a wait for a while.
    async fn stays_quiet(&mut self) {
        let asked = tokio::time::timeout(Duration::from_millis(400), self.0.recv()).await;
        assert!(asked.is_err(), "no wait should come");
    }
}

/// Log the first session in as Orla, with `on` the alert presets on.
async fn logged_in(on: &[&str]) -> Harness {
    let h = Harness::new(playing_as("Orla")).await;
    h.state.selected_profile().await.ui.enabled_presets =
        on.iter().map(|id| (*id).to_string()).collect();
    h.connect().await;
    h.until_shown("Welcome to the fake Aabahran, Orla.").await;
    h
}

/// The `session://reconnect` payloads of `session`, by kind.
fn kinds(h: &Harness, session: SessionId) -> Vec<String> {
    h.events_of(session, "session://reconnect")
        .iter()
        .map(|e| e["kind"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// The `session://reconnect` payload of `session` of `kind`, the last.
fn last_of(h: &Harness, session: SessionId, kind: &str) -> Option<Json> {
    h.events_of(session, "session://reconnect")
        .into_iter()
        .rev()
        .find(|e| e["kind"] == kind)
}

/// The titles of the alerts the first session rang.
fn rang(h: &Harness) -> Vec<String> {
    h.events_of(h.first, "session://alert")
        .iter()
        .map(|a| a["title"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// Whether a row on the terminal of `session` shows `text`.
fn shows(h: &Harness, session: SessionId, text: &str) -> bool {
    h.screen_of(session).iter().any(|row| row.contains(text))
}

/// The character `session` plays, from the game's Char.Status.
fn character_of(h: &Harness, session: SessionId) -> Option<String> {
    h.state.session(Some(session)).ok()?.character()
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_drop_while_you_play_redials_3_6_12_24_48_and_60_seconds_apart_and_stops_at_the_prompt() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = logged_in(&["alert_connection"]).await;
    let mut clock = Clock::hold(&h);
    let game = &h.servers[0];
    game.down();
    game.cut();

    let mut waits = Vec::new();
    for number in 1..=6 {
        let (session, wait, done) = clock.next().await;
        assert_eq!(session, h.first);
        waits.push(wait.as_secs());
        if number == 6 {
            // The game is back for the sixth try.
            game.up();
        }
        let _ = done.send(());
        if number < 6 {
            let line = format!("[reconnect] Try {number} failed (the game refused the connection)");
            h.until(&line, |h| shows(h, h.first, &line)).await;
        }
    }
    assert_eq!(waits, [3, 6, 12, 24, 48, 60]);
    h.until("the try that reached the game", |h| {
        last_of(h, h.first, "reached").is_some()
    })
    .await;
    assert_eq!(
        last_of(&h, h.first, "reached"),
        Some(json!({"kind": "reached", "try": 6}))
    );
    let sent_before = game.received.lock().expect("the bytes").len();
    // The series ends at the first try that connects, and Vosh sends no
    // line of its own on the link it opened.
    clock.stays_quiet().await;
    assert_eq!(game.connects.lock().expect("the connects").len(), 2);
    let after = game.received.lock().expect("the bytes")[sent_before..].to_vec();
    assert!(!after.windows(2).any(|w| w == b"\r\n"), "{after:?}");
    // The Connection preset rang at the drop and at the game's prompt.
    h.until("Ready to log in", |h| rang(h).len() == 2).await;
    assert_eq!(rang(&h), ["Connection lost", "Ready to log in"]);
    let posted = h.state.banners.recorded();
    assert_eq!(posted.len(), 2, "Vosh is in the background");
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn eight_failed_tries_stop_and_say_so() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = logged_in(&["alert_connection"]).await;
    let mut clock = Clock::hold(&h);
    h.servers[0].down();
    h.servers[0].cut();
    for _ in 1..=8 {
        let (_, _, done) = clock.next().await;
        let _ = done.send(());
    }
    let stopped = "[reconnect] Vosh stopped after 8 tries.";
    h.until(stopped, |h| shows(h, h.first, stopped)).await;
    assert_eq!(
        last_of(&h, h.first, "stopped"),
        Some(json!({"kind": "stopped", "tries": 8}))
    );
    h.until("the alert", |h| rang(h).len() == 2).await;
    assert_eq!(rang(&h), ["Connection lost", "Vosh stopped trying"]);
    // The tries forgot the character on the live connection, and the row
    // still names the character Orla played, so two sessions that redial
    // on one world read apart. Each banner names
    // the session as its row reads.
    assert_eq!(character_of(&h, h.first), None);
    let row = h.state.session_rows().into_iter().find(|r| r.id == h.first);
    assert_eq!(row.and_then(|r| r.character).as_deref(), Some("Orla"));
    let labels: Vec<Json> = h
        .events_of(h.first, "session://alert")
        .iter()
        .map(|a| a["label"].clone())
        .collect();
    assert_eq!(labels, [json!("Orla"), json!("Orla")]);
    clock.stays_quiet().await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn your_disconnect_never_redials() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = logged_in(&["alert_connection"]).await;
    let mut clock = Clock::hold(&h);
    h.disconnect().await;
    clock.stays_quiet().await;
    assert_eq!(kinds(&h, h.first), Vec::<String>::new());
    assert_eq!(rang(&h), Vec::<String>::new());
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_quit_you_sent_never_redials() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = logged_in(&["alert_connection"]).await;
    let mut clock = Clock::hold(&h);
    h.type_line("quit").await;
    h.until("the decline", |h| last_of(h, h.first, "declined").is_some())
        .await;
    assert_eq!(
        last_of(&h, h.first, "declined"),
        Some(json!({"kind": "declined", "why": "quit"}))
    );
    clock.stays_quiet().await;
    assert_eq!(rang(&h), Vec::<String>::new(), "a quit is no lost link");
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_closing_line_since_the_last_prompt_never_redials() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = logged_in(&[]).await;
    let mut clock = Clock::hold(&h);
    // The idle auto quit, update.c:4051, then the link closes.
    h.servers[0].push(b"\n\rYou have escaped from the Forsaken Lands.\n\r");
    h.until_shown("You have escaped from the Forsaken Lands.")
        .await;
    h.servers[0].cut();
    h.until("the decline", |h| last_of(h, h.first, "declined").is_some())
        .await;
    assert_eq!(
        last_of(&h, h.first, "declined"),
        Some(json!({"kind": "declined", "why": "quit"}))
    );
    clock.stays_quiet().await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_ban_line_since_the_last_prompt_never_redials() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = logged_in(&[]).await;
    let mut clock = Clock::hold(&h);
    // A ban as you play quits you, update.c:5094, then the link closes.
    h.servers[0].push(
        b"\n\rYour account has been banned.\n\rYou have escaped from the Forsaken Lands.\n\r",
    );
    h.until_shown("You have escaped from the Forsaken Lands.")
        .await;
    h.servers[0].cut();
    h.until("the decline", |h| last_of(h, h.first, "declined").is_some())
        .await;
    assert_eq!(
        last_of(&h, h.first, "declined"),
        Some(json!({"kind": "declined", "why": "banned"}))
    );
    clock.stays_quiet().await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_drop_at_the_account_menu_starts_nothing() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = logged_in(&["alert_connection"]).await;
    let mut clock = Clock::hold(&h);
    h.type_line("quit menu").await;
    h.until_shown("You step away from the Forsaken Lands and return to your account menu.")
        .await;
    h.servers[0].cut();
    h.until("the drop", |h| {
        h.events_of(h.first, "session://state")
            .iter()
            .any(|s| s["kind"] == "disconnected")
    })
    .await;
    clock.stays_quiet().await;
    assert_eq!(kinds(&h, h.first), Vec::<String>::new());
    assert_eq!(rang(&h), Vec::<String>::new());
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_reconnect_off_a_drop_rings_and_stays_down() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = logged_in(&["alert_connection"]).await;
    let on = || crate::ipc::session::reconnect_get(h.app.state(), None);
    assert_eq!(on().await, Ok(true), "on for every profile");
    crate::ipc::session::reconnect_set(h.app.handle().clone(), h.app.state(), false, None)
        .await
        .expect("the switch");
    assert_eq!(on().await, Ok(false));
    let named = crate::ipc::session::reconnect_get(
        h.app.state(),
        Some(crate::profile::set::DEFAULT_PROFILE_NAME.into()),
    );
    assert_eq!(named.await, Ok(false), "the profile by name");
    let file = std::fs::read_to_string(
        h.profile_file(crate::profile::set::DEFAULT_PROFILE_NAME)
            .await,
    )
    .expect("the profile saved");
    assert!(
        file.lines().any(|line| line == "reconnect = false"),
        "{file}"
    );
    let mut clock = Clock::hold(&h);
    h.servers[0].cut();
    h.until("the decline", |h| last_of(h, h.first, "declined").is_some())
        .await;
    assert_eq!(
        last_of(&h, h.first, "declined"),
        Some(json!({"kind": "declined", "why": "off"}))
    );
    assert_eq!(rang(&h), ["Connection lost"]);
    clock.stays_quiet().await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disconnect_during_a_wait_ends_the_series() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = logged_in(&[]).await;
    let mut clock = Clock::hold(&h);
    h.servers[0].cut();
    let (_, wait, done) = clock.next().await;
    assert_eq!(wait, Duration::from_secs(3));
    h.disconnect().await;
    assert_eq!(kinds(&h, h.first), ["waiting", "cancelled"]);
    // The page hears your Disconnect, with no reason, after the drop.
    let states = h.events_of(h.first, "session://state");
    assert_eq!(
        states.last(),
        Some(&json!({"kind": "disconnected", "reason": null}))
    );
    let _ = done.send(());
    clock.stays_quiet().await;
    assert_eq!(h.servers[0].connects.lock().expect("the connects").len(), 1);
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disconnect_after_a_series_reached_the_game_cancels_nothing() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = logged_in(&["alert_connection"]).await;
    let mut clock = Clock::hold(&h);
    h.servers[0].cut();
    let (_, _, done) = clock.next().await;
    let _ = done.send(());
    h.until("the try that reached the game", |h| {
        last_of(h, h.first, "reached").is_some() && rang(h).len() == 2
    })
    .await;
    assert_eq!(rang(&h), ["Connection lost", "Ready to log in"]);
    // The series ends at that try, so your Disconnect ends none.
    clock.stays_quiet().await;
    h.disconnect().await;
    assert_eq!(kinds(&h, h.first), ["waiting", "dialing", "reached"]);
    // Your own Connect after it rings nothing. The login comes after the
    // game's first text, so the read that rings has run by then.
    h.connect().await;
    h.until("the login on your link", |h| {
        character_of(h, h.first).as_deref() == Some("Orla")
    })
    .await;
    assert_eq!(rang(&h), ["Connection lost", "Ready to log in"]);
    assert_eq!(kinds(&h, h.first), ["waiting", "dialing", "reached"]);
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_session_runs_a_series_of_its_own() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(playing_as("Orla")).await;
    let mut clock = Clock::hold(&h);
    let (one, two) = (h.first, h.open_session().await);
    h.connect_to(one, &h.servers[0]).await;
    h.connect_to(two, &h.servers[1]).await;
    let welcome = "Welcome to the fake Aabahran, Orla.";
    h.until("both logins", |h| {
        shows(h, one, welcome) && shows(h, two, welcome)
    })
    .await;
    for game in &h.servers {
        game.down();
        game.cut();
    }
    let mut asked = Vec::new();
    let mut dones = Vec::new();
    for _ in 0..2 {
        let (session, wait, done) = clock.next().await;
        asked.push((session, wait.as_secs()));
        dones.push(done);
    }
    asked.sort();
    assert_eq!(asked, [(one, 3), (two, 3)]);
    // The first session's game comes back, and only its series ends.
    h.servers[0].up();
    for done in dones {
        let _ = done.send(());
    }
    let (session, wait, _) = clock.next().await;
    assert_eq!((session, wait.as_secs()), (two, 6));
    h.until("the first session's redial", |h| {
        last_of(h, one, "reached").is_some()
    })
    .await;
    h.disconnect_session(two).await;
    h.finish(grid).await;
}

/// Two sessions on the one game, the first logged in as Orla, and the
/// second connected as `second`.
async fn two_links_on_one_game(second: &str) -> (Harness, SessionId, SessionId) {
    let h = Harness::new(playing_as("Orla")).await;
    let (one, two) = (h.first, h.open_session().await);
    h.connect_to(one, &h.servers[0]).await;
    h.until("the first login", |h| {
        shows(h, one, "Welcome to the fake Aabahran, Orla.")
    })
    .await;
    h.servers[0].options.lock().expect("the options").name = second.into();
    h.connect_to(two, &h.servers[0]).await;
    let welcome = format!("Welcome to the fake Aabahran, {second}.");
    h.until("the second login", |h| shows(h, two, &welcome))
        .await;
    (h, one, two)
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_login_as_your_character_takes_it_and_the_first_link_stays_down() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_links_on_one_game("Orla").await;
    let mut clock = Clock::hold(&h);
    // The game closes the first link with no line as the second takes
    // the character.
    h.servers[0].cut_link(0);
    h.until("the decline", |h| last_of(h, one, "declined").is_some())
        .await;
    assert_eq!(
        last_of(&h, one, "declined"),
        Some(json!({"kind": "declined", "why": "taken"}))
    );
    let line = "[reconnect] Another session logged in as Orla, so Vosh does not reconnect here.";
    assert!(shows(&h, one, line), "{:#?}", h.screen_of(one));
    clock.stays_quiet().await;
    assert_eq!(kinds(&h, two), Vec::<String>::new());
    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_login_as_your_character_during_the_wait_ends_the_series() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = logged_in(&[]).await;
    let mut clock = Clock::hold(&h);
    let one = h.first;
    h.servers[0].cut_link(0);
    let (_, _, done) = clock.next().await;
    let two = h.open_session().await;
    h.connect_to(two, &h.servers[0]).await;
    h.until("the decline", |h| last_of(h, one, "declined").is_some())
        .await;
    assert_eq!(kinds(&h, one), ["waiting", "declined"]);
    let line = "[reconnect] Another session logged in as Orla, so Vosh does not reconnect here.";
    assert!(shows(&h, one, line), "{:#?}", h.screen_of(one));
    let _ = done.send(());
    clock.stays_quiet().await;
    assert_eq!(h.servers[0].connects.lock().expect("the connects").len(), 2);
    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_login_as_your_character_once_the_series_reached_the_game_says_nothing_of_it() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = logged_in(&[]).await;
    let mut clock = Clock::hold(&h);
    let one = h.first;
    h.servers[0].cut();
    let (_, _, done) = clock.next().await;
    let _ = done.send(());
    h.until("the try that reached the game", |h| {
        last_of(h, one, "reached").is_some()
    })
    .await;
    // The series ends at that try.
    clock.stays_quiet().await;
    // The fake game logs Orla in again on the link the redial opened.
    h.until("the login on that link", |h| {
        character_of(h, one).as_deref() == Some("Orla")
    })
    .await;
    let two = h.open_session().await;
    h.connect_to(two, &h.servers[0]).await;
    // Char.Status comes before the welcome, so the second login has
    // reached every other session by the time the welcome shows.
    h.until("the second login", |h| {
        shows(h, two, "Welcome to the fake Aabahran, Orla.")
    })
    .await;
    assert!(
        !shows(&h, one, "[reconnect] Another session"),
        "{:#?}",
        h.screen_of(one)
    );
    assert_eq!(kinds(&h, one), ["waiting", "dialing", "reached"]);
    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_yes_to_connect_anyway_in_another_session_takes_the_character() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    // The second link has yet to name a character.
    let (h, one, two) = two_links_on_one_game("Maren").await;
    let mut clock = Clock::hold(&h);
    // check_playing, comm.c:6531, then your Y closes the first link.
    h.servers[0].push_to(1, b"\n\rThat character is already playing.\n\r");
    h.until("the question", |h| {
        shows(h, two, "That character is already playing.")
    })
    .await;
    h.type_in(two, "y").await;
    h.until("the y", |h| {
        String::from_utf8_lossy(&h.servers[0].received.lock().expect("the bytes")).contains("y\r\n")
    })
    .await;
    h.servers[0].cut_link(0);
    h.until("the decline", |h| last_of(h, one, "declined").is_some())
        .await;
    assert_eq!(
        last_of(&h, one, "declined"),
        Some(json!({"kind": "declined", "why": "taken"}))
    );
    clock.stays_quiet().await;
    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_login_elsewhere_as_the_character_you_stepped_away_from_takes_nothing() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = logged_in(&["alert_connection"]).await;
    let mut clock = Clock::hold(&h);
    let one = h.first;
    // You step away to the account menu (act_comm.c:3248). The game puts
    // you there here, since a quit you typed would count for 10 seconds.
    let left = "You step away from the Forsaken Lands and return to your account menu.";
    h.servers[0].push_to(0, format!("\n\r{left}\n\r").as_bytes());
    h.until_shown(left).await;
    // Orla is free at the account menu, and the second session takes her.
    let two = h.open_session().await;
    h.connect_to(two, &h.servers[0]).await;
    h.until("the second login", |h| {
        character_of(h, two).as_deref() == Some("Orla")
    })
    .await;
    // You play Tolliver from the account menu, and connect_char sends his
    // Char.Status (gmcp.c:200).
    let status = gmcp(
        "Char.Status",
        r#"{"name":"Tolliver","level":12,"race":"human","class":"warrior"}"#,
    );
    h.servers[0].push_to(0, &status);
    h.until("the login as Tolliver", |h| {
        character_of(h, one).as_deref() == Some("Tolliver")
    })
    .await;
    // A real drop now redials, as for any drop while you play.
    h.servers[0].cut_link(0);
    let (session, wait, _done) = clock.next().await;
    assert_eq!((session, wait.as_secs()), (one, 3));
    assert!(
        !shows(&h, one, "[reconnect] Another session"),
        "{:#?}",
        h.screen_of(one)
    );
    h.until("the lost link", |h| !rang(h).is_empty()).await;
    assert_eq!(rang(&h), ["Connection lost"]);
    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_login_as_your_character_after_a_failed_try_ends_the_series() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = logged_in(&[]).await;
    let mut clock = Clock::hold(&h);
    let one = h.first;
    h.servers[0].down();
    h.servers[0].cut();
    let (_, _, done) = clock.next().await;
    let _ = done.send(());
    let failed = "[reconnect] Try 1 failed (the game refused the connection)";
    h.until(failed, |h| shows(h, one, failed)).await;
    // The try forgot the character and the address on the session, and
    // the series still knows what it redials.
    let (_, wait, done) = clock.next().await;
    assert_eq!(wait, Duration::from_secs(6));
    h.servers[0].up();
    let two = h.open_session().await;
    h.connect_to(two, &h.servers[0]).await;
    h.until("the decline", |h| last_of(h, one, "declined").is_some())
        .await;
    assert_eq!(
        kinds(&h, one),
        ["waiting", "dialing", "failed", "waiting", "declined"]
    );
    let line = "[reconnect] Another session logged in as Orla, so Vosh does not reconnect here.";
    assert!(shows(&h, one, line), "{:#?}", h.screen_of(one));
    let _ = done.send(());
    clock.stays_quiet().await;
    assert_eq!(h.servers[0].connects.lock().expect("the connects").len(), 2);
    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_yes_to_connect_anyway_during_the_wait_ends_the_series_before_its_try() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let (h, one, two) = two_links_on_one_game("Maren").await;
    let mut clock = Clock::hold(&h);
    // The link of the first session drops, and the game has yet to see
    // it go, so it asks the second session (comm.c:6531).
    h.servers[0].cut_link(0);
    let (_, _, done) = clock.next().await;
    h.servers[0].push_to(1, b"\n\rThat character is already playing.\n\r");
    h.until("the question", |h| {
        shows(h, two, "That character is already playing.")
    })
    .await;
    h.type_in(two, "y").await;
    h.until("the y", |h| {
        String::from_utf8_lossy(&h.servers[0].received.lock().expect("the bytes")).contains("y\r\n")
    })
    .await;
    let _ = done.send(());
    h.until("the decline", |h| last_of(h, one, "declined").is_some())
        .await;
    assert_eq!(kinds(&h, one), ["waiting", "declined"]);
    clock.stays_quiet().await;
    assert_eq!(h.servers[0].connects.lock().expect("the connects").len(), 2);
    h.disconnect_session(two).await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disconnect_while_a_try_connects_leaves_no_link_up() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = logged_in(&[]).await;
    let mut clock = Clock::hold(&h);
    let (gate, held) = std::sync::mpsc::channel();
    *h.state.redial_gate.lock().expect("the gate") = Some(gate);
    h.servers[0].cut();
    let (_, _, done) = clock.next().await;
    let _ = done.send(());
    // The try reached the game and waits before it takes the slot.
    let go_on = tokio::task::spawn_blocking(move || held.recv_timeout(Duration::from_secs(5)))
        .await
        .expect("the wait")
        .expect("the try connected");
    // Your Disconnect comes while the try holds, and the try then goes on.
    let session = h.state.session(Some(h.first)).expect("the session");
    let (app, state) = (h.app.handle().clone(), h.state.clone());
    let disconnecting = {
        let session = session.clone();
        tokio::spawn(async move { crate::session::disconnect(&app, &state, &session).await })
    };
    tokio::time::sleep(Duration::from_millis(200)).await;
    go_on.send(()).expect("the try waits");
    disconnecting.await.expect("the disconnect");
    assert!(
        session.slot.lock().await.is_none(),
        "the link the try opened stays up"
    );
    // The end of the series cut the try short or found it reached the
    // game, and the page hears one of the two.
    let heard = kinds(&h, h.first);
    assert!(
        heard == ["waiting", "dialing", "cancelled"] || heard == ["waiting", "dialing", "reached"],
        "{heard:?}"
    );
    clock.stays_quiet().await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconnect_now_dials_at_once_and_only_while_a_series_waits() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = logged_in(&[]).await;
    let mut clock = Clock::hold(&h);
    let now = || crate::ipc::session::session_reconnect_now(h.app.state(), Some(h.first));
    assert_eq!(
        now().await,
        Err("Vosh is not waiting to reconnect this session.".into())
    );
    h.servers[0].cut();
    // The first wait never ends on the clock.
    let (_, wait, _held) = clock.next().await;
    assert_eq!(wait, Duration::from_secs(3));
    assert_eq!(now().await, Ok(()));
    h.until("the try that reached the game", |h| {
        last_of(h, h.first, "reached").is_some()
    })
    .await;
    assert_eq!(kinds(&h, h.first), ["waiting", "dialing", "reached"]);
    clock.stays_quiet().await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_connect_during_a_wait_ends_the_series_and_dials_where_you_asked() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = logged_in(&[]).await;
    let mut clock = Clock::hold(&h);
    h.servers[0].cut();
    let (_, _, done) = clock.next().await;
    h.connect_through_vosh(h.first, &h.servers[1]).await;
    assert_eq!(kinds(&h, h.first), ["waiting", "cancelled"]);
    let _ = done.send(());
    clock.stays_quiet().await;
    assert_eq!(h.servers[0].connects.lock().expect("the connects").len(), 1);
    assert_eq!(h.servers[1].connects.lock().expect("the connects").len(), 1);
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_a_session_during_a_wait_ends_its_series() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(playing_as("Orla")).await;
    let mut clock = Clock::hold(&h);
    let two = h.open_session().await;
    h.connect_through_vosh(two, &h.servers[1]).await;
    h.until("the login", |h| {
        shows(h, two, "Welcome to the fake Aabahran, Orla.")
    })
    .await;
    h.servers[1].cut();
    let (session, _, done) = clock.next().await;
    assert_eq!(session, two);
    h.close_session(two).await;
    assert_eq!(kinds(&h, two), ["waiting", "cancelled"]);
    let _ = done.send(());
    clock.stays_quiet().await;
    assert_eq!(h.servers[1].connects.lock().expect("the connects").len(), 1);
    h.finish(grid).await;
}

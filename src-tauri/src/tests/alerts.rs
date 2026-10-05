//! Alerts through the real session against the fake game: a trigger's
//! alert table, the 10 second cap, and the focus rule with two sessions
//! (Alerts Q2 and Q6 with Sessions Q10). A test build keeps each banner
//! in a list instead of posting it, so no test reaches the system's
//! notification center.

use serde_json::Value as Json;
use vosh_automation::trigger::{Trigger, TriggerAction};
use vosh_prompt::testkit::{Build, Options};

use super::fake_mud::harness::Harness;
use crate::alert::AlertParts;
use crate::sessions::SessionId;

/// The fake game with Orla logging in.
fn orla() -> Options {
    Options {
        name: "Orla".into(),
        ..Options::new(Build::New)
    }
}

/// A line no command asked for, as `write_to_buffer` starts one.
fn line(text: &str) -> Vec<u8> {
    format!("\n\r{text}\n\r").into_bytes()
}

/// The trigger on `$n walks in.`, from `act_move.c:1053`, with `parts`.
fn visitor(parts: AlertParts) -> Trigger {
    Trigger {
        alert: Some(parts),
        ..Trigger::new(
            "visitor",
            r"^\w+ walks in\.$",
            TriggerAction::Route {
                pane: "chat".into(),
            },
        )
    }
}

/// The alerts `session` rang, oldest first.
fn alerts(h: &Harness, session: SessionId) -> Vec<Json> {
    h.events_of(session, "session://alert")
}

/// Give the profile `trigger`.
async fn set_up(h: &Harness, trigger: Trigger) {
    let mut p = h.state.selected_profile().await;
    p.triggers.set(trigger).expect("the trigger");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn away_from_vosh_a_trigger_rings_with_a_banner_under_the_cap() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(orla()).await;
    let banner = AlertParts {
        banner: true,
        ..AlertParts::default()
    };
    set_up(&h, visitor(banner)).await;
    h.connect().await;
    h.until_shown("Welcome to the fake Aabahran, Orla.").await;

    h.servers[0].push(&line("Maren walks in."));
    // A second match inside 10 seconds stays quiet.
    h.servers[0].push(&line("Tolliver walks in."));
    h.until_shown("Tolliver walks in.").await;
    h.until("the visitor", |h| !alerts(h, h.first).is_empty())
        .await;
    let rang = alerts(&h, h.first);
    assert_eq!(rang.len(), 1, "{rang:?}");
    let first = &rang[0];
    assert_eq!(first["title"], "visitor");
    assert_eq!(first["label"], "Orla", "the banner names the session");
    assert_eq!(first["banner"], true);
    assert_eq!(first["notice"], false);
    assert_eq!(first["words"], Json::Null, "Title only");
    assert_eq!(first["source"], "trigger:visitor");
    let posted: Vec<String> = h
        .state
        .banners
        .recorded()
        .into_iter()
        .map(|p| p.banner.title)
        .collect();
    assert_eq!(posted, ["visitor"]);
    // The line the trigger matched still showed, and routed.
    h.until_shown("Maren walks in.").await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_counts_as_in_front_only_while_vosh_is_and_its_row_is_selected() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(orla()).await;
    let parts = AlertParts {
        banner: true,
        sound: Some("chime".into()),
        ..AlertParts::default()
    };
    set_up(&h, visitor(parts)).await;
    let (one, two) = (h.first, h.open_session().await);
    h.connect_to(one, &h.servers[0]).await;
    h.connect_to(two, &h.servers[1]).await;
    h.until("both logins", |h| {
        let welcome = "Welcome to the fake Aabahran, Orla.";
        h.screen_of(one).iter().any(|r| r.contains(welcome))
            && h.screen_of(two).iter().any(|r| r.contains(welcome))
    })
    .await;
    // Vosh is in front with the first session selected.
    h.state.focus.set("main", true);

    // The match in the session you look at stays quiet, and the one in
    // the session behind plays its tone and asks the page for a notice,
    // with no banner over the window you read.
    h.servers[0].push(&line("Maren walks in."));
    h.servers[1].push(&line("Maren walks in."));
    h.until("the visitor behind", |h| !alerts(h, two).is_empty())
        .await;
    let behind = &alerts(&h, two)[0];
    assert_eq!(behind["notice"], true);
    assert_eq!(behind["banner"], false);
    assert_eq!(behind["sound"], "chime");
    h.until_shown("Maren walks in.").await;
    assert_eq!(alerts(&h, one), Vec::<Json>::new());

    // With Vosh in the background, the match posts its banner and names
    // the session.
    h.state.focus.set("main", false);
    h.servers[0].push(&line("Tolliver walks in."));
    h.until("the visitor away", |h| !alerts(h, one).is_empty())
        .await;
    let posted: Vec<(SessionId, String)> = h
        .state
        .banners
        .recorded()
        .into_iter()
        .map(|p| (p.banner.session, p.banner.title))
        .collect();
    assert_eq!(posted, [(one, "visitor".to_string())]);
    h.disconnect_session(two).await;
    h.finish(grid).await;
}

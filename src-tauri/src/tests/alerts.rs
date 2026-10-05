//! Alerts through the real session against the fake game: a trigger's
//! alert table, the presets the packets and lines ring, the 10 second
//! cap, and the focus rule with two sessions (Alerts Q2, Q5 and Q6 with
//! Sessions Q10). A test build keeps each banner in a list instead of
//! posting it, so no test reaches the system's notification center.

use serde_json::Value as Json;
use vosh_automation::trigger::{Trigger, TriggerAction};
use vosh_prompt::testkit::{gmcp, Build, Options};

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

/// The tell of fixtures/gmcp/aabahran/chat/tell.gmcp from `speaker`, as
/// the game writes it.
fn tell_from(speaker: &str) -> Vec<u8> {
    let text = include_str!("../../../fixtures/gmcp/aabahran/chat/tell.gmcp");
    let (package, json) = text.trim().split_once(' ').expect("a packet");
    gmcp(package, &json.replace("Tolliver", speaker))
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

/// The titles of the alerts `session` rang, oldest first.
fn rang(h: &Harness, session: SessionId) -> Vec<String> {
    h.events_of(session, "session://alert")
        .iter()
        .map(|alert| alert["title"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// The alerts `session` rang, oldest first.
fn alerts(h: &Harness, session: SessionId) -> Vec<Json> {
    h.events_of(session, "session://alert")
}

/// Turn on the presets `on` and give the profile `trigger`.
async fn set_up(h: &Harness, on: &[&str], trigger: Trigger) {
    let mut p = h.state.selected_profile().await;
    p.ui.enabled_presets = on.iter().map(|id| (*id).to_string()).collect();
    p.triggers.set(trigger).expect("the trigger");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn away_from_vosh_a_trigger_and_the_presets_ring_with_banners_under_the_cap() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(orla()).await;
    let banner = AlertParts {
        banner: true,
        ..AlertParts::default()
    };
    set_up(&h, &["alert_tells"], visitor(banner)).await;
    h.connect().await;
    h.until_shown("Welcome to the fake Aabahran, Orla.").await;
    // The fake game greets you by name, which Aabahran never does, so
    // Your name turns on once you are in.
    h.state
        .selected_profile()
        .await
        .ui
        .enabled_presets
        .push("alert_name".into());

    h.servers[0].push(&tell_from("Tolliver"));
    h.servers[0].push(&line("Maren walks in."));
    // `$n looks at $N.`, from act_info.c:1054.
    h.servers[0].push(&line("Maren looks at Orla."));
    // A second tell from Tolliver comes inside 10 seconds, and Maren's
    // does not share his cap.
    h.servers[0].push(&tell_from("Tolliver"));
    h.servers[0].push(&tell_from("Maren"));
    h.until("Maren's tell", |h| {
        rang(h, h.first).contains(&"Tell from Maren".to_string())
    })
    .await;
    assert_eq!(
        rang(&h, h.first),
        [
            "Tell from Tolliver",
            "visitor",
            "Someone named you",
            "Tell from Maren"
        ]
    );
    let first = &alerts(&h, h.first)[0];
    assert_eq!(first["label"], "Orla", "the banner names the session");
    assert_eq!(first["banner"], true);
    assert_eq!(first["notice"], false);
    assert_eq!(first["words"], Json::Null, "Title only");
    assert_eq!(first["source"], "preset:alert_tells");
    let posted: Vec<String> = h
        .state
        .banners
        .recorded()
        .into_iter()
        .map(|p| format!("{} ({})", p.banner.title, p.banner.body()))
        .collect();
    assert_eq!(
        posted,
        [
            "Tell from Tolliver (Vosh)",
            "visitor (Vosh)",
            "Someone named you (Vosh)",
            "Tell from Maren (Vosh)"
        ]
    );
    // The line the trigger matched still showed, and routed.
    h.until_shown("Maren walks in.").await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_counts_as_in_front_only_while_vosh_is_and_its_row_is_selected() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(orla()).await;
    // The trigger rings even while you look at its session.
    let always = AlertParts {
        banner: true,
        sound: Some("chime".into()),
        background: false,
        ..AlertParts::default()
    };
    set_up(&h, &["alert_tells"], visitor(always)).await;
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

    // The tell to the session you look at stays quiet, and the one to the
    // session behind plays its tone and asks the page for a notice, with
    // no banner over the window you read.
    h.servers[0].push(&tell_from("Tolliver"));
    h.servers[1].push(&tell_from("Tolliver"));
    h.until("the tell behind", |h| !alerts(h, two).is_empty())
        .await;
    let behind = &alerts(&h, two)[0];
    assert_eq!(behind["notice"], true);
    assert_eq!(behind["banner"], false);
    // A trigger with Only while you are not looking at its session off
    // rings in the session you look at, after the tell that stayed quiet.
    h.servers[0].push(&line("Maren walks in."));
    h.until("the visitor", |h| !alerts(h, one).is_empty()).await;
    assert_eq!(rang(&h, one), ["visitor"]);
    let looking = &alerts(&h, one)[0];
    assert_eq!(looking["banner"], true);
    assert_eq!(looking["sound"], "chime");
    assert_eq!(looking["notice"], false);

    // With Vosh in the background, a tell to either session posts its
    // banner and names the session.
    h.state.focus.set("main", false);
    h.servers[0].push(&tell_from("Maren"));
    h.until("the tell away", |h| rang(h, one).len() == 2).await;
    let posted: Vec<(SessionId, String)> = h
        .state
        .banners
        .recorded()
        .into_iter()
        .map(|p| (p.banner.session, p.banner.title))
        .collect();
    assert_eq!(
        posted,
        [
            (one, "visitor".to_string()),
            (one, "Tell from Maren".to_string())
        ]
    );
    h.disconnect_session(two).await;
    h.finish(grid).await;
}

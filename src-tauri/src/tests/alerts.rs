//! Alerts through the real session against the fake game: a trigger's
//! alert table, the presets the packets and lines ring, the 10 second
//! cap, and the focus rule with two sessions (Alerts Q2, Q5 and Q6 with
//! Sessions Q10). A test build keeps each banner in a list instead of
//! posting it, so no test reaches the system's notification center.

use serde_json::Value as Json;
use tauri::Manager;
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

/// Where each `session://mark` on the row of `session` came from.
fn marks(h: &Harness, session: SessionId) -> Vec<String> {
    h.events_of(session, "session://mark")
        .iter()
        .map(|mark| mark["source"].as_str().expect("a source").to_string())
        .collect()
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_every_preset_off_a_tell_behind_still_marks_its_row_and_rings_nothing() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(orla()).await;
    // A trigger on `$n looks at $N.`, from act_info.c:1054, whose alert
    // has nothing on.
    let looks = Trigger {
        alert: Some(AlertParts::default()),
        ..Trigger::new(
            "looks",
            r"^\w+ looks at \w+\.$",
            TriggerAction::Route {
                pane: "chat".into(),
            },
        )
    };
    set_up(&h, &[], looks).await;
    let (one, two) = (h.first, h.open_session().await);
    h.connect_to(one, &h.servers[0]).await;
    h.connect_to(two, &h.servers[1]).await;
    h.until("both logins", |h| {
        let welcome = "Welcome to the fake Aabahran, Orla.";
        h.screen_of(one).iter().any(|r| r.contains(welcome))
            && h.screen_of(two).iter().any(|r| r.contains(welcome))
    })
    .await;
    h.state.focus.set("main", true);
    // Every preset ships off. The welcome named Orla, and a tell to the
    // session behind marks its row again (Sessions Q9), each mark with
    // the alert it stands for. The one to the session you look at marks
    // nothing.
    h.servers[0].push(&tell_from("Tolliver"));
    // A line after the tell shows once the session read the tell.
    h.servers[0].push(&line("Maren walks in."));
    h.servers[1].push(&tell_from("Tolliver"));
    let named = "preset:alert_name".to_string();
    let tell = "preset:alert_tells".to_string();
    h.until("the mark behind", |h| marks(h, two).len() == 2)
        .await;
    assert_eq!(marks(&h, two), [named.clone(), tell.clone()]);
    h.until("the line after the tell", |h| {
        h.screen_of(one)
            .iter()
            .any(|r| r.contains("Maren walks in."))
    })
    .await;
    assert_eq!(marks(&h, one), Vec::<String>::new());
    assert_eq!(alerts(&h, one), Vec::<Json>::new());
    assert_eq!(alerts(&h, two), Vec::<Json>::new());
    assert_eq!(h.state.banners.recorded().len(), 0);
    // With Vosh in the background too, nothing rings and the second tell
    // adds a mark of its own.
    h.state.focus.set("main", false);
    h.servers[1].push(&tell_from("Maren"));
    h.until("the second mark", |h| marks(h, two).len() == 3)
        .await;
    assert_eq!(marks(&h, two), [named.clone(), tell.clone(), tell]);
    // A line that names you and that the trigger matches marks the row
    // once for each alert.
    h.servers[1].push(&line("Maren looks at Orla."));
    h.until("the look", |h| marks(h, two).len() == 5).await;
    assert_eq!(marks(&h, two)[3..], ["trigger:looks".to_string(), named]);
    assert_eq!(h.state.banners.recorded().len(), 0);
    h.disconnect_session(two).await;
    h.finish(grid).await;
}

/// Turn the plugin `name` on or off in the first session, and deliver
/// what it asks for.
async fn plugin(h: &Harness, name: &str, on: bool) {
    let session = h.state.selected_session();
    let plugins = crate::disk::paths::plugins_dir(h.dir.path());
    let apply = {
        let mut p = session.lock_profile().await;
        let mut c = session.connection.lock();
        let apply = if on {
            crate::app::plugins::plugin_on(&mut p, &mut c, &plugins, name)
        } else {
            crate::app::plugins::plugin_off(&mut p, &mut c, name)
        };
        apply.ran_under(p.open())
    };
    crate::app::plugins::follow_profile(h.app.handle(), &session, apply).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_plugin_alert_carries_its_owner_and_ends_as_the_plugin_turns_off() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(orla()).await;
    h.state
        .app_data
        .set(h.dir.path().to_path_buf())
        .expect("the app data folder");
    let folder = crate::disk::paths::plugins_dir(h.dir.path()).join("watch");
    std::fs::create_dir_all(&folder).expect("the plugin folder");
    std::fs::write(folder.join("manifest.toml"), "[plugin]\nname = \"watch\"\n")
        .expect("the manifest");
    std::fs::write(
        folder.join("main.lua"),
        "mud.trigger('visitor', 'walks in', function() \
             mud.alert('Someone walked in', {sound = 'knock', attention = 'once'}) end)",
    )
    .expect("the entry script");
    h.connect().await;
    h.until_shown("Welcome to the fake Aabahran, Orla.").await;
    plugin(&h, "watch", true).await;

    // A second match inside 10 seconds rings nothing more.
    h.servers[0].push(&line("Maren walks in."));
    h.servers[0].push(&line("Maren walks in."));
    h.until("both lines", |h| {
        h.screen()
            .iter()
            .filter(|r| r.contains("Maren walks in."))
            .count()
            == 2
    })
    .await;
    h.until("the alert", |h| !alerts(h, h.first).is_empty())
        .await;
    let rung = alerts(&h, h.first);
    assert_eq!(rung.len(), 1, "{rung:?}");
    assert_eq!(rung[0]["owner"], "plugin:watch");
    assert_eq!(rung[0]["source"], "lua:plugin:watch");
    assert_eq!(rung[0]["sound"], "knock");
    let posted = h.state.banners.recorded();
    assert_eq!(posted.len(), 1);
    assert_eq!(posted[0].banner.owner.as_deref(), Some("plugin:watch"));

    // Turning the plugin off ends its alerts. Its banners go, the page
    // hears so, and its trigger is gone.
    plugin(&h, "watch", false).await;
    h.until("the end of its alerts", |h| {
        h.events_of(h.first, "session://alerts-ended")
            .iter()
            .any(|e| e["owner"] == "plugin:watch")
    })
    .await;
    assert_eq!(h.state.banners.recorded(), Vec::new());
    h.servers[0].push(&line("Maren walks in."));
    h.until("the third line", |h| {
        h.screen()
            .iter()
            .filter(|r| r.contains("Maren walks in."))
            .count()
            == 3
    })
    .await;
    assert_eq!(alerts(&h, h.first).len(), 1);
    h.finish(grid).await;
}

/// The phases `session` told every window, oldest first.
fn phases(h: &Harness, session: SessionId) -> Vec<String> {
    h.events_of(session, "vosh://daylight-changed")
        .iter()
        .map(|e| e["phase"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_session_keeps_its_game_s_day_or_night_through_a_drop() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(orla()).await;
    let (one, two) = (h.first, h.open_session().await);
    h.connect_to(one, &h.servers[0]).await;
    h.connect_to(two, &h.servers[1]).await;
    // Each login sends World.Time at hour 14, in full light.
    h.until("both mornings", |h| {
        phases(h, one) == ["day"] && phases(h, two) == ["day"]
    })
    .await;
    // weather_update at hour 19, update.c:2326, in the first game alone.
    h.servers[0].push(&gmcp(
        "World.Time",
        r#"{"hour":19,"day":3,"month":5,"year":1203,"sunlight":"dark","sky":"cloudy"}"#,
    ));
    h.until("the night", |h| phases(h, one) == ["day", "night"])
        .await;
    assert_eq!(phases(&h, two), ["day"]);
    // The window holds what it showed through a drop.
    h.disconnect_session(one).await;
    let phase = |id| crate::ipc::tick::daylight_get(h.app.state(), Some(id));
    assert_eq!(phase(one).await, Ok(Some(crate::tick::Daylight::Night)));
    assert_eq!(phase(two).await, Ok(Some(crate::tick::Daylight::Day)));
    h.disconnect_session(two).await;
    h.finish(grid).await;
}

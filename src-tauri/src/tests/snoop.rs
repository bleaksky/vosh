//! Snoop against the fake game. Aabahran sends snoop packets only to a
//! client that names Snoop in Core.Supports, so Vosh asks for it on
//! every connect. Each session keeps a tab for each player it snoops,
//! whose text never reaches the line pipeline, and the page hears the
//! tabs and the text (Snoop SN3 and SN5). Each line goes in the session
//! log marked with the player name (SN4).

use serde_json::{json, Value as Json};
use tauri::Manager;
use vosh_automation::trigger::{HighlightStyle, Trigger, TriggerAction};
use vosh_prompt::testkit::mud::telnet::{GMCP, IAC, SB};
use vosh_prompt::testkit::{gmcp, Build, Options};

use super::fake_mud::harness::Harness;
use crate::app::events::{SNOOP, SNOOP_OUTPUT};
use crate::session::snoop::MAX_LINES;

const START: &str = include_str!("../../../fixtures/gmcp/aabahran/snoop-start.gmcp");
const OUTPUT: &str = include_str!("../../../fixtures/gmcp/aabahran/snoop-output.gmcp");
const STOP: &str = include_str!("../../../fixtures/gmcp/aabahran/snoop-stop.gmcp");

/// Whether `bytes` hold a GMCP Core.Supports.Set whose list has `entry`.
fn supports_set_names(bytes: &[u8], entry: &str) -> bool {
    let start = [IAC, SB, GMCP];
    bytes
        .windows(start.len())
        .enumerate()
        .filter(|(_, w)| *w == start)
        .any(|(i, _)| {
            let body = String::from_utf8_lossy(&bytes[i + start.len()..]);
            let body = body.split('\u{fffd}').next().unwrap_or_default();
            body.starts_with("Core.Supports.Set ") && body.contains(&format!("\"{entry}\""))
        })
}

// The guard keeps other tests off the shared native grid.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn vosh_asks_the_game_for_snoop_as_it_connects() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.connect().await;
    h.until("Core.Supports.Set with Snoop 1", |h| {
        let received = h.servers[0].received.lock().expect("the bytes");
        supports_set_names(&received, "Snoop 1")
    })
    .await;
    h.finish(grid).await;
}

/// The packet a fixture holds, as the game writes it.
fn packet(text: &str) -> Vec<u8> {
    let (package, json) = text.trim().split_once(' ').expect("a packet");
    gmcp(package, json)
}

/// The data of the packet a fixture holds.
fn data(text: &str) -> Json {
    let (_, json) = text.trim().split_once(' ').expect("a packet");
    serde_json::from_str(json).expect("the JSON")
}

/// Snoop.Start or Snoop.Stop for `name`, as `gmcp_send_snoop_state`
/// writes it.
fn state(on: bool, name: &str) -> Vec<u8> {
    let package = if on { "Snoop.Start" } else { "Snoop.Stop" };
    gmcp(package, &json!({ "name": name }).to_string())
}

/// A connected Orla with Tolliver's snoop started, its tab listed.
async fn snooping() -> Harness {
    let h = Harness::new(Options {
        name: "Orla".into(),
        ..Options::new(Build::New)
    })
    .await;
    h.connect().await;
    h.until_shown("Welcome to the fake Aabahran, Orla.").await;
    h.servers[0].push(&packet(START));
    h.until("the tab", |h| !lists(h).is_empty()).await;
    h
}

/// Every tab list `session://snoop` carried, oldest first.
fn lists(h: &Harness) -> Vec<Json> {
    h.events_of(h.first, SNOOP)
        .into_iter()
        .map(|list| list["tabs"].clone())
        .collect()
}

/// The newest tab list, as name and whether it is live.
fn tabs(h: &Harness) -> Vec<(String, bool)> {
    let Some(Json::Array(tabs)) = lists(h).pop() else {
        return Vec::new();
    };
    tabs.iter()
        .map(|tab| {
            let name = tab["name"].as_str().expect("a name").to_string();
            (name, tab["live"].as_bool().expect("live"))
        })
        .collect()
}

fn tab(name: &str, live: bool) -> (String, bool) {
    (name.to_string(), live)
}

/// What `snoop_get` returns for the first session.
async fn got(h: &Harness) -> Json {
    let tabs = crate::ipc::snoop::snoop_get(h.app.state(), Some(h.first))
        .await
        .expect("the tabs");
    serde_json::to_value(tabs).expect("json")
}

/// Everything the client sent the game, as text.
fn sent(h: &Harness) -> String {
    let received = h.servers[0].received.lock().expect("the bytes");
    String::from_utf8_lossy(&received).into_owned()
}

/// Press Stop on the snoop of `name`, or Stop every snoop with none.
async fn stop(h: &Harness, name: Option<&str>) {
    crate::ipc::snoop::snoop_stop(
        h.app.handle().clone(),
        h.app.state(),
        Some(h.first),
        name.map(str::to_string),
    )
    .await
    .expect("the stop");
}

/// A line no command asked for, so the test knows the session read on.
async fn read_on(h: &Harness, text: &str) {
    h.servers[0].push(format!("\n\r{text}\n\r").as_bytes());
    h.until_shown(text).await;
}

// The guard keeps other tests off the shared native grid.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_snoop_keeps_its_text_in_its_tab_and_out_of_the_line_pipeline() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = snooping().await;
    assert_eq!(tabs(&h), [tab("Tolliver", true)]);
    {
        let mut p = h.state.selected_profile().await;
        let send = TriggerAction::Send {
            template: "say tent".into(),
        };
        p.triggers
            .set(Trigger::new("tent", "Ramshackle", send))
            .expect("the send trigger");
        let paint = TriggerAction::Highlight {
            style: HighlightStyle::default(),
        };
        p.triggers
            .set(Trigger::new("paint", "Tent City", paint))
            .expect("the highlight");
    }
    h.type_line(
        "#lua mud.on_gmcp('Snoop.Output', function(d) mud.echo('Lua heard ' .. d.name) end)",
    )
    .await;

    h.servers[0].push(&packet(OUTPUT));
    h.until_shown("Lua heard Tolliver").await;
    read_on(&h, "Maren walks in.").await;
    let text = data(OUTPUT)["text"].as_str().expect("the text").to_string();
    assert_eq!(
        h.events_of(h.first, SNOOP_OUTPUT),
        [json!({"name": "Tolliver", "text": text})]
    );
    let all = got(&h).await;
    assert_eq!(all[0]["name"], "Tolliver");
    assert_eq!(all[0]["live"], true);
    assert_eq!(all[0]["text"], text.as_str());
    assert!(all[0]["last_output_at"].is_i64(), "{all}");
    // No trigger heard it, so nothing went to the game and nothing painted
    // the main terminal.
    assert!(!sent(&h).contains("say tent"), "{}", sent(&h));
    let outputs = h.events_of(h.first, "session://output");
    let shown = serde_json::to_string(&outputs).expect("json");
    assert!(!shown.contains("Ramshackle"), "{shown}");

    // A snoop that ends and starts again reuses its tab and its text.
    h.servers[0].push(&state(false, "Tolliver"));
    h.until("the end", |h| tabs(h) == [tab("Tolliver", false)])
        .await;
    h.servers[0].push(&packet(START));
    h.until("the start", |h| tabs(h) == [tab("Tolliver", true)])
        .await;
    assert_eq!(got(&h).await[0]["text"], text.as_str());
    h.finish(grid).await;
}

// The guard keeps other tests off the shared native grid.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_asks_the_game_and_an_end_you_did_not_ask_for_keeps_the_tab() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = snooping().await;
    h.servers[0].push(&state(true, "Maren"));
    h.until("two tabs", |h| tabs(h).len() == 2).await;

    stop(&h, Some("Tolliver")).await;
    h.until("snoop stop Tolliver", |h| {
        sent(h).contains("snoop stop Tolliver\r\n")
    })
    .await;
    h.until_shown("snoop stop Tolliver").await;
    // The tab waits for the game.
    assert_eq!(tabs(&h), [tab("Tolliver", true), tab("Maren", true)]);
    h.servers[0].push(&packet(STOP));
    h.until("the confirm", |h| tabs(h) == [tab("Maren", true)])
        .await;

    // Maren left the game, so her tab stays as ended until you close it.
    h.servers[0].push(&state(false, "Maren"));
    h.until("the end", |h| tabs(h) == [tab("Maren", false)])
        .await;
    assert!(got(&h).await[0]["ended_at"].is_i64());
    crate::ipc::snoop::snoop_close(
        h.app.handle().clone(),
        h.app.state(),
        Some(h.first),
        Some("Maren".into()),
    )
    .await
    .expect("the close");
    assert_eq!(tabs(&h), []);
    assert_eq!(got(&h).await, json!([]));

    // Stop every snoop sends a bare `snoop stop`, and the game ends each.
    h.servers[0].push(&state(true, "Tolliver"));
    h.servers[0].push(&state(true, "Orla"));
    h.until("two tabs", |h| tabs(h).len() == 2).await;
    stop(&h, None).await;
    h.until("snoop stop", |h| sent(h).contains("snoop stop\r\n"))
        .await;
    let mut both = state(false, "Tolliver");
    both.extend(state(false, "Orla"));
    h.servers[0].push(&both);
    h.until("no tabs", |h| tabs(h).is_empty()).await;
    h.finish(grid).await;
}

// The guard keeps other tests off the shared native grid.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cut_link_ends_every_snoop_and_the_ring_keeps_five_thousand_lines() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = snooping().await;
    h.servers[0].push(&state(true, "Maren"));
    let text: String = (0..MAX_LINES + 2).map(|i| i.to_string() + "\n\r").collect();
    let burst = json!({ "name": "Maren", "text": text }).to_string();
    h.servers[0].push(&gmcp("Snoop.Output", &burst));
    h.until("the text", |h| {
        !h.events_of(h.first, SNOOP_OUTPUT).is_empty()
    })
    .await;
    let kept = got(&h).await[1]["text"]
        .as_str()
        .expect("the text")
        .to_string();
    assert_eq!(kept.matches('\n').count(), MAX_LINES);
    assert!(kept.starts_with("\r2\n\r"), "{:?}", &kept[..8]);

    h.servers[0].cut();
    h.until("the ended tabs", |h| {
        tabs(h) == [tab("Tolliver", false), tab("Maren", false)]
    })
    .await;
    h.finish(grid).await;
}

/// The rows of the log a search for `pattern` finds, as text and raw.
/// None while the session writes to the log.
fn log_rows(h: &Harness, pattern: &str) -> Option<Vec<(String, String)>> {
    let guard = h.state.logs.try_lock().ok()?;
    let store = guard.as_ref().expect("the log");
    let hits = store
        .search(pattern, &vosh_log::SearchOptions::default())
        .expect("the search");
    let rows = hits.into_iter().map(|hit| {
        let raw = String::from_utf8_lossy(&hit.raw.unwrap_or_default()).into_owned();
        (hit.text, raw)
    });
    Some(rows.collect())
}

/// Wait until a search for `pattern` finds `count` rows, and return them.
async fn until_logged(h: &Harness, pattern: &str, count: usize) -> Vec<(String, String)> {
    h.until(&format!("{count} rows for {pattern}"), |h| {
        log_rows(h, pattern).is_some_and(|rows| rows.len() == count)
    })
    .await;
    log_rows(h, pattern).expect("the rows")
}

// The guard keeps other tests off the shared native grid.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_snoop_line_goes_in_the_log_marked_with_the_name() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = snooping().await;
    h.servers[0].push(&packet(OUTPUT));
    read_on(&h, "Maren walks in.").await;
    let marked = r"^Tolliver\|";
    let rows = until_logged(&h, marked, 11).await;
    assert_eq!(rows[0].0, "Tolliver| A Ramshackle Tent City");
    assert_eq!(
        rows[0].1,
        "Tolliver| \u{1b}[38;5;82m\u{1b}[0;33mA Ramshackle Tent City\u{1b}[0;0m\u{1b}[0;0m"
    );
    assert_eq!(rows[6].0, "Tolliver| ");
    assert_eq!(
        rows[9].0,
        "Tolliver| [KNIGHT] A ward of Praetorian guards stand in support."
    );
    // The prompt waits in the tab for its newline, the ring keeps no
    // mark, and your own lines carry none.
    assert!(!rows.iter().any(|(text, _)| text.contains("<612hp")));
    assert!(!got(&h).await[0]["text"]
        .as_str()
        .expect("the text")
        .contains("Tolliver|"));
    until_logged(&h, "^Maren walks in", 1).await;

    // The end of the link puts the partial in the log.
    h.servers[0].cut();
    h.until("the ended tab", |h| tabs(h) == [tab("Tolliver", false)])
        .await;
    let rows = until_logged(&h, marked, 12).await;
    assert_eq!(rows[11].0, "Tolliver| <612hp 480m 702mv> ");
    h.finish(grid).await;
}

//! The kinds the session logs, through the real session against the fake
//! game. fixtures/scenes/pairing.json plays two lines of a look ahead of a
//! say and a tell, with each packet ahead of the text as the game sent it
//! before d50e4a24 and right after its own line as it sends it since.
//! Either way the say and the tell take their channel, and the lines
//! before them stay plain.

use serde::Deserialize;
use vosh_log::LineKind;
use vosh_prompt::testkit::{gmcp, Build, Options};

use super::fake_mud::harness::Harness;

#[derive(Deserialize)]
struct Pairing {
    lines: Vec<Line>,
    packets: std::collections::HashMap<String, String>,
}

#[derive(Deserialize)]
struct Line {
    line: String,
    kind: String,
}

fn pairing() -> Pairing {
    serde_json::from_str(include_str!("../../../fixtures/scenes/pairing.json"))
        .expect("pairing.json reads")
}

/// The packet a fixture holds, as the game writes it.
fn packet(text: &str) -> Vec<u8> {
    let (package, json) = text.trim().split_once(' ').expect("a packet");
    gmcp(package, json)
}

/// The kind a fixture line names.
fn kind(name: &str) -> LineKind {
    match name {
        "text" => LineKind::Text,
        channel => LineKind::Channel(channel.to_string()),
    }
}

/// Every row of the session's log, as its plain text and kind, or none
/// while the session writes.
fn rows(h: &Harness) -> Vec<(String, Option<LineKind>)> {
    let log = *h.state.selected_session().logs().last().expect("a log");
    let Ok(guard) = h.state.logs.try_lock() else {
        return Vec::new();
    };
    let store = guard.as_ref().expect("the log");
    store
        .scene_lines(log, 0, i64::MAX, 10_000)
        .expect("the rows")
        .into_iter()
        .map(|line| (line.text, line.kind))
        .collect()
}

/// Play the fixture's pulse, each packet ahead of the text or after its
/// own line, and check every line's kind once the rows are in the log.
// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
async fn play(ahead: bool) {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options {
        name: "Orla".into(),
        ..Options::new(Build::New)
    })
    .await;
    h.connect().await;
    h.until_shown("Welcome to the fake Aabahran, Orla.").await;
    let fixture = pairing();
    let mut pulse = Vec::new();
    if ahead {
        for line in &fixture.lines {
            if let Some(text) = fixture.packets.get(&line.kind) {
                pulse.extend(packet(text));
            }
        }
    }
    for line in &fixture.lines {
        pulse.extend_from_slice(line.line.as_bytes());
        pulse.extend_from_slice(b"\n\r");
        if !ahead {
            if let Some(text) = fixture.packets.get(&line.kind) {
                pulse.extend(packet(text));
            }
        }
    }
    h.servers[0].push(&pulse);
    let last =
        vosh_protocol::ansi::plain_text(fixture.lines.last().expect("a line").line.as_bytes());
    h.until("the rows", |h| {
        rows(h).iter().any(|(text, _)| *text == last)
    })
    .await;
    let logged = rows(&h);
    for line in &fixture.lines {
        let text = vosh_protocol::ansi::plain_text(line.line.as_bytes());
        let found = logged
            .iter()
            .find(|(row, _)| *row == text)
            .unwrap_or_else(|| panic!("no row for {text}: {logged:#?}"));
        assert_eq!(found.1, Some(kind(&line.kind)), "{text}");
    }
    // The text before Char.Status is outside play.
    assert_eq!(logged[0].1, Some(LineKind::Login), "{logged:#?}");
    h.finish(grid).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_packet_ahead_of_its_line_names_it_past_the_lines_between() {
    play(true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_packet_after_its_line_names_it() {
    play(false).await;
}

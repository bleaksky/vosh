//! What a screen reader reads, through the real session against the fake
//! game (R21 and R25 review, Q19 to Q21): nothing while Read new game
//! lines is off, one event for the lines of one read once it is on, and
//! the switch from Settings takes effect on the next read.
//!
//! The lines are the game's own, from `update.c` and `magic.c` in the server
//! source.

use serde_json::{json, Value as Json};
use tauri::Manager;
use vosh_prompt::testkit::{Build, Options};

use super::fake_mud::harness::Harness;
use crate::app::events::SCREEN_READER;

const HUNGRY: &str = "You are hungry.";
const THIRSTY: &str = "You are thirsty.";
const TIRED: &str = "You feel less tired.";

/// Lines no command asked for, as `send_to_char` writes them after the
/// line end that ends the prompt.
fn lines(texts: &[&str]) -> Vec<u8> {
    format!("\n\r{}\n\r", texts.join("\n\r")).into_bytes()
}

fn sent(h: &Harness) -> Vec<Json> {
    h.events_of(h.first, SCREEN_READER)
}

/// Turn Read new game lines on or off from Settings.
async fn read_lines(h: &Harness, on: bool) {
    let field = json!({ "field": "screen_reader", "value": on });
    let fields = vec![serde_json::from_value(field).expect("a field")];
    crate::ipc::ui_config::ui_set_fields(h.app.handle().clone(), h.app.state(), fields, None)
        .await
        .expect("the save");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_read_sends_its_lines_once_and_only_while_the_reader_is_on() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options {
        name: "Orla".into(),
        ..Options::new(Build::New)
    })
    .await;
    h.connect().await;
    h.until_shown("Welcome to the fake Aabahran, Orla.").await;

    // Off, the default, a read sends nothing.
    h.servers[0].push(&lines(&[HUNGRY]));
    h.until_shown(HUNGRY).await;
    assert!(sent(&h).is_empty(), "{:#?}", sent(&h));

    // On, the next read sends its lines in one event.
    read_lines(&h, true).await;
    h.servers[0].push(&lines(&[THIRSTY, HUNGRY]));
    h.until("the lines", |h| !sent(h).is_empty()).await;
    let got = sent(&h);
    assert_eq!(got.len(), 1, "{got:#?}");
    assert_eq!(
        got[0],
        json!({
            "lines": [THIRSTY, HUNGRY],
            "count": 2,
            "prompt": null,
            "away": true,
        })
    );

    // Off again, the next read sends nothing.
    read_lines(&h, false).await;
    h.servers[0].push(&lines(&[TIRED]));
    h.until_shown(TIRED).await;
    assert_eq!(sent(&h).len(), 1);
    h.finish(grid).await;
}

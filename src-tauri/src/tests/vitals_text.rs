//! The vitals text push through the real session against the fake game:
//! nothing goes out until a footer or the status line watches, then a
//! render on the watch, on each Char.Vitals, each second while the text
//! reads the tick, and on a new text from Settings.

use std::time::Duration;

use serde_json::Value as Json;
use tauri::Manager;
use vosh_prompt::testkit::{gmcp, Build, Options};

use super::fake_mud::harness::Harness;
use crate::app::events::VITALS_TEXT;
use crate::session::last_packages::VITALS_PACKAGE;

/// The Char.Vitals of fixtures/gmcp/aabahran/char-vitals.gmcp.
fn vitals() -> Vec<u8> {
    let text = include_str!("../../../fixtures/gmcp/aabahran/char-vitals.gmcp");
    let (package, json) = text.trim().split_once(' ').expect("a packet");
    gmcp(package, json)
}

/// A line no command asked for, as `write_to_buffer` starts one.
fn line(text: &str) -> Vec<u8> {
    format!("\n\r{text}\n\r").into_bytes()
}

/// Every vitals text the app sent, whichever session it named.
fn texts(h: &Harness) -> Vec<Json> {
    h.events(VITALS_TEXT)
}

/// Connect the first session and wait for the vitals of the login.
async fn logged_in(h: &Harness) {
    h.connect().await;
    h.until("the vitals of the login", |h| {
        let session = h.state.session(Some(h.first)).expect("the session");
        session.last_packages.get(VITALS_PACKAGE).is_some()
    })
    .await;
}

/// Watch the first session's vitals text `cols` wide, or stop.
async fn watch(h: &Harness, cols: Option<usize>) {
    crate::ipc::vitals::vitals_text_watch(h.app.handle().clone(), h.app.state(), None, cols)
        .await
        .expect("the watch");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_watched_vitals_text_follows_char_vitals_and_names_its_session() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    logged_in(&h).await;
    assert!(texts(&h).is_empty(), "nothing watches yet");

    watch(&h, Some(40)).await;
    assert_eq!(texts(&h).len(), 1, "the watch draws the text at once");
    h.servers[0].push(&vitals());
    h.until("the render of the new vitals", |h| texts(h).len() == 2)
        .await;
    // Each render names the session it drew for.
    let named = h.events_of(h.first, VITALS_TEXT);
    assert_eq!(named.len(), 2);
    let drawn = &named[1];
    assert_eq!(drawn["live"]["plain"], "850/900hp 760/820mn 250/250mv");
    assert_eq!(drawn["full"]["plain"], "900/900hp 820/820mn 250/250mv");
    assert_eq!(drawn["fight"], serde_json::json!([false]));

    // A stopped watch sends nothing. The line after the packet shows
    // once the session handled the packet.
    watch(&h, None).await;
    h.servers[0].push(&vitals());
    h.servers[0].push(&line("Maren walks in."));
    h.until_shown("Maren walks in.").await;
    assert_eq!(texts(&h).len(), 2);
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_tick_draws_a_vitals_text_each_second_and_a_text_without_it_never() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.state.selected_profile().await.ui.vitals_text = "%hp (%tick)".into();
    logged_in(&h).await;

    let start = std::time::Instant::now();
    watch(&h, Some(40)).await;
    h.until("two renders the tick drove", |h| texts(h).len() == 3)
        .await;
    assert!(
        start.elapsed() >= Duration::from_millis(1900),
        "a render a second, {:?}",
        start.elapsed()
    );

    // A new text from Settings draws at once, and one without the tick
    // draws no more.
    let field = serde_json::json!({ "field": "vitals_text", "value": "%hp" });
    let fields = vec![serde_json::from_value(field).expect("a field")];
    crate::ipc::ui_config::ui_set_fields(h.app.handle().clone(), h.app.state(), fields, None)
        .await
        .expect("the save");
    let after = texts(&h).len();
    assert!(after >= 4, "the new text drew");
    assert_eq!(texts(&h)[after - 1]["live"]["plain"], "1020");
    tokio::time::sleep(Duration::from_millis(1600)).await;
    assert_eq!(texts(&h).len(), after);
    h.finish(grid).await;
}

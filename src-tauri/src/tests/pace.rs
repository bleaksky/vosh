//! The pace of what automation sends, against a fake game over a local
//! port. A trigger that answers a burst of game lines sends its first 10
//! at once and the rest at the pace, and one that answers each line with
//! the same command stops at the 40th, with a line that says so.

use vosh_automation::trigger::{Trigger, TriggerAction};
use vosh_prompt::testkit::{Build, Options};

use super::fake_mud::harness::Harness;

/// The counts of lines the pace held in the first session, as the page
/// heard them.
fn paced(h: &Harness) -> Vec<u64> {
    h.events_of(h.first, crate::app::events::PACED)
        .iter()
        .filter_map(|e| e["data"].as_u64())
        .collect()
}

/// Answer each `ping <n>` from the game with `template`.
async fn answer_pings(h: &Harness, template: &str) {
    h.state
        .selected_profile()
        .await
        .triggers
        .set(Trigger::new(
            "pong",
            r"^ping (\d+)$",
            TriggerAction::Send {
                template: template.into(),
            },
        ))
        .expect("the trigger");
}

/// `n` lines of `ping <i>` in one write.
fn pings(n: usize) -> Vec<u8> {
    (0..n)
        .flat_map(|i| format!("ping {i}\n\r").into_bytes())
        .collect()
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_burst_from_a_trigger_goes_at_the_pace() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.connect().await;
    answer_pings(&h, "say $1").await;
    h.servers[0].push(&pings(16));
    h.until("the pace to hold some", |h| paced(h).iter().any(|&n| n > 0))
        .await;
    assert!(paced(&h).iter().all(|&n| n <= 6), "{:?}", paced(&h));
    h.until("the pace to let them all go", |h| {
        paced(h).last() == Some(&0)
    })
    .await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_same_command_from_a_trigger_stops_at_the_fortieth() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    // The stop looks as each line leaves, so a quick pace keeps the wait
    // short.
    h.state.selected_profile().await.ui.send_pace_per_sec = 20;
    h.connect().await;
    answer_pings(&h, "kick").await;
    h.servers[0].push(&pings(45));
    h.until_shown("Vosh held back `kick`").await;
    h.finish(grid).await;
}

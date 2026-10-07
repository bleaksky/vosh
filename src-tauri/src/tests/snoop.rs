//! Snoop against the fake game. Aabahran sends snoop packets only to a
//! client that names Snoop in Core.Supports, so Vosh asks for it on
//! every connect.

use vosh_prompt::testkit::mud::telnet::{GMCP, IAC, SB};
use vosh_prompt::testkit::{Build, Options};

use super::fake_mud::harness::Harness;

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

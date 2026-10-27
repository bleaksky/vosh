//! How a profile reads the game's text, against a fake game over a local
//! port. A UTF-8 profile shows a Latin 1 byte as its letter, and a Latin
//! 1 profile reads every byte so and writes what you send so.

use vosh_prompt::testkit::{Build, Options};

use super::fake_mud::harness::Harness;
use crate::session::charset::Charset;

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_latin1_byte_shows_as_its_letter_in_a_utf8_profile() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.connect().await;
    h.servers[0].push("Orla says, 'café, naïve'\n\r".as_bytes());
    h.servers[0].push(b"Maren says, 'caf\xe9 au \x93lait\x94'\n\r");
    h.until_shown("Orla says, 'café, naïve'").await;
    h.until_shown("Maren says, 'café au \u{201C}lait\u{201D}'")
        .await;
    h.finish(grid).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_latin1_profile_reads_and_writes_latin1() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.state.selected_profile().await.charset = Charset::Latin1;
    h.connect().await;
    h.servers[0].push(b"Tolliver says, 'na\xefve \xc3\xa9'\n\r");
    h.until_shown("Tolliver says, 'naïve Ã©'").await;
    h.type_line("say café ÿ").await;
    h.until("the game to hear the line", |h| {
        let heard = h.servers[0].received.lock().expect("the bytes").clone();
        heard
            .windows(b"say caf\xe9 \xff\xff\r\n".len())
            .any(|w| w == b"say caf\xe9 \xff\xff\r\n")
    })
    .await;
    h.finish(grid).await;
}

//! The real session against the fake Aabahran of the test kit, over a
//! local port, in the new build and in the older build.
//!
//! Each test runs the session loop, the typed input path and the prompt
//! lookup with the mock runtime, and serves the fake game on a port of
//! its own. What the webview would hear arrives through the mock app's
//! listeners, and a native grid replays the output the way the terminal
//! shows it. The profile folder and the log live in a temporary folder.

pub(super) mod harness;

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use serde_json::Value as Json;
use tauri::{Listener, Manager};
use vosh_prompt::testkit::mud::{PROMPT, PROMPT_ALL};
use vosh_prompt::testkit::{Build, Options};

use crate::profile::set::DEFAULT_PROFILE_NAME;

use harness::{base64_decode, codes, codes_of, no_capture, output, Harness};

/// What the prompts off status says in `#prompt`.
const PROMPTS_OFF: &str =
    "You turned prompts off in the game. Type prompt in the game to turn them back on.";

/// The setting `prompt x` types in these tests, and what the game stores.
const TYPED_X: &str = "<%h/%Hhp %m/%Mmn>";
const PROMPT_X: &str = "<%h/%Hhp %m/%Mmn> ";

#[test]
fn base64_decodes_what_the_session_encodes() {
    for sample in [
        &b""[..],
        b"f",
        b"fo",
        b"foo",
        b"foob",
        b"\x1b]7717;o;1\x07<1020>\xff",
    ] {
        assert_eq!(base64_decode(&crate::output::base64_encode(sample)), sample);
    }
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_card_watches_your_prompt_and_an_edit_repaints_it() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.set_prompt(codes(PROMPT)).await;
    h.state
        .selected_profile()
        .await
        .set_name(DEFAULT_PROFILE_NAME);
    crate::ipc::prompt::prompt_watch(h.app.state(), true, None).expect("the card watches");
    h.connect().await;

    // While the card watches, the state follows each prompt, with the
    // pieces of the drawn design in the open row.
    h.until_last_row("<1020>").await;
    h.until("a prompt state", |h| {
        h.events("session://prompt-state")
            .last()
            .is_some_and(|s| !s["open_row"].is_null())
    })
    .await;
    let state = h
        .events("session://prompt-state")
        .pop()
        .expect("the prompt state");
    assert_eq!(state["new_build"], true);
    assert_eq!(state["status"]["status"], "matching");
    let spans: Vec<(i64, i64, i64)> = state["open_row"]["spans"]
        .as_array()
        .expect("spans")
        .iter()
        .map(|s| {
            (
                s["piece"].as_i64().unwrap(),
                s["col"].as_i64().unwrap(),
                s["width"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(spans, [(0, 0, 1), (1, 1, 4), (2, 5, 1)]);
    assert_eq!(state["open_row"]["plain"], "<1020>");
    // With them, the game's own line the drawn prompt replaced, for the
    // card's marks while it reads your codes.
    let raw = state["open_row"]["raw_lines"]
        .as_array()
        .expect("the lines");
    assert_eq!(raw.len(), 1);
    assert!(raw[0].as_str().is_some_and(|l| l.contains("hp")), "{raw:?}");
    assert_eq!(state["open_row"]["raw_from"], 0);
    let hp = state["catalog"]
        .as_array()
        .expect("the catalog")
        .iter()
        .find(|f| f["name"] == "hp")
        .expect("hp");
    assert_eq!(hp["state"], "value");
    assert_eq!(hp["in_prompt"], true);

    // The card edits the design and saves it, and the open row repaints
    // at once. Every window hears which profile's table changed.
    let op: vosh_prompt::card::edit::EditOp = serde_json::from_value(serde_json::json!({
        "op": "insert_field",
        "at": 3,
        "field": "mana",
    }))
    .expect("an op");
    let edited = {
        let session = h.state.selected_session();
        let p = h.state.selected_profile().await;
        let c = session.connection.lock();
        crate::prompt::edit(&p, &c, "<%hp>", &op).expect("the edit")
    };
    assert_eq!(edited.template, "<%hp>%mana");
    let config = vosh_prompt::PromptConfig {
        template: edited.template,
        ..h.prompt_table().await
    };
    crate::ipc::prompt::prompt_config_set(
        h.app.handle().clone(),
        h.app.state(),
        config,
        None,
        None,
    )
    .await
    .expect("the table saves");
    h.until_last_row("<1020>800").await;
    assert_eq!(
        h.events(crate::app::events::PROMPT_CONFIG_CHANGED),
        [serde_json::json!({"profile": DEFAULT_PROFILE_NAME})]
    );
    // The state follows the repaint too, so the card maps a pointer with
    // the pieces the row shows now.
    h.until("the state after the repaint", |h| {
        h.events("session://prompt-state")
            .last()
            .is_some_and(|s| s["open_row"]["plain"] == "<1020>800")
    })
    .await;
    let state = h
        .events("session://prompt-state")
        .pop()
        .expect("the prompt state");
    assert_eq!(state["open_row"]["spans"][3]["col"], 6);

    // Once the card stops watching, no state follows the prompts.
    crate::ipc::prompt::prompt_watch(h.app.state(), false, None).expect("the card stops watching");
    let watched = h.events("session://prompt-state").len();
    h.type_line("pulses 2").await;
    h.until_shown("Pulse 2 of 2.").await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(h.events("session://prompt-state").len(), watched);
    h.finish(grid).await;
}

/// The pieces of the open row in a `session://prompt-state` payload, as
/// (piece, column, width).
fn open_spans(state: &Json) -> Vec<(i64, i64, i64)> {
    state["open_row"]["spans"]
        .as_array()
        .expect("spans")
        .iter()
        .map(|s| {
            (
                s["piece"].as_i64().unwrap(),
                s["col"].as_i64().unwrap(),
                s["width"].as_i64().unwrap(),
            )
        })
        .collect()
}

/// The newest prompt state whose open row is `cols` wide.
fn state_at(h: &Harness, cols: usize) -> Option<Json> {
    h.events("session://prompt-state")
        .pop()
        .filter(|s| s["open_row"]["plain"].as_str().map(str::len) == Some(cols))
}

// While the card watches, a new width that draws a push to the right
// edge again sends the state with the repaint, so the card's marks and
// click targets move with the push. The design has no clock, so nothing
// else would send it before the next prompt.
// The guard keeps other tests off the shared native grid.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_width_tells_the_card_where_the_push_draws_now() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.set_prompt(vosh_prompt::PromptConfig {
        template: "<%hp>%{right}%mana".into(),
        ..codes(PROMPT)
    })
    .await;
    crate::ipc::prompt::prompt_watch(h.app.state(), true, None).expect("the card watches");
    h.connect().await;

    // The session starts 100 wide, so mana takes the last three columns.
    h.until("the state at 100 columns", |h| state_at(h, 100).is_some())
        .await;
    let wide = state_at(&h, 100).expect("the state");
    assert_eq!(
        open_spans(&wide),
        [(0, 0, 1), (1, 1, 4), (2, 5, 1), (3, 6, 91), (4, 97, 3)]
    );

    // Narrower, the same row draws again and the card hears it at once.
    if let Some(handle) = h.state.selected_session().slot.lock().await.as_ref() {
        assert!(handle.set_window_size(80, 40));
    }
    h.until("the state at 80 columns", |h| state_at(h, 80).is_some())
        .await;
    let narrow = state_at(&h, 80).expect("the state");
    assert_eq!(
        open_spans(&narrow),
        [(0, 0, 1), (1, 1, 4), (2, 5, 1), (3, 6, 71), (4, 77, 3)]
    );
    assert_eq!(
        repaints(&h).last().map(String::as_str),
        Some(format!("<1020>{}800", " ".repeat(71)).as_str())
    );
    // The state names the region the repaint wrote, which the card finds
    // on screen by its generation.
    let gen = narrow["open_row"]["gen"].as_u64().expect("a generation");
    let repaint = h
        .events("session://output")
        .last()
        .and_then(|payload| output(&payload.to_string()).replace)
        .expect("the repaint");
    let mark = vosh_prompt::stage::mark(gen);
    assert!(
        repaint.bytes.windows(mark.len()).any(|w| w == mark),
        "region {gen} in {:?}",
        String::from_utf8_lossy(&repaint.bytes)
    );
    h.finish(grid).await;
}

// The webview echoes your line and sends it with two calls nothing
// orders, so the session can hear of the echo only after the game
// answered. The echo came before the answer on screen, so the prompt
// that ends the answer stays the open row and an edit still repaints it.
// The guard keeps other tests off the shared native grid.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_echo_the_session_hears_of_late_leaves_the_prompt_after_it_open() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.set_prompt(codes(PROMPT)).await;
    h.state
        .selected_profile()
        .await
        .set_name(DEFAULT_PROFILE_NAME);
    h.connect().await;
    h.until_last_row("<1020>").await;

    // Your echo lands on the terminal after the login prompt, but your
    // line reaches the session first, and the game answers.
    let after = h.echo("look");
    crate::ipc::session::session_send_input(
        h.app.handle().clone(),
        h.app.state(),
        "look".into(),
        None,
    )
    .await
    .expect("the line goes out");
    h.until_shown("[Exits: south]").await;
    h.until_last_row("<1020>").await;

    // Only now does the session hear of the echo.
    if let Some(handle) = h.state.selected_session().slot.lock().await.as_ref() {
        let _ = handle.local_write(after);
    }

    // The prompt that answered your look is still the open row.
    let config = vosh_prompt::PromptConfig {
        template: "<%hp>%mana".into(),
        ..h.prompt_table().await
    };
    crate::ipc::prompt::prompt_config_set(
        h.app.handle().clone(),
        h.app.state(),
        config,
        None,
        None,
    )
    .await
    .expect("the table saves");
    h.until_last_row("<1020>800").await;
    assert_eq!(
        h.screen()
            .iter()
            .filter(|row| row.starts_with("<1020>"))
            .count(),
        2,
        "{:#?}",
        h.screen()
    );
    h.finish(grid).await;
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_new_build_gives_vosh_the_prompt_at_login_and_follows_the_game() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    // A capture that follows the game, on another setting than the one
    // the game holds.
    h.set_prompt(codes("<%hhp> ")).await;
    h.connect().await;

    // Login alone gives Vosh the PROMPT, and the first prompt reads with
    // it.
    h.until_last_row("<1020>").await;
    assert_eq!(
        codes_of(&h.capture().await),
        (
            PROMPT.to_string(),
            Some(vosh_prompt::config::CaptureSource::Gmcp)
        )
    );
    assert_eq!(
        h.events("session://game-prompt-seen"),
        [serde_json::json!({"kind": "gmcp", "text": PROMPT, "applied": true})]
    );
    let seen = crate::prompt::last_seen::last_seen(&h.state, &h.state.selected_session())
        .await
        .expect("the game sent it");
    assert_eq!(seen.source, "gmcp");
    assert!(seen.at_login);
    assert_eq!(seen.prompt.as_deref(), Some(PROMPT));
    assert_eq!(seen.enabled, Some(true));
    assert_eq!(seen.character.as_deref(), Some("Tester"));
    assert!(h
        .state
        .selected_session()
        .connection
        .lock()
        .prompt
        .vars
        .new_build());

    // prompt x in the game: Char.Prompt comes before its reply, and the
    // prompt right after the reply reads with the new codes.
    h.type_line(&format!("prompt {TYPED_X}")).await;
    h.until_shown(&format!("Prompt set to {TYPED_X}")).await;
    h.until_last_row("<1020>").await;
    assert_eq!(
        codes_of(&h.capture().await),
        (
            PROMPT_X.to_string(),
            Some(vosh_prompt::config::CaptureSource::Gmcp)
        )
    );
    let toasts: Vec<Json> = h
        .events("session://game-prompt-seen")
        .into_iter()
        .filter(|e| e["applied"] == true)
        .collect();
    assert_eq!(toasts.len(), 2, "one toast at login, one for prompt x");
    assert_eq!(toasts[1]["text"], PROMPT_X);
    let seen = crate::prompt::last_seen::last_seen(&h.state, &h.state.selected_session())
        .await
        .expect("seen");
    assert!(!seen.at_login);

    // prompt off: the prompts off status, and no miss while the packages
    // keep coming with no prompt text.
    h.type_line("prompt off").await;
    h.until("the prompts off status", |h| {
        h.events("session://prompt-status")
            .last()
            .is_some_and(|s| s["status"] == "prompts_off")
    })
    .await;
    h.type_line("pulses 5").await;
    h.until_shown("Pulse 5 of 5.").await;
    // One more pulse's worth of time for any late status.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let statuses = h.events("session://prompt-status");
    assert!(
        statuses.iter().all(|s| s["status"] != "not_matching"),
        "{statuses:?}"
    );
    assert_eq!(statuses.last().expect("a status")["status"], "prompts_off");
    assert_eq!(
        codes_of(&h.capture().await).0,
        PROMPT_X,
        "prompt off keeps the codes"
    );
    h.type_line("#prompt").await;
    h.until_shown(PROMPTS_OFF).await;

    // prompt turns them back on, and Vosh reads the prompt again.
    h.type_line("prompt").await;
    h.until("the matching status", |h| {
        h.events("session://prompt-status")
            .last()
            .is_some_and(|s| s["status"] == "matching")
    })
    .await;
    h.until_last_row("<1020>").await;
    h.finish(grid).await;
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_older_build_reads_your_prompt_from_the_game_replies() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    // No GA, so a prompt with no line end draws by the settle rule.
    let h = Harness::new(Options {
        ga: false,
        ..Options::new(Build::Older)
    })
    .await;
    h.set_prompt(no_capture()).await;
    h.connect().await;

    // Nothing reads the prompt yet, so the game's own prompt shows.
    h.until_last_row("[1020/1020hp 800/800mn 930/930mv]").await;
    let leftover = &h.events("session://game-prompt-seen");
    assert!(leftover.is_empty(), "{leftover:?}");

    // Paste the PROMPT.
    h.type_line(&format!("#prompt game {{{PROMPT}}}")).await;
    h.until_shown("Vosh reads Health, Mana, and Moves with their maxes from this prompt.")
        .await;
    assert_eq!(
        codes_of(&h.capture().await),
        (
            PROMPT.to_string(),
            Some(vosh_prompt::config::CaptureSource::Typed)
        )
    );
    h.type_line("look").await;
    h.until_last_row("<1020>").await;

    // prompt all, with no %c, draws, and the reply updates the capture.
    h.type_line("prompt all").await;
    h.until_shown("Prompt set to %n%P%C<%hhp %mm %vmv>").await;
    h.until_last_row("<1020>").await;
    assert_eq!(
        codes_of(&h.capture().await),
        (
            PROMPT_ALL.to_string(),
            Some(vosh_prompt::config::CaptureSource::Session)
        )
    );
    assert!(
        h.screen().iter().all(|row| !row.contains("930mv>")),
        "the game's prompt all never shows: {:#?}",
        h.screen()
    );
    h.type_line("look").await;
    h.until_last_row("<1020>").await;

    // prompt x updates the capture through the observer.
    h.type_line(&format!("prompt {TYPED_X}")).await;
    h.until("the capture to take prompt x", |h| {
        h.events("session://game-prompt-seen")
            .iter()
            .any(|e| e["text"] == PROMPT_X)
    })
    .await;
    assert_eq!(
        codes_of(&h.capture().await),
        (
            PROMPT_X.to_string(),
            Some(vosh_prompt::config::CaptureSource::Session)
        )
    );
    let seen = h.events("session://game-prompt-seen");
    assert_eq!(
        seen,
        [
            serde_json::json!({"kind": "prompt", "text": PROMPT_ALL, "applied": true}),
            serde_json::json!({"kind": "prompt", "text": PROMPT_X, "applied": true}),
        ]
    );
    h.until_last_row("<1020>").await;
    let last = crate::prompt::last_seen::last_seen(&h.state, &h.state.selected_session())
        .await
        .expect("seen");
    assert_eq!(last.source, "session");
    assert_eq!(last.prompt.as_deref(), Some(PROMPT_X));

    // prompt off saves nothing, and says prompts are off.
    h.type_line("prompt off").await;
    h.until_shown("You will no longer see prompts.").await;
    h.until("the prompts off status", |h| {
        h.events("session://prompt-status")
            .last()
            .is_some_and(|s| s["status"] == "prompts_off")
    })
    .await;
    assert_eq!(codes_of(&h.capture().await).0, PROMPT_X);
    assert_eq!(
        h.events("session://game-prompt-seen").last(),
        Some(&serde_json::json!({"kind": "off", "text": "", "applied": false}))
    );
    h.type_line("#prompt").await;
    h.until_shown(PROMPTS_OFF).await;
    h.finish(grid).await;
}

/// The text of each repaint the session sent, oldest first: the region a
/// replace with nothing after it writes, or the band it pins.
fn repaints(h: &Harness) -> Vec<String> {
    h.events("session://output")
        .iter()
        .map(|payload| output(&payload.to_string()))
        .filter(|out| out.bytes.is_empty())
        .filter_map(|out| {
            let bytes = out.replace.map(|r| r.bytes)?;
            Some(
                vosh_protocol::ansi::plain_text(&bytes)
                    .trim_end()
                    .to_string(),
            )
        })
        .collect()
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_tick_counts_down_in_your_idle_prompt_and_waits_while_you_read() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.set_prompt(codes(PROMPT)).await;
    h.connect().await;
    h.until_last_row("<1020>").await;
    // A design with no clock piece never repaints an idle prompt.
    let outputs = h.events("session://output").len();
    tokio::time::sleep(Duration::from_millis(1_500)).await;
    assert_eq!(h.events("session://output").len(), outputs);

    // With the tick in the design, the next prompt counts down.
    h.set_prompt(vosh_prompt::PromptConfig {
        template: "<%hp> %tick".into(),
        ..codes(PROMPT)
    })
    .await;
    h.type_line("look").await;
    h.until("the drawn tick", |h| h.last_row().starts_with("<1020> "))
        .await;
    h.until("three repaints of the tick", |h| repaints(h).len() >= 3)
        .await;
    // Each repaint shows the next second, with none skipped.
    let seconds: Vec<i64> = repaints(&h)
        .iter()
        .map(|row| {
            row.strip_prefix("<1020> ")
                .and_then(|n| n.parse().ok())
                .unwrap_or_else(|| panic!("a tick in {row:?}"))
        })
        .collect();
    for pair in seconds.windows(2) {
        assert_eq!(pair[1], pair[0] - 1, "{seconds:?}");
    }
    assert_eq!(
        h.last_row(),
        format!("<1020> {}", seconds[seconds.len() - 1])
    );

    // While you select text or read back, the row stays as it is.
    h.state
        .selected_session()
        .reader_busy
        .store(true, std::sync::atomic::Ordering::Release);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let held = repaints(&h).len();
    tokio::time::sleep(Duration::from_millis(1_500)).await;
    assert_eq!(repaints(&h).len(), held);
    // Once you let go it catches up within the second.
    h.state
        .selected_session()
        .reader_busy
        .store(false, std::sync::atomic::Ordering::Release);
    h.until("the tick again", |h| repaints(h).len() > held)
        .await;

    // Your line closes the row, and nothing repaints it after.
    h.type_line("#help").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let after = repaints(&h).len();
    tokio::time::sleep(Duration::from_millis(1_200)).await;
    assert_eq!(repaints(&h).len(), after);
    h.finish(grid).await;
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_game_that_answers_each_do_eor_ends_negotiation_in_one_round() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    // A game that answers every DO EOR with WILL EOR and then marks each
    // prompt with EOR alone. prompt all ends in no line end, and a
    // pattern that never settles waits for a mark, so only the EOR makes
    // it your prompt.
    let h = Harness::new(Options {
        prompt: PROMPT_ALL.into(),
        ga: false,
        eor: true,
        ..Options::new(Build::Older)
    })
    .await;
    h.set_prompt(vosh_prompt::PromptConfig {
        capture: vosh_prompt::CaptureConfig::Regex(vosh_prompt::config::RegexCapture {
            lines: vec![r"^<(?<hp>\d+)hp (?<mana>\d+)m (?<move>\d+)mv> $".into()],
            settle: false,
            ..vosh_prompt::config::RegexCapture::default()
        }),
        ..no_capture()
    })
    .await;
    h.connect().await;
    h.until_last_row("<1020>").await;
    h.type_line("look").await;
    h.until_shown("look").await;
    h.until_last_row("<1020>").await;
    // Long enough for a back and forth to show many asks.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(h.eor_asks.load(Ordering::SeqCst), 1);
    assert_eq!(h.last_row(), "<1020>");
    h.finish(grid).await;
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_code_reader_the_card_chose_hears_your_prompt_on_another_host() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    // A local server of the older build, which Vosh does not know as The
    // Forsaken Lands, and a profile that reads no prompt yet.
    let h = Harness::new(Options::new(Build::Older)).await;
    h.set_prompt(no_capture()).await;
    h.connect().await;
    h.until_shown("[1020/1020hp 800/800mn 930/930mv]").await;

    // Before the card chooses the code reader, the reply is just text.
    h.type_line("prompt").await;
    h.until_shown(&format!("Current prompt: {PROMPT}")).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let leftover = &h.events("session://game-prompt-seen");
    assert!(leftover.is_empty(), "{leftover:?}");

    // More > Use Forsaken Lands prompt codes… in the card, then prompt in
    // the game: the reply fills the card's fields (P2).
    crate::ipc::prompt::prompt_code_reader_set(h.app.state(), true, None)
        .await
        .expect("the card chose the code reader");
    h.type_line("prompt").await;
    h.until("the reply to prompt", |h| {
        !h.events("session://game-prompt-seen").is_empty()
    })
    .await;
    assert_eq!(
        h.events("session://game-prompt-seen"),
        [serde_json::json!({"kind": "prompt", "text": PROMPT, "applied": false})]
    );
    assert!(h.capture().await.is_none(), "nothing saves before you do");
    h.finish(grid).await;
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_log_lookup_finds_only_the_prompt_of_the_profiles_own_character() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::Older)).await;
    h.set_prompt(no_capture()).await;

    // Tester plays on the older build and asks for the prompt.
    h.connect().await;
    h.until_last_row("[1020/1020hp 800/800mn 930/930mv]").await;
    h.type_line("prompt").await;
    h.until_shown(&format!("Current prompt: {PROMPT}")).await;
    h.until_last_row("[1020/1020hp 800/800mn 930/930mv]").await;
    h.disconnect().await;

    // Default claims Tester, so its card prefills from the log.
    let seen = crate::prompt::last_seen::last_seen(&h.state, &h.state.selected_session())
        .await
        .expect("the log holds Tester's prompt");
    assert_eq!(seen.source, "log");
    assert_eq!(seen.prompt.as_deref(), Some(PROMPT));
    assert_eq!(seen.character.as_deref(), Some("Tester"));

    // Healer claims Healer, so its card starts empty.
    {
        let mut set = h.state.profile_set.lock().await;
        set.as_mut()
            .expect("the set")
            .switch("Healer")
            .expect("the switch");
    }
    assert_eq!(
        crate::prompt::last_seen::last_seen(&h.state, &h.state.selected_session()).await,
        None
    );

    // Healer logs in on the same game. The log still holds only
    // Tester's prompt, so Healer's card stays empty.
    h.fake.lock().expect("the options").name = "Healer".into();
    h.connect().await;
    h.until_shown("Welcome to the fake Aabahran, Healer.").await;
    h.until("the session to name Healer", |h| {
        h.state
            .selected_session()
            .current_character
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .as_deref()
            == Some("Healer")
    })
    .await;
    assert_eq!(
        crate::prompt::last_seen::last_seen(&h.state, &h.state.selected_session()).await,
        None
    );
    h.finish(grid).await;
}

/// What `session://hidden` says for each value.
fn hidden(vitals: bool, tank: bool, opponent: bool, affects: bool, group: bool) -> Json {
    serde_json::json!({
        "vitals": vitals,
        "tank": tank,
        "opponent": opponent,
        "affects": affects,
        "group": group,
    })
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lament_and_blindness_hide_what_each_build_hides() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let mut done = Vec::new();
    for build in [Build::New, Build::Unflagged, Build::Older] {
        let h = Harness::new(Options::new(build)).await;
        h.set_prompt(codes(PROMPT)).await;
        h.connect().await;
        h.until_last_row("<1020>").await;
        h.type_line("fight").await;
        h.until_last_row("<765>").await;
        // The design reads nothing on the tank line, so it shows as sent.
        assert!(h.screen().iter().any(|r| r == "Tester: [===|===|===|---]"));

        // The song hides every value it hides, on every build.
        h.type_line("lament").await;
        h.until_last_row("<?>").await;
        h.until(&format!("{build:?} to hide everything"), |h| {
            h.events("session://hidden").last() == Some(&hidden(true, true, true, true, true))
        })
        .await;
        assert!(h.screen().iter().any(|r| r == "Tester:"), "{build:?}");

        // The song ends, and each value shows again.
        h.type_line("lament").await;
        h.until(&format!("{build:?} to hide nothing"), |h| {
            h.events("session://hidden").last() == Some(&hidden(false, false, false, false, false))
        })
        .await;
        h.until_last_row("<765>").await;

        // Blindness withholds your opponent's health on the builds that
        // withhold it, and the tank's health stays.
        h.type_line("blind").await;
        h.until_shown("You are blinded!").await;
        h.until_last_row("<765>").await;
        let want = hidden(false, false, build != Build::Older, false, false);
        h.until(&format!("{build:?} blind"), |h| {
            h.events("session://hidden").last() == Some(&want)
        })
        .await;
        h.disconnect().await;
        done.push(h);
    }
    // Let the saves the song marked land before the folders go.
    drop(grid);
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    drop(done);
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reconnect_reads_the_prompt_until_char_prompt_comes_again() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options {
        reconnect: true,
        ..Options::new(Build::New)
    })
    .await;
    h.set_prompt(codes(PROMPT)).await;
    h.connect().await;
    // No Char.Prompt comes, so the session has no sign of the new build,
    // and the prompt reads from the saved codes all the same.
    h.until_shown("Reconnecting.").await;
    h.until_last_row("<1020>").await;
    assert!(!h
        .state
        .selected_session()
        .connection
        .lock()
        .prompt
        .vars
        .new_build());
    let leftover = &h.events("session://game-prompt-seen");
    assert!(leftover.is_empty(), "{leftover:?}");
    // prompt in the game sends Char.Prompt again.
    h.type_line("prompt").await;
    h.until_shown(&format!("Current prompt: {PROMPT}")).await;
    h.until("the game's prompt settings", |h| {
        !h.events("session://game-prompt-seen").is_empty()
    })
    .await;
    assert!(h
        .state
        .selected_session()
        .connection
        .lock()
        .prompt
        .vars
        .new_build());
    assert_eq!(
        h.events("session://game-prompt-seen"),
        [serde_json::json!({"kind": "gmcp", "text": PROMPT, "applied": false})]
    );
    let seen = crate::prompt::last_seen::last_seen(&h.state, &h.state.selected_session())
        .await
        .expect("seen");
    assert_eq!(seen.source, "gmcp");
    h.finish(grid).await;
}

/// What the page hears on `session://gmcp/Char-Name`, kept as it comes.
fn hear_char_name(h: &Harness) -> Arc<StdMutex<Vec<Json>>> {
    let heard = Arc::new(StdMutex::new(Vec::new()));
    let keep = heard.clone();
    h.app.listen_any("session://gmcp/Char-Name", move |e| {
        let payload: Json = serde_json::from_str(e.payload()).expect("a JSON payload");
        keep.lock().expect("the names").push(payload);
    });
    heard
}

// A character left link dead takes the new link with no Char.Status
// (`check_reconnect`, comm.c), so the name you picked at the account menu
// names the character once the game plays: the session, its row and the
// page all learn it. The guard keeps other tests off the shared native
// grid.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reconnect_names_the_character_you_picked_at_the_account_menu() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options {
        reconnect: true,
        account: vec!["Tolliver".into(), "Maren".into()],
        ..Options::new(Build::New)
    })
    .await;
    let names = hear_char_name(&h);
    h.connect().await;
    h.until_shown("Your choice>").await;
    h.type_line("2").await;
    h.until_shown("Reconnecting.").await;
    let session = h.state.selected_session();
    h.until("the character from the pick", |_| {
        session.character().as_deref() == Some("Maren")
    })
    .await;
    assert_eq!(session.row(true).character.as_deref(), Some("Maren"));
    h.until("the page hears the name", |_| {
        !names.lock().expect("the names").is_empty()
    })
    .await;
    let heard = names.lock().expect("the names").clone();
    assert_eq!(heard.len(), 1, "{heard:?}");
    assert_eq!(heard[0]["data"], serde_json::json!({ "name": "Maren" }));
    assert_eq!(heard[0]["session"], serde_json::json!(h.first));
    h.finish(grid).await;
}

// A fresh login names the character with Char.Status, so the pick names
// no one of its own and the page hears no Char.Name.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fresh_login_from_the_account_menu_takes_the_name_from_char_status() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options {
        account: vec!["Orla".into(), "Tester".into()],
        ..Options::new(Build::New)
    })
    .await;
    let names = hear_char_name(&h);
    h.connect().await;
    h.until_shown("Your choice>").await;
    h.type_line("2").await;
    h.until_shown("Welcome to the fake Aabahran, Tester.").await;
    let session = h.state.selected_session();
    h.until("the character from Char.Status", |_| {
        session.character().as_deref() == Some("Tester")
    })
    .await;
    h.until_shown("[1020/1020hp").await;
    assert!(names.lock().expect("the names").is_empty());
    h.finish(grid).await;
}

/// The pattern the old capture trigger held, for the PROMPT the fake
/// game starts with.
const OLD_PATTERN: &str =
    r"\[(?<hp>\d+)/(?<maxhp>\d+)hp (?<mana>\d+)/(?<maxmana>\d+)mn (?<move>\d+)/(?<maxmove>\d+)mv\]";

/// James's design.
const DESIGN: &str = vosh_prompt::testkit::designs::JAMES;

/// What his design draws at full health.
const DRAWN: &str = "[1020(100%)h 800(100%)m 930(100%)v]";

/// The prompt James typed after the move, and what the game stores.
const TYPED_NEW: &str = "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv (%K hp) %s [%S]>";
const PROMPT_NEW: &str = "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv (%K hp) %s [%S]> ";

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_changed_prompt_the_pattern_misses_says_no_prompt_matched() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let mut done = Vec::new();
    // On The Forsaken Lands three pulses miss, and elsewhere three sends.
    for forsaken in [true, false] {
        let h = Harness::new(Options::new(Build::New)).await;
        if forsaken {
            h.count_as_forsaken_lands();
        }
        h.set_prompt(no_capture()).await;
        h.connect().await;
        h.until_last_row("[1020/1020hp 800/800mn 930/930mv]").await;
        h.type_line(&format!("#prompt {{{OLD_PATTERN}}}")).await;
        h.until_shown("from your prompt with this pattern.").await;
        h.type_line("look").await;
        h.until_last_row("<1020>").await;
        let status = |h: &Harness| {
            h.events("session://prompt-status")
                .last()
                .map(|s| s["status"].as_str().unwrap_or_default().to_string())
        };
        assert_eq!(status(&h).as_deref(), Some("matching"), "{forsaken}");

        // A new prompt in the game, which the pattern does not read.
        h.type_line(&format!("prompt {TYPED_X}")).await;
        h.until_last_row("<1020/1020hp 800/800mn>").await;
        let rooms = |h: &Harness| {
            h.screen()
                .iter()
                .filter(|r| r.as_str() == "[Exits: south]")
                .count()
        };
        for _ in 0..3 {
            let before = rooms(&h);
            h.type_line("look").await;
            h.until("the room", |h| rooms(h) > before).await;
        }
        h.until("no prompt matching", |h| {
            status(h).as_deref() == Some("not_matching")
        })
        .await;
        let last = h.events("session://prompt-status").pop().expect("a status");
        assert!(last["last_match_at"].is_string(), "{forsaken}: {last}");
        h.type_line("#prompt").await;
        h.until_shown("If you changed it in the game, point at it again.")
            .await;
        // The prompt the pattern reads again clears it.
        h.type_line(&format!("prompt {PROMPT}")).await;
        h.until("matching again", |h| {
            status(h).as_deref() == Some("matching")
        })
        .await;
        h.disconnect().await;
        done.push(h);
    }
    // Let the saves the commands marked land before the folders go.
    drop(grid);
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    drop(done);
}

/// A table as the move from the old capture trigger wrote it: the
/// trigger's pattern, his design and drawing on.
fn migrated() -> vosh_prompt::PromptConfig {
    vosh_prompt::PromptConfig {
        draw: true,
        template: DESIGN.into(),
        capture: vosh_prompt::CaptureConfig::Regex(vosh_prompt::config::RegexCapture {
            lines: vec![OLD_PATTERN.into()],
            settle: false,
            source: Some(vosh_prompt::config::CaptureSource::Migrated),
            ..vosh_prompt::config::RegexCapture::default()
        }),
        ..vosh_prompt::PromptConfig::default()
    }
}

/// The codes an aabahran capture holds.
fn aabahran(capture: &vosh_prompt::CaptureConfig) -> vosh_prompt::config::AabahranCapture {
    match capture {
        vosh_prompt::CaptureConfig::Aabahran(codes) => codes.clone(),
        other => panic!("no aabahran capture: {other:?}"),
    }
}

/// The capture a profile file holds on disk, once it reads.
fn saved_capture(file: &std::path::Path) -> Option<vosh_prompt::CaptureConfig> {
    crate::profile::file::ProfileConfig::load(file)
        .ok()
        .map(|config| config.prompt_config().capture)
}

/// The reports the capture took, each of which raises the toast.
fn toasts(h: &Harness) -> Vec<Json> {
    h.events("session://game-prompt-seen")
        .into_iter()
        .filter(|e| e["applied"] == true)
        .collect()
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_moved_capture_switches_at_login_and_draws_his_new_prompt() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    // James plays an immortal, so (Wizi 60) and (Incog 60) come first.
    let h = Harness::new(Options {
        wizi: 60,
        incog: 60,
        ..Options::new(Build::New)
    })
    .await;
    h.count_as_forsaken_lands();
    h.set_prompt(migrated()).await;
    h.connect().await;

    // Login alone switches the pattern to the codes the game sent, and
    // the first prompt draws his design over the whole line.
    h.until_last_row(DRAWN).await;
    let table = h.prompt_table().await;
    let codes = aabahran(&table.capture);
    assert_eq!(codes.prompt, PROMPT);
    assert_eq!(codes.fprompt, "");
    assert!(codes.follow_game);
    assert_eq!(codes.source, Some(vosh_prompt::config::CaptureSource::Gmcp));
    assert!(codes.seen_at.is_some());
    assert!(table.draw, "the switch stays");
    assert_eq!(table.template, DESIGN, "the design stays");
    assert_eq!(
        h.events("session://game-prompt-seen"),
        [serde_json::json!({"kind": "gmcp", "text": PROMPT, "applied": true})]
    );
    assert!(
        h.screen().iter().all(|r| !r.contains("(Wizi 60)")),
        "{:#?}",
        h.screen()
    );

    // His new prompt in the game. Char.Prompt comes before the reply, so
    // the prompt right after it draws his design over the new line.
    h.type_line(&format!("prompt {TYPED_NEW}")).await;
    h.until_shown(&format!("Prompt set to {TYPED_NEW}")).await;
    h.until_last_row(DRAWN).await;
    let codes = aabahran(&h.capture().await);
    assert_eq!(codes.prompt, PROMPT_NEW);
    assert_eq!(codes.source, Some(vosh_prompt::config::CaptureSource::Gmcp));
    let screen = h.screen();
    assert!(
        screen
            .iter()
            .all(|r| !r.contains("(100 hp)") && !r.contains("(Wizi 60)")),
        "the new line never shows raw: {screen:#?}"
    );
    let taken = toasts(&h);
    assert_eq!(taken.len(), 2, "one toast at login, one for the new prompt");
    assert_eq!(taken[1]["text"], PROMPT_NEW);

    // The profile file holds the codes once the save lands.
    let file = h.profile_file(DEFAULT_PROFILE_NAME).await;
    h.until("the saved codes", |_| {
        saved_capture(&file).is_some_and(|c| {
            matches!(c, vosh_prompt::CaptureConfig::Aabahran(codes) if codes.prompt == PROMPT_NEW)
        })
    })
    .await;
    h.finish(grid).await;
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_older_build_switches_a_moved_capture_from_the_reply_to_prompt() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::Older)).await;
    h.count_as_forsaken_lands();
    h.set_prompt(migrated()).await;
    h.connect().await;

    // The pattern reads the old line and draws, and nothing switches at
    // login, since the older build sends no Char.Prompt.
    h.until_last_row(DRAWN).await;
    assert_eq!(h.prompt_table().await, migrated());
    let leftover = &h.events("session://game-prompt-seen");
    assert!(leftover.is_empty(), "{leftover:?}");

    // prompt off switches nothing, and says prompts are off.
    h.type_line("prompt off").await;
    h.until_shown("You will no longer see prompts.").await;
    h.until("the prompts off status", |h| {
        h.events("session://prompt-status")
            .last()
            .is_some_and(|s| s["status"] == "prompts_off")
    })
    .await;
    assert_eq!(h.prompt_table().await, migrated());
    let leftover = &toasts(&h);
    assert!(leftover.is_empty(), "{leftover:?}");

    // prompt x switches it through the observer, and the prompt after
    // the reply draws.
    h.type_line(&format!("prompt {TYPED_X}")).await;
    h.until("the capture to switch", |h| {
        toasts(h).iter().any(|e| e["text"] == PROMPT_X)
    })
    .await;
    let table = h.prompt_table().await;
    let codes = aabahran(&table.capture);
    assert_eq!(codes.prompt, PROMPT_X);
    assert!(codes.follow_game);
    assert_eq!(
        codes.source,
        Some(vosh_prompt::config::CaptureSource::Session)
    );
    assert_eq!(table.template, DESIGN);
    assert!(table.draw);
    assert_eq!(
        toasts(&h),
        [serde_json::json!({"kind": "prompt", "text": PROMPT_X, "applied": true})]
    );
    h.until_last_row(DRAWN).await;
    assert!(
        h.screen()
            .iter()
            .all(|r| !r.contains("[1020/1020hp 800/800mn]")),
        "the new line never shows raw: {:#?}",
        h.screen()
    );
    h.finish(grid).await;
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pattern_you_set_or_took_away_never_switches() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let mut done = Vec::new();
    for (build, kind) in [(Build::New, "gmcp"), (Build::Older, "prompt")] {
        let h = Harness::new(Options::new(build)).await;
        h.count_as_forsaken_lands();
        h.set_prompt(no_capture()).await;
        h.connect().await;
        h.until_last_row("[1020/1020hp 800/800mn 930/930mv]").await;

        // The pattern of the old trigger, set by hand.
        h.type_line(&format!("#prompt {{{OLD_PATTERN}}}")).await;
        h.until_shown("from your prompt with this pattern.").await;
        let typed = h.prompt_table().await;
        assert!(matches!(
            &typed.capture,
            vosh_prompt::CaptureConfig::Regex(r)
                if r.source == Some(vosh_prompt::config::CaptureSource::Typed)
        ));
        h.type_line(&format!("prompt {TYPED_X}")).await;
        h.until(&format!("{build:?} to show prompt x"), |h| {
            h.events("session://game-prompt-seen")
                .iter()
                .any(|e| e["text"] == PROMPT_X)
        })
        .await;
        assert_eq!(h.prompt_table().await, typed, "{build:?}");
        assert!(toasts(&h).is_empty(), "{build:?}");

        // #unprompt leaves nothing to switch.
        h.type_line("#unprompt").await;
        h.until_shown("Vosh stopped reading your prompt.").await;
        h.type_line("prompt").await;
        h.until(&format!("{build:?} to show the prompt again"), |h| {
            h.events("session://game-prompt-seen")
                .iter()
                .filter(|e| e["kind"] == kind && e["text"] == PROMPT_X)
                .count()
                == 2
        })
        .await;
        assert!(h.capture().await.is_none(), "{build:?}");
        assert!(toasts(&h).is_empty(), "{build:?}");
        h.disconnect().await;
        done.push(h);
    }
    // Let the saves the commands marked land before the folders go.
    drop(grid);
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    drop(done);
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_switch_to_a_profile_with_a_moved_capture_switches_it() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.count_as_forsaken_lands();
    // Default reads nothing, and Healer holds the moved pattern.
    h.set_prompt(no_capture()).await;
    let healer = h.profile_file("Healer").await;
    let mut file = crate::profile::file::ProfileConfig::default();
    file.set_prompt(migrated());
    file.save(&healer).expect("Healer's file");
    h.connect().await;
    h.until("Char.Prompt at login", |h| {
        !h.events("session://game-prompt-seen").is_empty()
    })
    .await;
    h.until_last_row("[1020/1020hp 800/800mn 930/930mv]").await;
    assert!(h.capture().await.is_none(), "default saves nothing from it");

    // The latest Char.Prompt switches Healer's pattern as the switch
    // hands it over.
    crate::profile::switch::apply_profile_switch(
        h.app.handle(),
        &h.state,
        &h.state.selected_session(),
        "Healer",
    )
    .await
    .expect("the switch");
    let table = h.prompt_table().await;
    let codes = aabahran(&table.capture);
    assert_eq!(codes.prompt, PROMPT);
    assert_eq!(codes.source, Some(vosh_prompt::config::CaptureSource::Gmcp));
    assert_eq!(table.template, DESIGN);
    assert_eq!(
        toasts(&h),
        [serde_json::json!({"kind": "gmcp", "text": PROMPT, "applied": true})]
    );
    h.type_line("look").await;
    h.until_last_row(DRAWN).await;

    // Back on Default, which still reads nothing, and Healer's file
    // holds the codes.
    crate::profile::switch::apply_profile_switch(
        h.app.handle(),
        &h.state,
        &h.state.selected_session(),
        DEFAULT_PROFILE_NAME,
    )
    .await
    .expect("the switch back");
    assert!(h.capture().await.is_none());
    assert_eq!(toasts(&h).len(), 1);
    let saved = saved_capture(&healer).expect("Healer's file reads");
    assert_eq!(aabahran(&saved).prompt, PROMPT);
    h.finish(grid).await;
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_prompt_vosh_cannot_read_keeps_the_moved_pattern_and_prompt_says_why() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    // A color code runs into %h, so the game prints a digit as a color.
    let h = Harness::new(Options {
        prompt: "<`%h> ".into(),
        ..Options::new(Build::New)
    })
    .await;
    h.count_as_forsaken_lands();
    h.set_prompt(migrated()).await;
    h.connect().await;
    h.until("Char.Prompt at login", |h| {
        !h.events("session://game-prompt-seen").is_empty()
    })
    .await;
    h.until_last_row("<020>").await;
    assert_eq!(h.prompt_table().await, migrated());
    let leftover = &toasts(&h);
    assert!(leftover.is_empty(), "{leftover:?}");
    // The sentence wraps at the 100 columns the screen has.
    h.type_line("#prompt").await;
    h.until("the reason the pattern stayed", |h| {
        h.screen().join(" ").contains(
            "Vosh kept the pattern from your old capture trigger because a color code runs into %h in the prompt the game sent.",
        )
    })
    .await;

    // A prompt Vosh reads switches it.
    h.type_line(&format!("prompt {PROMPT}")).await;
    h.until_last_row(DRAWN).await;
    assert_eq!(aabahran(&h.capture().await).prompt, PROMPT);
    assert_eq!(toasts(&h).len(), 1);
    h.finish(grid).await;
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_moved_pattern_that_fills_a_name_of_its_own_stays_and_draws() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.count_as_forsaken_lands();
    // The old trigger handed its first group to health, which no code
    // fills, and the design reads it.
    let moved = vosh_prompt::PromptConfig {
        draw: true,
        template: "HP=%health".into(),
        capture: vosh_prompt::CaptureConfig::Regex(vosh_prompt::config::RegexCapture {
            lines: vec![r"\[(?<h>\d+)/(?<maxhp>\d+)hp".into()],
            names: [("h".to_string(), "health".to_string())].into(),
            source: Some(vosh_prompt::config::CaptureSource::Migrated),
            ..vosh_prompt::config::RegexCapture::default()
        }),
        ..vosh_prompt::PromptConfig::default()
    };
    h.set_prompt(moved.clone()).await;
    h.connect().await;

    // Login leaves the pattern in place, so the design keeps its value.
    h.until("Char.Prompt at login", |h| {
        !h.events("session://game-prompt-seen").is_empty()
    })
    .await;
    h.until_last_row("HP=1020").await;
    assert_eq!(h.prompt_table().await, moved);
    let leftover = &toasts(&h);
    assert!(leftover.is_empty(), "{leftover:?}");
    h.type_line("#prompt").await;
    h.until("the reason the pattern stayed", |h| {
        h.screen().join(" ").contains(
            "Vosh kept the pattern from your old capture trigger because it fills a value named health, and no prompt code fills that name.",
        )
    })
    .await;

    // A new prompt in the game changes nothing, and the pattern draws
    // over the new line.
    h.type_line(&format!("prompt {TYPED_NEW}")).await;
    h.until_shown(&format!("Prompt set to {TYPED_NEW}")).await;
    h.until_last_row("HP=1020").await;
    assert!(h
        .events("session://game-prompt-seen")
        .iter()
        .any(|e| e["text"] == PROMPT_NEW));
    assert_eq!(h.prompt_table().await, moved);
    let leftover = &toasts(&h);
    assert!(leftover.is_empty(), "{leftover:?}");
    assert!(
        h.screen().iter().all(|r| !r.contains("(100 hp)")),
        "the new line never shows raw: {:#?}",
        h.screen()
    );
    h.finish(grid).await;
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reconnect_keeps_the_moved_pattern_until_the_game_sends_your_prompt() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options {
        reconnect: true,
        ..Options::new(Build::New)
    })
    .await;
    h.count_as_forsaken_lands();
    h.set_prompt(migrated()).await;
    h.connect().await;
    // No Char.Prompt comes, so the pattern stays and still draws.
    h.until_shown("Reconnecting.").await;
    h.until_last_row(DRAWN).await;
    assert_eq!(h.prompt_table().await, migrated());
    let leftover = &h.events("session://game-prompt-seen");
    assert!(leftover.is_empty(), "{leftover:?}");

    // prompt in the game sends Char.Prompt, which switches it.
    h.type_line("prompt").await;
    h.until_shown(&format!("Current prompt: {PROMPT}")).await;
    h.until("the switch", |h| !toasts(h).is_empty()).await;
    let codes = aabahran(&h.capture().await);
    assert_eq!(codes.prompt, PROMPT);
    assert_eq!(codes.source, Some(vosh_prompt::config::CaptureSource::Gmcp));
    h.until_last_row(DRAWN).await;
    h.finish(grid).await;
}

/// The affect fulls `pairs`, as the store keeps them.
fn fulls(pairs: &[(&str, i64)]) -> crate::affects::full::FullMap {
    pairs.iter().map(|(k, v)| ((*k).to_string(), *v)).collect()
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn affect_fulls_follow_a_cast_and_come_back_at_the_next_login() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    let file = crate::disk::paths::affect_full_path(h.dir.path());
    h.state.affect_file.set_path(file.clone());
    h.connect().await;
    // Tester logs in with bless at 6 and armor at 44, both first seen.
    h.until("the login fulls", |h| {
        h.fulls_of(h.first) == fulls(&[("armor", 44), ("bless", 6)])
    })
    .await;
    let passes = |h: &Harness| {
        h.screen()
            .iter()
            .filter(|r| r.contains("The hour passes."))
            .count()
    };
    // Two ticks drain both and change no full.
    h.type_line("tick").await;
    h.type_line("tick").await;
    h.until("two ticks", |h| passes(h) == 2).await;
    assert_eq!(h.fulls_of(h.first), fulls(&[("armor", 44), ("bless", 6)]));
    // A recast of armor for more hours starts its full over.
    h.type_line("cast 48 armor").await;
    h.until("the recast", |h| {
        h.fulls_of(h.first) == fulls(&[("armor", 48), ("bless", 6)])
    })
    .await;
    h.type_line("tick").await;
    h.until("a third tick", |h| passes(h) == 3).await;
    // The windows heard each change from the first session, the last
    // one the fulls now.
    let heard = h.events_of(h.first, crate::app::events::AFFECT_FULL_CHANGED);
    assert_eq!(
        heard,
        [
            serde_json::json!({ "data": { "armor": 44, "bless": 6 } }),
            serde_json::json!({ "data": { "armor": 48, "bless": 6 } }),
        ]
    );

    // Log out: the fulls are written for the next login and cleared.
    h.disconnect().await;
    let text = std::fs::read_to_string(&file).expect("the fulls are written");
    let key = format!("127.0.0.1:{} tester", h.port);
    let saved: toml::Table = text.parse().expect("the file reads");
    assert_eq!(
        saved["characters"][key.as_str()]["armor"].as_integer(),
        Some(48)
    );
    assert_eq!(
        saved["characters"][key.as_str()]["bless"].as_integer(),
        Some(6)
    );
    assert!(h.fulls_of(h.first).is_empty());

    // Log back in with armor at 47 and bless at 3, as the game kept them.
    h.fake.lock().expect("the options").affects = vec![
        vosh_prompt::testkit::Affect::spell("bless", 3),
        vosh_prompt::testkit::Affect::spell("armor", 47),
    ];
    h.connect().await;
    h.until("the same fulls", |h| {
        h.fulls_of(h.first) == fulls(&[("armor", 48), ("bless", 6)])
    })
    .await;
    h.finish(grid).await;
}

/// The fulls saved for Tester on the fake game, from the file.
fn saved_fulls(h: &Harness) -> crate::affects::full::FullMap {
    let file = crate::disk::paths::affect_full_path(h.dir.path());
    let text = std::fs::read_to_string(file).expect("the fulls are written");
    let table: toml::Table = text.parse().expect("the file reads");
    let key = format!("127.0.0.1:{} tester", h.port);
    table["characters"]
        .get(key.as_str())
        .and_then(toml::Value::as_table)
        .map(|t| {
            t.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_integer()?)))
                .collect()
        })
        .unwrap_or_default()
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn affect_fulls_outlast_quitting_to_the_menu_and_out_of_the_game() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.state
        .affect_file
        .set_path(crate::disk::paths::affect_full_path(h.dir.path()));
    h.connect().await;
    h.until("the login fulls", |h| {
        h.fulls_of(h.first) == fulls(&[("armor", 44), ("bless", 6)])
    })
    .await;
    h.type_line("cast 48 armor").await;
    h.type_line("tick").await;
    h.type_line("tick").await;
    h.until_shown("You cast armor.").await;
    h.until("two ticks", |h| {
        h.screen()
            .iter()
            .filter(|r| r.contains("The hour passes."))
            .count()
            == 2
    })
    .await;
    let want = fulls(&[("armor", 48), ("bless", 6)]);
    assert_eq!(h.fulls_of(h.first), want);

    // quit menu: the game takes each affect off in turn, and the pane
    // empties with it, on the same link.
    h.type_line("quit menu").await;
    h.until_shown("return to your account menu").await;
    h.until("the pane empties", |h| h.fulls_of(h.first).is_empty())
        .await;
    // Play Tester again: the game sends the affects the pfile kept, with
    // armor at 46 and bless at 4, and each keeps its full.
    h.type_line("").await;
    h.until("the fulls come back", |h| h.fulls_of(h.first) == want)
        .await;

    // quit: the same lists, then the game closes the link. Vosh writes
    // the fulls you quit with, not the empty list the game ended on.
    h.type_line("quit").await;
    h.until("the game closes the link", |h| {
        h.events("session://state")
            .iter()
            .any(|e| e["kind"] == "disconnected")
    })
    .await;
    h.until("the fulls are saved", |h| {
        h.fulls_of(h.first).is_empty() && saved_fulls(h) == want
    })
    .await;
    h.disconnect().await;

    // Log back in with the affects as the game kept them.
    h.fake.lock().expect("the options").affects = vec![
        vosh_prompt::testkit::Affect::spell("bless", 4),
        vosh_prompt::testkit::Affect::spell("armor", 46),
    ];
    h.connect().await;
    h.until("the same fulls", |h| h.fulls_of(h.first) == want)
        .await;
    h.finish(grid).await;
}

/// The text each pinned prompt the session sent shows, oldest first.
fn pins(h: &Harness) -> Vec<String> {
    h.events("session://output")
        .into_iter()
        .filter_map(|out| {
            out["pin"]
                .as_str()
                .map(|pin| vosh_prompt::testkit::shown(&base64_decode(pin)))
        })
        .collect()
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn prompt_default_draws_the_default_design_on_the_pinned_band_at_once() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.set_prompt(vosh_prompt::PromptConfig {
        show: vosh_prompt::PromptShow::Pinned,
        ..codes(PROMPT)
    })
    .await;
    h.connect().await;
    h.until("your design on the band", |h| {
        pins(h).last().map(String::as_str) == Some("<1020>")
    })
    .await;

    // No prompt comes from the game, and the band shows the default at
    // once.
    h.type_line("#prompt default").await;
    h.until_shown("Your design is now Vosh's default.").await;
    h.until("the default design on the band", |h| {
        pins(h)
            .last()
            .is_some_and(|pin| pin.starts_with("1020/1020hp 800/800mn 930/930mv"))
    })
    .await;
    // Out of a fight the band is the vitals row alone, as the gallery
    // mockup draws it.
    let band = pins(&h).pop().expect("a band");
    assert_eq!(band, "1020/1020hp 800/800mn 930/930mv  [S]  1,250g ");
    // In a fight the tank row comes first. Solo you are the tank.
    h.type_line("fight").await;
    h.until("the tank row on the band", |h| {
        pins(h)
            .last()
            .is_some_and(|pin| pin.starts_with("Tester: "))
    })
    .await;
    let band = pins(&h).pop().expect("a band");
    assert_eq!(
        band,
        "Tester: ████████░░\r\n765/1020hp 800/800mn 930/930mv  [S]  1,250g "
    );
    let table = h.prompt_table().await;
    assert_eq!(table.template, vosh_prompt::DEFAULT_DESIGN);
    assert_eq!(table.previous_templates, ["<%hp>"]);
    assert_eq!(table.show, vosh_prompt::PromptShow::Pinned);
    h.finish(grid).await;
}

/// Same as the game for `prompt`, for a mortal in your own body.
fn same_as_the_game(prompt: &str) -> String {
    vosh_prompt::card::presets::game(prompt, "", vosh_prompt::aabahran::Who::default())
        .expect("the codes compile")
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_no_design_of_your_own_vosh_draws_your_prompt_as_the_game_does() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    // A fresh profile whose capture follows the game, on another setting
    // than the one the game holds, with drawing off.
    h.set_prompt(vosh_prompt::PromptConfig {
        capture: codes("<%hhp> ").capture,
        ..vosh_prompt::PromptConfig::fresh()
    })
    .await;
    h.connect().await;

    // Login gives Vosh your PROMPT, and the design follows it. Drawing
    // stays off until you turn it on.
    h.until_last_row("[1020/1020hp 800/800mn 930/930mv]").await;
    let table = h.prompt_table().await;
    assert!(table.mirror);
    assert!(!table.draw);
    assert_eq!(table.template, same_as_the_game(PROMPT));

    // Turned on, Vosh draws your prompt exactly as the game does.
    h.type_line("#prompt draw on").await;
    h.until_shown("Drawing is on.").await;
    h.type_line("look").await;
    h.until_last_row("[1020/1020hp 800/800mn 930/930mv]").await;
    assert!(h.state.selected_session().connection.lock().prompt.draws());

    // Change it in the game, and the drawn prompt follows.
    h.type_line(&format!("prompt {TYPED_X}")).await;
    h.until_shown(&format!("Prompt set to {TYPED_X}")).await;
    h.until_last_row("<1020/1020hp 800/800mn>").await;
    let table = h.prompt_table().await;
    assert!(table.mirror);
    assert!(table.draw);
    assert_eq!(table.template, same_as_the_game(PROMPT_X));
    let leftover = &table.previous_templates;
    assert!(leftover.is_empty(), "{leftover:?}");
    h.finish(grid).await;
}

/// The session sends each GMCP package on the event
/// `fixtures/ipc/gmcp-events.json` names for it, as `{session, data}`.
/// `onGmcpPackage` on the page builds its listen from the same file in
/// src/ipc/session.test.ts and hands its listener the data, so a change
/// to the encoding on one side alone fails one of the two.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_gmcp_package_goes_out_on_the_event_the_page_hears() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let cases: Json = serde_json::from_str(include_str!("../../../fixtures/ipc/gmcp-events.json"))
        .expect("cases");
    let events: Vec<String> = cases["cases"]
        .as_array()
        .expect("a list of cases")
        .iter()
        .map(|case| case["event"].as_str().expect("an event").to_string())
        .collect();
    let h = Harness::new(Options::new(Build::New)).await;
    let heard = Arc::new(StdMutex::new(std::collections::BTreeMap::new()));
    for event in &events {
        let (heard, name) = (heard.clone(), event.clone());
        h.app.listen_any(event.clone(), move |e| {
            let payload: Json = serde_json::from_str(e.payload()).expect("a JSON payload");
            heard
                .lock()
                .expect("the events")
                .insert(name.clone(), payload);
        });
    }
    h.connect().await;
    h.until("every GMCP event at login", |_| {
        heard.lock().expect("the events").len() == events.len()
    })
    .await;
    let vitals = heard.lock().expect("the events")["session://gmcp/Char-Vitals"].clone();
    assert_eq!(vitals["session"], serde_json::json!(h.first), "{vitals}");
    assert!(vitals["data"].is_object(), "{vitals}");
    h.finish(grid).await;
}

// An alias set to run Lua runs its body when you type it, with the words
// after its name in captures, and the game hears what the body sends. It
// used to swallow the line, so nothing went out and no Lua ran. The guard
// keeps other tests off the shared native grid.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lua_alias_you_type_runs_its_body_and_the_game_hears_it() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.set_prompt(codes(PROMPT)).await;
    h.connect().await;
    h.until_last_row("<1020>").await;
    h.state.selected_profile().await.aliases.set(
        vosh_automation::alias::Alias::new("peer", "ignored")
            .with_script("mud.echo('You peer ' .. captures[1] .. '.')\nmud.send(captures[1])"),
    );

    // The login already showed the room, so count rooms to see the
    // game answer the look the body sends. The game answers in order, so
    // a raw `peer look` would show its Huh? before that room.
    let rooms = |h: &Harness| {
        h.screen()
            .iter()
            .filter(|r| r.as_str() == "[Exits: south]")
            .count()
    };
    let before = rooms(&h);
    h.type_line("peer look").await;
    h.until_shown("You peer look.").await;
    h.until("the game's answer to look", |h| rooms(h) > before)
        .await;
    let screen = h.screen();
    assert!(
        !screen.iter().any(|row| row.contains("Huh?")),
        "{screen:#?}"
    );
    h.finish(grid).await;
}

// A trigger's Lua that hands a line to mud.input runs it as if you typed
// it, so a Lua alias the line names runs its body and the game hears what
// the body sends. The guard keeps other tests off the shared native grid.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lua_alias_that_mud_input_names_runs_its_body() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.set_prompt(codes(PROMPT)).await;
    h.connect().await;
    h.until_last_row("<1020>").await;
    {
        let mut p = h.state.selected_profile().await;
        p.aliases.set(
            vosh_automation::alias::Alias::new("peer", "ignored")
                .with_script("mud.send(captures[1])"),
        );
        p.triggers
            .set(vosh_automation::trigger::Trigger::new(
                "exits",
                r"^\[Exits: south\]$",
                vosh_automation::trigger::TriggerAction::Script {
                    body: "mud.input('peer afk')".into(),
                },
            ))
            .expect("the trigger compiles");
    }

    h.type_line("look").await;
    h.until_shown("You are now in AFK mode.").await;
    let screen = h.screen();
    assert!(
        !screen.iter().any(|row| row.contains("Huh?")),
        "{screen:#?}"
    );
    h.finish(grid).await;
}

// Lua you type with #lua does all it asks, the way the Lua a trigger
// runs does. Its timers fire, a timer it cancels never does, its
// mud.input lines run, and the values it gives your prompt reach the
// windows. Each text the Lua prints is split in the line you type, so
// only the Lua itself shows it whole. The guard keeps other tests off
// the shared native grid.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lua_you_type_starts_timers_runs_input_and_sets_prompt_values() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.set_prompt(codes(PROMPT)).await;
    h.connect().await;
    h.until_last_row("<1020>").await;

    h.type_line("#lua mud.set_prompt_var('lua_mark', 'o' .. 'n')")
        .await;
    h.until("the value Lua gave your prompt", |h| {
        h.events("session://prompt-vars")
            .iter()
            .any(|vars| vars["data"]["lua_mark"] == "on")
    })
    .await;

    h.type_line("#lua mud.timer(0, function() mud.echo('timer' .. ' fired') end)")
        .await;
    h.until_shown("timer fired").await;

    // A timer cancelled in the chunk that starts it never fires, and
    // nor does one a later line cancels. The later timer is due after
    // both would have been.
    h.type_line(
        "#lua soon = mud.timer(0, function() mud.echo('same chunk' .. ' timer fired') end) \
         mud.cancel_timer(soon)",
    )
    .await;
    h.type_line("#lua slow = mud.timer(2, function() mud.echo('cancelled' .. ' timer fired') end)")
        .await;
    h.type_line("#lua mud.cancel_timer(slow)").await;
    h.type_line("#lua mud.timer(2.5, function() mud.echo('later' .. ' timer fired') end)")
        .await;
    h.until_shown("later timer fired").await;
    assert!(
        !h.screen().iter().any(|row| {
            row.contains("same chunk timer fired") || row.contains("cancelled timer fired")
        }),
        "{:#?}",
        h.screen()
    );

    // A line mud.input runs goes through the input pipeline, slash
    // commands and the game alike. The game answers compact with a line
    // nothing before it shows, so only a line that reached the game
    // brings it.
    h.type_line("#lua mud.input('#echo ' .. 'input' .. ' ran')")
        .await;
    h.until_shown("input ran").await;
    h.type_line("#lua mud.input('comp' .. 'act')").await;
    h.until_shown("Compact mode set.").await;

    // Lua that keeps asking mud.input to run it again stops at the depth
    // an alias may go.
    h.type_line("#lua function again() mud.input('#lua again()') end again()")
        .await;
    h.until_shown("[mud.input recursion limit hit (16)]").await;

    // Lua whose lines each ask for 100 more runs 100 lines in all.
    h.type_line("#lua function fan() for i = 1, 100 do mud.input('#lua fan()') end end fan()")
        .await;
    h.until_shown("[lua] Vosh ran 100 lines from mud.input and dropped the rest.")
        .await;
    h.finish(grid).await;
}

// A plugin you turned on does all its entry script asks as it loads at
// launch, the way the Lua you type does. Here it runs a line through
// mud.input, gives your prompt a value and starts a timer, which fires
// once the game connects. Of the slash commands it runs only #echo, so
// it neither makes an alias nor loads itself again for good. The guard
// keeps other tests off the shared native grid.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lua_a_plugin_runs_as_it_loads_starts_timers_and_runs_input() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    let plugins = h.dir.path().join("plugins");
    let plugin = plugins.join("on_load");
    std::fs::create_dir_all(&plugin).expect("the plugin folder");
    std::fs::write(
        plugin.join("manifest.toml"),
        "[plugin]\nname = \"on_load\"\n",
    )
    .expect("the manifest");
    std::fs::write(
        plugin.join("main.lua"),
        "mud.timer(0, function() mud.echo('the plugin timer fired') end)\n\
         mud.input('#echo the plugin ran ' .. 'mud.input')\n\
         mud.input('#alias plugged kick')\n\
         for i = 1, 50 do mud.input('#script reload') end\n\
         mud.set_prompt_var('plugin_mark', 'on')\n",
    )
    .expect("the entry script");
    h.state.selected_profile().await.plugins.enabled = vec!["on_load".into()];

    crate::app::plugins::load_enabled_plugins(h.app.handle(), &h.state.selected_session(), plugins)
        .await;
    assert!(
        h.state
            .selected_profile()
            .await
            .aliases
            .get("plugged")
            .is_none(),
        "a plugin made an alias you keep"
    );
    h.until("the value the plugin gave your prompt", |h| {
        h.events("session://prompt-vars")
            .iter()
            .any(|vars| vars["data"]["plugin_mark"] == "on")
    })
    .await;

    h.connect().await;
    h.until_shown("the plugin timer fired").await;
    h.until_shown("the plugin ran mud.input").await;
    h.until_shown("[lua] Vosh never runs #alias for a plugin.")
        .await;
    h.until_shown("[lua] Vosh never runs #script for a plugin.")
        .await;
    h.finish(grid).await;
}

// A Lua GMCP handler you make mid session runs at once on the last
// packet of its package, here the Char.Status of the login, and the
// packets end with the connection. The guard keeps other tests off the
// shared native grid.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lua_a_new_gmcp_handler_hears_the_last_packet_at_once() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.connect().await;
    h.until_shown("Welcome to the fake Aabahran, Tester.").await;
    h.type_line(
        "#lua mud.on_gmcp('Char.Status', function(d) mud.echo(d.name .. ' is level ' .. d.level) end)",
    )
    .await;
    h.until_shown("Tester is level 50").await;
    h.disconnect().await;
    let after = h.state.selected_session().connection.lock().script.eval(
        "mud.on_gmcp('Char.Status', function() mud.echo('stale') end)",
        "=#lua",
    );
    let leftover = &after.actions;
    assert!(leftover.is_empty(), "{leftover:?}");
    h.finish(grid).await;
}

// A profile switch turns on the plugins the next profile turns on and
// turns off the ones it does not, while you play. A plugin both turn on
// keeps running, and one that turns on sends to the game and hears the
// last Char.Status at once. The guard keeps other tests off the shared
// native grid.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lua_a_profile_switch_turns_its_plugins_on_and_the_others_off() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.state
        .app_data
        .set(h.dir.path().to_path_buf())
        .expect("the app data folder");
    let plugins = h.dir.path().join("plugins");
    for (name, body) in [
        ("everywhere", "mud.echo('everywhere loaded')"),
        (
            "healer_only",
            "mud.alias('hl', 'cast heal')\n\
             mud.send('afk')\n\
             mud.on_gmcp('Char.Status', function(d) mud.echo('healer_only sees ' .. d.name) end)",
        ),
    ] {
        let plugin = plugins.join(name);
        std::fs::create_dir_all(&plugin).expect("the plugin folder");
        std::fs::write(
            plugin.join("manifest.toml"),
            format!("[plugin]\nname = \"{name}\"\n"),
        )
        .expect("the manifest");
        std::fs::write(plugin.join("main.lua"), body).expect("the entry script");
    }
    let mut healer = crate::profile::file::ProfileConfig::default();
    healer.plugins.enabled = vec!["everywhere".into(), "healer_only".into()];
    healer
        .save(&h.profile_file("Healer").await)
        .expect("Healer's file");
    h.state.selected_profile().await.plugins.enabled = vec!["everywhere".into()];
    crate::app::plugins::load_enabled_plugins(h.app.handle(), &h.state.selected_session(), plugins)
        .await;
    h.connect().await;
    h.until_shown("Welcome to the fake Aabahran, Tester.").await;

    crate::profile::switch::apply_profile_switch(
        h.app.handle(),
        &h.state,
        &h.state.selected_session(),
        "Healer",
    )
    .await
    .expect("the switch");
    h.until_shown("healer_only sees Tester").await;
    // What it sends as it loads reaches the game.
    h.until_shown("You are now in AFK mode.").await;
    let shown = |h: &Harness, text: &str| h.screen().iter().filter(|r| r.contains(text)).count();
    assert_eq!(shown(&h, "everywhere loaded"), 1, "it kept running");
    {
        let session = h.state.selected_session();
        let c = session.connection.lock();
        assert_eq!(c.script.loaded_plugins(), ["everywhere", "healer_only"]);
        assert_eq!(c.plugin_aliases.list().len(), 1);
    }

    crate::profile::switch::apply_profile_switch(
        h.app.handle(),
        &h.state,
        &h.state.selected_session(),
        DEFAULT_PROFILE_NAME,
    )
    .await
    .expect("the switch back");
    {
        let session = h.state.selected_session();
        let c = session.connection.lock();
        assert_eq!(c.script.loaded_plugins(), ["everywhere"]);
        let leftover = &c.plugin_aliases.list();
        assert!(leftover.is_empty(), "{leftover:?}");
    }
    h.finish(grid).await;
}

// What a plugin prints as it loads at launch waits for a terminal, then
// shows once you connect: its print and its error as [lua] lines, and
// the stop of a plugin that runs away. A plugin the profile lists twice
// loads once. The guard keeps other tests off the shared native grid.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lua_a_plugin_load_prints_its_lines_once_you_connect() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    let plugins = h.dir.path().join("plugins");
    for (name, body) in [
        ("noisy", "print('noisy is here')\nmud.ech('typo')\n"),
        ("spin", "while true do end\n"),
    ] {
        let plugin = plugins.join(name);
        std::fs::create_dir_all(&plugin).expect("the plugin folder");
        std::fs::write(
            plugin.join("manifest.toml"),
            format!("[plugin]\nname = \"{name}\"\n"),
        )
        .expect("the manifest");
        std::fs::write(plugin.join("main.lua"), body).expect("the entry script");
    }
    h.state.selected_profile().await.plugins.enabled =
        vec!["noisy".into(), "spin".into(), "noisy".into()];
    crate::app::plugins::load_enabled_plugins(h.app.handle(), &h.state.selected_session(), plugins)
        .await;
    assert!(h
        .state
        .selected_session()
        .connection
        .lock()
        .script
        .is_stopped(&vosh_script::Owner::Plugin("spin".into())));

    h.connect().await;
    h.until_shown("[lua] noisy is here").await;
    h.until_shown("[lua] noisy/main.lua:2: attempt to call a nil value (field 'ech')")
        .await;
    h.until_shown("[lua] Vosh stopped spin at main.lua line 1 after 100 ms.")
        .await;
    h.until_shown(
        "[lua] spin stays off until you save it under Scripts in Settings or restart Vosh.",
    )
    .await;
    let noisy = h
        .screen()
        .iter()
        .filter(|row| row.contains("noisy is here"))
        .count();
    assert_eq!(noisy, 1, "{:#?}", h.screen());
    h.finish(grid).await;
}

// The command a Settings timer runs does all its Lua asks, as it does
// when you type it. Here the Lua starts a timer of its own and runs a
// line through mud.input. The guard keeps other tests off the shared
// native grid.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lua_a_settings_timer_runs_starts_timers_and_runs_input() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    {
        let mut p = h.state.selected_profile().await;
        for (id, command) in [
            (
                1,
                "#lua mud.timer(0, function() mud.echo('the timer Lua started fired') end)",
            ),
            (2, "#lua mud.input('#echo the timer ran mud.input')"),
        ] {
            p.timers.push(crate::profile::live::Timer {
                id,
                name: String::new(),
                interval_secs: 1,
                command: command.into(),
                enabled: true,
                group: None,
            });
        }
    }
    h.connect().await;
    h.until_shown("the timer Lua started fired").await;
    h.until_shown("the timer ran mud.input").await;
    h.finish(grid).await;
}

// Lua that changes an alias saves your profile, on whatever path it
// runs, since every path applies its result the same way. Here a
// trigger runs the Lua on a line the game sends at login, so no line
// you type marks the profile for saving. The guard keeps other tests
// off the shared native grid.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lua_that_changes_an_alias_saves_your_profile() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.state
        .selected_profile()
        .await
        .triggers
        .set(vosh_automation::trigger::Trigger::new(
            "learn",
            "a clerk nods at you",
            vosh_automation::trigger::TriggerAction::Script {
                body: "mud.alias('k', 'kick')".into(),
            },
        ))
        .expect("the trigger compiles");
    h.connect().await;

    let file = h.profile_file(DEFAULT_PROFILE_NAME).await;
    h.until("the alias the Lua made, saved", |_| {
        crate::profile::file::ProfileConfig::load(&file)
            .is_ok_and(|config| config.aliases.iter().any(|a| a.name == "k"))
    })
    .await;
    h.finish(grid).await;
}

/// The app on the fake game with your design `<1020>` on the band pinned
/// above the command line, and no line typed yet.
async fn pinned_band() -> Harness {
    let h = Harness::new(Options::new(Build::New)).await;
    h.set_prompt(vosh_prompt::PromptConfig {
        show: vosh_prompt::PromptShow::Pinned,
        ..codes(PROMPT)
    })
    .await;
    h.connect().await;
    h.until("your design on the band", |h| {
        pins(h).last().map(String::as_str) == Some("<1020>")
    })
    .await;
    h
}

/// Wait until the target display names goblin and the band shows Vosh's
/// default design. The fake game sends nothing unless you type, so no
/// prompt from the game draws the band again.
async fn until_goblin_and_the_default_band(h: &Harness) {
    h.until("goblin on the target display", |h| {
        h.events("session://target")
            .iter()
            .any(|target| target["name"] == "goblin")
    })
    .await;
    h.until("the default design on the band", |h| {
        pins(h)
            .last()
            .is_some_and(|pin| pin.starts_with("1020/1020hp 800/800mn 930/930mv"))
    })
    .await;
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_timer_line_moves_the_target_display_and_repaints_your_prompt() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = pinned_band().await;

    // Two Settings timers run the lines you would type. The tick command
    // runs its line the same way.
    {
        let mut p = h.state.selected_profile().await;
        for (id, command) in [(1, "tar goblin"), (2, "#prompt default")] {
            p.timers.push(crate::profile::live::Timer {
                id,
                name: String::new(),
                interval_secs: 1,
                command: command.into(),
                enabled: true,
                group: None,
            });
        }
    }
    until_goblin_and_the_default_band(&h).await;
    h.finish(grid).await;
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_line_from_mud_input_moves_the_target_display_and_repaints_your_prompt() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = pinned_band().await;

    // The game answers Huh? and its prompt. The trigger starts a Lua
    // timer, so its lines run after that prompt drew the band.
    h.state
        .selected_profile()
        .await
        .triggers
        .set(vosh_automation::trigger::Trigger::new(
            "huh",
            r"^Huh\?",
            vosh_automation::trigger::TriggerAction::Script {
                body: r##"mud.timer(0.3, function()
                    mud.input("tar goblin")
                    mud.input("#prompt default")
                end)"##
                    .into(),
            },
        ))
        .expect("the trigger compiles");
    h.type_line("xyzzy").await;
    h.until_shown("Huh?").await;
    until_goblin_and_the_default_band(&h).await;
    h.finish(grid).await;
}

// Bug 9. The game closing the link ends the session, and a line you type
// after it says [not connected], as it does after you disconnect.
// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_line_typed_after_the_game_closes_the_link_says_not_connected() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.connect().await;
    h.until_shown("Welcome to the fake Aabahran").await;
    h.type_line("quit").await;
    h.until_shown("Alas, all good things must come to an end.")
        .await;
    h.until("the game closes the link", |h| {
        h.events("session://state")
            .iter()
            .any(|e| e["kind"] == "disconnected")
    })
    .await;
    // An empty slot counts too, so a session that clears its own slot
    // as it ends still passes this wait.
    h.until("the session ends", |h| {
        h.state.selected_session().slot.try_lock().is_ok_and(|s| {
            s.as_ref()
                .is_none_or(crate::session::SessionHandle::has_ended)
        })
    })
    .await;

    h.echo("look");
    let sent = crate::ipc::session::session_send_input(
        h.app.handle().clone(),
        h.app.state(),
        "look".to_string(),
        None,
    )
    .await;
    assert_eq!(sent, Ok(()), "look finds no session to send to");
    h.until_shown("[not connected]").await;
    assert!(
        h.state.selected_session().slot.lock().await.is_none(),
        "the ended session leaves the app state"
    );
    h.finish(grid).await;
}

/// Your design with the sky, a value only the World.Time packet of the
/// login carries.
fn codes_and_sky() -> vosh_prompt::PromptConfig {
    vosh_prompt::PromptConfig {
        template: "<%hp> %sky".into(),
        ..codes(PROMPT)
    }
}

/// How long the count of a `session://tick` report has run.
fn tick_elapsed(tick: &Json) -> Duration {
    Duration::from_millis(
        tick["elapsed_ms"]
            .as_u64()
            .expect("the time since the tick"),
    )
}

/// Wait until the tick has counted a second, so a count that started
/// again would show, and return the newest report.
async fn a_second_of_the_tick(h: &Harness) -> Json {
    h.until("a second of the tick", |h| {
        h.events(crate::app::events::TICK)
            .last()
            .is_some_and(|tick| tick_elapsed(tick) >= Duration::from_secs(1))
    })
    .await;
    h.events(crate::app::events::TICK)
        .pop()
        .expect("a tick report")
}

/// Target goblin, send kill with gg, and give your prompt the value
/// `lua_mark` through Lua, which the windows hear.
async fn target_goblin_and_mark_your_prompt(h: &Harness) {
    h.type_line("tar goblin").await;
    h.type_line("#qkey gg kill").await;
    h.type_line("#lua mud.set_prompt_var('lua_mark','on')")
        .await;
    h.until("the value Lua gave your prompt", |h| {
        h.events("session://prompt-vars")
            .iter()
            .any(|vars| vars["data"]["lua_mark"] == "on")
    })
    .await;
}

// A profile switch while you play keeps what belongs to the connection.
// Your target and quick keys stay, the packets of the login stay, so the
// sky still draws, and the tick counts on and stays on, although the next
// profile saved it off. Only the values Lua gave your prompt drop. The
// guard keeps other tests off the shared native grid.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_profile_switch_while_connected_keeps_your_target_prompt_and_tick() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.set_prompt(codes_and_sky()).await;
    let mut healer = crate::profile::file::ProfileConfig::default();
    healer.set_prompt(codes_and_sky());
    healer.tick.enabled = false;
    healer
        .save(&h.profile_file("Healer").await)
        .expect("Healer's file");
    h.connect().await;
    h.until_last_row("<1020> cloudy").await;
    target_goblin_and_mark_your_prompt(&h).await;
    let tick = a_second_of_the_tick(&h).await;

    let targets = h.events("session://target").len();
    let vars = h.events("session://prompt-vars").len();
    let ticks = h.events(crate::app::events::TICK).len();
    crate::profile::switch::apply_profile_switch(
        h.app.handle(),
        &h.state,
        &h.state.selected_session(),
        "Healer",
    )
    .await
    .expect("the switch");

    // Your target and quick keys stay, and the target display hears
    // nothing that clears them.
    h.type_line("tar").await;
    h.until_shown("current target: goblin").await;
    h.type_line("#qkeys").await;
    h.until_shown("gg  ->  kill").await;
    let heard = &h.events("session://target")[targets..];
    assert!(heard.iter().all(|t| t["name"] == "goblin"), "{heard:?}");

    // The next pulse draws your prompt, and the sky comes from the
    // packet of the login, since the game sends no World.Time after it.
    let rooms = |h: &Harness| {
        h.screen()
            .iter()
            .filter(|r| r.as_str() == "[Exits: south]")
            .count()
    };
    let before = rooms(&h);
    h.type_line("look").await;
    h.until("the room", |h| rooms(h) > before).await;
    h.until_last_row("<1020> cloudy").await;

    // The value Lua gave your prompt dropped with the switch.
    h.until("the prompt values after the switch", |h| {
        h.events("session://prompt-vars").len() > vars
    })
    .await;
    let values = &h.events("session://prompt-vars")[vars]["data"];
    assert!(values.get("lua_mark").is_none(), "{values}");

    // The tick counts on from where it was, on and synced as before.
    h.until("two tick reports after the switch", |h| {
        h.events(crate::app::events::TICK).len() >= ticks + 2
    })
    .await;
    for after in &h.events(crate::app::events::TICK)[ticks..] {
        assert_eq!(after["enabled"], true, "{after}");
        assert_eq!(after["synced"], tick["synced"], "{after}");
        assert!(
            tick_elapsed(after) >= tick_elapsed(&tick),
            "{after} after {tick}"
        );
    }
    h.finish(grid).await;
}

// A disconnect ends what belongs to the connection. Your target, the room
// list and the values Lua gave your prompt clear, and your quick keys
// stay. A target you set while offline carries into the next connection,
// and the tick counts from the connect, unsynced. The guard keeps other
// tests off the shared native grid.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_disconnect_clears_your_target_the_room_list_and_both_prompt_feeds() {
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.set_prompt(codes(PROMPT)).await;
    h.connect().await;
    h.until_last_row("<1020>").await;
    target_goblin_and_mark_your_prompt(&h).await;
    // The fake game sends no Room.Chars, so the list goes in place the
    // way the session takes one.
    crate::input::target::set_room_chars(
        &mut h.state.selected_session().connection.lock(),
        crate::input::target::read_room_chars(&[
            serde_json::json!({"name": "a goblin", "npc": true}),
        ]),
    );
    h.type_line("tar").await;
    h.until_shown("1 char(s) in room:").await;
    let keys = h
        .events("session://target")
        .pop()
        .expect("the target display")["quick_keys"]
        .clone();
    assert!(
        keys.as_array()
            .expect("the quick keys")
            .contains(&serde_json::json!({"name": "gg", "verb": "kill"})),
        "{keys}"
    );
    a_second_of_the_tick(&h).await;

    h.disconnect().await;
    h.until("the target display to clear", |h| {
        h.events("session://target")
            .last()
            .is_some_and(|t| t["name"].is_null())
    })
    .await;
    let cleared = h.events("session://target").pop().expect("the clear");
    assert!(cleared["room_idx"].is_null(), "{cleared}");
    assert_eq!(cleared["quick_keys"], keys);

    // Offline, a target still takes.
    h.type_line("tar orc").await;
    h.until_shown("target: orc (not in room)").await;
    let vars = h.events("session://prompt-vars").len();
    let ticks = h.events(crate::app::events::TICK).len();
    let connected = std::time::Instant::now();
    h.connect().await;

    // The tick starts again with the connection, unsynced.
    h.until("the tick of the new connection", |h| {
        h.events(crate::app::events::TICK).len() > ticks
    })
    .await;
    let since = connected.elapsed();
    let first = &h.events(crate::app::events::TICK)[ticks];
    assert_eq!(first["enabled"], true, "{first}");
    assert_eq!(first["synced"], false, "{first}");
    assert!(tick_elapsed(first) <= since, "{first} within {since:?}");

    // Your prompt reads without the value Lua gave it before.
    h.until_last_row("<1020>").await;
    h.until("the prompt values of the new connection", |h| {
        h.events("session://prompt-vars").len() > vars
    })
    .await;
    let values = &h.events("session://prompt-vars")[vars]["data"];
    assert!(values.get("lua_mark").is_none(), "{values}");

    // The target you set offline stays, and the room list is gone.
    h.type_line("tar").await;
    h.until("tar to list no one in the room", |h| {
        h.screen()
            .windows(2)
            .any(|w| w[0] == "current target: orc" && w[1] == "(no Room.Chars data yet)")
    })
    .await;
    h.finish(grid).await;
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_round_trip_reads_while_you_play_and_lag_lists_it() {
    use crate::app::events::ROUND_TRIP;
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.connect().await;
    h.until_shown("[Exits: south]").await;
    // Nothing shows before the first reading, which comes two seconds in.
    assert_eq!(h.events_of(h.first, ROUND_TRIP), Vec::<Json>::new());
    h.until("the first reading", |h| {
        h.events_of(h.first, ROUND_TRIP)
            .iter()
            .any(|p| p["ms"].is_u64())
    })
    .await;

    h.type_line("#lag").await;
    h.until_shown("round trip to the game ").await;
    h.until_shown("no stalls since you connected at ").await;

    // The reading goes with the connection, and #lag says so.
    h.disconnect().await;
    h.until("the reading to clear", |h| {
        h.events_of(h.first, ROUND_TRIP)
            .last()
            .is_some_and(|p| p["ms"].is_null())
    })
    .await;
    h.type_line("#lag").await;
    h.until_shown("you are not connected, so there is no round trip to show")
        .await;
    h.finish(grid).await;
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bash_with_lines_typed_ahead_and_a_trigger_line_is_no_stall() {
    use crate::app::events::ROUND_TRIP;
    let grid = crate::native::grid::lock_shared_grid_for_test();
    let h = Harness::new(Options {
        bash_ms: 3_000,
        ..Options::new(Build::New)
    })
    .await;
    h.connect().await;
    h.until_shown("[Exits: south]").await;
    h.until("the first reading", |h| {
        h.events_of(h.first, ROUND_TRIP)
            .iter()
            .any(|p| p["ms"].is_u64())
    })
    .await;

    // The bash lags you three seconds. A trigger on its line sends afk
    // while you are lagged, and you type kick and look at once. The
    // game holds all three and answers them once the lag ends.
    h.type_line("#trigger slam {^You slam into Tolliver} send afk")
        .await;
    let bashed = tokio::time::Instant::now();
    h.type_line("bash Tolliver").await;
    h.type_line("kick").await;
    h.type_line("look").await;
    h.until_shown("You slam into Tolliver, and send him flying!")
        .await;
    h.until_shown("You are now in AFK mode.").await;
    h.until_shown("Huh?").await;
    h.until("the held look", |h| {
        h.screen()
            .iter()
            .filter(|r| r.contains("The Bank of Aabahran"))
            .count()
            == 2
    })
    .await;
    // The lag ran longer than the reading interval, so at least one
    // reading fell inside it.
    assert!(bashed.elapsed() > crate::session::round_trip::READ_EVERY);

    // The held lines came back in the order the game read them.
    let received =
        String::from_utf8_lossy(&h.servers[0].received.lock().expect("the bytes")).into_owned();
    let after_bash = &received[received.find("bash Tolliver").expect("the bash")..];
    let mut read: Vec<(usize, &str)> = [
        ("kick\r\n", "Huh?"),
        ("look\r\n", "The Bank of Aabahran"),
        ("afk\r\n", "You are now in AFK mode."),
    ]
    .into_iter()
    .map(|(line, answer)| (after_bash.find(line).expect("the held line"), answer))
    .collect();
    read.sort_unstable();
    let screen = h.screen();
    let slam = screen
        .iter()
        .position(|r| r.contains("You slam into Tolliver"))
        .expect("the bash");
    let shown: Vec<usize> = read
        .iter()
        .map(|(_, answer)| {
            slam + screen[slam..]
                .iter()
                .position(|r| r.contains(answer))
                .expect("the answer")
        })
        .collect();
    assert!(shown.is_sorted(), "{read:?} at {shown:?} in {screen:#?}");

    // No reading counted the lag, and #lag lists no stall.
    let payloads = h.events_of(h.first, ROUND_TRIP);
    assert!(
        payloads
            .iter()
            .all(|p| p["ms"].as_u64().is_some_and(|ms| ms < 300)),
        "{payloads:?}"
    );
    h.type_line("#lag").await;
    h.until_shown("no stalls since you connected at ").await;
    h.finish(grid).await;
}

//! What a typed line and the game's answer leave behind, and in what
//! order, on the path between your keypress and the frame that shows the
//! reply: the bytes the game hears, the screen, and the session log.
//!
//! Each test runs the real session loop with the mock runtime against a
//! game the test plays itself on a local port, so it decides exactly how
//! the answer is cut into socket reads. The log lives in a temporary
//! folder.

use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, Listener, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use crate::app::state::{AppState, SharedState};

const IAC: u8 = 255;
const GA: [u8; 2] = [IAC, 249];
const WILL_ECHO: [u8; 3] = [IAC, 251, 1];
const WONT_ECHO: [u8; 3] = [IAC, 252, 1];

/// How long a test waits for something that should come at once.
const WAIT: Duration = Duration::from_secs(5);

/// The app, one connection to the test's game, and what both heard.
struct Harness {
    app: App<MockRuntime>,
    state: SharedState,
    /// Every `session://output` payload, oldest first.
    outputs: Arc<StdMutex<Vec<serde_json::Value>>>,
    /// How many frames the session asked the native renderer for.
    frames: Arc<AtomicUsize>,
    /// What the game read from the client, as it came.
    from_client: mpsc::UnboundedReceiver<Vec<u8>>,
    heard: Vec<u8>,
    /// The game's side of the connection, which a flood can take.
    to_client: Arc<tokio::sync::Mutex<OwnedWriteHalf>>,
    _dir: tempfile::TempDir,
}

impl Harness {
    /// Connect the session to a game on a local port, with a log.
    async fn new() -> Self {
        Self::with_log(|_| {}).await
    }

    /// Connect as [`Self::new`] does, once `seed` has written to the log,
    /// with a second connection to it for searches, as the app opens.
    async fn with_log(seed: impl FnOnce(&mut vosh_log::LogStore)) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a local port");
        let port = listener.local_addr().expect("an address").port();
        let dir = tempfile::tempdir().expect("a temporary folder");
        let state: SharedState = Arc::new(AppState::default());
        let log = dir.path().join("pinned-session-log.db");
        let mut writer = vosh_log::LogStore::open(&log).expect("the log");
        seed(&mut writer);
        *state.logs.lock().await = Some(writer);
        *state.log_reader.lock().await = Some(vosh_log::LogStore::open(&log).expect("a reader"));

        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .expect("a mock app");
        app.manage::<SharedState>(state.clone());
        let outputs = Arc::new(StdMutex::new(Vec::new()));
        {
            let outputs = outputs.clone();
            app.listen_any("session://output", move |event| {
                let payload = serde_json::from_str(event.payload()).expect("a JSON payload");
                outputs.lock().expect("the outputs").push(payload);
            });
        }
        let frames = Arc::new(AtomicUsize::new(0));
        {
            let frames = frames.clone();
            app.listen_any(crate::output::TEST_FRAME_EVENT, move |_| {
                frames.fetch_add(1, Ordering::SeqCst);
            });
        }

        let accept = tokio::spawn(async move { listener.accept().await.expect("a client").0 });
        let handle = crate::session::spawn(
            app.handle().clone(),
            &state,
            &state.selected_session(),
            "127.0.0.1".into(),
            port,
            false,
            false,
            None,
            (100, 40),
        )
        .await
        .expect("the game answers");
        *state.selected_session().slot.lock().await = Some(handle);
        let socket = accept.await.expect("the accept task");
        socket.set_nodelay(true).expect("no delay");
        let (mut reader, to_client) = socket.into_split();
        let (tx, from_client) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        Self {
            app,
            state,
            outputs,
            frames,
            from_client,
            heard: Vec::new(),
            to_client: Arc::new(tokio::sync::Mutex::new(to_client)),
            _dir: dir,
        }
    }

    /// The game writes `bytes` in one write.
    async fn game_writes(&mut self, bytes: &[u8]) {
        let mut to_client = self.to_client.lock().await;
        to_client.write_all(bytes).await.expect("the write");
        to_client.flush().await.expect("the flush");
    }

    /// Whether the game hears `want` within `wait`.
    async fn game_hears(&mut self, want: &[u8], wait: Duration) -> bool {
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            if self.heard.windows(want.len()).any(|w| w == want) {
                return true;
            }
            match tokio::time::timeout_at(deadline, self.from_client.recv()).await {
                Ok(Some(bytes)) => self.heard.extend_from_slice(&bytes),
                Ok(None) | Err(_) => return false,
            }
        }
    }

    /// Type `line` and press Enter, as the command line does: the echo
    /// first, then the line through the input path.
    async fn type_line(&self, line: &str) {
        crate::ipc::terminal::terminal_local_write(
            self.app.state(),
            format!("{line}\r\n"),
            None,
            None,
        )
        .await
        .expect("the echo");
        crate::ipc::session::session_send_input(
            self.app.handle().clone(),
            self.app.state(),
            line.to_string(),
            None,
        )
        .await
        .expect("the line goes out");
    }

    /// Type a line into the masked password field.
    async fn type_masked(&self, line: &str) {
        crate::ipc::session::session_send_masked(
            self.app.handle().clone(),
            self.app.state(),
            line.to_string(),
            None,
        )
        .await
        .expect("the line goes out");
    }

    /// The text of every output so far, decoded and joined.
    fn shown(&self) -> String {
        let outputs = self.outputs.lock().expect("the outputs");
        let mut text = String::new();
        for out in outputs.iter() {
            if let Some(b64) = out["b64"].as_str() {
                text.push_str(&String::from_utf8_lossy(&decode(b64)));
            }
        }
        text
    }

    /// Wait until the output so far shows `text`.
    async fn until_shown(&self, text: &str) {
        let deadline = tokio::time::Instant::now() + WAIT;
        while !self.shown().contains(text) {
            assert!(
                tokio::time::Instant::now() < deadline,
                "{text:?} never showed. The output: {:?}",
                self.shown()
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    /// The session's log rows, oldest first, once there are `count`.
    async fn log_rows(&self, count: usize) -> Vec<String> {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let rows = {
                let guard = self.state.logs.lock().await;
                let store = guard.as_ref().expect("the log");
                let sessions = store
                    .list_sessions(0, &vosh_log::Scope::default())
                    .expect("the sessions");
                let id = sessions.first().expect("a session").id;
                store
                    .export_session(id, false)
                    .expect("the rows")
                    .lines()
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            };
            if rows.len() >= count || tokio::time::Instant::now() >= deadline {
                return rows;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    /// How many frames the session asked for so far.
    fn frames(&self) -> usize {
        self.frames.load(Ordering::SeqCst)
    }

    /// Wait until the session asked for `count` frames in all, then a
    /// little longer, so a frame too many has time to come.
    async fn settled_frames(&self, count: usize) -> usize {
        let deadline = tokio::time::Instant::now() + WAIT;
        while self.frames() < count && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        self.frames()
    }

    async fn disconnect(&self) {
        let handle = self.state.selected_session().slot.lock().await.take();
        if let Some(handle) = handle {
            handle.shutdown().await;
        }
    }
}

/// Decode standard base64, as the webview does with an output.
fn decode(b64: &str) -> Vec<u8> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0;
    for c in b64.bytes() {
        let Some(v) = ALPHABET.iter().position(|&a| a == c) else {
            continue;
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

/// A room as the game sends it, with its prompt and GA.
fn room(name: &str, lines: usize) -> Vec<u8> {
    let mut text = format!("\r\n{name}\r\n");
    for i in 0..lines {
        let _ = write!(text, "  Line {i} of the description of {name}.\r\n");
    }
    text.push_str("\r\n[Exits: east west]\r\n\r\n[329h 9999m 9999v] ");
    let mut bytes = text.into_bytes();
    bytes.extend_from_slice(&GA);
    bytes
}

/// The log keeps every line of a session, the game's and yours, in the
/// order they passed, with your password as `> (hidden)`: one prompt and
/// answer in a single read, a room long enough to take two reads, a
/// password prompt, and a line the input splits in two.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_log_keeps_every_row_in_stream_order() {
    let _grid = crate::native::grid::lock_shared_grid_for_test();
    crate::native::grid::blank_shared_grid_for_test(100, 40);
    let mut h = Harness::new().await;

    let mut greeting = b"Welcome to the test realm.\r\n\r\n[329h 9999m 9999v] ".to_vec();
    greeting.extend_from_slice(&GA);
    h.game_writes(&greeting).await;
    h.until_shown("Welcome to the test realm.").await;

    h.type_line("look").await;
    assert!(
        h.game_hears(b"look\r\n", WAIT).await,
        "the game never heard look"
    );
    h.game_writes(&room("Market Street", 3)).await;
    h.until_shown("Line 2 of the description of Market Street.")
        .await;

    // Long enough that one socket read cannot take it all.
    h.type_line("east").await;
    assert!(
        h.game_hears(b"east\r\n", WAIT).await,
        "the game never heard east"
    );
    h.game_writes(&room("The Long Hall", 220)).await;
    h.until_shown("Line 219 of the description of The Long Hall.")
        .await;

    let mut ask = WILL_ECHO.to_vec();
    ask.extend_from_slice(b"Password: ");
    ask.extend_from_slice(&GA);
    h.game_writes(&ask).await;
    h.until_shown("Password: ").await;
    h.type_masked("Tr0ub4dor&3").await;
    assert!(
        h.game_hears(b"Tr0ub4dor&3\r\n", WAIT).await,
        "the game never heard the password"
    );
    let mut back = WONT_ECHO.to_vec();
    back.extend_from_slice(b"\r\nWelcome back.\r\n\r\n[329h 9999m 9999v] ");
    back.extend_from_slice(&GA);
    h.game_writes(&back).await;
    h.until_shown("Welcome back.").await;

    h.type_line("say hi;west").await;
    assert!(
        h.game_hears(b"say hi\r\nwest\r\n", WAIT).await,
        "the game never heard both lines"
    );
    h.game_writes(b"You say 'hi'\r\n").await;
    h.game_writes(&room("An Alley", 1)).await;
    h.until_shown("Line 0 of the description of An Alley.")
        .await;

    // A prompt the GA ends shows but is not a log row, and neither is
    // the password prompt.
    let mut want = vec![
        "Welcome to the test realm.".to_string(),
        String::new(),
        "> look".to_string(),
        String::new(),
        "Market Street".to_string(),
    ];
    for i in 0..3 {
        want.push(format!("  Line {i} of the description of Market Street."));
    }
    want.extend([
        String::new(),
        "[Exits: east west]".to_string(),
        String::new(),
        "> east".to_string(),
        String::new(),
        "The Long Hall".to_string(),
    ]);
    for i in 0..220 {
        want.push(format!("  Line {i} of the description of The Long Hall."));
    }
    want.extend([
        String::new(),
        "[Exits: east west]".to_string(),
        String::new(),
        "> (hidden)".to_string(),
        String::new(),
        "Welcome back.".to_string(),
        String::new(),
        "> say hi".to_string(),
        "> west".to_string(),
        "You say 'hi'".to_string(),
        String::new(),
        "An Alley".to_string(),
        "  Line 0 of the description of An Alley.".to_string(),
        String::new(),
        "[Exits: east west]".to_string(),
        String::new(),
    ]);
    h.disconnect().await;
    let rows = h.log_rows(want.len()).await;
    assert_eq!(rows, want);
}

/// Your typed echo lands on the native grid on the row after the prompt
/// the GA ended, and the game's reply after it, whichever frame draws
/// them. The echo reaches the grid only where the surface draws it.
#[cfg(native_surface)]
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn your_echo_sits_between_the_prompt_and_the_reply_on_screen() {
    let _grid = crate::native::grid::lock_shared_grid_for_test();
    crate::native::grid::blank_shared_grid_for_test(100, 40);
    let mut h = Harness::new().await;
    let mut greeting = b"Welcome.\r\n\r\n[329h 9999m 9999v] ".to_vec();
    greeting.extend_from_slice(&GA);
    h.game_writes(&greeting).await;
    h.until_shown("Welcome.").await;

    h.type_line("look").await;
    assert!(
        h.game_hears(b"look\r\n", WAIT).await,
        "the game never heard look"
    );
    h.game_writes(&room("Market Street", 2)).await;
    h.until_shown("Line 1 of the description of Market Street.")
        .await;

    let rows = crate::native::grid::shared_screen_rows_for_test();
    let echo = rows
        .iter()
        .position(|r| r == "look")
        .unwrap_or_else(|| panic!("no echo: {rows:#?}"));
    assert_eq!(
        rows[..echo]
            .iter()
            .rev()
            .find(|r| !r.is_empty())
            .map(String::as_str),
        Some("[329h 9999m 9999v]"),
        "the echo does not follow the prompt: {rows:#?}"
    );
    let reply = rows
        .iter()
        .position(|r| r == "Market Street")
        .unwrap_or_else(|| panic!("no reply: {rows:#?}"));
    assert!(echo < reply, "the reply came before the echo: {rows:#?}");
    h.disconnect().await;
}

/// Your line reaches the game before its log row is written. The test
/// holds the log the way a slow write would, and the game still hears
/// the line. The row lands once the log is free.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn your_line_reaches_the_game_before_its_log_row() {
    let _grid = crate::native::grid::lock_shared_grid_for_test();
    crate::native::grid::blank_shared_grid_for_test(100, 40);
    let mut h = Harness::new().await;

    let log = h.state.logs.clone();
    let busy = log.lock().await;
    h.type_line("look").await;
    let heard = h.game_hears(b"look\r\n", Duration::from_secs(2)).await;
    drop(busy);
    assert!(heard, "the game heard nothing while the log was busy");
    assert_eq!(h.log_rows(1).await, vec!["> look".to_string()]);
    h.disconnect().await;
}

/// A search through every log you kept leaves the game and its log
/// alone. While the search holds the read connection, the writer stays
/// free, your line reaches the game, the reply shows and your line's row
/// lands. A new search then stops it. The session gets one async
/// worker, so a search that read on it would hold the reply until the
/// search ended.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_search_never_holds_up_the_game_or_its_log() {
    let _grid = crate::native::grid::lock_shared_grid_for_test();
    crate::native::grid::blank_shared_grid_for_test(100, 40);
    let mut h = Harness::with_log(|writer| {
        let id = writer
            .start_session("127.0.0.1", 4000, 0)
            .expect("an older log");
        let rows: Vec<_> = (0..500_000)
            .map(|n| vosh_log::LogEntry {
                session_id: id,
                ts_ms: n,
                text: format!("  Line {n} of the description of Market Street."),
                raw: None,
                kind: vosh_log::LineKind::Text,
            })
            .collect();
        writer.append_batch(&rows).expect("the rows");
    })
    .await;

    // All time with a count, for a line no row holds, so it reads every
    // row until a new search replaces it.
    let search = {
        let state = h.state.clone();
        tokio::spawn(async move {
            crate::ipc::logs::search_page(
                &state,
                "Maren bows".into(),
                false,
                50,
                crate::ipc::logs::LogScope::default(),
                None,
                None,
                true,
            )
            .await
        })
    };
    // A thread sleep, as a timer would wait on the worker the search
    // might hold.
    let deadline = std::time::Instant::now() + WAIT;
    while h.state.log_reader.try_lock().is_ok() {
        assert!(
            std::time::Instant::now() < deadline,
            "the search never took the reader"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        h.state.logs.try_lock().is_ok(),
        "the search holds the writer"
    );

    h.type_line("look").await;
    assert!(
        h.game_hears(b"look\r\n", WAIT).await,
        "the game never heard look"
    );
    assert_eq!(h.log_rows(1).await, vec!["> look".to_string()]);
    let before = h.frames();
    h.game_writes(&room("Market Street", 2)).await;
    h.until_shown("Line 1 of the description of Market Street.")
        .await;
    let deadline = tokio::time::Instant::now() + WAIT;
    while h.frames() == before {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the reply drew no frame"
        );
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert!(
        !search.is_finished(),
        "the search ended before the game answered"
    );

    h.state.log_searches.fetch_add(1, Ordering::AcqRel);
    let stopped = search
        .await
        .expect("the search task")
        .expect_err("a stopped search");
    assert!(stopped.starts_with("stopped"), "{stopped}");
    h.disconnect().await;
}

/// A room with wide lines, its prompt and GA, long enough that the
/// socket hands it over in two reads with the prompt in the second.
fn wide_room(name: &str, lines: usize) -> Vec<u8> {
    let mut text = format!("\r\n{name}\r\n");
    for i in 0..lines {
        let _ = write!(text, "  Line {i:03} of {name}, {}\r\n", "x".repeat(70));
    }
    text.push_str("\r\n[329h 9999m 9999v] ");
    let mut bytes = text.into_bytes();
    bytes.extend_from_slice(&GA);
    bytes
}

/// An answer in one read shows in one frame. A room the socket hands over
/// in several reads, the prompt and its GA in the last, asks for at most
/// one frame per read: the reads share a frame only when the next piece
/// already waits as the session drains the socket, and how the system
/// cuts a write differs by platform (on Windows the pieces often come
/// later). The log writes the rows once the socket is quiet, so a busy
/// log holds no read back and loses no row.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_answer_in_one_read_shows_in_one_frame_and_a_split_one_loses_no_row() {
    let _grid = crate::native::grid::lock_shared_grid_for_test();
    crate::native::grid::blank_shared_grid_for_test(100, 40);
    let mut h = Harness::new().await;
    let mut greeting = b"Welcome.\r\n\r\n[329h 9999m 9999v] ".to_vec();
    greeting.extend_from_slice(&GA);
    h.game_writes(&greeting).await;
    h.until_shown("Welcome.").await;
    let before = h.settled_frames(1).await;

    h.game_writes(&room("Market Street", 3)).await;
    h.until_shown("Line 2 of the description of Market Street.")
        .await;
    assert_eq!(h.settled_frames(before + 1).await, before + 1);

    let hall = wide_room("The Wide Hall", 100);
    assert!(hall.len() > 8 * 1024, "the hall fits in one read");
    let log = h.state.logs.clone();
    let busy = log.lock().await;
    let before = h.frames();
    let reads_before = h.outputs.lock().expect("the outputs").len();
    h.game_writes(&hall).await;
    h.until_shown("Line 099 of The Wide Hall").await;
    let reads = h.outputs.lock().expect("the outputs").len() - reads_before;
    assert!(reads >= 2, "the hall came in {reads} read");
    let frames = h.settled_frames(before + 1).await - before;
    assert!(
        (1..=reads).contains(&frames),
        "the hall asked for {frames} frames over {reads} reads"
    );
    drop(busy);

    let rows = h.log_rows(100).await;
    let hall_rows: Vec<_> = rows
        .iter()
        .filter(|r| r.contains("of The Wide Hall"))
        .collect();
    assert_eq!(hall_rows.len(), 100, "the log lost rows: {rows:#?}");
    assert!(hall_rows[0].starts_with("  Line 000"));
    assert!(hall_rows[99].starts_with("  Line 099"));
    h.disconnect().await;
}

/// Your line's row goes in the log as the line leaves, even while the
/// game never pauses. The game floods GMCP, which draws nothing, so the
/// socket never runs dry and no frame is owed to bring the rows along.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn your_line_reaches_the_log_while_the_game_never_pauses() {
    let _grid = crate::native::grid::lock_shared_grid_for_test();
    crate::native::grid::blank_shared_grid_for_test(100, 40);
    let h = Harness::new().await;

    let mut packet = vec![IAC, 250, 201];
    packet.extend_from_slice(br#"Test.Flood {"n":1}"#);
    packet.extend_from_slice(&[IAC, 240]);
    let chunk = packet.repeat(4096);
    let stop = Arc::new(AtomicBool::new(false));
    let flood = {
        let (to_client, stop) = (h.to_client.clone(), stop.clone());
        tokio::spawn(async move {
            let mut to_client = to_client.lock().await;
            while !stop.load(Ordering::SeqCst) {
                if to_client.write_all(&chunk).await.is_err() {
                    break;
                }
            }
        })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;

    h.type_line("look").await;
    let rows = h.log_rows(1).await;
    let flooding = !flood.is_finished();
    stop.store(true, Ordering::SeqCst);
    h.disconnect().await;
    let _ = flood.await;
    assert!(flooding, "the flood ended before the row landed");
    assert_eq!(rows, vec!["> look".to_string()]);
}

/// A GMCP packet as the game frames it.
fn packet(package: &str, json: &str) -> Vec<u8> {
    let mut bytes = vec![IAC, 250, 201];
    bytes.extend_from_slice(format!("{package} {json}").as_bytes());
    bytes.extend_from_slice(&[IAC, 240]);
    bytes
}

/// An answer that rings alerts still shows in one frame. The tell, the
/// trigger with a tone and your name all ring on the read that shows it,
/// and none of them holds the read back.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_answer_that_rings_alerts_still_shows_in_one_frame() {
    let _grid = crate::native::grid::lock_shared_grid_for_test();
    crate::native::grid::blank_shared_grid_for_test(100, 40);
    let mut h = Harness::new().await;
    {
        let mut p = h.state.selected_session().lock_profile().await;
        p.ui.enabled_presets = vec!["alert_tells".into(), "alert_name".into()];
        let visitor = vosh_automation::trigger::Trigger {
            alert: Some(crate::alert::AlertParts {
                banner: true,
                sound: Some("chime".into()),
                ..crate::alert::AlertParts::default()
            }),
            ..vosh_automation::trigger::Trigger::new(
                "visitor",
                r"^\w+ walks in\.$",
                vosh_automation::trigger::TriggerAction::Route {
                    pane: "chat".into(),
                },
            )
        };
        p.triggers.set(visitor).expect("the trigger");
    }
    let alerts = Arc::new(AtomicUsize::new(0));
    {
        let alerts = alerts.clone();
        h.app.listen_any(crate::app::events::ALERT, move |_| {
            alerts.fetch_add(1, Ordering::SeqCst);
        });
    }
    let mut greeting = packet(
        "Char.Status",
        r#"{"name":"Orla","level":50,"race":"human","class":"dark-knight"}"#,
    );
    greeting.extend_from_slice(b"Welcome.\r\n\r\n[329h 9999m 9999v] ");
    greeting.extend_from_slice(&GA);
    h.game_writes(&greeting).await;
    h.until_shown("Welcome.").await;
    let before = h.settled_frames(1).await;

    let tell = include_str!("../../../fixtures/gmcp/aabahran/chat/tell.gmcp");
    let (package, json) = tell.trim().split_once(' ').expect("a packet");
    let mut answer = packet(package, json);
    // `$n walks in.` (act_move.c:1053) and `$n looks at $N.`
    // (act_info.c:1054).
    answer.extend_from_slice(
        b"\r\nMaren walks in.\r\nMaren looks at Orla.\r\n\r\n[329h 9999m 9999v] ",
    );
    answer.extend_from_slice(&GA);
    h.game_writes(&answer).await;
    h.until_shown("Maren looks at Orla.").await;
    assert_eq!(h.settled_frames(before + 1).await, before + 1);
    assert_eq!(alerts.load(Ordering::SeqCst), 3);
    h.disconnect().await;
}

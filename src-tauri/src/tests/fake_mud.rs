//! The real session against the fake Aabahran of the test kit, over a
//! local port, in the new build and in the older build.
//!
//! Each test runs the session loop, the typed input path and the prompt
//! lookup with the mock runtime, and serves the fake game on a port of
//! its own. What the webview would hear arrives through the mock app's
//! listeners, and a native grid replays the output the way the terminal
//! shows it. The profile folder and the log live in a temporary folder.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use serde_json::Value as Json;
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, Listener, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use vosh_prompt::testkit::mud::{PROMPT, PROMPT_ALL};
use vosh_prompt::testkit::{Build, Mud, Options};

use crate::app::state::{AppState, SharedState};
use crate::profile::login_match::AutoMatch;
use crate::profile::set::{ProfileSet, DEFAULT_PROFILE_NAME};

/// The events the tests read, as the webview would hear them.
const EVENTS: [&str; 10] = [
    "session://output",
    "session://game-prompt-seen",
    "session://prompt-status",
    "session://prompt-state",
    "session://prompt-vars",
    "session://hidden",
    "session://state",
    "session://target",
    crate::app::events::AFFECT_FULL_CHANGED,
    crate::app::events::PROMPT_CONFIG_CHANGED,
];

/// What the prompts off status says in `#prompt`.
const PROMPTS_OFF: &str =
    "You turned prompts off in the game. Type prompt in the game to turn them back on.";

/// The setting `prompt x` types in these tests, and what the game stores.
const TYPED_X: &str = "<%h/%Hhp %m/%Mmn>";
const PROMPT_X: &str = "<%h/%Hhp %m/%Mmn> ";

/// Serve the fake game on a local port until the test ends. Each
/// connection plays the options `options` holds when it connects. `asks`
/// counts each IAC DO EOR a client sends.
async fn serve_fake(options: Arc<StdMutex<Options>>, asks: Arc<AtomicUsize>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a local port");
    let port = listener.local_addr().expect("an address").port();
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let options = options.lock().expect("the options").clone();
            tokio::spawn(play(socket, options, asks.clone()));
        }
    });
    port
}

/// One connection to the fake game, as `examples/fake_mud.rs` plays it.
async fn play(
    mut socket: TcpStream,
    options: Options,
    asks: Arc<AtomicUsize>,
) -> std::io::Result<()> {
    use vosh_prompt::testkit::mud::telnet::{DO, IAC, TELOPT_EOR};
    socket.set_nodelay(true)?;
    let mut mud = Mud::new(options);
    socket.write_all(&mud.greeting()).await?;
    let mut buf = [0u8; 4096];
    loop {
        let n = socket.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        let asked = buf[..n]
            .windows(3)
            .filter(|w| *w == [IAC, DO, TELOPT_EOR])
            .count();
        asks.fetch_add(asked, Ordering::SeqCst);
        for write in mud.receive(&buf[..n]) {
            if write.after_ms > 0 {
                tokio::time::sleep(Duration::from_millis(write.after_ms)).await;
            }
            socket.write_all(&write.bytes).await?;
            if write.close {
                return socket.shutdown().await;
            }
        }
    }
}

/// Something the terminal or the webview heard, in order.
#[derive(Debug, Clone)]
enum Heard {
    Event(&'static str, String),
    /// Your typed line, which the webview echoes itself.
    Echo(String),
}

/// The app with one profile folder, one log and one connection at a time.
struct Harness {
    app: App<MockRuntime>,
    state: SharedState,
    heard: Arc<StdMutex<Vec<Heard>>>,
    dir: tempfile::TempDir,
    port: u16,
    /// The fake game serves these options to the next connection.
    fake: Arc<StdMutex<Options>>,
    /// How many times a client asked the fake game for EOR.
    eor_asks: Arc<AtomicUsize>,
    /// Whether the fake game counts as The Forsaken Lands when the
    /// session connects.
    forsaken: AtomicBool,
}

impl Harness {
    /// A fake game of `build` on a port of its own, and an app whose
    /// profiles claim Tester (default) and Healer there, with a log.
    async fn new(options: Options) -> Self {
        let fake = Arc::new(StdMutex::new(options));
        let eor_asks = Arc::new(AtomicUsize::new(0));
        let port = serve_fake(fake.clone(), eor_asks.clone()).await;
        let dir = tempfile::tempdir().expect("a temporary folder");
        let state: SharedState = Arc::new(AppState::default());
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).expect("a set");
        let claim = |who: &str| AutoMatch {
            host: Some("127.0.0.1".into()),
            port: Some(port),
            characters: vec![who.into()],
            enabled: true,
        };
        set.set_metadata(DEFAULT_PROFILE_NAME, None, Some(claim("Tester")))
            .expect("default claims Tester");
        set.create("Healer").expect("Healer");
        set.set_metadata("Healer", None, Some(claim("Healer")))
            .expect("Healer claims Healer");
        *state.profile_set.lock().await = Some(set);
        let log = dir.path().join("logs.sqlite");
        *state.logs.lock().await = Some(vosh_log::LogStore::open(&log).expect("the log"));
        *state.log_reader.lock().await = Some(vosh_log::LogStore::open(&log).expect("a reader"));

        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .expect("a mock app");
        app.manage::<SharedState>(state.clone());
        let heard = Arc::new(StdMutex::new(Vec::new()));
        for name in EVENTS {
            let heard = heard.clone();
            app.listen_any(name, move |event| {
                heard
                    .lock()
                    .expect("the events")
                    .push(Heard::Event(name, event.payload().to_string()));
            });
        }
        Self {
            app,
            state,
            heard,
            dir,
            port,
            fake,
            eor_asks,
            forsaken: AtomicBool::new(false),
        }
    }

    /// Give the live profile the prompt table `config`, as a load does.
    async fn set_prompt(&self, config: vosh_prompt::PromptConfig) {
        self.state.profile.lock().await.set_prompt_config(config);
    }

    /// Connect to the fake game the way `session::connect` does, with no
    /// scrollback file.
    async fn connect(&self) {
        let state = &self.state;
        if let Ok(mut g) = state.current_connection.lock() {
            *g = Some(("127.0.0.1".into(), self.port));
        }
        if let Ok(mut g) = state.current_character.lock() {
            *g = None;
        }
        let handle = crate::session::spawn(
            self.app.handle().clone(),
            state,
            "127.0.0.1".into(),
            self.port,
            false,
            self.forsaken.load(Ordering::SeqCst),
            None,
            (100, 40),
        )
        .await
        .expect("the fake game answers");
        *state.session.lock().await = Some(handle);
    }

    /// Close the connection the way `session::disconnect` does.
    async fn disconnect(&self) {
        let handle = self.state.session.lock().await.take();
        if let Some(handle) = handle {
            handle.shutdown().await;
        }
        if let Ok(mut g) = self.state.current_connection.lock() {
            *g = None;
        }
        if let Ok(mut g) = self.state.current_character.lock() {
            *g = None;
        }
    }

    /// Type `line` and press Enter: the webview echoes it, tells the
    /// session it wrote after the newest output it took, and sends it
    /// through the input path.
    async fn type_line(&self, line: &str) {
        let after = self.echo(line);
        if let Some(handle) = self.state.session.lock().await.as_ref() {
            let _ = handle.local_write(after);
        }
        crate::ipc::session::session_send_input(
            self.app.handle().clone(),
            self.app.state(),
            line.to_string(),
        )
        .await
        .expect("the line goes out");
    }

    /// The webview echoes `line` on the terminal. Returns the newest
    /// output of the prompt stage the terminal took before it, which the
    /// echo follows.
    fn echo(&self, line: &str) -> u64 {
        let mut heard = self.heard.lock().expect("the events");
        let after = heard
            .iter()
            .filter_map(|h| match h {
                Heard::Event("session://output", payload) => {
                    serde_json::from_str::<Json>(payload).ok()?["id"].as_u64()
                }
                _ => None,
            })
            .max()
            .unwrap_or(0);
        heard.push(Heard::Echo(format!("{line}\r\n")));
        after
    }

    fn heard(&self) -> Vec<Heard> {
        self.heard.lock().expect("the events").clone()
    }

    /// Every payload of the event `name`, oldest first.
    fn events(&self, name: &str) -> Vec<Json> {
        self.heard()
            .into_iter()
            .filter_map(|h| match h {
                Heard::Event(n, payload) if n == name => {
                    Some(serde_json::from_str(&payload).expect("a JSON payload"))
                }
                _ => None,
            })
            .collect()
    }

    /// What the terminal shows, 100 wide, rows trimmed.
    fn screen(&self) -> Vec<String> {
        let mut grid = crate::native::grid::TermGrid::new(100, 200);
        for heard in self.heard() {
            match heard {
                Heard::Echo(text) => grid.local_write(text.as_bytes()),
                Heard::Event("session://output", payload) => {
                    grid.session_output(&output(&payload));
                }
                Heard::Event(..) => {}
            }
        }
        let mut rows: Vec<String> = (0..grid.screen_lines())
            .map(|line| grid.row_string(line).trim_end().to_string())
            .collect();
        while rows.last().is_some_and(String::is_empty) {
            rows.pop();
        }
        rows
    }

    /// The last row that shows anything.
    fn last_row(&self) -> String {
        self.screen().pop().unwrap_or_default()
    }

    /// Wait up to five seconds for `test` to hold.
    async fn until(&self, what: &str, test: impl Fn(&Self) -> bool) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while !test(self) {
            assert!(
                tokio::time::Instant::now() < deadline,
                "{what} never came. The screen:\n{:#?}\nThe prompt events: {:?} {:?}",
                self.screen(),
                self.events("session://game-prompt-seen"),
                self.events("session://prompt-status")
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Wait until the screen ends in `row`, the drawn prompt of a pulse.
    async fn until_last_row(&self, row: &str) {
        self.until(&format!("a last row {row:?}"), |h| h.last_row() == row)
            .await;
    }

    /// Wait until a row shows `text`.
    async fn until_shown(&self, text: &str) {
        self.until(&format!("a row with {text:?}"), |h| {
            h.screen().iter().any(|r| r.contains(text))
        })
        .await;
    }

    /// The live profile's capture.
    async fn capture(&self) -> vosh_prompt::CaptureConfig {
        self.state
            .profile
            .lock()
            .await
            .prompt
            .config()
            .capture
            .clone()
    }

    /// The live profile's whole `[prompt]` table.
    async fn prompt_table(&self) -> vosh_prompt::PromptConfig {
        self.state.profile.lock().await.prompt.config().clone()
    }

    /// Have the fake game count as The Forsaken Lands, as the real host
    /// does, so a capture that reads no Aabahran codes plays by its rules.
    /// It counts from the next connect on.
    fn count_as_forsaken_lands(&self) {
        self.forsaken.store(true, Ordering::SeqCst);
    }

    /// Where the profile `name` keeps its file.
    async fn profile_file(&self, name: &str) -> std::path::PathBuf {
        self.state
            .profile_set
            .lock()
            .await
            .as_ref()
            .expect("the set")
            .profile_path(name)
    }

    /// Close the connection, let other tests at the shared grid, and let
    /// the save a change marked land in the temporary folder before it
    /// goes.
    #[allow(clippy::await_holding_lock)]
    async fn finish(self, grid: std::sync::MutexGuard<'static, ()>) {
        self.disconnect().await;
        drop(grid);
        tokio::time::sleep(Duration::from_millis(2_500)).await;
        drop(self.dir);
    }
}

/// A `session://output` payload as the output it carries.
fn output(payload: &str) -> vosh_prompt::stage::Output {
    let json: Json = serde_json::from_str(payload).expect("an output payload");
    let mut out = vosh_prompt::stage::Output::new(false);
    out.bytes = base64_decode(json["b64"].as_str().unwrap_or_default());
    if let Some(replace) = json.get("replace").filter(|r| !r.is_null()) {
        out.replace = Some(vosh_prompt::stage::Replace {
            gen: replace["gen"].as_u64().expect("a generation"),
            bytes: base64_decode(replace["b64"].as_str().unwrap_or_default()),
            fresh: replace["fresh"].as_bool().unwrap_or(false),
            above: replace.get("above").filter(|a| !a.is_null()).map(|above| {
                vosh_prompt::stage::Above {
                    plain: above["plain"].as_str().unwrap_or_default().to_string(),
                    bytes: base64_decode(above["b64"].as_str().unwrap_or_default()),
                }
            }),
            tail: base64_decode(replace["tail"].as_str().unwrap_or_default()),
        });
    }
    if let Some(restore) = json.get("restore").and_then(Json::as_str) {
        out.restore = Some(base64_decode(restore));
    }
    out
}

fn base64_decode(text: &str) -> Vec<u8> {
    let value = |c: u8| -> u32 {
        match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("no base64 digit {c}"),
        }
    };
    let mut out = Vec::new();
    for chunk in text.as_bytes().chunks(4) {
        let digits: Vec<u8> = chunk.iter().copied().filter(|&c| c != b'=').collect();
        let mut n = 0u32;
        for (i, &c) in digits.iter().enumerate() {
            n |= value(c) << (18 - 6 * i);
        }
        let bytes = n.to_be_bytes();
        out.extend_from_slice(&bytes[1..digits.len()]);
    }
    out
}

/// A table that reads Aabahran's codes `prompt`, follows the game, and
/// draws `<%hp>` in place of the prompt.
fn codes(prompt: &str) -> vosh_prompt::PromptConfig {
    vosh_prompt::PromptConfig {
        draw: true,
        template: "<%hp>".into(),
        capture: vosh_prompt::CaptureConfig::Aabahran(vosh_prompt::config::AabahranCapture {
            prompt: prompt.into(),
            ..vosh_prompt::config::AabahranCapture::default()
        }),
        ..vosh_prompt::PromptConfig::default()
    }
}

/// A table that draws `<%hp>` and reads no prompt yet.
fn no_capture() -> vosh_prompt::PromptConfig {
    vosh_prompt::PromptConfig {
        draw: true,
        template: "<%hp>".into(),
        ..vosh_prompt::PromptConfig::default()
    }
}

/// The codes and source of an aabahran capture.
fn codes_of(
    capture: &vosh_prompt::CaptureConfig,
) -> (String, Option<vosh_prompt::config::CaptureSource>) {
    match capture {
        vosh_prompt::CaptureConfig::Aabahran(codes) => (codes.prompt.clone(), codes.source),
        other => panic!("no aabahran capture: {other:?}"),
    }
}

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
    h.state.note_active_profile(DEFAULT_PROFILE_NAME);
    crate::ipc::prompt::prompt_watch(h.app.state(), true);
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
    let edited =
        crate::prompt::edit(&*h.state.profile.lock().await, "<%hp>", &op).expect("the edit");
    assert_eq!(edited.template, "<%hp>%mana");
    let config = vosh_prompt::PromptConfig {
        template: edited.template,
        ..h.prompt_table().await
    };
    crate::ipc::prompt::prompt_config_set(h.app.handle().clone(), h.app.state(), config, None)
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
    crate::ipc::prompt::prompt_watch(h.app.state(), false);
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
    crate::ipc::prompt::prompt_watch(h.app.state(), true);
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
    if let Some(handle) = h.state.session.lock().await.as_ref() {
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
    h.state.note_active_profile(DEFAULT_PROFILE_NAME);
    h.connect().await;
    h.until_last_row("<1020>").await;

    // Your echo lands on the terminal after the login prompt, but your
    // line reaches the session first, and the game answers.
    let after = h.echo("look");
    crate::ipc::session::session_send_input(h.app.handle().clone(), h.app.state(), "look".into())
        .await
        .expect("the line goes out");
    h.until_shown("[Exits: south]").await;
    h.until_last_row("<1020>").await;

    // Only now does the session hear of the echo.
    if let Some(handle) = h.state.session.lock().await.as_ref() {
        let _ = handle.local_write(after);
    }

    // The prompt that answered your look is still the open row.
    let config = vosh_prompt::PromptConfig {
        template: "<%hp>%mana".into(),
        ..h.prompt_table().await
    };
    crate::ipc::prompt::prompt_config_set(h.app.handle().clone(), h.app.state(), config, None)
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
    let seen = crate::prompt::last_seen::last_seen(&h.state)
        .await
        .expect("the game sent it");
    assert_eq!(seen.source, "gmcp");
    assert!(seen.at_login);
    assert_eq!(seen.prompt.as_deref(), Some(PROMPT));
    assert_eq!(seen.enabled, Some(true));
    assert_eq!(seen.character.as_deref(), Some("Tester"));
    assert!(h.state.profile.lock().await.prompt.vars.new_build());

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
    let seen = crate::prompt::last_seen::last_seen(&h.state)
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
    let last = crate::prompt::last_seen::last_seen(&h.state)
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
        .reader_busy
        .store(true, std::sync::atomic::Ordering::Release);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let held = repaints(&h).len();
    tokio::time::sleep(Duration::from_millis(1_500)).await;
    assert_eq!(repaints(&h).len(), held);
    // Once you let go it catches up within the second.
    h.state
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
    crate::ipc::prompt::prompt_code_reader_set(h.app.state(), true)
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
    let seen = crate::prompt::last_seen::last_seen(&h.state)
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
    assert_eq!(crate::prompt::last_seen::last_seen(&h.state).await, None);

    // Healer logs in on the same game. The log still holds only
    // Tester's prompt, so Healer's card stays empty.
    h.fake.lock().expect("the options").name = "Healer".into();
    h.connect().await;
    h.until_shown("Welcome to the fake Aabahran, Healer.").await;
    h.until("the session to name Healer", |h| {
        h.state
            .current_character
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .as_deref()
            == Some("Healer")
    })
    .await;
    assert_eq!(crate::prompt::last_seen::last_seen(&h.state).await, None);
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
    assert!(!h.state.profile.lock().await.prompt.vars.new_build());
    let leftover = &h.events("session://game-prompt-seen");
    assert!(leftover.is_empty(), "{leftover:?}");
    // prompt in the game sends Char.Prompt again.
    h.type_line("prompt").await;
    h.until_shown(&format!("Current prompt: {PROMPT}")).await;
    h.until("the game's prompt settings", |h| {
        !h.events("session://game-prompt-seen").is_empty()
    })
    .await;
    assert!(h.state.profile.lock().await.prompt.vars.new_build());
    assert_eq!(
        h.events("session://game-prompt-seen"),
        [serde_json::json!({"kind": "gmcp", "text": PROMPT, "applied": false})]
    );
    let seen = crate::prompt::last_seen::last_seen(&h.state)
        .await
        .expect("seen");
    assert_eq!(seen.source, "gmcp");
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
    crate::profile::switch::apply_profile_switch(h.app.handle(), &h.state, "Healer")
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
    crate::profile::switch::apply_profile_switch(h.app.handle(), &h.state, DEFAULT_PROFILE_NAME)
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
    h.state.affect_full.set_path(file.clone());
    h.connect().await;
    // Tester logs in with bless at 6 and armor at 44, both first seen.
    h.until("the login fulls", |h| {
        h.state.affect_full.map() == fulls(&[("armor", 44), ("bless", 6)])
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
    assert_eq!(
        h.state.affect_full.map(),
        fulls(&[("armor", 44), ("bless", 6)])
    );
    // A recast of armor for more hours starts its full over.
    h.type_line("cast 48 armor").await;
    h.until("the recast", |h| {
        h.state.affect_full.map() == fulls(&[("armor", 48), ("bless", 6)])
    })
    .await;
    h.type_line("tick").await;
    h.until("a third tick", |h| passes(h) == 3).await;
    // The windows heard each change, the last one the fulls now.
    let heard = h.events(crate::app::events::AFFECT_FULL_CHANGED);
    assert_eq!(
        heard,
        [
            serde_json::json!({ "armor": 44, "bless": 6 }),
            serde_json::json!({ "armor": 48, "bless": 6 }),
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
    assert!(h.state.affect_full.map().is_empty());

    // Log back in with armor at 47 and bless at 3, as the game kept them.
    h.fake.lock().expect("the options").affects = vec![
        vosh_prompt::testkit::Affect::spell("bless", 3),
        vosh_prompt::testkit::Affect::spell("armor", 47),
    ];
    h.connect().await;
    h.until("the same fulls", |h| {
        h.state.affect_full.map() == fulls(&[("armor", 48), ("bless", 6)])
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
        .affect_full
        .set_path(crate::disk::paths::affect_full_path(h.dir.path()));
    h.connect().await;
    h.until("the login fulls", |h| {
        h.state.affect_full.map() == fulls(&[("armor", 44), ("bless", 6)])
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
    assert_eq!(h.state.affect_full.map(), want);

    // quit menu: the game takes each affect off in turn, and the pane
    // empties with it, on the same link.
    h.type_line("quit menu").await;
    h.until_shown("return to your account menu").await;
    h.until("the pane empties", |h| h.state.affect_full.map().is_empty())
        .await;
    // Play Tester again: the game sends the affects the pfile kept, with
    // armor at 46 and bless at 4, and each keeps its full.
    h.type_line("").await;
    h.until("the fulls come back", |h| h.state.affect_full.map() == want)
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
        h.state.affect_full.map().is_empty() && saved_fulls(h) == want
    })
    .await;
    h.disconnect().await;

    // Log back in with the affects as the game kept them.
    h.fake.lock().expect("the options").affects = vec![
        vosh_prompt::testkit::Affect::spell("bless", 4),
        vosh_prompt::testkit::Affect::spell("armor", 46),
    ];
    h.connect().await;
    h.until("the same fulls", |h| h.state.affect_full.map() == want)
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

/// The session sends each GMCP package on the event
/// `fixtures/ipc/gmcp-events.json` names for it. `onGmcpPackage` on the
/// page builds its listen from the same file in session.test.ts, so a
/// change to the encoding on one side alone fails one of the two.
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
    let heard = Arc::new(StdMutex::new(std::collections::BTreeSet::new()));
    for event in &events {
        let (heard, name) = (heard.clone(), event.clone());
        h.app.listen_any(event.clone(), move |_| {
            heard.lock().expect("the events").insert(name.clone());
        });
    }
    h.connect().await;
    h.until("every GMCP event at login", |_| {
        heard.lock().expect("the events").len() == events.len()
    })
    .await;
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
    h.state.profile.lock().await.aliases.set(
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
        let mut p = h.state.profile.lock().await;
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
            .any(|vars| vars["lua_mark"] == "on")
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
    h.finish(grid).await;
}

// A plugin you turned on does all its entry script asks as it loads at
// launch, the way the Lua you type does. Here it runs a line through
// mud.input, gives your prompt a value and starts a timer, which fires
// once the game connects. The guard keeps other tests off the shared
// native grid.
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
         mud.input('#alias plugged kick')\n\
         mud.set_prompt_var('plugin_mark', 'on')\n",
    )
    .expect("the entry script");
    h.state.profile.lock().await.plugins.enabled = vec!["on_load".into()];

    crate::app::plugins::load_enabled_plugins(h.app.handle(), &h.state, plugins).await;
    assert!(
        h.state
            .profile
            .lock()
            .await
            .aliases
            .get("plugged")
            .is_some(),
        "the mud.input line ran"
    );
    h.until("the value the plugin gave your prompt", |h| {
        h.events("session://prompt-vars")
            .iter()
            .any(|vars| vars["plugin_mark"] == "on")
    })
    .await;

    h.connect().await;
    h.until_shown("the plugin timer fired").await;
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
        let mut p = h.state.profile.lock().await;
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
        .profile
        .lock()
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
        let mut p = h.state.profile.lock().await;
        for (id, command) in [(1, "tar goblin"), (2, "#prompt default")] {
            p.timers.push(crate::profile::live::Timer {
                id,
                name: String::new(),
                interval_secs: 1,
                command: command.into(),
                enabled: true,
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
        .profile
        .lock()
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
        h.state.session.try_lock().is_ok_and(|s| {
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
    )
    .await;
    assert_eq!(sent, Ok(()), "look finds no session to send to");
    h.until_shown("[not connected]").await;
    assert!(
        h.state.session.lock().await.is_none(),
        "the ended session leaves the app state"
    );
    h.finish(grid).await;
}

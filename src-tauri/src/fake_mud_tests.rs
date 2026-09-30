//! The real session against the fake Aabahran of the test kit, over a
//! local port, in the new build and in the older build (the phase 3 gate
//! of the prompt editor build spec).
//!
//! Each test runs the session loop, the typed input path and the prompt
//! lookup with the mock runtime, and serves the fake game on a port of
//! its own. What the webview would hear arrives through the mock app's
//! listeners, and a native grid replays the output the way the terminal
//! shows it. The profile folder and the log live in a temporary folder.

use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use serde_json::Value as Json;
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, Listener, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use vosh_prompt::testkit::mud::{PROMPT, PROMPT_ALL};
use vosh_prompt::testkit::{Build, Mud, Options};

use crate::commands::{AppState, SharedState};
use crate::profile_set::{AutoMatch, ProfileSet, DEFAULT_PROFILE_NAME};

/// The events the tests read, as the webview would hear them.
const EVENTS: [&str; 5] = [
    "session://output",
    "session://game-prompt-seen",
    "session://prompt-status",
    "session://hidden",
    "session://state",
];

/// What the prompts off status says in `#prompt`.
const PROMPTS_OFF: &str =
    "You turned prompts off in the game. Type prompt in the game to turn them back on.";

/// The setting `prompt x` types in these tests, and what the game stores.
const TYPED_X: &str = "<%h/%Hhp %m/%Mmn>";
const PROMPT_X: &str = "<%h/%Hhp %m/%Mmn> ";

/// Serve the fake game on a local port until the test ends. Each
/// connection plays the options `options` holds when it connects.
async fn serve_fake(options: Arc<StdMutex<Options>>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a local port");
    let port = listener.local_addr().expect("an address").port();
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let options = options.lock().expect("the options").clone();
            tokio::spawn(play(socket, options));
        }
    });
    port
}

/// One connection to the fake game, as `examples/fake_mud.rs` plays it.
async fn play(mut socket: TcpStream, options: Options) -> std::io::Result<()> {
    socket.set_nodelay(true)?;
    let mut mud = Mud::new(options);
    socket.write_all(&mud.greeting()).await?;
    let mut buf = [0u8; 4096];
    loop {
        let n = socket.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
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
}

impl Harness {
    /// A fake game of `build` on a port of its own, and an app whose
    /// profiles claim Tester (default) and Healer there, with a log.
    async fn new(options: Options) -> Self {
        let fake = Arc::new(StdMutex::new(options));
        let port = serve_fake(fake.clone()).await;
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
        }
    }

    /// Give the live profile the prompt table `config`, as a load does.
    async fn set_prompt(&self, config: vosh_prompt::PromptConfig) {
        self.state.profile.lock().await.set_prompt_config(config);
    }

    /// Connect to the fake game the way `session_connect` does, with no
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
            "127.0.0.1".into(),
            self.port,
            false,
            state.profile.clone(),
            state.map.clone(),
            state.script_timers.clone(),
            state.logs.clone(),
            state.scrollback.clone(),
            None,
            (100, 40),
        )
        .await
        .expect("the fake game answers");
        *state.session.lock().await = Some(handle);
    }

    /// Close the connection the way `session_disconnect` does.
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
    /// session it wrote, and sends it through the input path.
    async fn type_line(&self, line: &str) {
        self.heard
            .lock()
            .expect("the events")
            .push(Heard::Echo(format!("{line}\r\n")));
        if let Some(handle) = self.state.session.lock().await.as_ref() {
            let _ = handle.local_write();
        }
        crate::commands::session_send_input(
            self.app.handle().clone(),
            self.app.state(),
            line.to_string(),
        )
        .await
        .expect("the line goes out");
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
        let mut grid = crate::term_grid::TermGrid::new(100, 200);
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
    fn count_as_forsaken_lands(&self) {
        crate::session::count_as_forsaken_lands(self.port);
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
        assert_eq!(
            base64_decode(&crate::session::base64_encode(sample)),
            sample
        );
    }
}

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_new_build_gives_vosh_the_prompt_at_login_and_follows_the_game() {
    let grid = crate::term_grid::lock_shared_grid_for_test();
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
    let seen = crate::prompt_lookup::last_seen(&h.state)
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
    let seen = crate::prompt_lookup::last_seen(&h.state)
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
    let grid = crate::term_grid::lock_shared_grid_for_test();
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
    assert!(h.events("session://game-prompt-seen").is_empty());

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
    let last = crate::prompt_lookup::last_seen(&h.state)
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

// The guard keeps other tests off the shared native grid, which every
// session output also feeds. No task of the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_log_lookup_finds_only_the_prompt_of_the_profiles_own_character() {
    let grid = crate::term_grid::lock_shared_grid_for_test();
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
    let seen = crate::prompt_lookup::last_seen(&h.state)
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
    assert_eq!(crate::prompt_lookup::last_seen(&h.state).await, None);

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
    assert_eq!(crate::prompt_lookup::last_seen(&h.state).await, None);
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
    let grid = crate::term_grid::lock_shared_grid_for_test();
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
    let grid = crate::term_grid::lock_shared_grid_for_test();
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
    assert!(h.events("session://game-prompt-seen").is_empty());
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
    let seen = crate::prompt_lookup::last_seen(&h.state)
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
const DESIGN: &str = "%{c:100,100,100}[%c_reset%s_italic%hp(%c_hp%pct_hp%c_reset%s_italic%)h %mana(%{c:128,200,255}%pct_mana%c_reset%s_italic%)m %move(%{c:200,255,23}%pct_move%c_reset%s_italic%)v%c_reset%{c:100,100,100}] %c_reset";

/// What his design draws at full health.
const DRAWN: &str = "[1020(100%)h 800(100%)m 930(100%)v]";

/// The prompt James typed after the move, and what the game stores.
const TYPED_NEW: &str = "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv (%K hp) %s [%S]>";
const PROMPT_NEW: &str = "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv (%K hp) %s [%S]> ";

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
    crate::profile_config::ProfileConfig::load(file)
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
    let grid = crate::term_grid::lock_shared_grid_for_test();
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
    let grid = crate::term_grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::Older)).await;
    h.count_as_forsaken_lands();
    h.set_prompt(migrated()).await;
    h.connect().await;

    // The pattern reads the old line and draws, and nothing switches at
    // login, since the older build sends no Char.Prompt.
    h.until_last_row(DRAWN).await;
    assert_eq!(h.prompt_table().await, migrated());
    assert!(h.events("session://game-prompt-seen").is_empty());

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
    assert!(toasts(&h).is_empty());

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
    let grid = crate::term_grid::lock_shared_grid_for_test();
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
    let grid = crate::term_grid::lock_shared_grid_for_test();
    let h = Harness::new(Options::new(Build::New)).await;
    h.count_as_forsaken_lands();
    // Default reads nothing, and Healer holds the moved pattern.
    h.set_prompt(no_capture()).await;
    let healer = h.profile_file("Healer").await;
    let mut file = crate::profile_config::ProfileConfig::default();
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
    crate::commands::apply_profile_switch(h.app.handle(), &h.state, "Healer")
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
    crate::commands::apply_profile_switch(h.app.handle(), &h.state, DEFAULT_PROFILE_NAME)
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
    let grid = crate::term_grid::lock_shared_grid_for_test();
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
    assert!(toasts(&h).is_empty());
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
async fn a_reconnect_keeps_the_moved_pattern_until_the_game_sends_your_prompt() {
    let grid = crate::term_grid::lock_shared_grid_for_test();
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
    assert!(h.events("session://game-prompt-seen").is_empty());

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

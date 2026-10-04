//! The fake game the fake MUD tests play against, on a local port,
//! and the app that plays it, with what it heard and showed.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use serde_json::Value as Json;
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, Listener, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use vosh_prompt::testkit::{Mud, Options};

use crate::app::state::{AppState, SharedState};
use crate::profile::login_match::AutoMatch;
use crate::profile::set::{ProfileSet, DEFAULT_PROFILE_NAME};
use crate::sessions::SessionId;

/// The events the tests read, as the webview would hear them.
const EVENTS: [&str; 11] = [
    "session://output",
    "session://game-prompt-seen",
    "session://prompt-status",
    "session://prompt-state",
    "session://prompt-vars",
    "session://hidden",
    "session://state",
    "session://target",
    crate::app::events::TICK,
    crate::app::events::AFFECT_FULL_CHANGED,
    crate::app::events::PROMPT_CONFIG_CHANGED,
];

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
    /// An event, with the session its payload names, if any, and the
    /// rest of the payload.
    Event(&'static str, Option<SessionId>, String),
    /// Your typed line, which the webview echoes itself.
    Echo(String),
}

/// The session `payload` names, taken out of it, and the rest of the
/// payload, so a test reads what a session sent apart from whose it is.
fn named(payload: &str) -> (Option<SessionId>, String) {
    let mut json: Json = serde_json::from_str(payload).expect("a JSON payload");
    let session = json
        .as_object_mut()
        .and_then(|fields| fields.remove("session"))
        .map(|id| serde_json::from_value(id).expect("a session id"));
    (session, json.to_string())
}

/// The app with one profile folder, one log and one connection at a time.
pub(crate) struct Harness {
    pub(crate) app: App<MockRuntime>,
    pub(crate) state: SharedState,
    heard: Arc<StdMutex<Vec<Heard>>>,
    pub(crate) dir: tempfile::TempDir,
    pub(crate) port: u16,
    /// The fake game serves these options to the next connection.
    pub(crate) fake: Arc<StdMutex<Options>>,
    /// How many times a client asked the fake game for EOR.
    pub(crate) eor_asks: Arc<AtomicUsize>,
    /// Whether the fake game counts as The Forsaken Lands when the
    /// session connects.
    forsaken: AtomicBool,
}

impl Harness {
    /// A fake game of `build` on a port of its own, and an app whose
    /// profiles claim Tester (default) and Healer there, with a log.
    pub(crate) async fn new(options: Options) -> Self {
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
                let (session, payload) = named(event.payload());
                heard
                    .lock()
                    .expect("the events")
                    .push(Heard::Event(name, session, payload));
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
    pub(crate) async fn set_prompt(&self, config: vosh_prompt::PromptConfig) {
        let session = self.state.selected_session();
        let mut p = self.state.profile.lock().await;
        let mut c = session.connection.lock();
        crate::prompt::take_config(&mut p, &mut c, config);
    }

    /// Connect to the fake game the way `session::connect` does, with no
    /// scrollback file.
    pub(crate) async fn connect(&self) {
        let state = &self.state;
        let session = state.selected_session();
        if let Ok(mut g) = session.current_connection.lock() {
            *g = Some(("127.0.0.1".into(), self.port));
        }
        if let Ok(mut g) = session.current_character.lock() {
            *g = None;
        }
        let handle = crate::session::spawn(
            self.app.handle().clone(),
            state,
            &session,
            "127.0.0.1".into(),
            self.port,
            false,
            self.forsaken.load(Ordering::SeqCst),
            None,
            (100, 40),
        )
        .await
        .expect("the fake game answers");
        *session.slot.lock().await = Some(handle);
    }

    /// Close the connection the way `session::disconnect` does.
    pub(crate) async fn disconnect(&self) {
        let session = self.state.selected_session();
        let handle = session.slot.lock().await.take();
        if let Some(handle) = handle {
            handle.shutdown().await;
        }
        if let Ok(mut g) = session.current_connection.lock() {
            *g = None;
        }
        if let Ok(mut g) = session.current_character.lock() {
            *g = None;
        };
    }

    /// Type `line` and press Enter: the webview echoes it, tells the
    /// session it wrote after the newest output it took, and sends it
    /// through the input path.
    pub(crate) async fn type_line(&self, line: &str) {
        let after = self.echo(line);
        if let Some(handle) = self.state.selected_session().slot.lock().await.as_ref() {
            let _ = handle.local_write(after);
        }
        crate::ipc::session::session_send_input(
            self.app.handle().clone(),
            self.app.state(),
            line.to_string(),
            None,
        )
        .await
        .expect("the line goes out");
    }

    /// The webview echoes `line` on the terminal of the selected
    /// session. Returns the newest output of the prompt stage that
    /// terminal took before it, which the echo follows.
    pub(crate) fn echo(&self, line: &str) -> u64 {
        let shown = Some(self.state.selected_session().id);
        let mut heard = self.heard.lock().expect("the events");
        let after = heard
            .iter()
            .filter_map(|h| match h {
                Heard::Event("session://output", session, payload) if *session == shown => {
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
    pub(crate) fn events(&self, name: &str) -> Vec<Json> {
        self.heard()
            .into_iter()
            .filter_map(|h| match h {
                Heard::Event(n, _, payload) if n == name => {
                    Some(serde_json::from_str(&payload).expect("a JSON payload"))
                }
                _ => None,
            })
            .collect()
    }

    /// What the terminal of the selected session shows, 100 wide, rows
    /// trimmed.
    pub(crate) fn screen(&self) -> Vec<String> {
        let shown = Some(self.state.selected_session().id);
        let mut grid = crate::native::grid::TermGrid::new(100, 200);
        for heard in self.heard() {
            match heard {
                Heard::Echo(text) => grid.local_write(text.as_bytes()),
                Heard::Event("session://output", session, payload) if session == shown => {
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
    pub(crate) fn last_row(&self) -> String {
        self.screen().pop().unwrap_or_default()
    }

    /// Wait up to five seconds for `test` to hold.
    pub(crate) async fn until(&self, what: &str, test: impl Fn(&Self) -> bool) {
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
    pub(crate) async fn until_last_row(&self, row: &str) {
        self.until(&format!("a last row {row:?}"), |h| h.last_row() == row)
            .await;
    }

    /// Wait until a row shows `text`.
    pub(crate) async fn until_shown(&self, text: &str) {
        self.until(&format!("a row with {text:?}"), |h| {
            h.screen().iter().any(|r| r.contains(text))
        })
        .await;
    }

    /// The live profile's capture, as its file saves it.
    pub(crate) async fn capture(&self) -> vosh_prompt::CaptureConfig {
        self.state.profile.lock().await.prompt.capture.clone()
    }

    /// The live profile's whole `[prompt]` table, as its file saves it.
    pub(crate) async fn prompt_table(&self) -> vosh_prompt::PromptConfig {
        self.state.profile.lock().await.prompt.clone()
    }

    /// Have the fake game count as The Forsaken Lands, as the real host
    /// does, so a capture that reads no Aabahran codes plays by its rules.
    /// It counts from the next connect on.
    pub(crate) fn count_as_forsaken_lands(&self) {
        self.forsaken.store(true, Ordering::SeqCst);
    }

    /// Where the profile `name` keeps its file.
    pub(crate) async fn profile_file(&self, name: &str) -> std::path::PathBuf {
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
    pub(crate) async fn finish(self, grid: std::sync::MutexGuard<'static, ()>) {
        self.disconnect().await;
        drop(grid);
        tokio::time::sleep(Duration::from_millis(2_500)).await;
        drop(self.dir);
    }
}

/// A `session://output` payload as the output it carries.
pub(crate) fn output(payload: &str) -> vosh_prompt::stage::Output {
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

pub(crate) fn base64_decode(text: &str) -> Vec<u8> {
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
pub(crate) fn codes(prompt: &str) -> vosh_prompt::PromptConfig {
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
pub(crate) fn no_capture() -> vosh_prompt::PromptConfig {
    vosh_prompt::PromptConfig {
        draw: true,
        template: "<%hp>".into(),
        ..vosh_prompt::PromptConfig::default()
    }
}

/// The codes and source of an aabahran capture.
pub(crate) fn codes_of(
    capture: &vosh_prompt::CaptureConfig,
) -> (String, Option<vosh_prompt::config::CaptureSource>) {
    match capture {
        vosh_prompt::CaptureConfig::Aabahran(codes) => (codes.prompt.clone(), codes.source),
        other => panic!("no aabahran capture: {other:?}"),
    }
}

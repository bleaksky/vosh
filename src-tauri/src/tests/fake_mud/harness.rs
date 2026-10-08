//! The fake game the fake MUD tests play against, on a local port,
//! and the app that plays it, with what it heard and showed.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use serde_json::Value as Json;
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, Listener, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use vosh_prompt::testkit::{Mud, Options};

use crate::app::state::{AppState, SharedState};
use crate::profile::login_match::AutoMatch;
use crate::profile::set::{ProfileSet, DEFAULT_PROFILE_NAME};
use crate::sessions::SessionId;

/// The events the tests read, as the webview would hear them.
const EVENTS: [&str; 22] = [
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
    crate::app::events::ALERT,
    crate::app::events::MARK,
    crate::app::events::ALERTS_ENDED,
    crate::app::events::RECONNECT,
    crate::app::events::DAYLIGHT_CHANGED,
    crate::app::events::SESSIONS_CHANGED,
    crate::app::events::LUA_OUTPUT,
    crate::app::events::LUA_PANES,
    crate::app::events::VITALS_TEXT,
    crate::app::events::SNOOP,
    crate::app::events::SNOOP_OUTPUT,
];

/// What a test hands a link of the fake game that plays.
enum Push {
    /// Bytes the game writes, such as a packet or a line no command asked
    /// for.
    Bytes(Vec<u8>),
    /// Close the link with nothing more, as a reset or a dropped line
    /// does.
    Cut,
}

/// A fake game on a local port of its own.
pub(crate) struct FakeServer {
    pub(crate) port: u16,
    /// The game serves these options to the next connection.
    pub(crate) options: Arc<StdMutex<Options>>,
    /// Every byte a client sent the game, in order.
    pub(crate) received: Arc<StdMutex<Vec<u8>>>,
    /// The links that play now.
    links: Arc<StdMutex<Vec<mpsc::UnboundedSender<Push>>>>,
    /// Counts each IAC DO EOR a client sends.
    asks: Arc<AtomicUsize>,
    /// When each connection came, on the test's clock.
    pub(crate) connects: Arc<StdMutex<Vec<tokio::time::Instant>>>,
    /// The task that takes connections, while the game is up.
    accepting: StdMutex<Option<JoinHandle<()>>>,
}

impl FakeServer {
    /// Write `bytes` on every link that plays, as the game does at the
    /// end of a pulse.
    pub(crate) fn push(&self, bytes: &[u8]) {
        for link in self.links.lock().expect("the links").iter() {
            let _ = link.send(Push::Bytes(bytes.to_vec()));
        }
    }

    /// Write `bytes` on the link that came `nth`, counting from 0.
    pub(crate) fn push_to(&self, nth: usize, bytes: &[u8]) {
        if let Some(link) = self.links.lock().expect("the links").get(nth) {
            let _ = link.send(Push::Bytes(bytes.to_vec()));
        }
    }

    /// Close every link that plays with nothing more, as a reset does.
    pub(crate) fn cut(&self) {
        for link in self.links.lock().expect("the links").iter() {
            let _ = link.send(Push::Cut);
        }
    }

    /// Close the link that came `nth`, counting from 0, with nothing
    /// more, as the game does to the first link when a second one takes
    /// its character.
    pub(crate) fn cut_link(&self, nth: usize) {
        if let Some(link) = self.links.lock().expect("the links").get(nth) {
            let _ = link.send(Push::Cut);
        }
    }

    /// Stop taking connections, so each dial is refused, as while the
    /// game reboots.
    pub(crate) fn down(&self) {
        if let Some(task) = self.accepting.lock().expect("the listener").take() {
            task.abort();
        }
    }

    /// Take connections again on the same port.
    pub(crate) fn up(&self) {
        let listener = listen(self.port).expect("the port again");
        self.accept(listener);
    }

    fn accept(&self, listener: TcpListener) {
        let (options, received, links, asks, connects) = (
            self.options.clone(),
            self.received.clone(),
            self.links.clone(),
            self.asks.clone(),
            self.connects.clone(),
        );
        let task = tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                connects
                    .lock()
                    .expect("the connects")
                    .push(tokio::time::Instant::now());
                let options = options.lock().expect("the options").clone();
                let (tx, rx) = mpsc::unbounded_channel();
                links.lock().expect("the links").push(tx);
                tokio::spawn(play(socket, options, asks.clone(), received.clone(), rx));
            }
        });
        *self.accepting.lock().expect("the listener") = Some(task);
    }
}

/// A listener on `port` of the local host, 0 for any, that a later
/// listener can take again once this one goes.
fn listen(port: u16) -> std::io::Result<TcpListener> {
    let socket = TcpSocket::new_v4()?;
    socket.set_reuseaddr(true)?;
    socket.bind(std::net::SocketAddr::from(([127, 0, 0, 1], port)))?;
    socket.listen(16)
}

/// Serve the fake game on a local port until the test ends. Each
/// connection plays the options the server holds when it connects. `asks`
/// counts each IAC DO EOR a client sends.
fn serve_fake(options: Options, asks: Arc<AtomicUsize>) -> FakeServer {
    let listener = listen(0).expect("a local port");
    let server = FakeServer {
        port: listener.local_addr().expect("an address").port(),
        options: Arc::new(StdMutex::new(options)),
        received: Arc::default(),
        links: Arc::default(),
        asks,
        connects: Arc::default(),
        accepting: StdMutex::new(None),
    };
    server.accept(listener);
    server
}

/// One connection to the fake game, as `examples/fake_mud.rs` plays it,
/// with what the test pushes.
async fn play(
    mut socket: TcpStream,
    options: Options,
    asks: Arc<AtomicUsize>,
    received: Arc<StdMutex<Vec<u8>>>,
    mut pushes: mpsc::UnboundedReceiver<Push>,
) -> std::io::Result<()> {
    use vosh_prompt::testkit::mud::telnet::{DO, IAC, TELOPT_EOR};
    socket.set_nodelay(true)?;
    let mut mud = Mud::new(options);
    socket.write_all(&mud.greeting()).await?;
    let mut buf = [0u8; 4096];
    loop {
        let n = tokio::select! {
            read = socket.read(&mut buf) => read?,
            push = pushes.recv() => match push {
                Some(Push::Bytes(bytes)) => {
                    socket.write_all(&bytes).await?;
                    continue;
                }
                Some(Push::Cut) | None => return Ok(()),
            },
        };
        if n == 0 {
            return Ok(());
        }
        received
            .lock()
            .expect("the bytes")
            .extend_from_slice(&buf[..n]);
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
    /// Your typed line, which the webview echoes itself on the terminal
    /// of the session you typed it in.
    Echo(SessionId, String),
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

/// The app with one profile folder and one log, and two fake games. A
/// session holds one connection at a time.
pub(crate) struct Harness {
    pub(crate) app: App<MockRuntime>,
    pub(crate) state: SharedState,
    heard: Arc<StdMutex<Vec<Heard>>>,
    pub(crate) dir: tempfile::TempDir,
    /// Two fake games of the same options, each on a port of its own.
    pub(crate) servers: [FakeServer; 2],
    /// The first game's port and options, the ones `servers[0]` holds.
    pub(crate) port: u16,
    pub(crate) fake: Arc<StdMutex<Options>>,
    /// How many times a client asked the first game for EOR.
    pub(crate) eor_asks: Arc<AtomicUsize>,
    /// The session the app starts with. `connect`, `type_line`,
    /// `disconnect` and `screen` act on it and the first game.
    pub(crate) first: SessionId,
    /// Whether the fake games count as The Forsaken Lands when a session
    /// connects.
    forsaken: AtomicBool,
}

impl Harness {
    /// Two fake games that play `options`, each on a port of its own,
    /// and an app whose profiles claim Tester (default) and Healer on the
    /// first, with a log.
    pub(crate) async fn new(options: Options) -> Self {
        let eor_asks = Arc::new(AtomicUsize::new(0));
        let servers = [
            serve_fake(options.clone(), eor_asks.clone()),
            serve_fake(options, Arc::default()),
        ];
        let port = servers[0].port;
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
        state.set_profiles(set).await;
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
        let first = state.selected_session().id;
        Self {
            app,
            state,
            heard,
            dir,
            port,
            fake: servers[0].options.clone(),
            servers,
            eor_asks,
            first,
            forsaken: AtomicBool::new(false),
        }
    }

    /// Give the live profile the prompt table `config`, as a load does.
    pub(crate) async fn set_prompt(&self, config: vosh_prompt::PromptConfig) {
        let session = self.state.selected_session();
        let mut p = self.state.selected_profile().await;
        let mut c = session.connection.lock();
        crate::prompt::take_config(&mut p, &mut c, config);
    }

    /// Connect the first session to the first game.
    pub(crate) async fn connect(&self) {
        self.connect_to(self.first, &self.servers[0]).await;
    }

    /// Open a session after the others, as the page does.
    pub(crate) async fn open_session(&self) -> SessionId {
        crate::ipc::session::session_open(self.app.handle().clone(), self.app.state(), None)
            .await
            .expect("a new session")
    }

    /// Connect `session` to the fake game `server` the way
    /// `session::connect` does, with no scrollback file.
    pub(crate) async fn connect_to(&self, session: SessionId, server: &FakeServer) {
        let state = &self.state;
        let session = state.session(Some(session)).expect("the session");
        if let Ok(mut g) = session.current_connection.lock() {
            *g = Some(("127.0.0.1".into(), server.port));
        }
        // Where a redial dials.
        *session.address.lock().expect("the address") = Some(crate::sessions::Address {
            host: "127.0.0.1".into(),
            port: server.port,
            tls: false,
        });
        if let Ok(mut g) = session.current_character.lock() {
            *g = None;
        }
        let handle = crate::session::spawn(
            self.app.handle().clone(),
            state,
            &session,
            "127.0.0.1".into(),
            server.port,
            false,
            self.forsaken.load(Ordering::SeqCst),
            None,
            (100, 40),
        )
        .await
        .expect("the fake game answers");
        *session.slot.lock().await = Some(handle);
    }

    /// Close the first session's connection.
    pub(crate) async fn disconnect(&self) {
        self.disconnect_session(self.first).await;
    }

    /// Close the connection of `session` through `session::disconnect`,
    /// as Disconnect does.
    pub(crate) async fn disconnect_session(&self, session: SessionId) {
        let session = self.state.session(Some(session)).expect("the session");
        crate::session::disconnect(self.app.handle(), &self.state, &session).await;
    }

    /// Connect `session` to the fake game `server` through
    /// `session::connect`, as Connect does. The game counts as no world
    /// Vosh knows.
    pub(crate) async fn connect_through_vosh(&self, session: SessionId, server: &FakeServer) {
        let session = self.state.session(Some(session)).expect("the session");
        crate::session::connect(
            self.app.handle(),
            &self.state,
            &session,
            "127.0.0.1".into(),
            server.port,
            false,
        )
        .await
        .expect("the fake game answers");
    }

    /// Close `session` through `session_close`, as its row does.
    pub(crate) async fn close_session(&self, session: SessionId) {
        crate::ipc::session::session_close(self.app.handle().clone(), self.app.state(), session)
            .await
            .expect("the session closes");
    }

    /// The affect fulls of `session`, as its store keeps them.
    pub(crate) fn fulls_of(&self, session: SessionId) -> crate::affects::full::FullMap {
        let session = self.state.session(Some(session)).expect("the session");
        session.affect_full.map()
    }

    /// Type `line` in the first session and press Enter.
    pub(crate) async fn type_line(&self, line: &str) {
        self.type_in(self.first, line).await;
    }

    /// Type `line` in `session` and press Enter: the webview echoes it,
    /// tells the session it wrote after the newest output it took, and
    /// sends it through the input path.
    pub(crate) async fn type_in(&self, session: SessionId, line: &str) {
        let after = self.echo_in(session, line);
        let typed_in = self.state.session(Some(session)).expect("the session");
        if let Some(handle) = typed_in.slot.lock().await.as_ref() {
            let _ = handle.local_write(after);
        }
        crate::ipc::session::session_send_input(
            self.app.handle().clone(),
            self.app.state(),
            line.to_string(),
            Some(session),
        )
        .await
        .expect("the line goes out");
    }

    /// The webview echoes `line` on the terminal of the first session.
    pub(crate) fn echo(&self, line: &str) -> u64 {
        self.echo_in(self.first, line)
    }

    /// The webview echoes `line` on the terminal of `session`. Returns
    /// the newest output of the prompt stage that terminal took before
    /// it, which the echo follows.
    fn echo_in(&self, session: SessionId, line: &str) -> u64 {
        let mut heard = self.heard.lock().expect("the events");
        let after = heard
            .iter()
            .filter_map(|h| match h {
                Heard::Event("session://output", Some(from), payload) if *from == session => {
                    serde_json::from_str::<Json>(payload).ok()?["id"].as_u64()
                }
                _ => None,
            })
            .max()
            .unwrap_or(0);
        heard.push(Heard::Echo(session, format!("{line}\r\n")));
        after
    }

    fn heard(&self) -> Vec<Heard> {
        self.heard.lock().expect("the events").clone()
    }

    /// Every payload of the event `name`, oldest first, whichever
    /// session sent it.
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

    /// Every payload of the event `name` that names `session`, oldest
    /// first, without its session field.
    pub(crate) fn events_of(&self, session: SessionId, name: &str) -> Vec<Json> {
        self.heard()
            .into_iter()
            .filter_map(|h| match h {
                Heard::Event(n, Some(from), payload) if n == name && from == session => {
                    Some(serde_json::from_str(&payload).expect("a JSON payload"))
                }
                _ => None,
            })
            .collect()
    }

    /// What the terminal of the first session shows.
    pub(crate) fn screen(&self) -> Vec<String> {
        self.screen_of(self.first)
    }

    /// What the terminal of `session` shows, 100 wide, rows trimmed.
    pub(crate) fn screen_of(&self, session: SessionId) -> Vec<String> {
        let mut grid = crate::native::grid::TermGrid::new(100, 200);
        for heard in self.heard() {
            match heard {
                Heard::Echo(from, text) if from == session => grid.local_write(text.as_bytes()),
                Heard::Event("session://output", Some(from), payload) if from == session => {
                    grid.session_output(&output(&payload));
                }
                Heard::Echo(..) | Heard::Event(..) => {}
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
        self.state.selected_profile().await.prompt.capture.clone()
    }

    /// The live profile's whole `[prompt]` table, as its file saves it.
    pub(crate) async fn prompt_table(&self) -> vosh_prompt::PromptConfig {
        self.state.selected_profile().await.prompt.clone()
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

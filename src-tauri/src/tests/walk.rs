//! `#walk` against a fake game that moves you, over a local port.
//!
//! Each test runs the real session loop and the typed input path with the
//! mock runtime. The fake game plays four rooms of Caranduin from the
//! Map.Tiles fixtures in `fixtures/gmcp/aabahran/map`. Its Room.Info
//! carries the `num` the walker reads and the exits the tiles list for
//! that room, and it sends Map.Tiles for the two rooms the fixtures were
//! taken in. Every line it prints is the game's own: the exits line of
//! `do_exits`, the failure lines of `move_char`, the blind and dark
//! looks of `do_look`, and the prompt `prompt all` sets, as the room
//! colors fixtures print it.
//!
//! It writes as the game does for a new character. No IAC GA ends a
//! prompt, since the game sends one only for `COMM_TELNET_GA`
//! (`comm.c:1626`), which a new character lacks (`save.c:1961`) and only
//! an immortal can set (`act_wiz.c:9626`). The answer to a command starts
//! no new row (`comm.c:2111`), so it runs on from the prompt before it. Char.Prompt comes at login (`comm.c:2578`), so a
//! profile whose capture follows the game reads each prompt as it ends a
//! read, and the line after it starts clean.
//!
//! The fake answers each step as the test scripts it: it moves you, sends
//! you elsewhere, fails, goes dark, starts a fight, sits you down, says
//! nothing, or waits for the test.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use serde_json::{json, Value};
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, Listener, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;

use crate::app::state::{AppState, SharedState};

const IAC: u8 = 255;
const DONT: u8 = 254;
const DO: u8 = 253;
const WILL: u8 = 251;
const SB: u8 = 250;
const SE: u8 = 240;
const GMCP: u8 = 201;

/// The prompt `prompt all` sets, with the numbers the room colors
/// fixtures print.
const PROMPT: &str = "<1020hp 800m 930mv> ";

/// West of the City Fountain in Caranduin, where every test starts.
pub(crate) const FOUNTAIN: i64 = 4406;
/// The Common Road, one step west of the fountain.
pub(crate) const ROAD: i64 = 4405;
/// The room west of the Common Road.
const ROAD_WEST: i64 = 4404;
/// The room north of the fountain.
const NORTH_OF_FOUNTAIN: i64 = 4631;

/// The rooms of the fake game, each with the rooms its exits lead to in
/// the game's door order, as the Map.Tiles fixtures list them in `ex`.
const ROOMS: &[(i64, &[(&str, i64)])] = &[
    (
        FOUNTAIN,
        &[("north", 4631), ("south", 4633), ("west", ROAD)],
    ),
    (
        ROAD,
        &[
            ("north", 4508),
            ("east", FOUNTAIN),
            ("south", 4514),
            ("west", ROAD_WEST),
        ],
    ),
    (
        ROAD_WEST,
        &[
            ("north", 4506),
            ("east", ROAD),
            ("south", 4512),
            ("west", 4403),
        ],
    ),
    (
        NORTH_OF_FOUNTAIN,
        &[
            ("north", 4499),
            ("east", 4446),
            ("south", FOUNTAIN),
            ("west", 4510),
        ],
    ),
];

/// The exits of room `num`.
fn exits(num: i64) -> &'static [(&'static str, i64)] {
    ROOMS
        .iter()
        .find(|(room, _)| *room == num)
        .map_or(&[], |(_, exits)| exits)
}

/// The Room.Info data for room `num`: its `num` and its exits, as
/// `gmcp_send_room` writes them.
pub(crate) fn room_info(num: i64) -> Value {
    let exits: serde_json::Map<String, Value> = exits(num)
        .iter()
        .map(|(dir, to)| ((*dir).to_string(), json!(to)))
        .collect();
    json!({ "num": num, "exits": exits })
}

/// The Map.Tiles data the game sends for room `num`, for the two rooms
/// the fixtures were taken in.
pub(crate) fn map_tiles(num: i64) -> Option<Value> {
    let name = match num {
        FOUNTAIN => "caranduin-west-of-the-fountain",
        ROAD => "caranduin-the-common-road",
        _ => return None,
    };
    let path = format!(
        "{}/../fixtures/gmcp/aabahran/map/{name}.gmcp",
        env!("CARGO_MANIFEST_DIR")
    );
    let raw = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let msg = vosh_protocol::gmcp::parse(&raw).expect("the fixture parses");
    assert_eq!(msg.package, "Map.Tiles");
    Some(msg.data)
}

/// The Char.Prompt the game sends at login for `prompt all`, from the
/// fixture.
fn char_prompt() -> Vec<u8> {
    let path = format!(
        "{}/../fixtures/gmcp/aabahran/char-prompt.gmcp",
        env!("CARGO_MANIFEST_DIR")
    );
    let raw = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let msg = vosh_protocol::gmcp::parse(&raw).expect("the fixture parses");
    assert_eq!(msg.package, "Char.Prompt");
    packet(&msg.package, &msg.data)
}

/// One GMCP packet on the wire.
fn packet(package: &str, data: &Value) -> Vec<u8> {
    let mut out = vec![IAC, SB, GMCP];
    out.extend_from_slice(format!("{package} {data}").as_bytes());
    out.extend_from_slice(&[IAC, SE]);
    out
}

/// What the fake game does with the next step it reads.
#[derive(Clone)]
enum Answer {
    /// Moves you when the room has the exit, as `move_char` does, and
    /// says you cannot go when it has none.
    Go,
    /// Moves you to this room instead, as misdirection does.
    Elsewhere(i64),
    /// Prints this line and leaves you where you stand.
    Fail(&'static str),
    /// Moves you into a dark room.
    Dark,
    /// Moves you while you are blind.
    Blind,
    /// Moves you, and someone attacks you as you arrive.
    Fight,
    /// Moves you, and you end up sitting.
    Sit,
    /// Says nothing at all and leaves you where you stand.
    Silent,
    /// Waits until the test lets it go, then moves you.
    Wait(Arc<Notify>),
}

/// The fake game's side of the connection.
#[derive(Default)]
struct World {
    here: i64,
    /// How the next steps go. A step past the end moves you.
    answers: VecDeque<Answer>,
    /// Every command the game read, in order.
    heard: Vec<String>,
    fighting: bool,
    sitting: bool,
}

/// What the game reads.
enum Input {
    /// The client agreed to GMCP, which logs you in.
    Login,
    Command(String),
}

impl World {
    /// What the game writes for `input`, and what to wait for first.
    fn answer(&mut self, input: Input) -> (Option<Arc<Notify>>, Answer, Option<char>) {
        match input {
            Input::Login => (None, Answer::Go, None),
            Input::Command(text) => {
                self.heard.push(text.clone());
                let mut letters = text.chars();
                let dir = letters
                    .next()
                    .filter(|_| letters.next().is_none())
                    .filter(|c| "neswud".contains(*c));
                let Some(dir) = dir else {
                    return (None, Answer::Silent, None);
                };
                let answer = self.answers.pop_front().unwrap_or(Answer::Go);
                match answer {
                    Answer::Wait(notify) => (Some(notify), Answer::Go, Some(dir)),
                    answer => (None, answer, Some(dir)),
                }
            }
        }
    }

    /// The bytes of the answer to a step `dir`, or of the login when
    /// there is no step, after any wait.
    fn write(&mut self, answer: Answer, dir: Option<char>) -> Vec<u8> {
        let Some(dir) = dir else {
            return match answer {
                // A command that is no step gets its prompt.
                Answer::Silent => self.pulse(Vec::new()),
                // The login look, then Char.Prompt.
                _ => {
                    let mut out = self.look(self.here);
                    out.extend(char_prompt());
                    self.pulse(out)
                }
            };
        };
        if self.fighting {
            return self.say("No way!  You are still fighting!");
        }
        if self.sitting {
            return self.say("Better stand up first.");
        }
        let to = exits(self.here)
            .iter()
            .find(|(word, _)| word.starts_with(dir))
            .map(|(_, to)| *to);
        match answer {
            Answer::Silent => Vec::new(),
            Answer::Fail(line) => self.say(line),
            _ if to.is_none() && !matches!(answer, Answer::Elsewhere(_)) => {
                self.say("Alas, you cannot go that way.")
            }
            Answer::Elsewhere(num) => {
                self.here = num;
                self.arrive(num)
            }
            Answer::Dark | Answer::Blind => {
                self.here = to.unwrap_or(self.here);
                let line = if matches!(answer, Answer::Dark) {
                    "It is pitch black ... "
                } else {
                    "You can't see a thing!"
                };
                self.say(line)
            }
            Answer::Fight | Answer::Sit | Answer::Go | Answer::Wait(_) => {
                self.fighting = matches!(answer, Answer::Fight);
                self.sitting = matches!(answer, Answer::Sit);
                let to = to.unwrap_or(self.here);
                self.here = to;
                self.arrive(to)
            }
        }
    }

    /// The pulse after a step that reaches room `num`: its look.
    fn arrive(&mut self, num: i64) -> Vec<u8> {
        let look = self.look(num);
        self.pulse(look)
    }

    /// The look in room `num`: its tiles and Room.Info, then its exits
    /// line, as `do_look` sends them.
    fn look(&self, num: i64) -> Vec<u8> {
        let mut out = Vec::new();
        if let Some(tiles) = map_tiles(num) {
            out.extend(packet("Map.Tiles", &tiles));
        }
        out.extend(packet("Room.Info", &room_info(num)));
        let words: Vec<&str> = exits(num).iter().map(|(word, _)| *word).collect();
        out.extend_from_slice(format!("[Exits: {}]\n\r", words.join(" ")).as_bytes());
        out
    }

    /// The pulse after a command that prints `line` and nothing else.
    fn say(&self, line: &str) -> Vec<u8> {
        self.pulse(format!("{line}\n\r").into_bytes())
    }

    /// One pulse: what the command wrote, then a blank line, the prompt,
    /// and Char.Combat and Char.State, which each prompt sends after it,
    /// as `process_output` writes them (`comm.c:1621` to `1627`).
    fn pulse(&self, wrote: Vec<u8>) -> Vec<u8> {
        let mut out = wrote;
        out.extend_from_slice(b"\n\r");
        out.extend_from_slice(PROMPT.as_bytes());
        let combat = if self.fighting {
            json!({"target": "a Blackwatch guard", "condition": "quite a few wounds", "hp_pct": 54})
        } else {
            json!({})
        };
        out.extend(packet("Char.Combat", &combat));
        let position = if self.sitting { "sitting" } else { "standing" };
        out.extend(packet(
            "Char.State",
            &json!({"position": position, "language": "common"}),
        ));
        out
    }
}

/// Serve the fake game for one connection on a local port.
async fn serve(world: Arc<StdMutex<World>>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a local port");
    let port = listener.local_addr().expect("an address").port();
    tokio::spawn(async move {
        if let Ok((socket, _)) = listener.accept().await {
            let _ = play(socket, world).await;
        }
    });
    port
}

/// One connection to the fake game. IAC DO GMCP logs you in, and each
/// line after it is a command.
async fn play(mut socket: TcpStream, world: Arc<StdMutex<World>>) -> std::io::Result<()> {
    socket.set_nodelay(true)?;
    socket.write_all(&[IAC, WILL, GMCP]).await?;
    let mut input = Vec::new();
    let mut line = Vec::new();
    let mut gmcp = false;
    let mut buf = [0u8; 4096];
    loop {
        let n = socket.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        input.extend_from_slice(&buf[..n]);
        let mut read = Vec::new();
        let mut i = 0;
        while i < input.len() {
            let byte = input[i];
            if byte == IAC {
                match input.get(i + 1).copied() {
                    None => break,
                    Some(SB) => {
                        let Some(end) = input[i..].windows(2).position(|w| w == [IAC, SE]) else {
                            break;
                        };
                        i += end + 2;
                    }
                    Some(verb @ WILL..=DONT) => {
                        let Some(&option) = input.get(i + 2) else {
                            break;
                        };
                        if verb == DO && option == GMCP && !gmcp {
                            gmcp = true;
                            read.push(Input::Login);
                        }
                        i += 3;
                    }
                    Some(_) => i += 2,
                }
                continue;
            }
            i += 1;
            if byte == b'\n' {
                let text = String::from_utf8_lossy(&line)
                    .trim_end_matches('\r')
                    .to_string();
                line.clear();
                read.push(Input::Command(text));
            } else {
                line.push(byte);
            }
        }
        input.drain(..i);
        for each in read {
            let (wait, answer, dir) = world.lock().expect("the world").answer(each);
            if let Some(notify) = wait {
                notify.notified().await;
            }
            let bytes = world.lock().expect("the world").write(answer, dir);
            socket.write_all(&bytes).await?;
        }
    }
}

/// What the terminal showed, in order.
#[derive(Clone)]
enum Shown {
    /// A `session://output` payload.
    Output(String),
    /// Your typed line, which the webview echoes itself.
    Echo(String),
}

/// The app, one fake game and one connection to it.
struct Harness {
    app: App<MockRuntime>,
    state: SharedState,
    shown: Arc<StdMutex<Vec<Shown>>>,
    /// Every `session://walk` payload the page heard.
    walks: Arc<StdMutex<Vec<Value>>>,
    world: Arc<StdMutex<World>>,
    port: u16,
    /// The folder the session log lives in.
    _dir: tempfile::TempDir,
}

impl Harness {
    /// A fake game that starts you at the fountain, and an app not yet
    /// connected, whose capture follows the game's prompt settings. It
    /// loads no profile set, so a save writes nothing, and keeps a log in
    /// a temporary folder.
    async fn new() -> Self {
        let h = Self::unread().await;
        let session = h.state.selected_session();
        crate::prompt::take_config(
            &mut *h.state.selected_profile().await,
            &mut session.connection.lock(),
            vosh_prompt::PromptConfig {
                capture: vosh_prompt::CaptureConfig::Aabahran(
                    vosh_prompt::config::AabahranCapture::default(),
                ),
                ..vosh_prompt::PromptConfig::default()
            },
        );
        h
    }

    /// [`Harness::new`] with a profile that reads no prompt, so each
    /// prompt waits in the session for the line that ends it.
    async fn unread() -> Self {
        let world = Arc::new(StdMutex::new(World {
            here: FOUNTAIN,
            ..World::default()
        }));
        let port = serve(world.clone()).await;
        let state: SharedState = Arc::new(AppState::default());
        let dir = tempfile::tempdir().expect("a temporary folder");
        let log = vosh_log::LogStore::open(&dir.path().join("logs.sqlite")).expect("the log");
        *state.logs.lock().await = Some(log);
        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .expect("a mock app");
        app.manage::<SharedState>(state.clone());
        let shown = Arc::new(StdMutex::new(Vec::new()));
        let heard = shown.clone();
        app.listen_any("session://output", move |event| {
            heard
                .lock()
                .expect("the outputs")
                .push(Shown::Output(event.payload().to_string()));
        });
        let walks = Arc::new(StdMutex::new(Vec::new()));
        let heard = walks.clone();
        app.listen_any("session://walk", move |event| {
            let payload = serde_json::from_str(event.payload()).expect("a walk payload");
            heard.lock().expect("the walks").push(payload);
        });
        Self {
            app,
            state,
            shown,
            walks,
            world,
            port,
            _dir: dir,
        }
    }

    /// Connect the way `session::connect` does, and wait for the look the
    /// game sends at login, and for a profile that reads the prompt, the
    /// prompt after it.
    async fn connect(&self) {
        let handle = crate::session::spawn(
            self.app.handle().clone(),
            &self.state,
            &self.state.selected_session(),
            "127.0.0.1".into(),
            self.port,
            false,
            false,
            None,
            (100, 40),
        )
        .await
        .expect("the fake game answers");
        *self.state.selected_session().slot.lock().await = Some(handle);
        self.until("the look at login", |h| h.text().contains("[Exits:"))
            .await;
        let reads = !self.state.selected_profile().await.prompt.capture.is_none();
        if !reads {
            return;
        }
        for _ in 0..1000 {
            let read = self
                .state
                .selected_session()
                .connection
                .lock()
                .prompt
                .vars
                .prompt_vars();
            if read.values().any(|value| value == "1020") {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("the prompt at login was never read");
    }

    /// The step answers the game gives, in order.
    fn script(&self, answers: impl IntoIterator<Item = Answer>) {
        self.world
            .lock()
            .expect("the world")
            .answers
            .extend(answers);
    }

    /// Type `line` and press Enter. The webview echoes it after the
    /// newest output it took, and tells the session so.
    async fn type_line(&self, line: &str) {
        let after = {
            let mut shown = self.shown.lock().expect("the outputs");
            let after = shown
                .iter()
                .filter_map(|s| match s {
                    Shown::Output(payload) => {
                        serde_json::from_str::<Value>(payload).ok()?["id"].as_u64()
                    }
                    Shown::Echo(_) => None,
                })
                .max()
                .unwrap_or(0);
            shown.push(Shown::Echo(format!("{line}\r\n")));
            after
        };
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

    /// Click a room on the map, the path to it planned from `start`.
    async fn click(&self, steps: &str, start: i64, rooms: &[i64]) {
        crate::ipc::session::session_walk_route(
            self.app.state(),
            steps.to_string(),
            start,
            rooms.to_vec(),
            None,
        )
        .await
        .expect("the path reads");
    }

    /// Wait until the page heard `walks` on `session://walk`.
    async fn until_walks(&self, walks: &[Value]) {
        self.until(&format!("the page hearing {walks:?}"), |h| {
            *h.walks.lock().expect("the walks") == walks
        })
        .await;
    }

    /// Press Esc in the command line.
    async fn escape(&self) {
        crate::ipc::session::session_walk_stop(self.app.state(), None)
            .await
            .expect("Esc reaches the session");
    }

    /// Every command the game read.
    fn heard(&self) -> Vec<String> {
        self.world.lock().expect("the world").heard.clone()
    }

    /// The room the game has you in.
    fn here(&self) -> i64 {
        self.world.lock().expect("the world").here
    }

    /// Everything the terminal got, as plain text, each output's region
    /// it replaces first, as the renderers write it, and your typed
    /// echoes.
    fn text(&self) -> String {
        let shown = self.shown.lock().expect("the outputs").clone();
        let mut bytes = Vec::new();
        for each in shown {
            let payload = match each {
                Shown::Output(payload) => payload,
                Shown::Echo(line) => {
                    bytes.extend_from_slice(line.as_bytes());
                    continue;
                }
            };
            let json: Value = serde_json::from_str(&payload).expect("an output payload");
            for part in [&json["replace"]["b64"], &json["b64"], &json["hold"]] {
                if let Some(text) = part.as_str() {
                    bytes.extend(base64_decode(text));
                }
            }
        }
        vosh_protocol::ansi::plain_text(&bytes)
    }

    /// Every line Vosh printed about walking, in order.
    fn walk_lines(&self) -> Vec<String> {
        self.text()
            .split(['\r', '\n'])
            .filter(|line| line.starts_with("[walk]") || line.starts_with("[#walk"))
            .map(str::to_string)
            .collect()
    }

    /// How many looks the terminal showed.
    fn looks(&self) -> usize {
        self.text().matches("[Exits:").count()
    }

    /// Wait for `test` to hold, a sleep of 5 ms at a time, at most a
    /// thousand times.
    async fn until(&self, what: &str, test: impl Fn(&Self) -> bool) {
        for _ in 0..1000 {
            if test(self) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!(
            "{what} never came. The game heard {:?}, and the terminal shows\n{}",
            self.heard(),
            self.text()
        );
    }

    /// Wait until the game heard `commands`.
    async fn until_heard(&self, commands: &[&str]) {
        self.until(&format!("the game hearing {commands:?}"), |h| {
            h.heard() == commands
        })
        .await;
    }

    /// Wait until Vosh printed `lines` about walking.
    async fn until_said(&self, lines: &[&str]) {
        self.until(&format!("Vosh saying {lines:?}"), |h| {
            h.walk_lines() == lines
        })
        .await;
    }

    /// End the session, and return every row its log holds.
    async fn end(&self) -> Vec<String> {
        let handle = self.state.selected_session().slot.lock().await.take();
        if let Some(handle) = handle {
            handle.shutdown().await;
        }
        let guard = self.state.logs.lock().await;
        let store = guard.as_ref().expect("the log");
        let id = store.list_sessions(0, false).expect("the sessions")[0].id;
        store
            .export_session(id, false)
            .expect("the rows")
            .lines()
            .map(str::to_string)
            .collect()
    }

    async fn finish(self) {
        let handle = self.state.selected_session().slot.lock().await.take();
        if let Some(handle) = handle {
            handle.shutdown().await;
        }
    }
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
        out.extend_from_slice(&n.to_be_bytes()[1..digits.len()]);
    }
    out
}

/// Every session output also feeds the native grid the whole process
/// shares, so a test holds the lock the grid tests take.
fn grid() -> std::sync::MutexGuard<'static, ()> {
    crate::native::grid::lock_shared_grid_for_test()
}

// The guard keeps the grid tests off the shared native grid. No task of
// the session takes it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_walk_goes_room_by_room_then_sends_what_followed_it() {
    let _grid = grid();
    let h = Harness::new().await;
    h.connect().await;
    h.type_line("#walk 2w2e;get all").await;
    h.until_heard(&["w", "w", "e", "e", "get all"]).await;
    assert_eq!(h.here(), FOUNTAIN);
    assert!(h.walk_lines().is_empty(), "{:?}", h.walk_lines());
    h.finish().await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bare_walk_says_how_many_steps_are_left() {
    let _grid = grid();
    let h = Harness::new().await;
    h.connect().await;
    h.type_line("#walk").await;
    h.until_said(&["[walk] You are not walking."]).await;
    let go = Arc::new(Notify::new());
    h.script([Answer::Wait(go.clone())]);
    h.type_line("#walk 2w").await;
    h.until_heard(&["w"]).await;
    h.type_line("#walk").await;
    h.until_said(&["[walk] You are not walking.", "[walk] 2 of 2 steps left."])
        .await;
    go.notify_one();
    h.until_heard(&["w", "w"]).await;
    h.until("the second look", |h| h.looks() == 3).await;
    h.type_line("#walk").await;
    h.until_said(&[
        "[walk] You are not walking.",
        "[walk] 2 of 2 steps left.",
        "[walk] You are not walking.",
    ])
    .await;
    h.finish().await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failure_line_stops_the_walk_and_drops_the_rest_of_the_line() {
    let _grid = grid();
    let h = Harness::new().await;
    h.connect().await;
    h.script([Answer::Go, Answer::Fail("You need a boat to go there.")]);
    h.type_line("#walk 2w2e;get all").await;
    h.until_said(&[
        "[walk] Stopped after 1 of 4 steps, so Vosh did not send the rest of the line.",
    ])
    .await;
    assert_eq!(h.heard(), ["w", "w"]);
    assert_eq!(h.here(), ROAD);

    // With no exit that way the game says so, and the walk stops there.
    h.type_line("#walk 3e2w").await;
    h.until_said(&[
        "[walk] Stopped after 1 of 4 steps, so Vosh did not send the rest of the line.",
        "[walk] Stopped after 1 of 5 steps.",
    ])
    .await;
    assert_eq!(h.heard(), ["w", "w", "e", "e"]);
    assert_eq!(h.here(), FOUNTAIN);
    h.finish().await;
}

// A profile that reads no prompt leaves each one waiting for the line
// that ends it, and the game's answer to a step runs on from it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_answer_that_runs_on_from_an_unread_prompt_still_stops_the_walk() {
    let _grid = grid();
    let h = Harness::unread().await;
    h.connect().await;
    h.script([Answer::Go, Answer::Fail("You need a boat to go there.")]);
    h.type_line("#walk 2w2e;get all").await;
    h.until_said(&[
        "[walk] Stopped after 1 of 4 steps, so Vosh did not send the rest of the line.",
    ])
    .await;
    assert_eq!(h.heard(), ["w", "w"]);
    let text = h.text();
    assert!(
        text.contains(&format!("{PROMPT}You need a boat to go there.")),
        "{text}"
    );

    h.script([Answer::Dark]);
    h.type_line("#walk e").await;
    h.until_said(&[
        "[walk] Stopped after 1 of 4 steps, so Vosh did not send the rest of the line.",
        "[walk] Stopped after 1 of 1 step. Vosh lost sight of the room.",
    ])
    .await;
    assert!(h
        .text()
        .contains(&format!("{PROMPT}It is pitch black ... ")));
    assert_eq!(h.heard(), ["w", "w", "e"]);
    h.finish().await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_room_the_tiles_did_not_promise_stops_the_walk() {
    let _grid = grid();
    let h = Harness::new().await;
    h.connect().await;
    h.script([Answer::Elsewhere(NORTH_OF_FOUNTAIN)]);
    h.type_line("#walk 2w").await;
    h.until_said(&["[walk] Stopped after 0 of 2 steps."]).await;
    assert_eq!(h.heard(), ["w"]);
    assert_eq!(h.here(), NORTH_OF_FOUNTAIN);
    h.finish().await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fight_stops_the_walk() {
    let _grid = grid();
    let h = Harness::new().await;
    h.connect().await;
    h.script([Answer::Fight]);
    h.type_line("#walk 3w").await;
    h.until_said(&["[walk] Stopped after 1 of 3 steps."]).await;
    // The second step left with the Room.Info, before the fight showed,
    // and the game refused it. Nothing went after it.
    h.until("the refusal", |h| {
        h.text().contains("No way!  You are still fighting!")
    })
    .await;
    assert_eq!(h.heard(), ["w", "w"]);
    assert_eq!(h.here(), ROAD);
    h.finish().await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_position_but_standing_stops_the_walk() {
    let _grid = grid();
    let h = Harness::new().await;
    h.connect().await;
    h.script([Answer::Sit]);
    h.type_line("#walk 3w").await;
    h.until_said(&["[walk] Stopped after 1 of 3 steps."]).await;
    h.until("the refusal", |h| {
        h.text().contains("Better stand up first.")
    })
    .await;
    assert_eq!(h.heard(), ["w", "w"]);
    h.finish().await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_dark_and_blind_looks_lose_sight_of_the_room() {
    let _grid = grid();
    let h = Harness::new().await;
    h.connect().await;
    h.script([Answer::Dark]);
    h.type_line("#walk 2w").await;
    h.until_said(&["[walk] Stopped after 1 of 2 steps. Vosh lost sight of the room."])
        .await;
    assert_eq!(h.heard(), ["w"]);
    // Vosh prints its line after the prompt that ends the dark look.
    let text = h.text();
    let dark = text.rfind("It is pitch black ... ").expect("the dark look");
    let prompt = text.rfind(PROMPT).expect("a prompt");
    let said = text.rfind("[walk] Stopped").expect("the line");
    assert!(dark < prompt && prompt < said, "{text}");

    h.script([Answer::Blind]);
    h.type_line("#walk e;get all").await;
    h.until_said(&[
        "[walk] Stopped after 1 of 2 steps. Vosh lost sight of the room.",
        "[walk] Stopped after 1 of 1 step, so Vosh did not send the rest of the line. Vosh lost sight of the room.",
    ])
    .await;
    assert_eq!(h.heard(), ["w", "e"]);
    h.finish().await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn walk_stop_and_esc_stop_the_walk() {
    let _grid = grid();
    let h = Harness::new().await;
    h.connect().await;
    let go = Arc::new(Notify::new());
    h.script([Answer::Wait(go.clone())]);
    h.type_line("#walk 2w").await;
    h.until_heard(&["w"]).await;
    h.type_line("#walk stop").await;
    h.until_said(&["[walk] Stopped after 0 of 2 steps."]).await;
    go.notify_one();
    h.until("the look after the step", |h| h.looks() == 2).await;
    assert_eq!(h.heard(), ["w"]);
    h.type_line("#walk stop").await;
    h.until_said(&[
        "[walk] Stopped after 0 of 2 steps.",
        "[walk] You are not walking.",
    ])
    .await;

    let go = Arc::new(Notify::new());
    h.script([Answer::Wait(go.clone())]);
    h.type_line("#walk 2e").await;
    h.until_heard(&["w", "e"]).await;
    h.escape().await;
    h.until_said(&[
        "[walk] Stopped after 0 of 2 steps.",
        "[walk] You are not walking.",
        "[walk] Stopped after 0 of 2 steps.",
    ])
    .await;
    go.notify_one();
    h.until("the look after the step", |h| h.looks() == 3).await;
    assert_eq!(h.heard(), ["w", "e"]);
    // Esc says nothing when you are not walking.
    h.escape().await;
    h.type_line("#walk").await;
    h.until("the answer", |h| h.walk_lines().len() == 4).await;
    assert_eq!(h.walk_lines()[3], "[walk] You are not walking.");
    h.finish().await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_command_you_send_stops_the_walk_and_a_hash_command_does_not() {
    let _grid = grid();
    let h = Harness::new().await;
    h.connect().await;
    let go = Arc::new(Notify::new());
    h.script([Answer::Wait(go.clone())]);
    h.type_line("#walk 2w").await;
    h.until_heard(&["w"]).await;
    // A slash command and a bare Enter leave the walk going. The game
    // reads the Enter once it answers the step.
    h.type_line("#echo still walking").await;
    h.type_line("").await;
    h.until("the echo", |h| h.text().contains("still walking"))
        .await;
    go.notify_one();
    h.until_heard(&["w", "", "w"]).await;
    h.until("the end of the walk", |h| h.looks() == 3).await;
    assert!(h.walk_lines().is_empty(), "{:?}", h.walk_lines());

    let go = Arc::new(Notify::new());
    h.script([Answer::Wait(go.clone())]);
    h.type_line("#walk 2e").await;
    h.until_heard(&["w", "", "w", "e"]).await;
    h.type_line("look").await;
    h.until_said(&["[walk] Stopped after 0 of 2 steps."]).await;
    go.notify_one();
    h.until_heard(&["w", "", "w", "e", "look"]).await;
    h.until("the look after the step", |h| h.looks() == 4).await;
    assert_eq!(h.heard(), ["w", "", "w", "e", "look"]);
    h.finish().await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_command_after_bare_walk_or_walk_stop_goes_out_as_you_typed_it() {
    let _grid = grid();
    let h = Harness::new().await;
    h.connect().await;
    let go = Arc::new(Notify::new());
    h.script([Answer::Wait(go.clone())]);
    h.type_line("#walk 2w").await;
    h.until_heard(&["w"]).await;
    // A command after a bare #walk stops the walk, as it does alone.
    h.type_line("#walk;look").await;
    h.until_said(&[
        "[walk] 2 of 2 steps left.",
        "[walk] Stopped after 0 of 2 steps.",
    ])
    .await;
    go.notify_one();
    h.until_heard(&["w", "look"]).await;
    h.type_line("#walk stop;look").await;
    h.until_heard(&["w", "look", "look"]).await;
    h.until_said(&[
        "[walk] 2 of 2 steps left.",
        "[walk] Stopped after 0 of 2 steps.",
        "[walk] You are not walking.",
    ])
    .await;
    // Both join the log as lines you sent.
    let log = h.end().await;
    assert_eq!(
        log.iter().filter(|row| *row == "> look").count(),
        2,
        "{log:#?}"
    );
    h.finish().await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_alias_a_macro_and_a_piece_of_a_line_each_walk() {
    let _grid = grid();
    let h = Harness::new().await;
    {
        let mut p = h.state.selected_profile().await;
        p.aliases.set(vosh_automation::alias::Alias::new(
            "road",
            "#walk w;get all",
        ));
        p.aliases.set(
            vosh_automation::alias::Alias::new("kk", "ignored").with_script("mud.send('kick')"),
        );
        p.macros.push(crate::profile::live::Macro {
            key: "F1".into(),
            command: "#walk e;kk".into(),
            group: None,
            enabled: true,
            preset: None,
        });
    }
    h.connect().await;

    // An alias walks, and what follows it waits.
    h.type_line("road").await;
    h.until_heard(&["w", "get all"]).await;
    assert_eq!(h.here(), ROAD);

    // A macro sends its command the way the command line sends a line,
    // and a script alias it holds runs once you arrive.
    let command = h.state.selected_profile().await.macros[0].command.clone();
    h.type_line(&command).await;
    h.until_heard(&["w", "get all", "e", "kick"]).await;
    assert_eq!(h.here(), FOUNTAIN);

    // A piece of a typed line walks after the pieces before it.
    h.type_line("look;#walk w;get all").await;
    h.until_heard(&["w", "get all", "e", "kick", "look", "w", "get all"])
        .await;
    assert_eq!(h.here(), ROAD);
    assert!(h.walk_lines().is_empty(), "{:?}", h.walk_lines());
    h.finish().await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_walk_takes_over_once_the_step_in_flight_lands() {
    let _grid = grid();
    let h = Harness::new().await;
    h.connect().await;
    let go = Arc::new(Notify::new());
    h.script([Answer::Wait(go.clone())]);
    h.type_line("#walk 2w;get all").await;
    h.until_heard(&["w"]).await;
    h.type_line("#walk e;say back").await;
    // The new walk waits for the step on its way, and the old one sends
    // nothing more once it lands.
    go.notify_one();
    h.until_heard(&["w", "e", "say back"]).await;
    assert_eq!(
        h.walk_lines(),
        ["[walk] Stopped after 1 of 2 steps, so Vosh did not send the rest of the line."]
    );
    assert_eq!(h.here(), FOUNTAIN);
    h.finish().await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_walk_that_arrives_as_a_new_one_waits_sends_what_it_held_first() {
    let _grid = grid();
    let h = Harness::new().await;
    h.connect().await;
    let go = Arc::new(Notify::new());
    h.script([Answer::Wait(go.clone())]);
    h.type_line("#walk w;get all").await;
    h.until_heard(&["w"]).await;
    h.type_line("#walk e;look").await;
    // The step was the walk's last, so what the walk held acts on the
    // Common Road before the new walk leaves it.
    go.notify_one();
    h.until_heard(&["w", "get all", "e", "look"]).await;
    assert_eq!(h.here(), FOUNTAIN);
    assert!(h.walk_lines().is_empty(), "{:?}", h.walk_lines());
    h.finish().await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn walk_with_no_connection_says_so() {
    let _grid = grid();
    let h = Harness::new().await;
    h.type_line("#walk").await;
    h.type_line("#walk stop").await;
    h.type_line("#walk 2w").await;
    h.until("the answers", |h| h.text().contains("[not connected]"))
        .await;
    assert_eq!(
        h.walk_lines(),
        ["[walk] You are not walking.", "[walk] You are not walking."]
    );
    h.escape().await;
    assert_eq!(h.walk_lines().len(), 2);
    h.finish().await;
}

// The clock is the test's once you are logged in, since the connect's own
// timeout would run out at once on a paused clock. The step goes out and
// the game never answers, so only the backstop ends the walk, ten seconds
// after the step left.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "current_thread")]
async fn ten_seconds_with_no_room_lose_track_of_the_walk() {
    let _grid = grid();
    let h = Harness::new().await;
    h.connect().await;
    tokio::time::pause();
    h.script([Answer::Silent]);
    h.type_line("#walk 2w;get all").await;
    h.until_heard(&["w"]).await;
    let left = tokio::time::Instant::now();
    tokio::time::sleep_until(left + Duration::from_millis(9_900)).await;
    assert!(h.walk_lines().is_empty(), "{:?}", h.walk_lines());
    tokio::time::sleep_until(left + Duration::from_millis(10_100)).await;
    h.until_said(&[
        "[walk] Stopped, so Vosh did not send the rest of the line. Vosh lost track of the walk.",
    ])
    .await;
    assert_eq!(h.heard(), ["w"]);

    // A walk with nothing after it says the line the board gives.
    h.script([Answer::Silent]);
    h.type_line("#walk e").await;
    h.until_heard(&["w", "e"]).await;
    tokio::time::sleep(Duration::from_secs(11)).await;
    h.until_said(&[
        "[walk] Stopped, so Vosh did not send the rest of the line. Vosh lost track of the walk.",
        "[walk] Stopped. Vosh lost track of the walk.",
    ])
    .await;
    h.finish().await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_click_walks_its_path_and_the_page_hears_how_it_goes() {
    let _grid = grid();
    let h = Harness::new().await;
    h.connect().await;
    let walking = |done: usize, left: &str| json!({"session": 1, "kind": "walking", "done": done, "total": 2, "left": left, "route": true});
    let idle = json!({"session": 1, "kind": "idle"});
    h.click("2w", FOUNTAIN, &[ROAD, ROAD_WEST]).await;
    h.until_heard(&["w", "w"]).await;
    h.until_walks(&[walking(0, "2w"), walking(1, "w"), idle.clone()])
        .await;
    assert_eq!(h.here(), ROAD_WEST);
    assert!(h.walk_lines().is_empty(), "{:?}", h.walk_lines());

    // The game refuses the first step back, so the walk stops there.
    h.script([Answer::Fail("You are too exhausted.")]);
    h.click("2e", ROAD_WEST, &[ROAD, FOUNTAIN]).await;
    h.until_said(&["[walk] Stopped after 0 of 2 steps."]).await;
    h.until_walks(&[
        walking(0, "2w"),
        walking(1, "w"),
        idle,
        walking(0, "2e"),
        json!({"session": 1, "kind": "stopped", "done": 0, "total": 2, "why": "plain"}),
    ])
    .await;
    assert_eq!(h.heard(), ["w", "w", "e"]);
    h.finish().await;
}

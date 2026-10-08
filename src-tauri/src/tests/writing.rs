//! The writing card's writer against a fake game with the game's own
//! line editor, over a local port.
//!
//! The fake answers as the game does. `description` prints `Your
//! description is:` and the text (`act_info.c:7623`), `description edit`
//! prints the APPEND banner and the text and waits behind `> `
//! (`olc.c:3383`, `comm.c:1583`), and inside the editor it takes `.c`,
//! `.s`, `.rl`, `.d`, `./`, `@` and plain lines as `string_add` does
//! (`olc.c:3607`). The game's own prompt comes with Char.Vitals, the
//! prompt tick (`gmcp.c:935`), and the editor's `> ` with none.

use std::fmt::Write as _;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use serde_json::{json, Value};
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, Listener, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::app::state::{AppState, SharedState};

const IAC: u8 = 255;
const DO: u8 = 253;
const WILL: u8 = 251;
const SB: u8 = 250;
const SE: u8 = 240;
const GMCP: u8 = 201;

const PROMPT: &str = "<1020hp 800m 930mv> ";

/// The fake game: your description, whether its editor is open, and
/// every line it read.
#[derive(Default)]
struct World {
    description: Vec<String>,
    editing: bool,
    heard: Vec<String>,
}

fn packet(package: &str, data: &Value) -> Vec<u8> {
    let mut out = vec![IAC, SB, GMCP];
    out.extend_from_slice(format!("{package} {data}").as_bytes());
    out.extend_from_slice(&[IAC, SE]);
    out
}

impl World {
    /// The game's prompt after `wrote`, with its tick.
    fn prompt(wrote: &str) -> Vec<u8> {
        let mut out = format!("{wrote}\n\r{PROMPT}").into_bytes();
        out.extend(packet(
            "Char.Vitals",
            &json!({"hp": 1020, "maxhp": 1020, "mana": 800, "maxmana": 800, "move": 930, "maxmove": 930}),
        ));
        out
    }

    /// The text as the game keeps it, each line ending `\n\r`.
    fn text(&self) -> String {
        self.description
            .iter()
            .fold(String::new(), |mut text, line| {
                text.push_str(line);
                text.push_str("\n\r");
                text
            })
    }

    fn answer(&mut self, line: &str) -> Vec<u8> {
        self.heard.push(line.to_string());
        if self.editing {
            return self.edit(line);
        }
        match line {
            "description" => Self::prompt(&format!("Your description is:\n\r{}", self.text())),
            "description edit" | "desc edit" => {
                self.editing = true;
                let mut out = String::from(
                    "-=======- Entering APPEND Mode -========-\n\r    Type .h on a new line for help\n\r Terminate with a ~ or @ on a blank line.\n\r-=======================================-\n\r",
                );
                out.push_str(&self.text());
                if self.description.is_empty() {
                    out.push_str("\n\r");
                }
                out.push_str("> ");
                out.into_bytes()
            }
            _ => Self::prompt("Huh?"),
        }
    }

    /// A line `string_add` takes.
    fn edit(&mut self, line: &str) -> Vec<u8> {
        let said = |text: &str| format!("{text}\n\r> ").into_bytes();
        if let Some(command) = line.strip_prefix("./ ") {
            return said(&format!("You {command}."));
        }
        if line == ".c" {
            self.description.clear();
            return said("String cleared.");
        }
        if line == ".s" {
            let mut out = String::new();
            if self.description.is_empty() {
                out.push_str("\x1b[0;35m 1\x1b[0;0m ");
            }
            for (n, held) in self.description.iter().enumerate() {
                let _ = write!(out, "\x1b[0;35m{:>2}\x1b[0;0m {held}\n\r", n + 1);
            }
            out.push_str("> ");
            return out.into_bytes();
        }
        if let Some(rest) = line.strip_prefix(".rl ") {
            let (n, text) = rest.split_once(' ').expect("a line and its text");
            let n: usize = n.parse().expect("a number");
            let text = &text[1..text.len() - 1];
            self.description[n - 1] = text.to_string();
            return said("Line replaced.");
        }
        if let Some(n) = line.strip_prefix(".d ") {
            let n: usize = n.parse().expect("a number");
            self.description.remove(n - 1);
            return said(&format!("Line {n} deleted."));
        }
        if line.starts_with('@') {
            self.editing = false;
            return Self::prompt("");
        }
        let held = if line.is_empty() { " " } else { line };
        self.description.push(held.replace('"', "'"));
        b"> ".to_vec()
    }
}

async fn serve(world: Arc<StdMutex<World>>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let port = listener.local_addr().expect("an address").port();
    tokio::spawn(async move {
        if let Ok((socket, _)) = listener.accept().await {
            let _ = play(socket, world).await;
        }
    });
    port
}

/// One connection. IAC DO GMCP logs you in at the game's prompt, and each
/// line after it is read on its own pulse.
async fn play(mut socket: TcpStream, world: Arc<StdMutex<World>>) -> std::io::Result<()> {
    socket.set_nodelay(true)?;
    socket.write_all(&[IAC, WILL, GMCP]).await?;
    let mut line = Vec::new();
    let mut buf = [0u8; 4096];
    let mut skip = 0;
    let mut in_sb = false;
    loop {
        let n = socket.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        let mut answers = Vec::new();
        for (i, &byte) in buf[..n].iter().enumerate() {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            if in_sb {
                if byte == IAC && buf.get(i + 1) == Some(&SE) {
                    in_sb = false;
                    skip = 1;
                }
                continue;
            }
            if byte == IAC {
                match buf.get(i + 1) {
                    Some(&SB) => {
                        in_sb = true;
                        skip = 1;
                    }
                    Some(&DO) if buf.get(i + 2) == Some(&GMCP) => {
                        answers.push(World::prompt("Welcome back."));
                        skip = 2;
                    }
                    _ => skip = 2,
                }
                continue;
            }
            if byte == b'\n' {
                let text = String::from_utf8_lossy(&line)
                    .trim_end_matches('\r')
                    .to_string();
                line.clear();
                answers.push(world.lock().expect("the world").answer(&text));
            } else {
                line.push(byte);
            }
        }
        for answer in answers {
            // Each line the game reads waits for its own pulse.
            tokio::time::sleep(Duration::from_millis(20)).await;
            socket.write_all(&answer).await?;
        }
    }
}

struct Harness {
    app: App<MockRuntime>,
    state: SharedState,
    world: Arc<StdMutex<World>>,
    writing: Arc<StdMutex<Vec<Value>>>,
    port: u16,
    _dir: tempfile::TempDir,
}

impl Harness {
    async fn new(description: &[&str]) -> Self {
        let world = Arc::new(StdMutex::new(World {
            description: description.iter().copied().map(String::from).collect(),
            ..World::default()
        }));
        let port = serve(world.clone()).await;
        let state: SharedState = Arc::new(AppState::default());
        let dir = tempfile::tempdir().expect("a folder");
        let log = vosh_log::LogStore::open(&dir.path().join("logs.sqlite")).expect("the log");
        *state.logs.lock().await = Some(log);
        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .expect("a mock app");
        app.manage::<SharedState>(state.clone());
        let writing = Arc::new(StdMutex::new(Vec::new()));
        let heard = writing.clone();
        app.listen_any("session://writing", move |event| {
            let payload = serde_json::from_str(event.payload()).expect("a writing payload");
            heard.lock().expect("the states").push(payload);
        });
        let h = Self {
            app,
            state,
            world,
            writing,
            port,
            _dir: dir,
        };
        let handle = crate::session::spawn(
            h.app.handle().clone(),
            &h.state,
            &h.state.selected_session(),
            "127.0.0.1".into(),
            h.port,
            false,
            false,
            None,
            (100, 40),
        )
        .await
        .expect("the fake game answers");
        *h.state.selected_session().slot.lock().await = Some(handle);
        h.until("the game's prompt", |h| h.last()["game"] == "prompt")
            .await;
        h
    }

    fn last(&self) -> Value {
        self.writing
            .lock()
            .expect("the states")
            .last()
            .cloned()
            .unwrap_or(Value::Null)
    }

    fn heard(&self) -> Vec<String> {
        self.world.lock().expect("the world").heard.clone()
    }

    async fn until(&self, what: &str, test: impl Fn(&Self) -> bool) {
        for _ in 0..1000 {
            if test(self) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!(
            "{what} never came. The game heard {:?}, and the writer said {}",
            self.heard(),
            self.last()
        );
    }

    async fn start(&self, job: Value) {
        let job = serde_json::from_value(job).expect("a job");
        crate::ipc::writing::writing_start(self.app.state(), job, None)
            .await
            .expect("the writer hears");
    }

    async fn type_line(&self, line: &str) {
        crate::ipc::session::session_send_input(
            self.app.handle().clone(),
            self.app.state(),
            line.to_string(),
            None,
        )
        .await
        .expect("the line goes out");
    }

    async fn done(&self) -> Value {
        self.until("the job's end", |h| !h.last()["done"].is_null())
            .await;
        self.last()["done"]["result"].clone()
    }

    async fn finish(self) {
        let handle = self.state.selected_session().slot.lock().await.take();
        if let Some(handle) = handle {
            handle.shutdown().await;
        }
    }
}

fn grid() -> std::sync::MutexGuard<'static, ()> {
    crate::native::grid::lock_shared_grid_for_test()
}

const OLD: [&str; 1] = ["An older text."];
const NEW: [&str; 3] = [
    "This tall elf stands with a straight back,",
    "",
    "\"her\" silver hair bound behind her.",
];

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_description_goes_through_the_editor_and_reads_back() {
    let _grid = grid();
    let h = Harness::new(&OLD).await;
    h.start(json!({"id": 7, "kind": "description", "action": "send", "lines": NEW}))
        .await;
    let done = h.done().await;
    assert_eq!(done["kind"], "sent", "{done}");
    assert_eq!(
        done["lines"],
        json!([NEW[0], "", "'her' silver hair bound behind her."])
    );
    assert_eq!(done["restore"], json!(OLD));
    assert_eq!(
        h.world.lock().expect("the world").description,
        vec![NEW[0], " ", "'her' silver hair bound behind her."]
    );
    assert_eq!(
        h.heard(),
        vec![
            "description edit",
            ".s",
            ".c",
            NEW[0],
            " ",
            NEW[2],
            ".s",
            "@",
            "description"
        ]
    );
    h.finish().await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn other_sends_wait_and_a_typed_line_goes_as_a_game_command() {
    let _grid = grid();
    let h = Harness::new(&OLD).await;
    h.start(json!({"id": 1, "kind": "description", "action": "send", "lines": NEW}))
        .await;
    h.until("the editor", |h| h.heard().contains(&".c".to_string()))
        .await;
    // A #walk step is one of the sends that wait.
    h.type_line("#walk w").await;
    h.type_line("look").await;
    h.until("the held walk", |h| h.last()["held"] == 1).await;
    h.done().await;
    h.until("the walk step", |h| {
        h.heard().last().map(String::as_str) == Some("w")
    })
    .await;
    let heard = h.heard();
    assert!(heard.contains(&"./ look".to_string()), "{heard:?}");
    assert_eq!(heard.iter().filter(|l| *l == "w").count(), 1);
    assert_eq!(h.last()["held"], 0);
    h.finish().await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_card_offers_itself_when_you_open_the_editor() {
    let _grid = grid();
    let h = Harness::new(&OLD).await;
    h.type_line("desc edit").await;
    h.until("the offer", |h| !h.last()["offer"].is_null()).await;
    assert_eq!(h.last()["editor"], "description");
    let id = h.last()["offer"]["id"].as_u64().expect("an id");
    crate::ipc::writing::writing_take(h.app.state(), id, None)
        .await
        .expect("the writer hears");
    let done = h.done().await;
    assert_eq!(done["kind"], "read", "{done}");
    assert_eq!(done["lines"], json!(OLD));
    assert_eq!(h.heard(), vec!["desc edit", "@"]);
    h.finish().await;
}

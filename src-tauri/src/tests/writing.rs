//! The writing card's writer against a fake game with the game's own
//! line editor and note board, over a local port.
//!
//! The fake answers as the game does. `description` prints `Your
//! description is:` and the text (`act_info.c:7623`), `description edit`
//! and `note edit` print the APPEND banner and the text and wait behind
//! `> ` (`olc.c:3383`, `comm.c:1583`), and inside the editor it takes
//! `.c`, `.s`, `.rl`, `.d`, `./`, `@` and plain lines as `string_add`
//! does (`olc.c:3607`). The board takes `show`, `to`, `subject`, `clear`,
//! `post` and `list` as `parse_note` does (`recycle.c`).
//!
//! The game's own prompt comes with the prompt tick (`gmcp.c:932`), and
//! the editor's `> ` with none. Where the tick lands beside the text is
//! the [`Order`] the fake plays, and the default is Aabahran's own: the
//! game writes GMCP straight to the socket (`gmcp.c:21`) and the text of
//! the pulse after it (`comm.c:1629`).

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

/// What the game prints as you take over your character again
/// (`comm.c:6611`).
const RECONNECTING: &str = "Reconnecting. Type replay to see missed tells.";

/// Where the prompt tick lands beside the text of its pulse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Order {
    /// Before the reply, each packet a write of its own, as Aabahran
    /// sends it.
    #[default]
    First,
    /// After the reply and before the prompt text, in the stream.
    Middle,
    /// After the prompt text, in the stream.
    Last,
}

const ORDERS: [Order; 3] = [Order::First, Order::Middle, Order::Last];

/// The text the game's editor is open on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Editing {
    Description,
    Note,
}

/// A note in progress, as `note_attach` starts it.
#[derive(Default)]
struct Note {
    to: String,
    subject: String,
    text: Vec<String>,
}

/// The fake game: your description, your note in progress, the notes on
/// the board, whether its editor is open, and every line it read.
#[derive(Default)]
struct World {
    order: Order,
    description: Vec<String>,
    note: Option<Note>,
    /// The board, each note's sender and subject.
    board: Vec<(String, String)>,
    editing: Option<Editing>,
    /// The link drops as the game reads `note post`, once it posted.
    cut_on_post: bool,
    /// How long the game waits before it answers the next link.
    welcome_after: Duration,
    links: usize,
    heard: Vec<String>,
    /// What `show` puts before the sender, such as an immortal's rank
    /// (`recycle.c:4535`).
    rank: &'static str,
    /// The line your prompt prints above its last, which a `%c` in it
    /// makes (`comm.c:1930`).
    above: Option<&'static str>,
}

/// What the game writes for one line it read.
enum Answer {
    /// Writes in order, each one `write` of the game.
    Writes(Vec<Vec<u8>>),
    /// The link drops.
    Cut,
}

fn packet(package: &str, data: &Value) -> Vec<u8> {
    let mut out = vec![IAC, SB, GMCP];
    out.extend_from_slice(format!("{package} {data}").as_bytes());
    out.extend_from_slice(&[IAC, SE]);
    out
}

/// The prompt tick, one packet each, as `gmcp_send_prompt_tick` sends it
/// (`gmcp.c:932`) for a character alone and out of a fight.
fn prompt_tick() -> Vec<Vec<u8>> {
    vec![
        packet(
            "Char.Vitals",
            &json!({"hp": 1020, "maxhp": 1020, "mana": 800, "maxmana": 800, "move": 930, "maxmove": 930}),
        ),
        packet(
            "Char.Worth",
            &json!({"gold": 120, "bank": 5000, "exp": 2_400_000, "tnl": 0, "trains": 3, "practices": 12, "cps": 0, "rps": 0, "cabal": "none"}),
        ),
        packet("Char.Combat", &json!({})),
        packet("Group.Info", &json!({})),
        packet(
            "Char.State",
            &json!({"position": "standing", "language": "common"}),
        ),
        packet(
            "Room.Weather",
            &json!({"sky": "cloudless", "temp": 61, "unit": "F", "region": "temperate"}),
        ),
    ]
}

/// A text as the game keeps it, each line ending `\n\r`.
fn kept(text: &[String]) -> String {
    text.iter().fold(String::new(), |mut out, line| {
        out.push_str(line);
        out.push_str("\n\r");
        out
    })
}

impl World {
    /// The game's prompt after `wrote`, with its tick in the world's
    /// order: the reply, the blank line before the prompt, the prompt.
    fn prompt(&self, wrote: &str) -> Answer {
        let reply = format!("{wrote}\n\r");
        let tick = prompt_tick();
        let prompt = match self.above {
            Some(above) => format!("{above}\n\r{PROMPT}"),
            None => PROMPT.to_string(),
        };
        Answer::Writes(match self.order {
            Order::First => {
                let mut writes = tick;
                writes.push(format!("{reply}{prompt}").into_bytes());
                writes
            }
            Order::Middle => {
                let mut out = reply.into_bytes();
                out.extend(tick.concat());
                out.extend_from_slice(prompt.as_bytes());
                vec![out]
            }
            Order::Last => {
                let mut out = format!("{reply}{prompt}").into_bytes();
                out.extend(tick.concat());
                vec![out]
            }
        })
    }

    fn said(text: &str) -> Answer {
        Answer::Writes(vec![format!("{text}\n\r> ").into_bytes()])
    }

    fn answer(&mut self, line: &str) -> Answer {
        self.heard.push(line.to_string());
        if let Some(editing) = self.editing {
            return self.edit(editing, line);
        }
        if let Some(rest) = line.strip_prefix("note ") {
            return self.board(rest);
        }
        match line {
            "description" => self.prompt(&format!(
                "Your description is:\n\r{}",
                kept(&self.description)
            )),
            "description edit" | "desc edit" => self.open(Editing::Description),
            _ => self.prompt("Huh?"),
        }
    }

    /// `string_append` on the text (`olc.c:3383`).
    fn open(&mut self, editing: Editing) -> Answer {
        self.editing = Some(editing);
        let text = kept(self.text(editing));
        let mut out = String::from(
            "-=======- Entering APPEND Mode -========-\n\r    Type .h on a new line for help\n\r Terminate with a ~ or @ on a blank line.\n\r-=======================================-\n\r",
        );
        if text.is_empty() {
            out.push_str("\n\r");
        }
        out.push_str(&text);
        out.push_str("> ");
        Answer::Writes(vec![out.into_bytes()])
    }

    fn text(&mut self, editing: Editing) -> &mut Vec<String> {
        match editing {
            Editing::Description => &mut self.description,
            Editing::Note => &mut self.note.get_or_insert_with(Note::default).text,
        }
    }

    /// `parse_note` on the note board, for the words after `note`.
    fn board(&mut self, rest: &str) -> Answer {
        let (word, argument) = rest.split_once(' ').unwrap_or((rest, ""));
        match word {
            "show" => match &self.note {
                None => self.prompt("You have no note in progress."),
                Some(note) => {
                    let shown = format!(
                        "{}Orla: {}\n\rTo: {}\n\r{}",
                        self.rank,
                        note.subject,
                        note.to,
                        kept(&note.text)
                    );
                    self.prompt(&shown)
                }
            },
            "to" => {
                self.note.get_or_insert_with(Note::default).to = argument.to_string();
                self.prompt("Ok.")
            }
            "subject" => {
                self.note.get_or_insert_with(Note::default).subject = argument.to_string();
                self.prompt("Ok.")
            }
            "clear" => {
                self.note = None;
                self.prompt("Ok.")
            }
            "edit" => {
                self.note.get_or_insert_with(Note::default);
                self.open(Editing::Note)
            }
            "post" => {
                let Some(note) = self.note.take() else {
                    return self.prompt("You have no note in progress.");
                };
                self.board.push(("Orla".to_string(), note.subject));
                if self.cut_on_post {
                    return Answer::Cut;
                }
                self.prompt("Ok.")
            }
            "list" => {
                if self.board.is_empty() {
                    return self.prompt("There are no notes for you.");
                }
                // `list from` a name prints only that sender's rows, each
                // with its number on the board, and nothing when none
                // match (`recycle.c:4013`, `4089`).
                let from = argument.strip_prefix("from ");
                let mut rows = String::new();
                for (n, (sender, subject)) in self.board.iter().enumerate() {
                    if from.is_some_and(|from| !sender.eq_ignore_ascii_case(from)) {
                        continue;
                    }
                    let _ = write!(rows, " [ {n:>3}N] {sender}: {subject}\n\r");
                }
                if rows.is_empty() {
                    return self.prompt("");
                }
                rows.truncate(rows.len() - 2);
                self.prompt(&rows)
            }
            _ => self.prompt("You can't do that."),
        }
    }

    /// A line `string_add` takes.
    fn edit(&mut self, editing: Editing, line: &str) -> Answer {
        if let Some(command) = line.strip_prefix("./ ") {
            return Self::said(&format!("You {command}."));
        }
        if line.starts_with('@') {
            self.editing = None;
            return self.prompt("");
        }
        let text = self.text(editing);
        if line == ".c" {
            text.clear();
            return Self::said("String cleared.");
        }
        if line == ".s" {
            let mut out = String::new();
            if text.is_empty() {
                out.push_str("\x1b[0;35m 1\x1b[0;0m ");
            }
            for (n, held) in text.iter().enumerate() {
                let _ = write!(out, "\x1b[0;35m{:>2}\x1b[0;0m {held}\n\r", n + 1);
            }
            out.push_str("> ");
            return Answer::Writes(vec![out.into_bytes()]);
        }
        if let Some(rest) = line.strip_prefix(".rl ") {
            let (n, held) = rest.split_once(' ').expect("a line and its text");
            let n: usize = n.parse().expect("a number");
            text[n - 1] = held[1..held.len() - 1].to_string();
            return Self::said("Line replaced.");
        }
        if let Some(n) = line.strip_prefix(".d ") {
            let n: usize = n.parse().expect("a number");
            text.remove(n - 1);
            return Self::said(&format!("Line {n} deleted."));
        }
        let held = if line.is_empty() { " " } else { line };
        text.push(held.replace('"', "'"));
        Answer::Writes(vec![b"> ".to_vec()])
    }
}

async fn serve(world: Arc<StdMutex<World>>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let port = listener.local_addr().expect("an address").port();
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let _ = play(socket, world.clone()).await;
        }
    });
    port
}

/// One connection. IAC DO GMCP logs you in at the game's prompt, and each
/// line after it is read on its own pulse.
async fn play(mut socket: TcpStream, world: Arc<StdMutex<World>>) -> std::io::Result<()> {
    socket.set_nodelay(true)?;
    socket.write_all(&[IAC, WILL, GMCP]).await?;
    let welcome_after = {
        let mut world = world.lock().expect("the world");
        world.links += 1;
        world.welcome_after
    };
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
                        tokio::time::sleep(welcome_after).await;
                        answers.push(world.lock().expect("the world").prompt(RECONNECTING));
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
            match answer {
                Answer::Writes(writes) => {
                    for write in writes {
                        socket.write_all(&write).await?;
                    }
                }
                Answer::Cut => return Ok(()),
            }
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
        Self::with(World {
            description: description.iter().copied().map(String::from).collect(),
            ..World::default()
        })
        .await
    }

    /// A harness on `world`, connected at the game's prompt.
    async fn with(world: World) -> Self {
        let world = Arc::new(StdMutex::new(world));
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
        h.connect().await;
        h.until("the game's prompt", |h| h.last()["game"] == "prompt")
            .await;
        h
    }

    /// Open a link to the game.
    async fn connect(&self) {
        let h = self;
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

    /// How the job `id` ended.
    async fn done_of(&self, id: u64) -> Value {
        self.until("the job's end", |h| h.last()["done"]["id"] == id)
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

/// The writer counts the lines the editor holds as you type into it.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_writer_counts_the_lines_you_type_into_the_editor() {
    let _grid = grid();
    let h = Harness::new(&OLD).await;
    h.type_line("description edit").await;
    h.until("the opened text", |h| h.last()["lines"] == 1).await;
    h.type_line(NEW[0]).await;
    h.type_line("").await;
    h.until("two more lines", |h| h.last()["lines"] == 3).await;
    h.type_line(".d 1").await;
    h.until("a line gone", |h| h.last()["lines"] == 2).await;
    h.type_line(".c").await;
    h.until("the text cleared", |h| h.last()["lines"] == 0)
        .await;
    h.type_line("@").await;
    h.until("the prompt", |h| h.last()["lines"].is_null()).await;
    h.finish().await;
}

/// The card opens on what the editor holds after the lines you typed.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_card_opens_on_what_the_editor_holds_now() {
    let _grid = grid();
    let h = Harness::new(&OLD).await;
    let typed = [NEW[0], "her hair bound in silver."];
    h.type_line("description edit").await;
    h.until("the opened text", |h| h.last()["lines"] == 1).await;
    for line in typed {
        h.type_line(line).await;
    }
    h.until("two more lines", |h| h.last()["lines"] == 3).await;
    crate::ipc::writing::writing_take_editor(h.app.state(), 7, None)
        .await
        .expect("the writer hears");
    let done = h.done().await;
    assert_eq!(done["kind"], "read", "{done}");
    assert_eq!(done["lines"], json!([OLD[0], typed[0], typed[1]]));
    assert_eq!(h.last()["done"]["id"], 7);
    let heard = h.heard();
    assert_eq!(heard[heard.len() - 2..], [".s", "@"], "{heard:?}");
    h.finish().await;
}

/// A description sent in each order the prompt tick can come in.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_description_goes_out_in_every_order_of_the_prompt_tick() {
    let _grid = grid();
    for order in ORDERS {
        let h = Harness::with(World {
            order,
            description: OLD.iter().copied().map(String::from).collect(),
            ..World::default()
        })
        .await;
        h.start(json!({"id": 7, "kind": "description", "action": "send", "lines": NEW}))
            .await;
        let done = h.done().await;
        assert_eq!(done["kind"], "sent", "{order:?} {done}");
        assert_eq!(done["restore"], json!(OLD), "{order:?}");
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
            ],
            "{order:?}"
        );
        h.finish().await;
    }
}

/// The note the posts write.
const NOTE: [&str; 2] = ["The gate stands open at dusk.", "Bring a lantern."];

fn post(id: u64) -> Value {
    json!({"id": id, "kind": "note", "action": "post", "lines": NOTE, "to": "all", "subject": "The Great Milieu", "name": "Orla"})
}

/// A note posted in each order the prompt tick can come in.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_note_posts_in_every_order_of_the_prompt_tick() {
    let _grid = grid();
    for order in ORDERS {
        let h = Harness::with(World {
            order,
            ..World::default()
        })
        .await;
        h.start(post(3)).await;
        let done = h.done().await;
        assert_eq!(done["kind"], "posted", "{order:?} {done}");
        assert_eq!(
            h.heard(),
            vec![
                "note show",
                "note to all",
                "note subject The Great Milieu",
                "note edit",
                ".c",
                NOTE[0],
                NOTE[1],
                ".s",
                "@",
                "note show",
                "note list from Orla",
                "note post"
            ],
            "{order:?}"
        );
        assert_eq!(
            h.world.lock().expect("the world").board,
            vec![("Orla".to_string(), "The Great Milieu".to_string())],
            "{order:?}"
        );
        h.finish().await;
    }
}

/// An immortal's note read back under a prompt of two lines, in the
/// order the game sends them: the reply, its tick, then the prompt, whose
/// first line is no line of the note.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_note_reads_back_under_a_prompt_of_two_lines() {
    let _grid = grid();
    let h = Harness::with(World {
        order: Order::Middle,
        rank: "IMP ",
        above: Some("3001"),
        ..World::default()
    })
    .await;
    let mut job = post(6);
    job["immortal"] = json!(true);
    job["lines"] = json!([NOTE[0], "", NOTE[1]]);
    h.start(job).await;
    let done = h.done().await;
    assert_eq!(done["kind"], "posted", "{done}");
    assert_eq!(h.heard().last().map(String::as_str), Some("note post"));
    h.finish().await;
}

/// A post whose link drops once `post` went out, then the find on the
/// next link, asked for before the game's first prompt, in each order.
#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_find_after_a_drop_reads_the_list_in_every_order_of_the_prompt_tick() {
    let _grid = grid();
    for order in ORDERS {
        let h = Harness::with(World {
            order,
            cut_on_post: true,
            ..World::default()
        })
        .await;
        h.start(post(4)).await;
        let done = h.done_of(4).await;
        assert_eq!(done["kind"], "dropped", "{order:?} {done}");
        assert_eq!(done["posted"], true, "{order:?} {done}");
        h.world.lock().expect("the world").welcome_after = Duration::from_millis(150);
        h.until("the drop", |h| h.last()["game"] == "unknown").await;
        h.connect().await;
        h.start(json!({"id": 5, "kind": "note", "action": "find", "lines": [], "to": "all", "subject": "The Great Milieu", "name": "Orla"}))
            .await;
        let found = h.done_of(5).await;
        assert_eq!(found["kind"], "found", "{order:?} {found}");
        assert_eq!(found["number"], 0, "{order:?} {found}");
        assert_eq!(
            h.heard().last().map(String::as_str),
            Some("note list from Orla")
        );
        assert_eq!(h.world.lock().expect("the world").links, 2);
        h.finish().await;
    }
}

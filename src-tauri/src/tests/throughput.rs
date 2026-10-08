//! Output throughput. A captured session goes
//! through the real session loop to the native grid as fast as the
//! socket takes it, and the test reports how long the grid took to show
//! all of it.
//!
//! The session is the fake Aabahran's greeting and the synthetic reads in
//! `fixtures/prompt/aabahran/wire`, which stay synthetic. It plays
//! `login-new` once, then [`CYCLES`] rounds
//! of `quiet`, `fight-tank` and `lament-new`, each round closed by the
//! numbered pulse the fake game writes for `pulses`. Nobody types, so
//! each answer starts on the row of the prompt before it. The game
//! writes it all at once and then closes, so the socket never runs dry
//! and the loop cuts its reads wherever they fall. The mock runtime
//! stands in for the app, the log lives in a temporary folder, and your
//! profile draws Vosh's default design over the PROMPT the fixtures
//! carry.
//!
//! Skipped by default. Run it with
//! `cargo test -p vosh-app --release --lib p2_ -- --ignored --nocapture`.
//! A dev build runs it too, about ten times slower. Each run times the
//! first byte the game writes to the last output the grid takes.
//!
//! The numbers alone guard nothing, so the test also holds the session
//! to what it must deliver under that load. The log keeps every row of
//! every round in order, from the greeting to the rows after the last
//! pulse, and the grid keeps every round its history holds, each one
//! the same as the others and the last one last. The grid shows each
//! prompt drawn in your design and never the game's own, so a session
//! that stops drawing fails here and never reads as a faster run.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri::{Listener, Manager};
use vosh_prompt::testkit::mud::{Build, Mud, Options, PROMPT};

use crate::app::state::{AppState, SharedState};

/// Rounds in the session, about 9.5 MB on the wire.
const CYCLES: usize = 3000;

/// Sessions the test plays. It reports each, the best and the median.
const RUNS: usize = 5;

/// The grid's size, the one the other session tests use.
const COLUMNS: usize = 100;
const ROWS: usize = 40;

/// The wire cases one round plays, in order.
const ROUND: [&str; 3] = ["quiet", "fight-tank", "lament-new"];

/// How long one session may take before the test gives up on it.
const LIMIT: Duration = Duration::from_secs(600);

/// How the game's own prompt row ends. PROMPT prints as
/// `[1020/1020hp 800/800mn 930/930mv]`.
const RAW_PROMPT_END: &str = "mv]";

/// A synthetic socket read from fixtures/prompt/aabahran/wire.
fn wire(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/../fixtures/prompt/aabahran/wire/{name}.bin",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The row the numbered pulse that closes round `i` prints.
fn pulse_row(i: usize) -> String {
    format!("Pulse {i} of {CYCLES}.")
}

/// The round a row closes, when it is a pulse row.
fn pulse_of(row: &str) -> Option<usize> {
    row.strip_prefix("Pulse ")?
        .strip_suffix(&format!(" of {CYCLES}."))?
        .parse()
        .ok()
}

/// The whole session as the game writes it.
fn captured_session() -> Vec<u8> {
    let options = Options::new(Build::New);
    let mut bytes = Mud::new(options.clone()).greeting();
    bytes.extend(wire("login-new"));
    let round: Vec<u8> = ROUND.iter().flat_map(|name| wire(name)).collect();
    let mut pulses = Mud::playing(options);
    for i in 1..=CYCLES {
        bytes.extend_from_slice(&round);
        bytes.extend(pulses.pulse_later(&pulse_row(i)));
    }
    bytes
}

/// What one session took, and what it left behind.
struct Run {
    /// From the first byte the game wrote to the last output the grid
    /// took.
    to_grid: Duration,
    /// From the first byte to the disconnected state, which comes after
    /// the last log rows are written.
    to_closed: Duration,
    outputs: usize,
    frames: usize,
    /// The session's log rows, oldest first.
    log: Vec<String>,
    /// Every row the grid holds, its history first.
    grid: Vec<String>,
}

/// Play `session` through the session loop once, from a blank grid.
async fn play(session: Arc<Vec<u8>>) -> Run {
    crate::native::grid::blank_shared_grid_for_test(COLUMNS, ROWS);
    let listener = TcpListener::bind("127.0.0.1:0").expect("a local port");
    let port = listener.local_addr().expect("an address").port();
    let dir = tempfile::tempdir().expect("a temporary folder");
    let state: SharedState = Arc::new(AppState::default());
    let log = dir.path().join("logs.sqlite");
    *state.logs.lock().await = Some(vosh_log::LogStore::open(&log).expect("the log"));
    let selected = state.selected_session();
    crate::prompt::take_config(
        &mut *state.selected_profile().await,
        &mut selected.connection.lock(),
        vosh_prompt::PromptConfig {
            capture: vosh_prompt::CaptureConfig::Aabahran(vosh_prompt::config::AabahranCapture {
                prompt: PROMPT.into(),
                ..vosh_prompt::config::AabahranCapture::default()
            }),
            // Vosh's default as your choice. A design that follows the
            // game draws the row the game sends, which `drawn` cannot
            // tell from the game's own prompt.
            ..vosh_prompt::PromptConfig::from_legacy(true, vosh_prompt::DEFAULT_DESIGN)
        },
    );

    let app = mock_builder()
        .build(mock_context(noop_assets()))
        .expect("a mock app");
    app.manage::<SharedState>(state.clone());
    // The listeners run in the session's task as it emits, so they only
    // count, and leave the payloads alone.
    let start = Instant::now();
    let outputs = Arc::new(AtomicUsize::new(0));
    let last_output = Arc::new(AtomicU64::new(0));
    {
        let (outputs, last_output) = (outputs.clone(), last_output.clone());
        app.listen_any("session://output", move |_| {
            outputs.fetch_add(1, Ordering::Relaxed);
            let at = u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);
            last_output.fetch_max(at, Ordering::Relaxed);
        });
    }
    let frames = Arc::new(AtomicUsize::new(0));
    {
        let frames = frames.clone();
        app.listen_any(crate::output::TEST_FRAME_EVENT, move |_| {
            frames.fetch_add(1, Ordering::Relaxed);
        });
    }
    let (closed_tx, closed_rx) = tokio::sync::oneshot::channel::<Instant>();
    {
        let closed_tx = std::sync::Mutex::new(Some(closed_tx));
        app.listen_any("session://state", move |event| {
            if event.payload().contains("\"disconnected\"") {
                if let Some(tx) = closed_tx.lock().expect("the sender").take() {
                    let _ = tx.send(Instant::now());
                }
            }
        });
    }

    // The game runs on threads of its own, so a busy runtime never keeps
    // it from writing and the socket never runs dry. What the client says
    // goes nowhere, so its writes never wait.
    let (go, start_writing) = std::sync::mpsc::channel::<()>();
    let game = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("a client");
        socket.set_nodelay(true).expect("no delay");
        let mut from_client = socket.try_clone().expect("a reader");
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while matches!(from_client.read(&mut buf), Ok(n) if n > 0) {}
        });
        start_writing.recv().expect("the start");
        socket.write_all(&session).expect("the session");
        socket.shutdown(Shutdown::Write).expect("the close");
    });
    // The fake Aabahran counts as The Forsaken Lands.
    let handle = crate::session::spawn(
        app.handle().clone(),
        &state,
        &selected,
        "127.0.0.1".into(),
        port,
        false,
        true,
        None,
        (COLUMNS as u16, ROWS as u16),
    )
    .await
    .expect("the game answers");
    *selected.slot.lock().await = Some(handle);

    let begin = Instant::now();
    let base = begin.duration_since(start);
    go.send(()).expect("the game waits");
    let closed = tokio::time::timeout(LIMIT, closed_rx)
        .await
        .expect("the session closed in time")
        .expect("the closed state");
    let last = Duration::from_nanos(last_output.load(Ordering::Relaxed));
    game.join().expect("the game");

    let log = {
        let guard = state.logs.lock().await;
        let store = guard.as_ref().expect("the log");
        let id = store
            .list_sessions(0, &vosh_log::Scope::default())
            .expect("the sessions")[0]
            .id;
        store
            .export_session(id, false)
            .expect("the rows")
            .lines()
            .map(str::to_string)
            .collect()
    };
    let grid = crate::native::grid::with_grid(|grid| {
        let grid = grid.expect("the grid");
        let top = -(grid.scrollback_len() as i32);
        (top..grid.screen_lines() as i32)
            .map(|line| {
                (0..grid.columns())
                    .map(|col| grid.cell_at_line(line, col).0)
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    });
    selected.slot.lock().await.take();
    Run {
        to_grid: last.saturating_sub(base),
        to_closed: closed.duration_since(begin),
        outputs: outputs.load(Ordering::Relaxed),
        frames: frames.load(Ordering::Relaxed),
        log,
        grid,
    }
}

/// The rounds `rows` holds, each from the row after one pulse row to the
/// next pulse row, with the pulse row's number taken out. Asserts the
/// pulse rows count up by one and end at the last round.
fn rounds(rows: &[String], what: &str) -> Vec<Vec<String>> {
    let pulses: Vec<(usize, usize)> = rows
        .iter()
        .enumerate()
        .filter_map(|(at, row)| pulse_of(row).map(|i| (at, i)))
        .collect();
    let numbers: Vec<usize> = pulses.iter().map(|&(_, i)| i).collect();
    let first = numbers.first().copied().unwrap_or(0);
    let want: Vec<usize> = (first..=CYCLES).collect();
    assert_eq!(
        numbers, want,
        "the {what} lost, doubled or reordered a round"
    );
    pulses
        .windows(2)
        .map(|pair| {
            let mut round = rows[pair[0].0 + 1..=pair[1].0].to_vec();
            if let Some(last) = round.last_mut() {
                *last = "Pulse".into();
            }
            round
        })
        .collect()
}

/// Every round in `rows` reads the same as the first. Returns the
/// rounds.
fn same_rounds(rows: &[String], what: &str) -> Vec<Vec<String>> {
    let rounds = rounds(rows, what);
    let first = rounds
        .first()
        .unwrap_or_else(|| panic!("no round in the {what}"));
    assert!(
        first.len() > ROUND.len(),
        "the {what} keeps too little of a round: {first:#?}"
    );
    for (at, round) in rounds.iter().enumerate() {
        assert_eq!(
            round, first,
            "a round in the {what} differs from the first, {at} rounds on"
        );
    }
    rounds
}

/// The log keeps the rows [`rounds`] leaves out, around `first`, the
/// first round it returns. The log starts with the greeting's line.
/// Round 1 sits just before the first pulse row, laid out like `first`.
/// What follows the last pulse row is how `first` starts, as after every
/// other pulse row.
fn whole_log(log: &[String], first: &[String]) {
    let greeting = vosh_prompt::testkit::shown(&Mud::new(Options::new(Build::New)).greeting());
    let greeting = greeting.lines().next().unwrap_or_default().trim_end();
    assert_eq!(
        log.first().map(String::as_str),
        Some(greeting),
        "the log lost the greeting"
    );
    let round_1 = &first[..first.len() - 1];
    let pulse_1 = log
        .iter()
        .position(|row| pulse_of(row).is_some())
        .expect("a pulse row");
    assert!(
        pulse_1 >= round_1.len() && log[pulse_1 - round_1.len()..pulse_1] == *round_1,
        "the log lost or changed round 1: {:#?}",
        &log[..=pulse_1]
    );
    let last = log
        .iter()
        .rposition(|row| pulse_of(row).is_some())
        .expect("a pulse row");
    let tail = &log[last + 1..];
    assert!(
        !tail.is_empty() && first.starts_with(tail),
        "the log lost or changed the rows after the last pulse: {tail:#?}"
    );
}

/// The grid's `round` shows each prompt drawn in your design. The game's
/// own prompt row never reaches the grid, and the grid shows rows the
/// log's `logged` round lacks, since the log keeps what the game wrote
/// and not what Vosh drew. Nothing here reads the design itself, so a
/// new default design passes too.
fn drawn(round: &[String], logged: &[String]) {
    assert!(
        PROMPT.trim_end_matches("%c").ends_with(RAW_PROMPT_END),
        "PROMPT no longer ends in {RAW_PROMPT_END}"
    );
    let raw: Vec<&String> = round
        .iter()
        .filter(|row| row.ends_with(RAW_PROMPT_END))
        .collect();
    assert!(
        raw.is_empty(),
        "the game's own prompt reached the grid: {raw:#?}"
    );
    let written: HashSet<&str> = logged.iter().map(|row| row.trim_end()).collect();
    assert!(
        round.iter().any(|row| !written.contains(row.as_str())),
        "the grid shows nothing the design drew: {round:#?}"
    );
}

#[allow(clippy::cast_precision_loss)]
fn per_second(count: usize, took: Duration) -> f64 {
    count as f64 / took.as_secs_f64()
}

#[allow(clippy::await_holding_lock)]
#[ignore = "P2 benchmark, run with --ignored"]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn p2_a_captured_session_reaches_the_grid() {
    let _grid = crate::native::grid::lock_shared_grid_for_test();
    let session = Arc::new(captured_session());
    let mut to_grid = Vec::new();
    for run in 1..=RUNS {
        let r = play(session.clone()).await;
        // The whole log, every round.
        let logged = same_rounds(&r.log, "log");
        assert_eq!(logged.len(), CYCLES - 1, "the log holds every round");
        whole_log(&r.log, &logged[0]);
        // The grid's history holds the newest rounds, each with its
        // prompts drawn.
        let shown = same_rounds(&r.grid, "grid");
        drawn(&shown[0], &logged[0]);
        assert_eq!(
            r.grid.iter().rev().find_map(|row| pulse_of(row)),
            Some(CYCLES),
            "the last round never reached the grid"
        );
        println!(
            "P2 run {run}: {} bytes, {CYCLES} rounds, to the grid {:.1} ms ({:.2} MB/s, {:.0} log rows/s), \
             closed {:.1} ms, {} outputs, {} frames, {} log rows, {} rounds on the grid",
            session.len(),
            r.to_grid.as_secs_f64() * 1e3,
            per_second(session.len(), r.to_grid) / 1e6,
            per_second(r.log.len(), r.to_grid),
            r.to_closed.as_secs_f64() * 1e3,
            r.outputs,
            r.frames,
            r.log.len(),
            shown.len(),
        );
        to_grid.push(r.to_grid);
    }
    // The best run is the steadiest number on a busy machine.
    to_grid.sort_unstable();
    let (best, mid) = (to_grid[0], to_grid[RUNS / 2]);
    println!(
        "P2 of {RUNS} runs: best {:.1} ms ({:.2} MB/s), median {:.1} ms ({:.2} MB/s)",
        best.as_secs_f64() * 1e3,
        per_second(session.len(), best) / 1e6,
        mid.as_secs_f64() * 1e3,
        per_second(session.len(), mid) / 1e6,
    );
}

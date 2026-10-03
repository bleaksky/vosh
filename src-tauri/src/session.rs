//! Per-session task. Wires the connection, the telnet parser, the line
//! accumulator, and the trigger engine together. Emits Tauri events.

mod connection;
pub(crate) mod echo;
mod gmcp_vars;
pub(crate) mod highlight_ground;
mod lines;
pub(crate) mod room_block;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::{debug, error, info, warn};
use vosh_automation::trigger::{LineResult, MatchScope};
use vosh_prompt::stage::{Block, BlockLine, End, Offer, Output};
use vosh_protocol::telnet::{
    codes as telnet_codes, option as telnet_option, Event as TelnetEvent, Negotiator, Parser,
};

use crate::app::events::{self, broadcast_list_changes, ListChanges, ListRevisions};
use crate::input;
use crate::output::{emit_counted, emit_output, emit_repaint, output_count, request_frame};
use crate::profile::Profile;
use crate::profile_config::SharedLayer;
use crate::script::{self, ApplyResult, PendingTimer, SharedTimers};
use crate::tick::TickStep;

use connection::{ConnectionError, Stream};
use echo::ServerEcho;
use lines::{Line, LineAccumulator, Partial};

const TICK_EMIT_INTERVAL: Duration = Duration::from_millis(250);

const READ_BUFFER_BYTES: usize = 8 * 1024;

const PERF_REPORT_INTERVAL: Duration = Duration::from_secs(1);

/// How long a GMCP packet that changes your prompt waits for text before
/// Vosh repaints your prompt with it, the late GMCP repaint. A prompt that
/// comes in that time draws with the packet, so the repaint never fires.
const LATE_REPAINT: Duration = Duration::from_millis(60);

/// How long after a clock piece turns to its next second Vosh repaints
/// your prompt with it, so the render never lands a hair early and draws
/// the second before.
const CLOCK_SLACK: Duration = Duration::from_millis(5);

/// Hot-path performance counters owned by the single `io_loop` task.
/// Plain `u64` fields are fine because nothing else writes to them.
/// Rolled up once per second by `report_and_reset` and emitted as
/// one `tracing::debug!` line on the `vosh::perf` target. Silent
/// under default `RUST_LOG=info`; bring it back with
/// `RUST_LOG=info,vosh::perf=debug` when revisiting the save/IO
/// audit numbers, or `RUST_LOG=vosh::perf=debug` to see only the
/// per-second rollup.
///
/// Originally landed as Phase 1 instrumentation for the save/IO
/// performance audit, kept in the code at debug level so future
/// measurements do not need to re-instrument the hot path. The
/// per-line `Instant::now()` cost is single-digit ns on macOS so
/// the counters can stay live with no measurable overhead.
#[derive(Default)]
struct PerfCounters {
    socket_reads: u64,
    bytes_in: u64,
    lines_processed: u64,
    trigger_lua_ns: u64,
    mutex_wait_ns: u64,
    mutex_acquires: u64,
    log_append_ns: u64,
    log_appends: u64,
    scrollback_push_ns: u64,
    scrollback_pushes: u64,
    output_emits: u64,
    output_emit_bytes: u64,
    gmcp_packets: u64,
    tick_emits: u64,
    routed_emits: u64,
}

impl PerfCounters {
    /// Emit a single `info!` line summarising the last second of work
    /// (or nothing at all if the session was idle) and zero the
    /// counters. Per-event averages are reported in microseconds so
    /// the user can eyeball lock contention without doing the math.
    fn report_and_reset(&mut self) {
        let any_activity = self.socket_reads > 0
            || self.lines_processed > 0
            || self.gmcp_packets > 0
            || self.tick_emits > 0;
        if !any_activity {
            return;
        }
        let div_us = |total_ns: u64, n: u64| -> u64 { total_ns.checked_div(n).unwrap_or(0) / 1000 };
        let avg_trigger_us = div_us(self.trigger_lua_ns, self.lines_processed);
        let avg_lock_us = div_us(self.mutex_wait_ns, self.mutex_acquires);
        let avg_append_us = div_us(self.log_append_ns, self.log_appends);
        let avg_sb_us = div_us(self.scrollback_push_ns, self.scrollback_pushes);
        tracing::debug!(
            target: "vosh::perf",
            reads = self.socket_reads,
            bytes = self.bytes_in,
            lines = self.lines_processed,
            avg_trigger_us,
            avg_lock_us,
            lock_acq = self.mutex_acquires,
            avg_append_us,
            appends = self.log_appends,
            avg_sb_us,
            sb_pushes = self.scrollback_pushes,
            emits = self.output_emits,
            emit_bytes = self.output_emit_bytes,
            gmcp = self.gmcp_packets,
            ticks = self.tick_emits,
            routes = self.routed_emits,
            "perf 1s"
        );
        *self = Self::default();
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum StatePayload {
    Connecting { host: String, port: u16, tls: bool },
    Connected { host: String, port: u16, tls: bool },
    Disconnected { reason: Option<String> },
}

/// Emitted on `session://target` whenever the active user target
/// changes (set, cycled, cleared, or wiped by disconnect). The
/// frontend uses this to mark the targeted char in the room info
/// chips and to drive the `TargetBar` when no Char.Combat is active.
///
/// `room_idx` is the 1-based position in the latest `Room.Chars`
/// push that matches the user's target string (substring,
/// case-insensitive). It exists so the frontend doesn't have to
/// re-implement the matching logic for the chip `>` marker —
/// short keywords like "gris" won't equality-match
/// "The Baron Grisvald" but the backend already resolved the
/// pointer via substring.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct TargetPayload {
    pub name: Option<String>,
    pub room_idx: Option<usize>,
    /// Snapshot of the current quick-key bindings (name + verb).
    /// Frontend renders them next to the target name on the
    /// `TargetBar` so the user always sees which slots are armed.
    pub quick_keys: Vec<crate::profile::QuickKey>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RoutedPayload {
    pub pane: String,
    pub text: String,
}

/// Sent when the server's WILL/WONT ECHO negotiation flips. ROM derivatives
/// use this to ask for passwords: WILL ECHO means "the server is taking
/// over echoing, so don't show what the user types"; WONT ECHO means
/// "back to normal local echo." The frontend masks the input row when
/// password=true.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct InputModePayload {
    pub password: bool,
}

/// GMCP packages we ask the server to enable in Core.Supports.Set. Char,
/// Room, and Comm cover the player view; World powers the tick timer reset
/// (Aabahran ticks fire the moment its `World.Time.hour` field advances);
/// Map carries the server-rendered tile grid for the map pane's server
/// mode; Imm.Queues carries the staff work-queue counters the imm panel
/// renders (the server only sends it to immortals, so declaring it costs
/// mortals nothing). Group carries the roster the Group pane shows.
/// Aabahran sends every package without this list, so it names them
/// for servers that honor it.
const REQUESTED_GMCP_PACKAGES: &[&str] = &[
    "Char 1",
    "Room 1",
    "Comm 1",
    "World 1",
    "Map 1",
    "Imm.Queues 1",
    "Group 1",
];

/// Bytes flowing to the server. The frontend echoes typed commands into the
/// terminal pane synchronously, so the `io_loop` only writes to the wire.
pub(crate) enum OutgoingMsg {
    /// Bytes for the wire. `masked` is true for a line typed into the
    /// masked password field, which the session log hides even when the
    /// server has already handed echo back.
    Send { bytes: Vec<u8>, masked: bool },
    /// Update the advertised terminal size and, if NAWS has already
    /// been negotiated, push a fresh NAWS subnegotiation to the wire
    /// so the server re-wraps its output at the new width.
    WindowSize { cols: u16, rows: u16 },
    /// Repaint the open row as the profile's `[prompt]` table now says,
    /// as a change to the switch or the design asks. Only the session
    /// task writes session output, so a command sends this instead.
    PromptRepaint,
    /// The webview wrote to the terminal itself, such as your typed echo
    /// or an error notice, so the open row is no longer the last thing on
    /// screen. `after` is the newest output of the prompt stage that the
    /// renderer that shows took before the text (`Output::id`), since
    /// the session can hear of the text after it wrote more.
    LocalWrite { after: u64 },
}

/// Everything one socket read writes to the terminal and reports, kept
/// in stream order and sent once at the end of the read, so a prompt
/// that arrives in one read never flashes. The Line pass, the GMCP
/// handler's echoes and the GA path all write here.
struct ReadBatch {
    /// The terminal output, with the regions the prompt stage marks.
    out: Output,
    /// Log rows, written in one transaction once the socket is quiet.
    log: Vec<vosh_log::LogEntry>,
    /// A prompt var changed or a prompt was read, so the prompt vars go
    /// out after the output even when they read the same.
    prompt_vars: bool,
    /// Vosh read your prompt in this read, so the prompt state goes out
    /// after it while the card watches.
    prompt: bool,
    /// Triggers that hid a prompt while nothing reads it, each named
    /// once a session.
    gag_without_reader: Vec<String>,
    /// The character Char.Status named in this read, for the log's
    /// session row.
    character: Option<String>,
    /// The read ended on a partial that can still become your prompt, so
    /// it waits a moment for the next read instead of painting raw.
    hold: bool,
    /// The read brought GMCP packets after the last prompt Vosh read in
    /// it, which can change what that prompt shows. Packets before a
    /// prompt in the same read draw with it.
    gmcp: bool,
}

impl ReadBatch {
    /// A batch for the next read. `seen` is the output count after the
    /// session last wrote, so output from elsewhere since then closes the
    /// open row.
    fn new(seen: u64) -> Self {
        Self {
            out: Output::new(output_count() != seen),
            log: Vec::new(),
            prompt_vars: false,
            prompt: false,
            gag_without_reader: Vec::new(),
            character: None,
            hold: false,
            gmcp: false,
        }
    }
}

/// How long a burst of reads can go on with no frame, and how long a log
/// row waits in it. A game that never pauses still shows its output and
/// gets its rows written this often, while the log is free.
const FRAME_BUDGET: Duration = Duration::from_millis(16);

/// What the reads of a burst owe once the socket has nothing more for
/// now: one frame for all their output, then their log rows and the rows
/// of the lines you sent between them, in the order they passed. An
/// answer the game's writes cut into reads, such as a prompt whose GA
/// comes in the next read, shows in one frame, and the log write never
/// sits between those reads or ahead of the frame. A burst that never
/// ends still owes the frame once output waited [`FRAME_BUDGET`] for it,
/// and the rows once the oldest of them waited as long, drawn or not.
#[derive(Default)]
struct Settle {
    /// Output went to the grid since the last frame was asked for.
    frame: bool,
    /// When the first output with no frame yet went to the grid.
    since: Option<Instant>,
    /// Log rows waiting for the log, oldest first.
    log: Vec<vosh_log::LogEntry>,
    /// When the oldest row waiting for the log joined the queue.
    log_since: Option<Instant>,
}

impl Settle {
    /// Output went to the grid.
    fn drew(&mut self) {
        self.frame = true;
        self.since.get_or_insert_with(Instant::now);
    }

    /// Rows join the queue for the log, behind the rows before them.
    fn queue_rows(&mut self, rows: impl IntoIterator<Item = vosh_log::LogEntry>) {
        self.log.extend(rows);
        if !self.log.is_empty() {
            self.log_since.get_or_insert_with(Instant::now);
        }
    }

    /// Ask for the frame the output so far owes, if any.
    fn frame_now<R: tauri::Runtime>(&mut self, app: &AppHandle<R>) {
        if std::mem::take(&mut self.frame) {
            self.since = None;
            request_frame(app);
        }
    }

    /// Whether the burst went on so long it shows a frame now, before
    /// the socket runs dry.
    fn frame_overdue(&self) -> bool {
        self.since.is_some_and(|t| t.elapsed() >= FRAME_BUDGET)
    }

    /// Whether rows waited so long they go in the log now, before the
    /// socket runs dry, whether or not anything drew.
    fn log_overdue(&self) -> bool {
        self.log_since.is_some_and(|t| t.elapsed() >= FRAME_BUDGET)
    }

    /// What a game that never pauses is owed before the socket runs dry:
    /// the frame once output waited [`FRAME_BUDGET`] for it, and the rows
    /// once the oldest waited as long, even when nothing drew, such as
    /// the row of a line you sent or reads of GMCP alone. A busy log
    /// keeps the rows for the next quiet moment, so the loop never waits
    /// on it here.
    fn overdue_now<R: tauri::Runtime>(
        &mut self,
        app: &AppHandle<R>,
        logs: &crate::logs::SharedLogStore,
        perf: &mut PerfCounters,
    ) {
        if self.frame_overdue() {
            self.frame_now(app);
        }
        if self.log_overdue() {
            if let Ok(mut guard) = logs.try_lock() {
                self.write_log(guard.as_mut(), perf);
            }
        }
    }

    /// Write the waiting rows to the log, in one transaction.
    fn write_log(&mut self, store: Option<&mut vosh_log::LogStore>, perf: &mut PerfCounters) {
        self.log_since = None;
        let rows = std::mem::take(&mut self.log);
        if rows.is_empty() {
            return;
        }
        if let Some(store) = store {
            let append_t0 = std::time::Instant::now();
            perf.log_appends += rows.len() as u64;
            if let Err(e) = store.append_batch(&rows) {
                warn!(error = %e, "log append_batch failed");
            }
            perf.log_append_ns += append_t0.elapsed().as_nanos() as u64;
        }
    }
}

/// Where a step writes to the terminal: the batch of the read it runs
/// in, or straight out for work outside a read, such as a timer.
enum OutputSink<'a> {
    Batch(&'a mut ReadBatch),
    Direct,
}

impl OutputSink<'_> {
    /// Write `bytes` to the terminal.
    fn write<R: tauri::Runtime>(&mut self, app: &AppHandle<R>, bytes: Vec<u8>) {
        match self {
            OutputSink::Batch(batch) => batch.out.text(&bytes),
            OutputSink::Direct => emit_output(app, bytes),
        }
    }

    /// A prompt var changed. A read sends the prompt vars once after its
    /// output, and anything else sends them now.
    async fn prompt_vars<R: tauri::Runtime>(
        &mut self,
        app: &AppHandle<R>,
        profile: &Arc<Mutex<Profile>>,
    ) {
        match self {
            OutputSink::Batch(batch) => batch.prompt_vars = true,
            OutputSink::Direct => emit_prompt_vars(app, profile, true).await,
        }
    }
}

pub(crate) struct SessionHandle {
    tx_outgoing: mpsc::UnboundedSender<OutgoingMsg>,
    task: JoinHandle<()>,
}

impl SessionHandle {
    /// Send raw bytes to the connection. Returns false when the session has
    /// already been torn down.
    pub(crate) fn send(&self, bytes: Vec<u8>) -> bool {
        self.tx_outgoing
            .send(OutgoingMsg::Send {
                bytes,
                masked: false,
            })
            .is_ok()
    }

    /// Send a line typed into the masked password field. The session
    /// log keeps `> (hidden)` for it whatever the echo state is when it
    /// leaves. Returns false when the session has already been torn down.
    pub(crate) fn send_masked(&self, bytes: Vec<u8>) -> bool {
        self.tx_outgoing
            .send(OutgoingMsg::Send {
                bytes,
                masked: true,
            })
            .is_ok()
    }

    /// Push a terminal resize event into the session task so it can
    /// update the negotiator and emit a NAWS subnegotiation.
    pub(crate) fn set_window_size(&self, cols: u16, rows: u16) -> bool {
        self.tx_outgoing
            .send(OutgoingMsg::WindowSize { cols, rows })
            .is_ok()
    }

    /// Tell the session the webview wrote to the terminal itself, on a
    /// renderer whose newest output of the prompt stage was `after`. That
    /// closes the open row when the row came no later. Returns false when
    /// the session has already been torn down.
    pub(crate) fn local_write(&self, after: u64) -> bool {
        self.tx_outgoing
            .send(OutgoingMsg::LocalWrite { after })
            .is_ok()
    }

    /// Repaint the open row as the `[prompt]` table now says. Returns
    /// false when the session has already been torn down.
    pub(crate) fn prompt_repaint(&self) -> bool {
        self.tx_outgoing.send(OutgoingMsg::PromptRepaint).is_ok()
    }

    /// True once the session loop has ended, so nothing sent reaches the
    /// game. The loop says it disconnected just before it ends, so a test
    /// waits on this to know the session is gone. Such a test also
    /// accepts an empty slot, in case the ended session cleared it.
    #[cfg(test)]
    pub(crate) fn has_ended(&self) -> bool {
        self.tx_outgoing.is_closed()
    }

    pub(crate) async fn shutdown(self) {
        drop(self.tx_outgoing);
        let _ = self.task.await;
    }
}

/// Open a connection, install a parser plus negotiator, and spin up the IO
/// loop. The returned handle owns the outgoing channel; drop it to close.
///
/// `initial_window_size` is the (cols, rows) the negotiator should
/// carry into the first NAWS subnegotiation. The caller (typically
/// `session_connect`) reads this from `AppState.window_size` so the
/// server's first wrap-width decision is based on the actual
/// terminal geometry instead of the negotiator's 80×24 fallback.
///
/// `known_host` is whether the host is The Forsaken Lands, whose rules
/// the custom prompt follows. The caller says so, which lets a test have
/// a fake game on a local port count as it.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn spawn<R: tauri::Runtime>(
    app: AppHandle<R>,
    host: String,
    port: u16,
    tls: bool,
    known_host: bool,
    profile: Arc<Mutex<Profile>>,
    timers: SharedTimers,
    logs: crate::logs::SharedLogStore,
    scrollback: crate::logs::SharedScrollback,
    scrollback_path: Option<std::path::PathBuf>,
    initial_window_size: (u16, u16),
) -> Result<SessionHandle, ConnectionError> {
    emit_state(
        &app,
        StatePayload::Connecting {
            host: host.clone(),
            port,
            tls,
        },
    );

    let mut stream = connection::connect(&host, port, tls).await?;
    info!(%host, port, tls, "session connected");

    // Seed the negotiator with the size we already know about so the
    // first `DO NAWS` from the server gets a correct subneg, instead
    // of the 80×24 default carrying through until the user nudges
    // the window. Stale-NAWS was visible in `who` output wrapping
    // mid-sentence before the user reported it.
    let mut negotiator = Negotiator::new();
    negotiator.set_window_size(initial_window_size.0, initial_window_size.1);
    // Proactively ask for end-of-record so the server marks each prompt.
    // Without this, ROM derivatives that gate EOR on negotiation never
    // send the byte, and we have to merge the prompt with the next room
    // line. Other negotiations stay reactive in handle_event. The
    // negotiator keeps the ask, so the WILL EOR that answers it gets no
    // answer back (RFC 1143).
    let ask = negotiator.ask(telnet_option::EOR);
    if let Err(e) = stream.write_all(&ask).await {
        warn!(error = %e, "failed to send initial DO EOR");
    }
    let _ = stream.flush().await;

    emit_state(
        &app,
        StatePayload::Connected {
            host: host.clone(),
            port,
            tls,
        },
    );

    // Open a log session row up front so every line emitted by the loop
    // can attach to the same id. If logging is disabled or fails, the
    // io_loop just skips the appends.
    let log_session_id = {
        let mut guard = logs.lock().await;
        match guard.as_mut() {
            Some(store) => match store.start_session(&host, port, now_ms()) {
                Ok(id) => Some(id),
                Err(e) => {
                    warn!(error = %e, "failed to open log session");
                    None
                }
            },
            None => None,
        }
    };

    let (tx_outgoing, rx_outgoing) = mpsc::unbounded_channel::<OutgoingMsg>();
    let task = tokio::spawn(io_loop(
        app,
        stream,
        rx_outgoing,
        profile,
        timers,
        logs,
        log_session_id,
        scrollback,
        scrollback_path,
        negotiator,
        known_host,
    ));

    Ok(SessionHandle { tx_outgoing, task })
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

#[allow(clippy::too_many_arguments)]
async fn io_loop<R: tauri::Runtime>(
    app: AppHandle<R>,
    mut stream: Stream,
    mut rx_outgoing: mpsc::UnboundedReceiver<OutgoingMsg>,
    profile: Arc<Mutex<Profile>>,
    timers: SharedTimers,
    logs: crate::logs::SharedLogStore,
    log_session_id: Option<i64>,
    scrollback: crate::logs::SharedScrollback,
    scrollback_path: Option<std::path::PathBuf>,
    mut negotiator: Negotiator,
    known_host: bool,
) {
    let mut parser = Parser::new();
    let mut accumulator = LineAccumulator::new();
    let mut buf = vec![0u8; READ_BUFFER_BYTES];
    // Who echoes your input on this connection. Every read updates it
    // in wire order before the loop takes the next outgoing line, so
    // each send is logged by the state in force as its bytes leave.
    // It lives and dies with this loop, so a connection that dropped
    // mid password prompt hands nothing to the next one.
    let mut server_echo = ServerEcho::default();

    // Activate the tick timer for this session, unsynced until the game's
    // first tick. The user can disable it later through the slash command.
    // The prompt engine starts with no packets and the host's rules.
    {
        let mut p = profile.lock().await;
        p.tick.start_session(Instant::now());
        start_prompt(&mut p, known_host);
        // A push to the right edge reaches to the width the game is told.
        p.prompt.set_cols(usize::from(negotiator.window_size.0));
    }

    let mut tick_interval = tokio::time::interval(TICK_EMIT_INTERVAL);
    tick_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Per-timer next-fire deadlines for the Settings interval timers.
    // Seeded on first sight in fire_due_profile_timers; cleared here so
    // each connection starts its timers fresh.
    let mut timer_next: HashMap<u32, Instant> = HashMap::new();

    // The output count after this session last wrote. Output from
    // elsewhere moves it, which closes the open row.
    let mut seen_output = output_count();
    // The log's row for this connection, named once the game names the
    // character.
    let mut log_session = LogSession::new(log_session_id);
    // When a partial that can still become your prompt stops waiting for
    // the next read and paints raw.
    let mut hold_until: Option<Instant> = None;
    // When a GMCP packet that changed your prompt, with no text after it,
    // repaints it.
    let mut late_until: Option<Instant> = None;
    // When a clock piece in your design next shows another second, while
    // your design draws one.
    let mut clock_until: Option<Instant> = None;

    // The frame and the log rows the reads since the socket was last
    // quiet owe.
    let mut settle = Settle::default();

    // Phase 1 audit instrumentation. See `PerfCounters` doc.
    let mut perf = PerfCounters::default();
    let mut perf_report_interval = tokio::time::interval(PERF_REPORT_INTERVAL);
    perf_report_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let disconnect_reason = loop {
        tokio::select! {
            biased;
            outgoing = rx_outgoing.recv() => match outgoing {
                Some(OutgoingMsg::Send { bytes, masked }) => {
                    // A partial waiting for the next read paints before
                    // your line leaves, so it never goes unseen.
                    if hold_until.take().is_some() {
                        flush_hold(
                            &app,
                            &profile,
                            &mut accumulator,
                            &mut seen_output,
                            &mut settle,
                        )
                        .await;
                    }
                    // Lines held for the rest of a prompt let go as they
                    // show, before your line leaves, since it follows them.
                    if let Err(e) = let_go_held_lines(
                        &app,
                        &mut stream,
                        &profile,
                        &timers,
                        &scrollback,
                        &logs,
                        &mut log_session,
                        &mut seen_output,
                        &mut settle,
                        &mut perf,
                    )
                    .await
                    {
                        warn!(error = %e, "letting go of held lines failed");
                    }
                    // The send records a prompt candidate and closes the
                    // open row. On a server that sends no Char.Vitals it
                    // also starts the next pulse, after which the values
                    // the last prompt set go stale.
                    let pulse = {
                        let mut p = profile.lock().await;
                        send_step(&mut p, &accumulator, &bytes, now_ms())
                    };
                    // The frontend already echoed the typed line inline
                    // with the on-screen prompt. Drop the buffered partial
                    // so the next chunk from the server starts fresh on a
                    // new row instead of merging with the displayed prompt.
                    accumulator.forget_partial();
                    if pulse {
                        emit_hidden_change(&app, &profile).await;
                        emit_prompt_vars(&app, &profile, false).await;
                    }
                    // The input line(s) go in the same log session as
                    // server output so transcripts include both
                    // directions. While the server holds echo (a
                    // password prompt), and for any line typed into the
                    // masked field, each line is logged as `> (hidden)`
                    // and its text never reaches the store. See
                    // `vosh_log::sent_rows`. The rows and their
                    // time are taken as the line leaves, and they wait
                    // behind the rows before them for the log, which
                    // writes them once the socket is quiet, so a log
                    // write never holds your line or its answer back.
                    let sent = log_session_id.map(|sid| {
                        let rows = vosh_log::sent_rows(&bytes, server_echo.hides(masked));
                        (sid, now_ms(), rows)
                    });
                    let wrote = match stream.write_all(&bytes).await {
                        Err(e) => Err(("write failed", e)),
                        Ok(()) => stream.flush().await.map_err(|e| ("flush failed", e)),
                    };
                    if let Some((sid, at, rows)) = sent {
                        settle.queue_rows(vosh_log::sent_entries(sid, at, rows));
                    }
                    if let Err((what, e)) = wrote {
                        error!(error = %e, "{what}");
                        break Some(format!("{what}: {e}"));
                    }
                    // Lines sent back to back never wait on the log, but
                    // their rows still go in once they waited too long.
                    settle.overdue_now(&app, &logs, &mut perf);
                }
                Some(OutgoingMsg::WindowSize { cols, rows }) => {
                    // A design that pushes part of a row to the right
                    // edge draws again at the new width. The card hears
                    // the state with it, so its marks move with the push.
                    let (out, state) = {
                        let mut p = profile.lock().await;
                        let redraw = window_size_step(
                            &mut p,
                            &mut negotiator,
                            cols,
                            rows,
                            watching_prompt(&app),
                        );
                        let out = redraw.then(|| {
                            repaint_step(&mut p, output_count() != seen_output, Instant::now())
                        });
                        let state = out
                            .as_ref()
                            .filter(|out| !out.is_empty())
                            .and_then(|_| watched_state(&app, &p));
                        (out, state)
                    };
                    if let Some(out) = out.filter(|out| !out.is_empty()) {
                        emit_repaint(&app, &out);
                    }
                    emit_prompt_state(&app, state);
                    // Once the server sent DO NAWS and Vosh agreed, every
                    // new size goes out, so the game wraps at the new
                    // column count.
                    if negotiator.vosh_does(telnet_option::NAWS) {
                        let bytes = negotiator.naws_subnegotiation();
                        if let Err(e) = stream.write_all(&bytes).await {
                            error!(error = %e, "naws write failed");
                            break Some(format!("naws write failed: {e}"));
                        }
                        if let Err(e) = stream.flush().await {
                            error!(error = %e, "naws flush failed");
                            break Some(format!("naws flush failed: {e}"));
                        }
                    }
                }
                Some(OutgoingMsg::LocalWrite { after }) => {
                    // The webview echoes your line and sends it with two
                    // calls, so the session can hear of the echo after the
                    // game answered. Text that landed before output the
                    // session already sent leaves that output alone: its
                    // open row, its pinned prompt and what it holds all
                    // came after the text.
                    let landed_last = !profile.lock().await.prompt.stage.wrote_after(after);
                    if landed_last {
                        if hold_until.take().is_some() {
                            flush_hold(
                                &app,
                                &profile,
                                &mut accumulator,
                                &mut seen_output,
                                &mut settle,
                            )
                            .await;
                        }
                        // Your typed echo follows the lines held for the
                        // rest of a prompt, so they let go as they show.
                        if let Err(e) = let_go_held_lines(
                            &app,
                            &mut stream,
                            &profile,
                            &timers,
                            &scrollback,
                            &logs,
                            &mut log_session,
                            &mut seen_output,
                            &mut settle,
                            &mut perf,
                        )
                        .await
                        {
                            warn!(error = %e, "letting go of held lines failed");
                        }
                    }
                    let mut p = profile.lock().await;
                    p.prompt.stage.local_write(after);
                }
                Some(OutgoingMsg::PromptRepaint) => {
                    // The card asked for it, so its state follows even when
                    // the bytes stay the same, since the pieces in them can
                    // be numbered anew.
                    let (out, state) = {
                        let mut p = profile.lock().await;
                        let now = Instant::now();
                        let out = repaint_step(&mut p, output_count() != seen_output, now);
                        // The design may have gained or lost a clock piece.
                        clock_until = clock_after(&p, now);
                        (out, watched_state(&app, &p))
                    };
                    if !out.is_empty() {
                        emit_repaint(&app, &out);
                    }
                    emit_prompt_state(&app, state);
                }
                None => {
                    debug!("outgoing channel closed; shutting down session");
                    break None;
                }
            },
            read = stream.read(&mut buf) => match read {
                Ok(0) => {
                    info!("server closed connection");
                    break Some("server closed connection".to_string());
                }
                Ok(n) => {
                    perf.socket_reads += 1;
                    perf.bytes_in += n as u64;
                    let events = parser.feed(&buf[..n]);
                    let mut batch = ReadBatch::new(seen_output);
                    for event in events {
                        if let Err(e) = handle_event(
                            &app,
                            &mut stream,
                            &mut negotiator,
                            &mut accumulator,
                            &profile,
                            &timers,
                            log_session_id,
                            &scrollback,
                            &mut server_echo,
                            event,
                            &mut batch,
                            &mut perf,
                        ).await {
                            warn!(error = %e, "event handling failed");
                            break;
                        }
                    }
                    if let Err(e) = end_read(
                        &app,
                        &mut stream,
                        &mut accumulator,
                        &profile,
                        &timers,
                        log_session_id,
                        &scrollback,
                        &mut batch,
                        &mut perf,
                    ).await {
                        warn!(error = %e, "prompt handling at the end of a read failed");
                    }
                    // The next read ends a hold. One that goes on holding
                    // keeps the first deadline, so a partial waits at most
                    // HOLD_MS in all.
                    hold_until = batch.hold.then(|| {
                        hold_until.unwrap_or_else(|| {
                            Instant::now() + Duration::from_millis(vosh_prompt::stage::HOLD_MS)
                        })
                    });
                    let (gmcp, prompt, wrote) = (batch.gmcp, batch.prompt, batch.out.writes_text());
                    clock_until = finish_read(
                        &app,
                        &profile,
                        &logs,
                        &mut log_session,
                        batch,
                        &mut seen_output,
                        &mut settle,
                        &mut perf,
                    )
                    .await;
                    if gmcp || late_until.is_some() {
                        let p = profile.lock().await;
                        late_until =
                            late_repaint_after(&p, late_until, gmcp, prompt, wrote, Instant::now());
                    }
                    // A game that never pauses still gets its frame and
                    // its rows every FRAME_BUDGET.
                    settle.overdue_now(&app, &logs, &mut perf);
                }
                Err(e) => {
                    error!(error = %e, "read failed");
                    // Some MUDs (Forsaken Lands among them) close with
                    // SO_LINGER 0 on quit, sending an RST instead of a
                    // graceful FIN. On macOS / BSD that can race with
                    // buffered bytes in the kernel recv queue, so the
                    // read returns ECONNRESET while the goodbye text is
                    // still pending. Try non-blocking reads to scoop up
                    // anything the kernel still has before we tear the
                    // session down. The cap is a belt-and-braces against
                    // a misbehaving stack returning Ok forever.
                    let mut drained_bytes = 0usize;
                    for _ in 0..32 {
                        match stream.try_read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => {
                                perf.socket_reads += 1;
                                perf.bytes_in += n as u64;
                                drained_bytes += n;
                                let events = parser.feed(&buf[..n]);
                                let mut batch = ReadBatch::new(seen_output);
                                for event in events {
                                    if let Err(handle_err) = handle_event(
                                        &app,
                                        &mut stream,
                                        &mut negotiator,
                                        &mut accumulator,
                                        &profile,
                                        &timers,
                                        log_session_id,
                                        &scrollback,
                                        &mut server_echo,
                                        event,
                                        &mut batch,
                                        &mut perf,
                                    )
                                    .await
                                    {
                                        warn!(
                                            error = %handle_err,
                                            "event handling failed during drain",
                                        );
                                        break;
                                    }
                                }
                                if let Err(e) = end_read(
                                    &app,
                                    &mut stream,
                                    &mut accumulator,
                                    &profile,
                                    &timers,
                                    log_session_id,
                                    &scrollback,
                                    &mut batch,
                                    &mut perf,
                                )
                                .await
                                {
                                    warn!(error = %e, "prompt handling during drain failed");
                                }
                                // The connection is going, so nothing waits.
                                if batch.hold {
                                    let mut p = profile.lock().await;
                                    hold_step(&mut p, &mut accumulator, &mut batch.out);
                                }
                                hold_until = None;
                                // The connection is going, so no clock
                                // repaints after it.
                                let _ = finish_read(
                                    &app,
                                    &profile,
                                    &logs,
                                    &mut log_session,
                                    batch,
                                    &mut seen_output,
                                    &mut settle,
                                    &mut perf,
                                )
                                .await;
                            }
                            Err(drain_err)
                                if drain_err.kind() == std::io::ErrorKind::WouldBlock =>
                            {
                                break;
                            }
                            Err(_) => break,
                        }
                    }
                    if drained_bytes > 0 {
                        debug!(drained_bytes, "recovered bytes after read error");
                    }
                    break Some(format_disconnect_reason(&e));
                }
            },
            () = sleep_until_hold(hold_until), if hold_until.is_some() => {
                hold_until = None;
                flush_hold(&app, &profile, &mut accumulator, &mut seen_output, &mut settle).await;
            }
            () = sleep_until_hold(late_until), if late_until.is_some() => {
                late_until = None;
                let (out, state) = {
                    let mut p = profile.lock().await;
                    let out =
                        late_repaint_step(&mut p, output_count() != seen_output, Instant::now());
                    let state = if out.is_empty() { None } else { watched_state(&app, &p) };
                    (out, state)
                };
                if !out.is_empty() {
                    emit_repaint(&app, &out);
                }
                emit_prompt_state(&app, state);
            }
            () = sleep_until_hold(clock_until), if clock_until.is_some() => {
                // A clock piece shows another second. Your idle prompt
                // repaints with it, unless you are selecting text or
                // reading back, and then the next second tries again.
                let reading = reader_busy(&app);
                let (out, state) = {
                    let mut p = profile.lock().await;
                    let now = Instant::now();
                    let out = clock_step(&mut p, output_count() != seen_output, reading, now);
                    clock_until = clock_after(&p, now);
                    let state = if out.is_empty() { None } else { watched_state(&app, &p) };
                    (out, state)
                };
                if !out.is_empty() {
                    emit_repaint(&app, &out);
                }
                emit_prompt_state(&app, state);
            }
            _ = tick_interval.tick() => {
                if let Err(e) = handle_tick(&app, &mut stream, &profile, &timers).await {
                    error!(error = %e, "tick handling failed");
                    break Some(format!("tick handling failed: {e}"));
                }
                if let Err(e) = fire_due_script_timers(&app, &mut stream, &profile, &timers).await {
                    error!(error = %e, "script timer firing failed");
                }
                if let Err(e) = fire_due_profile_timers(
                    &app,
                    &mut stream,
                    &profile,
                    &timers,
                    &mut timer_next,
                )
                .await
                {
                    error!(error = %e, "profile timer firing failed");
                }
                perf.tick_emits += 1;
            }
            _ = perf_report_interval.tick() => {
                perf.report_and_reset();
            }
            // Nothing else is ready, so the socket has nothing more for
            // now and the burst of reads that just ended shows in one
            // frame.
            () = std::future::ready(()), if settle.frame => {
                settle.frame_now(&app);
            }
            // Then the log takes the burst's rows once it is free, so a
            // busy log never holds the loop.
            mut guard = logs.lock(), if !settle.log.is_empty() => {
                settle.write_log(guard.as_mut(), &mut perf);
            }
        }
    };

    // A preview the card shows on your prompt goes with the connection,
    // so the live render goes back on the row first.
    let out = {
        let mut p = profile.lock().await;
        end_preview_step(&mut p, output_count() != seen_output, Instant::now())
    };
    if !out.is_empty() {
        emit_repaint(&app, &out);
    }

    // Capture the MUD's final partial line before teardown drops it. A
    // `quit` logout banner usually arrives without a trailing newline,
    // so it sits in the accumulator as a partial: painted at the end of
    // its read but never run through the per-line path that logs and
    // scrollback-records it.
    // Flush it now, ahead of the scrollback dump and log close below, so
    // the goodbye is captured like every other client captures it.
    if hold_until.is_some() {
        flush_hold(
            &app,
            &profile,
            &mut accumulator,
            &mut seen_output,
            &mut settle,
        )
        .await;
    }
    // The last burst still owes its frame and its rows, which go in the
    // log before the lines the session captures as it ends.
    settle.frame_now(&app);
    settle.write_log(logs.lock().await.as_mut(), &mut perf);
    capture_held_lines(&profile, &logs, log_session_id, &scrollback).await;
    capture_pending_line(&app, &logs, log_session_id, &scrollback, &mut accumulator).await;

    {
        let mut p = profile.lock().await;
        p.tick.end_session();
    }

    // Close the log session row and flush the scrollback ring buffer to
    // disk so the next launch can restore it. Failures here are
    // non-fatal; we still want the disconnect state to propagate.
    if let Some(sid) = log_session_id {
        let mut guard = logs.lock().await;
        if let Some(store) = guard.as_mut() {
            if let Err(e) = store.end_session(sid, now_ms()) {
                warn!(error = %e, "log end_session failed");
            }
        }
    }
    if let Some(path) = scrollback_path {
        let bytes = scrollback.lock().await.dump();
        if let Err(e) = std::fs::write(&path, bytes) {
            warn!(path = %path.display(), error = %e, "scrollback write failed");
        }
    }

    let line_triggers;
    // Session-only target state and the cached Room.Chars list clear
    // on disconnect. Quick-key verb bindings outlive the session (never
    // a restart), but the active target and room snapshot end with it.
    let target_after = {
        let mut p = profile.lock().await;
        let had = p.target.name.is_some();
        p.target.name = None;
        p.target.room_idx = None;
        p.room_chars.clear();
        p.room_block = room_block::RoomBlock::default();
        p.vars.remove("target");
        line_triggers = p.prompt.stage.line_trigger_notice();
        end_prompt(&mut p);
        had.then(|| p.target.quick_keys.clone())
    };
    // Line triggers no longer see a prompt the profile reads, so the first
    // session that read yours names the ones that matched it, once, at the
    // next launch.
    if let Some(names) = line_triggers {
        let state = app.state::<crate::app::state::SharedState>();
        crate::prompt_migration::note_line_triggers(state.inner(), names).await;
    }
    if let Some(quick_keys) = target_after {
        let _ = app.emit(
            events::TARGET,
            TargetPayload {
                name: None,
                room_idx: None,
                quick_keys,
            },
        );
    }
    let _ = stream.shutdown().await;
    // The affects list goes stale with the session, as the frontend
    // store drops its copy on the disconnected state below. The affect
    // fulls are written for the next login, then cleared.
    let shared = app.state::<crate::app::state::SharedState>();
    shared.last_affects.clear();
    crate::affect_full::disconnect(&app, shared.inner());
    // Reset password mode on disconnect so the next session starts with
    // a normal-text input even if the server bailed mid-password-prompt.
    emit_input_mode(&app, false);
    emit_state(
        &app,
        StatePayload::Disconnected {
            reason: disconnect_reason,
        },
    );
}

async fn handle_tick<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stream: &mut Stream,
    profile: &Arc<Mutex<Profile>>,
    timers: &SharedTimers,
) -> std::io::Result<()> {
    // Take the firing decision under the lock, then run the Send each
    // tick command, if the timer fired, after releasing it.
    let step = {
        let mut p = profile.lock().await;
        p.tick.poll(Instant::now())
    };
    if !step.payload.enabled && !step.payload.fired {
        return Ok(());
    }
    if let Some(text) = &step.warn_echo {
        emit_output(app, text.clone().into_bytes());
    }
    deliver_tick_step(app, stream, profile, timers, step, &mut OutputSink::Direct).await
}

/// Report a tick step on `session://tick`, so the frontend counts and
/// plays the sound when it fired, then run its Send each tick command
/// through the full input pipeline like a timer command.
async fn deliver_tick_step<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stream: &mut Stream,
    profile: &Arc<Mutex<Profile>>,
    timers: &SharedTimers,
    step: TickStep,
    sink: &mut OutputSink<'_>,
) -> std::io::Result<()> {
    if let Err(e) = app.emit(events::TICK, &step.payload) {
        warn!(error = %e, "failed to emit tick payload");
    }
    if let Some(command) = step.command {
        run_fired_command(app, stream, profile, timers, &command, sink).await?;
    }
    Ok(())
}

/// Fire the Settings interval timers whose deadline has elapsed.
/// `timer_next` maps timer id to its next-fire `Instant`; a timer is
/// seeded on first sight (scheduled one interval out, not fired
/// immediately) and advanced past any missed slots so a stall never
/// burst-fires. Disabled or deleted timers drop their deadline. Each
/// due command runs through the same path as the tick auto-fire:
/// `input::process`, echo its lines, send its bytes.
async fn fire_due_profile_timers<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stream: &mut Stream,
    profile: &Arc<Mutex<Profile>>,
    timers: &SharedTimers,
    timer_next: &mut HashMap<u32, Instant>,
) -> std::io::Result<()> {
    let now = Instant::now();
    let due: Vec<String> = {
        let p = profile.lock().await;
        let live: HashSet<u32> = p
            .timers
            .iter()
            .filter(|t| t.enabled)
            .map(|t| t.id)
            .collect();
        timer_next.retain(|id, _| live.contains(id));
        let mut due = Vec::new();
        for t in p
            .timers
            .iter()
            .filter(|t| t.enabled && !t.command.is_empty())
        {
            let interval = Duration::from_secs(u64::from(t.interval_secs.max(1)));
            match timer_next.get(&t.id).copied() {
                None => {
                    timer_next.insert(t.id, now + interval);
                }
                Some(next) if now >= next => {
                    due.push(t.command.clone());
                    let mut n = next + interval;
                    while n <= now {
                        n += interval;
                    }
                    timer_next.insert(t.id, n);
                }
                Some(_) => {}
            }
        }
        due
    };
    for command in due {
        run_fired_command(
            app,
            stream,
            profile,
            timers,
            &command,
            &mut OutputSink::Direct,
        )
        .await?;
    }
    Ok(())
}

/// Run `line` through the input pipeline for a path other than typed
/// input, and note what it asks of the saved profile the way the typed
/// path does. Call with the profile lock held. A `#profile reset`, or a
/// `#profile load` that reads its file, swaps the live UI config and
/// panes, so their generations move in the same step. The profile file it reads
/// holds none of the shared settings, so `shared` goes back over the
/// result as it does for typed input.
fn process_fired_line(
    p: &mut Profile,
    line: &str,
    effects: &mut input::LineEffects,
    shared: Option<&SharedLayer>,
) -> input::Ran {
    let ran = match shared.filter(|_| input::may_replace_profile(line)) {
        Some(layer) => layer.keep_across(p, |p| input::run_line(p, line)),
        None => input::run_line(p, line),
    };
    effects.note_ran(line, &ran);
    if ran.replaced {
        crate::app::state::note_ui_config_replaced();
    }
    ran
}

/// What one line the input pipeline ran asks for, as one script result:
/// its own bytes and echo lines, with what the Lua bodies of its script
/// aliases send among them in the order you typed them, then all else
/// the Lua it ran asks for.
pub(crate) fn line_script_result(ran: input::Ran) -> ApplyResult {
    let input::Ran { result, lua, .. } = ran;
    let mut apply = ApplyResult {
        send_bytes: result.bytes,
        echoes: result.echo,
        ..ApplyResult::default()
    };
    apply.append(lua);
    apply
}

/// What a line can change that shows outside the terminal text: the
/// target display and how your prompt looks.
struct Shown {
    target: TargetPayload,
    look: (bool, String, vosh_prompt::PromptShow),
}

impl Shown {
    fn of(p: &Profile) -> Self {
        Self {
            target: TargetPayload {
                name: p.target.name.clone(),
                room_idx: p.target.room_idx,
                quick_keys: p.target.quick_keys.clone(),
            },
            look: crate::prompt::prompt_look(p),
        }
    }
}

/// What lines a timer, the tick command or Lua ran changed outside the
/// terminal text. Typed input sends the same two things for a typed line.
#[derive(Debug, Default, PartialEq)]
struct ShownChanges {
    /// The target display, sent on `session://target`.
    target: Option<TargetPayload>,
    /// Your prompt looks different, so the open row repaints.
    repaint: bool,
}

impl ShownChanges {
    /// What `p` changed since `before` was taken.
    fn since(before: Shown, p: &Profile) -> Self {
        let after = Shown::of(p);
        Self {
            repaint: after.look != before.look,
            target: (after.target != before.target).then_some(after.target),
        }
    }

    /// Ask for the repaint and send the target, as typed input does. The
    /// repaint request goes through the session handle, as the typed one
    /// does, from a task of its own, since `session_disconnect` holds the
    /// handle's lock while it waits for this session to end.
    fn send<R: tauri::Runtime>(self, app: &AppHandle<R>) {
        if self.repaint {
            let state = app
                .state::<crate::app::state::SharedState>()
                .inner()
                .clone();
            tokio::spawn(async move { crate::prompt::request_prompt_repaint(&state).await });
        }
        if let Some(payload) = self.target {
            let _ = app.emit(events::TARGET, payload);
        }
    }
}

/// What lines from a timer, the tick or `mud.input` produced under the
/// profile lock.
struct FiredRun {
    /// What the lines ask for, with every list they changed.
    apply: ApplyResult,
    /// What they changed outside the terminal text.
    shown: ShownChanges,
    effects: input::LineEffects,
}

/// The part of [`run_fired_command`] that runs under the profile lock:
/// the input pipeline, which runs the Lua bodies of any script aliases
/// in the command where they stand, and all the Lua it ran asks for.
fn run_fired_locked(p: &mut Profile, command: &str, shared: Option<&SharedLayer>) -> FiredRun {
    run_lines_locked(p, [command], shared)
}

/// Run `lines` through the input pipeline under the profile lock, for a
/// path other than typed input, each as [`line_script_result`] reads it.
fn run_lines_locked<'a>(
    p: &mut Profile,
    lines: impl IntoIterator<Item = &'a str>,
    shared: Option<&SharedLayer>,
) -> FiredRun {
    let lists_before = ListRevisions::of(p);
    let shown_before = Shown::of(p);
    let mut effects = input::LineEffects::default();
    let mut apply = ApplyResult::default();
    for line in lines {
        let ran = process_fired_line(p, line, &mut effects, shared);
        apply.append(line_script_result(ran));
    }
    apply.lists = ListChanges::since(lists_before, p);
    FiredRun {
        apply,
        shown: ShownChanges::since(shown_before, p),
        effects,
    }
}

/// Run one command produced by a timer (or any non-typed source) through
/// the full input pipeline and deliver its results through
/// [`apply_script_result`]: echo lines to the terminal and bytes to the
/// server, with what the Lua bodies of its script aliases send among them
/// in the order the command names them, and all else the Lua it ran asks
/// for.
/// Mirrors the typed-input handler so a timer command behaves exactly
/// like the same line typed at the prompt, including `#lua` and
/// script-bodied aliases.
async fn run_fired_command<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stream: &mut Stream,
    profile: &Arc<Mutex<Profile>>,
    timers: &SharedTimers,
    command: &str,
    sink: &mut OutputSink<'_>,
) -> std::io::Result<()> {
    let shared = crate::profile::switch::shared_layer_for_lines(app, [command]).await;
    let FiredRun {
        apply,
        shown,
        effects,
    } = {
        let mut p = profile.lock().await;
        run_fired_locked(&mut p, command, shared.as_ref())
    };
    crate::disk::save::settle_line_effects(app, effects).await;
    shown.send(app);
    let mut io = ScriptIo::Session(stream, sink);
    apply_script_result(app, &mut io, profile, timers, apply).await
}

/// Echo lines outside a trigger's own line, each on its own row with a
/// line end before the first.
fn framed_echoes<S: AsRef<str>>(lines: &[S]) -> Vec<u8> {
    let mut buf = Vec::new();
    for line in lines {
        buf.extend_from_slice(b"\r\n");
        buf.extend_from_slice(line.as_ref().as_bytes());
    }
    buf.extend_from_slice(b"\r\n");
    buf
}

#[allow(clippy::too_many_arguments)]
async fn handle_event<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stream: &mut Stream,
    negotiator: &mut Negotiator,
    accumulator: &mut LineAccumulator,
    profile: &Arc<Mutex<Profile>>,
    timers: &SharedTimers,
    log_session_id: Option<i64>,
    scrollback: &crate::logs::SharedScrollback,
    server_echo: &mut ServerEcho,
    event: TelnetEvent,
    batch: &mut ReadBatch,
    perf: &mut PerfCounters,
) -> std::io::Result<()> {
    // WILL ECHO means the server takes over echoing what you type, which
    // ROM derivatives do for a password prompt. WONT ECHO hands echo back.
    // Note it before anything else in the event runs, then tell the
    // frontend to mask or unmask the input row. The negotiation reply
    // goes out through the catch all arm below.
    if let Some(held) = server_echo.observe(&event) {
        emit_input_mode(app, held);
    }
    match event {
        TelnetEvent::Data(bytes) => {
            // Every byte for the terminal lands in the read's batch, which
            // goes out as one `session://output` at the end of the read
            // instead of one per line. Tauri events serialize through the
            // bridge and xterm renders each write on its own frame, so
            // without batching a 50-line response paints line by line
            // ("typewriter") at the speed of event delivery. The partial
            // after the last line end waits for the end of the read.
            //
            // Triggers, Lua callbacks, route emissions, log writes,
            // and tick-reset bookkeeping still run per-line because
            // they have ordering semantics (a `gag` action mutates the
            // line's display before it lands in the batch). Log rows
            // flush in one transaction once the socket is quiet.
            for line in accumulator.feed(&bytes) {
                perf.lines_processed += 1;
                let plain = vosh_protocol::ansi::plain_text(&line.bytes);
                let trigger_t0 = std::time::Instant::now();
                // Phase 5 perf fix: take the tick step for a line that
                // matches the Reset on pattern under the same lock as
                // trigger/Lua matching so we never reacquire `profile`
                // later just to read the tick. The line is the game's
                // tick, so the step fires once per tick and carries the
                // Send each tick command to run after the lock drops.
                let steps = {
                    let lock_t0 = std::time::Instant::now();
                    let mut p = profile.lock().await;
                    perf.mutex_wait_ns += lock_t0.elapsed().as_nanos() as u64;
                    perf.mutex_acquires += 1;
                    line_step(&mut p, batch, line, plain, Instant::now(), log_session_id)
                };
                perf.trigger_lua_ns += trigger_t0.elapsed().as_nanos() as u64;
                for step in steps {
                    deliver_line_step(app, stream, profile, timers, scrollback, batch, step, perf)
                        .await?;
                }
            }
            Ok(())
        }
        TelnetEvent::Subnegotiation { option, payload } if option == telnet_option::GMCP => {
            perf.gmcp_packets += 1;
            batch.gmcp = true;
            handle_gmcp(app, profile, timers, stream, &payload, batch, perf).await?;
            Ok(())
        }
        TelnetEvent::Command(byte) if byte == telnet_codes::EOR || byte == telnet_codes::GA => {
            // The server marked the end of a prompt. The partial it ends
            // is your prompt when the capture reads it, and otherwise
            // runs through Prompts triggers and ends its row, so the next
            // line lands below it. A GA or EOR never makes a line a
            // prompt on its own, since the pager and editor prompts end
            // with one too. Either way the candidates ring records one
            // entry.
            let steps = {
                let mut p = profile.lock().await;
                marker_step(&mut p, accumulator, batch, Instant::now(), log_session_id)
            };
            for step in steps {
                deliver_line_step(app, stream, profile, timers, scrollback, batch, step, perf)
                    .await?;
            }
            Ok(())
        }
        TelnetEvent::Will(opt) if opt == telnet_option::GMCP => {
            // Accept GMCP via the negotiator, then immediately announce
            // ourselves and the packages we want. A WILL GMCP once it is
            // on gets no answer and no second hello.
            let was_on = negotiator.server_does(opt);
            let response = negotiator.handle(&TelnetEvent::Will(opt));
            stream.write_all(&response).await?;
            if !was_on && negotiator.server_does(opt) {
                stream.write_all(&hello_subnegotiation()).await?;
                stream.write_all(&supports_subnegotiation()).await?;
            }
            stream.flush().await?;
            Ok(())
        }
        other => {
            let response = negotiator.handle(&other);
            if !response.is_empty() {
                stream.write_all(&response).await?;
                stream.flush().await?;
            }
            Ok(())
        }
    }
}

/// What the Line pass decided for one line that is not your prompt.
struct LinePass {
    result: LineResult,
    /// The tick the line reset, with its Send each tick command.
    tick_step: Option<TickStep>,
    apply: ApplyResult,
}

/// Run one complete line through Line triggers, the tick reset pattern,
/// Lua triggers and the Script bodies the triggers queued, all under the
/// profile lock the caller holds. `plain` is the line without ANSI, so
/// no pattern has to allow for escape bytes and the line is stripped
/// once. `scope` is [`MatchScope::Room`] for a line that lists a room's
/// armies, things or people, so Room triggers run on it too, and
/// [`MatchScope::RoomTarget`] for the line of the person you target.
fn line_pass(
    p: &mut Profile,
    bytes: &[u8],
    plain: &str,
    scope: MatchScope,
    now: Instant,
) -> LinePass {
    let result = vosh_automation::trigger::process_on_ground(
        &p.triggers,
        bytes,
        plain,
        scope,
        highlight_ground::get(),
    );
    let tick_step = tick_reset(p, plain, now);
    script::snapshot_vars(&p.script, &p.vars);
    let mut outcome = match p.script.match_line(plain) {
        Ok(o) => o,
        Err(err) => {
            warn!(error = %err, "lua match_line failed");
            vosh_script::ScriptOutcome::default()
        }
    };
    // The Lua bodies of this line's Script actions join the outcome the
    // Lua registered triggers wrote, so one apply takes both.
    outcome
        .actions
        .extend(run_trigger_scripts(p, &result, "trigger-script"));
    let apply = script::apply_actions(p, outcome);
    LinePass {
        result,
        tick_step,
        apply,
    }
}

/// The tick step when `plain` matches the tick's Reset on pattern.
fn tick_reset(p: &mut Profile, plain: &str, now: Instant) -> Option<TickStep> {
    if p.tick.check_reset_match(plain) {
        p.tick.on_game_tick(now)
    } else {
        None
    }
}

/// Run the Lua bodies of the Script actions in `result`, with their
/// captures, and return the actions they produced.
fn run_trigger_scripts(
    p: &mut Profile,
    result: &LineResult,
    chunk: &str,
) -> Vec<vosh_script::Action> {
    let mut actions = Vec::new();
    for call in &result.scripts {
        match p.script.run_body(&call.body, &call.captures, chunk) {
            Ok(o) => actions.extend(o.actions),
            Err(err) => {
                warn!(error = %err, chunk, "trigger script eval failed");
            }
        }
    }
    actions
}

/// What one line, prompt or partial left for the session to do once the
/// profile lock drops: routes, sends, the Lua actions' IO, the tick
/// command, and the scrollback push.
struct LineStep {
    result: LineResult,
    apply: ApplyResult,
    tick_step: Option<TickStep>,
    /// The lines to keep in the scrollback ring, each as it shows.
    scrollback: Vec<Vec<u8>>,
    /// What Collapse repeated lines made of the line, when it is on, and
    /// the region the run shows in: the line kept starts a run of repeated
    /// lines, or takes the place of the run's line in the ring, the count
    /// before it, as on screen.
    repeat: Option<crate::logs::KeptRun>,
}

/// Handle one complete line under the profile lock. The stage reads it
/// as your prompt, alone or as the last line of a prompt that spans lines,
/// through [`prompt_block`], or holds it as the top line of one. Lines it
/// held and let go, then a line that is no prompt, run the Line pass and
/// land in the batch as their triggers left them, each replacing the
/// start an earlier read painted.
fn line_step(
    p: &mut Profile,
    batch: &mut ReadBatch,
    line: Line,
    plain: String,
    now: Instant,
    log_session_id: Option<i64>,
) -> Vec<LineStep> {
    p.prompt.note_text();
    // Without Char.Prompt this session, the game's reply to your own
    // `prompt` tells Vosh your setting.
    if p.prompt.observing(now_ms()) {
        p.prompt
            .observe_line(&line.bytes, &plain, chrono::Local::now().fixed_offset());
    }
    let offered = p
        .prompt
        .stage
        .offer(&line.bytes, &plain, line.painted, End::Line);
    let mut steps = released_steps(p, batch, offered.released, now, log_session_id);
    match offered.offer {
        Offer::Prompt(block, painted) => {
            steps.push(prompt_block(p, batch, block, painted, now, log_session_id));
        }
        Offer::Held => {}
        Offer::Line => steps.push(text_line_step(
            p,
            batch,
            line.bytes,
            plain,
            Shows::Now(line.painted),
            now,
            log_session_id,
        )),
    }
    steps
}

/// Lines the stage held and let go, each through the Line pass in order.
fn released_steps(
    p: &mut Profile,
    batch: &mut ReadBatch,
    released: Vec<vosh_prompt::stage::Released>,
    now: Instant,
    log_session_id: Option<i64>,
) -> Vec<LineStep> {
    released
        .into_iter()
        .map(|line| {
            text_line_step(
                p,
                batch,
                line.raw,
                line.plain,
                Shows::Now(line.painted),
                now,
                log_session_id,
            )
        })
        .collect()
}

/// Let go of the lines the stage holds for the rest of a prompt, as your
/// send or a local write does. The end of their read painted them, and
/// what you typed follows them, so each stays as it shows. Each runs the
/// Line pass, so triggers see it and it is logged and kept for
/// scrollback.
fn let_go_held(
    p: &mut Profile,
    batch: &mut ReadBatch,
    now: Instant,
    log_session_id: Option<i64>,
) -> Vec<LineStep> {
    p.prompt
        .stage
        .release()
        .into_iter()
        .map(|line| {
            text_line_step(
                p,
                batch,
                line.raw,
                line.plain,
                Shows::Painted,
                now,
                log_session_id,
            )
        })
        .collect()
}

/// The lines the stage still holds as the session ends. The end of their
/// read painted them, so each is logged and kept for scrollback as it
/// shows. Returns the log rows and the scrollback lines.
fn end_held(
    p: &mut Profile,
    log_session_id: Option<i64>,
) -> (Vec<vosh_log::LogEntry>, Vec<Vec<u8>>) {
    let mut log = Vec::new();
    let mut kept = Vec::new();
    for line in p.prompt.stage.release() {
        if let Some(sid) = log_session_id {
            log.push(vosh_log::LogEntry {
                session_id: sid,
                ts_ms: now_ms(),
                text: line.plain,
                raw: Some(line.raw.clone()),
            });
        }
        kept.push(line.raw);
    }
    (log, kept)
}

/// Where a line that is not your prompt shows.
#[derive(Debug, Clone, Copy)]
enum Shows {
    /// It writes now, over the region an earlier read painted its start
    /// in, if any.
    Now(Option<u64>),
    /// The end of its read painted it whole and output followed it, so it
    /// stays as it shows.
    Painted,
}

/// The scope a complete line that is not your prompt runs in, from the
/// room look tracker, which reads every such line in the order the game
/// sent it. [`MatchScope::RoomTarget`] for the person at the place your
/// target holds in Room.Chars, [`MatchScope::Room`] for any other army,
/// thing or person the look lists, and [`MatchScope::Line`] for any other
/// line. The look's Room.Chars packet comes before its text, so the
/// place is one in this look, the one `tar` marks with `>`.
fn room_scope(p: &mut Profile, plain: &str, bytes: &[u8]) -> MatchScope {
    use room_block::RoomLine;
    match p.room_block.line(plain, bytes) {
        RoomLine::Other => MatchScope::Line,
        RoomLine::Person(place) if p.target.room_idx == Some(place) => MatchScope::RoomTarget,
        RoomLine::Army | RoomLine::Thing | RoomLine::Person(_) => MatchScope::Room,
    }
}

/// A complete line that is not your prompt. It runs the Line pass and
/// lands in the batch as its triggers left it, replacing the region an
/// earlier read painted its start in, or stays as an earlier read
/// painted it (`shows`).
fn text_line_step(
    p: &mut Profile,
    batch: &mut ReadBatch,
    bytes: Vec<u8>,
    plain: String,
    shows: Shows,
    now: Instant,
    log_session_id: Option<i64>,
) -> LineStep {
    // Every complete line that is not your prompt passes the room look
    // tracker in the order the game sent it, so it knows the lines that
    // list a room's armies, things and people.
    let scope = room_scope(p, &plain, &bytes);
    let LinePass {
        result,
        tick_step,
        mut apply,
    } = line_pass(p, &bytes, &plain, scope, now);
    if result.display.is_none() {
        note_gag_without_reader(p, batch, &plain, scope);
    }
    // In-place echo replacement. When a trigger gags the line AND its
    // Script action emits one or more `mud.echo(...)` outputs, those
    // echoes land right where the gagged line would have rendered.
    // Without this they fall through to `apply_script_result`, which
    // frames each echo with a line end before and after, and against a
    // gagged line that reads as a blank row followed by the echo.
    let mut shown = Vec::new();
    if result.display.is_none() {
        for echo in apply.echoes.drain(..) {
            shown.extend_from_slice(echo.as_bytes());
            shown.extend_from_slice(b"\r\n");
        }
    }
    let collapse = p.ui.collapse_repeats;
    p.prompt.stage.set_collapse(collapse);
    let mut repeat = None;
    // Whether the ring keeps the line. While Collapse repeated lines is
    // on, it keeps what the screen shows, so the line end a pinned
    // prompt's row took, which writes nothing, stays out of it.
    let mut ring = true;
    let kept = match shows {
        // Collapse repeated lines shows a line the same as the one before
        // it once, with the count before it. Only what shows collapses.
        // Its triggers ran above, and it is logged below as any line that
        // shows.
        Shows::Now(painted)
            if collapse
                && result
                    .display
                    .as_ref()
                    .is_some_and(|text| vosh_prompt::stage::collapsible(text.as_bytes())) =>
        {
            let text = result.display.as_deref().unwrap_or_default().as_bytes();
            let made = p
                .prompt
                .stage
                .repeat_line(&mut batch.out, &bytes, &plain, painted, text);
            // The run as it shows, its count before it in the colors the
            // text carried into it, and the region it shows in.
            let (shows, gen) = p
                .prompt
                .stage
                .run_shown()
                .unwrap_or_else(|| (text.to_vec(), 0));
            repeat = Some(crate::logs::KeptRun { repeat: made, gen });
            Some(shows)
        }
        Shows::Now(painted) => {
            if let Some(text) = &result.display {
                shown.extend_from_slice(text.as_bytes());
                shown.extend_from_slice(b"\r\n");
            }
            let swallowed = p
                .prompt
                .stage
                .line(&mut batch.out, &bytes, &plain, painted, &shown);
            ring = !(collapse && swallowed);
            result.display.as_ref().map(|text| text.as_bytes().to_vec())
        }
        Shows::Painted => {
            // It shows as the game sent it, whatever its triggers do. What
            // a script echoed in place of a hidden line lands after it.
            p.prompt
                .stage
                .line(&mut batch.out, &bytes, &plain, None, &shown);
            Some(bytes.clone())
        }
    };
    // A line that shows is logged, in one transaction once the socket is
    // quiet, and kept in the ring buffer that becomes scrollback on the
    // next launch. The raw bytes carry ANSI, and the plain text drives
    // the regex search.
    let mut scrollback = Vec::new();
    if let Some(text) = kept {
        if let Some(sid) = log_session_id {
            batch.log.push(vosh_log::LogEntry {
                session_id: sid,
                ts_ms: now_ms(),
                text: plain,
                raw: Some(bytes),
            });
        }
        if ring {
            scrollback.push(text);
        }
    }
    LineStep {
        result,
        apply,
        tick_step,
        scrollback,
        repeat,
    }
}

/// Your prompt, recognized. Under the profile lock the Line pass takes,
/// in this order: the capture's values merge and the hidden state
/// follows them, Prompts triggers run on its final line so Lua
/// `mud.set_prompt_var` lands before the render, and then the design
/// draws in its place, or with drawing off it shows as sent. Line
/// triggers and Lua `match_line` never see it, and the tick reset
/// pattern still does. A drawn prompt is neither logged nor kept for
/// scrollback, and every line of it that shows is both, as any line that
/// shows. The prompt vars go out after the batch.
fn prompt_block(
    p: &mut Profile,
    batch: &mut ReadBatch,
    block: Block,
    painted: Option<u64>,
    now: Instant,
    log_session_id: Option<i64>,
) -> LineStep {
    // Your prompt ends any room look before it.
    p.room_block.end();
    let disagree = p.prompt.vars.capture(vosh_prompt::Capture {
        values: block.values.clone(),
        raw: Some(block.raw_text()),
    });
    p.prompt.note_prompt(chrono::Local::now().fixed_offset());
    if !disagree.is_empty() {
        debug!(target: "vosh::prompt", fields = ?disagree, "the prompt and GMCP disagree");
    }
    let mut tick_step = None;
    for line in &block.lines {
        if let Some(step) = tick_reset(p, &line.plain, now) {
            tick_step.get_or_insert(step);
        }
        // Line triggers no longer see it. Note the ones that would have
        // fired, for the one-time notice.
        let matched =
            vosh_automation::trigger::matching(&p.triggers, &line.plain, MatchScope::Line);
        p.prompt
            .stage
            .line_triggers_matched(matched.into_iter().map(|t| t.name.as_str()));
    }

    let last = block.final_line().clone();
    let result = vosh_automation::trigger::process_on_ground(
        &p.triggers,
        &last.raw,
        &last.plain,
        MatchScope::Prompt,
        highlight_ground::get(),
    );
    if !result.scripts.is_empty() {
        script::snapshot_vars(&p.script, &p.vars);
    }
    let outcome = vosh_script::ScriptOutcome {
        actions: run_trigger_scripts(p, &result, "prompt-trigger-script"),
    };
    let mut apply = script::apply_actions(p, outcome);
    batch.prompt_vars = true;
    batch.prompt = true;
    // The prompt draws with the packets that came before it.
    batch.gmcp = false;

    let mut before = Vec::new();
    let mut scrollback = Vec::new();
    // Pinned, the prompt leaves the text for the band above the command
    // line. It is logged and kept exactly as it is in the text.
    let pinned = p.prompt.show() == vosh_prompt::PromptShow::Pinned;
    // The away prompt shows as sent, even while Vosh draws.
    if p.prompt.draws() && !block.afk {
        // Echoes land where the prompt was, above the drawn prompt.
        for echo in apply.echoes.drain(..) {
            before.extend_from_slice(echo.as_bytes());
            before.extend_from_slice(b"\r\n");
        }
        // What the open card shows, a preview among them, with the live
        // render behind it, which the region carries as its restore.
        let view = prompt_view(p, now);
        // A line above the last one your design reads nothing on has
        // nothing drawn in its place, so it shows as the game sent it.
        let last_index = block.lines.len() - 1;
        let heads_shown: Vec<BlockLine> = block.lines[..last_index]
            .iter()
            .enumerate()
            .filter(|(index, _)| !block.replaced.contains(index))
            .map(|(_, line)| line.clone())
            .collect();
        if pinned {
            p.prompt
                .stage
                .pin_view(&mut batch.out, block, painted, &before, view.stage());
        } else {
            p.prompt
                .stage
                .draw_view(&mut batch.out, block, painted, &before, view.stage());
        }
        for head in &heads_shown {
            keep_shown(batch, &mut scrollback, head, &head.raw, log_session_id);
        }
    } else {
        if result.display.is_none() {
            for echo in apply.echoes.drain(..) {
                before.extend_from_slice(echo.as_bytes());
                before.extend_from_slice(b"\r\n");
            }
        }
        let display = result.display.as_deref().map(str::as_bytes);
        // The lines above the last one show as sent, whatever Prompts
        // triggers do to the last one.
        let heads: Vec<BlockLine> = block.lines[..block.lines.len() - 1].to_vec();
        if pinned {
            p.prompt
                .stage
                .pin_shown(&mut batch.out, block, painted, &before, display);
        } else {
            p.prompt
                .stage
                .show_as_sent(&mut batch.out, block, painted, &before, display);
        }
        for head in &heads {
            keep_shown(batch, &mut scrollback, head, &head.raw, log_session_id);
        }
        if let Some(text) = &result.display {
            keep_shown(
                batch,
                &mut scrollback,
                &last,
                text.as_bytes(),
                log_session_id,
            );
        }
    }
    LineStep {
        result,
        apply,
        tick_step,
        scrollback,
        repeat: None,
    }
}

/// A line of your prompt that shows: logged, in one transaction once the
/// socket is quiet, and kept in the ring buffer that becomes scrollback,
/// as `shown`, what the terminal shows of it.
fn keep_shown(
    batch: &mut ReadBatch,
    scrollback: &mut Vec<Vec<u8>>,
    line: &BlockLine,
    shown: &[u8],
    log_session_id: Option<i64>,
) {
    if let Some(sid) = log_session_id {
        batch.log.push(vosh_log::LogEntry {
            session_id: sid,
            ts_ms: now_ms(),
            text: line.plain.clone(),
            raw: Some(line.raw.clone()),
        });
    }
    scrollback.push(shown.to_vec());
}

/// A partial Vosh does not read as your prompt, ended by a GA or EOR. It
/// runs through Prompts triggers as it always did and ends its row. A
/// trigger that hides it and sets prompt values while nothing reads your
/// prompt is named once a session.
fn unread_partial(
    p: &mut Profile,
    batch: &mut ReadBatch,
    partial: &Partial,
    plain: &str,
) -> LineStep {
    let result = vosh_automation::trigger::process_on_ground(
        &p.triggers,
        &partial.bytes,
        plain,
        MatchScope::Prompt,
        highlight_ground::get(),
    );
    let effect = match &result.display {
        None => true,
        Some(text) => text.as_bytes() != partial.bytes,
    } || !result.sends.is_empty()
        || !result.routes.is_empty()
        || !result.scripts.is_empty();
    let mut apply = ApplyResult::default();
    if effect {
        script::snapshot_vars(&p.script, &p.vars);
        let outcome = vosh_script::ScriptOutcome {
            actions: run_trigger_scripts(p, &result, "prompt-trigger-script"),
        };
        apply = script::apply_actions(p, outcome);
        // The webview hears every prompt a Prompts trigger acted on.
        batch.prompt_vars = true;
    }
    if result.display.is_none() {
        note_gag_without_reader(p, batch, plain, MatchScope::Prompt);
    }
    let mut before = Vec::new();
    if result.display.is_none() {
        for echo in apply.echoes.drain(..) {
            before.extend_from_slice(echo.as_bytes());
            before.extend_from_slice(b"\r\n");
        }
    }
    p.prompt.stage.end_partial(
        &mut batch.out,
        &partial.bytes,
        partial.painted,
        &before,
        result.display.as_deref().map(str::as_bytes),
    );
    LineStep {
        result,
        apply,
        tick_step: None,
        scrollback: Vec::new(),
        repeat: None,
    }
}

/// A GA or EOR arrived. The partial it ends is your prompt when the
/// capture reads it, alone or after held lines, and otherwise goes
/// through [`unread_partial`]. Held lines it does not finish run the Line
/// pass first. The candidates ring records one entry either way.
fn marker_step(
    p: &mut Profile,
    accumulator: &mut LineAccumulator,
    batch: &mut ReadBatch,
    now: Instant,
    log_session_id: Option<i64>,
) -> Vec<LineStep> {
    let Some(partial) = accumulator.take_partial() else {
        // A GA after lines the stage held ends them, since the rest of
        // the prompt never came.
        let released = p.prompt.stage.release();
        let steps = released_steps(p, batch, released, now, log_session_id);
        p.prompt.record(None, now_ms());
        // The marker ends any room look before it.
        p.room_block.end();
        return steps;
    };
    let plain = vosh_protocol::ansi::plain_text(&partial.bytes);
    let painted = partial.painted.map(|(gen, _)| gen);
    let offered = p
        .prompt
        .stage
        .offer(&partial.bytes, &plain, painted, End::Marker);
    let mut steps = released_steps(p, batch, offered.released, now, log_session_id);
    match offered.offer {
        Offer::Prompt(block, painted) => {
            steps.push(prompt_block(p, batch, block, painted, now, log_session_id));
            p.prompt.record(None, now_ms());
        }
        Offer::Held | Offer::Line => {
            // The lines it released took the region the partial was
            // painted in, so the partial writes after them.
            let partial = if steps.is_empty() {
                partial
            } else {
                Partial {
                    painted: None,
                    ..partial
                }
            };
            steps.push(unread_partial(p, batch, &partial, &plain));
            p.prompt.record(Some((&partial.bytes, &plain)), now_ms());
        }
    }
    p.room_block.end();
    steps
}

/// The end of a read. A partial the capture settles on, alone or after
/// held lines, is your prompt now, so it draws in this read and never
/// flashes. Any other partial paints as a region a later read replaces,
/// with the held lines before it. Held lines with no partial after them
/// paint the same way. Then the stage catches up with everything the read
/// wrote.
fn partial_step(
    p: &mut Profile,
    accumulator: &mut LineAccumulator,
    batch: &mut ReadBatch,
    now: Instant,
    log_session_id: Option<i64>,
) -> Option<LineStep> {
    let mut step = None;
    if let Some(bytes) = accumulator.partial().map(<[u8]>::to_vec) {
        p.prompt.note_text();
        let plain = vosh_protocol::ansi::plain_text(&bytes);
        match p.prompt.stage.settle(&bytes, &plain) {
            Some((block, region)) => {
                let painted = accumulator
                    .take_partial()
                    .and_then(|t| t.painted)
                    .map(|(gen, _)| gen);
                step = Some(prompt_block(
                    p,
                    batch,
                    block,
                    region.or(painted),
                    now,
                    log_session_id,
                ));
            }
            // It can still become your prompt, and it was not painted
            // yet, so it waits a moment for the next read.
            None if accumulator.painted().is_none() && p.prompt.stage.live(&plain) => {
                batch.hold = true;
            }
            None => {
                let painted =
                    p.prompt
                        .stage
                        .paint_partial(&mut batch.out, &bytes, accumulator.painted());
                accumulator.set_painted(painted);
            }
        }
    } else {
        p.prompt.stage.end_read(&mut batch.out);
    }
    p.prompt.stage.finish(&mut batch.out);
    step
}

/// A partial that waited for the next read stops waiting: it paints
/// raw, with any held lines before it, as a region a later read
/// replaces.
fn hold_step(p: &mut Profile, accumulator: &mut LineAccumulator, out: &mut Output) {
    if let Some(bytes) = accumulator.partial().map(<[u8]>::to_vec) {
        let painted = p
            .prompt
            .stage
            .paint_partial(out, &bytes, accumulator.painted());
        accumulator.set_painted(painted);
    }
    p.prompt.stage.finish(out);
}

/// Paint a partial that waited and send it out. `seen` becomes the output
/// count after it.
async fn flush_hold<R: tauri::Runtime>(
    app: &AppHandle<R>,
    profile: &Arc<Mutex<Profile>>,
    accumulator: &mut LineAccumulator,
    seen: &mut u64,
    settle: &mut Settle,
) {
    let out = {
        let mut p = profile.lock().await;
        let mut out = Output::new(output_count() != *seen);
        hold_step(&mut p, accumulator, &mut out);
        out
    };
    if !out.is_empty() {
        *seen = emit_session_output(app, &out, settle);
    }
}

/// Let go of the lines the stage holds for the rest of a prompt, through
/// [`let_go_held`], and send what their Line pass left: the routes, the
/// scrollback, what their triggers send, and the log rows. `seen`
/// becomes the output count after it.
#[allow(clippy::too_many_arguments)]
async fn let_go_held_lines<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stream: &mut Stream,
    profile: &Arc<Mutex<Profile>>,
    timers: &SharedTimers,
    scrollback: &crate::logs::SharedScrollback,
    logs: &crate::logs::SharedLogStore,
    log_session: &mut LogSession,
    seen: &mut u64,
    settle: &mut Settle,
    perf: &mut PerfCounters,
) -> std::io::Result<()> {
    let mut batch = ReadBatch::new(*seen);
    let steps = {
        let mut p = profile.lock().await;
        if !p.prompt.stage.holds() {
            return Ok(());
        }
        let_go_held(&mut p, &mut batch, Instant::now(), log_session.id)
    };
    for step in steps {
        deliver_line_step(
            app, stream, profile, timers, scrollback, &mut batch, step, perf,
        )
        .await?;
    }
    finish_read(app, profile, logs, log_session, batch, seen, settle, perf).await;
    Ok(())
}

/// Log the lines the stage still holds as the session ends, and keep them
/// for scrollback, through [`end_held`].
async fn capture_held_lines(
    profile: &Arc<Mutex<Profile>>,
    logs: &crate::logs::SharedLogStore,
    log_session_id: Option<i64>,
    scrollback: &crate::logs::SharedScrollback,
) {
    let (log, kept) = end_held(&mut *profile.lock().await, log_session_id);
    if !kept.is_empty() {
        let mut ring = scrollback.lock().await;
        for text in kept {
            ring.push(text);
        }
    }
    if !log.is_empty() {
        let mut guard = logs.lock().await;
        if let Some(store) = guard.as_mut() {
            if let Err(e) = store.append_batch(&log) {
                warn!(error = %e, "disconnect held lines log append failed");
            }
        }
    }
}

/// When the late GMCP repaint fires, after a read. `waiting`
/// is when it was going to fire, `gmcp` says the read brought packets
/// after the last prompt it read, `prompt` that it read one, and `wrote`
/// that it wrote to the text. What follows the packets draws with them,
/// so in the text and lifted text cancels a repaint an earlier read
/// started, and pinned a prompt does, since the band is not in the text.
/// Packets after the read's last prompt start it while there is an open
/// row or a band to repaint, and one already waiting keeps its time.
/// Whether the packets changed what your prompt shows is decided when it
/// fires, so deciding here draws nothing, and a pulse whose packets and
/// text come in two reads costs no render.
fn late_repaint_after(
    p: &Profile,
    waiting: Option<Instant>,
    gmcp: bool,
    prompt: bool,
    wrote: bool,
    now: Instant,
) -> Option<Instant> {
    let pinned = p.prompt.show() == vosh_prompt::PromptShow::Pinned;
    let cancel = if pinned { prompt } else { wrote };
    let waiting = waiting.filter(|_| !cancel);
    if waiting.is_some() || !gmcp {
        return waiting;
    }
    p.prompt.stage.repaintable().then(|| now + LATE_REPAINT)
}

/// The late GMCP repaint fires: your prompt as it shows now, when there is
/// still a row or a band to repaint, which is empty when nothing changed.
/// `other` says output from elsewhere landed since the session last wrote.
fn late_repaint_step(p: &mut Profile, other: bool, now: Instant) -> Output {
    if !p.prompt.stage.repaintable() {
        return Output::new(other);
    }
    repaint_step(p, other, now)
}

/// When the next clock repaint looks at your prompt: when a
/// clock piece in your design next shows another second, or None while
/// your design draws none, so a design without one never wakes the
/// session. The tick turns on its own seconds, counted from its last
/// restart, so the repaint lands on each of them and never skips one.
/// Once a late tick has none left, the seconds since it keep counting
/// up from the same restart. The time and the date turn on the local
/// clock's seconds. With both, the tick sets the pace, so your prompt
/// repaints at most once a second.
fn clock_after(p: &Profile, now: Instant) -> Option<Instant> {
    let clock = p.prompt.clock()?;
    let tick = clock
        .tick
        .then(|| p.tick.remaining(now))
        .flatten()
        .filter(|left| !left.is_zero());
    let late = clock.tick.then(|| p.tick.elapsed(now)).flatten();
    let wait = match (tick, late) {
        (Some(left), _) => match left.as_nanos() % 1_000_000_000 {
            0 => Duration::from_secs(1),
            part => Duration::from_nanos(u64::try_from(part).unwrap_or(0)),
        },
        (None, Some(since)) => {
            let into = since.as_nanos() % 1_000_000_000;
            Duration::from_nanos(u64::try_from(1_000_000_000 - into).unwrap_or(0))
        }
        (None, None) => {
            let into = chrono::Local::now().timestamp_subsec_nanos() % 1_000_000_000;
            Duration::from_nanos(u64::from(1_000_000_000 - into))
        }
    };
    Some(now + wait + CLOCK_SLACK)
}

/// A clock piece shows another second: your prompt as it
/// shows now, the open row while it is the last thing on screen or the
/// band while pinned, and empty when the second changed nothing it
/// shows. The open row waits while `reading`, which says you are
/// selecting text or reading back, and the band does not, since it is not
/// in the text. It waits while the card shows a
/// preview, whose render carries its own restore. Each repaint is a
/// replace with nothing after it, so it never lands on your typed text
/// and never reaches history. `other` says output from elsewhere landed
/// since the session last wrote, which closed the row.
fn clock_step(p: &mut Profile, other: bool, reading: bool, now: Instant) -> Output {
    let pinned = p.prompt.show() == vosh_prompt::PromptShow::Pinned;
    if p.prompt.clock().is_none()
        || p.prompt.preview().is_some()
        || (reading && !pinned)
        || !p.prompt.stage.repaintable()
    {
        return Output::new(other);
    }
    repaint_step(p, other, now)
}

/// You are selecting text or reading back in the terminal. The webview
/// says so for xterm (`terminal_reader_busy`), and the native grid holds
/// its own selection and scroll.
fn reader_busy<R: tauri::Runtime>(app: &AppHandle<R>) -> bool {
    let webview = app
        .try_state::<crate::app::state::SharedState>()
        .is_some_and(|state| state.reader_busy.load(std::sync::atomic::Ordering::Acquire));
    webview || native_reader_busy()
}

#[cfg(native_surface)]
fn native_reader_busy() -> bool {
    crate::term_grid::reader_busy()
}

#[cfg(not(native_surface))]
fn native_reader_busy() -> bool {
    false
}

/// Wait until `until`, or forever with no deadline.
async fn sleep_until_hold(until: Option<Instant>) {
    match until {
        Some(until) => tokio::time::sleep_until(until).await,
        None => std::future::pending().await,
    }
}

/// A line you sent. The candidates ring records the prompt it answers,
/// before the partial goes, and the open row closes, since your typed
/// echo follows it. `sent` is what went to the game, aliases expanded,
/// which opens the observer's window. Returns true when the send started
/// a pulse, on a server that sends no Char.Vitals.
fn send_step(p: &mut Profile, accumulator: &LineAccumulator, sent: &[u8], at_ms: i64) -> bool {
    let partial = accumulator
        .partial()
        .map(|bytes| (bytes.to_vec(), vosh_protocol::ansi::plain_text(bytes)));
    p.prompt.record(
        partial
            .as_ref()
            .map(|(bytes, plain)| (&bytes[..], plain.as_str())),
        at_ms,
    );
    p.prompt.stage.close();
    p.prompt.note_send(&String::from_utf8_lossy(sent), at_ms)
}

/// A window size message. While the card is closed, a new width closes
/// the open row, since each renderer wraps it again at that width, and
/// nothing repaints until the next prompt. A new height wraps
/// nothing, such as when your prompt leaves the pinned band for the text
/// and the terminal grows, so the row stays open and each renderer finds
/// its region where it was. While the card is open, which `card_open`
/// says when it watches your prompt and a preview on the row says too,
/// the row stays open at any size: each renderer finds its region again
/// in its own buffer, so edits and previews keep repainting it and
/// clearing a preview puts the live render back. The size the session
/// already holds, which the webview sends again on every connect, leaves
/// the row open. Pinned, the band is not in the text, so no size closes
/// it.
///
/// A design that pushes part of a row to the right edge (`%{right}`)
/// reaches to the new width, so a new width keeps the row open as the
/// card does and returns true: your prompt draws again at that width, on
/// the row or on the band.
fn window_size_step(
    p: &mut Profile,
    negotiator: &mut Negotiator,
    cols: u16,
    rows: u16,
    card_open: bool,
) -> bool {
    let card_open = card_open || p.prompt.preview().is_some();
    let new_width = negotiator.window_size.0 != cols;
    p.prompt.set_cols(usize::from(cols));
    let redraw = new_width && p.prompt.pushes_right();
    if new_width && !card_open && !redraw {
        p.prompt.stage.close();
    }
    negotiator.set_window_size(cols, rows);
    redraw
}

/// Repaint the open row as the `[prompt]` table now says: your design
/// while Vosh draws, else the lines the design replaced, as the game sent
/// them, so turning drawing off shows the game's prompt at once. A
/// preview the open card shows draws in place of the live render, which
/// the row carries as its restore. `other` says output from elsewhere
/// landed since the session last wrote, which closed the row. Returns the
/// repaint, empty when no row is open.
fn repaint_step(p: &mut Profile, other: bool, now: Instant) -> Output {
    let mut out = Output::new(other);
    let view = prompt_view(p, now);
    p.prompt.stage.repaint_view(&mut out, view.stage());
    out
}

/// The connection is going. A preview the card shows on your prompt goes
/// with it, so a repaint puts the live render back on the
/// row, or on the band while pinned, since nothing else may land to make
/// the renderers write the restore they hold. Empty with no preview, and
/// when no row or band is left to repaint.
fn end_preview_step(p: &mut Profile, other: bool, now: Instant) -> Output {
    if p.prompt.preview().is_none() {
        return Output::new(other);
    }
    p.prompt.set_preview(None);
    repaint_step(p, other, now)
}

/// A trigger hid a line or partial. When nothing reads your prompt in
/// this profile and the trigger also sets prompt values, it hides your
/// prompt with nothing drawn in its place, so the webview hears its name
/// once a session.
fn note_gag_without_reader(p: &mut Profile, batch: &mut ReadBatch, plain: &str, scope: MatchScope) {
    if p.prompt.stage.has_recognizer() {
        return;
    }
    for trigger in vosh_automation::trigger::matching(&p.triggers, plain, scope) {
        if hides_and_reads_prompt(trigger) && p.prompt.stage.gag_without_reader(&trigger.name) {
            batch.gag_without_reader.push(trigger.name.clone());
        }
    }
}

/// The trigger hides what it matches and its script sets prompt values,
/// the shape of a capture trigger.
fn hides_and_reads_prompt(trigger: &vosh_automation::trigger::Trigger) -> bool {
    use vosh_automation::trigger::TriggerAction;
    trigger
        .actions
        .iter()
        .any(|a| matches!(a, TriggerAction::Gag))
        && trigger
            .actions
            .iter()
            .any(|a| matches!(a, TriggerAction::Script { body } if body.contains("set_prompt_var")))
}

#[cfg(test)]
thread_local! {
    /// How many times this thread drew your design from the live values,
    /// so a test can tell what a step costs.
    static RENDERS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Your design drawn from the live values. The vosh-prompt resolver reads
/// the values the capture and scripts set, then the latest GMCP packets,
/// then what Vosh itself knows, and draws `?` for a value the game hides.
/// The spans say where each piece landed, which the open row keeps for
/// the prompt card.
fn render_prompt(p: &Profile, now: Instant) -> vosh_prompt::Rendered {
    #[cfg(test)]
    RENDERS.with(|n| n.set(n.get() + 1));
    let vosh = prompt_supplies(p, now);
    vosh_prompt::render_str(
        &p.prompt.config().template,
        &p.prompt.vars.resolver(&vosh),
        p.prompt.render_options(false),
    )
}

/// What your prompt shows: the lines the game sent with drawing off, and
/// with it on, your design as the open card shows it, with the live
/// render behind it while that differs.
struct PromptView {
    /// What your prompt shows, or None for the lines the game sent.
    shown: Option<vosh_prompt::Rendered>,
    /// The live render while `shown` is a preview in its place.
    live: Option<vosh_prompt::Rendered>,
}

impl PromptView {
    /// The view the stage takes, with where each piece of the design
    /// landed in what your prompt shows, which the open row keeps for the
    /// prompt card.
    fn stage(&self) -> vosh_prompt::stage::View<'_> {
        vosh_prompt::stage::View {
            shown: self.shown.as_ref().map(|r| r.ansi.as_str()),
            live: self.live.as_ref().map(|r| r.ansi.as_str()),
            spans: self.shown.as_ref().map_or(&[], |r| r.spans.as_slice()),
            plain: self.shown.as_ref().map_or("", |r| r.plain.as_str()),
        }
    }
}

/// What your prompt shows now. With drawing on and the open card showing a
/// preview, the design draws with the preview's values, and with the labels
/// of values that have nothing to show while the card asks for them, or the
/// row shows the lines the game sent while the card reads your codes. The
/// live render rides behind it. Overrides never reach
/// `session://prompt-vars`, so the panes keep the live values.
fn prompt_view(p: &Profile, now: Instant) -> PromptView {
    if !p.prompt.draws() {
        return PromptView {
            shown: None,
            live: None,
        };
    }
    let live = render_prompt(p, now);
    let Some(preview) = p.prompt.preview() else {
        return PromptView {
            shown: Some(live),
            live: None,
        };
    };
    if preview.raw {
        return PromptView {
            shown: None,
            live: Some(live),
        };
    }
    let vosh = prompt_supplies(p, now);
    let resolver = p.prompt.vars.resolver(&vosh);
    let overrides = preview.overrides(&resolver);
    let shown = vosh_prompt::render_str(
        &p.prompt.config().template,
        &vosh_prompt::values::overrides::Overridden::new(
            &resolver,
            &overrides,
            chrono::Local::now().naive_local(),
        ),
        p.prompt.render_options(preview.placeholders),
    );
    PromptView {
        shown: Some(shown),
        live: Some(live),
    }
}

/// Do what a line step left for after the profile lock: emit its routes,
/// keep it for scrollback, send what its triggers send, apply its Lua
/// actions' IO into the batch, and run the tick command it fired.
#[allow(clippy::too_many_arguments)]
async fn deliver_line_step<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stream: &mut Stream,
    profile: &Arc<Mutex<Profile>>,
    timers: &SharedTimers,
    scrollback: &crate::logs::SharedScrollback,
    batch: &mut ReadBatch,
    step: LineStep,
    perf: &mut PerfCounters,
) -> std::io::Result<()> {
    let LineStep {
        result,
        apply,
        tick_step,
        scrollback: kept,
        repeat,
    } = step;
    if !result.routes.is_empty() {
        perf.routed_emits += result.routes.len() as u64;
    }
    emit_line_routes(app, &result);
    for text in kept {
        let sb_t0 = std::time::Instant::now();
        // The ring keeps a run of repeated lines once, as the screen shows
        // it.
        scrollback.lock().await.keep(text, repeat);
        perf.scrollback_push_ns += sb_t0.elapsed().as_nanos() as u64;
        perf.scrollback_pushes += 1;
    }
    send_trigger_outputs(stream, &result.sends).await?;
    let mut sink = OutputSink::Batch(batch);
    apply_script_result(
        app,
        &mut ScriptIo::Session(stream, &mut sink),
        profile,
        timers,
        apply,
    )
    .await?;
    if let Some(step) = tick_step {
        deliver_tick_step(app, stream, profile, timers, step, &mut sink).await?;
    }
    Ok(())
}

/// The end of a read, see [`partial_step`], and the IO its prompt left.
#[allow(clippy::too_many_arguments)]
async fn end_read<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stream: &mut Stream,
    accumulator: &mut LineAccumulator,
    profile: &Arc<Mutex<Profile>>,
    timers: &SharedTimers,
    log_session_id: Option<i64>,
    scrollback: &crate::logs::SharedScrollback,
    batch: &mut ReadBatch,
    perf: &mut PerfCounters,
) -> std::io::Result<()> {
    let step = {
        let mut p = profile.lock().await;
        partial_step(&mut p, accumulator, batch, Instant::now(), log_session_id)
    };
    if let Some(step) = step {
        deliver_line_step(app, stream, profile, timers, scrollback, batch, step, perf).await?;
    }
    Ok(())
}

/// What Vosh itself supplies to the custom prompt: the tick timer, your
/// target, the profile's name and the affects you track. The clock reads
/// the local time.
pub(crate) fn prompt_supplies(p: &Profile, now: Instant) -> vosh_prompt::ClientValues {
    let tick = p.tick.remaining(now).map(|left| vosh_prompt::values::Tick {
        remaining: i64::try_from(left.as_millis().div_ceil(1000)).unwrap_or(i64::MAX),
        interval: i64::try_from(p.tick.config.interval.as_secs()).ok(),
        since: p
            .tick
            .elapsed(now)
            .and_then(|since| i64::try_from(since.as_secs()).ok()),
    });
    vosh_prompt::ClientValues {
        tick,
        target: p.target.name.clone(),
        profile: p.display_name.clone(),
        now: None,
        tracked: p
            .ui
            .tracked_affects
            .iter()
            .map(|t| t.name.clone())
            .collect(),
    }
}

/// Start the custom prompt's session with no packets and no values.
/// `known_host` is whether the host is The Forsaken Lands, whose rules
/// also hold when the profile's capture reads Aabahran's codes.
fn start_prompt(p: &mut Profile, known_host: bool) {
    p.prompt.connect(known_host);
    p.prompt.stage.set_collapse(p.ui.collapse_repeats);
}

/// The custom prompt's packets, values and hidden state go with the
/// connection. The webview stores clear on the disconnected state, so
/// the hidden state that ends here is never reported.
fn end_prompt(p: &mut Profile) {
    p.prompt.disconnect();
    let _ = p.prompt.vars.take_hidden_change();
}

/// Keep a GMCP packet for the custom prompt, stamped with the local
/// time it arrived.
fn observe_prompt_gmcp(p: &mut Profile, msg: &vosh_protocol::gmcp::Message) {
    p.prompt.observe(
        &msg.package,
        msg.data.clone(),
        chrono::Local::now().fixed_offset(),
    );
}

/// Tell the webview which values the game hides, when that changed
/// since the last report. The session calls it once per socket read and
/// after a send that starts a pulse.
async fn emit_hidden_change<R: tauri::Runtime>(app: &AppHandle<R>, profile: &Arc<Mutex<Profile>>) {
    let change = profile.lock().await.prompt.vars.take_hidden_change();
    if let Some(hidden) = change {
        if let Err(e) = app.emit(events::HIDDEN, hidden) {
            warn!(error = %e, "failed to emit the hidden state");
        }
    }
}

/// Send what one socket read gathered: its output, the triggers that hid
/// a prompt with nothing to draw in its place, then the prompt vars when
/// a prompt was read or they changed, and the hidden state when it
/// changed. Once per read, so the packets of one pulse never show the
/// panes a state between them. Its frame and its log rows wait in
/// `settle` for the end of the burst of reads. `seen` becomes the output
/// count after this read's output. Returns when a clock piece in your
/// design next shows another second, which the lock this takes anyway
/// reads, so a read costs no other lock for it.
#[allow(clippy::too_many_arguments)]
async fn finish_read<R: tauri::Runtime>(
    app: &AppHandle<R>,
    profile: &Arc<Mutex<Profile>>,
    logs: &crate::logs::SharedLogStore,
    log_session: &mut LogSession,
    batch: ReadBatch,
    seen: &mut u64,
    settle: &mut Settle,
    perf: &mut PerfCounters,
) -> Option<Instant> {
    let ReadBatch {
        mut out,
        log,
        prompt_vars,
        prompt,
        gag_without_reader,
        character,
        hold: _,
        gmcp: _,
    } = batch;
    let watched = prompt && watching_prompt(app);
    let (vars, hidden, prompt_seen, status, prompt_state, clock) = {
        let mut p = profile.lock().await;
        // Echoes the end of the read wrote close the open row.
        p.prompt.stage.finish(&mut out);
        (
            p.prompt.take_prompt_vars(prompt_vars),
            p.prompt.vars.take_hidden_change(),
            p.prompt.take_seen(),
            p.prompt.take_status_change(),
            watched.then(|| crate::prompt::prompt_state(&p)),
            clock_after(&p, Instant::now()),
        )
    };
    if !out.is_empty() {
        perf.output_emits += 1;
        perf.output_emit_bytes +=
            (out.bytes.len() + out.hold.len() + out.replace.as_ref().map_or(0, |r| r.bytes.len()))
                as u64;
        *seen = emit_session_output(app, &out, settle);
    }
    settle.queue_rows(log);
    if let Some(character) = character {
        log_session.name(logs, &character).await;
    }
    for trigger in gag_without_reader {
        if let Err(e) = app.emit(
            events::PROMPT_GAG_WITHOUT_READER,
            GagWithoutReaderPayload { trigger },
        ) {
            warn!(error = %e, "failed to emit a trigger that hides the prompt");
        }
    }
    if let Some(vars) = vars {
        send_prompt_vars(app, &vars);
    }
    if let Some(hidden) = hidden {
        if let Err(e) = app.emit(events::HIDDEN, hidden) {
            warn!(error = %e, "failed to emit the hidden state");
        }
    }
    report_game_prompt_seen(app, prompt_seen);
    if let Some(status) = status {
        if let Err(e) = app.emit(events::PROMPT_STATUS, status) {
            warn!(error = %e, "failed to emit the prompt status");
        }
    }
    emit_prompt_state(app, prompt_state);
    clock
}

/// The prompt state while the card watches your prompt, for
/// `session://prompt-state` after a repaint. None while it does not.
fn watched_state<R: tauri::Runtime>(
    app: &AppHandle<R>,
    p: &Profile,
) -> Option<vosh_prompt::card::state::PromptState> {
    watching_prompt(app).then(|| crate::prompt::prompt_state(p))
}

/// Send `state` on `session://prompt-state`, when there is one.
fn emit_prompt_state<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: Option<vosh_prompt::card::state::PromptState>,
) {
    if let Some(state) = state {
        if let Err(e) = app.emit(events::PROMPT_STATE, state) {
            warn!(error = %e, "failed to emit the prompt state");
        }
    }
}

/// The prompt card watches your prompt (`prompt_watch`), so the prompt
/// state follows each prompt Vosh reads.
fn watching_prompt<R: tauri::Runtime>(app: &AppHandle<R>) -> bool {
    app.try_state::<crate::app::state::SharedState>()
        .is_some_and(|state| {
            state
                .prompt_watch
                .load(std::sync::atomic::Ordering::Acquire)
        })
}

/// Tell the webview what the game said of your prompt settings, on
/// `session://game-prompt-seen`. When the active profile's capture took
/// a new setting, the profile saves shortly and every window reads the
/// `[prompt]` table again.
pub(crate) fn report_game_prompt_seen<R: tauri::Runtime>(
    app: &AppHandle<R>,
    seen: Vec<vosh_prompt::GamePromptSeen>,
) {
    let applied = seen.iter().any(|s| s.applied);
    for payload in seen {
        if let Err(e) = app.emit(events::GAME_PROMPT_SEEN, payload) {
            warn!(error = %e, "failed to emit the game's prompt settings");
        }
    }
    if applied {
        crate::disk::save::mark_profile_dirty(app);
        broadcast_list_changes(app, ListChanges::PROMPT);
    }
}

/// `session://prompt-gag-without-reader`: a trigger hid your prompt and
/// set prompt values while this profile reads no prompt, so Vosh drew
/// nothing in its place.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct GagWithoutReaderPayload {
    pub trigger: String,
}

/// The log's row for this connection, and whether it names the
/// character yet.
struct LogSession {
    id: Option<i64>,
    named: bool,
}

impl LogSession {
    fn new(id: Option<i64>) -> Self {
        Self { id, named: false }
    }

    /// Name the character the row belongs to, the first time Char.Status
    /// names one, so the prompt lookup can tell whose session it was.
    /// Char.Status comes again on later pulses, and those write nothing.
    async fn name(&mut self, logs: &crate::logs::SharedLogStore, character: &str) {
        let Some(id) = self.id else {
            return;
        };
        if self.named {
            return;
        }
        self.named = true;
        let mut guard = logs.lock().await;
        if let Some(store) = guard.as_mut() {
            if let Err(e) = store.set_session_character(id, character) {
                warn!(error = %e, "failed to name the log session's character");
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_gmcp<R: tauri::Runtime>(
    app: &AppHandle<R>,
    profile: &Arc<Mutex<Profile>>,
    timers: &SharedTimers,
    stream: &mut Stream,
    payload: &[u8],
    batch: &mut ReadBatch,
    perf: &mut PerfCounters,
) -> std::io::Result<()> {
    let msg = match vosh_protocol::gmcp::parse(payload) {
        Ok(m) => m,
        Err(e) => {
            warn!(error = %e, "failed to parse GMCP payload");
            return Ok(());
        }
    };
    // Package name at info; the full payload only at debug. Display-
    // formatting every radius-7 Map.Tiles grid into a log line sat on
    // the session hot path per movement. Capture raw payloads with
    // RUST_LOG=vosh_app_lib=debug when needed (e.g. the Group.Info
    // duplicate-member server bug).
    info!(package = %msg.package, "gmcp received");
    tracing::debug!(package = %msg.package, data = %msg.data, "gmcp payload");
    // Phase 5: same fold as the per-line path. Take the tick step for a
    // World.Time hour change under the existing lock so it does not force
    // a second `profile.lock().await` after release.
    let (tick_step, script_apply) = {
        let lock_t0 = std::time::Instant::now();
        let mut p = profile.lock().await;
        perf.mutex_wait_ns += lock_t0.elapsed().as_nanos() as u64;
        perf.mutex_acquires += 1;
        gmcp_step(&mut p, &msg, Instant::now())
    };

    // Char.Status / Char.Name carry the logged-in character name on
    // Aabahran (and most ROM derivatives). Extract it so the auto-
    // switch path can re-resolve profiles against the now-known
    // character. The handler short-circuits on duplicate observations
    // so this is cheap even though Char.Status fires every vitals
    // update.
    if msg.package == "Char.Status" || msg.package == "Char.Name" {
        if let Some(name) = msg.data.get("name").and_then(|v| v.as_str()) {
            let owned = name.trim().to_string();
            if !owned.is_empty() {
                if msg.package == "Char.Status" {
                    batch.character = Some(owned.clone());
                }
                let state = app.state::<crate::app::state::SharedState>();
                crate::profile::switch::handle_char_known_for_auto_switch(
                    app,
                    state.inner(),
                    &owned,
                )
                .await;
            }
        }
    }
    let mut sink = OutputSink::Batch(batch);
    if let Some(step) = tick_step {
        perf.tick_emits += 1;
        deliver_tick_step(app, stream, profile, timers, step, &mut sink).await?;
    }
    apply_script_result(
        app,
        &mut ScriptIo::Session(stream, &mut sink),
        profile,
        timers,
        script_apply,
    )
    .await?;
    // Phase 4 perf fix: emit on a per-package event channel so each
    // frontend listener subscribes only to the packages it cares
    // about, instead of all 12 listeners running on every packet and
    // filtering by `payload.package === '...'`. Tauri event names
    // only allow alphanumeric, `-`, `/`, `:`, `_`, so we have to
    // encode the `.` that GMCP packages use as a namespace
    // separator (`Char.Vitals` → `Char-Vitals`). The frontend's
    // `onGmcpPackage` helper does the same replacement when
    // computing its listen target.
    // Keep the last affects list for a window that opens between ticks.
    app.state::<crate::app::state::SharedState>()
        .last_affects
        .observe(&msg.package, &msg.data);
    // A list that changes the affect fulls sends them first, so the
    // windows never draw the list against the old ones (a recast at
    // fewer hours than the old full).
    crate::affect_full::observe(app, &msg.package, &msg.data);
    let event_name = format!("session://gmcp/{}", msg.package.replace('.', "-"));
    if let Err(e) = app.emit(&event_name, &msg.data) {
        warn!(error = %e, package = %msg.package, "failed to emit GMCP event");
    }
    // `perf.gmcp_packets` already incremented by the caller before
    // we ran. This `emit` count would otherwise duplicate that, so
    // we leave gmcp_packets as the single source.
    Ok(())
}

/// What a GMCP packet does to the profile, under the profile lock the
/// caller holds: the variables and the custom prompt take it, Room.Chars
/// is kept for the target commands, a World.Time hour change is the
/// tick, and Lua GMCP handlers run. Returns the tick step and what the
/// handlers asked for, which the caller delivers once the lock drops.
fn gmcp_step(
    p: &mut Profile,
    msg: &vosh_protocol::gmcp::Message,
    now: Instant,
) -> (Option<TickStep>, ApplyResult) {
    gmcp_vars::apply(&mut p.vars, msg);
    // Before Lua, so a value a GMCP handler sets with
    // `mud.set_prompt_var` belongs to the pulse this packet starts.
    observe_prompt_gmcp(p, msg);
    // Cache the latest Room.Chars snapshot in the profile so
    // bare `tar <index>` / `tarn` / `tarp` commands can resolve
    // against the current room without round-tripping to the
    // frontend.
    if msg.package == "Room.Chars" {
        if let Some(arr) = msg.data.as_array() {
            // The look this packet goes with lists one line for each
            // entry after its things.
            p.room_block.room_chars(arr.len());
            let chars: Vec<crate::profile::RoomChar> = arr
                .iter()
                .filter_map(|v| {
                    let obj = v.as_object()?;
                    let name = obj.get("name").and_then(|n| n.as_str())?.to_string();
                    if name.is_empty() {
                        return None;
                    }
                    let npc = match obj.get("npc") {
                        Some(serde_json::Value::Bool(b)) => *b,
                        Some(serde_json::Value::String(s)) => s == "1" || s == "true",
                        Some(serde_json::Value::Number(n)) => n.as_i64().is_some_and(|x| x != 0),
                        _ => false,
                    };
                    Some(crate::profile::RoomChar { name, npc })
                })
                .collect();
            crate::input::target::set_room_chars(p, chars);
        }
    }
    // The look this packet goes with lists a line for each long text its
    // objects share, five spaces or their count before it.
    if msg.package == "Room.Items" {
        if let Some(arr) = msg.data.as_array() {
            p.room_block.room_items(arr.len());
        }
    }
    let tick_step = crate::tick::observe_world_time_for_tick(&mut p.tick, msg, now);
    script::snapshot_vars(&p.script, &p.vars);
    let outcome = match p.script.dispatch_gmcp(&msg.package, &msg.data) {
        Ok(o) => o,
        Err(err) => {
            warn!(error = %err, "lua dispatch_gmcp failed");
            vosh_script::ScriptOutcome::default()
        }
    };
    let apply = script::apply_actions(p, outcome);
    (tick_step, apply)
}

fn hello_subnegotiation() -> Vec<u8> {
    let body = vosh_protocol::gmcp::build(
        "Core.Hello",
        &json!({
            "client": "vosh",
            "version": env!("CARGO_PKG_VERSION"),
        }),
    )
    .unwrap_or_default();
    Negotiator::build_gmcp_subnegotiation(&body)
}

fn supports_subnegotiation() -> Vec<u8> {
    let body = vosh_protocol::gmcp::build("Core.Supports.Set", &REQUESTED_GMCP_PACKAGES.to_vec())
        .unwrap_or_default();
    Negotiator::build_gmcp_subnegotiation(&body)
}

/// Route emissions stay per-line because consumers (chat panel etc.)
/// expect one event per routed line. The volume here is tiny relative
/// to the display stream so per-event cost does not show up as lag.
fn emit_line_routes<R: tauri::Runtime>(app: &AppHandle<R>, result: &LineResult) {
    if let Some(text) = &result.display {
        for pane in &result.routes {
            if let Err(e) = app.emit(
                events::ROUTED,
                RoutedPayload {
                    pane: pane.clone(),
                    text: text.clone(),
                },
            ) {
                warn!(error = %e, "failed to emit routed line");
            }
        }
    }
}

/// Where the bytes and echo lines a script result asks for go.
enum ScriptIo<'a, 'b> {
    /// The session loop: its connection, and the read's batch or the
    /// terminal.
    Session(&'a mut Stream, &'a mut OutputSink<'b>),
    /// Anywhere else, such as a typed line or a plugin load. The bytes
    /// and echo lines collect for the caller, which sends and prints them
    /// with its own.
    Collect {
        bytes: &'a mut Vec<u8>,
        echoes: &'a mut Vec<String>,
    },
}

impl ScriptIo<'_, '_> {
    async fn send(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        match self {
            ScriptIo::Session(stream, _) => {
                stream.write_all(bytes).await?;
                stream.flush().await
            }
            ScriptIo::Collect { bytes: out, .. } => {
                out.extend_from_slice(bytes);
                Ok(())
            }
        }
    }

    fn echo<R: tauri::Runtime>(&mut self, app: &AppHandle<R>, lines: Vec<String>) {
        match self {
            ScriptIo::Session(_, sink) => sink.write(app, framed_echoes(&lines)),
            ScriptIo::Collect { echoes, .. } => echoes.extend(lines),
        }
    }

    async fn prompt_vars<R: tauri::Runtime>(
        &mut self,
        app: &AppHandle<R>,
        profile: &Arc<Mutex<Profile>>,
    ) {
        match self {
            ScriptIo::Session(_, sink) => sink.prompt_vars(app, profile).await,
            ScriptIo::Collect { .. } => emit_prompt_vars(app, profile, true).await,
        }
    }
}

/// How many rounds of `mud.input` lines one script result runs, each
/// round the lines the Lua of the round before asked for. Lua that keeps
/// asking stops here, at the depth an alias may go.
const MUD_INPUT_DEPTH: usize = vosh_automation::alias::DEFAULT_MAX_DEPTH;

/// Perform the IO and timer bookkeeping a script result asks for. Every
/// path that runs Lua applies its result here: the game's lines and
/// GMCP, Lua timers, the lines you type, a Settings timer, the tick
/// command, and a plugin load. Sends and echoes flow to `io`, timers
/// register with the shared list, values a script gave your prompt reach
/// the windows, and `mud.input` lines are run through the input pipeline
/// so they pick up aliases and slash commands too, with all their own Lua
/// asks for applied in turn.
async fn apply_script_result<R: tauri::Runtime>(
    app: &AppHandle<R>,
    io: &mut ScriptIo<'_, '_>,
    profile: &Arc<Mutex<Profile>>,
    timers: &SharedTimers,
    apply: ApplyResult,
) -> std::io::Result<()> {
    let mut apply = apply;
    let mut depth = 0;
    loop {
        // Durable Lua mutations (mud.alias / set_var / group toggles
        // fired by triggers or timers) historically never reached disk.
        // Ride the same debounced persist the slash commands use.
        if apply.durable_changed {
            crate::disk::save::mark_profile_dirty(app);
        }
        // A Lua `mud.alias` changes the list an open Settings page shows.
        broadcast_list_changes(app, apply.lists);

        if !apply.send_bytes.is_empty() {
            io.send(&apply.send_bytes).await?;
        }
        if !apply.echoes.is_empty() {
            io.echo(app, apply.echoes);
        }
        if !apply.new_timers.is_empty() || !apply.cancel_timers.is_empty() {
            // New timers go in before the cancels run, so a timer that
            // one result both starts and cancels never fires. Lua never
            // gives two timers the same id, so a cancel only ever takes
            // the timer it names.
            let mut guard = timers.lock().await;
            guard.extend(apply.new_timers);
            guard.retain(|t| !apply.cancel_timers.contains(&t.timer_id));
        }
        if apply.prompt_vars_changed {
            io.prompt_vars(app, profile).await;
        }
        if apply.inputs.is_empty() {
            return Ok(());
        }
        if depth == MUD_INPUT_DEPTH {
            warn!(depth, "mud.input went too deep");
            io.echo(
                app,
                vec![format!("[mud.input recursion limit hit ({depth})]")],
            );
            return Ok(());
        }
        depth += 1;
        let shared = crate::profile::switch::shared_layer_for_lines(
            app,
            apply.inputs.iter().map(String::as_str),
        )
        .await;
        let FiredRun {
            apply: next,
            shown,
            effects,
        } = {
            let mut p = profile.lock().await;
            run_lines_locked(
                &mut p,
                apply.inputs.iter().map(String::as_str),
                shared.as_ref(),
            )
        };
        crate::disk::save::settle_line_effects(app, effects).await;
        shown.send(app);
        apply = next;
    }
}

/// [`apply_script_result`] outside the session loop, as for a typed line
/// or a plugin load. Returns the bytes for the game and the echo lines
/// for the terminal, which the caller sends and prints.
pub(crate) async fn collect_script_result<R: tauri::Runtime>(
    app: &AppHandle<R>,
    profile: &Arc<Mutex<Profile>>,
    timers: &SharedTimers,
    apply: ApplyResult,
) -> (Vec<u8>, Vec<String>) {
    let mut bytes = Vec::new();
    let mut echoes = Vec::new();
    let mut io = ScriptIo::Collect {
        bytes: &mut bytes,
        echoes: &mut echoes,
    };
    // Collecting writes to no stream, so it never fails.
    if let Err(e) = apply_script_result(app, &mut io, profile, timers, apply).await {
        warn!(error = %e, "applying a script result failed");
    }
    (bytes, echoes)
}

/// Push the prompt vars to the frontend as a single snapshot, the fresh
/// values the capture and scripts set, with a value the game hides as
/// `?`. `always` sends them even when they read as the webview last heard
/// them, as a Lua action that set one asks. Otherwise they go only when
/// they changed, such as when a pulse left the capture's values stale.
/// The vitals store replaces its copy with the payload, so a value that
/// went stale or was unset drops out.
async fn emit_prompt_vars<R: tauri::Runtime>(
    app: &AppHandle<R>,
    profile: &Arc<Mutex<Profile>>,
    always: bool,
) {
    let vars = profile.lock().await.prompt.take_prompt_vars(always);
    if let Some(vars) = vars {
        send_prompt_vars(app, &vars);
    }
}

fn send_prompt_vars<R: tauri::Runtime>(app: &AppHandle<R>, vars: &BTreeMap<String, String>) {
    if let Err(e) = app.emit(events::PROMPT_VARS, vars) {
        warn!(error = %e, "failed to emit prompt vars");
    }
}

async fn fire_due_script_timers<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stream: &mut Stream,
    profile: &Arc<Mutex<Profile>>,
    timers: &SharedTimers,
) -> std::io::Result<()> {
    let now = Instant::now();
    let due: Vec<PendingTimer> = {
        let mut guard = timers.lock().await;
        let (ready, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut *guard)
            .into_iter()
            .partition(|t| t.deadline <= now);
        *guard = keep;
        ready
    };
    if due.is_empty() {
        return Ok(());
    }
    let apply = {
        let mut p = profile.lock().await;
        script::snapshot_vars(&p.script, &p.vars);
        let mut outcome = vosh_script::ScriptOutcome::default();
        for t in due {
            match p.script.fire_timer(t.callback_id) {
                Ok(o) => outcome.actions.extend(o.actions),
                Err(err) => warn!(error = %err, "lua timer fire failed"),
            }
        }
        script::apply_actions(&mut p, outcome)
    };
    let mut sink = OutputSink::Direct;
    let mut io = ScriptIo::Session(stream, &mut sink);
    apply_script_result(app, &mut io, profile, timers, apply).await
}

/// Flush a partial line still buffered when the session ends so the MUD's
/// final output (a logout banner on `quit`, most often) is captured rather
/// than dropped with the session loop's accumulator. The end of its
/// read painted it, so display only needs the terminating newline. The
/// value of this pass is logging it and pushing it into the scrollback
/// ring that the dump persists.
async fn capture_pending_line<R: tauri::Runtime>(
    app: &AppHandle<R>,
    logs: &crate::logs::SharedLogStore,
    log_session_id: Option<i64>,
    scrollback: &crate::logs::SharedScrollback,
    accumulator: &mut LineAccumulator,
) {
    let Some(Partial { bytes, painted }) = accumulator.take_partial() else {
        return;
    };
    let plain = vosh_protocol::ansi::plain_text(&bytes);
    // Terminate the line on screen. Write only what the end of its read
    // did not paint, to avoid printing the goodbye twice.
    let shown = painted.map_or(0, |(_, len)| len.min(bytes.len()));
    let mut out = Vec::with_capacity(bytes.len() - shown + 2);
    out.extend_from_slice(&bytes[shown..]);
    out.extend_from_slice(b"\r\n");
    emit_output(app, out);
    scrollback.lock().await.push(bytes.clone());
    if let Some(sid) = log_session_id {
        let mut guard = logs.lock().await;
        if let Some(store) = guard.as_mut() {
            if let Err(e) = store.append_batch(&[vosh_log::LogEntry {
                session_id: sid,
                ts_ms: now_ms(),
                text: plain,
                raw: Some(bytes),
            }]) {
                warn!(error = %e, "disconnect partial log append failed");
            }
        }
    }
}

async fn send_trigger_outputs(stream: &mut Stream, sends: &[String]) -> std::io::Result<()> {
    if sends.is_empty() {
        return Ok(());
    }
    let mut payload = Vec::new();
    for cmd in sends {
        payload.extend_from_slice(cmd.as_bytes());
        payload.extend_from_slice(b"\r\n");
    }
    stream.write_all(&payload).await?;
    stream.flush().await
}

/// Map a read-side `io::Error` to a short human-readable disconnect
/// reason. The previous "read failed: Connection reset by peer (os
/// error 54)" wording read as an internal panic; MUD quits routinely
/// land here because Diku / ROM derivatives close with `SO_LINGER` 0 and
/// the OS surfaces it as `ECONNRESET`. Phrase it like a normal disconnect
/// instead, and fall back to the raw text for anything we have not
/// classified.
fn format_disconnect_reason(err: &std::io::Error) -> String {
    use std::io::ErrorKind;
    match err.kind() {
        ErrorKind::ConnectionReset => "connection reset by server".to_string(),
        ErrorKind::ConnectionAborted => "connection aborted by server".to_string(),
        ErrorKind::BrokenPipe => "broken pipe".to_string(),
        ErrorKind::UnexpectedEof => "server closed connection".to_string(),
        ErrorKind::TimedOut => "connection timed out".to_string(),
        _ => format!("read failed: {err}"),
    }
}

/// Send one read's output. Returns the output count after it. It asks
/// for no frame, since the session asks for one through `settle` when
/// the burst of reads it came in ends.
fn emit_session_output<R: tauri::Runtime>(
    app: &AppHandle<R>,
    out: &Output,
    settle: &mut Settle,
) -> u64 {
    settle.drew();
    emit_counted(app, out, true, true, false)
}

fn emit_state<R: tauri::Runtime>(app: &AppHandle<R>, payload: StatePayload) {
    if let Err(e) = app.emit(events::STATE, payload) {
        warn!(error = %e, "failed to emit session state");
    }
}

fn emit_input_mode<R: tauri::Runtime>(app: &AppHandle<R>, password: bool) {
    if let Err(e) = app.emit(events::INPUT_MODE, InputModePayload { password }) {
        warn!(error = %e, "failed to emit input mode");
    }
}

#[cfg(test)]
mod tests;

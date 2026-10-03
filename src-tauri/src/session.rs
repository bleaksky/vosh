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
use crate::profile::Profile;
use crate::profile_config::SharedLayer;
use crate::script::{self, ApplyResult, PendingTimer, SharedTimers};
use crate::tick::{TickRuntime, TickStep};

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
pub(crate) struct OutputPayload {
    /// Output bytes as standard base64. A raw `Vec<u8>` serializes to a
    /// JSON array of decimal numbers (~4x the wire size, one number per
    /// byte, plus an N-element JS array to walk on the other side);
    /// base64 is a single compact string the webview decodes in one pass.
    pub b64: String,
    /// Replace a region an earlier payload marked, applied before `b64`.
    /// See `vosh_prompt::stage::Replace`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replace: Option<ReplacePayload>,
    /// The live render for the region this payload leaves open, as
    /// base64, written back before anything else lands on the renderer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restore: Option<String>,
    /// What the band above the command line shows from now on, as base64,
    /// while your prompt shows pinned. An empty string clears the band.
    /// Only the band reads it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pin: Option<String>,
    /// Where each piece of your design landed on the band `pin` shows,
    /// rows counted from the band's first. Absent when the band shows no
    /// design. See `vosh_prompt::stage::Output::pin_spans`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pin_spans: Option<Vec<vosh_prompt::Span>>,
    /// Line ends at the end of this payload that each renderer keeps back
    /// until the next write lands on it, as base64. See
    /// `vosh_prompt::stage::Output::hold`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold: Option<String>,
    /// While your prompt shows pinned, whether the pinned prompt's row is
    /// still where the next thing lands after this payload, so each
    /// renderer drops the line end that would end that row. See
    /// `vosh_prompt::stage::close_pin_row`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pin_row: Option<bool>,
    /// Which output of the prompt stage this is (`Output::id`). Each
    /// renderer keeps the newest it took, so text the webview writes
    /// itself can tell the session which output it follows. Absent on
    /// output from elsewhere, such as a slash command's echo, which the
    /// stage never sees.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
}

/// `OutputPayload.replace`: region `gen`, its new bytes as base64, and
/// whether they are written on a new row when the region is closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ReplacePayload {
    pub gen: u64,
    pub b64: String,
    pub fresh: bool,
    /// The lines the region's prompt shows right above it, which a change
    /// of where your prompt shows moves with it. See
    /// `vosh_prompt::stage::Above`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub above: Option<AbovePayload>,
    /// The end of the region the bytes leave out, as base64, which a
    /// renderer that writes them on a new row, or finds the region open
    /// with nothing held back, holds back in their place. See
    /// `vosh_prompt::stage::Replace::tail`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tail: Option<String>,
}

/// `ReplacePayload.above`: the lines' plain text, and what to write from
/// their first row, as base64, when a renderer finds them there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct AbovePayload {
    pub plain: String,
    pub b64: String,
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

/// The test event a frame request sends, so a test can count frames.
#[cfg(test)]
pub(crate) const TEST_FRAME_EVENT: &str = "test://frame";

/// Ask the native renderer for a frame of what the grid holds now.
fn request_frame<R: tauri::Runtime>(app: &AppHandle<R>) {
    #[cfg(native_surface)]
    crate::native_surface::request_redraw();
    #[cfg(test)]
    let _ = app.emit(TEST_FRAME_EVENT, ());
    #[cfg(not(test))]
    let _ = app;
}

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
#[allow(clippy::too_many_arguments)]
pub(crate) async fn spawn<R: tauri::Runtime>(
    app: AppHandle<R>,
    host: String,
    port: u16,
    tls: bool,
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
        forsaken_host(&host, port),
    ));

    Ok(SessionHandle { tx_outgoing, task })
}

/// True when the host is The Forsaken Lands, whose rules the custom
/// prompt follows (D17). A test can have the fake Aabahran on a local
/// port count as The Forsaken Lands too.
fn forsaken_host(host: &str, port: u16) -> bool {
    crate::profile_set::is_forsaken_lands(host) || forsaken_for_test(port)
}

/// The local ports tests have count as The Forsaken Lands. Each fake game
/// listens on a port of its own, and a port leaves the list when its test
/// ends, so one test never changes another, even when a later fake game
/// gets the same port.
#[cfg(test)]
static FORSAKEN_TEST_PORTS: std::sync::Mutex<Vec<u16>> = std::sync::Mutex::new(Vec::new());

/// A local port that counts as The Forsaken Lands until this drops.
#[cfg(test)]
#[derive(Debug)]
pub(crate) struct ForsakenTestPort(u16);

#[cfg(test)]
impl Drop for ForsakenTestPort {
    fn drop(&mut self) {
        if let Ok(mut ports) = FORSAKEN_TEST_PORTS.lock() {
            ports.retain(|&port| port != self.0);
        }
    }
}

/// Have the fake game on the local `port` count as The Forsaken Lands, so
/// a capture that reads no Aabahran codes plays by its rules there, until
/// the returned guard drops.
#[cfg(test)]
pub(crate) fn count_as_forsaken_lands(port: u16) -> ForsakenTestPort {
    if let Ok(mut ports) = FORSAKEN_TEST_PORTS.lock() {
        ports.push(port);
    }
    ForsakenTestPort(port)
}

#[cfg(test)]
fn forsaken_for_test(port: u16) -> bool {
    FORSAKEN_TEST_PORTS
        .lock()
        .is_ok_and(|ports| ports.contains(&port))
}

#[cfg(not(test))]
fn forsaken_for_test(_port: u16) -> bool {
    false
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
            crate::input::set_room_chars(p, chars);
        }
    }
    // The look this packet goes with lists a line for each long text its
    // objects share, five spaces or their count before it.
    if msg.package == "Room.Items" {
        if let Some(arr) = msg.data.as_array() {
            p.room_block.room_items(arr.len());
        }
    }
    let tick_step = observe_world_time_for_tick(&mut p.tick, msg, now);
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

/// Detect the game's tick from a GMCP `World.Time` push. Aabahran (and
/// most ROM derivatives that ship World.Time) advance the `hour` field
/// every server tick, so an hour change is the tick. Returns the step to
/// deliver when the change counted as a tick.
fn observe_world_time_for_tick(
    tick: &mut TickRuntime,
    msg: &vosh_protocol::gmcp::Message,
    now: Instant,
) -> Option<TickStep> {
    if msg.package != "World.Time" {
        return None;
    }
    let hour_str = match msg.data.as_object()?.get("hour")? {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => return None,
    };
    if tick.observe_world_hour(&hour_str) {
        tick.on_game_tick(now)
    } else {
        None
    }
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

/// Standard base64 (RFC 4648, padded) encoder. Hand-rolled to keep the
/// output hot path dependency-free; the webview decodes with the
/// built-in `atob`.
pub(crate) fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(*chunk.get(1).unwrap_or(&0));
        let b2 = u32::from(*chunk.get(2).unwrap_or(&0));
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

impl OutputPayload {
    /// Build the wire payload for an output, its bytes as base64.
    pub(crate) fn from_output(out: &Output) -> Self {
        Self {
            b64: base64_encode(&out.bytes),
            replace: out.replace.as_ref().map(|r| ReplacePayload {
                gen: r.gen,
                b64: base64_encode(&r.bytes),
                fresh: r.fresh,
                above: r.above.as_ref().map(|a| AbovePayload {
                    plain: a.plain.clone(),
                    b64: base64_encode(&a.bytes),
                }),
                tail: (!r.tail.is_empty()).then(|| base64_encode(&r.tail)),
            }),
            restore: out.restore.as_deref().map(base64_encode),
            pin: out.pin.as_deref().map(base64_encode),
            pin_spans: out.pin_spans.clone(),
            hold: (!out.hold.is_empty()).then(|| base64_encode(&out.hold)),
            pin_row: out.pin_row,
            id: None,
        }
    }
}

/// Held across both halves of [`emit_output`], so the native grid and
/// xterm take the output of every caller in the same order.
static OUTPUT_ORDER: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// How many outputs reached the terminal, repaints aside. The session
/// notes it after each of its writes, and a count that moved since means
/// output from elsewhere, such as a slash command's echo, landed after
/// the open row and closed it.
static OUTPUT_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The output count now, see [`OUTPUT_COUNT`].
fn output_count() -> u64 {
    OUTPUT_COUNT.load(std::sync::atomic::Ordering::Acquire)
}

/// Print `bytes` in the terminal. Every write to the terminal pane goes
/// through here or through the session's own batch: a slash command's
/// echo, the `#logs` reply, a timer's echo, and the rest. Nothing else
/// emits `session://output`. It moves the output count, so it closes
/// the open row.
pub(crate) fn emit_output<R: tauri::Runtime>(app: &AppHandle<R>, bytes: Vec<u8>) {
    let mut out = Output::new(false);
    out.text(&bytes);
    emit_counted(app, &out, true, false, true);
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

/// Send a repaint of the open row. It leaves the output count alone,
/// since the row it writes is still the last thing on screen.
fn emit_repaint<R: tauri::Runtime>(app: &AppHandle<R>, out: &Output) {
    let _ = emit_counted(app, out, false, true, true);
}

/// Send `out` to both renderers under [`OUTPUT_ORDER`]. `count` moves the
/// output count, which a repaint of the open row never does. `staged`
/// says the prompt stage made `out`, so it carries its id, which each
/// renderer keeps as the newest it took. The session task makes and
/// sends those in order. Output from elsewhere can take an id before an
/// output of the session and still go out after it, so it carries none.
/// `frame` asks the native renderer for a frame at once. Returns the
/// count after it.
fn emit_counted<R: tauri::Runtime>(
    app: &AppHandle<R>,
    out: &Output,
    count: bool,
    staged: bool,
    frame: bool,
) -> u64 {
    let id = staged.then(|| out.id());
    let payload = OutputPayload {
        id,
        ..OutputPayload::from_output(out)
    };
    // The session loop and the command handlers write from different
    // tasks. Without the lock, two writes could reach the grid in one
    // order and xterm in the other.
    let _order = OUTPUT_ORDER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let seen = if count {
        OUTPUT_COUNT.fetch_add(1, std::sync::atomic::Ordering::AcqRel) + 1
    } else {
        output_count()
    };
    // Tier 3: feed the native terminal grid the same bytes xterm receives,
    // for every output path, then repaint. This is the single choke point
    // so nothing reaches xterm without also reaching the grid.
    // Word wrapped at the grid width, matching the frontend WordWrapper
    // that xterm receives this same stream through. The grid finds each
    // region in its own rows, as xterm does.
    #[cfg(native_surface)]
    crate::term_grid::feed_session_output(out, id);
    if frame {
        request_frame(app);
    }
    if let Err(e) = app.emit(events::OUTPUT, payload) {
        warn!(error = %e, "failed to emit session output");
    }
    seen
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
mod tests {
    use super::base64_encode;
    use crate::input::LineEffects;
    use crate::profile::Profile;

    #[test]
    fn a_test_port_counts_as_the_forsaken_lands_only_while_its_guard_lives() {
        // Port 1 is never a fake game's, which binds port 0.
        assert!(!super::forsaken_host("127.0.0.1", 1));
        let guard = super::count_as_forsaken_lands(1);
        assert!(super::forsaken_host("127.0.0.1", 1));
        assert!(!super::forsaken_host("127.0.0.1", 2));
        drop(guard);
        assert!(!super::forsaken_host("127.0.0.1", 1));
        assert!(super::forsaken_host("play.theforsakenlands.com", 1));
    }

    #[test]
    fn core_supports_set_names_every_package_vosh_reads() {
        let body = vosh_protocol::gmcp::build(
            "Core.Supports.Set",
            &super::REQUESTED_GMCP_PACKAGES.to_vec(),
        )
        .expect("the list serializes");
        let msg = vosh_protocol::gmcp::parse(&body).expect("the body parses");
        let modules: Vec<&str> = msg
            .data
            .as_array()
            .expect("a list")
            .iter()
            .filter_map(|v| v.as_str()?.split(' ').next())
            .collect();
        for read in [
            "Char.Vitals",
            "Char.Affects",
            "Char.Combat",
            "Char.Prompt",
            "Char.State",
            "Char.Worth",
            "Room.Info",
            "Room.Weather",
            "Comm.Channel",
            "World.Time",
            "World.Moons",
            "Map.Tiles",
            "Imm.Queues",
            "Group.Info",
        ] {
            assert!(
                modules
                    .iter()
                    .any(|m| read == *m || read.starts_with(&format!("{m}."))),
                "Core.Supports.Set leaves out {read}"
            );
        }
    }

    #[test]
    fn a_timer_command_that_edits_the_profile_marks_it_dirty() {
        let mut p = Profile::default();
        let run = super::run_fired_locked(&mut p, "#alias greet wave", None);
        assert!(run.effects.dirty);
        assert!(run.apply.lists.aliases);
        assert!(p.aliases.get("greet").is_some());

        let run = super::run_fired_locked(&mut p, "#trigger flee {^You flee} send look", None);
        assert!(run.effects.dirty);
        assert!(run.apply.lists.triggers);

        // A plain command leaves the saved profile alone.
        let run = super::run_fired_locked(&mut p, "greet", None);
        assert_eq!(run.effects, LineEffects::default());
        assert_eq!(run.apply.send_bytes, b"wave\r\n");
    }

    #[test]
    fn a_script_alias_body_hands_on_all_it_asks_for() {
        let mut p = Profile::default();
        p.aliases.set(
            vosh_automation::alias::Alias::new("kk", "ignored").with_script(
                "mud.echo('ready') mud.send(captures[1]) mud.timer(1, function() end) \
             mud.input('look') mud.set_prompt_var('mark', 'on')",
            ),
        );
        let ran = crate::input::run_line(&mut p, "stand;kk orc");
        let apply = super::line_script_result(ran);
        // What the body sends goes out where you typed the alias, and all
        // else it asks for comes with the line.
        assert_eq!(apply.send_bytes, b"stand\r\norc\r\n");
        assert_eq!(apply.echoes, ["ready"]);
        assert_eq!(apply.new_timers.len(), 1);
        assert_eq!(apply.inputs, ["look"]);
        assert!(apply.prompt_vars_changed);
    }

    #[test]
    fn a_timer_command_runs_the_body_of_a_lua_alias() {
        let mut p = Profile::default();
        p.aliases.set(
            vosh_automation::alias::Alias::new("kk", "ignored")
                .with_script("mud.send('kick ' .. captures[1])\nmud.echo('kicked')"),
        );
        let run = super::run_fired_locked(&mut p, "kk dragon", None);
        assert_eq!(run.apply.send_bytes, b"kick dragon\r\n");
        assert_eq!(run.apply.echoes, ["kicked"]);
        assert_eq!(run.effects, LineEffects::default());
        // What the body sends goes out where the command names the alias.
        let run = super::run_fired_locked(&mut p, "kk dragon;wave", None);
        assert_eq!(run.apply.send_bytes, b"kick dragon\r\nwave\r\n");
        let run = super::run_fired_locked(&mut p, "wave;kk dragon;bow", None);
        assert_eq!(run.apply.send_bytes, b"wave\r\nkick dragon\r\nbow\r\n");
    }

    #[test]
    fn a_trigger_body_reads_the_whole_match_then_each_group() {
        // Unlike an alias body, captures[1] is the whole match, so a
        // trigger that reads hp from captures[2] keeps reading it.
        let mut p = Profile::default();
        p.triggers
            .set(vosh_automation::trigger::Trigger::new(
                "says",
                r"^(\w+) says (\w+)",
                vosh_automation::trigger::TriggerAction::Script {
                    body: "mud.send(captures[1] .. '|' .. captures[2] .. '|' .. captures[3])"
                        .into(),
                },
            ))
            .expect("the trigger compiles");
        let result = vosh_automation::trigger::process(&p.triggers, b"Bob says hi");
        assert_eq!(
            super::run_trigger_scripts(&mut p, &result, "t"),
            [vosh_script::Action::Send("Bob says hi|Bob|hi".into())]
        );
    }

    #[test]
    fn a_timer_group_line_reports_a_macro_group_that_turned() {
        // The command line keeps its own map of the macro keys that fire,
        // so a timer or tick `#group` line that turns a macro group off
        // has to reach it, or the keys go on firing.
        let mut p = Profile::default();
        p.macros.push(crate::profile::Macro {
            key: "F1".into(),
            command: "kick".into(),
            group: Some("combat".into()),
            enabled: true,
        });
        let run = super::run_fired_locked(&mut p, "#group combat off", None);
        assert!(run.apply.lists.macro_groups);
        assert!(p.disabled_macro_groups.contains("combat"));
        // Off already, so nothing turned.
        let run = super::run_fired_locked(&mut p, "#group combat off", None);
        assert!(!run.apply.lists.macro_groups);
    }

    #[test]
    fn tick_and_lua_lines_note_what_they_ask_of_the_profile() {
        let mut p = Profile::default();
        let mut effects = LineEffects::default();
        let _ = super::process_fired_line(&mut p, "#alias greet wave", &mut effects, None);
        assert!(effects.dirty);
        assert!(p.aliases.get("greet").is_some());

        // A reset from a timer or a script keeps the blanked profile off
        // the disk, as it does when you type it.
        let _ = super::process_fired_line(&mut p, "#profile reset", &mut effects, None);
        assert_eq!(
            effects,
            LineEffects {
                replaced: true,
                dirty: false,
                tick_changed: false,
            }
        );
        assert!(p.aliases.get("greet").is_none());
    }

    #[test]
    fn a_reset_from_a_timer_turns_away_a_config_save_read_before_it() {
        let mut p = Profile::default();
        let mut effects = LineEffects::default();
        let before = crate::app::state::ui_config_generation();
        let _ = super::process_fired_line(&mut p, "#profile reset", &mut effects, None);
        assert!(crate::app::state::ui_config_generation() > before);
    }

    #[test]
    fn a_tick_command_from_a_timer_notes_the_tick_change() {
        let mut p = Profile::default();
        let run = super::run_fired_locked(&mut p, "#tick warn at 10", None);
        assert!(run.effects.tick_changed);
        assert_eq!(p.tick.config.warn_at_secs, Some(10));
        let run = super::run_fired_locked(&mut p, "#tick", None);
        assert!(!run.effects.tick_changed);
    }

    #[test]
    fn a_timer_command_says_what_it_changed_outside_the_text() {
        let mut p = Profile::default();
        // A plain command changes neither, so nothing goes out.
        let run = super::run_fired_locked(&mut p, "look", None);
        assert_eq!(run.shown, super::ShownChanges::default());

        let run = super::run_fired_locked(&mut p, "tar goblin", None);
        let target = run.shown.target.expect("the new target");
        assert_eq!(target.name.as_deref(), Some("goblin"));
        assert!(!run.shown.repaint);
        // The same target again changes nothing.
        let run = super::run_fired_locked(&mut p, "tar goblin", None);
        assert_eq!(run.shown, super::ShownChanges::default());

        let run = super::run_fired_locked(&mut p, "#prompt default", None);
        assert!(run.shown.repaint);
        assert!(run.shown.target.is_none());
    }

    #[test]
    fn a_timer_reset_keeps_the_shared_settings() {
        // No global.toml yet, so the live shared values are the ones to
        // keep, as they are for a reset you type.
        let dir = tempfile::tempdir().unwrap();
        let layer = crate::profile_config::SharedLayer::read(
            &dir.path().join("global.toml"),
            crate::profile_set::ScopeConfig::default(),
        );
        let mut p = Profile::default();
        p.ui.theme = "night-ink".into();
        p.ui.font_family = "Iosevka".into();
        p.ui.keep_last_command = true;
        let _ = super::run_fired_locked(&mut p, "#alias greet wave", None);

        let run = super::run_fired_locked(&mut p, "#profile reset", Some(&layer));

        assert!(run.effects.replaced);
        assert!(p.aliases.get("greet").is_none());
        assert_eq!(p.ui.theme, "night-ink");
        assert_eq!(p.ui.font_family, "Iosevka");
        assert!(p.ui.keep_last_command);
    }

    fn world_time(hour: serde_json::Value) -> vosh_protocol::gmcp::Message {
        vosh_protocol::gmcp::Message {
            package: "World.Time".into(),
            data: serde_json::json!({ "hour": hour }),
        }
    }

    #[test]
    fn a_world_hour_change_is_the_tick_and_fires_once() {
        let t0 = tokio::time::Instant::now();
        let mut tick = crate::tick::TickRuntime::default();
        tick.config.auto_fire = Some("score".into());
        tick.start_session(t0);
        let at = |s: u64| t0 + std::time::Duration::from_secs(s);

        // The first hour of the session primes.
        assert!(
            super::observe_world_time_for_tick(&mut tick, &world_time(9.into()), at(1)).is_none()
        );
        // The same hour again is no tick.
        assert!(
            super::observe_world_time_for_tick(&mut tick, &world_time(9.into()), at(5)).is_none()
        );
        let step = super::observe_world_time_for_tick(&mut tick, &world_time("10".into()), at(12))
            .expect("the hour moved");
        assert!(step.payload.fired);
        assert_eq!(step.command.as_deref(), Some("score"));
        assert!(tick.synced);
        // A Reset on line for the same tick does not fire again.
        assert!(tick.on_game_tick(at(13)).is_none());
        // Past the interval the timer waits for the next hour.
        assert!(!tick.poll(at(45)).payload.fired);
        let step = super::observe_world_time_for_tick(&mut tick, &world_time(11.into()), at(46))
            .expect("the next tick");
        assert!(step.payload.fired);

        // Other packages and a World.Time without an hour are no tick.
        let other = vosh_protocol::gmcp::Message {
            package: "Char.Vitals".into(),
            data: serde_json::json!({ "hour": 12 }),
        };
        assert!(super::observe_world_time_for_tick(&mut tick, &other, at(80)).is_none());
        let no_hour = vosh_protocol::gmcp::Message {
            package: "World.Time".into(),
            data: serde_json::json!({ "sunlight": "light" }),
        };
        assert!(super::observe_world_time_for_tick(&mut tick, &no_hour, at(80)).is_none());
    }

    /// James's design, with colors by how full and the `%)h` trick that
    /// prints a percent sign.
    const TEMPLATE: &str = vosh_prompt::testkit::designs::JAMES;

    /// The game prompt `%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c` at full.
    const PROMPT_LINE: &str = "[1020/1020hp 800/800mn 930/930mv]";

    /// The capture the migration writes from the trigger `#prompt` used
    /// to write, unanchored as the trigger was.
    const CAPTURE: &str = r"\[(?<hp>\d+)/(?<maxhp>\d+)hp (?<mana>\d+)/(?<maxmana>\d+)mn (?<move>\d+)/(?<maxmove>\d+)mv\]";

    /// A regex capture of `pattern`, as `[prompt.capture]` holds it.
    fn regex_capture(pattern: &str, settle: bool) -> vosh_prompt::CaptureConfig {
        vosh_prompt::CaptureConfig::Regex(vosh_prompt::config::RegexCapture {
            lines: vec![pattern.to_string()],
            settle,
            ..vosh_prompt::config::RegexCapture::default()
        })
    }

    /// A profile that reads the prompt with the migrated capture and
    /// draws `template` in its place.
    fn capture_profile(template: &str) -> Profile {
        let mut p = Profile::default();
        p.set_prompt_config(vosh_prompt::PromptConfig {
            draw: true,
            template: template.to_string(),
            capture: regex_capture(CAPTURE, false),
            ..vosh_prompt::PromptConfig::default()
        });
        p
    }

    /// Run one complete line through the session's line step and return
    /// the drawn prompt, the bytes after its region mark, when it drew.
    fn draw_line(p: &mut Profile, line: &str) -> Option<String> {
        let mut batch = super::ReadBatch::new(super::output_count());
        let _ = super::line_step(
            p,
            &mut batch,
            super::Line {
                bytes: line.as_bytes().to_vec(),
                painted: None,
            },
            vosh_protocol::ansi::plain_text(line.as_bytes()),
            tokio::time::Instant::now(),
            None,
        );
        p.prompt.stage.open_row()?;
        drawn_in(&batch.out.bytes)
    }

    /// The bytes after the last region mark, as text.
    fn drawn_in(bytes: &[u8]) -> Option<String> {
        let text = String::from_utf8_lossy(bytes);
        let at = text.rfind("\x1b]7717;o;")?;
        let rest = &text[at..];
        let end = rest.find('\x07')?;
        Some(rest[end + 1..].to_string())
    }

    fn plain(ansi: &str) -> String {
        vosh_protocol::ansi::plain_text(ansi.as_bytes())
    }

    fn mark(gen: u64) -> Vec<u8> {
        vosh_prompt::stage::mark(gen)
    }

    /// A terminal's worth of the session: the profile and the line
    /// accumulator, fed one read at a time through the same steps the
    /// session runs.
    struct Wire {
        p: Profile,
        acc: super::LineAccumulator,
        /// The telnet parser, for reads of raw wire bytes.
        parser: vosh_protocol::telnet::Parser,
        /// The first generation this wire hands out, so tests can name
        /// the marks by number.
        gen0: u64,
    }

    impl Wire {
        fn new(p: Profile) -> Self {
            let mut wire = Self {
                p,
                acc: super::LineAccumulator::new(),
                parser: vosh_protocol::telnet::Parser::new(),
                gen0: 0,
            };
            wire.gen0 = wire.p.prompt.stage.next_gen();
            wire
        }

        /// The mark for the nth region this wire hands out, from 1.
        fn mark(&self, n: u64) -> Vec<u8> {
            mark(self.gen0 + n)
        }

        /// One socket read: `data`, then `ga` when the read ends in a GA,
        /// then the end of the read. Returns what the terminal gets.
        fn read_with(&mut self, data: &[u8], ga: bool, other: bool) -> super::ReadBatch {
            let mut batch = super::ReadBatch::new(super::output_count());
            batch.out = vosh_prompt::stage::Output::new(other);
            let now = tokio::time::Instant::now();
            for line in self.acc.feed(data) {
                let plain = vosh_protocol::ansi::plain_text(&line.bytes);
                let _ = super::line_step(&mut self.p, &mut batch, line, plain, now, None);
            }
            if ga {
                let _ = super::marker_step(&mut self.p, &mut self.acc, &mut batch, now, None);
            }
            let _ = super::partial_step(&mut self.p, &mut self.acc, &mut batch, now, None);
            // The hold's deadline passes before the next read.
            if batch.hold {
                super::hold_step(&mut self.p, &mut self.acc, &mut batch.out);
            }
            batch
        }

        /// One socket read that leaves a partial waiting, as the session
        /// does until the next read or the deadline. Returns the batch.
        fn read_holding(&mut self, data: &[u8]) -> super::ReadBatch {
            let mut batch = super::ReadBatch::new(super::output_count());
            let now = tokio::time::Instant::now();
            for line in self.acc.feed(data) {
                let plain = vosh_protocol::ansi::plain_text(&line.bytes);
                let _ = super::line_step(&mut self.p, &mut batch, line, plain, now, None);
            }
            let _ = super::partial_step(&mut self.p, &mut self.acc, &mut batch, now, None);
            batch
        }

        fn read(&mut self, data: &[u8]) -> vosh_prompt::stage::Output {
            self.read_with(data, false, false).out
        }

        fn read_ga(&mut self, data: &[u8]) -> vosh_prompt::stage::Output {
            self.read_with(data, true, false).out
        }

        /// You send a line. Held lines let go first, as in the session.
        fn send(&mut self) {
            let mut batch = super::ReadBatch::new(super::output_count());
            let _ = super::let_go_held(&mut self.p, &mut batch, tokio::time::Instant::now(), None);
            let _ = super::send_step(&mut self.p, &self.acc, b"look\r\n", 0);
            self.acc.forget_partial();
        }

        /// You send `line` now.
        fn send_line(&mut self, line: &str) {
            let _ = super::send_step(
                &mut self.p,
                &self.acc,
                format!("{line}\r\n").as_bytes(),
                super::now_ms(),
            );
            self.acc.forget_partial();
        }

        /// One socket read of raw wire bytes, through the telnet parser
        /// and the steps the session runs for each event: text through
        /// the Line pass, each GMCP packet through the GMCP step, and a
        /// GA or EOR through the marker step. Then the end of the read,
        /// and the hold's deadline before the next one.
        fn read_wire(&mut self, data: &[u8]) -> vosh_prompt::stage::Output {
            let mut batch = super::ReadBatch::new(super::output_count());
            batch.out = vosh_prompt::stage::Output::new(false);
            let now = tokio::time::Instant::now();
            for event in self.parser.feed(data) {
                match event {
                    super::TelnetEvent::Data(bytes) => {
                        for line in self.acc.feed(&bytes) {
                            let plain = vosh_protocol::ansi::plain_text(&line.bytes);
                            let _ =
                                super::line_step(&mut self.p, &mut batch, line, plain, now, None);
                        }
                    }
                    super::TelnetEvent::Subnegotiation { option, payload }
                        if option == super::telnet_option::GMCP =>
                    {
                        let msg =
                            vosh_protocol::gmcp::parse(&payload).expect("every packet parses");
                        let _ = super::gmcp_step(&mut self.p, &msg, now);
                    }
                    super::TelnetEvent::Command(byte)
                        if byte == super::telnet_codes::GA || byte == super::telnet_codes::EOR =>
                    {
                        let _ =
                            super::marker_step(&mut self.p, &mut self.acc, &mut batch, now, None);
                    }
                    _ => {}
                }
            }
            let _ = super::partial_step(&mut self.p, &mut self.acc, &mut batch, now, None);
            if batch.hold {
                super::hold_step(&mut self.p, &mut self.acc, &mut batch.out);
            }
            batch.out
        }

        /// One socket read of `events` in order, as the session handles
        /// them, then the end of the read.
        fn read_events(&mut self, events: &[Ev]) -> vosh_prompt::stage::Output {
            let mut batch = super::ReadBatch::new(super::output_count());
            let now = tokio::time::Instant::now();
            for event in events {
                match event {
                    Ev::Data(data) => {
                        for line in self.acc.feed(data) {
                            let plain = vosh_protocol::ansi::plain_text(&line.bytes);
                            let _ =
                                super::line_step(&mut self.p, &mut batch, line, plain, now, None);
                        }
                    }
                    Ev::Ga => {
                        let _ =
                            super::marker_step(&mut self.p, &mut self.acc, &mut batch, now, None);
                    }
                }
            }
            let _ = super::partial_step(&mut self.p, &mut self.acc, &mut batch, now, None);
            if batch.hold {
                super::hold_step(&mut self.p, &mut self.acc, &mut batch.out);
            }
            batch.out
        }
    }

    /// One event of a socket read.
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Ev {
        Data(Vec<u8>),
        Ga,
    }

    /// What the game sends: bytes with a GA wherever `*` stands.
    fn stream(text: &str) -> Vec<Ev> {
        let mut events = Vec::new();
        for (i, part) in text.split('*').enumerate() {
            if i > 0 {
                events.push(Ev::Ga);
            }
            if !part.is_empty() {
                events.push(Ev::Data(part.as_bytes().to_vec()));
            }
        }
        events
    }

    /// `events` cut into two reads after `at` bytes, a GA counting as
    /// one.
    fn split_reads(events: &[Ev], at: usize) -> [Vec<Ev>; 2] {
        let mut reads: [Vec<Ev>; 2] = [Vec::new(), Vec::new()];
        let mut seen = 0;
        for event in events {
            match event {
                Ev::Ga => {
                    reads[usize::from(seen >= at)].push(Ev::Ga);
                    seen += 1;
                }
                Ev::Data(data) => {
                    let cut = at.saturating_sub(seen).min(data.len());
                    if cut > 0 {
                        reads[0].push(Ev::Data(data[..cut].to_vec()));
                    }
                    if cut < data.len() {
                        reads[1].push(Ev::Data(data[cut..].to_vec()));
                    }
                    seen += data.len();
                }
            }
        }
        reads
    }

    /// How many cuts `events` has, a GA counting as one byte.
    fn stream_len(events: &[Ev]) -> usize {
        events
            .iter()
            .map(|e| match e {
                Ev::Data(data) => data.len(),
                Ev::Ga => 1,
            })
            .sum()
    }

    /// The screen a native grid `columns` wide shows after `reads`, rows
    /// trimmed, up to the last row that shows anything.
    fn screen_of(profile: &dyn Fn() -> Profile, columns: usize, reads: &[Vec<Ev>]) -> Vec<String> {
        let mut wire = Wire::new(profile());
        let mut grid = crate::term_grid::TermGrid::new(columns, 40);
        for read in reads {
            grid.session_output(&wire.read_events(read));
        }
        let mut rows: Vec<String> = (0..grid.screen_lines())
            .map(|line| grid.row_string(line).trim_end().to_string())
            .collect();
        while rows.last().is_some_and(String::is_empty) {
            rows.pop();
        }
        rows
    }

    /// Cut `text` into two reads at every byte, and check each screen is
    /// the one a single read gives, at 40 and 12 wide, for the profile
    /// `profile` makes. Returns the 40 wide screen.
    fn same_at_every_split(profile: &dyn Fn() -> Profile, text: &str) -> Vec<String> {
        let events = stream(text);
        let mut wide = Vec::new();
        for columns in [40, 12] {
            let whole = screen_of(profile, columns, std::slice::from_ref(&events));
            for at in 1..stream_len(&events) {
                let reads = split_reads(&events, at);
                assert_eq!(
                    screen_of(profile, columns, &reads),
                    whole,
                    "{columns} wide, cut after {at}: {reads:?}"
                );
            }
            if columns == 40 {
                wide = whole;
            }
        }
        wide
    }

    fn with(parts: &[&[u8]]) -> Vec<u8> {
        parts.concat()
    }

    /// Draws the hp read by the capture, so each byte of a draw is known.
    const HP: &str = "<%hp>";

    #[test]
    fn a_line_prompt_draws_in_place_with_no_line_end() {
        let mut wire = Wire::new(capture_profile(HP));
        let out = wire.read(b"You are hungry.\n\r[1020/1020hp 800/800mn 930/930mv]\n\r");
        assert_eq!(
            out.bytes,
            with(&[b"You are hungry.\r\n", &wire.mark(1), b"<1020>\x1b[0m"])
        );
        assert_eq!(out.replace, None);
        let open = wire.p.prompt.stage.open_row().expect("the open row");
        assert_eq!(open.gen, wire.gen0 + 1);
    }

    #[test]
    fn the_prompt_draws_the_template_byte_for_byte() {
        let mut p = capture_profile(TEMPLATE);
        let drawn = draw_line(&mut p, PROMPT_LINE).expect("the prompt draws");
        assert_eq!(plain(&drawn), "[1020(100%)h 800(100%)m 930(100%)v] ");
        // Health at full in the theme's green, where the first renderer
        // drew 256 color 42. Every other byte is as it drew them.
        assert!(drawn.contains("\x1b[32m100"), "{drawn:?}");
        let first = drawn.replace("\x1b[32m100", "\x1b[38;5;42m100");
        assert_eq!(
            first,
            "\x1b[38;2;100;100;100m[\x1b[0m\x1b[3m1020(\x1b[38;5;42m100\x1b[0m\x1b[3m%)h 800(\x1b[38;2;128;200;255m100\x1b[0m\x1b[3m%)m 930(\x1b[38;2;200;255;23m100\x1b[0m\x1b[3m%)v\x1b[0m\x1b[38;2;100;100;100m] \x1b[0m\x1b[0m"
        );
        assert!(drawn.ends_with("\x1b[0m"));

        // Any other line shows as sent and draws nothing.
        assert_eq!(draw_line(&mut p, "You are hungry."), None);
    }

    #[test]
    fn with_drawing_off_the_prompt_shows_as_sent_and_is_logged() {
        let mut p = capture_profile(HP);
        p.set_prompt_config(vosh_prompt::PromptConfig {
            draw: false,
            ..p.prompt.config().clone()
        });
        let mut wire = Wire::new(p);
        let out = wire.read(b"[1020/1020hp 800/800mn 930/930mv]\n\r");
        assert_eq!(out.bytes, b"[1020/1020hp 800/800mn 930/930mv]\r\n");
        assert_eq!(wire.p.prompt.stage.open_row(), None);
        // The capture still read it.
        let vars = wire.p.prompt.vars.prompt_vars();
        assert_eq!(vars.get("hp").map(String::as_str), Some("1020"));

        let mut batch = super::ReadBatch::new(super::output_count());
        let step = super::line_step(
            &mut wire.p,
            &mut batch,
            super::Line {
                bytes: PROMPT_LINE.as_bytes().to_vec(),
                painted: None,
            },
            PROMPT_LINE.to_string(),
            tokio::time::Instant::now(),
            Some(7),
        );
        assert_eq!(batch.log.len(), 1, "a prompt that shows is logged");
        assert_eq!(step.len(), 1);
        assert_eq!(step[0].scrollback, [PROMPT_LINE.as_bytes()]);
        // A drawn prompt is neither logged nor kept for scrollback.
        let mut p = capture_profile(HP);
        let mut batch = super::ReadBatch::new(super::output_count());
        let step = super::line_step(
            &mut p,
            &mut batch,
            super::Line {
                bytes: PROMPT_LINE.as_bytes().to_vec(),
                painted: None,
            },
            PROMPT_LINE.to_string(),
            tokio::time::Instant::now(),
            Some(7),
        );
        assert!(batch.log.is_empty());
        assert_eq!(step.len(), 1);
        let leftover = &step[0].scrollback;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(batch.prompt_vars, "the prompt vars follow a prompt");
    }

    #[test]
    fn a_profile_without_a_capture_shows_the_game_prompt() {
        let mut p = Profile::default();
        p.set_prompt_config(vosh_prompt::PromptConfig::from_legacy(true, HP));
        let mut wire = Wire::new(p);
        let out = wire.read(b"[1020/1020hp 800/800mn 930/930mv]\n\r");
        assert_eq!(out.bytes, b"[1020/1020hp 800/800mn 930/930mv]\r\n");
        assert_eq!(wire.p.prompt.stage.open_row(), None);
    }

    #[test]
    fn a_prompt_split_across_reads_replaces_its_painted_start() {
        let mut wire = Wire::new(capture_profile(HP));
        let first = wire.read(b"You are hungry.\n\r[1020/1020hp 80");
        assert_eq!(
            first.bytes,
            with(&[b"You are hungry.\r\n", &wire.mark(1), b"[1020/1020hp 80"])
        );
        let second = wire.read(b"0/800mn 930/930mv]\n\r");
        assert_eq!(
            second.replace,
            Some(vosh_prompt::stage::Replace {
                gen: wire.gen0 + 1,
                bytes: with(&[&wire.mark(2), b"<1020>\x1b[0m"]),
                fresh: true,
                above: None,
                tail: Vec::new(),
            })
        );
        let leftover = &second.bytes;
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn a_line_split_across_reads_replaces_its_painted_start() {
        let mut wire = Wire::new(capture_profile(HP));
        let first = wire.read(b"You are hun");
        assert_eq!(first.bytes, with(&[&wire.mark(1), b"You are hun"]));
        // The partial grew, so it paints again whole.
        let second = wire.read(b"gry");
        assert_eq!(
            second.replace,
            Some(vosh_prompt::stage::Replace {
                gen: wire.gen0 + 1,
                bytes: with(&[&wire.mark(2), b"You are hungry"]),
                fresh: true,
                above: None,
                tail: Vec::new(),
            })
        );
        let third = wire.read(b".\n\rNext.\n\r");
        assert_eq!(
            third.replace,
            Some(vosh_prompt::stage::Replace {
                gen: wire.gen0 + 2,
                bytes: b"You are hungry.\r\n".to_vec(),
                fresh: true,
                above: None,
                tail: Vec::new(),
            })
        );
        assert_eq!(third.bytes, b"Next.\r\n");
    }

    #[test]
    fn a_partial_the_capture_settles_on_draws_in_the_same_read() {
        let mut p = capture_profile(HP);
        p.set_prompt_config(vosh_prompt::PromptConfig {
            capture: regex_capture(r"^<(?<hp>\d+)hp (?<mana>\d+)m> $", true),
            ..p.prompt.config().clone()
        });
        let mut wire = Wire::new(p);
        let out = wire.read(b"You are hungry.\n\r<100hp 50m> ");
        assert_eq!(
            out.bytes,
            with(&[b"You are hungry.\r\n", &wire.mark(1), b"<100>\x1b[0m"])
        );
        assert_eq!(wire.acc.partial(), None, "a drawn partial is gone");

        // Split before the final space, it paints and waits, then draws
        // over the painted start.
        let first = wire.read(b"\n\r<90hp 50m>");
        assert_eq!(first.bytes, with(&[b"\r\n", &wire.mark(2), b"<90hp 50m>"]));
        let second = wire.read(b" ");
        assert_eq!(
            second.replace,
            Some(vosh_prompt::stage::Replace {
                gen: wire.gen0 + 2,
                bytes: with(&[&wire.mark(3), b"<90>\x1b[0m"]),
                fresh: true,
                above: None,
                tail: Vec::new(),
            })
        );
    }

    #[test]
    fn an_unanchored_capture_waits_for_the_line_end() {
        let mut wire = Wire::new(capture_profile(HP));
        let out = wire.read(b"[1020/1020hp 800/800mn 930/930mv]");
        assert_eq!(
            out.bytes,
            with(&[&wire.mark(1), b"[1020/1020hp 800/800mn 930/930mv]"])
        );
        assert_eq!(wire.p.prompt.stage.open_row(), None);
    }

    #[test]
    fn a_ga_in_the_same_read_draws_with_no_flash() {
        let mut wire = Wire::new(capture_profile(HP));
        let out = wire.read_ga(b"[1020/1020hp 800/800mn 930/930mv]");
        assert_eq!(out.bytes, with(&[&wire.mark(1), b"<1020>\x1b[0m"]));
        assert_eq!(out.replace, None);
        assert_eq!(wire.acc.partial(), None);
    }

    #[test]
    fn a_ga_in_the_next_read_draws_over_the_painted_prompt() {
        let mut wire = Wire::new(capture_profile(HP));
        let first = wire.read(b"[1020/1020hp 800/800mn 930/930mv]");
        assert_eq!(
            first.bytes,
            with(&[&wire.mark(1), b"[1020/1020hp 800/800mn 930/930mv]"])
        );
        let second = wire.read_ga(b"");
        assert_eq!(
            second.replace,
            Some(vosh_prompt::stage::Replace {
                gen: wire.gen0 + 1,
                bytes: with(&[&wire.mark(2), b"<1020>\x1b[0m"]),
                fresh: true,
                above: None,
                tail: Vec::new(),
            })
        );
        let leftover = &second.bytes;
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn a_ga_on_a_partial_nothing_reads_ends_its_row() {
        let mut wire = Wire::new(Profile::default());
        let out = wire.read_ga(b"<100hp> ");
        assert_eq!(out.bytes, b"<100hp> \r\n");
        let first = wire.read(b"<90hp> ");
        assert_eq!(first.bytes, with(&[&wire.mark(1), b"<90hp> "]));
        let second = wire.read_ga(b"");
        assert_eq!(second.bytes, b"\r\n");
        assert_eq!(second.replace, None);
    }

    #[test]
    fn a_ga_on_a_partial_that_grew_after_its_paint_writes_the_whole_of_it() {
        // The game's prompt, cut inside by TCP, on a profile that reads
        // no prompt.
        let mut wire = Wire::new(Profile::default());
        let first = wire.read(b"Huh?\n\r<100hp 50");
        assert_eq!(
            first.bytes,
            with(&[b"Huh?\r\n", &wire.mark(1), b"<100hp 50"])
        );
        let second = wire.read_ga(b"m 30mv> ");
        assert_eq!(
            second.replace,
            Some(vosh_prompt::stage::Replace {
                gen: wire.gen0 + 1,
                bytes: b"<100hp 50m 30mv> \r\n".to_vec(),
                fresh: true,
                above: None,
                tail: Vec::new(),
            })
        );
        let leftover = &second.bytes;
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn a_game_prompt_nothing_reads_shows_whole_wherever_the_reads_split() {
        let screen = same_at_every_split(
            &Profile::default,
            "Huh?\n\r<1020hp 800m 930mv> *\n\rYou are hungry.\n\r<1020hp 800m 930mv> *",
        );
        assert_eq!(
            screen,
            [
                "Huh?",
                "<1020hp 800m 930mv>",
                "",
                "You are hungry.",
                "<1020hp 800m 930mv>"
            ]
        );
    }

    /// A profile that draws `<%hp>` from a capture that settles.
    fn settling_profile(draw: bool) -> Profile {
        let mut p = capture_profile(HP);
        p.set_prompt_config(vosh_prompt::PromptConfig {
            draw,
            capture: regex_capture(r"^<(?<hp>\d+)hp (?<mana>\d+)m> $", true),
            ..p.prompt.config().clone()
        });
        p
    }

    #[test]
    fn a_prompt_whole_before_its_line_end_keeps_the_line_end() {
        let mut wire = Wire::new(settling_profile(true));
        let out = wire.read(b"<1020hp 800m> \n\rA rat arrives.\n\r");
        assert_eq!(
            out.bytes,
            with(&[&wire.mark(1), b"<1020>\x1b[0m\r\n", b"A rat arrives.\r\n"])
        );
    }

    #[test]
    fn a_prompt_that_settles_draws_the_same_wherever_the_reads_split() {
        let draws = || settling_profile(true);
        let shows = || settling_profile(false);
        // No GA: the line end after a prompt ends its row, as it does
        // when the prompt settled at the end of an earlier read.
        let text = "You are hungry.\n\r<1020hp 800m> \n\rA rat arrives.\n\r<1000hp 800m> ";
        assert_eq!(
            same_at_every_split(&draws, text),
            ["You are hungry.", "<1020>", "A rat arrives.", "<1000>"]
        );
        assert_eq!(
            same_at_every_split(&shows, text),
            [
                "You are hungry.",
                "<1020hp 800m>",
                "A rat arrives.",
                "<1000hp 800m>"
            ]
        );
        // With a GA, drawn or shown as sent, the prompt keeps the cursor
        // after it, as it does when it settled before the GA came.
        let text = "<1020hp 800m> *\n\rA rat arrives.\n\r<1000hp 800m> *";
        assert_eq!(
            same_at_every_split(&draws, text),
            ["<1020>", "A rat arrives.", "<1000>"]
        );
        assert_eq!(
            same_at_every_split(&shows, text),
            ["<1020hp 800m>", "A rat arrives.", "<1000hp 800m>"]
        );
    }

    #[test]
    fn a_prompt_that_waits_for_its_line_end_draws_the_same_wherever_the_reads_split() {
        // The migrated capture on a prompt ending in %c, which the game
        // follows with a space and a GA.
        let text = "You are hungry.\n\r[1020/1020hp 800/800mn 930/930mv]\n\r *\n\rA rat arrives.\n\r[1000/1020hp 800/800mn 930/930mv]\n\r *";
        assert_eq!(
            same_at_every_split(&|| capture_profile(HP), text),
            ["You are hungry.", "<1020>", "", "A rat arrives.", "<1000>"]
        );
    }

    #[test]
    fn the_open_row_closes_on_a_send_and_on_other_output() {
        let mut wire = Wire::new(capture_profile(HP));
        let _ = wire.read(PROMPT_ROW);
        assert!(wire.p.prompt.stage.open_row().is_some());
        wire.send();
        assert_eq!(wire.p.prompt.stage.open_row(), None);

        let _ = wire.read(PROMPT_ROW);
        assert!(wire.p.prompt.stage.open_row().is_some());
        let _ = wire.read_with(b"", false, true);
        assert_eq!(
            wire.p.prompt.stage.open_row(),
            None,
            "output from elsewhere"
        );

        let _ = wire.read(PROMPT_ROW);
        let _ = wire.read(b"You flee!\n\r");
        assert_eq!(wire.p.prompt.stage.open_row(), None, "a line after it");
    }

    const PROMPT_ROW: &[u8] = b"[1020/1020hp 800/800mn 930/930mv]\n\r";

    #[test]
    fn a_capture_trigger_with_no_reader_hides_the_prompt_and_is_named_once() {
        // The trigger older builds wrote for `#prompt`.
        let mut p = Profile::default();
        p.triggers
            .set(vosh_automation::trigger::Trigger {
                name: "prompt-capture".into(),
                patterns: vec![vosh_automation::trigger::TriggerPattern {
                    pattern: CAPTURE.into(),
                    enabled: true,
                }],
                priority: 100,
                enabled: true,
                actions: vec![
                    vosh_automation::trigger::TriggerAction::Gag,
                    vosh_automation::trigger::TriggerAction::Script {
                        body: "mud.set_prompt_var(\"hp\", captures[2])".into(),
                    },
                ],
                preset: None,
                group: None,
                target: vosh_automation::trigger::TriggerTarget::Line,
            })
            .expect("the trigger compiles");
        p.set_prompt_config(vosh_prompt::PromptConfig::from_legacy(true, HP));
        let mut wire = Wire::new(p);
        let batch = wire.read_with(PROMPT_ROW, false, false);
        // The trigger hid the prompt, and nothing draws in its place.
        assert!(batch.out.is_empty());
        assert_eq!(batch.gag_without_reader, ["prompt-capture"]);
        let batch = wire.read_with(PROMPT_ROW, false, false);
        assert!(batch.gag_without_reader.is_empty(), "once a session");

        // With a capture in the profile, the capture reads the prompt
        // and the trigger never sees it.
        let config = vosh_prompt::PromptConfig {
            capture: regex_capture(CAPTURE, false),
            ..wire.p.prompt.config().clone()
        };
        wire.p.set_prompt_config(config);
        let batch = wire.read_with(PROMPT_ROW, false, false);
        assert_eq!(drawn_in(&batch.out.bytes).as_deref(), Some("<1020>\x1b[0m"));
        let leftover = &batch.gag_without_reader;
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn a_prompts_trigger_acts_on_the_recognized_prompt() {
        let mut p = capture_profile(HP);
        p.set_prompt_config(vosh_prompt::PromptConfig {
            draw: false,
            ..p.prompt.config().clone()
        });
        p.triggers
            .set(vosh_automation::trigger::Trigger {
                target: vosh_automation::trigger::TriggerTarget::Prompt,
                ..vosh_automation::trigger::Trigger::new(
                    "mark",
                    "hp",
                    vosh_automation::trigger::TriggerAction::Replace {
                        template: "HP".into(),
                    },
                )
            })
            .expect("the trigger compiles");
        let mut wire = Wire::new(p);
        let out = wire.read(PROMPT_ROW);
        assert_eq!(out.bytes, b"[1020/1020HP 800/800mn 930/930mv]\r\n");
    }

    #[test]
    fn line_triggers_that_matched_a_read_prompt_are_noted() {
        let mut p = capture_profile(HP);
        let highlight = |name: &str, pattern: &str, target| vosh_automation::trigger::Trigger {
            target,
            ..vosh_automation::trigger::Trigger::new(
                name,
                pattern,
                vosh_automation::trigger::TriggerAction::Gag,
            )
        };
        for trigger in [
            highlight(
                "hp-watch",
                r"\d+hp",
                vosh_automation::trigger::TriggerTarget::Line,
            ),
            highlight(
                "prompt-look",
                "hp",
                vosh_automation::trigger::TriggerTarget::Prompt,
            ),
            highlight(
                "hungry",
                "hungry",
                vosh_automation::trigger::TriggerTarget::Line,
            ),
        ] {
            p.triggers.set(trigger).expect("the trigger compiles");
        }
        let mut wire = Wire::new(p);
        let _ = wire.read(b"You are hungry.\n\r");
        assert_eq!(wire.p.prompt.stage.line_trigger_notice(), None);
        // The Line trigger does not hide the prompt, since it never sees
        // it, and it is named.
        let out = wire.read(PROMPT_ROW);
        assert_eq!(drawn_in(&out.bytes).as_deref(), Some("<1020>\x1b[0m"));
        assert_eq!(
            wire.p.prompt.stage.line_trigger_notice(),
            Some(vec!["hp-watch".to_string()])
        );
    }

    #[test]
    fn the_open_row_keeps_where_each_piece_of_the_design_landed() {
        let mut wire = Wire::new(capture_profile("<%hp/%{maxhp}> %mana"));
        let batch = wire.read_with(PROMPT_ROW, false, false);
        assert!(batch.prompt, "the read brought a prompt");
        let spans = |wire: &Wire| -> Vec<(usize, usize, usize)> {
            wire.p
                .prompt
                .stage
                .open_row()
                .map(|o| o.spans.iter().map(|s| (s.piece, s.col, s.width)).collect())
                .unwrap_or_default()
        };
        assert_eq!(spans(&wire), [(0, 0, 1), (1, 1, 9), (2, 10, 2), (3, 12, 3)]);
        // With the rows they sit in, which the webview wraps at its width.
        let plain = |wire: &Wire| wire.p.prompt.stage.open_row().map(|o| o.plain.clone());
        assert_eq!(plain(&wire).as_deref(), Some("<1020/1020> 800"));

        // Drawing off shows the game's own line, which has no pieces, and
        // drawing on again brings them back with the repaint.
        let now = tokio::time::Instant::now();
        let config = vosh_prompt::PromptConfig {
            draw: false,
            ..wire.p.prompt.config().clone()
        };
        wire.p.set_prompt_config(config);
        let _ = super::repaint_step(&mut wire.p, false, now);
        let leftover = &spans(&wire);
        assert!(leftover.is_empty(), "{leftover:?}");
        let config = vosh_prompt::PromptConfig {
            draw: true,
            template: "[%hp]".into(),
            ..wire.p.prompt.config().clone()
        };
        wire.p.set_prompt_config(config);
        let _ = super::repaint_step(&mut wire.p, false, now);
        assert_eq!(spans(&wire), [(0, 0, 1), (1, 1, 4), (2, 5, 1)]);
        assert_eq!(plain(&wire).as_deref(), Some("[1020]"));

        // Other output closes the row, and its pieces go with it. A line
        // that is no prompt leaves the flag down.
        let batch = wire.read_with(b"You are hungry.\n\r", false, false);
        assert_eq!(wire.p.prompt.stage.open_row(), None);
        assert!(!batch.prompt);
    }

    #[test]
    fn draw_off_repaints_the_open_row_as_the_game_sent_it() {
        let mut wire = Wire::new(capture_profile(HP));
        let _ = wire.read(PROMPT_ROW);
        let config = vosh_prompt::PromptConfig {
            draw: false,
            ..wire.p.prompt.config().clone()
        };
        wire.p.set_prompt_config(config);
        let now = tokio::time::Instant::now();
        let off = super::repaint_step(&mut wire.p, false, now);
        assert_eq!(
            off.replace,
            Some(vosh_prompt::stage::Replace {
                gen: wire.gen0 + 1,
                bytes: with(&[&wire.mark(2), PROMPT_ROW_SHOWN]),
                fresh: false,
                above: None,
                tail: Vec::new(),
            })
        );
        let leftover = &off.bytes;
        assert!(leftover.is_empty(), "{leftover:?}");

        // Drawing back on paints the design over the same row, and a new
        // design repaints it.
        let config = vosh_prompt::PromptConfig {
            draw: true,
            ..wire.p.prompt.config().clone()
        };
        wire.p.set_prompt_config(config);
        let on = super::repaint_step(&mut wire.p, false, now);
        assert_eq!(
            on.replace.map(|r| (r.gen, r.bytes)),
            Some((wire.gen0 + 2, with(&[&wire.mark(3), b"<1020>\x1b[0m"])))
        );
        assert!(super::repaint_step(&mut wire.p, false, now).is_empty());
        let config = vosh_prompt::PromptConfig {
            template: "[%hp]".into(),
            ..wire.p.prompt.config().clone()
        };
        wire.p.set_prompt_config(config);
        let new = super::repaint_step(&mut wire.p, false, now);
        assert_eq!(
            new.replace.map(|r| r.bytes),
            Some(with(&[&wire.mark(4), b"[1020]\x1b[0m"]))
        );

        // Nothing repaints once other output closed the row, after a
        // send, or after the webview wrote to the terminal itself.
        assert!(super::repaint_step(&mut wire.p, true, now).is_empty());
        let _ = wire.read(PROMPT_ROW);
        wire.send();
        assert!(super::repaint_step(&mut wire.p, false, now).is_empty());
        let _ = wire.read(PROMPT_ROW);
        assert!(wire.p.prompt.stage.open_row().is_some());
        wire.p.prompt.stage.close();
        assert!(super::repaint_step(&mut wire.p, false, now).is_empty());
    }

    #[test]
    fn only_a_new_width_closes_the_open_row() {
        let mut wire = Wire::new(capture_profile(HP));
        let mut negotiator = vosh_protocol::telnet::Negotiator::new();
        negotiator.set_window_size(94, 41);
        let _ = wire.read(PROMPT_ROW);
        // The webview sends the size the session already holds on every
        // connect. The row stays open, so turning drawing off repaints it.
        super::window_size_step(&mut wire.p, &mut negotiator, 94, 41, false);
        assert!(wire.p.prompt.stage.open_row().is_some());
        let config = vosh_prompt::PromptConfig {
            draw: false,
            ..wire.p.prompt.config().clone()
        };
        wire.p.set_prompt_config(config);
        let now = tokio::time::Instant::now();
        let off = super::repaint_step(&mut wire.p, false, now);
        assert_eq!(
            off.replace.map(|r| r.bytes),
            Some(with(&[&wire.mark(2), PROMPT_ROW_SHOWN]))
        );

        // A new height wraps nothing again, such as when your prompt
        // leaves the band for the text and the terminal grows, so the
        // row stays open and each change of drawing repaints it.
        super::window_size_step(&mut wire.p, &mut negotiator, 94, 43, false);
        assert_eq!(negotiator.window_size, (94, 43));
        assert!(wire.p.prompt.stage.open_row().is_some());
        for draw in [true, false] {
            let open = wire.p.prompt.stage.open_row().map(|r| r.gen);
            let config = vosh_prompt::PromptConfig {
                draw,
                ..wire.p.prompt.config().clone()
            };
            wire.p.set_prompt_config(config);
            let repaint = super::repaint_step(&mut wire.p, false, now);
            assert_eq!(repaint.replace.map(|r| r.gen), open, "draw {draw}");
        }

        // A new width wraps the row again, so it closes, and nothing
        // repaints.
        super::window_size_step(&mut wire.p, &mut negotiator, 80, 43, false);
        assert_eq!(negotiator.window_size, (80, 43));
        assert!(wire.p.prompt.stage.open_row().is_none());
        let config = vosh_prompt::PromptConfig {
            draw: true,
            ..wire.p.prompt.config().clone()
        };
        wire.p.set_prompt_config(config);
        assert!(super::repaint_step(&mut wire.p, false, now).is_empty());
    }

    /// The prompt row as the game sent it, with its line end.
    const PROMPT_ROW_SHOWN: &[u8] = b"[1020/1020hp 800/800mn 930/930mv]\r\n";

    #[test]
    fn the_ring_records_a_candidate_on_every_send_and_ga() {
        // Drawing on.
        let mut wire = Wire::new(capture_profile(HP));
        let _ = wire.read(PROMPT_ROW);
        wire.send();
        let _ = wire.read_ga(b"You say hi.\n\r[1000/1020hp 800/800mn 930/930mv]\n\r");
        let ring: Vec<(String, bool, bool, bool)> = wire
            .p
            .prompt
            .stage
            .ring()
            .map(|c| (c.plain.clone(), c.recognized, c.draw, c.capture))
            .collect();
        assert_eq!(
            ring,
            [
                (PROMPT_LINE.to_string(), true, true, true),
                (
                    "[1000/1020hp 800/800mn 930/930mv]".to_string(),
                    true,
                    true,
                    true
                ),
            ]
        );

        // Drawing off.
        let mut p = capture_profile(HP);
        p.set_prompt_config(vosh_prompt::PromptConfig {
            draw: false,
            ..p.prompt.config().clone()
        });
        let mut wire = Wire::new(p);
        let _ = wire.read(PROMPT_ROW);
        wire.send();
        let entry = wire.p.prompt.stage.ring().next().expect("an entry");
        assert!(entry.recognized && !entry.draw && entry.capture);

        // No capture: the prompt line, and a partial at a send.
        let mut wire = Wire::new(Profile::default());
        let _ = wire.read(b"You are hungry.\n\r<100hp> ");
        wire.send();
        let _ = wire.read_ga(b"<90hp> ");
        let _ = wire.read(PROMPT_ROW);
        wire.send();
        let ring: Vec<(String, bool, bool)> = wire
            .p
            .prompt
            .stage
            .ring()
            .map(|c| (c.plain.clone(), c.recognized, c.capture))
            .collect();
        assert_eq!(
            ring,
            [
                ("<100hp> ".to_string(), false, false),
                ("<90hp> ".to_string(), false, false),
                (PROMPT_LINE.to_string(), false, false),
            ]
        );
    }

    /// A profile that draws `template` over the capture on The Forsaken
    /// Lands, started the way the session starts it.
    fn forsaken_profile(template: &str) -> Profile {
        let mut p = capture_profile(template);
        super::start_prompt(
            &mut p,
            crate::profile_set::is_forsaken_lands("play.theforsakenlands.com"),
        );
        p
    }

    /// Hand a packet from fixtures/gmcp/aabahran to the session the way
    /// a socket read does.
    fn feed(p: &mut Profile, file: &str) {
        let path = format!(
            "{}/../fixtures/gmcp/aabahran/{file}",
            env!("CARGO_MANIFEST_DIR")
        );
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let msg = vosh_protocol::gmcp::parse(&bytes).unwrap_or_else(|e| panic!("{file}: {e}"));
        super::observe_prompt_gmcp(p, &msg);
    }

    fn feed_inline(p: &mut Profile, package: &str, data: serde_json::Value) {
        super::observe_prompt_gmcp(
            p,
            &vosh_protocol::gmcp::Message {
                package: package.into(),
                data,
            },
        );
    }

    /// The lamented tears cases each server build sends, shared with the
    /// store tests in the webview.
    fn lament_cases() -> Vec<serde_json::Value> {
        let text = include_str!("../../fixtures/gmcp/aabahran/lament.json");
        let doc: serde_json::Value = serde_json::from_str(text).expect("lament.json reads");
        doc["cases"].as_array().expect("a list of cases").clone()
    }

    #[test]
    fn the_three_lament_cases_draw_hidden_vitals_and_report_them() {
        for case in lament_cases() {
            let name = case["name"].as_str().unwrap_or_default();
            let mut p = forsaken_profile(TEMPLATE);
            for file in case["packets"].as_array().expect("packets") {
                feed(&mut p, file.as_str().expect("a file name"));
            }
            // The game prints zeros for every vital under the song.
            let drawn = draw_line(&mut p, "[0/0hp 0/0mn 0/0mv]").expect("the prompt draws");
            assert_eq!(plain(&drawn), "[?(?%)h ?(?%)m ?(?%)v] ", "{name}");
            // Each mark in bright black, then the look before it.
            assert!(
                drawn.contains("\x1b[3m\x1b[90m?\x1b[39m("),
                "{name}: {drawn:?}"
            );
            // The report the panes read, once.
            let hidden = p
                .prompt
                .vars
                .take_hidden_change()
                .expect("a change to report");
            assert_eq!(
                serde_json::to_value(hidden).expect("it serializes"),
                case["hidden"],
                "{name}"
            );
            assert!(p.prompt.vars.take_hidden_change().is_none(), "{name}");
            // The vitals store never reads a hidden value from the vars.
            let vars = p.prompt.vars.prompt_vars();
            for key in ["hp", "maxhp", "mana", "maxmana", "move", "maxmove"] {
                assert_eq!(vars.get(key).map(String::as_str), Some("?"), "{name} {key}");
            }
        }
    }

    #[test]
    fn a_prompt_draws_again_once_the_song_ends() {
        let cases = lament_cases();
        let older = &cases[2];
        let mut p = forsaken_profile(TEMPLATE);
        for file in older["packets"].as_array().expect("packets") {
            feed(&mut p, file.as_str().expect("a file name"));
        }
        assert!(p.prompt.vars.take_hidden_change().is_some());
        // The song ends. Char.Affects comes at once, the rest at the next
        // prompt.
        feed(&mut p, "char-affects.gmcp");
        feed(&mut p, "char-vitals.gmcp");
        feed(&mut p, "group-info-own-row.gmcp");
        let drawn = draw_line(&mut p, "[850/900hp 760/820mn 250/250mv]").expect("the prompt draws");
        assert_eq!(plain(&drawn), "[850(94%)h 760(93%)m 250(100%)v] ");
        let hidden = p
            .prompt
            .vars
            .take_hidden_change()
            .expect("a change to report");
        assert_eq!(
            serde_json::to_value(hidden).expect("it serializes"),
            serde_json::json!({"vitals":false,"tank":false,"opponent":false,"affects":false,"group":false})
        );
    }

    #[test]
    fn a_reconnect_in_the_song_hides_the_vitals_from_the_prompt_alone() {
        // The older build after a link dead reconnect sends no
        // Char.Affects until the next tick, and Char.Vitals carries the
        // true values. Only the prompt the capture reads shows the song.
        let mut p = forsaken_profile(TEMPLATE);
        feed(&mut p, "char-vitals.gmcp");
        let drawn = draw_line(&mut p, "[0/0hp 0/0mn 0/0mv]").expect("the prompt draws");
        assert_eq!(plain(&drawn), "[?(?%)h ?(?%)m ?(?%)v] ");
        let hidden = p
            .prompt
            .vars
            .take_hidden_change()
            .expect("a change to report");
        assert_eq!(
            serde_json::to_value(hidden).expect("it serializes"),
            serde_json::json!({"vitals":true,"tank":false,"opponent":false,"affects":false,"group":false})
        );
        let vars = p.prompt.vars.prompt_vars();
        for key in ["hp", "maxhp", "mana", "maxmana", "move", "maxmove"] {
            assert_eq!(vars.get(key).map(String::as_str), Some("?"), "{key}");
        }
    }

    #[test]
    fn other_hosts_hide_nothing() {
        let mut p = capture_profile("%hp/%maxhp %opponent %{opponent_hp:pct}");
        super::start_prompt(&mut p, crate::profile_set::is_forsaken_lands("127.0.0.1"));
        for file in [
            "char-affects-lament.gmcp",
            "char-vitals.gmcp",
            "char-combat-lament-older.gmcp",
        ] {
            feed(&mut p, file);
        }
        assert_eq!(
            plain(&draw_line(&mut p, PROMPT_LINE).expect("the prompt draws")),
            "1020/1020 a Blackwatch guard 41"
        );
        assert!(p.prompt.vars.take_hidden_change().is_none());
    }

    /// The pieces the phase 1 gate draws from GMCP, with a separator
    /// between groups.
    const GATE: &str = "%gold %opponent|%{moon1:game} %{moon3:word}|%pos %lang %weather %{temp:unit} %region|%tank %{tank_hp:game}|%exits";

    /// The packets the new build sends at login and in a fight, plus
    /// Char.Worth and World.Moons.
    fn new_build_fight(p: &mut Profile) {
        for file in [
            "char-prompt.gmcp",
            "char-vitals.gmcp",
            "char-combat-tank.gmcp",
            "char-state.gmcp",
            "room-weather.gmcp",
            "room-info.gmcp",
        ] {
            feed(p, file);
        }
        feed_inline(
            p,
            "Char.Worth",
            serde_json::json!({"gold":1250,"bank":5000,"exp":125_000,"tnl":1250,"trains":3,"practices":12,"cps":40,"rps":7,"cabal":"none"}),
        );
        feed_inline(
            p,
            "World.Moons",
            serde_json::json!({"moons":[
                {"name":"Lysenties","active":true,"phase":4,"phase_name":"full and whole"},
                {"name":"Nercuros","active":false,"phase":2,"phase_name":"half-lit and growing"},
                {"name":"Dyphrities","active":true,"phase":7,"phase_name":"a thin crescent, fading"}
            ],"eclipse":false,"triad":false,"near_alignment":true}),
        );
    }

    #[test]
    fn the_session_draws_the_gate_pieces_from_the_new_build_packets() {
        let mut p = forsaken_profile(GATE);
        new_build_fight(&mut p);
        assert_eq!(
            plain(&draw_line(&mut p, PROMPT_LINE).expect("the prompt draws")),
            "1250 a Blackwatch guard|FUL waning crescent|sit common rainy 60°F Coastal North|Tester [===|===|===|=--]|S"
        );
        // Nothing is hidden, so nothing is reported.
        assert!(p.prompt.vars.take_hidden_change().is_none());
    }

    #[test]
    fn exits_draw_from_room_info_only_on_the_new_build() {
        let mut p = forsaken_profile("[%exits]");
        feed(&mut p, "char-vitals.gmcp");
        feed(&mut p, "room-info.gmcp");
        // No Char.Prompt this session, so Room.Info feeds no exits.
        assert_eq!(
            plain(&draw_line(&mut p, PROMPT_LINE).expect("it draws")),
            "[]"
        );
        feed(&mut p, "char-prompt.gmcp");
        assert_eq!(
            plain(&draw_line(&mut p, PROMPT_LINE).expect("it draws")),
            "[S]"
        );
    }

    #[test]
    fn vosh_supplies_the_tick_target_tracked_affects_and_profile() {
        let mut p = forsaken_profile("%tick|%{tick:unit}|%target|%{missing:names}|%profile");
        let now = tokio::time::Instant::now();
        p.tick.enable(now);
        p.target.name = Some("guard".into());
        p.display_name = Some("Default".into());
        p.ui.tracked_affects = vec![crate::profile_config::TrackedAffect {
            name: "sanctuary".into(),
            label: None,
        }];
        let supplied = super::prompt_supplies(&p, now);
        let interval = i64::try_from(p.tick.config.interval.as_secs()).expect("seconds");
        assert_eq!(
            supplied.tick,
            Some(vosh_prompt::values::Tick {
                remaining: interval,
                interval: Some(interval),
                since: Some(0),
            })
        );
        feed(&mut p, "char-affects.gmcp");
        let drawn = plain(&draw_line(&mut p, PROMPT_LINE).expect("it draws"));
        let parts: Vec<&str> = drawn.split('|').collect();
        assert!(
            parts[0]
                .parse::<i64>()
                .is_ok_and(|s| s > 0 && s <= interval),
            "{drawn}"
        );
        assert!(parts[1].ends_with('s'), "{drawn}");
        assert_eq!(&parts[2..], ["guard", "sanctuary", "Default"]);
    }

    #[test]
    fn a_new_connection_starts_the_prompt_over() {
        let mut p = forsaken_profile(GATE);
        new_build_fight(&mut p);
        assert!(p.prompt.vars.new_build());
        let _ = draw_line(&mut p, PROMPT_LINE);
        assert!(!p.prompt.vars.prompt_vars().is_empty());

        super::end_prompt(&mut p);
        assert!(!p.prompt.vars.new_build());
        assert!(p.prompt.vars.prompt_vars().is_empty());
        assert!(p.prompt.vars.gmcp().get("Char.Worth").is_none());
        // The hidden state that ended with the connection is never
        // reported, since the stores clear on the disconnect.
        assert!(p.prompt.vars.take_hidden_change().is_none());
        // The profile's [prompt] table outlives the connection.
        assert!(p.prompt.config().draw);
        assert_eq!(p.prompt.config().template, GATE);

        super::start_prompt(&mut p, false);
        assert!(!p.prompt.forsaken());
        assert_eq!(p.prompt.config().template, GATE);
    }

    /// James's PROMPT as the game stores it, and the tank block it prints
    /// while someone in the group tanks.
    const CODES: &str = vosh_prompt::testkit::mud::PROMPT;
    const TANK_LINE: &str = "Tester: [===|===|---|---]";
    const FIGHT_LINE: &str = "[159/1020hp 310/800mn 489/930mv]";

    /// A profile that reads Aabahran's codes `prompt` and draws
    /// `template` in its place.
    fn codes_profile(prompt: &str, template: &str) -> Profile {
        let mut p = Profile::default();
        p.set_prompt_config(vosh_prompt::PromptConfig {
            draw: true,
            template: template.to_string(),
            capture: vosh_prompt::CaptureConfig::Aabahran(vosh_prompt::config::AabahranCapture {
                prompt: prompt.to_string(),
                ..vosh_prompt::config::AabahranCapture::default()
            }),
            ..vosh_prompt::PromptConfig::default()
        });
        p
    }

    #[test]
    fn the_codes_read_and_draw_a_one_line_prompt() {
        let mut wire = Wire::new(codes_profile(CODES, HP));
        let out = wire.read(b"You are hungry.\n\r[1020/1020hp 800/800mn 930/930mv]\n\r");
        assert_eq!(
            out.bytes,
            with(&[b"You are hungry.\r\n", &wire.mark(1), b"<1020>\x1b[0m"])
        );
        assert!(wire.p.prompt.stage.open_row().is_some());
    }

    #[test]
    fn a_tank_line_shows_as_sent_when_the_design_reads_nothing_on_it() {
        let mut wire = Wire::new(codes_profile(CODES, HP));
        let out = wire.read(format!("{TANK_LINE}\n\r{FIGHT_LINE}\n\r").as_bytes());
        assert_eq!(
            out.bytes,
            with(&[
                TANK_LINE.as_bytes(),
                b"\r\n",
                &wire.mark(1),
                b"<159>\x1b[0m"
            ])
        );
        // The capture read the whole block.
        let vars = wire.p.prompt.vars.prompt_vars();
        assert_eq!(vars.get("tank").map(String::as_str), Some("Tester"));
        assert_eq!(vars.get("fight").map(String::as_str), Some("1"));
        // Drawing off brings back only the line the design replaced, so
        // the tank line never shows twice.
        let block = wire.p.prompt.stage.last_raw().expect("the block").clone();
        assert_eq!(block.replaced, [1]);
        assert_eq!(block.shown(), format!("{FIGHT_LINE}\r\n").into_bytes());
    }

    #[test]
    fn a_design_that_reads_the_tank_takes_over_the_whole_block() {
        for template in ["%tank <%hp>", "%{tank_hp:game} <%hp>", "%{raw}"] {
            let mut wire = Wire::new(codes_profile(CODES, template));
            let out = wire.read(format!("{TANK_LINE}\n\r{FIGHT_LINE}\n\r").as_bytes());
            let text = String::from_utf8_lossy(&out.bytes).into_owned();
            assert!(
                text.starts_with(&String::from_utf8_lossy(&wire.mark(1)).into_owned()),
                "{template}: {text:?}"
            );
            let block = wire.p.prompt.stage.last_raw().expect("the block");
            assert_eq!(block.replaced, [0, 1], "{template}");
        }
    }

    #[test]
    fn a_tank_block_draws_the_same_wherever_the_reads_split() {
        // The game follows a prompt ending in %c with a space and a GA,
        // and starts the next output with a line end.
        let text =
            format!("You flee.\n\r{TANK_LINE}\n\r{FIGHT_LINE}\n\r *\n\rThe guard arrives.\n\r");
        let screen = same_at_every_split(&|| codes_profile(CODES, HP), &text);
        assert_eq!(
            screen,
            ["You flee.", TANK_LINE, "<159>", "", "The guard arrives."]
        );
        let screen = same_at_every_split(&|| codes_profile(CODES, "%tank <%hp>"), &text);
        assert_eq!(
            screen,
            ["You flee.", "Tester <159>", "", "The guard arrives."]
        );
    }

    #[test]
    fn a_held_tank_line_paints_at_the_end_of_a_read_and_the_prompt_replaces_it() {
        let mut wire = Wire::new(codes_profile(CODES, HP));
        let out = wire.read(format!("{TANK_LINE}\n\r").as_bytes());
        assert_eq!(
            out.bytes,
            with(&[&wire.mark(1), TANK_LINE.as_bytes(), b"\r\n"])
        );
        let out = wire.read(format!("{FIGHT_LINE}\n\r").as_bytes());
        let leftover = &out.bytes;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(
            out.replace,
            Some(vosh_prompt::stage::Replace {
                gen: wire.gen0 + 1,
                bytes: with(&[
                    TANK_LINE.as_bytes(),
                    b"\r\n",
                    &wire.mark(2),
                    b"<159>\x1b[0m"
                ]),
                fresh: true,
                above: None,
                tail: Vec::new(),
            })
        );
        // A send forgets a painted tank line, which stays as it shows,
        // and the next prompt reads on its own.
        let _ = wire.read(format!("{TANK_LINE}\n\r").as_bytes());
        wire.send();
        let out = wire.read(format!("{FIGHT_LINE}\n\r").as_bytes());
        assert_eq!(out.replace, None);
        assert_eq!(out.bytes, with(&[&wire.mark(4), b"<159>\x1b[0m"]));
    }

    #[test]
    fn a_held_line_your_send_lets_go_runs_the_line_pass_and_is_logged() {
        let mut p = codes_profile(CODES, HP);
        p.triggers
            .set(vosh_automation::trigger::Trigger::new(
                "answer",
                "^Bob says: ",
                vosh_automation::trigger::TriggerAction::Send {
                    template: "nod".into(),
                },
            ))
            .unwrap();
        let mut wire = Wire::new(p);
        // A line that can start a tank block ends the read, so the stage
        // holds it and paints it.
        let out = wire.read(b"You flee.\n\rBob says: \n\r");
        assert_eq!(
            out.bytes,
            with(&[b"You flee.\r\n", &wire.mark(1), b"Bob says: \r\n"])
        );
        assert!(wire.p.prompt.stage.holds());
        // You send before the next read. The line stays as it shows and
        // runs the Line pass, so its trigger answers and it is logged and
        // kept for scrollback.
        let mut batch = super::ReadBatch::new(super::output_count());
        let steps = super::let_go_held(
            &mut wire.p,
            &mut batch,
            tokio::time::Instant::now(),
            Some(3),
        );
        assert!(!wire.p.prompt.stage.holds());
        assert!(batch.out.is_empty(), "it stays as it shows");
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].result.sends, ["nod"]);
        assert_eq!(steps[0].scrollback, bytes_of(&["Bob says: "]));
        let logged: Vec<&str> = batch.log.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(logged, ["Bob says: "]);
        // The next read is read on its own.
        wire.send();
        let out = wire.read(b"The Bank of Aabahran\n\r");
        assert_eq!(out.replace, None);
        assert_eq!(out.bytes, b"The Bank of Aabahran\r\n");
    }

    #[test]
    fn a_held_line_a_script_hides_still_shows_and_its_echo_follows() {
        let mut p = codes_profile(CODES, HP);
        p.triggers
            .set(vosh_automation::trigger::Trigger {
                name: "swap".into(),
                patterns: vec![vosh_automation::trigger::TriggerPattern {
                    pattern: "^Bob says: ".into(),
                    enabled: true,
                }],
                priority: 0,
                enabled: true,
                actions: vec![
                    vosh_automation::trigger::TriggerAction::Gag,
                    vosh_automation::trigger::TriggerAction::Script {
                        body: "mud.echo('Bob speaks.')".into(),
                    },
                ],
                preset: None,
                group: None,
                target: vosh_automation::trigger::TriggerTarget::Line,
            })
            .unwrap();
        let mut wire = Wire::new(p);
        let _ = wire.read(b"Bob says: \n\r");
        let mut batch = super::ReadBatch::new(super::output_count());
        let steps = super::let_go_held(
            &mut wire.p,
            &mut batch,
            tokio::time::Instant::now(),
            Some(3),
        );
        // The end of its read painted it, so it is logged and kept as it
        // shows, and the echo lands after it.
        assert_eq!(batch.out.bytes, b"Bob speaks.\r\n");
        assert_eq!(steps[0].scrollback, bytes_of(&["Bob says: "]));
        assert_eq!(batch.log.len(), 1);
    }

    #[test]
    fn a_held_line_at_the_end_of_the_session_is_logged_and_kept() {
        let mut wire = Wire::new(codes_profile(CODES, HP));
        let _ = wire.read(format!("{TANK_LINE}\n\r").as_bytes());
        let (log, kept) = super::end_held(&mut wire.p, Some(3));
        let logged: Vec<&str> = log.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(logged, [TANK_LINE]);
        assert_eq!(kept, bytes_of(&[TANK_LINE]));
        assert!(!wire.p.prompt.stage.holds());
        // Without a log session it is still kept.
        let _ = wire.read(format!("{TANK_LINE}\n\r").as_bytes());
        let (log, kept) = super::end_held(&mut wire.p, None);
        assert!(log.is_empty());
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn drawing_off_on_a_tank_block_brings_back_only_the_line_it_replaced() {
        let mut wire = Wire::new(codes_profile(CODES, HP));
        let _ = wire.read(format!("{TANK_LINE}\n\r{FIGHT_LINE}\n\r").as_bytes());
        wire.p.set_prompt_config(vosh_prompt::PromptConfig {
            draw: false,
            ..wire.p.prompt.config().clone()
        });
        let out = super::repaint_step(&mut wire.p, false, tokio::time::Instant::now());
        assert_eq!(
            out.replace,
            Some(vosh_prompt::stage::Replace {
                gen: wire.gen0 + 1,
                bytes: with(&[&wire.mark(2), FIGHT_LINE.as_bytes(), b"\r\n"]),
                fresh: false,
                above: None,
                tail: Vec::new(),
            })
        );
    }

    #[test]
    fn a_ga_after_prompt_all_draws_in_the_same_and_the_next_read() {
        let profile = || codes_profile("%n%P%C<%hhp %mm %vmv> ", HP);
        let mut wire = Wire::new(profile());
        let out = wire.read_ga(b"<159hp 310m 489mv> ");
        assert_eq!(out.bytes, with(&[&wire.mark(1), b"<159>\x1b[0m"]));
        let screen = same_at_every_split(
            &profile,
            "You flee.\n\rTester: [===|===|===|---]\n\r<159hp 310m 489mv> *\n\rThe guard arrives.\n\r<159hp 310m 489mv> *",
        );
        assert_eq!(
            screen,
            [
                "You flee.",
                "Tester: [===|===|===|---]",
                "<159>",
                "The guard arrives.",
                "<159>"
            ]
        );
    }

    #[test]
    fn a_held_line_the_rest_never_follows_shows_as_any_line() {
        let text = format!("{TANK_LINE}\n\rYou are hungry.\n\r{FIGHT_LINE}\n\r");
        let screen = same_at_every_split(&|| codes_profile(CODES, HP), &text);
        assert_eq!(screen, [TANK_LINE, "You are hungry.", "<159>"]);
        // A GA ends a held line, since the prompt it started never came.
        let screen = same_at_every_split(
            &|| codes_profile(CODES, HP),
            &format!("{TANK_LINE}\n\r*You flee.\n\r"),
        );
        assert_eq!(screen, [TANK_LINE, "You flee."]);
    }

    #[test]
    fn a_released_line_runs_the_line_pass_and_is_logged() {
        let mut p = codes_profile(CODES, HP);
        p.triggers
            .set(vosh_automation::trigger::Trigger::new(
                "hush",
                "^Tester: ",
                vosh_automation::trigger::TriggerAction::Gag,
            ))
            .unwrap();
        let mut batch = super::ReadBatch::new(super::output_count());
        let now = tokio::time::Instant::now();
        let mut acc = super::LineAccumulator::new();
        let mut steps = Vec::new();
        for line in acc.feed(format!("{TANK_LINE}\n\rYou are hungry.\n\r").as_bytes()) {
            let plain = vosh_protocol::ansi::plain_text(&line.bytes);
            steps.extend(super::line_step(
                &mut p,
                &mut batch,
                line,
                plain,
                now,
                Some(3),
            ));
        }
        // The tank line ran the Line pass once it was let go, and its
        // trigger hid it.
        assert_eq!(batch.out.bytes, b"You are hungry.\r\n");
        assert_eq!(steps.len(), 2);
        assert_eq!(batch.log.len(), 1, "only the line that shows is logged");
    }

    #[test]
    fn drawing_off_shows_the_whole_block_as_sent_and_logs_it() {
        let mut p = codes_profile(CODES, HP);
        p.set_prompt_config(vosh_prompt::PromptConfig {
            draw: false,
            ..p.prompt.config().clone()
        });
        let mut wire = Wire::new(p);
        let out = wire.read(format!("{TANK_LINE}\n\r{FIGHT_LINE}\n\r").as_bytes());
        assert_eq!(
            out.bytes,
            format!("{TANK_LINE}\r\n{FIGHT_LINE}\r\n").into_bytes()
        );
        let mut batch = super::ReadBatch::new(super::output_count());
        let now = tokio::time::Instant::now();
        let mut acc = super::LineAccumulator::new();
        for line in acc.feed(format!("{TANK_LINE}\n\r{FIGHT_LINE}\n\r").as_bytes()) {
            let plain = vosh_protocol::ansi::plain_text(&line.bytes);
            let _ = super::line_step(&mut wire.p, &mut batch, line, plain, now, Some(3));
        }
        let logged: Vec<&str> = batch.log.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(logged, [TANK_LINE, FIGHT_LINE]);
    }

    /// Run `text` through the Line pass as one read. Returns what it
    /// logged, what it kept for scrollback, and what it wrote.
    fn logged_and_kept(p: &mut Profile, text: &str) -> (Vec<String>, Vec<Vec<u8>>, Vec<u8>) {
        let mut batch = super::ReadBatch::new(super::output_count());
        let now = tokio::time::Instant::now();
        let mut acc = super::LineAccumulator::new();
        let mut kept = Vec::new();
        for line in acc.feed(text.as_bytes()) {
            let plain = vosh_protocol::ansi::plain_text(&line.bytes);
            for step in super::line_step(p, &mut batch, line, plain, now, Some(3)) {
                kept.extend(step.scrollback);
            }
        }
        let logged = batch.log.iter().map(|e| e.text.clone()).collect();
        (logged, kept, batch.out.bytes)
    }

    fn bytes_of(lines: &[&str]) -> Vec<Vec<u8>> {
        lines.iter().map(|l| l.as_bytes().to_vec()).collect()
    }

    #[test]
    fn a_tank_line_that_shows_while_vosh_draws_is_logged_and_kept() {
        let fight = format!("You flee.\n\r{TANK_LINE}\n\r{FIGHT_LINE}\n\r");
        let mut p = codes_profile(CODES, HP);
        let (logged, kept, _) = logged_and_kept(&mut p, &fight);
        assert_eq!(logged, ["You flee.", TANK_LINE]);
        assert_eq!(kept, bytes_of(&["You flee.", TANK_LINE]));
        // A design that reads the tank draws in place of the line, which
        // then is neither logged nor kept.
        let mut p = codes_profile(CODES, "%tank <%hp>");
        let (logged, kept, _) = logged_and_kept(&mut p, &fight);
        assert_eq!(logged, ["You flee."]);
        assert_eq!(kept, bytes_of(&["You flee."]));
    }

    #[test]
    fn with_drawing_off_every_line_that_shows_is_logged_and_kept() {
        let block = format!("{TANK_LINE}\n\r{FIGHT_LINE}\n\r");
        let mut p = codes_profile(CODES, HP);
        p.set_prompt_config(vosh_prompt::PromptConfig {
            draw: false,
            ..p.prompt.config().clone()
        });
        let (logged, kept, _) = logged_and_kept(&mut p, &block);
        assert_eq!(logged, [TANK_LINE, FIGHT_LINE]);
        assert_eq!(kept, bytes_of(&[TANK_LINE, FIGHT_LINE]));
        // A Prompts trigger hides the final line. The tank line still
        // shows, so it is still logged and kept.
        p.triggers
            .set(vosh_automation::trigger::Trigger {
                target: vosh_automation::trigger::TriggerTarget::Prompt,
                ..vosh_automation::trigger::Trigger::new(
                    "hide-prompt",
                    "hp ",
                    vosh_automation::trigger::TriggerAction::Gag,
                )
            })
            .unwrap();
        let (logged, kept, shown) = logged_and_kept(&mut p, &block);
        assert_eq!(shown, format!("{TANK_LINE}\r\n").into_bytes());
        assert_eq!(logged, [TANK_LINE]);
        assert_eq!(kept, bytes_of(&[TANK_LINE]));
    }

    #[test]
    fn prompt_all_settles_with_its_tank_line_in_one_read() {
        let mut wire = Wire::new(codes_profile("%n%P%C<%hhp %mm %vmv> ", HP));
        let out = wire.read(b"Tester: [===|===|===|---]\n\r<159hp 310m 489mv> ");
        assert_eq!(
            out.bytes,
            with(&[
                b"Tester: [===|===|===|---]\r\n",
                &wire.mark(1),
                b"<159>\x1b[0m"
            ])
        );
        let screen = same_at_every_split(
            &|| codes_profile("%n%P%C<%hhp %mm %vmv> ", HP),
            "You flee.\n\rTester: [===|===|===|---]\n\r<159hp 310m 489mv> ",
        );
        assert_eq!(screen, ["You flee.", "Tester: [===|===|===|---]", "<159>"]);
    }

    #[test]
    fn the_away_prompt_shows_as_sent_and_notes_you_are_away() {
        let mut wire = Wire::new(codes_profile("%n%P%C<%hhp %mm %vmv> ", HP));
        let out = wire.read(b"<AFK> ");
        assert_eq!(out.bytes, b"<AFK> ");
        assert_eq!(wire.p.prompt.stage.open_row(), None);
        let vars = wire.p.prompt.vars.prompt_vars();
        assert_eq!(vars.get("afk").map(String::as_str), Some("1"));
    }

    #[test]
    fn a_char_prompt_before_its_text_reads_the_new_codes_at_once() {
        let mut p = codes_profile("<%hhp> ", HP);
        super::start_prompt(&mut p, true);
        let mut wire = Wire::new(p);
        feed_inline(
            &mut wire.p,
            "Char.Prompt",
            serde_json::json!({"enabled": true, "prompt": "%n%P%C<%hhp %mm %vmv> ", "fprompt": ""}),
        );
        let seen = wire.p.prompt.take_seen();
        assert!(seen[0].applied);
        let out = wire.read(b"Prompt set to %n%P%C<%hhp %mm %vmv> \n\r<159hp 310m 489mv> ");
        assert_eq!(
            out.bytes,
            with(&[
                b"Prompt set to %n%P%C<%hhp %mm %vmv> \r\n",
                &wire.mark(1),
                b"<159>\x1b[0m"
            ])
        );
    }

    #[test]
    fn the_reply_to_your_prompt_updates_the_capture_before_the_next_prompt() {
        let mut p = codes_profile("<%hhp> ", HP);
        super::start_prompt(&mut p, true);
        let mut wire = Wire::new(p);
        wire.send_line("prom %n%P%C<%hhp %mm %vmv>");
        let out = wire.read(b"Prompt set to %n%P%C<%hhp %mm %vmv> \n\r<159hp 310m 489mv> ");
        assert_eq!(
            out.bytes,
            with(&[
                b"Prompt set to %n%P%C<%hhp %mm %vmv> \r\n",
                &wire.mark(1),
                b"<159>\x1b[0m"
            ])
        );
        let seen = wire.p.prompt.take_seen();
        assert!(seen[0].applied);

        // prompt off sets nothing, whatever the reply says.
        wire.send_line("prompt off");
        let _ = wire.read(b"You will no longer see prompts.\n\rPrompt set to \x01\x02\n\r");
        assert!(wire.p.prompt.prompts_off());
        let vosh_prompt::CaptureConfig::Aabahran(codes) = &wire.p.prompt.config().capture else {
            panic!("an aabahran capture");
        };
        assert_eq!(codes.prompt, "%n%P%C<%hhp %mm %vmv> ");
    }

    #[test]
    fn the_new_build_with_prompts_off_raises_no_not_matching() {
        let mut p = codes_profile("%n%P%C<%hhp %mm %vmv> ", HP);
        super::start_prompt(&mut p, true);
        let mut wire = Wire::new(p);
        feed(&mut wire.p, "char-prompt-off.gmcp");
        // Each pulse brings the prompt time packages and no prompt text.
        for _ in 0..5 {
            feed(&mut wire.p, "char-vitals.gmcp");
            feed(&mut wire.p, "char-state.gmcp");
            let _ = wire.read(b"");
        }
        let report = wire.p.prompt.take_status_change().expect("a report");
        assert_eq!(report.status, vosh_prompt::Status::PromptsOff);
        // Prompts on again, and the prompt reads.
        feed(&mut wire.p, "char-prompt.gmcp");
        feed(&mut wire.p, "char-vitals.gmcp");
        let _ = wire.read(b"<159hp 310m 489mv> ");
        assert_eq!(wire.p.prompt.status(), vosh_prompt::Status::Matching);
    }

    #[test]
    fn a_live_partial_waits_for_the_next_read_and_never_flashes() {
        let mut wire = Wire::new(codes_profile(CODES, HP));
        let batch = wire.read_holding(b"You flee.\n\r[1020/1020hp 80");
        assert!(batch.hold);
        assert_eq!(batch.out.bytes, b"You flee.\r\n", "nothing raw yet");
        let out = wire.read(b"0/800mn 930/930mv]\n\r");
        assert_eq!(out.replace, None);
        assert_eq!(out.bytes, with(&[&wire.mark(1), b"<1020>\x1b[0m"]));

        // Held tank lines wait with the partial after them.
        let mut wire = Wire::new(codes_profile(CODES, HP));
        let batch = wire.read_holding(format!("{TANK_LINE}\n\r[159/10").as_bytes());
        assert!(batch.hold);
        let leftover = &batch.out.bytes;
        assert!(leftover.is_empty(), "{leftover:?}");
        let out = wire.read(b"20hp 310/800mn 489/930mv]\n\r");
        assert_eq!(out.replace, None);
        assert_eq!(
            out.bytes,
            with(&[
                TANK_LINE.as_bytes(),
                b"\r\n",
                &wire.mark(1),
                b"<159>\x1b[0m"
            ])
        );
    }

    #[test]
    fn a_partial_no_shape_can_become_paints_at_once() {
        let mut wire = Wire::new(codes_profile("<%hhp %mm %vmv> ", HP));
        let batch = wire.read_holding(b"By what name do you wish to be known? ");
        assert!(!batch.hold);
        assert_eq!(
            batch.out.bytes,
            with(&[&wire.mark(1), b"By what name do you wish to be known? "])
        );
        // Nothing reads a prompt in a profile without a capture.
        let mut wire = Wire::new(Profile::default());
        assert!(!wire.read_holding(b"<10hp 2").hold);
    }

    #[test]
    fn a_partial_that_waited_paints_at_the_deadline() {
        let mut wire = Wire::new(codes_profile("<%hhp %mm %vmv> ", HP));
        let batch = wire.read_holding(b"<10hp 2");
        assert!(batch.hold);
        let mut out = vosh_prompt::stage::Output::new(false);
        super::hold_step(&mut wire.p, &mut wire.acc, &mut out);
        assert_eq!(out.bytes, with(&[&wire.mark(1), b"<10hp 2"]));
        // The rest of it replaces what painted.
        let out = wire.read(b"0m 30mv> ");
        assert_eq!(
            out.replace,
            Some(vosh_prompt::stage::Replace {
                gen: wire.gen0 + 1,
                bytes: with(&[&wire.mark(2), b"<10>\x1b[0m"]),
                fresh: true,
                above: None,
                tail: Vec::new(),
            })
        );
    }

    #[test]
    fn an_empty_setting_draws_over_the_fallback() {
        let mut wire = Wire::new(codes_profile("", HP));
        let out = wire.read(b"<20hp 100m 110mv> ");
        assert_eq!(out.bytes, with(&[&wire.mark(1), b"<20>\x1b[0m"]));
    }

    #[tokio::test]
    async fn the_log_row_takes_the_first_character_char_status_names() {
        let mut store = vosh_log::LogStore::in_memory().unwrap();
        let id = store.start_session("h", 1, 0).unwrap();
        let logs: crate::logs::SharedLogStore =
            std::sync::Arc::new(tokio::sync::Mutex::new(Some(store)));
        let mut session = super::LogSession::new(Some(id));
        session.name(&logs, "Tester").await;
        session.name(&logs, "Other").await;
        let named = logs
            .lock()
            .await
            .as_ref()
            .unwrap()
            .session_character(id)
            .unwrap();
        assert_eq!(named.as_deref(), Some("Tester"));
        // A connection with logging off names nothing.
        super::LogSession::new(None).name(&logs, "Tester").await;
    }

    /// A synthetic socket read from fixtures/prompt/aabahran/wire.
    fn wire_fixture(name: &str) -> Vec<u8> {
        let path = format!(
            "{}/../fixtures/prompt/aabahran/wire/{name}.bin",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    /// A profile that reads Aabahran's codes `prompt` on a connection to
    /// the fake game on a local port, and draws `<%hp>` in its place.
    fn fake_profile(prompt: &str) -> Profile {
        let mut p = codes_profile(prompt, HP);
        super::start_prompt(&mut p, false);
        p
    }

    /// The screen a native grid `columns` wide shows after `reads` of raw
    /// wire bytes, rows trimmed, up to the last row that shows anything.
    fn wire_screen(wire: &mut Wire, columns: usize, reads: &[&[u8]]) -> Vec<String> {
        let mut grid = crate::term_grid::TermGrid::new(columns, 60);
        for read in reads {
            grid.session_output(&wire.read_wire(read));
        }
        let mut rows: Vec<String> = (0..grid.screen_lines())
            .map(|line| grid.row_string(line).trim_end().to_string())
            .collect();
        while rows.last().is_some_and(String::is_empty) {
            rows.pop();
        }
        rows
    }

    /// Where to cut `bytes` in two: after every byte of text, and around
    /// and inside each GMCP packet (in its IAC SB GMCP head, halfway
    /// through its body, and between its IAC and SE). A cut anywhere else
    /// in a packet's body reads the same as the one halfway through it.
    fn cuts(bytes: &[u8]) -> Vec<usize> {
        let mut cuts = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == 255 && bytes.get(i + 1) == Some(&250) {
                let end = bytes[i..]
                    .windows(2)
                    .position(|w| w == [255, 240])
                    .map_or(bytes.len(), |p| i + p + 2);
                cuts.extend([i, i + 1, i + 2, i + 3, (i + end) / 2, end - 1]);
                i = end;
                continue;
            }
            cuts.push(i);
            i += 1;
        }
        cuts.retain(|&c| c > 0 && c < bytes.len());
        cuts.sort_unstable();
        cuts.dedup();
        cuts
    }

    /// Read `bytes` as one read and as two cut at every place [`cuts`]
    /// names, at 40 and 12 wide, and check each screen is the one a single
    /// read gives. Returns the 80 wide screen of one read and the wire
    /// that read it.
    fn wire_same_at_every_split(
        profile: &dyn Fn() -> Profile,
        bytes: &[u8],
    ) -> (Vec<String>, Wire) {
        for columns in [40, 12] {
            let whole = wire_screen(&mut Wire::new(profile()), columns, &[bytes]);
            for at in cuts(bytes) {
                let reads = vosh_prompt::testkit::reads(bytes, &[at]);
                assert_eq!(
                    wire_screen(&mut Wire::new(profile()), columns, &reads),
                    whole,
                    "{columns} wide, cut after {at}"
                );
            }
        }
        let mut wire = Wire::new(profile());
        let screen = wire_screen(&mut wire, 80, &[bytes]);
        (screen, wire)
    }

    /// Every value lamented tears hides, as `session://hidden` reports it.
    fn all_hidden() -> serde_json::Value {
        serde_json::json!({"vitals": true, "tank": true, "opponent": true, "affects": true, "group": true})
    }

    const ROOM: [&str; 3] = [
        "The Bank of Aabahran",
        "  Marble counters line the hall, and a clerk nods at you.",
        "[Exits: south]",
    ];

    #[test]
    fn the_quiet_wire_draws_its_prompt_at_every_split() {
        let bytes = wire_fixture("quiet");
        let (screen, wire) = wire_same_at_every_split(&|| fake_profile(CODES), &bytes);
        assert_eq!(screen, [ROOM[0], ROOM[1], ROOM[2], "", "<1020>"]);
        let vars = wire.p.prompt.vars.prompt_vars();
        assert_eq!(vars.get("maxhp").map(String::as_str), Some("1020"));
        assert_eq!(wire.p.prompt.status(), vosh_prompt::Status::Matching);
    }

    #[test]
    fn the_fight_wire_reads_the_tank_block_at_every_split() {
        let bytes = wire_fixture("fight-tank");
        let (screen, wire) = wire_same_at_every_split(&|| fake_profile(CODES), &bytes);
        // The design reads nothing on the tank line, so it shows as sent.
        assert_eq!(
            screen,
            [
                "A Blackwatch guard attacks you!",
                "A Blackwatch guard has quite a few wounds.",
                "",
                "Tester: [===|===|===|---]",
                "<765>"
            ]
        );
        let vars = wire.p.prompt.vars.prompt_vars();
        assert_eq!(vars.get("tank").map(String::as_str), Some("Tester"));
        assert_eq!(vars.get("fight").map(String::as_str), Some("1"));
        // A design that reads the tank takes over the whole block.
        let profile = || {
            let mut p = codes_profile(CODES, "%tank %{tank_hp:pct}%% <%hp>");
            super::start_prompt(&mut p, false);
            p
        };
        let (screen, _) = wire_same_at_every_split(&profile, &bytes);
        assert_eq!(screen[3..], ["Tester 75% <765>"]);
    }

    #[test]
    fn each_lament_wire_hides_what_the_song_hides_at_every_split() {
        for (name, battle) in [
            ("lament-new", false),
            ("lament-243cac5c", false),
            ("lament-older", true),
        ] {
            let bytes = wire_fixture(name);
            let (screen, mut wire) = wire_same_at_every_split(&|| fake_profile(CODES), &bytes);
            let mut want = vec!["Tears fall as the lament takes you."];
            if battle {
                want.push("A Blackwatch guard has quite a few wounds.");
            }
            want.extend(["", "Tester:", "<?>"]);
            assert_eq!(screen, want, "{name}");
            let hidden = wire
                .p
                .prompt
                .vars
                .take_hidden_change()
                .expect("a change to report");
            assert_eq!(
                serde_json::to_value(hidden).expect("it serializes"),
                all_hidden(),
                "{name}"
            );
        }
        // A new build session that had Char.Prompt at login hides the
        // same values by the flags alone.
        let mut p = fake_profile(CODES);
        feed(&mut p, "char-prompt.gmcp");
        let mut wire = Wire::new(p);
        let _ = wire.read_wire(&wire_fixture("lament-new"));
        assert!(wire.p.prompt.vars.new_build());
        let hidden = wire.p.prompt.vars.take_hidden_change().expect("a change");
        assert_eq!(
            serde_json::to_value(hidden).expect("it serializes"),
            all_hidden()
        );
    }

    #[test]
    fn prompt_all_that_the_next_pulse_completes_draws_both_prompts() {
        let bytes = wire_fixture("prompt-all-next");
        let (screen, _) =
            wire_same_at_every_split(&|| fake_profile("%n%P%C<%hhp %mm %vmv> "), &bytes);
        assert_eq!(
            screen,
            [
                ROOM[0],
                ROOM[1],
                ROOM[2],
                "",
                "<1020>",
                "A Blackwatch guard arrives from the south.",
                "",
                "<1020>"
            ]
        );
    }

    #[test]
    fn a_ga_after_prompt_all_draws_with_no_flash_at_every_split() {
        let bytes = wire_fixture("ga");
        let (screen, _) =
            wire_same_at_every_split(&|| fake_profile("%n%P%C<%hhp %mm %vmv> "), &bytes);
        assert_eq!(screen, [ROOM[0], ROOM[1], ROOM[2], "", "<1020>"]);
        // In one read the game's own prompt never reaches the terminal.
        let mut wire = Wire::new(fake_profile("%n%P%C<%hhp %mm %vmv> "));
        let out = wire.read_wire(&bytes);
        assert!(!plain(&String::from_utf8_lossy(&out.bytes)).contains("mv>"));
    }

    #[test]
    fn an_eor_ends_a_prompt_as_a_ga_does() {
        let ga = wire_fixture("ga");
        let mark = ga.len() - 2;
        assert_eq!(ga[mark..], [255, 249]);
        let mut eor = ga.clone();
        eor[mark + 1] = 239;
        // A pattern that never settles, so only the mark makes the
        // partial your prompt.
        let profile = || {
            let mut p = Profile::default();
            p.set_prompt_config(vosh_prompt::PromptConfig {
                draw: true,
                template: HP.into(),
                capture: vosh_prompt::CaptureConfig::Regex(vosh_prompt::config::RegexCapture {
                    lines: vec![r"^<(?<hp>\d+)hp (?<mana>\d+)m (?<move>\d+)mv> $".into()],
                    settle: false,
                    ..vosh_prompt::config::RegexCapture::default()
                }),
                ..vosh_prompt::PromptConfig::default()
            });
            super::start_prompt(&mut p, false);
            p
        };
        let (at_ga, _) = wire_same_at_every_split(&profile, &ga);
        let (at_eor, _) = wire_same_at_every_split(&profile, &eor);
        assert_eq!(at_eor, at_ga);
        assert_eq!(at_eor.last().map(String::as_str), Some("<1020>"));
        // With no mark the game's own prompt shows.
        let bare = wire_screen(&mut Wire::new(profile()), 80, &[&ga[..mark]]);
        assert_eq!(bare.last().map(String::as_str), Some("<1020hp 800m 930mv>"));
    }

    #[test]
    fn the_login_wire_gives_vosh_the_prompt_with_no_typing() {
        let bytes = wire_fixture("login-new");
        // A capture that follows the game, started on another setting.
        let (screen, mut wire) = wire_same_at_every_split(&|| fake_profile("<%hhp> "), &bytes);
        assert_eq!(
            screen,
            [
                "Welcome to the fake Aabahran, Tester.",
                ROOM[0],
                ROOM[1],
                ROOM[2],
                "",
                "<1020>"
            ]
        );
        let vosh_prompt::CaptureConfig::Aabahran(codes) = &wire.p.prompt.config().capture else {
            panic!("an aabahran capture");
        };
        assert_eq!(codes.prompt, CODES);
        assert_eq!(codes.source, Some(vosh_prompt::config::CaptureSource::Gmcp));
        assert!(wire.p.prompt.vars.new_build());
        let seen = wire.p.prompt.take_seen();
        assert_eq!(
            seen,
            [vosh_prompt::GamePromptSeen {
                kind: vosh_prompt::engine::SeenKind::Gmcp,
                text: CODES.into(),
                applied: true,
                lost: Vec::new(),
            }]
        );
        // A profile that reads no prompt keeps the setting for the card.
        let mut p = Profile::default();
        super::start_prompt(&mut p, false);
        let mut wire = Wire::new(p);
        let _ = wire.read_wire(&bytes);
        let packet = wire
            .p
            .prompt
            .vars
            .gmcp()
            .char_prompt()
            .expect("the login Char.Prompt");
        assert_eq!(packet.prompt, CODES);
        assert!(packet.at_login);
        assert!(!wire.p.prompt.take_seen()[0].applied);
    }

    #[test]
    fn the_prompt_x_wire_reads_the_new_codes_right_after_the_reply() {
        let bytes = wire_fixture("prompt-x-new");
        let (screen, mut wire) = wire_same_at_every_split(&|| fake_profile(CODES), &bytes);
        assert_eq!(screen, ["Prompt set to <%h/%Hhp %m/%Mmn>", "", "<1020>"]);
        let vosh_prompt::CaptureConfig::Aabahran(codes) = &wire.p.prompt.config().capture else {
            panic!("an aabahran capture");
        };
        assert_eq!(codes.prompt, vosh_prompt::testkit::wire::PROMPT_X);
        let seen = wire.p.prompt.take_seen();
        assert_eq!(seen.len(), 1, "one toast, from Char.Prompt: {seen:?}");
        assert!(seen[0].applied);
    }

    #[test]
    fn the_prompts_off_wire_counts_no_miss_while_the_packages_keep_coming() {
        let bytes = wire_fixture("prompts-off-new");
        let (screen, mut wire) = wire_same_at_every_split(&|| fake_profile(CODES), &bytes);
        assert_eq!(
            screen,
            [
                "You will no longer see prompts.",
                "",
                "",
                "Pulse 1 of 3.",
                "",
                "",
                "Pulse 2 of 3.",
                "",
                "",
                "Pulse 3 of 3."
            ]
        );
        assert_eq!(wire.p.prompt.status(), vosh_prompt::Status::PromptsOff);
        let report = wire.p.prompt.take_status_change().expect("a report");
        assert_eq!(report.status, vosh_prompt::Status::PromptsOff);
        let echo = crate::input::run_line(&mut wire.p, "#prompt").result.echo;
        assert_eq!(
            echo.last().map(String::as_str),
            Some(
                "You turned prompts off in the game. Type prompt in the game to turn them back on."
            )
        );
    }

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn base64_handles_raw_ansi_and_high_bytes() {
        // ESC [ 3 1 m  (a red SGR sequence) plus a high byte.
        assert_eq!(base64_encode(&[0x1b, 0x5b, 0x33, 0x31, 0x6d]), "G1szMW0=");
        assert_eq!(base64_encode(&[0xff, 0xfe, 0xfd]), "//79");
    }
}

#[cfg(test)]
#[path = "session_show_tests.rs"]
mod show_tests;

#[cfg(test)]
#[path = "session_preview_tests.rs"]
mod preview_tests;

#[cfg(test)]
#[path = "session_repaint_tests.rs"]
mod repaint_tests;

#[cfg(test)]
#[path = "session_clock_tests.rs"]
mod clock_tests;

#[cfg(test)]
#[path = "session_right_tests.rs"]
mod right_tests;

#[cfg(test)]
#[path = "session_pointer_tests.rs"]
mod pointer_tests;

#[cfg(test)]
#[path = "session_room_tests.rs"]
mod room_tests;

#[cfg(test)]
#[path = "session_collapse_tests.rs"]
mod collapse_tests;

#[cfg(test)]
mod settle_tests {
    use super::{PerfCounters, Settle, FRAME_BUDGET};

    fn row(session_id: i64, text: &str) -> vosh_log::LogEntry {
        vosh_log::LogEntry {
            session_id,
            ts_ms: 0,
            text: text.to_string(),
            raw: None,
        }
    }

    #[test]
    fn a_burst_that_never_pauses_owes_its_frame_once_the_budget_runs_out() {
        let mut settle = Settle::default();
        assert!(!settle.frame_overdue(), "nothing drew yet");
        settle.drew();
        assert!(!settle.frame_overdue());
        std::thread::sleep(FRAME_BUDGET);
        assert!(settle.frame_overdue());
    }

    #[test]
    fn rows_with_nothing_drawn_come_due_once_the_budget_runs_out() {
        let mut settle = Settle::default();
        settle.queue_rows(Vec::new());
        assert!(!settle.log_overdue(), "no rows wait yet");
        settle.queue_rows([row(1, "> east")]);
        assert!(!settle.log_overdue());
        std::thread::sleep(FRAME_BUDGET);
        assert!(settle.log_overdue(), "the row waited out the budget");
        assert!(!settle.frame_overdue(), "nothing drew");
        settle.write_log(None, &mut PerfCounters::default());
        assert!(!settle.log_overdue(), "the clock stops with the write");
        settle.queue_rows([row(1, "> west")]);
        assert!(!settle.log_overdue(), "a new row starts a new clock");
    }

    #[test]
    fn the_waiting_rows_go_in_once_in_the_order_they_came() {
        let mut store = vosh_log::LogStore::in_memory().expect("a log");
        let id = store.start_session("h", 1, 0).expect("a session");
        let mut settle = Settle::default();
        settle.queue_rows([row(id, "a room"), row(id, "> east")]);
        settle.queue_rows([row(id, "the next room")]);
        settle.write_log(Some(&mut store), &mut PerfCounters::default());
        assert!(settle.log.is_empty());
        settle.write_log(Some(&mut store), &mut PerfCounters::default());
        assert_eq!(
            store.export_session(id, false).expect("the rows"),
            "a room\n> east\nthe next room\n"
        );
    }
}

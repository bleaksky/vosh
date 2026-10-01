//! Per-session task. Wires the connection, the telnet parser, the line
//! accumulator, and the trigger engine together. Emits Tauri events.

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
use vosh_prompt::stage::{Block, BlockLine, End, Offer, Output};
use vosh_telnet::{
    codes as telnet_codes, option as telnet_option, Event as TelnetEvent, Negotiator, Parser,
};
use vosh_trigger::{LineResult, MatchScope};

use crate::connection::{self, ConnectionError, Stream};
use crate::gmcp_bind;
use crate::hidden_input::{self, ServerEcho};
use crate::input;
use crate::line_accumulator::{Line, LineAccumulator, Partial};
use crate::list_events::{broadcast_list_changes, ListChanges, ListRevisions};
use crate::map_state::{self, SharedMap};
use crate::profile::Profile;
use crate::profile_config::SharedLayer;
use crate::script_state::{self, ApplyResult, PendingTimer, SharedTimers};
use crate::tick::{TickRuntime, TickStep};

const TICK_EMIT_INTERVAL: Duration = Duration::from_millis(250);

const READ_BUFFER_BYTES: usize = 8 * 1024;

const PERF_REPORT_INTERVAL: Duration = Duration::from_secs(1);

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
/// short keywords like "helg" won't equality-match
/// "The Baron Helgardium" but the backend already resolved the
/// pointer via substring.
#[derive(Debug, Clone, Serialize)]
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
    /// screen.
    LocalWrite,
}

/// Everything one socket read writes to the terminal and reports, kept
/// in stream order and sent once at the end of the read, so a prompt
/// that arrives in one read never flashes. The Line pass, the GMCP
/// handler's echoes and the GA path all write here.
struct ReadBatch {
    /// The terminal output, with the regions the prompt stage marks.
    out: Output,
    /// Log rows, flushed in one transaction.
    log: Vec<vosh_log::LogEntry>,
    /// A prompt var changed or a prompt was read, so the prompt vars go
    /// out after the output even when they read the same.
    prompt_vars: bool,
    /// Triggers that hid a prompt while nothing reads it, each named
    /// once a session.
    gag_without_reader: Vec<String>,
    /// The character Char.Status named in this read, for the log's
    /// session row.
    character: Option<String>,
    /// The read ended on a partial that can still become your prompt, so
    /// it waits a moment for the next read instead of painting raw.
    hold: bool,
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
            gag_without_reader: Vec::new(),
            character: None,
            hold: false,
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

    /// Tell the session the webview wrote to the terminal itself, which
    /// closes the open row. Returns false when the session has already
    /// been torn down.
    pub(crate) fn local_write(&self) -> bool {
        self.tx_outgoing.send(OutgoingMsg::LocalWrite).is_ok()
    }

    /// Repaint the open row as the `[prompt]` table now says. Returns
    /// false when the session has already been torn down.
    pub(crate) fn prompt_repaint(&self) -> bool {
        self.tx_outgoing.send(OutgoingMsg::PromptRepaint).is_ok()
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
    map: SharedMap,
    timers: SharedTimers,
    logs: crate::log_state::SharedLogStore,
    scrollback: crate::log_state::SharedScrollback,
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

    // Proactively ask for end-of-record so the server marks each prompt.
    // Without this, ROM derivatives that gate EOR on negotiation never
    // send the byte, and we have to merge the prompt with the next room
    // line. Other negotiations stay reactive in handle_event.
    let initial = [vosh_telnet::IAC, telnet_codes::DO, telnet_option::EOR];
    if let Err(e) = stream.write_all(&initial).await {
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
        map,
        timers,
        logs,
        log_session_id,
        scrollback,
        scrollback_path,
        initial_window_size,
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
    map: SharedMap,
    timers: SharedTimers,
    logs: crate::log_state::SharedLogStore,
    log_session_id: Option<i64>,
    scrollback: crate::log_state::SharedScrollback,
    scrollback_path: Option<std::path::PathBuf>,
    initial_window_size: (u16, u16),
    known_host: bool,
) {
    let mut parser = Parser::new();
    let mut negotiator = Negotiator::new();
    // Seed the negotiator with the size we already know about so the
    // first `DO NAWS` from the server gets a correct subneg, instead
    // of the 80×24 default carrying through until the user nudges
    // the window. Stale-NAWS was visible in `who` output wrapping
    // mid-sentence before the user reported it.
    negotiator.set_window_size(initial_window_size.0, initial_window_size.1);
    let mut accumulator = LineAccumulator::new();
    let mut buf = vec![0u8; READ_BUFFER_BYTES];
    // Track whether NAWS has been negotiated. Server sends DO NAWS,
    // we respond WILL NAWS + initial subneg. From then on, every
    // OutgoingMsg::WindowSize emits a fresh NAWS subneg so the MUD
    // re-wraps its output at the new column count.
    let mut naws_active = false;
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
                        flush_hold(&app, &profile, &mut accumulator, &mut seen_output).await;
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
                    // Append the input line(s) to the same log session
                    // as server output so transcripts include both
                    // directions. While the server holds echo (a
                    // password prompt), and for any line typed into the
                    // masked field, each line is logged as `> (hidden)`
                    // and its text never reaches the store. See
                    // `hidden_input::sent_log_rows`.
                    if let Some(sid) = log_session_id {
                        let rows =
                            hidden_input::sent_log_rows(&bytes, server_echo.hides(masked));
                        if !rows.is_empty() {
                            let mut guard = logs.lock().await;
                            if let Some(store) = guard.as_mut() {
                                hidden_input::append_sent_rows(store, sid, now_ms(), &rows);
                            }
                        }
                    }
                    if let Err(e) = stream.write_all(&bytes).await {
                        error!(error = %e, "write failed");
                        break Some(format!("write failed: {e}"));
                    }
                    if let Err(e) = stream.flush().await {
                        error!(error = %e, "flush failed");
                        break Some(format!("flush failed: {e}"));
                    }
                }
                Some(OutgoingMsg::WindowSize { cols, rows }) => {
                    {
                        let mut p = profile.lock().await;
                        window_size_step(&mut p, &mut negotiator, cols, rows);
                    }
                    if naws_active {
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
                Some(OutgoingMsg::LocalWrite) => {
                    if hold_until.take().is_some() {
                        flush_hold(&app, &profile, &mut accumulator, &mut seen_output).await;
                    }
                    // Your typed echo follows the lines held for the rest
                    // of a prompt, so they let go as they show.
                    if let Err(e) = let_go_held_lines(
                        &app,
                        &mut stream,
                        &profile,
                        &timers,
                        &scrollback,
                        &logs,
                        &mut log_session,
                        &mut seen_output,
                        &mut perf,
                    )
                    .await
                    {
                        warn!(error = %e, "letting go of held lines failed");
                    }
                    let mut p = profile.lock().await;
                    p.prompt.stage.local_write();
                }
                Some(OutgoingMsg::PromptRepaint) => {
                    let out = {
                        let mut p = profile.lock().await;
                        repaint_step(&mut p, output_count() != seen_output, Instant::now())
                    };
                    if !out.is_empty() {
                        emit_repaint(&app, &out);
                    }
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
                        // Once the server sends DO NAWS we know NAWS is
                        // active and future window-size changes can push
                        // a fresh subneg.
                        if let TelnetEvent::Do(opt) = &event {
                            if *opt == telnet_option::NAWS {
                                naws_active = true;
                            }
                        }
                        if let Err(e) = handle_event(
                            &app,
                            &mut stream,
                            &negotiator,
                            &mut accumulator,
                            &profile,
                            &map,
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
                    finish_read(
                        &app,
                        &profile,
                        &logs,
                        &mut log_session,
                        batch,
                        &mut seen_output,
                        &mut perf,
                    )
                    .await;
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
                                        &negotiator,
                                        &mut accumulator,
                                        &profile,
                                        &map,
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
                                finish_read(
                                    &app,
                                    &profile,
                                    &logs,
                                    &mut log_session,
                                    batch,
                                    &mut seen_output,
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
                flush_hold(&app, &profile, &mut accumulator, &mut seen_output).await;
            }
            _ = tick_interval.tick() => {
                if let Err(e) = handle_tick(&app, &mut stream, &profile).await {
                    error!(error = %e, "tick handling failed");
                    break Some(format!("tick handling failed: {e}"));
                }
                if let Err(e) = fire_due_script_timers(&app, &mut stream, &profile, &timers).await {
                    error!(error = %e, "script timer firing failed");
                }
                if let Err(e) =
                    fire_due_profile_timers(&app, &mut stream, &profile, &mut timer_next).await
                {
                    error!(error = %e, "profile timer firing failed");
                }
                perf.tick_emits += 1;
            }
            _ = perf_report_interval.tick() => {
                perf.report_and_reset();
            }
        }
    };

    // Capture the MUD's final partial line before teardown drops it. A
    // `quit` logout banner usually arrives without a trailing newline,
    // so it sits in the accumulator as a partial: painted at the end of
    // its read but never run through the per-line path that logs and
    // scrollback-records it.
    // Flush it now, ahead of the scrollback dump and log close below, so
    // the goodbye is captured like every other client captures it.
    if hold_until.is_some() {
        flush_hold(&app, &profile, &mut accumulator, &mut seen_output).await;
    }
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

    accumulator.reset();
    let line_triggers;
    // Session-only target state and the cached Room.Chars list clear
    // on disconnect — quick-key verb bindings persist via the profile
    // config but the active target and room snapshot are ephemeral.
    let target_after = {
        let mut p = profile.lock().await;
        let had = p.target.name.is_some();
        p.target.name = None;
        p.target.room_idx = None;
        p.room_chars.clear();
        p.vars.remove("target");
        line_triggers = p.prompt.stage.line_trigger_notice();
        end_prompt(&mut p);
        had.then(|| p.target.quick_keys.clone())
    };
    // The first session that read your prompt names the Line triggers
    // that matched it, once, at the next launch (D6).
    if let Some(names) = line_triggers {
        let state = app.state::<crate::commands::SharedState>();
        crate::prompt_migration::note_line_triggers(state.inner(), names).await;
    }
    if let Some(quick_keys) = target_after {
        let _ = app.emit(
            "session://target",
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
    let shared = app.state::<crate::commands::SharedState>();
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
    deliver_tick_step(app, stream, profile, step, &mut OutputSink::Direct).await
}

/// Report a tick step on `session://tick`, so the frontend counts and
/// plays the sound when it fired, then run its Send each tick command
/// through the full input pipeline like a timer command.
async fn deliver_tick_step<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stream: &mut Stream,
    profile: &Arc<Mutex<Profile>>,
    step: TickStep,
    sink: &mut OutputSink<'_>,
) -> std::io::Result<()> {
    if let Err(e) = app.emit("session://tick", &step.payload) {
        warn!(error = %e, "failed to emit tick payload");
    }
    if let Some(command) = step.command {
        run_fired_command(app, stream, profile, &command, sink).await?;
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
        run_fired_command(app, stream, profile, &command, &mut OutputSink::Direct).await?;
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
) -> input::InputResult {
    let ran = match shared.filter(|_| input::may_replace_profile(line)) {
        Some(layer) => layer.keep_across(p, |p| input::run_line(p, line)),
        None => input::run_line(p, line),
    };
    effects.note_ran(line, &ran);
    if ran.replaced {
        crate::commands::note_ui_config_replaced();
    }
    ran.result
}

/// What one timer command produced under the profile lock.
struct FiredRun {
    echoes: Vec<String>,
    bytes: Vec<u8>,
    lists: ListChanges,
    effects: input::LineEffects,
}

/// The part of [`run_fired_command`] that runs under the profile lock:
/// the input pipeline, then the Lua bodies of any script aliases it
/// queued.
fn run_fired_locked(p: &mut Profile, command: &str, shared: Option<&SharedLayer>) -> FiredRun {
    let lists_before = ListRevisions::of(p);
    let mut effects = input::LineEffects::default();
    let result = process_fired_line(p, command, &mut effects, shared);
    let mut echoes = result.echo;
    let mut bytes = result.bytes;
    // Evaluate any Lua bodies queued by script-bodied aliases and
    // fold their sends / echoes in, same as the typed-input path.
    if !result.scripts.is_empty() {
        let mut outcome = vosh_script::ScriptOutcome::default();
        for call in &result.scripts {
            match script_state::eval_with_captures(
                &mut p.script,
                &call.body,
                &call.captures,
                "timer-script",
            ) {
                Ok(o) => outcome.actions.extend(o.actions),
                Err(err) => warn!(error = %err, "timer script eval failed"),
            }
        }
        let apply = script_state::apply_actions(p, outcome);
        effects.note_script(apply.durable_changed);
        echoes.extend(apply.echoes);
        bytes.extend(apply.send_bytes);
    }
    FiredRun {
        echoes,
        bytes,
        lists: ListChanges::since(lists_before, p),
        effects,
    }
}

/// Run one command produced by a timer (or any non-typed source) through
/// the full input pipeline and deliver its results: echo lines to the
/// terminal, queued alias-script Lua bodies evaluated and their actions
/// applied, and the combined bytes sent to the server. Mirrors the
/// typed-input handler so a timer command behaves exactly like the same
/// line typed at the prompt, including `#lua` and script-bodied aliases.
async fn run_fired_command<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stream: &mut Stream,
    profile: &Arc<Mutex<Profile>>,
    command: &str,
    sink: &mut OutputSink<'_>,
) -> std::io::Result<()> {
    let shared = crate::commands::shared_layer_for_lines(app, [command]).await;
    let FiredRun {
        echoes,
        bytes,
        lists,
        effects,
    } = {
        let mut p = profile.lock().await;
        run_fired_locked(&mut p, command, shared.as_ref())
    };
    broadcast_list_changes(app, lists);
    crate::commands::settle_line_effects(app, effects).await;
    if !echoes.is_empty() {
        sink.write(app, framed_echoes(&echoes));
    }
    if !bytes.is_empty() {
        stream.write_all(&bytes).await?;
        stream.flush().await?;
    }
    Ok(())
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
    negotiator: &Negotiator,
    accumulator: &mut LineAccumulator,
    profile: &Arc<Mutex<Profile>>,
    map: &SharedMap,
    timers: &SharedTimers,
    log_session_id: Option<i64>,
    scrollback: &crate::log_state::SharedScrollback,
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
            // flush in one transaction at the end of the read.
            for line in accumulator.feed(&bytes) {
                perf.lines_processed += 1;
                let plain = vosh_ansi::plain_text(&line.bytes);
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
            handle_gmcp(app, profile, map, timers, stream, &payload, batch, perf).await?;
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
            // ourselves and the packages we want.
            let response = negotiator.handle(&TelnetEvent::Will(opt));
            stream.write_all(&response).await?;
            stream.write_all(&hello_subnegotiation()).await?;
            stream.write_all(&supports_subnegotiation()).await?;
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
/// once.
fn line_pass(p: &mut Profile, bytes: &[u8], plain: &str, now: Instant) -> LinePass {
    let result = vosh_trigger::process_with_plain(&p.triggers, bytes, plain, MatchScope::Line);
    let tick_step = tick_reset(p, plain, now);
    script_state::snapshot_vars(&p.script, &p.vars);
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
    let apply = script_state::apply_actions(p, outcome);
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
        match script_state::eval_with_captures(&mut p.script, &call.body, &call.captures, chunk) {
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
    let LinePass {
        result,
        tick_step,
        mut apply,
    } = line_pass(p, &bytes, &plain, now);
    if result.display.is_none() {
        note_gag_without_reader(p, batch, &plain, MatchScope::Line);
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
    let kept = match shows {
        Shows::Now(painted) => {
            if let Some(text) = &result.display {
                shown.extend_from_slice(text.as_bytes());
                shown.extend_from_slice(b"\r\n");
            }
            p.prompt
                .stage
                .line(&mut batch.out, &bytes, &plain, painted, &shown);
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
    // A line that shows is logged, in one transaction at the end of the
    // read, and kept in the ring buffer that becomes scrollback on the
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
        scrollback.push(text);
    }
    LineStep {
        result,
        apply,
        tick_step,
        scrollback,
    }
}

/// Your prompt, recognized. Under the profile lock the Line pass takes,
/// in this order: the capture's values merge and the hidden state
/// follows them, Prompts triggers run on its final line so Lua
/// `mud.set_prompt_var` lands before the render, and then the design
/// draws in its place, or with drawing off it shows as sent. Line
/// triggers and Lua `match_line` never see it (D6), and the tick reset
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
        // fired, for the one-time notice (D6).
        let matched = vosh_trigger::matching(&p.triggers, &line.plain, MatchScope::Line);
        p.prompt
            .stage
            .line_triggers_matched(matched.into_iter().map(|t| t.name.as_str()));
    }

    let last = block.final_line().clone();
    let result =
        vosh_trigger::process_with_plain(&p.triggers, &last.raw, &last.plain, MatchScope::Prompt);
    if !result.scripts.is_empty() {
        script_state::snapshot_vars(&p.script, &p.vars);
    }
    let outcome = vosh_script::ScriptOutcome {
        actions: run_trigger_scripts(p, &result, "prompt-trigger-script"),
    };
    let mut apply = script_state::apply_actions(p, outcome);
    batch.prompt_vars = true;

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
        let rendered = render_prompt(p, now);
        // A line above the last one your design reads nothing on shows
        // as the game sent it (D7).
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
                .pin_drawn(&mut batch.out, block, painted, &before, &rendered.ansi);
        } else {
            p.prompt
                .stage
                .draw(&mut batch.out, block, painted, &before, &rendered.ansi);
            p.prompt.stage.set_open_spans(rendered.spans);
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
                .show(&mut batch.out, block, painted, &before, display);
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
    }
}

/// A line of your prompt that shows: logged, in one transaction at the
/// end of the read, and kept in the ring buffer that becomes scrollback,
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
    let result =
        vosh_trigger::process_with_plain(&p.triggers, &partial.bytes, plain, MatchScope::Prompt);
    let effect = match &result.display {
        None => true,
        Some(text) => text.as_bytes() != partial.bytes,
    } || !result.sends.is_empty()
        || !result.routes.is_empty()
        || !result.scripts.is_empty();
    let mut apply = ApplyResult::default();
    if effect {
        script_state::snapshot_vars(&p.script, &p.vars);
        let outcome = vosh_script::ScriptOutcome {
            actions: run_trigger_scripts(p, &result, "prompt-trigger-script"),
        };
        apply = script_state::apply_actions(p, outcome);
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
        return steps;
    };
    let plain = vosh_ansi::plain_text(&partial.bytes);
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
        let plain = vosh_ansi::plain_text(&bytes);
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
) {
    let out = {
        let mut p = profile.lock().await;
        let mut out = Output::new(output_count() != *seen);
        hold_step(&mut p, accumulator, &mut out);
        out
    };
    if !out.is_empty() {
        *seen = emit_session_output(app, &out);
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
    scrollback: &crate::log_state::SharedScrollback,
    logs: &crate::log_state::SharedLogStore,
    log_session: &mut LogSession,
    seen: &mut u64,
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
    finish_read(app, profile, logs, log_session, batch, seen, perf).await;
    Ok(())
}

/// Log the lines the stage still holds as the session ends, and keep them
/// for scrollback, through [`end_held`].
async fn capture_held_lines(
    profile: &Arc<Mutex<Profile>>,
    logs: &crate::log_state::SharedLogStore,
    log_session_id: Option<i64>,
    scrollback: &crate::log_state::SharedScrollback,
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
        .map(|bytes| (bytes.to_vec(), vosh_ansi::plain_text(bytes)));
    p.prompt.record(
        partial
            .as_ref()
            .map(|(bytes, plain)| (&bytes[..], plain.as_str())),
        at_ms,
    );
    p.prompt.stage.close();
    p.prompt.note_send(&String::from_utf8_lossy(sent), at_ms)
}

/// A window size message. A new size closes the open row, since each
/// renderer wraps it again at its new width, and nothing repaints until
/// the next prompt (D21). The size the session already holds, which the
/// webview sends again on every connect, leaves the row open.
fn window_size_step(p: &mut Profile, negotiator: &mut Negotiator, cols: u16, rows: u16) {
    if negotiator.window_size != (cols, rows) {
        p.prompt.stage.close();
    }
    negotiator.set_window_size(cols, rows);
}

/// Repaint the open row as the `[prompt]` table now says: your design
/// while Vosh draws, else the lines the design replaced, as the game sent
/// them, so turning drawing off shows the game's prompt at once. `other`
/// says output from elsewhere landed since the session last wrote, which
/// closed the row. Returns the repaint, empty when no row is open.
fn repaint_step(p: &mut Profile, other: bool, now: Instant) -> Output {
    let mut out = Output::new(other);
    let rendered = p.prompt.draws().then(|| render_prompt(p, now));
    p.prompt
        .stage
        .repaint(&mut out, rendered.as_ref().map(|r| r.ansi.as_str()));
    p.prompt
        .stage
        .set_open_spans(rendered.map(|r| r.spans).unwrap_or_default());
    out
}

/// A trigger hid a line or partial. When nothing reads your prompt in
/// this profile and the trigger also sets prompt values, it hides your
/// prompt with nothing drawn in its place, so the webview hears its name
/// once a session.
fn note_gag_without_reader(p: &mut Profile, batch: &mut ReadBatch, plain: &str, scope: MatchScope) {
    if p.prompt.stage.has_recognizer() {
        return;
    }
    for trigger in vosh_trigger::matching(&p.triggers, plain, scope) {
        if hides_and_reads_prompt(trigger) && p.prompt.stage.gag_without_reader(&trigger.name) {
            batch.gag_without_reader.push(trigger.name.clone());
        }
    }
}

/// The trigger hides what it matches and its script sets prompt values,
/// the shape of a capture trigger.
fn hides_and_reads_prompt(trigger: &vosh_trigger::Trigger) -> bool {
    use vosh_trigger::TriggerAction;
    trigger
        .actions
        .iter()
        .any(|a| matches!(a, TriggerAction::Gag))
        && trigger
            .actions
            .iter()
            .any(|a| matches!(a, TriggerAction::Script { body } if body.contains("set_prompt_var")))
}

/// Your design drawn from the live values. The vosh-prompt resolver reads
/// the values the capture and scripts set, then the latest GMCP packets,
/// then what Vosh itself knows, and draws `?` for a value the game hides.
/// The spans say where each piece landed, which the open row keeps for
/// the prompt card.
fn render_prompt(p: &Profile, now: Instant) -> vosh_prompt::Rendered {
    let vosh = prompt_supplies(p, now);
    vosh_prompt::render_str(
        &p.prompt.config().template,
        &p.prompt.vars.resolver(&vosh),
        vosh_prompt::RenderOptions::default(),
    )
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
    scrollback: &crate::log_state::SharedScrollback,
    batch: &mut ReadBatch,
    step: LineStep,
    perf: &mut PerfCounters,
) -> std::io::Result<()> {
    let LineStep {
        result,
        apply,
        tick_step,
        scrollback: kept,
    } = step;
    if !result.routes.is_empty() {
        perf.routed_emits += result.routes.len() as u64;
    }
    emit_line_routes(app, &result);
    for text in kept {
        let sb_t0 = std::time::Instant::now();
        scrollback.lock().await.push(text);
        perf.scrollback_push_ns += sb_t0.elapsed().as_nanos() as u64;
        perf.scrollback_pushes += 1;
    }
    send_trigger_outputs(stream, &result.sends).await?;
    let mut sink = OutputSink::Batch(batch);
    apply_script_result(app, stream, profile, timers, apply, &mut sink).await?;
    if let Some(step) = tick_step {
        deliver_tick_step(app, stream, profile, step, &mut sink).await?;
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
    scrollback: &crate::log_state::SharedScrollback,
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
pub(crate) fn prompt_supplies(p: &Profile, now: Instant) -> vosh_prompt::Vosh {
    let tick = p.tick.remaining(now).map(|left| vosh_prompt::vars::Tick {
        remaining: i64::try_from(left.as_millis().div_ceil(1000)).unwrap_or(i64::MAX),
        interval: i64::try_from(p.tick.config.interval.as_secs()).ok(),
    });
    vosh_prompt::Vosh {
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
fn observe_prompt_gmcp(p: &mut Profile, msg: &vosh_gmcp::Message) {
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
        if let Err(e) = app.emit("session://hidden", hidden) {
            warn!(error = %e, "failed to emit the hidden state");
        }
    }
}

/// Send what one socket read gathered: its output, its log rows in one
/// transaction, the triggers that hid a prompt with nothing to draw in
/// its place, then the prompt vars when a prompt was read or they
/// changed, and the hidden state when it changed. Once per read, so the
/// packets of one pulse never show the panes a state between them.
/// `seen` becomes the output count after this read's output.
async fn finish_read<R: tauri::Runtime>(
    app: &AppHandle<R>,
    profile: &Arc<Mutex<Profile>>,
    logs: &crate::log_state::SharedLogStore,
    log_session: &mut LogSession,
    batch: ReadBatch,
    seen: &mut u64,
    perf: &mut PerfCounters,
) {
    let ReadBatch {
        mut out,
        log,
        prompt_vars,
        gag_without_reader,
        character,
        hold: _,
    } = batch;
    let (vars, hidden, prompt_seen, status) = {
        let mut p = profile.lock().await;
        // Echoes the end of the read wrote close the open row.
        p.prompt.stage.finish(&mut out);
        (
            p.prompt.take_prompt_vars(prompt_vars),
            p.prompt.vars.take_hidden_change(),
            p.prompt.take_seen(),
            p.prompt.take_status_change(),
        )
    };
    if !out.is_empty() {
        perf.output_emits += 1;
        perf.output_emit_bytes +=
            (out.bytes.len() + out.hold.len() + out.replace.as_ref().map_or(0, |r| r.bytes.len()))
                as u64;
        *seen = emit_session_output(app, &out);
    }
    if !log.is_empty() {
        let lock_t0 = std::time::Instant::now();
        let mut guard = logs.lock().await;
        perf.mutex_wait_ns += lock_t0.elapsed().as_nanos() as u64;
        perf.mutex_acquires += 1;
        if let Some(store) = guard.as_mut() {
            let append_t0 = std::time::Instant::now();
            perf.log_appends += log.len() as u64;
            if let Err(e) = store.append_batch(&log) {
                warn!(error = %e, "log append_batch failed");
            }
            perf.log_append_ns += append_t0.elapsed().as_nanos() as u64;
        }
    }
    if let Some(character) = character {
        log_session.name(logs, &character).await;
    }
    for trigger in gag_without_reader {
        if let Err(e) = app.emit(
            "session://prompt-gag-without-reader",
            GagWithoutReaderPayload { trigger },
        ) {
            warn!(error = %e, "failed to emit a trigger that hides the prompt");
        }
    }
    if let Some(vars) = vars {
        send_prompt_vars(app, &vars);
    }
    if let Some(hidden) = hidden {
        if let Err(e) = app.emit("session://hidden", hidden) {
            warn!(error = %e, "failed to emit the hidden state");
        }
    }
    report_game_prompt_seen(app, prompt_seen);
    if let Some(status) = status {
        if let Err(e) = app.emit("session://prompt-status", status) {
            warn!(error = %e, "failed to emit the prompt status");
        }
    }
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
        if let Err(e) = app.emit("session://game-prompt-seen", payload) {
            warn!(error = %e, "failed to emit the game's prompt settings");
        }
    }
    if applied {
        crate::commands::mark_profile_dirty(app);
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
    async fn name(&mut self, logs: &crate::log_state::SharedLogStore, character: &str) {
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
    map: &SharedMap,
    timers: &SharedTimers,
    stream: &mut Stream,
    payload: &[u8],
    batch: &mut ReadBatch,
    perf: &mut PerfCounters,
) -> std::io::Result<()> {
    let msg = match vosh_gmcp::parse(payload) {
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
                let state = app.state::<crate::commands::SharedState>();
                crate::commands::handle_char_known_for_auto_switch(app, state.inner(), &owned)
                    .await;
            }
        }
    }
    let mut sink = OutputSink::Batch(batch);
    if let Some(step) = tick_step {
        perf.tick_emits += 1;
        deliver_tick_step(app, stream, profile, step, &mut sink).await?;
    }
    apply_script_result(app, stream, profile, timers, script_apply, &mut sink).await?;
    if msg.package == "Room.Info" {
        // Map-store SQLite writes ride a dedicated single-consumer task
        // (ordering preserved) instead of running inline on the io loop,
        // where they sat between a socket read and the next outgoing
        // command write and contributed to command latency.
        let _ = map_writer(map).send(msg.clone());
    }
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
    app.state::<crate::commands::SharedState>()
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
    msg: &vosh_gmcp::Message,
    now: Instant,
) -> (Option<TickStep>, ApplyResult) {
    gmcp_bind::apply(&mut p.vars, msg);
    // Before Lua, so a value a GMCP handler sets with
    // `mud.set_prompt_var` belongs to the pulse this packet starts.
    observe_prompt_gmcp(p, msg);
    // Cache the latest Room.Chars snapshot in the profile so
    // bare `tar <index>` / `tarn` / `tarp` commands can resolve
    // against the current room without round-tripping to the
    // frontend.
    if msg.package == "Room.Chars" {
        if let Some(arr) = msg.data.as_array() {
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
    let tick_step = observe_world_time_for_tick(&mut p.tick, msg, now);
    script_state::snapshot_vars(&p.script, &p.vars);
    let outcome = match p.script.dispatch_gmcp(&msg.package, &msg.data) {
        Ok(o) => o,
        Err(err) => {
            warn!(error = %err, "lua dispatch_gmcp failed");
            vosh_script::ScriptOutcome::default()
        }
    };
    let apply = script_state::apply_actions(p, outcome);
    (tick_step, apply)
}

/// Lazily-started single-consumer task that applies `Room.Info` map
/// updates off the session io loop. One consumer preserves room-visit
/// ordering (spawn-per-message would not).
static MAP_WRITER: std::sync::OnceLock<tokio::sync::mpsc::UnboundedSender<vosh_gmcp::Message>> =
    std::sync::OnceLock::new();

fn map_writer(
    map: &crate::map_state::SharedMap,
) -> &'static tokio::sync::mpsc::UnboundedSender<vosh_gmcp::Message> {
    MAP_WRITER.get_or_init(|| {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<vosh_gmcp::Message>();
        let map = map.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if let Err(e) = map_state::handle_room_info(&map, &msg).await {
                    warn!(error = %e, "failed to update map from Room.Info");
                }
            }
        });
        tx
    })
}

/// Detect the game's tick from a GMCP `World.Time` push. Aabahran (and
/// most ROM derivatives that ship World.Time) advance the `hour` field
/// every server tick, so an hour change is the tick. Returns the step to
/// deliver when the change counted as a tick.
fn observe_world_time_for_tick(
    tick: &mut TickRuntime,
    msg: &vosh_gmcp::Message,
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
    let body = vosh_gmcp::build(
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
    let body = vosh_gmcp::build("Core.Supports.Set", &REQUESTED_GMCP_PACKAGES.to_vec())
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
                "session://routed",
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

/// Perform the IO and timer bookkeeping for the actions a Lua callback
/// produced. Sends and echoes flow to the server and the terminal pane;
/// timers register with the shared list; `mud.input` lines are run through
/// the input pipeline so they pick up aliases and slash commands too.
async fn apply_script_result<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stream: &mut Stream,
    profile: &Arc<Mutex<Profile>>,
    timers: &SharedTimers,
    apply: ApplyResult,
    sink: &mut OutputSink<'_>,
) -> std::io::Result<()> {
    // Durable Lua mutations (mud.alias / set_var / group toggles fired by
    // triggers or timers) historically never reached disk. Ride the same
    // debounced persist the slash commands use.
    if apply.durable_changed {
        crate::commands::mark_profile_dirty(app);
    }
    // A Lua `mud.alias` changes the list an open Settings page shows.
    broadcast_list_changes(app, apply.lists);

    if !apply.send_bytes.is_empty() {
        stream.write_all(&apply.send_bytes).await?;
        stream.flush().await?;
    }
    if !apply.echoes.is_empty() {
        sink.write(app, framed_echoes(&apply.echoes));
    }
    if !apply.inputs.is_empty() {
        let mut input_bytes = Vec::new();
        let mut input_echoes: Vec<String> = Vec::new();
        let shared =
            crate::commands::shared_layer_for_lines(app, apply.inputs.iter().map(String::as_str))
                .await;
        let mut effects = input::LineEffects::default();
        let lists = {
            let mut p = profile.lock().await;
            let before = ListRevisions::of(&p);
            for line in apply.inputs {
                let result = process_fired_line(&mut p, &line, &mut effects, shared.as_ref());
                input_bytes.extend(result.bytes);
                input_echoes.extend(result.echo);
            }
            ListChanges::since(before, &p)
        };
        broadcast_list_changes(app, lists);
        crate::commands::settle_line_effects(app, effects).await;
        if !input_bytes.is_empty() {
            stream.write_all(&input_bytes).await?;
            stream.flush().await?;
        }
        if !input_echoes.is_empty() {
            sink.write(app, framed_echoes(&input_echoes));
        }
    }
    if !apply.new_timers.is_empty() || !apply.cancel_timers.is_empty() {
        let mut guard = timers.lock().await;
        for cancel in apply.cancel_timers {
            guard.retain(|t| t.timer_id != cancel);
        }
        guard.extend(apply.new_timers);
    }
    if apply.prompt_vars_changed {
        sink.prompt_vars(app, profile).await;
    }
    Ok(())
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
    if let Err(e) = app.emit("session://prompt-vars", vars) {
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
        script_state::snapshot_vars(&p.script, &p.vars);
        let mut outcome = vosh_script::ScriptOutcome::default();
        for t in due {
            match p.script.fire_timer(t.callback_id) {
                Ok(o) => outcome.actions.extend(o.actions),
                Err(err) => warn!(error = %err, "lua timer fire failed"),
            }
        }
        script_state::apply_actions(&mut p, outcome)
    };
    apply_script_result(app, stream, profile, timers, apply, &mut OutputSink::Direct).await
}

/// Flush a partial line still buffered when the session ends so the MUD's
/// final output (a logout banner on `quit`, most often) is captured rather
/// than discarded by the disconnect `accumulator.reset()`. The end of its
/// read painted it, so display only needs the terminating newline. The
/// value of this pass is logging it and pushing it into the scrollback
/// ring that the dump persists.
async fn capture_pending_line<R: tauri::Runtime>(
    app: &AppHandle<R>,
    logs: &crate::log_state::SharedLogStore,
    log_session_id: Option<i64>,
    scrollback: &crate::log_state::SharedScrollback,
    accumulator: &mut LineAccumulator,
) {
    let Some(Partial { bytes, painted }) = accumulator.take_partial() else {
        return;
    };
    let plain = vosh_ansi::plain_text(&bytes);
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
            }),
            restore: out.restore.as_deref().map(base64_encode),
            pin: out.pin.as_deref().map(base64_encode),
            hold: (!out.hold.is_empty()).then(|| base64_encode(&out.hold)),
            pin_row: out.pin_row,
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
    emit_counted(app, &out, true);
}

/// Send one read's output. Returns the output count after it.
fn emit_session_output<R: tauri::Runtime>(app: &AppHandle<R>, out: &Output) -> u64 {
    emit_counted(app, out, true)
}

/// Send a repaint of the open row. It leaves the output count alone,
/// since the row it writes is still the last thing on screen.
fn emit_repaint<R: tauri::Runtime>(app: &AppHandle<R>, out: &Output) {
    let _ = emit_counted(app, out, false);
}

/// Send `out` to both renderers under [`OUTPUT_ORDER`]. `count` moves the
/// output count, which a repaint of the open row never does. Returns the
/// count after it.
fn emit_counted<R: tauri::Runtime>(app: &AppHandle<R>, out: &Output, count: bool) -> u64 {
    let payload = OutputPayload::from_output(out);
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
    #[cfg(native_surface)]
    {
        // Word wrapped at the grid width, matching the frontend
        // WordWrapper that xterm receives this same stream through. The
        // grid finds each region in its own rows, as xterm does (D22).
        crate::term_grid::feed_session_output(out);
        crate::native_surface::request_redraw();
    }
    if let Err(e) = app.emit("session://output", payload) {
        warn!(error = %e, "failed to emit session output");
    }
    seen
}

fn emit_state<R: tauri::Runtime>(app: &AppHandle<R>, payload: StatePayload) {
    if let Err(e) = app.emit("session://state", payload) {
        warn!(error = %e, "failed to emit session state");
    }
}

fn emit_input_mode<R: tauri::Runtime>(app: &AppHandle<R>, password: bool) {
    if let Err(e) = app.emit("session://input-mode", InputModePayload { password }) {
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
        let body = vosh_gmcp::build(
            "Core.Supports.Set",
            &super::REQUESTED_GMCP_PACKAGES.to_vec(),
        )
        .expect("the list serializes");
        let msg = vosh_gmcp::parse(&body).expect("the body parses");
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
        assert!(run.lists.aliases);
        assert!(p.aliases.get("greet").is_some());

        let run = super::run_fired_locked(&mut p, "#trigger flee {^You flee} send look", None);
        assert!(run.effects.dirty);
        assert!(run.lists.triggers);

        // A plain command leaves the saved profile alone.
        let run = super::run_fired_locked(&mut p, "greet", None);
        assert_eq!(run.effects, LineEffects::default());
        assert_eq!(run.bytes, b"wave\r\n");
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
        let before = crate::commands::ui_config_generation();
        let _ = super::process_fired_line(&mut p, "#profile reset", &mut effects, None);
        assert!(crate::commands::ui_config_generation() > before);
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

    fn world_time(hour: serde_json::Value) -> vosh_gmcp::Message {
        vosh_gmcp::Message {
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
        let other = vosh_gmcp::Message {
            package: "Char.Vitals".into(),
            data: serde_json::json!({ "hour": 12 }),
        };
        assert!(super::observe_world_time_for_tick(&mut tick, &other, at(80)).is_none());
        let no_hour = vosh_gmcp::Message {
            package: "World.Time".into(),
            data: serde_json::json!({ "sunlight": "light" }),
        };
        assert!(super::observe_world_time_for_tick(&mut tick, &no_hour, at(80)).is_none());
    }

    /// A prompt template in the style of the one in the fixtures, with
    /// colors by how full and the `%)h` trick that prints a percent sign.
    const TEMPLATE: &str = "%{c:100,100,100}[%c_reset%s_italic%hp(%c_hp%pct_hp%c_reset%s_italic%)h %mana(%{c:128,200,255}%pct_mana%c_reset%s_italic%)m %move(%{c:200,255,23}%pct_move%c_reset%s_italic%)v%c_reset%{c:100,100,100}] %c_reset";

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
            vosh_ansi::plain_text(line.as_bytes()),
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
        vosh_ansi::plain_text(ansi.as_bytes())
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
        parser: vosh_telnet::Parser,
        /// The first generation this wire hands out, so tests can name
        /// the marks by number.
        gen0: u64,
    }

    impl Wire {
        fn new(p: Profile) -> Self {
            let mut wire = Self {
                p,
                acc: super::LineAccumulator::new(),
                parser: vosh_telnet::Parser::new(),
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
                let plain = vosh_ansi::plain_text(&line.bytes);
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
                let plain = vosh_ansi::plain_text(&line.bytes);
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
                            let plain = vosh_ansi::plain_text(&line.bytes);
                            let _ =
                                super::line_step(&mut self.p, &mut batch, line, plain, now, None);
                        }
                    }
                    super::TelnetEvent::Subnegotiation { option, payload }
                        if option == super::telnet_option::GMCP =>
                    {
                        let msg = vosh_gmcp::parse(&payload).expect("every packet parses");
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
                            let plain = vosh_ansi::plain_text(&line.bytes);
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
        assert!(step[0].scrollback.is_empty());
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
            })
        );
        assert!(second.bytes.is_empty());
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
            })
        );
        assert!(second.bytes.is_empty());
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
            })
        );
        assert!(second.bytes.is_empty());
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
            .set(vosh_trigger::Trigger {
                name: "prompt-capture".into(),
                patterns: vec![vosh_trigger::TriggerPattern {
                    pattern: CAPTURE.into(),
                    enabled: true,
                }],
                priority: 100,
                enabled: true,
                actions: vec![
                    vosh_trigger::TriggerAction::Gag,
                    vosh_trigger::TriggerAction::Script {
                        body: "mud.set_prompt_var(\"hp\", captures[2])".into(),
                    },
                ],
                preset: None,
                group: None,
                target: vosh_trigger::TriggerTarget::Line,
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
        // and the trigger never sees it (D6).
        let config = vosh_prompt::PromptConfig {
            capture: regex_capture(CAPTURE, false),
            ..wire.p.prompt.config().clone()
        };
        wire.p.set_prompt_config(config);
        let batch = wire.read_with(PROMPT_ROW, false, false);
        assert_eq!(drawn_in(&batch.out.bytes).as_deref(), Some("<1020>\x1b[0m"));
        assert!(batch.gag_without_reader.is_empty());
    }

    #[test]
    fn a_prompts_trigger_acts_on_the_recognized_prompt() {
        let mut p = capture_profile(HP);
        p.set_prompt_config(vosh_prompt::PromptConfig {
            draw: false,
            ..p.prompt.config().clone()
        });
        p.triggers
            .set(vosh_trigger::Trigger {
                name: "mark".into(),
                patterns: vec![vosh_trigger::TriggerPattern {
                    pattern: "hp".into(),
                    enabled: true,
                }],
                priority: 0,
                enabled: true,
                actions: vec![vosh_trigger::TriggerAction::Replace {
                    template: "HP".into(),
                }],
                preset: None,
                group: None,
                target: vosh_trigger::TriggerTarget::Prompt,
            })
            .expect("the trigger compiles");
        let mut wire = Wire::new(p);
        let out = wire.read(PROMPT_ROW);
        assert_eq!(out.bytes, b"[1020/1020HP 800/800mn 930/930mv]\r\n");
    }

    #[test]
    fn line_triggers_that_matched_a_read_prompt_are_noted() {
        let mut p = capture_profile(HP);
        let highlight = |name: &str, pattern: &str, target| vosh_trigger::Trigger {
            name: name.into(),
            patterns: vec![vosh_trigger::TriggerPattern {
                pattern: pattern.into(),
                enabled: true,
            }],
            priority: 0,
            enabled: true,
            actions: vec![vosh_trigger::TriggerAction::Gag],
            preset: None,
            group: None,
            target,
        };
        for trigger in [
            highlight("hp-watch", r"\d+hp", vosh_trigger::TriggerTarget::Line),
            highlight("prompt-look", "hp", vosh_trigger::TriggerTarget::Prompt),
            highlight("hungry", "hungry", vosh_trigger::TriggerTarget::Line),
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
        let _ = wire.read(PROMPT_ROW);
        let spans = |wire: &Wire| -> Vec<(usize, usize, usize)> {
            wire.p
                .prompt
                .stage
                .open_row()
                .map(|o| o.spans.iter().map(|s| (s.piece, s.col, s.width)).collect())
                .unwrap_or_default()
        };
        assert_eq!(spans(&wire), [(0, 0, 1), (1, 1, 9), (2, 10, 2), (3, 12, 3)]);

        // Drawing off shows the game's own line, which has no pieces, and
        // drawing on again brings them back with the repaint.
        let now = tokio::time::Instant::now();
        let config = vosh_prompt::PromptConfig {
            draw: false,
            ..wire.p.prompt.config().clone()
        };
        wire.p.set_prompt_config(config);
        let _ = super::repaint_step(&mut wire.p, false, now);
        assert!(spans(&wire).is_empty());
        let config = vosh_prompt::PromptConfig {
            draw: true,
            template: "[%hp]".into(),
            ..wire.p.prompt.config().clone()
        };
        wire.p.set_prompt_config(config);
        let _ = super::repaint_step(&mut wire.p, false, now);
        assert_eq!(spans(&wire), [(0, 0, 1), (1, 1, 4), (2, 5, 1)]);

        // Other output closes the row, and its pieces go with it.
        let _ = wire.read(b"You are hungry.\n\r");
        assert_eq!(wire.p.prompt.stage.open_row(), None);
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
            })
        );
        assert!(off.bytes.is_empty());

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
    fn only_a_new_window_size_closes_the_open_row() {
        let mut wire = Wire::new(capture_profile(HP));
        let mut negotiator = vosh_telnet::Negotiator::new();
        negotiator.set_window_size(94, 41);
        let _ = wire.read(PROMPT_ROW);
        // The webview sends the size the session already holds on every
        // connect. The row stays open, so turning drawing off repaints it.
        super::window_size_step(&mut wire.p, &mut negotiator, 94, 41);
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

        // A new size closes it, and nothing repaints.
        super::window_size_step(&mut wire.p, &mut negotiator, 94, 40);
        assert_eq!(negotiator.window_size, (94, 40));
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
        let msg = vosh_gmcp::parse(&bytes).unwrap_or_else(|e| panic!("{file}: {e}"));
        super::observe_prompt_gmcp(p, &msg);
    }

    fn feed_inline(p: &mut Profile, package: &str, data: serde_json::Value) {
        super::observe_prompt_gmcp(
            p,
            &vosh_gmcp::Message {
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
            Some(vosh_prompt::vars::Tick {
                remaining: interval,
                interval: Some(interval),
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
    const CODES: &str = "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c";
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
        assert!(out.bytes.is_empty());
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
            .set(vosh_trigger::Trigger {
                name: "answer".into(),
                patterns: vec![vosh_trigger::TriggerPattern {
                    pattern: "^Bob says: ".into(),
                    enabled: true,
                }],
                priority: 0,
                enabled: true,
                actions: vec![vosh_trigger::TriggerAction::Send {
                    template: "nod".into(),
                }],
                preset: None,
                group: None,
                target: vosh_trigger::TriggerTarget::Line,
            })
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
            .set(vosh_trigger::Trigger {
                name: "swap".into(),
                patterns: vec![vosh_trigger::TriggerPattern {
                    pattern: "^Bob says: ".into(),
                    enabled: true,
                }],
                priority: 0,
                enabled: true,
                actions: vec![
                    vosh_trigger::TriggerAction::Gag,
                    vosh_trigger::TriggerAction::Script {
                        body: "mud.echo('Bob speaks.')".into(),
                    },
                ],
                preset: None,
                group: None,
                target: vosh_trigger::TriggerTarget::Line,
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
            .set(vosh_trigger::Trigger {
                name: "hush".into(),
                patterns: vec![vosh_trigger::TriggerPattern {
                    pattern: "^Tester: ".into(),
                    enabled: true,
                }],
                priority: 0,
                enabled: true,
                actions: vec![vosh_trigger::TriggerAction::Gag],
                preset: None,
                group: None,
                target: vosh_trigger::TriggerTarget::Line,
            })
            .unwrap();
        let mut batch = super::ReadBatch::new(super::output_count());
        let now = tokio::time::Instant::now();
        let mut acc = super::LineAccumulator::new();
        let mut steps = Vec::new();
        for line in acc.feed(format!("{TANK_LINE}\n\rYou are hungry.\n\r").as_bytes()) {
            let plain = vosh_ansi::plain_text(&line.bytes);
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
            let plain = vosh_ansi::plain_text(&line.bytes);
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
            let plain = vosh_ansi::plain_text(&line.bytes);
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
            .set(vosh_trigger::Trigger {
                name: "hide-prompt".into(),
                patterns: vec![vosh_trigger::TriggerPattern {
                    pattern: "hp ".into(),
                    enabled: true,
                }],
                priority: 0,
                enabled: true,
                actions: vec![vosh_trigger::TriggerAction::Gag],
                preset: None,
                group: None,
                target: vosh_trigger::TriggerTarget::Prompt,
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
        assert!(batch.out.bytes.is_empty());
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
        let logs: crate::log_state::SharedLogStore =
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
                kind: vosh_prompt::SeenKind::Gmcp,
                text: CODES.into(),
                applied: true,
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

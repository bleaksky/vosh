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
use vosh_telnet::{
    codes as telnet_codes, option as telnet_option, Event as TelnetEvent, Negotiator, Parser,
};
use vosh_trigger::LineResult;

use crate::connection::{self, ConnectionError, Stream};
use crate::gmcp_bind;
use crate::hidden_input::{self, ServerEcho};
use crate::input;
use crate::line_accumulator::{ChunkOp, LineAccumulator};
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
pub(crate) async fn spawn(
    app: AppHandle,
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
        crate::profile_set::is_forsaken_lands(&host),
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
async fn io_loop(
    app: AppHandle,
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

    // Phase 1 audit instrumentation. See `PerfCounters` doc.
    let mut perf = PerfCounters::default();
    let mut perf_report_interval = tokio::time::interval(PERF_REPORT_INTERVAL);
    perf_report_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let disconnect_reason = loop {
        tokio::select! {
            biased;
            outgoing = rx_outgoing.recv() => match outgoing {
                Some(OutgoingMsg::Send { bytes, masked }) => {
                    // The frontend already echoed the typed line inline
                    // with the on-screen prompt. Drop the buffered partial
                    // so the next chunk from the server starts fresh on a
                    // new row instead of merging with the displayed prompt.
                    accumulator.forget_partial();
                    // On a server that sends no Char.Vitals your send
                    // starts the next pulse, after which the values the
                    // last prompt set go stale.
                    if profile.lock().await.prompt.vars.on_send() {
                        emit_hidden_change(&app, &profile).await;
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
                    negotiator.set_window_size(cols, rows);
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
                            &logs,
                            log_session_id,
                            &scrollback,
                            &mut server_echo,
                            event,
                            &mut perf,
                        ).await {
                            warn!(error = %e, "event handling failed");
                            break;
                        }
                    }
                    // Once per read, so the packets of one pulse never
                    // show the panes a state between them.
                    emit_hidden_change(&app, &profile).await;
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
                                for event in events {
                                    if let Err(handle_err) = handle_event(
                                        &app,
                                        &mut stream,
                                        &negotiator,
                                        &mut accumulator,
                                        &profile,
                                        &map,
                                        &timers,
                                        &logs,
                                        log_session_id,
                                        &scrollback,
                                        &mut server_echo,
                                        event,
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
                                emit_hidden_change(&app, &profile).await;
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
    // so it sits in the accumulator as a partial: painted live but never
    // run through the per-line path that logs and scrollback-records it.
    // Flush it now, ahead of the scrollback dump and log close below, so
    // the goodbye is captured like every other client captures it.
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
        end_prompt(&mut p);
        had.then(|| p.target.quick_keys.clone())
    };
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
    // store drops its copy on the disconnected state below.
    app.state::<crate::commands::SharedState>()
        .last_affects
        .clear();
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

async fn handle_tick(
    app: &AppHandle,
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
    deliver_tick_step(app, stream, profile, step).await
}

/// Report a tick step on `session://tick`, so the frontend counts and
/// plays the sound when it fired, then run its Send each tick command
/// through the full input pipeline like a timer command.
async fn deliver_tick_step(
    app: &AppHandle,
    stream: &mut Stream,
    profile: &Arc<Mutex<Profile>>,
    step: TickStep,
) -> std::io::Result<()> {
    if let Err(e) = app.emit("session://tick", &step.payload) {
        warn!(error = %e, "failed to emit tick payload");
    }
    if let Some(command) = step.command {
        run_fired_command(app, stream, profile, &command).await?;
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
async fn fire_due_profile_timers(
    app: &AppHandle,
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
        run_fired_command(app, stream, profile, &command).await?;
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
async fn run_fired_command(
    app: &AppHandle,
    stream: &mut Stream,
    profile: &Arc<Mutex<Profile>>,
    command: &str,
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
        let mut buf = Vec::new();
        for line in &echoes {
            buf.extend_from_slice(b"\r\n");
            buf.extend_from_slice(line.as_bytes());
        }
        buf.extend_from_slice(b"\r\n");
        emit_output(app, buf);
    }
    if !bytes.is_empty() {
        stream.write_all(&bytes).await?;
        stream.flush().await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn handle_event(
    app: &AppHandle,
    stream: &mut Stream,
    negotiator: &Negotiator,
    accumulator: &mut LineAccumulator,
    profile: &Arc<Mutex<Profile>>,
    map: &SharedMap,
    timers: &SharedTimers,
    logs: &crate::log_state::SharedLogStore,
    log_session_id: Option<i64>,
    scrollback: &crate::log_state::SharedScrollback,
    server_echo: &mut ServerEcho,
    event: TelnetEvent,
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
            // Batch every byte we want to push to xterm across all
            // ChunkOps from this single Data event into one buffer so
            // we emit one `session://output` instead of one-per-line.
            // Tauri events serialize through the bridge and xterm
            // renders each write on its own frame; without batching a
            // 50-line response paints line-by-line ("typewriter") at
            // the speed of Tauri event delivery + xterm framing.
            //
            // Triggers, Lua callbacks, route emissions, log writes,
            // and tick-reset bookkeeping still run per-line because
            // they have ordering semantics (a `gag` action mutates the
            // line's display before it lands in the batch).
            let mut display_batch: Vec<u8> = Vec::new();
            // Log rows for this socket read, flushed in one batched
            // transaction after the loop instead of one INSERT + one
            // lock acquisition per line.
            let mut log_entries: Vec<vosh_log::LogEntry> = Vec::new();
            for op in accumulator.feed(&bytes) {
                match op {
                    ChunkOp::RawDisplay(b) => {
                        display_batch.extend_from_slice(&b);
                    }
                    ChunkOp::LineComplete { bytes, clear_first } => {
                        perf.lines_processed += 1;
                        let plain = vosh_ansi::plain_text(&bytes);
                        let trigger_t0 = std::time::Instant::now();
                        // Phase 5 perf fix: take the tick step for a line
                        // that matches the Reset on pattern under the same
                        // lock as trigger/Lua matching so we never
                        // reacquire `profile` later just to read the tick.
                        // The line is the game's tick, so the step fires
                        // once per tick and carries the Send each tick
                        // command to run after the lock drops.
                        let LinePass {
                            result,
                            tick_step,
                            apply: script_apply,
                            rendered_prompt,
                        } = {
                            let lock_t0 = std::time::Instant::now();
                            let mut p = profile.lock().await;
                            perf.mutex_wait_ns += lock_t0.elapsed().as_nanos() as u64;
                            perf.mutex_acquires += 1;
                            line_pass(&mut p, &bytes, &plain, Instant::now())
                        };
                        perf.trigger_lua_ns += trigger_t0.elapsed().as_nanos() as u64;
                        // In-place echo replacement. When a trigger gags the
                        // line AND its Script action emits one or more
                        // `mud.echo(...)` outputs, drain those echoes into
                        // the display batch right where the gagged line
                        // would have rendered. Without this, the echoes
                        // fall through to `apply_script_result` which
                        // frames each echo with leading and trailing
                        // `\r\n` for non-trigger contexts (timers, async
                        // scripts) — and against a gagged prompt that
                        // reads as a blank row followed by the echo on a
                        // new line. Replacement makes the echo land at
                        // the exact row of the gagged content, which is
                        // what users mean by "the trigger replaced the
                        // prompt with my echo."
                        let mut script_apply = script_apply;
                        if result.display.is_none() && !script_apply.echoes.is_empty() {
                            for line in script_apply.echoes.drain(..) {
                                display_batch.extend_from_slice(line.as_bytes());
                                display_batch.extend_from_slice(b"\r\n");
                            }
                        }
                        append_line_result(&mut display_batch, &result, clear_first);
                        // The custom prompt renders on the gagged prompt's
                        // row, in the same batch — no erased-row flash. No
                        // trailing newline: the cursor sits after the prompt
                        // like a real MUD prompt (and the typed-command echo
                        // lands beside it), matching the old frontend render.
                        // Placed after append_line_result so a clear_first
                        // wipe cannot erase it.
                        if let Some(rendered) = &rendered_prompt {
                            display_batch.extend_from_slice(rendered.as_bytes());
                        }
                        if !result.routes.is_empty() {
                            perf.routed_emits += result.routes.len() as u64;
                        }
                        emit_line_routes(app, &result);
                        if let Some(text) = &result.display {
                            // Collect the searchable SQLite log row (flushed
                            // in one transaction after the loop) and push the
                            // ring buffer that becomes scrollback on next
                            // launch. The raw bytes carry ANSI; the plain
                            // text column drives the regex search. `plain`
                            // and `bytes` are not used past this point, so
                            // they move into the entry instead of cloning.
                            if let Some(sid) = log_session_id {
                                log_entries.push(vosh_log::LogEntry {
                                    session_id: sid,
                                    ts_ms: now_ms(),
                                    text: plain,
                                    raw: Some(bytes),
                                });
                            }
                            let sb_t0 = std::time::Instant::now();
                            scrollback.lock().await.push(text.as_bytes().to_vec());
                            perf.scrollback_push_ns += sb_t0.elapsed().as_nanos() as u64;
                            perf.scrollback_pushes += 1;
                        }
                        send_trigger_outputs(stream, &result.sends).await?;
                        apply_script_result(app, stream, profile, timers, script_apply).await?;
                        if let Some(step) = tick_step {
                            deliver_tick_step(app, stream, profile, step).await?;
                        }
                    }
                }
            }
            if !display_batch.is_empty() {
                perf.output_emits += 1;
                perf.output_emit_bytes += display_batch.len() as u64;
                emit_output(app, display_batch);
            }
            // Flush this read's log rows in one transaction under one
            // lock acquisition, instead of per line inside the loop.
            if !log_entries.is_empty() {
                let lock_t0 = std::time::Instant::now();
                let mut guard = logs.lock().await;
                perf.mutex_wait_ns += lock_t0.elapsed().as_nanos() as u64;
                perf.mutex_acquires += 1;
                if let Some(store) = guard.as_mut() {
                    let append_t0 = std::time::Instant::now();
                    perf.log_appends += log_entries.len() as u64;
                    if let Err(e) = store.append_batch(&log_entries) {
                        warn!(error = %e, "log append_batch failed");
                    }
                    perf.log_append_ns += append_t0.elapsed().as_nanos() as u64;
                }
            }
            Ok(())
        }
        TelnetEvent::Subnegotiation { option, payload } if option == telnet_option::GMCP => {
            perf.gmcp_packets += 1;
            handle_gmcp(app, profile, map, timers, stream, &payload, perf).await?;
            Ok(())
        }
        TelnetEvent::Command(byte) if byte == telnet_codes::EOR || byte == telnet_codes::GA => {
            // The server marked the end of a prompt. Two jobs:
            //
            // 1. Run any `target=prompt` triggers against the partial
            //    buffer so `#prompt`-style triggers can gag the
            //    prompt and capture vars via `mud.set_prompt_var`.
            // 2. Drop the cursor onto its own line so the next
            //    complete line lands cleanly below the prompt
            //    instead of merging into it.
            //
            // The trigger pass uses the same `process_scoped` engine
            // as the line pass, so all action kinds (Gag, Replace,
            // Highlight, Send, Route, Script) work identically.
            // Gag here means the on-screen partial is erased with
            // `ESC[2K\r` before the trailing newline ends the row.
            dispatch_prompt_buffer(app, profile, stream, timers, accumulator).await?;
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

/// What the line pass decided for one complete line.
struct LinePass {
    result: LineResult,
    /// The tick the line reset, with its Send each tick command.
    tick_step: Option<TickStep>,
    apply: ApplyResult,
    /// The custom prompt to draw where the gagged line was.
    rendered_prompt: Option<String>,
}

/// Run one complete line through Line triggers, the tick reset pattern,
/// Lua triggers and the Script bodies the triggers queued, all under the
/// profile lock the caller holds. `plain` is the line without ANSI, so
/// no pattern has to allow for escape bytes and the line is stripped
/// once.
///
/// A gagged line whose script set prompt vars is the prompt. The custom
/// template renders here, under the same lock, so the replacement lands
/// in the same display batch exactly where the gagged prompt was.
/// Rendering used to happen in the frontend off the prompt-vars event,
/// which flashed a blank row on every prompt.
fn line_pass(p: &mut Profile, bytes: &[u8], plain: &str, now: Instant) -> LinePass {
    let result =
        vosh_trigger::process_with_plain(&p.triggers, bytes, plain, vosh_trigger::MatchScope::Line);
    let tick_step = if p.tick.check_reset_match(plain) {
        p.tick.on_game_tick(now)
    } else {
        None
    };
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
    for call in &result.scripts {
        match script_state::eval_with_captures(
            &mut p.script,
            &call.body,
            &call.captures,
            "trigger-script",
        ) {
            Ok(o) => outcome.actions.extend(o.actions),
            Err(err) => {
                warn!(error = %err, "trigger script eval failed");
            }
        }
    }
    let apply = script_state::apply_actions(p, outcome);
    let rendered_prompt = if result.display.is_none() && apply.prompt_vars_changed {
        render_custom_prompt(p)
    } else {
        None
    };
    LinePass {
        result,
        tick_step,
        apply,
        rendered_prompt,
    }
}

/// What Prompt triggers did to the partial a GA or EOR ended.
struct PromptPass {
    result: LineResult,
    apply: ApplyResult,
    /// The custom prompt to draw in place of the gagged partial.
    rendered_prompt: Option<String>,
}

/// Run Prompt triggers over the partial a GA or EOR ended, under the
/// profile lock the caller holds. None when no trigger changed the
/// output, sent, routed or ran a script. The engine hands back the line
/// as it was when nothing matched, so a byte for byte check tells no
/// effect from a highlight or a replace. Without it every GA would erase
/// and paint the prompt again, one extra row per prompt.
fn prompt_pass(p: &mut Profile, bytes: &[u8], already_shown: bool) -> Option<PromptPass> {
    let result = vosh_trigger::process_scoped(&p.triggers, bytes, vosh_trigger::MatchScope::Prompt);
    let changed_output = match &result.display {
        None => true,
        Some(text) => text.as_bytes() != bytes,
    };
    if !changed_output
        && result.sends.is_empty()
        && result.routes.is_empty()
        && result.scripts.is_empty()
    {
        return None;
    }
    // A trigger fired. Its Script bodies run the way the line pass runs
    // them, so `mud.set_prompt_var` lands before the render.
    script_state::snapshot_vars(&p.script, &p.vars);
    let mut outcome = vosh_script::ScriptOutcome::default();
    for call in &result.scripts {
        match script_state::eval_with_captures(
            &mut p.script,
            &call.body,
            &call.captures,
            "prompt-trigger-script",
        ) {
            Ok(o) => outcome.actions.extend(o.actions),
            Err(err) => {
                warn!(error = %err, "prompt-trigger script eval failed");
            }
        }
    }
    let apply = script_state::apply_actions(p, outcome);
    // Render while the lock is held, so the erase and the replacement
    // leave in one output batch. An IPC round trip between them showed
    // the blank erased row for a frame on every prompt.
    let rendered_prompt = if result.display.is_none() {
        let rendered = render_custom_prompt(p);
        tracing::debug!(
            template_len = p.prompt.config().template.len(),
            rendered_len = rendered.as_ref().map_or(0, String::len),
            already_shown,
            "prompt: template render"
        );
        rendered
    } else {
        None
    };
    Some(PromptPass {
        result,
        apply,
        rendered_prompt,
    })
}

/// The custom prompt drawn from the live values, or None while drawing
/// is off or the template draws nothing. The vosh-prompt resolver reads
/// the values triggers set, then the latest GMCP packets, then what Vosh
/// itself knows, and draws `?` for a value the game hides.
fn render_custom_prompt(p: &Profile) -> Option<String> {
    let config = p.prompt.config();
    if !config.draw {
        return None;
    }
    let vosh = prompt_supplies(p, Instant::now());
    let rendered = vosh_prompt::render_str(
        &config.template,
        &p.prompt.vars.resolver(&vosh),
        vosh_prompt::RenderOptions::default(),
    );
    (!rendered.ansi.is_empty()).then_some(rendered.ansi)
}

/// What Vosh itself supplies to the custom prompt: the tick timer, your
/// target, the profile's name and the affects you track. The clock reads
/// the local time.
fn prompt_supplies(p: &Profile, now: Instant) -> vosh_prompt::Vosh {
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
    p.prompt.vars.observe(
        &msg.package,
        msg.data.clone(),
        chrono::Local::now().fixed_offset(),
    );
}

/// Tell the webview which values the game hides, when that changed
/// since the last report. The session calls it once per socket read and
/// after a send that starts a pulse.
async fn emit_hidden_change(app: &AppHandle, profile: &Arc<Mutex<Profile>>) {
    let change = profile.lock().await.prompt.vars.take_hidden_change();
    if let Some(hidden) = change {
        if let Err(e) = app.emit("session://hidden", hidden) {
            warn!(error = %e, "failed to emit the hidden state");
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_gmcp(
    app: &AppHandle,
    profile: &Arc<Mutex<Profile>>,
    map: &SharedMap,
    timers: &SharedTimers,
    stream: &mut Stream,
    payload: &[u8],
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
        gmcp_bind::apply(&mut p.vars, &msg);
        // Before Lua, so a value a GMCP handler sets with
        // `mud.set_prompt_var` belongs to the pulse this packet starts.
        observe_prompt_gmcp(&mut p, &msg);
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
                            Some(serde_json::Value::Number(n)) => {
                                n.as_i64().is_some_and(|x| x != 0)
                            }
                            _ => false,
                        };
                        Some(crate::profile::RoomChar { name, npc })
                    })
                    .collect();
                crate::input::set_room_chars(&mut p, chars);
            }
        }
        let tick_step = observe_world_time_for_tick(&mut p.tick, &msg, Instant::now());
        script_state::snapshot_vars(&p.script, &p.vars);
        let outcome = match p.script.dispatch_gmcp(&msg.package, &msg.data) {
            Ok(o) => o,
            Err(err) => {
                warn!(error = %err, "lua dispatch_gmcp failed");
                vosh_script::ScriptOutcome::default()
            }
        };
        let apply = script_state::apply_actions(&mut p, outcome);
        (tick_step, apply)
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
                let state = app.state::<crate::commands::SharedState>();
                crate::commands::handle_char_known_for_auto_switch(app, state.inner(), &owned)
                    .await;
            }
        }
    }
    if let Some(step) = tick_step {
        perf.tick_emits += 1;
        deliver_tick_step(app, stream, profile, step).await?;
    }
    apply_script_result(app, stream, profile, timers, script_apply).await?;
    if msg.package == "Room.Info" {
        // Map-store SQLite writes ride a dedicated single-consumer task
        // (ordering preserved) instead of running inline on the io loop,
        // where they sat between a socket read and the next outgoing
        // command write and contributed to command latency.
        let _ = map_writer(app, map).send(msg.clone());
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
    let event_name = format!("session://gmcp/{}", msg.package.replace('.', "-"));
    if let Err(e) = app.emit(&event_name, &msg.data) {
        warn!(error = %e, package = %msg.package, "failed to emit GMCP event");
    }
    // `perf.gmcp_packets` already incremented by the caller before
    // we ran. This `emit` count would otherwise duplicate that, so
    // we leave gmcp_packets as the single source.
    Ok(())
}

/// Lazily-started single-consumer task that applies `Room.Info` map
/// updates off the session io loop. One consumer preserves room-visit
/// ordering (spawn-per-message would not).
static MAP_WRITER: std::sync::OnceLock<tokio::sync::mpsc::UnboundedSender<vosh_gmcp::Message>> =
    std::sync::OnceLock::new();

fn map_writer(
    app: &AppHandle,
    map: &crate::map_state::SharedMap,
) -> &'static tokio::sync::mpsc::UnboundedSender<vosh_gmcp::Message> {
    MAP_WRITER.get_or_init(|| {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<vosh_gmcp::Message>();
        let app = app.clone();
        let map = map.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if let Err(e) = map_state::handle_room_info(&app, &map, &msg).await {
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

/// Append this line's display bytes to a per-Data-event batch. The
/// caller drains the batch with a single `emit_output` at the end of
/// the for loop so a multi-line response paints in one xterm.write.
fn append_line_result(batch: &mut Vec<u8>, result: &LineResult, clear_first: bool) {
    if clear_first {
        // Wipe the partial that was already shown raw so the trigger-
        // processed line replaces it cleanly. ESC [ 2 K clears the entire
        // line, then \r returns the cursor to column zero.
        batch.extend_from_slice(b"\x1b[2K\r");
    }
    if let Some(text) = &result.display {
        batch.extend_from_slice(text.as_bytes());
        batch.extend_from_slice(b"\r\n");
    }
}

/// Route emissions stay per-line because consumers (chat panel etc.)
/// expect one event per routed line. The volume here is tiny relative
/// to the display stream so per-event cost does not show up as lag.
fn emit_line_routes(app: &AppHandle, result: &LineResult) {
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
async fn apply_script_result(
    app: &AppHandle,
    stream: &mut Stream,
    profile: &Arc<Mutex<Profile>>,
    timers: &SharedTimers,
    apply: ApplyResult,
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
        let mut buf = Vec::new();
        for line in &apply.echoes {
            buf.extend_from_slice(b"\r\n");
            buf.extend_from_slice(line.as_bytes());
        }
        buf.extend_from_slice(b"\r\n");
        emit_output(app, buf);
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
            let mut buf = Vec::new();
            for line in &input_echoes {
                buf.extend_from_slice(b"\r\n");
                buf.extend_from_slice(line.as_bytes());
            }
            buf.extend_from_slice(b"\r\n");
            emit_output(app, buf);
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
        emit_prompt_vars(app, profile).await;
    }
    Ok(())
}

/// Push the prompt vars to the frontend as a single snapshot, the values
/// triggers set this pulse and those only scripts supply, with a value
/// the game hides as `?`. Re-emitted whenever a Lua action sets one and
/// after every prompt trigger. The vitals store replaces its copy with
/// the payload, so a value that went stale or was unset drops out.
async fn emit_prompt_vars(app: &AppHandle, profile: &Arc<Mutex<Profile>>) {
    let snapshot: BTreeMap<String, String> = profile.lock().await.prompt.vars.prompt_vars();
    if let Err(e) = app.emit("session://prompt-vars", &snapshot) {
        warn!(error = %e, "failed to emit prompt vars");
    }
}

async fn fire_due_script_timers(
    app: &AppHandle,
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
    apply_script_result(app, stream, profile, timers, apply).await
}

/// Run `target=prompt` triggers against the partial-prompt buffer the
/// telnet parser just flushed (GA / EOR / idle timeout). Handles:
///   * Gag — erases the on-screen partial with `ESC[2K\r` so the
///     prompt does not stay visible after the trigger consumes it.
///   * Replace — re-emits the substituted text in place of the
///     original partial.
///   * Script — evaluates Lua bodies with the regex captures, the
///     primary mechanism for `mud.set_prompt_var(...)` to populate
///     vitals from a parsed prompt.
///   * Send / Route — same semantics as the line pass.
///
/// When no prompt trigger fires (display matches original), the only
/// observable effect is the trailing `\r\n` that drops the cursor
/// onto the next line — same as the legacy partial-flush behavior.
/// Flush a partial line still buffered when the session ends so the MUD's
/// final output (a logout banner on `quit`, most often) is captured rather
/// than discarded by the disconnect `accumulator.reset()`. The partial was
/// already painted live as it streamed in, so display only needs the
/// terminating newline; the value of this pass is logging it and pushing it
/// into the scrollback ring that the dump persists.
async fn capture_pending_line(
    app: &AppHandle,
    logs: &crate::log_state::SharedLogStore,
    log_session_id: Option<i64>,
    scrollback: &crate::log_state::SharedScrollback,
    accumulator: &mut LineAccumulator,
) {
    let Some((bytes, already_shown)) = accumulator.flush_partial() else {
        return;
    };
    let plain = vosh_ansi::plain_text(&bytes);
    // Terminate the line on screen. Re-emit the bytes only in the unlikely
    // case they were never shown raw, to avoid printing the goodbye twice.
    let mut out = Vec::with_capacity(bytes.len() + 2);
    if !already_shown {
        out.extend_from_slice(&bytes);
    }
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

async fn dispatch_prompt_buffer(
    app: &AppHandle,
    profile: &Arc<Mutex<Profile>>,
    stream: &mut Stream,
    timers: &SharedTimers,
    accumulator: &mut LineAccumulator,
) -> std::io::Result<()> {
    let Some((bytes, already_shown)) = accumulator.flush_partial() else {
        return Ok(());
    };
    let pass = {
        let mut p = profile.lock().await;
        prompt_pass(&mut p, &bytes, already_shown)
    };
    let Some(PromptPass {
        result,
        apply: script_apply,
        rendered_prompt,
    }) = pass
    else {
        // No prompt trigger changed anything. End the row the partial
        // was painted on, as the legacy partial flush did.
        if already_shown {
            emit_output(app, b"\r\n".to_vec());
        }
        return Ok(());
    };
    // Repaint the visible partial in ONE output batch: erase, then the
    // replacement. Splitting these across emits (or rendering the prompt in
    // the frontend, as before) shows the blank erased row for a frame.
    let mut script_apply = script_apply;
    let mut out = Vec::new();
    if already_shown {
        out.extend_from_slice(b"\x1b[2K\r");
    }
    if let Some(text) = &result.display {
        // No `\r\n` before the text — the line was just cleared.
        out.extend_from_slice(text.as_bytes());
        out.extend_from_slice(b"\r\n");
    } else {
        // Gagged. Script echoes render where the prompt was (matching the
        // line-pipeline replacement semantics; drained so
        // `apply_script_result` does not also frame them with newlines),
        // then the custom prompt lands as a partial on the erased row —
        // the cursor stays after it, like a real MUD prompt.
        let echoed = !script_apply.echoes.is_empty();
        for line in script_apply.echoes.drain(..) {
            out.extend_from_slice(line.as_bytes());
            out.extend_from_slice(b"\r\n");
        }
        if let Some(rendered) = &rendered_prompt {
            out.extend_from_slice(rendered.as_bytes());
        } else if already_shown && !echoed {
            // Gag with no replacement at all: just terminate the
            // (now-blank) line so the next chunk does not paint into the
            // cleared row.
            out.extend_from_slice(b"\r\n");
        }
    }
    if !out.is_empty() {
        emit_output(app, out);
    }
    send_trigger_outputs(stream, &result.sends).await?;
    // Always emit prompt-vars after a prompt-scope trigger has
    // had effect, even when none of the captured values changed.
    // The frontend's custom prompt renderer needs to re-paint on
    // every prompt as a "server is ready for input" indicator;
    // gating on value-change suppresses the emit when the player
    // is at full vitals and the prompt repeats unchanged. Clear
    // the apply flag so apply_script_result doesn't double-emit
    // when values DID change.
    emit_prompt_vars(app, profile).await;
    script_apply.prompt_vars_changed = false;
    apply_script_result(app, stream, profile, timers, script_apply).await
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
fn base64_encode(input: &[u8]) -> String {
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
    /// Build the wire payload from raw output bytes (base64-encoded).
    pub(crate) fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            b64: base64_encode(bytes),
        }
    }
}

/// Held across both halves of [`emit_output`], so the native grid and
/// xterm take the output of every caller in the same order.
static OUTPUT_ORDER: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Print `bytes` in the terminal. Every write to the terminal pane goes
/// through here: the session loop, a slash command's echo, the `#logs`
/// reply, and the rest. Nothing else emits `session://output`.
pub(crate) fn emit_output<R: tauri::Runtime>(app: &AppHandle<R>, bytes: Vec<u8>) {
    let payload = OutputPayload::from_bytes(&bytes);
    // The session loop and the command handlers write from different
    // tasks. Without the lock, two writes could reach the grid in one
    // order and xterm in the other.
    let _order = OUTPUT_ORDER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // Tier 3: feed the native terminal grid the same bytes xterm receives,
    // for every output path (line pipeline AND the prompt pipeline's gag
    // clearing / replaced text / echoes), then repaint. This is the single
    // choke point so nothing reaches xterm without also reaching the grid.
    #[cfg(native_surface)]
    {
        // Word-wrapped at the grid width, matching the frontend WordWrapper
        // that xterm receives this same stream through.
        crate::term_grid::feed_session_bytes(&bytes);
        crate::native_surface::request_redraw();
    }
    if let Err(e) = app.emit("session://output", payload) {
        warn!(error = %e, "failed to emit session output");
    }
}

fn emit_state(app: &AppHandle, payload: StatePayload) {
    if let Err(e) = app.emit("session://state", payload) {
        warn!(error = %e, "failed to emit session state");
    }
}

fn emit_input_mode(app: &AppHandle, password: bool) {
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

    /// A profile with the prompt capture `#prompt` writes, a Line
    /// trigger that gags the prompt and hands each group to
    /// `mud.set_prompt_var`, and the custom prompt drawn from `template`.
    fn capture_profile(template: &str) -> Profile {
        let mut p = Profile::default();
        let ran = crate::input::run_line(
            &mut p,
            r"#prompt {\[(?<hp>\d+)/(?<maxhp>\d+)hp (?<mana>\d+)/(?<maxmana>\d+)mn (?<move>\d+)/(?<maxmove>\d+)mv\]}",
        );
        assert!(
            ran.result.echo[0].starts_with("prompt-capture trigger set"),
            "{:?}",
            ran.result.echo
        );
        p.set_prompt_config(vosh_prompt::PromptConfig::from_legacy(true, template));
        p
    }

    fn pass_line(p: &mut Profile, line: &str) -> super::LinePass {
        super::line_pass(
            p,
            line.as_bytes(),
            &vosh_ansi::plain_text(line.as_bytes()),
            tokio::time::Instant::now(),
        )
    }

    fn plain(ansi: &str) -> String {
        vosh_ansi::plain_text(ansi.as_bytes())
    }

    #[test]
    fn the_line_pass_draws_the_custom_prompt_in_place_of_the_capture() {
        let mut p = capture_profile(TEMPLATE);
        let pass = pass_line(&mut p, PROMPT_LINE);
        assert!(pass.result.display.is_none(), "the capture gags the prompt");
        assert!(pass.apply.prompt_vars_changed);
        let drawn = pass.rendered_prompt.expect("the prompt draws");
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
        let pass = pass_line(&mut p, "You are hungry.");
        assert_eq!(pass.result.display.as_deref(), Some("You are hungry."));
        assert!(pass.rendered_prompt.is_none());

        // With drawing off the capture still gags, and nothing draws.
        p.set_prompt_config(vosh_prompt::PromptConfig::from_legacy(false, TEMPLATE));
        let pass = pass_line(&mut p, PROMPT_LINE);
        assert!(pass.result.display.is_none());
        assert!(pass.rendered_prompt.is_none());
    }

    #[test]
    fn the_prompt_pass_draws_over_a_partial_a_prompt_trigger_gags() {
        let mut p = capture_profile(TEMPLATE);
        // A partial no Prompt trigger reads has no effect.
        assert!(super::prompt_pass(&mut p, PROMPT_LINE.as_bytes(), true).is_none());
        // The same capture aimed at the partial a GA ends.
        let mut capture = p
            .triggers
            .get("prompt-capture")
            .expect("the capture")
            .clone();
        capture.target = vosh_trigger::TriggerTarget::Prompt;
        p.triggers.set(capture).expect("the capture compiles");
        let pass = super::prompt_pass(&mut p, PROMPT_LINE.as_bytes(), true).expect("it fires");
        assert!(pass.result.display.is_none());
        let drawn = pass.rendered_prompt.expect("the prompt draws");
        assert_eq!(plain(&drawn), "[1020(100%)h 800(100%)m 930(100%)v] ");
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
            let pass = pass_line(&mut p, "[0/0hp 0/0mn 0/0mv]");
            assert!(pass.result.display.is_none(), "{name}");
            let drawn = pass.rendered_prompt.expect("the prompt draws");
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
        let pass = pass_line(&mut p, "[850/900hp 760/820mn 250/250mv]");
        let drawn = pass.rendered_prompt.expect("the prompt draws");
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
        let pass = pass_line(&mut p, "[0/0hp 0/0mn 0/0mv]");
        let drawn = pass.rendered_prompt.expect("the prompt draws");
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
        let pass = pass_line(&mut p, PROMPT_LINE);
        assert_eq!(
            plain(&pass.rendered_prompt.expect("the prompt draws")),
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
        let pass = pass_line(&mut p, PROMPT_LINE);
        assert_eq!(
            plain(&pass.rendered_prompt.expect("the prompt draws")),
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
        let pass = pass_line(&mut p, PROMPT_LINE);
        assert_eq!(plain(&pass.rendered_prompt.expect("it draws")), "[]");
        feed(&mut p, "char-prompt.gmcp");
        let pass = pass_line(&mut p, PROMPT_LINE);
        assert_eq!(plain(&pass.rendered_prompt.expect("it draws")), "[S]");
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
        let pass = pass_line(&mut p, PROMPT_LINE);
        let drawn = plain(&pass.rendered_prompt.expect("it draws"));
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
        let _ = pass_line(&mut p, PROMPT_LINE);
        assert!(!p.prompt.vars.prompt_vars().is_empty());

        super::end_prompt(&mut p);
        assert!(!p.prompt.vars.new_build());
        assert!(p.prompt.vars.prompt_vars().is_empty());
        assert!(p.prompt.vars.gmcp().get("Char.Worth").is_none());
        // The hidden state that ended with the connection is never
        // reported, since the stores clear on the disconnect.
        assert!(p.prompt.vars.take_hidden_change().is_none());

        super::start_prompt(&mut p, false);
        assert!(!p.prompt.forsaken());
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

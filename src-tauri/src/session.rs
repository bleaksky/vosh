//! Per-session task. Wires the connection, the telnet parser, the line
//! accumulator, and the trigger engine together. Emits Tauri events.

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
use crate::input;
use crate::line_accumulator::{ChunkOp, LineAccumulator};
use crate::map_state::{self, SharedMap};
use crate::profile::Profile;
use crate::script_state::{self, ApplyResult, PendingTimer, SharedTimers};
use crate::tick::{TickPayload, TickRuntime};

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
    pub bytes: Vec<u8>,
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
/// mode. More packages land alongside the script engine in Phase 8.
const REQUESTED_GMCP_PACKAGES: &[&str] = &["Char 1", "Room 1", "Comm 1", "World 1", "Map 1"];

/// Bytes flowing to the server. The frontend echoes typed commands into the
/// terminal pane synchronously, so the `io_loop` only writes to the wire.
pub(crate) enum OutgoingMsg {
    Send(Vec<u8>),
    /// Update the advertised terminal size and, if NAWS has already
    /// been negotiated, push a fresh NAWS subnegotiation to the wire
    /// so the server re-wraps its output at the new width.
    WindowSize {
        cols: u16,
        rows: u16,
    },
}

pub(crate) struct SessionHandle {
    tx_outgoing: mpsc::UnboundedSender<OutgoingMsg>,
    task: JoinHandle<()>,
}

impl SessionHandle {
    /// Send raw bytes to the connection. Returns false when the session has
    /// already been torn down.
    pub(crate) fn send(&self, bytes: Vec<u8>) -> bool {
        self.tx_outgoing.send(OutgoingMsg::Send(bytes)).is_ok()
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

    // Activate the tick timer for this session. The user can disable it
    // later through the slash command.
    {
        let mut p = profile.lock().await;
        p.tick.enable(Instant::now());
    }

    let mut tick_interval = tokio::time::interval(TICK_EMIT_INTERVAL);
    tick_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Phase 1 audit instrumentation. See `PerfCounters` doc.
    let mut perf = PerfCounters::default();
    let mut perf_report_interval = tokio::time::interval(PERF_REPORT_INTERVAL);
    perf_report_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let disconnect_reason = loop {
        tokio::select! {
            biased;
            outgoing = rx_outgoing.recv() => match outgoing {
                Some(OutgoingMsg::Send(bytes)) => {
                    // The frontend already echoed the typed line inline
                    // with the on-screen prompt. Drop the buffered partial
                    // so the next chunk from the server starts fresh on a
                    // new row instead of merging with the displayed prompt.
                    accumulator.forget_partial();
                    // Append the input line(s) to the same log session
                    // as server output so transcripts include both
                    // directions. The wire payload is one or more
                    // commands terminated by `\r\n`; split on those
                    // boundaries, prefix each line with `> ` so a
                    // future viewer can distinguish input from output
                    // at a glance, and skip empty lines (a bare Enter
                    // shows up here as `\r\n` with no command body).
                    if let Some(sid) = log_session_id {
                        let text = String::from_utf8_lossy(&bytes);
                        let mut to_append: Vec<String> = Vec::new();
                        for raw in text.split('\n') {
                            let line = raw.trim_end_matches('\r').trim_end();
                            if line.is_empty() {
                                continue;
                            }
                            to_append.push(format!("> {line}"));
                        }
                        if !to_append.is_empty() {
                            let mut guard = logs.lock().await;
                            if let Some(store) = guard.as_mut() {
                                let ts = now_ms();
                                for line in &to_append {
                                    if let Err(e) = store.append(sid, ts, line, None) {
                                        warn!(error = %e, "log append (input) failed");
                                    }
                                }
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
                            event,
                            &mut perf,
                        ).await {
                            warn!(error = %e, "event handling failed");
                            break;
                        }
                    }
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
                perf.tick_emits += 1;
            }
            _ = perf_report_interval.tick() => {
                perf.report_and_reset();
            }
        }
    };

    {
        let mut p = profile.lock().await;
        p.tick.disable();
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
    let now = Instant::now();
    // Take the firing decision under the lock. If the timer fired, capture
    // the auto-fire command (if any) so we can run it after releasing the
    // lock.
    let (payload, auto_fire, warn_echo) = {
        let mut p = profile.lock().await;
        let fired = p.tick.try_consume_fire(now);
        let warned = p.tick.try_consume_warn(now);
        let payload = TickPayload::from_runtime(&p.tick, now, fired);
        let auto_fire = if fired {
            p.tick.config.auto_fire.clone()
        } else {
            None
        };
        let warn_echo = if warned {
            let message = p.tick.config.warn_message.clone().unwrap_or_else(|| {
                match p.tick.config.warn_at_secs {
                    Some(s) => format!("TICK IN {s}s"),
                    None => "TICK INCOMING".to_string(),
                }
            });
            let color = crate::tick::warn_color_escape(p.tick.config.warn_color.as_deref());
            Some(format!("\r\n{color}{message}\x1b[0m\r\n"))
        } else {
            None
        };
        (payload, auto_fire, warn_echo)
    };

    if !payload.enabled && !payload.fired {
        return Ok(());
    }

    if let Some(text) = warn_echo {
        emit_output(app, text.into_bytes());
    }

    if let Err(e) = app.emit("session://tick", &payload) {
        warn!(error = %e, "failed to emit tick payload");
    }

    if let Some(command) = auto_fire {
        let result = {
            let mut p = profile.lock().await;
            input::process(&mut p, &command)
        };
        if !result.echo.is_empty() {
            let mut buf = Vec::new();
            for line in &result.echo {
                buf.extend_from_slice(b"\r\n");
                buf.extend_from_slice(line.as_bytes());
            }
            buf.extend_from_slice(b"\r\n");
            emit_output(app, buf);
        }
        if !result.bytes.is_empty() {
            stream.write_all(&result.bytes).await?;
            stream.flush().await?;
        }
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
    event: TelnetEvent,
    perf: &mut PerfCounters,
) -> std::io::Result<()> {
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
            for op in accumulator.feed(&bytes) {
                match op {
                    ChunkOp::RawDisplay(b) => {
                        display_batch.extend_from_slice(&b);
                    }
                    ChunkOp::LineComplete { bytes, clear_first } => {
                        perf.lines_processed += 1;
                        let plain = vosh_ansi::plain_text(&bytes);
                        let trigger_t0 = std::time::Instant::now();
                        // Phase 5 perf fix: build the tick-reset payload
                        // under the same lock as trigger/Lua matching so
                        // we never reacquire `profile` later just to read
                        // five fields out of `p.tick`. `TickPayload` is
                        // trivially cheap (no allocations), so computing
                        // it under lock is free; the second
                        // `profile.lock().await` it replaces was an
                        // unconditional await every time a line matched
                        // the user's tick reset pattern.
                        let (result, tick_reset_payload, script_apply) = {
                            let lock_t0 = std::time::Instant::now();
                            let mut p = profile.lock().await;
                            perf.mutex_wait_ns += lock_t0.elapsed().as_nanos() as u64;
                            perf.mutex_acquires += 1;
                            let result = vosh_trigger::process(&p.triggers, &bytes);
                            let tick_payload = if p.tick.check_reset_match(&plain) {
                                p.tick.reset(Instant::now());
                                Some(TickPayload::from_runtime(&p.tick, Instant::now(), false))
                            } else {
                                None
                            };
                            // Run Lua triggers on the same plain text so
                            // patterns can match without worrying about
                            // ANSI escape bytes.
                            script_state::snapshot_vars(&p.script, &p.vars);
                            let mut outcome = match p.script.match_line(&plain) {
                                Ok(o) => o,
                                Err(err) => {
                                    warn!(error = %err, "lua match_line failed");
                                    vosh_script::ScriptOutcome::default()
                                }
                            };
                            // Execute Lua bodies queued by
                            // `TriggerAction::Script` actions on this
                            // line. The trigger engine collects the
                            // body + capture vector per fire; we run
                            // each here and merge the produced
                            // actions into the same outcome the Lua-
                            // registered triggers wrote, so a single
                            // apply_actions call below picks them
                            // both up.
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
                            let apply = script_state::apply_actions(&mut p, outcome);
                            (result, tick_payload, apply)
                        };
                        perf.trigger_lua_ns += trigger_t0.elapsed().as_nanos() as u64;
                        append_line_result(&mut display_batch, &result, clear_first);
                        if !result.routes.is_empty() {
                            perf.routed_emits += result.routes.len() as u64;
                        }
                        emit_line_routes(app, &result);
                        if let Some(text) = &result.display {
                            // Persist to the searchable SQLite log and the
                            // ring buffer that becomes scrollback on next
                            // launch. The raw bytes carry ANSI; the plain
                            // text column drives the regex search.
                            if let Some(sid) = log_session_id {
                                let lock_t0 = std::time::Instant::now();
                                let mut guard = logs.lock().await;
                                perf.mutex_wait_ns += lock_t0.elapsed().as_nanos() as u64;
                                perf.mutex_acquires += 1;
                                if let Some(store) = guard.as_mut() {
                                    let append_t0 = std::time::Instant::now();
                                    let append_res =
                                        store.append(sid, now_ms(), &plain, Some(&bytes));
                                    perf.log_append_ns += append_t0.elapsed().as_nanos() as u64;
                                    perf.log_appends += 1;
                                    if let Err(e) = append_res {
                                        warn!(error = %e, "log append failed");
                                    }
                                }
                            }
                            let sb_t0 = std::time::Instant::now();
                            scrollback.lock().await.push(text.as_bytes().to_vec());
                            perf.scrollback_push_ns += sb_t0.elapsed().as_nanos() as u64;
                            perf.scrollback_pushes += 1;
                        }
                        send_trigger_outputs(stream, &result.sends).await?;
                        apply_script_result(app, stream, profile, timers, script_apply).await?;
                        if let Some(payload) = tick_reset_payload {
                            if let Err(e) = app.emit("session://tick", &payload) {
                                warn!(error = %e, "failed to emit tick reset payload");
                            }
                        }
                    }
                }
            }
            if !display_batch.is_empty() {
                perf.output_emits += 1;
                perf.output_emit_bytes += display_batch.len() as u64;
                emit_output(app, display_batch);
            }
            Ok(())
        }
        TelnetEvent::Subnegotiation { option, payload } if option == telnet_option::GMCP => {
            perf.gmcp_packets += 1;
            handle_gmcp(app, profile, map, timers, stream, &payload, perf).await?;
            Ok(())
        }
        TelnetEvent::Command(byte) if byte == telnet_codes::EOR || byte == telnet_codes::GA => {
            // The server marked the end of a prompt. Flush any partial we
            // had buffered so the prompt sits on its own line and the
            // next chunk's first complete line lands cleanly below it
            // instead of merging into the prompt.
            flush_partial_prompt(app, accumulator);
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
        TelnetEvent::Will(opt) if opt == telnet_option::ECHO => {
            // Server is taking over echo (password prompt incoming).
            // Acknowledge and tell the frontend to mask the input.
            let response = negotiator.handle(&TelnetEvent::Will(opt));
            if !response.is_empty() {
                stream.write_all(&response).await?;
                stream.flush().await?;
            }
            emit_input_mode(app, true);
            Ok(())
        }
        TelnetEvent::Wont(opt) if opt == telnet_option::ECHO => {
            // Server hands echo back to us (password done).
            let response = negotiator.handle(&TelnetEvent::Wont(opt));
            if !response.is_empty() {
                stream.write_all(&response).await?;
                stream.flush().await?;
            }
            emit_input_mode(app, false);
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
    info!(package = %msg.package, data = %msg.data, "gmcp received");
    // Phase 5: same fold as the per-line path. Build the tick-reset
    // payload under the existing lock so a World.Time hour change
    // doesn't force a second `profile.lock().await` after release.
    let (tick_reset_payload, script_apply) = {
        let lock_t0 = std::time::Instant::now();
        let mut p = profile.lock().await;
        perf.mutex_wait_ns += lock_t0.elapsed().as_nanos() as u64;
        perf.mutex_acquires += 1;
        gmcp_bind::apply(&mut p.vars, &msg);
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
        let ticked = observe_world_time_for_tick(&mut p.tick, &msg);
        let tick_payload = if ticked {
            Some(TickPayload::from_runtime(&p.tick, Instant::now(), false))
        } else {
            None
        };
        script_state::snapshot_vars(&p.script, &p.vars);
        let outcome = match p.script.dispatch_gmcp(&msg.package, &msg.data) {
            Ok(o) => o,
            Err(err) => {
                warn!(error = %err, "lua dispatch_gmcp failed");
                vosh_script::ScriptOutcome::default()
            }
        };
        let apply = script_state::apply_actions(&mut p, outcome);
        (tick_payload, apply)
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
    if let Some(payload) = tick_reset_payload {
        if let Err(e) = app.emit("session://tick", &payload) {
            warn!(error = %e, "failed to emit tick payload after world hour change");
        } else {
            perf.tick_emits += 1;
        }
    }
    apply_script_result(app, stream, profile, timers, script_apply).await?;
    if let Err(e) = map_state::handle_room_info(app, map, &msg).await {
        warn!(error = %e, "failed to update map from Room.Info");
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
    let event_name = format!("session://gmcp/{}", msg.package.replace('.', "-"));
    if let Err(e) = app.emit(&event_name, &msg.data) {
        warn!(error = %e, package = %msg.package, "failed to emit GMCP event");
    }
    // `perf.gmcp_packets` already incremented by the caller before
    // we ran. This `emit` count would otherwise duplicate that, so
    // we leave gmcp_packets as the single source.
    Ok(())
}

/// Detect a tick fire from a GMCP `World.Time` push. Aabahran (and most ROM
/// derivatives that ship World.Time) advance the `hour` field every server
/// tick, so an hour change is the natural reset signal. Returns true when
/// the tick was reset.
fn observe_world_time_for_tick(tick: &mut TickRuntime, msg: &vosh_gmcp::Message) -> bool {
    if msg.package != "World.Time" {
        return false;
    }
    let Some(obj) = msg.data.as_object() else {
        return false;
    };
    let Some(hour_value) = obj.get("hour") else {
        return false;
    };
    let hour_str = match hour_value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => return false,
    };
    if tick.observe_world_hour(&hour_str) {
        tick.reset(Instant::now());
        true
    } else {
        false
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
        {
            let mut p = profile.lock().await;
            for line in apply.inputs {
                let result = crate::input::process(&mut p, &line);
                input_bytes.extend(result.bytes);
                input_echoes.extend(result.echo);
            }
        }
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
    Ok(())
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

/// Treat any buffered partial as a complete prompt line. The prompt is
/// already on screen as raw text (the `LineAccumulator` emitted it as a
/// `RawDisplay` chunk when the bytes first arrived). All we need to do
/// is push the cursor past it so the next chunk's content lands on a
/// fresh row. Skipping the trigger reprocess and clear-rewrite avoids
/// the visible flicker that previously read as input lag.
///
/// Called both when the telnet parser reports a GA or EOR command and
/// when the user submits typed input.
fn flush_partial_prompt(app: &AppHandle, accumulator: &mut LineAccumulator) {
    let Some((_bytes, already_shown)) = accumulator.flush_partial() else {
        return;
    };
    if already_shown {
        emit_output(app, b"\r\n".to_vec());
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

fn emit_output(app: &AppHandle, bytes: Vec<u8>) {
    if let Err(e) = app.emit("session://output", OutputPayload { bytes }) {
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

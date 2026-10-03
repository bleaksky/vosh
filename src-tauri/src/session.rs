//! Per-session task. Wires the connection, the telnet parser, the line
//! accumulator, and the trigger engine together. Emits Tauri events.

mod batch;
mod connection;
pub(crate) mod echo;
pub(crate) mod effects;
mod gmcp;
mod gmcp_vars;
pub(crate) mod highlight_ground;
mod lines;
mod perf;
pub(crate) mod prompt_view;
pub(crate) mod room_block;
mod steps;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::{debug, error, info, warn};
use vosh_automation::trigger::LineResult;
use vosh_prompt::stage::Output;
use vosh_protocol::telnet::{
    codes as telnet_codes, option as telnet_option, Event as TelnetEvent, Negotiator, Parser,
};

use crate::app::events;
use crate::output::{emit_output, emit_repaint, output_count};
use crate::profile::Profile;
use crate::script::{self, PendingTimer, SharedTimers};

use batch::{emit_session_output, ReadBatch, Settle};
use connection::{ConnectionError, Stream};
use echo::ServerEcho;
use effects::{apply_script_result, deliver_tick_step, run_fired_command, OutputSink, ScriptIo};
use gmcp::{handle_gmcp, hello_subnegotiation, supports_subnegotiation};
use lines::{LineAccumulator, Partial};
use perf::{PerfCounters, PERF_REPORT_INTERVAL};
use prompt_view::{
    emit_hidden_change, emit_prompt_state, emit_prompt_vars, end_prompt, report_game_prompt_seen,
    send_prompt_vars, start_prompt, watched_state, watching_prompt,
};
use steps::{
    clock_after, clock_step, end_held, end_preview_step, hold_step, late_repaint_after,
    late_repaint_step, let_go_held, line_step, marker_step, partial_step, repaint_step, send_step,
    window_size_step, LineStep,
};

/// The 250 ms poll that drives the tick, the Lua timers and the Settings
/// timers.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

const READ_BUFFER_BYTES: usize = 8 * 1024;

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
    lua_timers: SharedTimers,
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
        lua_timers,
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
    lua_timers: SharedTimers,
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

    let mut tick_interval = tokio::time::interval(POLL_INTERVAL);
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
                        &lua_timers,
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
                            &lua_timers,
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
                            &lua_timers,
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
                        &lua_timers,
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
                                        &lua_timers,
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
                                    &lua_timers,
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
                if let Err(e) = handle_tick(&app, &mut stream, &profile, &lua_timers).await {
                    error!(error = %e, "tick handling failed");
                    break Some(format!("tick handling failed: {e}"));
                }
                if let Err(e) =
                    fire_due_script_timers(&app, &mut stream, &profile, &lua_timers).await
                {
                    error!(error = %e, "script timer firing failed");
                }
                if let Err(e) = fire_due_profile_timers(
                    &app,
                    &mut stream,
                    &profile,
                    &lua_timers,
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
    lua_timers: &SharedTimers,
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
    deliver_tick_step(
        app,
        stream,
        profile,
        lua_timers,
        step,
        &mut OutputSink::Direct,
    )
    .await
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
    lua_timers: &SharedTimers,
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
            lua_timers,
            &command,
            &mut OutputSink::Direct,
        )
        .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn handle_event<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stream: &mut Stream,
    negotiator: &mut Negotiator,
    accumulator: &mut LineAccumulator,
    profile: &Arc<Mutex<Profile>>,
    lua_timers: &SharedTimers,
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
                    deliver_line_step(
                        app, stream, profile, lua_timers, scrollback, batch, step, perf,
                    )
                    .await?;
                }
            }
            Ok(())
        }
        TelnetEvent::Subnegotiation { option, payload } if option == telnet_option::GMCP => {
            perf.gmcp_packets += 1;
            batch.gmcp = true;
            handle_gmcp(app, profile, lua_timers, stream, &payload, batch, perf).await?;
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
                deliver_line_step(
                    app, stream, profile, lua_timers, scrollback, batch, step, perf,
                )
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
    lua_timers: &SharedTimers,
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
            app, stream, profile, lua_timers, scrollback, &mut batch, step, perf,
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

/// Do what a line step left for after the profile lock: emit its routes,
/// keep it for scrollback, send what its triggers send, apply its Lua
/// actions' IO into the batch, and run the tick command it fired.
#[allow(clippy::too_many_arguments)]
async fn deliver_line_step<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stream: &mut Stream,
    profile: &Arc<Mutex<Profile>>,
    lua_timers: &SharedTimers,
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
        lua_timers,
        apply,
    )
    .await?;
    if let Some(step) = tick_step {
        deliver_tick_step(app, stream, profile, lua_timers, step, &mut sink).await?;
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
    lua_timers: &SharedTimers,
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
        deliver_line_step(
            app, stream, profile, lua_timers, scrollback, batch, step, perf,
        )
        .await?;
    }
    Ok(())
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

async fn fire_due_script_timers<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stream: &mut Stream,
    profile: &Arc<Mutex<Profile>>,
    lua_timers: &SharedTimers,
) -> std::io::Result<()> {
    let now = Instant::now();
    let due: Vec<PendingTimer> = {
        let mut guard = lua_timers.lock().await;
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
    apply_script_result(app, &mut io, profile, lua_timers, apply).await
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

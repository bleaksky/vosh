//! The session loop and the [`Conn`] it owns for a connection. The loop
//! sends your lines to the game, takes each socket read through the read
//! path, repaints your prompt when a deadline passes, and polls the tick,
//! the Lua timers and the Settings timers. When the connection ends, it
//! captures what the game sent last, saves the scrollback and clears what
//! lasts only as long as the session.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{mpsc, Mutex};
use tokio::time::Instant;
use tracing::{debug, error, info, warn};
use vosh_protocol::telnet::{option as telnet_option, Negotiator, Parser};

use crate::app::events;
use crate::output::{emit_output, emit_repaint, output_count};
use crate::profile::live::Profile;
use crate::script::SharedTimers;

use super::batch::Settle;
use super::connection::Stream;
use super::echo::ServerEcho;
use super::effects::{deliver_tick_step, run_fired_command, OutputSink};
use super::lines::LineAccumulator;
use super::log_sink::{capture_held_lines, capture_pending_line, LogSink};
use super::lua_timers;
use super::perf::{PerfCounters, PERF_REPORT_INTERVAL};
use super::prompt_view::{
    emit_hidden_change, emit_prompt_state, emit_prompt_vars, end_prompt, start_prompt,
    watched_state, watching_prompt,
};
use super::read::{finish_read, flush_hold, let_go_held_lines, READ_BUFFER_BYTES};
use super::steps::{
    clock_after, clock_step, end_preview_step, hold_step, late_repaint_after, late_repaint_step,
    repaint_step, send_step, window_size_step,
};
use super::{
    emit_input_mode, emit_state, now_ms, room_block, OutgoingMsg, StatePayload, TargetPayload,
};

/// The 250 ms poll that drives the tick, the Lua timers and the Settings
/// timers.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// What the loop owns for one connection. The read path and the GMCP
/// handler take it whole, with the connection's [`LogSink`] beside it,
/// in place of a list of its parts.
pub(super) struct Conn<R: tauri::Runtime> {
    pub(super) app: AppHandle<R>,
    pub(super) stream: Stream,
    pub(super) negotiator: Negotiator,
    pub(super) parser: Parser,
    pub(super) accumulator: LineAccumulator,
    /// Who echoes your input on this connection. Every read updates it
    /// in wire order before the loop takes the next outgoing line, so
    /// each send is logged by the state in force as its bytes leave.
    /// It lives and dies with the connection, so one that dropped mid
    /// password prompt hands nothing to the next one.
    pub(super) server_echo: ServerEcho,
    pub(super) profile: Arc<Mutex<Profile>>,
    pub(super) lua_timers: SharedTimers,
    /// What the loop counts on its hot path, see [`PerfCounters`].
    pub(super) perf: PerfCounters,
    /// The output count after this session last wrote. Output from
    /// elsewhere moves it, which closes the open row.
    pub(super) seen_output: u64,
    /// The frame and the log rows the reads since the socket was last
    /// quiet owe.
    pub(super) settle: Settle,
}

pub(super) async fn io_loop<R: tauri::Runtime>(
    app: AppHandle<R>,
    stream: Stream,
    mut rx_outgoing: mpsc::UnboundedReceiver<OutgoingMsg>,
    profile: Arc<Mutex<Profile>>,
    lua_timers: SharedTimers,
    mut log_sink: LogSink,
    negotiator: Negotiator,
    known_host: bool,
) {
    let mut buf = vec![0u8; READ_BUFFER_BYTES];

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

    let mut poll = tokio::time::interval(POLL_INTERVAL);
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Per-timer next-fire deadlines for the Settings interval timers.
    // Seeded on first sight in fire_due_settings_timers; cleared here so
    // each connection starts its timers fresh.
    let mut timer_next: HashMap<u32, Instant> = HashMap::new();

    let mut conn = Conn {
        app,
        stream,
        negotiator,
        parser: Parser::new(),
        accumulator: LineAccumulator::new(),
        server_echo: ServerEcho::default(),
        profile,
        lua_timers,
        perf: PerfCounters::default(),
        seen_output: output_count(),
        settle: Settle::default(),
    };
    // When a partial that can still become your prompt stops waiting for
    // the next read and paints raw.
    let mut hold_until: Option<Instant> = None;
    // When a GMCP packet that changed your prompt, with no text after it,
    // repaints it.
    let mut late_until: Option<Instant> = None;
    // When a clock piece in your design next shows another second, while
    // your design draws one.
    let mut clock_until: Option<Instant> = None;

    // The arm that writes a burst's log rows waits on the log through a
    // handle of its own, since the guard it yields would borrow the log
    // sink the other arms change.
    let log_store = log_sink.logs.clone();

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
                        flush_hold(&mut conn).await;
                    }
                    // Lines held for the rest of a prompt let go as they
                    // show, before your line leaves, since it follows them.
                    if let Err(e) = let_go_held_lines(&mut conn, &mut log_sink).await {
                        warn!(error = %e, "letting go of held lines failed");
                    }
                    // The send records a prompt candidate and closes the
                    // open row. On a server that sends no Char.Vitals it
                    // also starts the next pulse, after which the values
                    // the last prompt set go stale.
                    let pulse = {
                        let mut p = conn.profile.lock().await;
                        send_step(&mut p, &conn.accumulator, &bytes, now_ms())
                    };
                    // The frontend already echoed the typed line inline
                    // with the on-screen prompt. Drop the buffered partial
                    // so the next chunk from the server starts fresh on a
                    // new row instead of merging with the displayed prompt.
                    conn.accumulator.forget_partial();
                    if pulse {
                        emit_hidden_change(&conn.app, &conn.profile).await;
                        emit_prompt_vars(&conn.app, &conn.profile, false).await;
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
                    let sent = log_sink.id().map(|sid| {
                        let rows = vosh_log::sent_rows(&bytes, conn.server_echo.hides(masked));
                        (sid, now_ms(), rows)
                    });
                    let wrote = match conn.stream.write_all(&bytes).await {
                        Err(e) => Err(("write failed", e)),
                        Ok(()) => conn.stream.flush().await.map_err(|e| ("flush failed", e)),
                    };
                    if let Some((sid, at, rows)) = sent {
                        conn.settle.queue_rows(vosh_log::sent_entries(sid, at, rows));
                    }
                    if let Err((what, e)) = wrote {
                        error!(error = %e, "{what}");
                        break Some(format!("{what}: {e}"));
                    }
                    // Lines sent back to back never wait on the log, but
                    // their rows still go in once they waited too long.
                    conn.settle.overdue_now(&conn.app, &log_sink, &mut conn.perf);
                }
                Some(OutgoingMsg::WindowSize { cols, rows }) => {
                    // A design that pushes part of a row to the right
                    // edge draws again at the new width. The card hears
                    // the state with it, so its marks move with the push.
                    let (out, state) = {
                        let mut p = conn.profile.lock().await;
                        let redraw = window_size_step(
                            &mut p,
                            &mut conn.negotiator,
                            cols,
                            rows,
                            watching_prompt(&conn.app),
                        );
                        let out = redraw.then(|| {
                            repaint_step(&mut p, output_count() != conn.seen_output, Instant::now())
                        });
                        let state = out
                            .as_ref()
                            .filter(|out| !out.is_empty())
                            .and_then(|_| watched_state(&conn.app, &p));
                        (out, state)
                    };
                    if let Some(out) = out.filter(|out| !out.is_empty()) {
                        emit_repaint(&conn.app, &out);
                    }
                    emit_prompt_state(&conn.app, state);
                    // Once the server sent DO NAWS and Vosh agreed, every
                    // new size goes out, so the game wraps at the new
                    // column count.
                    if conn.negotiator.vosh_does(telnet_option::NAWS) {
                        let bytes = conn.negotiator.naws_subnegotiation();
                        if let Err(e) = conn.stream.write_all(&bytes).await {
                            error!(error = %e, "naws write failed");
                            break Some(format!("naws write failed: {e}"));
                        }
                        if let Err(e) = conn.stream.flush().await {
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
                    let landed_last = !conn.profile.lock().await.prompt.stage.wrote_after(after);
                    if landed_last {
                        if hold_until.take().is_some() {
                            flush_hold(&mut conn).await;
                        }
                        // Your typed echo follows the lines held for the
                        // rest of a prompt, so they let go as they show.
                        if let Err(e) = let_go_held_lines(&mut conn, &mut log_sink).await {
                            warn!(error = %e, "letting go of held lines failed");
                        }
                    }
                    let mut p = conn.profile.lock().await;
                    p.prompt.stage.local_write(after);
                }
                Some(OutgoingMsg::PromptRepaint) => {
                    // The card asked for it, so its state follows even when
                    // the bytes stay the same, since the pieces in them can
                    // be numbered anew.
                    let (out, state) = {
                        let mut p = conn.profile.lock().await;
                        let now = Instant::now();
                        let out = repaint_step(&mut p, output_count() != conn.seen_output, now);
                        // The design may have gained or lost a clock piece.
                        clock_until = clock_after(&p, now);
                        (out, watched_state(&conn.app, &p))
                    };
                    if !out.is_empty() {
                        emit_repaint(&conn.app, &out);
                    }
                    emit_prompt_state(&conn.app, state);
                }
                None => {
                    debug!("outgoing channel closed; shutting down session");
                    break None;
                }
            },
            read = conn.stream.read(&mut buf) => match read {
                Ok(0) => {
                    info!("server closed connection");
                    break Some("server closed connection".to_string());
                }
                Ok(n) => {
                    let batch = conn.handle_read(&buf[..n], &log_sink).await;
                    // The next read ends a hold. One that goes on holding
                    // keeps the first deadline, so a partial waits at most
                    // HOLD_MS in all.
                    hold_until = batch.hold.then(|| {
                        hold_until.unwrap_or_else(|| {
                            Instant::now() + Duration::from_millis(vosh_prompt::stage::HOLD_MS)
                        })
                    });
                    let (gmcp, prompt, wrote) = (batch.gmcp, batch.prompt, batch.out.writes_text());
                    clock_until = finish_read(&mut conn, &mut log_sink, batch).await;
                    if gmcp || late_until.is_some() {
                        let p = conn.profile.lock().await;
                        late_until =
                            late_repaint_after(&p, late_until, gmcp, prompt, wrote, Instant::now());
                    }
                    // A game that never pauses still gets its frame and
                    // its rows every FRAME_BUDGET.
                    conn.settle.overdue_now(&conn.app, &log_sink, &mut conn.perf);
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
                        match conn.stream.try_read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => {
                                drained_bytes += n;
                                let mut batch = conn.handle_read(&buf[..n], &log_sink).await;
                                // The connection is going, so nothing waits.
                                if batch.hold {
                                    let mut p = conn.profile.lock().await;
                                    hold_step(&mut p, &mut conn.accumulator, &mut batch.out);
                                }
                                hold_until = None;
                                // The connection is going, so no clock
                                // repaints after it.
                                let _ = finish_read(&mut conn, &mut log_sink, batch).await;
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
                flush_hold(&mut conn).await;
            }
            () = sleep_until_hold(late_until), if late_until.is_some() => {
                late_until = None;
                let (out, state) = {
                    let mut p = conn.profile.lock().await;
                    let out = late_repaint_step(
                        &mut p,
                        output_count() != conn.seen_output,
                        Instant::now(),
                    );
                    let state = if out.is_empty() { None } else { watched_state(&conn.app, &p) };
                    (out, state)
                };
                if !out.is_empty() {
                    emit_repaint(&conn.app, &out);
                }
                emit_prompt_state(&conn.app, state);
            }
            () = sleep_until_hold(clock_until), if clock_until.is_some() => {
                // A clock piece shows another second. Your idle prompt
                // repaints with it, unless you are selecting text or
                // reading back, and then the next second tries again.
                let reading = reader_busy(&conn.app);
                let (out, state) = {
                    let mut p = conn.profile.lock().await;
                    let now = Instant::now();
                    let out = clock_step(&mut p, output_count() != conn.seen_output, reading, now);
                    clock_until = clock_after(&p, now);
                    let state = if out.is_empty() { None } else { watched_state(&conn.app, &p) };
                    (out, state)
                };
                if !out.is_empty() {
                    emit_repaint(&conn.app, &out);
                }
                emit_prompt_state(&conn.app, state);
            }
            _ = poll.tick() => {
                if let Err(e) =
                    handle_tick(&conn.app, &mut conn.stream, &conn.profile, &conn.lua_timers).await
                {
                    error!(error = %e, "tick handling failed");
                    break Some(format!("tick handling failed: {e}"));
                }
                if let Err(e) = lua_timers::fire_due(
                    &conn.app,
                    &mut conn.stream,
                    &conn.profile,
                    &conn.lua_timers,
                )
                .await
                {
                    error!(error = %e, "lua timer firing failed");
                }
                if let Err(e) = fire_due_settings_timers(
                    &conn.app,
                    &mut conn.stream,
                    &conn.profile,
                    &conn.lua_timers,
                    &mut timer_next,
                )
                .await
                {
                    error!(error = %e, "settings timer firing failed");
                }
            }
            _ = perf_report_interval.tick() => {
                conn.perf.report_and_reset();
            }
            // Nothing else is ready, so the socket has nothing more for
            // now and the burst of reads that just ended shows in one
            // frame.
            () = std::future::ready(()), if conn.settle.frame => {
                conn.settle.frame_now(&conn.app);
            }
            // Then the log takes the burst's rows once it is free, so a
            // busy log never holds the loop.
            mut guard = log_store.lock(), if !conn.settle.log.is_empty() => {
                conn.settle.write_log(guard.as_mut(), &mut conn.perf);
            }
        }
    };

    // A preview the card shows on your prompt goes with the connection,
    // so the live render goes back on the row first.
    let out = {
        let mut p = conn.profile.lock().await;
        end_preview_step(&mut p, output_count() != conn.seen_output, Instant::now())
    };
    if !out.is_empty() {
        emit_repaint(&conn.app, &out);
    }

    // Capture the MUD's final partial line before teardown drops it. A
    // `quit` logout banner usually arrives without a trailing newline,
    // so it sits in the accumulator as a partial: painted at the end of
    // its read but never run through the per-line path that logs and
    // scrollback-records it.
    // Flush it now, ahead of the scrollback dump and log close below, so
    // the goodbye is captured like every other client captures it.
    if hold_until.is_some() {
        flush_hold(&mut conn).await;
    }
    // The last burst still owes its frame and its rows, which go in the
    // log before the lines the session captures as it ends.
    conn.settle.frame_now(&conn.app);
    conn.settle
        .write_log(log_sink.logs.lock().await.as_mut(), &mut conn.perf);
    capture_held_lines(&conn.profile, &log_sink).await;
    capture_pending_line(&conn.app, &log_sink, &mut conn.accumulator).await;

    {
        let mut p = conn.profile.lock().await;
        p.tick.end_session();
    }

    log_sink.close().await;

    let line_triggers;
    // Session-only target state and the cached Room.Chars list clear
    // on disconnect. Quick-key verb bindings outlive the session (never
    // a restart), but the active target and room snapshot end with it.
    let target_after = {
        let mut p = conn.profile.lock().await;
        let had = p.target.name.is_some();
        p.target.name = None;
        p.target.room_idx = None;
        p.room_chars.clear();
        p.room_block = room_block::RoomBlock::default();
        p.vars.remove("target");
        line_triggers = p.prompt.stage.line_trigger_notice();
        end_prompt(&mut p);
        // A new GMCP handler gets the last packet of its package, and
        // the packets of this connection end with it.
        p.script.forget_gmcp_packets();
        had.then(|| TargetPayload::of(&p))
    };
    // Line triggers no longer see a prompt the profile reads, so the first
    // session that read yours names the ones that matched it, once, at the
    // next launch.
    if let Some(names) = line_triggers {
        let state = conn.app.state::<crate::app::state::SharedState>();
        crate::disk::upgrades::line_triggers::note_line_triggers(state.inner(), names).await;
    }
    if let Some(payload) = target_after {
        let _ = conn.app.emit(events::TARGET, payload);
    }
    let _ = conn.stream.shutdown().await;
    // The affects list goes stale with the session, as the frontend
    // store drops its copy on the disconnected state below. The affect
    // fulls are written for the next login, then cleared.
    let shared = conn.app.state::<crate::app::state::SharedState>();
    shared.last_affects.clear();
    crate::affects::full::disconnect(&conn.app, shared.inner());
    // Reset password mode on disconnect so the next session starts with
    // a normal-text input even if the server bailed mid-password-prompt.
    emit_input_mode(&conn.app, false);
    emit_state(
        &conn.app,
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
async fn fire_due_settings_timers<R: tauri::Runtime>(
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
    crate::native::grid::reader_busy()
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

/// Map a read-side `io::Error` to a short human-readable disconnect
/// reason. MUD quits routinely land here, because Diku and ROM
/// derivatives close with `SO_LINGER` 0 and the OS reports it as
/// `ECONNRESET`, and the raw "read failed: Connection reset by peer (os
/// error 54)" reads like an internal panic. So the common kinds read like
/// a normal disconnect, and anything else falls back to the raw text.
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

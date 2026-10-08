//! The session loop and the [`Conn`] it owns for a connection. The loop
//! sends your lines to the game, takes each socket read through the read
//! path, hands the walker your `#walk` lines and gives up on a step that
//! waited too long, repaints your prompt when a deadline passes, polls
//! the tick, the Lua timers and the Settings timers, and reads the round
//! trip to the game every two seconds. It sends what the writer asks of
//! the game's line editor and holds the session's other sends while the
//! writer works. When the
//! connection ends, it captures what the game sent last, saves the
//! scrollback and clears what lasts only as long as the session.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tracing::{debug, error, info, warn};
use vosh_protocol::telnet::{option as telnet_option, Negotiator, Parser};

use crate::app::events;
use crate::input::walk::WalkCommand;
use crate::output::{echo_lines, emit_output, emit_repaint};
use crate::profile::live::Profile;
use crate::sessions::Session;

use super::batch::Settle;
use super::echo::ServerEcho;
use super::effects::{
    collect_script_result, deliver_tick_step, framed_echoes, run_fired_command, Collected,
    OutputSink, ScriptIo,
};
use super::lines::LineAccumulator;
use super::log_sink::{capture_held_lines, capture_pending_line, LogSink};
use super::lua_timers;
use super::perf::{PerfCounters, PERF_REPORT_INTERVAL};
use super::prompt_view::{
    emit_hidden_change, emit_prompt_state, emit_prompt_vars, end_prompt, start_prompt,
    watched_state, watching_prompt,
};
use super::read::{
    finish_read, flush_hold, flush_reader_wait, hear_reader_wait, let_go_held_lines,
    READ_BUFFER_BYTES,
};
use super::round_trip::{RoundTripPayload, READ_EVERY};
use super::socket::Stream;
use super::steps::{
    clock_after, clock_step, end_preview_step, hold_step, late_repaint_after, late_repaint_step,
    repaint_step, send_step, window_size_step,
};
use super::walk::{self, WalkProgress, Walker};
use super::writer::{Writer, WritingState};
use super::{emit_input_mode, emit_state, now_ms, OutgoingMsg, StatePayload, TargetPayload};

/// The 250 ms poll that drives the tick, the Lua timers and the Settings
/// timers.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// What the loop owns for one connection, and the [`Session`] it runs
/// for, whose [`Connection`](super::connection::Connection) it shares
/// with the commands. The read path and the GMCP handler take it whole,
/// with the connection's [`LogSink`] beside it, in place of a list of its
/// parts.
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
    /// The session this loop runs for. Lock its connection after the
    /// profile it plays.
    pub(super) session: Arc<Session>,
    /// What the loop counts on its hot path, see [`PerfCounters`].
    pub(super) perf: PerfCounters,
    /// The session's output count after this loop last wrote. Output
    /// from elsewhere in the session moves the count, which closes the
    /// open row.
    pub(super) seen_output: u64,
    /// The frame and the log rows the reads since the socket was last
    /// quiet owe.
    pub(super) settle: Settle,
    /// The walker, which sends the steps of a `#walk` one at a time.
    pub(super) walker: Walker,
    /// What the walker said during the read under way, which shows at
    /// its end.
    pub(super) walk_lines: Vec<String>,
    /// Where the walk stood when the page last heard, see
    /// [`Conn::tell_walk`].
    pub(super) walk_told: WalkProgress,
    /// The writer, which drives the game's line editor for the writing
    /// card.
    pub(super) writer: Writer,
    /// The lines the writer asked for during the read under way, which go
    /// out once the read ends, see [`send_writer`].
    pub(super) writer_send: Vec<String>,
    /// Where the writer stood when the page last heard, see
    /// [`Conn::tell_writing`].
    pub(super) writing_told: WritingState,
}

impl<R: tauri::Runtime> Conn<R> {
    /// Output from elsewhere in the session, such as a slash command's
    /// echo, landed since this loop last wrote, which closes the open row.
    pub(super) fn others_wrote(&self) -> bool {
        self.session.output_count() != self.seen_output
    }

    /// Tell the page where the walk stands when that changed. The loop
    /// asks once at the end of each pass, which covers every event the
    /// walker hears.
    fn tell_walk(&mut self) {
        let progress = self.walker.progress();
        if progress != self.walk_told {
            self.session.emit(&self.app, events::WALK, &progress);
            self.walk_told = progress;
        }
    }

    /// Tell the page where the writer stands when that changed, once at
    /// the end of each pass as for the walk.
    fn tell_writing(&mut self) {
        let state = self.writer.state(self.stream.held_lines());
        if state != self.writing_told {
            self.session.emit(&self.app, events::WRITING, &state);
            self.writing_told = state;
        }
    }
}

pub(super) async fn io_loop<R: tauri::Runtime>(
    app: AppHandle<R>,
    stream: Stream,
    mut rx_outgoing: mpsc::UnboundedReceiver<OutgoingMsg>,
    session: Arc<Session>,
    mut log_sink: LogSink,
    negotiator: Negotiator,
    known_host: bool,
) {
    let mut buf = vec![0u8; READ_BUFFER_BYTES];

    // Start the tick timer for this session, unsynced until the game's
    // first tick. A connect turns your switch on, unless another session
    // on the profile is connected, whose count already follows the switch
    // as it stands. The prompt engine starts with no packets and the
    // host's rules.
    let shared = app.state::<crate::app::state::SharedState>();
    let others = shared.other_sessions(session.id);
    {
        let mut p = session.lock_profile().await;
        let joins = p.players(&others).any(|other| other.connected());
        let mut c = session.connection.lock();
        if joins {
            c.tick.join_connected(&p.tick, Instant::now());
        } else {
            c.tick.start_session(&mut p.tick, Instant::now());
        }
        // A new link starts with nothing the last one followed for the
        // alerts and the redial.
        c.preset_watch.reset();
        c.link = super::reconnect::LinkWatch::default();
        c.log_kinds = super::log_kinds::LogKinds::default();
        c.round_trip.connect(chrono::Local::now().time());
        start_prompt(&mut p, &mut c, known_host);
        // A push to the right edge reaches to the width the game is told.
        c.prompt.set_cols(usize::from(negotiator.window_size.0));
    }
    // The row reads the session as connected from here, since the count
    // marks it in session, after the connected state went out.
    crate::sessions::broadcast_sessions(&app, &shared);

    let mut poll = tokio::time::interval(POLL_INTERVAL);
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Per-timer next-fire deadlines for the Settings interval timers.
    // Seeded on first sight in fire_due_settings_timers; cleared here so
    // each connection starts its timers fresh.
    let mut timer_next: HashMap<u32, Instant> = HashMap::new();

    let seen_output = session.output_count();
    let mut conn = Conn {
        app,
        stream,
        negotiator,
        parser: Parser::new(),
        accumulator: LineAccumulator::new(),
        server_echo: ServerEcho::default(),
        session,
        perf: PerfCounters::default(),
        seen_output,
        settle: Settle::default(),
        walker: Walker::default(),
        walk_lines: Vec::new(),
        walk_told: WalkProgress::Idle,
        writer: Writer::default(),
        writer_send: Vec::new(),
        writing_told: WritingState::default(),
    };
    // When a partial that can still become your prompt stops waiting for
    // the next read and paints raw.
    let mut hold_until: Option<Instant> = None;
    // When a screen reader reads the partial a read ended on, if it is
    // still there, since nothing came to end its line.
    let mut reader_until: Option<Instant> = None;
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

    // The round trip to the game, read first one interval in and sent
    // whenever it moves.
    let mut round_trip_interval = tokio::time::interval_at(Instant::now() + READ_EVERY, READ_EVERY);
    round_trip_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut round_trip_sent = RoundTripPayload::of(None);

    let disconnect_reason = loop {
        // When the step on its way gives up waiting for its room.
        let walk_until = conn.walker.deadline();
        // When the writer next looks at the time.
        let write_until = conn.writer.deadline();
        tokio::select! {
            biased;
            outgoing = rx_outgoing.recv() => match outgoing {
                Some(OutgoingMsg::Send { bytes, masked }) => {
                    let sent = send_typed(&mut conn, &mut log_sink, &mut hold_until, &bytes, masked, From::Typed);
                    if let Err(reason) = sent.await {
                        break Some(reason);
                    }
                }
                Some(OutgoingMsg::Writer(command)) => {
                    let out = conn.stream.lines_out();
                    let send = conn.writer.command(command, out, Instant::now());
                    conn.writer_send.extend(send);
                    if let Err(reason) = send_writer(&mut conn, &mut log_sink, &mut hold_until).await {
                        break Some(reason);
                    }
                }
                Some(OutgoingMsg::WindowSize { cols, rows }) => {
                    // A design that pushes part of a row to the right
                    // edge draws again at the new width. The card hears
                    // the state with it, so its marks move with the push.
                    let (out, state) = {
                        let p = conn.session.lock_profile().await;
                        let mut c = conn.session.connection.lock();
                        let redraw = window_size_step(
                            &mut c,
                            &mut conn.negotiator,
                            cols,
                            rows,
                            watching_prompt(&conn.session),
                        );
                        let out = redraw.then(|| {
                            repaint_step(&p, &mut c, conn.others_wrote(), Instant::now())
                        });
                        let state = out
                            .as_ref()
                            .filter(|out| !out.is_empty())
                            .and_then(|_| watched_state(&conn.session, &p, &c));
                        (out, state)
                    };
                    if let Some(out) = out.filter(|out| !out.is_empty()) {
                        emit_repaint(&conn.app, &conn.session, &out);
                    }
                    emit_prompt_state(&conn.app, &conn.session, state);
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
                    let landed_last =
                        !conn.session.connection.lock().prompt.stage.wrote_after(after);
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
                    conn.session.connection.lock().prompt.stage.local_write(after);
                }
                Some(OutgoingMsg::Walk(command)) => {
                    let walked = walk_command(&mut conn, &mut log_sink, &mut hold_until, command);
                    if let Err(reason) = walked.await {
                        break Some(reason);
                    }
                }
                Some(OutgoingMsg::PromptRepaint) => {
                    // The card asked for it, so its state follows even when
                    // the bytes stay the same, since the pieces in them can
                    // be numbered anew.
                    let (out, state) = {
                        let p = conn.session.lock_profile().await;
                        let mut c = conn.session.connection.lock();
                        let now = Instant::now();
                        let out = repaint_step(&p, &mut c, conn.others_wrote(), now);
                        // The design may have gained or lost a clock piece.
                        clock_until = clock_after(&p, &c, now);
                        (out, watched_state(&conn.session, &p, &c))
                    };
                    if !out.is_empty() {
                        emit_repaint(&conn.app, &conn.session, &out);
                    }
                    emit_prompt_state(&conn.app, &conn.session, state);
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
                    reader_until = batch
                        .reader_wait
                        .then(|| Instant::now() + super::reader::PARTIAL_WAIT);
                    let (gmcp, prompt, wrote) = (batch.gmcp, batch.prompt, batch.out.writes_text());
                    clock_until = finish_read(&mut conn, &mut log_sink, batch).await;
                    if let Err(reason) = send_writer(&mut conn, &mut log_sink, &mut hold_until).await {
                        break Some(reason);
                    }
                    if gmcp || late_until.is_some() {
                        let c = conn.session.connection.lock();
                        late_until =
                            late_repaint_after(&c, late_until, gmcp, prompt, wrote, Instant::now());
                    }
                    // A game that never pauses still gets its frame and
                    // its rows every FRAME_BUDGET.
                    conn.settle
                        .overdue_now(&conn.app, &conn.session, &log_sink, &mut conn.perf);
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
                                    let mut c = conn.session.connection.lock();
                                    hold_step(&mut c, &mut conn.accumulator, &mut batch.out);
                                }
                                if batch.reader_wait {
                                    hear_reader_wait(&mut conn, &mut batch.reader).await;
                                }
                                hold_until = None;
                                reader_until = None;
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
            () = sleep_until_hold(reader_until), if reader_until.is_some() => {
                reader_until = None;
                flush_reader_wait(&mut conn).await;
            }
            () = sleep_until_hold(late_until), if late_until.is_some() => {
                late_until = None;
                let (out, state) = {
                    let p = conn.session.lock_profile().await;
                    let mut c = conn.session.connection.lock();
                    let out = late_repaint_step(&p, &mut c, conn.others_wrote(), Instant::now());
                    let state = if out.is_empty() {
                        None
                    } else {
                        watched_state(&conn.session, &p, &c)
                    };
                    (out, state)
                };
                if !out.is_empty() {
                    emit_repaint(&conn.app, &conn.session, &out);
                }
                emit_prompt_state(&conn.app, &conn.session, state);
            }
            () = sleep_until_hold(walk_until), if walk_until.is_some() => {
                let out = conn.walker.expire(Instant::now());
                if !out.lines.is_empty() {
                    emit_output(&conn.app, &conn.session, framed_echoes(&out.lines));
                }
            }
            () = sleep_until_hold(write_until), if write_until.is_some() => {
                let send = conn.writer.poll(Instant::now());
                conn.writer_send.extend(send);
                if let Err(reason) = send_writer(&mut conn, &mut log_sink, &mut hold_until).await {
                    break Some(reason);
                }
            }
            () = sleep_until_hold(clock_until), if clock_until.is_some() => {
                // A clock piece shows another second. Your idle prompt
                // repaints with it, unless you are selecting text or
                // reading back, and then the next second tries again.
                let reading = reader_busy(&conn.session);
                let (out, state) = {
                    let p = conn.session.lock_profile().await;
                    let mut c = conn.session.connection.lock();
                    let now = Instant::now();
                    let out = clock_step(&p, &mut c, conn.others_wrote(), reading, now);
                    clock_until = clock_after(&p, &c, now);
                    let state = if out.is_empty() {
                        None
                    } else {
                        watched_state(&conn.session, &p, &c)
                    };
                    (out, state)
                };
                if !out.is_empty() {
                    emit_repaint(&conn.app, &conn.session, &out);
                }
                emit_prompt_state(&conn.app, &conn.session, state);
            }
            _ = poll.tick() => {
                if let Err(e) = handle_tick(
                    &conn.app,
                    &mut conn.stream,
                    &mut conn.walker,
                    &conn.session,
                )
                .await
                {
                    error!(error = %e, "tick handling failed");
                    break Some(format!("tick handling failed: {e}"));
                }
                if let Err(e) = lua_timers::fire_due(
                    &conn.app,
                    &mut conn.stream,
                    &mut conn.walker,
                    &conn.session,
                )
                .await
                {
                    error!(error = %e, "lua timer firing failed");
                }
                if let Err(e) = fire_due_settings_timers(
                    &conn.app,
                    &mut conn.stream,
                    &mut conn.walker,
                    &conn.session,
                    &mut timer_next,
                )
                .await
                {
                    error!(error = %e, "settings timer firing failed");
                }
                super::vitals_text::on_poll(&conn.app, &conn.session).await;
            }
            _ = perf_report_interval.tick() => {
                conn.perf.report_and_reset();
            }
            _ = round_trip_interval.tick() => {
                let now = Instant::now();
                let sample = conn.stream.round_trip(now);
                if let Some(sample) = sample {
                    let at = chrono::Local::now().time();
                    conn.session.connection.lock().round_trip.record(sample, now, at);
                }
                let payload = RoundTripPayload::of(sample.map(|s| s.reading));
                if payload != round_trip_sent {
                    round_trip_sent = payload;
                    conn.session.emit(&conn.app, events::ROUND_TRIP, &payload);
                }
            }
            // Nothing else is ready, so the socket has nothing more for
            // now and the burst of reads that just ended shows in one
            // frame.
            () = std::future::ready(()), if conn.settle.frame => {
                conn.settle.frame_now(&conn.app, &conn.session);
            }
            // Then the log takes the burst's rows once it is free, so a
            // busy log never holds the loop.
            mut guard = log_store.lock(), if conn.settle.owes_log() => {
                conn.settle.write_log(guard.as_mut(), &mut conn.perf);
            }
        }
        conn.tell_walk();
        conn.tell_writing();
    };
    // The walk ends with the connection, and so does the writer's job.
    conn.walker = Walker::default();
    conn.tell_walk();
    conn.writer.dropped();
    conn.writer.settle();
    conn.tell_writing();

    // A preview the card shows on your prompt goes with the connection,
    // so the live render goes back on the row first.
    let out = {
        let p = conn.session.lock_profile().await;
        let mut c = conn.session.connection.lock();
        end_preview_step(&p, &mut c, conn.others_wrote(), Instant::now())
    };
    if !out.is_empty() {
        emit_repaint(&conn.app, &conn.session, &out);
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
    if reader_until.is_some() {
        flush_reader_wait(&mut conn).await;
    }
    // Every snoop ends with the link, and each tab stays as ended. The
    // partial each one ended on joins the burst's rows.
    let snoop_rows = {
        let mut c = conn.session.connection.lock();
        let at = now_ms();
        c.snoops.link_ended(at);
        c.snoops.take_log(log_sink.id(), at)
    };
    conn.settle.queue_rows(snoop_rows);
    // The last burst still owes its frame and its rows, which go in the
    // log before the lines the session captures as it ends.
    conn.settle.frame_now(&conn.app, &conn.session);
    conn.settle
        .write_log(log_sink.logs.lock().await.as_mut(), &mut conn.perf);
    capture_held_lines(&conn.session.connection, &log_sink).await;
    capture_pending_line(&conn.app, &conn.session, &log_sink, &mut conn.accumulator).await;

    conn.session.connection.lock().tick.end_session();
    conn.session.set_since(None);

    log_sink.close().await;

    let line_triggers;
    let link;
    // Your target, the Room.Chars list, the room look and the fight's
    // tail end with the connection, and so does this session's variable
    // that mirrors the target. Your quick keys outlive it, though not a
    // restart.
    let (target_after, snoops) = {
        let mut p = conn.session.lock_profile().await;
        let mut c = conn.session.connection.lock();
        let had = c.clear_on_disconnect();
        c.round_trip.disconnect();
        link = std::mem::take(&mut c.link);
        line_triggers = c.prompt.stage.line_trigger_notice();
        end_prompt(&mut p, &mut c);
        // A new GMCP handler gets the last packet of its package, and
        // the packets of this connection end with it.
        c.script.forget_gmcp_packets();
        (had.then(|| TargetPayload::of(&c)), c.snoops.take_changes())
    };
    super::snoop::emit(&conn.app, &conn.session, snoops);
    // Line triggers no longer see a prompt the profile reads, so the first
    // session that read yours names the ones that matched it, once, at the
    // next launch.
    if let Some(names) = line_triggers {
        let state = conn.app.state::<crate::app::state::SharedState>();
        crate::disk::upgrades::line_triggers::note_line_triggers(state.inner(), names).await;
    }
    if let Some(payload) = target_after {
        conn.session.emit(&conn.app, events::TARGET, &payload);
    }
    // The reading goes with the connection.
    if round_trip_sent.ms.is_some() {
        conn.session
            .emit(&conn.app, events::ROUND_TRIP, &RoundTripPayload::of(None));
    }
    let _ = conn.stream.shutdown().await;
    // The affects, vitals and combat go stale with the session, as the
    // affects and vitals stores on the page drop theirs on the
    // disconnected state below.
    // The affect fulls are written for the next login, then cleared.
    conn.session.last_packages.clear();
    let shared = conn.app.state::<crate::app::state::SharedState>();
    crate::affects::full::disconnect(&conn.app, shared.inner(), &conn.session);
    // Reset password mode on disconnect so the next session starts with
    // a normal-text input even if the server bailed mid-password-prompt.
    emit_input_mode(&conn.app, &conn.session, false);
    emit_state(
        &conn.app,
        &conn.session,
        StatePayload::Disconnected {
            reason: disconnect_reason.clone(),
        },
    );
    // A drop while you play may dial again, see `reconnect`.
    super::reconnect::after_drop(&conn.app, &conn.session, disconnect_reason.as_deref(), link)
        .await;
}

/// Who a line comes from.
enum From<'a> {
    /// You typed it.
    Typed,
    /// The writer sends it, the line without its line end, which echoes
    /// as a line you type does.
    Card(&'a str),
}

/// Send `bytes`, a line you typed with its line ends, or a line of the
/// writer's, to the game. A partial waiting for the next read paints
/// first, and the lines held for the rest of a prompt let go. A command
/// you type stops a walk and goes past the writer's hold, as `./` and
/// the line while the writer has the game's editor open. The writer's
/// line echoes. The send records a prompt candidate and closes the open
/// row, the partial goes, and the line joins the log. `masked` says you
/// typed it into the masked field. Returns the reason the connection
/// ends when the write fails.
async fn send_typed<R: tauri::Runtime>(
    conn: &mut Conn<R>,
    log_sink: &mut LogSink,
    hold_until: &mut Option<Instant>,
    bytes: &[u8],
    masked: bool,
    from: From<'_>,
) -> Result<(), String> {
    // A partial waiting for the next read paints before your line leaves,
    // so it never goes unseen.
    if hold_until.take().is_some() {
        flush_hold(conn).await;
    }
    // Lines held for the rest of a prompt let go as they show, before
    // your line leaves, since it follows them.
    if let Err(e) = let_go_held_lines(conn, log_sink).await {
        warn!(error = %e, "letting go of held lines failed");
    }
    let bytes = match from {
        From::Typed => {
            // A command you send stops a walk, and the walk says so under
            // the echo of your line.
            let stopped = conn.walker.typed(bytes);
            echo_lines(&conn.app, &conn.session, &stopped.lines);
            let out = conn.stream.lines_out() + super::socket::lines_in(bytes);
            conn.writer.typed(bytes, out)
        }
        From::Card(line) => {
            let echo = crate::input::command_echo(line, &conn.session.lock_profile().await.ui);
            echo_lines(&conn.app, &conn.session, &[echo]);
            bytes.to_vec()
        }
    };
    let bytes = bytes.as_slice();
    // The send records a prompt candidate and closes the open row. On a
    // server that sends no Char.Vitals it also starts the next pulse,
    // after which the values the last prompt set go stale.
    // Its rows are login outside play, so a name you type at the
    // account menu stays out of every scene.
    let (pulse, kind) = {
        let mut c = conn.session.connection.lock();
        let pulse = send_step(&mut c, &conn.accumulator, bytes, now_ms());
        let playing = c.log_kinds.in_play(c.link.playing());
        (pulse, c.log_kinds.sent(bytes, playing))
    };
    // The frontend already echoed the typed line inline with the
    // on-screen prompt. Drop the buffered partial so the next chunk from
    // the server starts fresh on a new row instead of merging with the
    // displayed prompt.
    conn.accumulator.forget_partial();
    if pulse {
        emit_hidden_change(&conn.app, &conn.session).await;
        emit_prompt_vars(&conn.app, &conn.session, false).await;
    }
    // The input line(s) go in the same log session as server output so
    // transcripts include both directions. While the server holds echo
    // (a password prompt), and for any line typed into the masked field,
    // each line is logged as `> (hidden)` and its text never reaches the
    // store. See `vosh_log::sent_rows`. The rows and their time are taken
    // as the line leaves, and they wait behind the rows before them for
    // the log, which writes them once the socket is quiet, so a log write
    // never holds your line or its answer back.
    let sent = log_sink.id().map(|sid| {
        let rows = vosh_log::sent_rows(bytes, conn.server_echo.hides(masked));
        (sid, now_ms(), rows)
    });
    let wrote = match conn.stream.write_now(bytes).await {
        Err(e) => Err(("write failed", e)),
        Ok(()) => conn.stream.flush().await.map_err(|e| ("flush failed", e)),
    };
    if let Some((sid, at, rows)) = sent {
        conn.settle
            .queue_rows(vosh_log::sent_entries(sid, at, rows, kind));
    }
    if let Err((what, e)) = wrote {
        error!(error = %e, "{what}");
        return Err(format!("{what}: {e}"));
    }
    // Lines sent back to back never wait on the log, but their rows still
    // go in once they waited too long.
    conn.settle
        .overdue_now(&conn.app, &conn.session, log_sink, &mut conn.perf);
    Ok(())
}

/// Hand the walker a `#walk` you typed, or Esc. The step goes out at
/// once. A line the walker prints follows the echo of your typed line,
/// and Esc echoes nothing, so its line starts a row of its own. What a
/// `#walk stop` or a bare `#walk` held is the rest of the line you typed,
/// so it runs right after and goes out through [`send_typed`] as a typed
/// line does. A `#walk` among it comes back here. Returns the reason the
/// connection ends when a write fails.
async fn walk_command<R: tauri::Runtime>(
    conn: &mut Conn<R>,
    log_sink: &mut LogSink,
    hold_until: &mut Option<Instant>,
    command: WalkCommand,
) -> Result<(), String> {
    let mut next = Some(command);
    while let Some(command) = next.take() {
        let key = matches!(command, WalkCommand::Stop { key: true, .. });
        let out = conn.walker.command(command, Instant::now());
        if !out.send.is_empty() {
            let wrote = match conn.stream.write_all(&out.send).await {
                Ok(()) => conn.stream.flush().await,
                Err(e) => Err(e),
            };
            if let Err(e) = wrote {
                error!(error = %e, "walk failed");
                return Err(format!("write failed: {e}"));
            }
        }
        if key {
            if !out.lines.is_empty() {
                emit_output(&conn.app, &conn.session, framed_echoes(&out.lines));
            }
        } else {
            echo_lines(&conn.app, &conn.session, &out.lines);
        }
        if out.release.is_empty() {
            continue;
        }
        let apply = walk::release(&conn.session, out.release).await;
        let Collected {
            bytes,
            echoes,
            walk,
        } = collect_script_result(&conn.app, &conn.session, apply).await;
        echo_lines(&conn.app, &conn.session, &echoes);
        if !bytes.is_empty() {
            send_typed(conn, log_sink, hold_until, &bytes, false, From::Typed).await?;
        }
        next = walk;
    }
    Ok(())
}

/// Send the lines the writer asked for, each as a line you type goes,
/// and hold the session's other sends while its job runs. Once the job
/// ends, what the hold kept goes out. Returns the reason the connection
/// ends when a write fails.
async fn send_writer<R: tauri::Runtime>(
    conn: &mut Conn<R>,
    log_sink: &mut LogSink,
    hold_until: &mut Option<Instant>,
) -> Result<(), String> {
    if conn.writer.holds() {
        conn.stream.hold();
    }
    for line in std::mem::take(&mut conn.writer_send) {
        let bytes = format!("{line}\r\n");
        send_typed(
            conn,
            log_sink,
            hold_until,
            bytes.as_bytes(),
            false,
            From::Card(&line),
        )
        .await?;
    }
    conn.writer.settle();
    if !conn.writer.holds() {
        if let Err(e) = conn.stream.release().await {
            error!(error = %e, "sending what the writer held failed");
            return Err(format!("write failed: {e}"));
        }
    }
    Ok(())
}

async fn handle_tick<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stream: &mut Stream,
    walker: &mut Walker,
    session: &Arc<Session>,
) -> std::io::Result<()> {
    // Take the firing decision under the locks, then run the Send each
    // tick command, if the timer fired, after releasing them.
    let step = {
        let p = session.lock_profile().await;
        let mut c = session.connection.lock();
        c.tick.poll(&p.tick, Instant::now())
    };
    if !step.payload.enabled && !step.payload.fired {
        return Ok(());
    }
    if let Some(text) = &step.warn_echo {
        emit_output(app, session, text.clone().into_bytes());
    }
    deliver_tick_step(
        app,
        &mut ScriptIo::Session(stream, &mut OutputSink::Direct, walker),
        session,
        step,
    )
    .await
}

/// Fire the Settings interval timers whose deadline has elapsed, the
/// ones [`due_settings_timers`] picks. Each due command runs through the
/// same path as the tick auto-fire: `input::process`, echo its lines,
/// send its bytes.
async fn fire_due_settings_timers<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stream: &mut Stream,
    walker: &mut Walker,
    session: &Arc<Session>,
    timer_next: &mut HashMap<u32, Instant>,
) -> std::io::Result<()> {
    let due = due_settings_timers(&*session.lock_profile().await, timer_next, Instant::now());
    let mut sink = OutputSink::Direct;
    let mut io = ScriptIo::Session(stream, &mut sink, walker);
    for command in due {
        run_fired_command(app, &mut io, session, &command).await?;
    }
    Ok(())
}

/// The commands of the Settings interval timers due at `now`, in list
/// order. `timer_next` maps timer id to its next-fire `Instant`; a timer
/// is seeded on first sight (scheduled one interval out, not fired
/// immediately) and advanced past any missed slots so a stall never
/// burst-fires. A timer that is off, in a group that is off, or deleted
/// drops its deadline, so it starts a fresh interval when it comes back.
pub(super) fn due_settings_timers(
    p: &Profile,
    timer_next: &mut HashMap<u32, Instant>,
    now: Instant,
) -> Vec<String> {
    let live: HashSet<u32> = p
        .timers
        .iter()
        .filter(|t| p.timer_fires(t))
        .map(|t| t.id)
        .collect();
    timer_next.retain(|id, _| live.contains(id));
    let mut due = Vec::new();
    for t in p
        .timers
        .iter()
        .filter(|t| p.timer_fires(t) && !t.command.is_empty())
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
}

/// You are selecting text or reading back in the terminal. The webview
/// says so for xterm (`terminal_reader_busy`), and the session's native
/// grid holds its own selection and scroll.
fn reader_busy(session: &Session) -> bool {
    let webview = session
        .reader_busy
        .load(std::sync::atomic::Ordering::Acquire);
    webview || native_reader_busy(session)
}

#[cfg(any(native_surface, test))]
fn native_reader_busy(session: &Session) -> bool {
    crate::native::grid::reader_busy(session.id)
}

#[cfg(not(any(native_surface, test)))]
fn native_reader_busy(_session: &Session) -> bool {
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

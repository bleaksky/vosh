//! The socket read path. [`Conn::handle_read`] takes the bytes of each
//! read, from the loop or from the drain after a read error, through the
//! telnet parser. Each telnet event of a read runs through the line
//! steps, and what a step leaves for after the profile lock goes out from
//! here, with its routes, its scrollback, what its triggers send
//! and the IO its Lua asks for. The end of the read sends what the read
//! gathered, once. A partial that waited for the next read, and the lines
//! held for the rest of a prompt, go out the same way.

use std::sync::Arc;

use tauri::AppHandle;
use tokio::time::Instant;
use tracing::warn;
use vosh_automation::trigger::LineResult;
use vosh_prompt::stage::Output;
use vosh_protocol::telnet::{codes as telnet_codes, option as telnet_option, Event as TelnetEvent};

use crate::app::events;
use crate::profile::open::OpenProfile;
use crate::prompt::report_game_prompt_seen;
use crate::sessions::Session;

use super::batch::{emit_session_output, ReadBatch};
use super::conn::Conn;
use super::effects::{apply_script_result, deliver_tick_step, framed_echoes, OutputSink, ScriptIo};
use super::gmcp::{handle_gmcp, hello_subnegotiation, room_chars_ahead, supports_subnegotiation};
use super::log_sink::LogSink;
use super::prompt_view::{emit_prompt_state, send_prompt_vars, watching_prompt};
use super::reader::ReaderFeed;
use super::round_trip::ends_on_prompt;
use super::socket::Stream;
use super::steps::{
    clock_after, hold_step, let_go_held, line_step, marker_step, partial_step, reader_wait_step,
    LineStep,
};
use super::walk::{self, WalkOut};
use super::writer::game_text::GameLine;
use super::{emit_input_mode, GagWithoutReaderPayload, RoutedPayload};

pub(super) const READ_BUFFER_BYTES: usize = 8 * 1024;

impl<R: tauri::Runtime> Conn<R> {
    /// Take the bytes of one socket read through the parser, each telnet
    /// event they hold through [`handle_event`] in order, and the end of
    /// the read through [`end_read`]. Returns the read's batch, which the
    /// caller finishes, since the drain after a read error ends a read
    /// apart from the loop.
    pub(super) async fn handle_read(&mut self, bytes: &[u8], log_sink: &LogSink) -> ReadBatch {
        self.perf.socket_reads += 1;
        self.perf.bytes_in += bytes.len() as u64;
        let events = self.parser.feed(bytes);
        let mut batch = ReadBatch::new(self.others_wrote());
        // The text of the read, for the writer, which counts a `> ` that
        // came alone in it.
        let mut text = Vec::new();
        // Whether the read brought any text, which the writer's prompt
        // tick waits for.
        let mut data_seen = false;
        // The Room.Chars each event finds ahead in the read, so the people
        // of a look whose packets follow it read their places before they
        // show.
        let ahead = room_chars_ahead(&events);
        for (i, event) in events.into_iter().enumerate() {
            batch.room_ahead = ahead.get(i).cloned().flatten();
            if let TelnetEvent::Data(data) = &event {
                data_seen |= !data.is_empty();
                if self.writer.watching() {
                    text.extend_from_slice(data);
                }
            }
            if let Err(e) = handle_event(self, log_sink, event, &mut batch).await {
                warn!(error = %e, "event handling failed");
                break;
            }
        }
        // Where the game takes your input now, from the partial the read
        // ended on, before the prompt step takes it.
        let partial = self
            .accumulator
            .partial()
            .map(vosh_protocol::ansi::plain_text)
            .unwrap_or_default();
        let text = vosh_protocol::ansi::plain_text(&text);
        let send = self.writer.read_end(
            &text,
            data_seen,
            &partial,
            self.stream.lines_out(),
            Instant::now(),
        );
        self.writer_send.extend(send);
        if let Err(e) = end_read(self, log_sink, &mut batch).await {
            warn!(error = %e, "prompt handling at the end of a read failed");
        }
        // What the walker said in this read shows after it, below the
        // prompt that ends it.
        if !self.walk_lines.is_empty() {
            let lines = std::mem::take(&mut self.walk_lines);
            batch.out.text(&framed_echoes(&lines));
        }
        batch
    }
}

async fn handle_event<R: tauri::Runtime>(
    conn: &mut Conn<R>,
    log_sink: &LogSink,
    event: TelnetEvent,
    batch: &mut ReadBatch,
) -> std::io::Result<()> {
    // WILL ECHO means the server takes over echoing what you type, which
    // ROM derivatives do for a password prompt. WONT ECHO hands echo back.
    // Note it before anything else in the event runs, then tell the
    // frontend to mask or unmask the input row. The negotiation reply
    // goes out through the catch all arm below.
    if let Some(held) = conn.server_echo.observe(&event) {
        emit_input_mode(&conn.app, &conn.session, held);
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
            //
            // While a step is in flight, the walker reads each line once
            // it shows, whatever its triggers do to it. The first line
            // ends the partial the read found, which can be a prompt the
            // answer to the step runs on from, so the walker reads what
            // follows it.
            let mut partial = conn
                .accumulator
                .partial()
                .filter(|_| conn.walker.watching())
                .map(vosh_protocol::ansi::plain_text);
            for line in conn.accumulator.feed(&bytes) {
                conn.perf.lines_processed += 1;
                let plain = vosh_protocol::ansi::plain_text(&line.bytes);
                let ended = partial.take();
                let watched = conn
                    .walker
                    .watching()
                    .then(|| walk::answer(&plain, ended.as_deref()).to_string());
                conn.writer.heard(&plain);
                if conn.writer.watching() {
                    let out = conn.stream.lines_out();
                    conn.writer.line(&GameLine::new(&plain, &line.bytes), out);
                }
                let trigger_t0 = std::time::Instant::now();
                // The tick step for a line that matches the Reset on
                // pattern comes under the same locks as the triggers and
                // Lua, so reading the tick takes no lock of its own. The
                // line is the game's tick, so the step fires once per
                // tick and carries the Send each tick command to run
                // after the locks drop.
                let (open, steps) = {
                    let lock_t0 = std::time::Instant::now();
                    let mut p = conn.session.lock_profile().await;
                    conn.perf.mutex_wait_ns += lock_t0.elapsed().as_nanos() as u64;
                    conn.perf.mutex_acquires += 1;
                    let mut c = conn.session.connection.lock();
                    let steps = line_step(
                        &mut p,
                        &mut c,
                        batch,
                        line,
                        plain,
                        Instant::now(),
                        log_sink.id(),
                    );
                    (p.open().clone(), steps)
                };
                conn.perf.trigger_lua_ns += trigger_t0.elapsed().as_nanos() as u64;
                for step in steps {
                    deliver_line_step(conn, log_sink, batch, &open, step).await?;
                }
                if let Some(plain) = watched {
                    let out = conn.walker.line(&plain, Instant::now());
                    walked(conn, out, batch).await?;
                }
            }
            // Text after the last line end is the game's prompt, waiting
            // for your next command. The note editor ends on a line end
            // and answers no line of the note, so what you write there
            // never reads as a stall.
            let prompted = ends_on_prompt(conn.accumulator.partial());
            conn.stream.game_prompted(prompted);
            Ok(())
        }
        TelnetEvent::Subnegotiation { option, payload } if option == telnet_option::GMCP => {
            conn.perf.gmcp_packets += 1;
            batch.gmcp = true;
            handle_gmcp(conn, log_sink.id(), &payload, batch).await?;
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
            let (open, steps) = {
                let mut p = conn.session.lock_profile().await;
                let mut c = conn.session.connection.lock();
                let steps = marker_step(
                    &mut p,
                    &mut c,
                    &mut conn.accumulator,
                    batch,
                    Instant::now(),
                    log_sink.id(),
                );
                (p.open().clone(), steps)
            };
            for step in steps {
                deliver_line_step(conn, log_sink, batch, &open, step).await?;
            }
            conn.stream.game_prompted(true);
            conn.writer.marker();
            Ok(())
        }
        TelnetEvent::Will(opt) if opt == telnet_option::GMCP => {
            // Accept GMCP via the negotiator, then immediately announce
            // ourselves and the packages we want. A WILL GMCP once it is
            // on gets no answer and no second hello.
            let was_on = conn.negotiator.server_does(opt);
            let response = conn.negotiator.handle(&TelnetEvent::Will(opt));
            conn.stream.write_all(&response).await?;
            if !was_on && conn.negotiator.server_does(opt) {
                // The game can now say when you play, so the rows before
                // that are outside it.
                conn.session.connection.lock().log_kinds.gmcp_on();
                conn.stream.write_all(&hello_subnegotiation()).await?;
                conn.stream.write_all(&supports_subnegotiation()).await?;
            }
            conn.stream.flush().await?;
            Ok(())
        }
        other => {
            let response = conn.negotiator.handle(&other);
            if !response.is_empty() {
                conn.stream.write_all(&response).await?;
                conn.stream.flush().await?;
            }
            Ok(())
        }
    }
}

/// Paint a partial that waited and send it out. `seen_output` becomes
/// the output count after it.
pub(super) async fn flush_hold<R: tauri::Runtime>(conn: &mut Conn<R>) {
    let out = {
        let mut c = conn.session.connection.lock();
        let mut out = Output::new(conn.others_wrote());
        hold_step(&mut c, &mut conn.accumulator, &mut out);
        out
    };
    if !out.is_empty() {
        conn.seen_output = emit_session_output(&conn.app, &conn.session, &out, &mut conn.settle);
    }
}

/// Send what a screen reader reads of a partial that waited
/// [`super::reader::PARTIAL_WAIT`].
pub(super) async fn flush_reader_wait<R: tauri::Runtime>(conn: &mut Conn<R>) {
    let mut reader = ReaderFeed::default();
    hear_reader_wait(conn, &mut reader).await;
    super::reader::emit(&conn.app, &conn.session, reader);
}

/// Read a partial that waited into `reader`, through
/// [`reader_wait_step`], under the profile lock, which says whether a
/// screen reader reads it.
pub(super) async fn hear_reader_wait<R: tauri::Runtime>(
    conn: &mut Conn<R>,
    reader: &mut ReaderFeed,
) {
    let p = conn.session.lock_profile().await;
    let mut c = conn.session.connection.lock();
    reader_wait_step(&p, &mut c, &conn.accumulator, reader);
}

/// Let go of the lines the stage holds for the rest of a prompt, through
/// [`let_go_held`], and send what their Line pass left: the routes, the
/// scrollback, what their triggers send, and the log rows. `seen_output`
/// becomes the output count after it.
pub(super) async fn let_go_held_lines<R: tauri::Runtime>(
    conn: &mut Conn<R>,
    log_sink: &mut LogSink,
) -> std::io::Result<()> {
    let mut batch = ReadBatch::new(conn.others_wrote());
    let (open, steps) = {
        let mut p = conn.session.lock_profile().await;
        let mut c = conn.session.connection.lock();
        if !c.prompt.stage.holds() {
            return Ok(());
        }
        let steps = let_go_held(&mut p, &mut c, &mut batch, Instant::now(), log_sink.id());
        (p.open().clone(), steps)
    };
    for step in steps {
        deliver_line_step(conn, log_sink, &mut batch, &open, step).await?;
    }
    finish_read(conn, log_sink, batch).await;
    Ok(())
}

/// Do what a line step left for after the lock of `open`, the profile it
/// ran under: emit its routes, keep it for scrollback, send what its
/// triggers send, apply its Lua actions' IO into the batch, and run the
/// tick command it fired.
async fn deliver_line_step<R: tauri::Runtime>(
    conn: &mut Conn<R>,
    log_sink: &LogSink,
    batch: &mut ReadBatch,
    open: &Arc<OpenProfile>,
    step: LineStep,
) -> std::io::Result<()> {
    let LineStep {
        result,
        apply,
        tick_step,
        scrollback: kept,
        repeat,
    } = step;
    if !result.routes.is_empty() {
        conn.perf.routed_emits += result.routes.len() as u64;
    }
    emit_line_routes(&conn.app, &conn.session, &result);
    for text in kept {
        let sb_t0 = std::time::Instant::now();
        // The ring keeps a run of repeated lines once, as the screen shows
        // it.
        log_sink.scrollback.lock().await.keep(text, repeat);
        conn.perf.scrollback_push_ns += sb_t0.elapsed().as_nanos() as u64;
        conn.perf.scrollback_pushes += 1;
    }
    send_trigger_outputs(&mut conn.stream, &result.sends).await?;
    let mut sink = OutputSink::Batch(batch);
    let mut io = ScriptIo::Session(&mut conn.stream, &mut sink, &mut conn.walker);
    apply_script_result(&conn.app, &mut io, &conn.session, apply.ran_under(open)).await?;
    if let Some(step) = tick_step {
        conn.perf.ticks += 1;
        deliver_tick_step(&conn.app, &mut io, &conn.session, step).await?;
    }
    Ok(())
}

/// Do what the walker asked for in a read: keep its lines for the end of
/// the read, run what a walk held once you arrive, as its line would have
/// run it, and send its step at once. What an arrived walk held goes
/// before the first step of a walk that takes over from it, so it acts in
/// the room the walk reached.
pub(super) async fn walked<R: tauri::Runtime>(
    conn: &mut Conn<R>,
    out: WalkOut,
    batch: &mut ReadBatch,
) -> std::io::Result<()> {
    let WalkOut {
        send,
        lines,
        release,
    } = out;
    conn.walk_lines.extend(lines);
    if !release.is_empty() {
        let apply = walk::release(&conn.session, release).await;
        apply_script_result(
            &conn.app,
            &mut ScriptIo::Session(
                &mut conn.stream,
                &mut OutputSink::Batch(batch),
                &mut conn.walker,
            ),
            &conn.session,
            apply,
        )
        .await?;
    }
    if !send.is_empty() {
        conn.stream.write_all(&send).await?;
        conn.stream.flush().await?;
    }
    Ok(())
}

/// The end of a read, see [`partial_step`], and the IO its prompt left.
async fn end_read<R: tauri::Runtime>(
    conn: &mut Conn<R>,
    log_sink: &LogSink,
    batch: &mut ReadBatch,
) -> std::io::Result<()> {
    let (open, step) = {
        let mut p = conn.session.lock_profile().await;
        let mut c = conn.session.connection.lock();
        let step = partial_step(
            &mut p,
            &mut c,
            &mut conn.accumulator,
            batch,
            Instant::now(),
            log_sink.id(),
        );
        (p.open().clone(), step)
    };
    if let Some(step) = step {
        deliver_line_step(conn, log_sink, batch, &open, step).await?;
    }
    Ok(())
}

/// Send what one socket read gathered: its output, what a screen reader
/// reads of it, the triggers that hid
/// a prompt with nothing to draw in its place, then the prompt vars when
/// a prompt was read or they changed, what the plugins changed in their
/// panes, the snoops, and the hidden state when it changed. Once per read, so the
/// packets of one pulse never show the panes a state between them. Its frame and its log rows wait in
/// `settle` for the end of the burst of reads. `seen_output` becomes the
/// output count after this read's output. Returns when a clock piece in
/// your design next shows another second, which the lock this takes
/// anyway reads, so a read costs no other lock for it.
pub(super) async fn finish_read<R: tauri::Runtime>(
    conn: &mut Conn<R>,
    log_sink: &mut LogSink,
    batch: ReadBatch,
) -> Option<Instant> {
    let ReadBatch {
        mut out,
        log,
        prompt_vars,
        lua_panes,
        snoop,
        prompt,
        gag_without_reader,
        character,
        hold: _,
        reader_wait: _,
        gmcp,
        since_prompt: _,
        reader,
        room_ahead: _,
    } = batch;
    let (app, session) = (&conn.app, &conn.session);
    let watched = prompt && watching_prompt(session);
    let (open, vars, panes, snoops, hidden, prompt_seen, status, prompt_state, clock, rings) = {
        let p = conn.session.lock_profile().await;
        let mut c = session.connection.lock();
        // Echoes the end of the read wrote close the open row.
        c.prompt.stage.finish(&mut out);
        let vars = c.prompt.take_prompt_vars(prompt_vars);
        let panes = lua_panes.then(|| c.lua_panes.take_changes()).flatten();
        let snoops = snoop.then(|| c.snoops.take_changes());
        let hidden = c.prompt.vars.take_hidden_change();
        // Low health follows what the vitals panes read once the read's
        // packets and prompt values landed, whether its alert is on or
        // off, since it marks a row either way.
        let follow = gmcp || prompt_vars || hidden.is_some();
        let mut rings: Vec<crate::alert::Alert> = follow
            .then(|| crate::alert::presets::health(&c.prompt.vars))
            .flatten()
            .and_then(|(hp, maxhp, hid)| c.preset_watch.health(&p, hp, maxhp, hid))
            .into_iter()
            .collect();
        // The first text of a link a redial opened is the game's prompt,
        // which waits for your login.
        if out.writes_text() {
            rings.extend(super::reconnect::reached_prompt(session, &p));
        }
        (
            p.open().clone(),
            vars,
            panes,
            snoops,
            hidden,
            c.prompt.take_seen(),
            c.prompt.take_status_change(),
            watched.then(|| crate::prompt::prompt_state(&p, &c)),
            clock_after(&p, &c, Instant::now()),
            rings,
        )
    };
    if !out.is_empty() {
        conn.perf.output_emits += 1;
        conn.perf.output_emit_bytes +=
            (out.bytes.len() + out.hold.len() + out.replace.as_ref().map_or(0, |r| r.bytes.len()))
                as u64;
        conn.seen_output = emit_session_output(app, session, &out, &mut conn.settle);
    }
    super::reader::emit(app, session, reader);
    conn.settle.queue_rows(log);
    if let Some(named) = character.and_then(|character| log_sink.name(&character)) {
        conn.settle.queue_name(named);
    }
    for trigger in gag_without_reader {
        session.emit(
            app,
            events::PROMPT_GAG_WITHOUT_READER,
            &GagWithoutReaderPayload { trigger },
        );
    }
    if let Some(vars) = vars {
        send_prompt_vars(app, session, &vars);
    }
    if let Some(panes) = panes {
        session.emit(app, events::LUA_PANES, &panes);
    }
    if let Some(snoops) = snoops {
        super::snoop::emit(app, session, snoops);
    }
    if let Some(hidden) = hidden {
        session.emit(app, events::HIDDEN, &hidden);
    }
    report_game_prompt_seen(app, session, &open, prompt_seen);
    if let Some(status) = status {
        session.emit(app, events::PROMPT_STATUS, &status);
    }
    emit_prompt_state(app, session, prompt_state);
    crate::alert::ring(app, session, rings);
    clock
}

/// Route emissions stay per-line because consumers (chat panel etc.)
/// expect one event per routed line. The volume here is tiny relative
/// to the display stream so per-event cost does not show up as lag.
fn emit_line_routes<R: tauri::Runtime>(app: &AppHandle<R>, session: &Session, result: &LineResult) {
    if let Some(text) = &result.display {
        for pane in &result.routes {
            session.emit(
                app,
                events::ROUTED,
                &RoutedPayload {
                    pane: pane.clone(),
                    text: text.clone(),
                },
            );
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

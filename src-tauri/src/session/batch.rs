//! The frame and the log rows a burst of reads owes, and the batch each read fills.

use std::time::Duration;

use tauri::AppHandle;
use tokio::time::Instant;
use tracing::warn;
use vosh_prompt::stage::Output;

use crate::output::{emit_counted, request_frame};
use crate::sessions::Session;

use super::log_sink::LogSink;
use super::perf::PerfCounters;
use super::reader::ReaderFeed;

/// Everything one socket read writes to the terminal and reports, kept
/// in stream order and sent once at the end of the read, so a prompt
/// that arrives in one read never flashes. The Line pass, the GMCP
/// handler's echoes and the GA path all write here.
pub(super) struct ReadBatch {
    /// The terminal output, with the regions the prompt stage marks.
    pub(super) out: Output,
    /// Log rows, written in one transaction once the socket is quiet.
    pub(super) log: Vec<vosh_log::LogEntry>,
    /// Where the rows since the last prompt or GA start in `log`, for a
    /// Comm.Channel packet that comes after its line.
    pub(super) since_prompt: usize,
    /// A prompt var changed or a prompt was read, so the prompt vars go
    /// out after the output even when they read the same.
    pub(super) prompt_vars: bool,
    /// A plugin changed its panes, so what changed goes out once after
    /// the output.
    pub(super) lua_panes: bool,
    /// A Snoop packet came, so the tab list and the new text go out once
    /// after the output.
    pub(super) snoop: bool,
    /// Vosh read your prompt in this read, so the prompt state goes out
    /// after it while the card watches.
    pub(super) prompt: bool,
    /// Triggers that hid a prompt while nothing reads it, each named
    /// once a session.
    pub(super) gag_without_reader: Vec<String>,
    /// The character Char.Status named in this read, for the log's
    /// session row.
    pub(super) character: Option<String>,
    /// The read ended on a partial that can still become your prompt, so
    /// it waits a moment for the next read instead of painting raw.
    pub(super) hold: bool,
    /// The read painted a partial raw while Read new game lines is on, so
    /// a screen reader reads it if it is still there
    /// [`super::reader::PARTIAL_WAIT`] later.
    pub(super) reader_wait: bool,
    /// The read brought GMCP packets after the last prompt Vosh read in
    /// it, which can change what that prompt shows. Packets before a
    /// prompt in the same read draw with it.
    pub(super) gmcp: bool,
    /// What a screen reader reads of the read, filled only while Read new
    /// game lines is on.
    pub(super) reader: ReaderFeed,
}

impl ReadBatch {
    /// A batch for the next read. `closed` says output from elsewhere
    /// landed since the session last wrote, which closes the open row.
    pub(super) fn new(closed: bool) -> Self {
        Self {
            out: Output::new(closed),
            log: Vec::new(),
            since_prompt: 0,
            prompt_vars: false,
            lua_panes: false,
            snoop: false,
            prompt: false,
            gag_without_reader: Vec::new(),
            character: None,
            hold: false,
            reader_wait: false,
            gmcp: false,
            reader: ReaderFeed::default(),
        }
    }
}

/// How long a burst of reads can go on with no frame, and how long a log
/// row waits in it. A game that never pauses still shows its output and
/// gets its rows written this often, while the log is free.
pub(super) const FRAME_BUDGET: Duration = Duration::from_millis(16);

/// What the reads of a burst owe once the socket has nothing more for
/// now: one frame for all their output, then their log rows and the rows
/// of the lines you sent between them, in the order they passed. An
/// answer the game's writes cut into reads, such as a prompt whose GA
/// comes in the next read, shows in one frame, and the log write never
/// sits between those reads or ahead of the frame. A burst that never
/// ends still owes the frame once output waited [`FRAME_BUDGET`] for it,
/// and the rows once the oldest of them waited as long, drawn or not.
#[derive(Default)]
pub(super) struct Settle {
    /// Output went to the grid since the last frame was asked for.
    pub(super) frame: bool,
    /// When the first output with no frame yet went to the grid.
    since: Option<Instant>,
    /// Log rows waiting for the log, oldest first.
    pub(super) log: Vec<vosh_log::LogEntry>,
    /// The log's row and the character Char.Status first named, waiting
    /// for the log with the rows.
    name: Option<(i64, String)>,
    /// When the oldest row or name waiting for the log joined the queue.
    log_since: Option<Instant>,
}

impl Settle {
    /// Output went to the grid.
    pub(super) fn drew(&mut self) {
        self.frame = true;
        self.since.get_or_insert_with(Instant::now);
    }

    /// Rows join the queue for the log, behind the rows before them.
    pub(super) fn queue_rows(&mut self, rows: impl IntoIterator<Item = vosh_log::LogEntry>) {
        self.log.extend(rows);
        if !self.log.is_empty() {
            self.log_since.get_or_insert_with(Instant::now);
        }
    }

    /// The character to name on the log's row joins the queue.
    pub(super) fn queue_name(&mut self, named: (i64, String)) {
        self.name = Some(named);
        self.log_since.get_or_insert_with(Instant::now);
    }

    /// Whether rows or a name wait for the log.
    pub(super) fn owes_log(&self) -> bool {
        !self.log.is_empty() || self.name.is_some()
    }

    /// Ask for the frame the output of `session` so far owes, if any.
    pub(super) fn frame_now<R: tauri::Runtime>(&mut self, app: &AppHandle<R>, session: &Session) {
        if std::mem::take(&mut self.frame) {
            self.since = None;
            request_frame(app, session);
        }
    }

    /// Whether the burst went on so long it shows a frame now, before
    /// the socket runs dry.
    pub(super) fn frame_overdue(&self) -> bool {
        self.since.is_some_and(|t| t.elapsed() >= FRAME_BUDGET)
    }

    /// Whether rows waited so long they go in the log now, before the
    /// socket runs dry, whether or not anything drew.
    pub(super) fn log_overdue(&self) -> bool {
        self.log_since.is_some_and(|t| t.elapsed() >= FRAME_BUDGET)
    }

    /// What a game that never pauses is owed before the socket runs dry:
    /// the frame once output waited [`FRAME_BUDGET`] for it, and the rows
    /// once the oldest waited as long, even when nothing drew, such as
    /// the row of a line you sent or reads of GMCP alone. A busy log
    /// keeps the rows for the next quiet moment, so the loop never waits
    /// on it here.
    pub(super) fn overdue_now<R: tauri::Runtime>(
        &mut self,
        app: &AppHandle<R>,
        session: &Session,
        log_sink: &LogSink,
        perf: &mut PerfCounters,
    ) {
        if self.frame_overdue() {
            self.frame_now(app, session);
        }
        if self.log_overdue() {
            if let Ok(mut guard) = log_sink.logs.try_lock() {
                self.write_log(guard.as_mut(), perf);
            }
        }
    }

    /// Name the character on the log's row, then write the waiting rows
    /// to the log, in one transaction.
    pub(super) fn write_log(
        &mut self,
        store: Option<&mut vosh_log::LogStore>,
        perf: &mut PerfCounters,
    ) {
        self.log_since = None;
        let name = self.name.take();
        let rows = std::mem::take(&mut self.log);
        let Some(store) = store else {
            return;
        };
        if let Some((id, character)) = name {
            if let Err(e) = store.set_session_character(id, &character) {
                warn!(error = %e, "failed to name the log session's character");
            }
        }
        if !rows.is_empty() {
            let append_t0 = std::time::Instant::now();
            perf.log_appends += rows.len() as u64;
            if let Err(e) = store.append_batch(&rows) {
                warn!(error = %e, "log append_batch failed");
            }
            perf.log_append_ns += append_t0.elapsed().as_nanos() as u64;
        }
    }
}

/// Send one read's output in `session`. Returns the session's output
/// count after it. It asks for no frame, since the session asks for one
/// through `settle` when the burst of reads it came in ends.
pub(super) fn emit_session_output<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: &Session,
    out: &Output,
    settle: &mut Settle,
) -> u64 {
    settle.drew();
    emit_counted(app, session, out, true, true, false)
}

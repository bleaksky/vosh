//! Your logs. This module keeps the session log and the scrollback ring,
//! [`forget_passwords`] blanks the lines in that log where you sent a
//! password, [`retention`] deletes the logs past Keep logs for,
//! [`scene`] saves a stretch of one log as a scene to share, and [`file`]
//! saves what the log view reads as a file.
//!
//! The shared log store wraps `vosh_log::LogStore` in an async mutex so
//! the session `io_loop`, the search commands, and the scrollback flush
//! path can all reach the same `SQLite` handle.
//!
//! The ring buffer holds the most recent terminal lines and survives
//! across runs as a plain text scrollback file.

pub(crate) mod file;
pub(crate) mod forget_passwords;
pub(crate) mod retention;
pub(crate) mod scene;

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::Mutex;

pub(crate) type SharedLogStore = Arc<Mutex<Option<vosh_log::LogStore>>>;

/// The lines the ring keeps until the session says, the default
/// Scrollback size.
const SCROLLBACK_CAP: usize = crate::profile::ui::DEFAULT_SCROLLBACK_LINES as usize;

/// A line the ring keeps that Collapse repeated lines made something of:
/// whether it starts a run of repeated lines or joins the run the screen
/// ends on, and the region the run shows in on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KeptRun {
    pub repeat: vosh_prompt::stage::Repeat,
    pub gen: u64,
}

/// Where the ring keeps the run of repeated lines the screen ends on: how
/// many lines it kept after the run's line, and the region the run shows
/// in on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RingRun {
    after: usize,
    gen: u64,
}

/// The bytes of a ring at one moment, numbered in the order the copies
/// were taken, so a later copy always wins on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Snapshot {
    seq: u64,
    bytes: Vec<u8>,
}

/// The number the next [`Snapshot`] takes.
static NEXT_SNAPSHOT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub(crate) struct Scrollback {
    lines: VecDeque<Vec<u8>>,
    /// The run of repeated lines the screen ends on, while Collapse
    /// repeated lines is on.
    run: Option<RingRun>,
    /// The ring changed since its file was last written.
    changed: bool,
    /// The most lines the ring keeps, Scrollback size (D40).
    cap: usize,
}

impl Default for Scrollback {
    fn default() -> Self {
        Self {
            lines: VecDeque::new(),
            run: None,
            changed: false,
            cap: SCROLLBACK_CAP,
        }
    }
}

impl Scrollback {
    /// Keep at most `lines`, dropping the oldest past it now.
    pub(crate) fn set_cap(&mut self, lines: usize) {
        self.cap = lines.max(1);
        while self.lines.len() > self.cap {
            self.lines.pop_front();
            self.changed = true;
        }
        if self.run.is_some_and(|run| run.after >= self.lines.len()) {
            self.run = None;
        }
    }

    /// Forget every line, for Clear scrollback. The next launch restores
    /// nothing from before it.
    pub(crate) fn clear(&mut self) {
        self.lines.clear();
        self.run = None;
        self.changed = true;
    }

    /// What the scrollback file should hold when the ring changed since
    /// the last call, see [`Scrollback::snapshot`].
    pub(crate) fn take_changed(&mut self) -> Option<Snapshot> {
        self.changed.then(|| self.snapshot())
    }

    /// What the scrollback file should hold now, see
    /// [`Scrollback::dump`]. The ring reads as unchanged after it.
    pub(crate) fn snapshot(&mut self) -> Snapshot {
        self.changed = false;
        Snapshot {
            seq: NEXT_SNAPSHOT.fetch_add(1, Ordering::Relaxed),
            bytes: self.dump(),
        }
    }

    /// Say the file is behind the ring again, after a write that failed,
    /// so the next pass tries it again.
    pub(crate) fn mark_changed(&mut self) {
        self.changed = true;
    }

    pub(crate) fn push(&mut self, raw_line: Vec<u8>) {
        self.changed = true;
        if self.lines.len() >= self.cap {
            self.lines.pop_front();
            // The run's line was the oldest, and left with it.
            if self.run.is_some_and(|run| run.after + 1 >= self.cap) {
                self.run = None;
            }
        }
        self.lines.push_back(raw_line);
        if let Some(run) = self.run.as_mut() {
            run.after += 1;
        }
    }

    /// Keep `line` as Collapse repeated lines made of it on screen: on its
    /// own while it is off, as the start of a run, or as the run's line
    /// with its count once a line joined the run.
    pub(crate) fn keep(&mut self, line: Vec<u8>, run: Option<KeptRun>) {
        match run {
            None => self.push(line),
            Some(KeptRun {
                repeat: vosh_prompt::stage::Repeat::Starts,
                gen,
            }) => self.start_run(line, gen),
            Some(KeptRun {
                repeat: vosh_prompt::stage::Repeat::Joins(_),
                gen,
            }) => self.join_run(line, gen),
        }
    }

    /// Keep `line`, the first of a run of repeated lines that shows in
    /// region `gen`, and note it, so the lines that join the run take its
    /// place.
    pub(crate) fn start_run(&mut self, line: Vec<u8>, gen: u64) {
        self.run = None;
        self.push(line);
        self.run = Some(RingRun { after: 0, gen });
    }

    /// Keep `line`, the run of repeated lines with its count, now in
    /// region `gen`, in place of the run's line, as the screen shows the
    /// run once. Lines kept after the run's line never showed after it on
    /// screen, such as a pinned prompt, or the run would have ended. The
    /// run's line goes, and `line` comes after them, so the ring keeps the
    /// order the run's last line came in. A run whose line already left
    /// the ring starts again.
    pub(crate) fn join_run(&mut self, line: Vec<u8>, gen: u64) {
        if let Some(run) = self.run.take() {
            if let Some(at) = self.lines.len().checked_sub(run.after + 1) {
                self.lines.remove(at);
            }
        }
        self.start_run(line, gen);
    }

    /// The kept lines, oldest first, colors included.
    pub(crate) fn lines(&self) -> impl Iterator<Item = &[u8]> {
        self.lines.iter().map(Vec::as_slice)
    }

    /// Concatenate the buffered lines into one byte stream suitable for
    /// writing to disk or replaying into the terminal. Each line is
    /// separated by `\r\n`.
    pub(crate) fn dump(&self) -> Vec<u8> {
        let total: usize = self.lines.iter().map(|l| l.len() + 2).sum();
        let mut out = Vec::with_capacity(total);
        for line in &self.lines {
            out.extend_from_slice(line);
            out.extend_from_slice(b"\r\n");
        }
        out
    }

    /// The kept lines as [`Scrollback::dump`] writes them, for a terminal
    /// that loads them while the session runs, such as the history of the
    /// split. While the run of repeated lines the screen ends on is the
    /// last line kept, the mark of its region goes before it, so the
    /// terminal finds the run open and the next line that joins it writes
    /// the count in place. The file the next launch restores never holds
    /// a mark, since its regions mean nothing to another session.
    pub(crate) fn dump_live(&self) -> Vec<u8> {
        let (Some(RingRun { after: 0, gen }), Some(last)) = (self.run, self.lines.back()) else {
            return self.dump();
        };
        let mark = vosh_prompt::stage::mark(gen);
        let total: usize = self.lines.iter().map(|l| l.len() + 2).sum();
        let mut out = Vec::with_capacity(total + mark.len());
        for line in self.lines.range(..self.lines.len() - 1) {
            out.extend_from_slice(line);
            out.extend_from_slice(b"\r\n");
        }
        out.extend(mark);
        out.extend_from_slice(last);
        out.extend_from_slice(b"\r\n");
        out
    }

    /// Read the lines of a scrollback file. The file already holds
    /// them, so the ring reads as unchanged.
    pub(crate) fn load_from_bytes(&mut self, bytes: &[u8]) {
        self.lines.clear();
        self.run = None;
        // Keep empty intermediate rows so blank lines in the original
        // output show up again on restore. Splitting on \n always yields
        // one trailing empty chunk after a final \n; that one is an
        // artifact of the encoding and gets dropped.
        let parts: Vec<&[u8]> = bytes.split(|b| *b == b'\n').collect();
        let n = parts.len();
        for (i, chunk) in parts.iter().enumerate() {
            if i == n - 1 && chunk.is_empty() {
                continue;
            }
            let trimmed = if chunk.last() == Some(&b'\r') {
                &chunk[..chunk.len() - 1]
            } else {
                *chunk
            };
            self.push(trimmed.to_vec());
        }
        self.changed = false;
    }
}

pub(crate) type SharedScrollback = Arc<Mutex<Scrollback>>;

/// How often a running Vosh writes the scrollback that changed, so a
/// crash loses at most this much of it.
const SAVE_EVERY: std::time::Duration = std::time::Duration::from_secs(3 * 60);

/// The snapshot each scrollback file last took. Its lock lets one write
/// run at a time, so writers never meet on a file.
static WRITTEN: std::sync::Mutex<BTreeMap<PathBuf, u64>> = std::sync::Mutex::new(BTreeMap::new());

/// Write `snapshot` to the scrollback file at `path` whole: to a file
/// beside it first, then over it in one step, so a crash mid write leaves
/// the last file as it was. A snapshot older than the one the file holds
/// writes nothing. False when the write failed, so the caller can mark
/// its ring changed and try again later.
pub(crate) fn write_scrollback(path: &Path, snapshot: &Snapshot) -> bool {
    let mut written = WRITTEN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if written.get(path).is_some_and(|&seq| seq >= snapshot.seq) {
        return true;
    }
    let mut next = path.as_os_str().to_owned();
    next.push(format!(".{}.next", snapshot.seq));
    let next = PathBuf::from(next);
    let result = std::fs::write(&next, &snapshot.bytes).and_then(|()| std::fs::rename(&next, path));
    match result {
        Ok(()) => {
            written.insert(path.to_path_buf(), snapshot.seq);
            true
        }
        Err(e) => {
            let _ = std::fs::remove_file(&next);
            tracing::warn!(path = %path.display(), error = %e, "scrollback write failed");
            false
        }
    }
}

/// Write the scrollback file of each session whose ring changed since its
/// file was last written, see [`save_scrollback`].
pub(crate) async fn save_changed_scrollback(state: &crate::app::state::SharedState) {
    save_scrollback(state, Scrollback::take_changed).await;
}

/// Write the scrollback file of each session `copy` takes a snapshot of.
/// The session loop never waits on it: each ring is locked only to copy
/// its bytes, and the writes run on the blocking pool. A ring whose write
/// failed reads as changed again.
async fn save_scrollback(
    state: &crate::app::state::SharedState,
    copy: impl Fn(&mut Scrollback) -> Option<Snapshot>,
) {
    let Some(dir) = state.app_data.get().cloned() else {
        return;
    };
    let mut files = Vec::new();
    for session in state.all_sessions() {
        let snapshot = copy(&mut *session.scrollback.lock().await);
        if let Some(snapshot) = snapshot {
            let path = crate::disk::paths::scrollback_path(&dir, session.id);
            files.push((session.scrollback.clone(), path, snapshot));
        }
    }
    if files.is_empty() {
        return;
    }
    let failed = tokio::task::spawn_blocking(move || {
        files
            .into_iter()
            .filter(|(_, path, snapshot)| !write_scrollback(path, snapshot))
            .map(|(ring, ..)| ring)
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();
    for ring in failed {
        ring.lock().await.mark_changed();
    }
}

/// Keep `lines` of scrollback in `session`: its ring, and so its file,
/// and its native grid. xterm follows the same field on the page (D40).
pub(crate) async fn keep_scrollback_lines(session: &crate::sessions::Session, lines: u32) {
    session.scrollback.lock().await.set_cap(lines as usize);
    #[cfg(any(native_surface, test))]
    crate::native::grid::set_history(session.id, lines as usize);
}

/// On the way out: write every scrollback ring, so the next launch shows
/// what you saw last, and end each log still open, since a quit while
/// connected never reaches the end of the session loop. Every ring takes
/// a fresh copy, so a three minute write still running when you quit is
/// waited on or skipped as older, never left to land after the exit. The
/// two run side by side, so a log store busy with a rebuild cannot hold
/// up the scrollback. A log the quit could not end, the launch sweep ends.
pub(crate) async fn on_quit(state: &crate::app::state::SharedState) {
    let end_logs = async {
        if let Some(store) = state.logs.lock().await.as_mut() {
            if let Err(e) = store.end_open_sessions(crate::session::now_ms()) {
                tracing::warn!(error = %e, "could not end the open logs on quit");
            }
        }
    };
    tokio::join!(
        save_scrollback(state, |ring| Some(ring.snapshot())),
        end_logs
    );
}

/// Write the scrollback that changed every few minutes, for as long as
/// Vosh runs, so a crash loses little of it.
pub(crate) fn start_saving_scrollback(state: &crate::app::state::SharedState) {
    let state = state.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(SAVE_EVERY).await;
            save_changed_scrollback(&state).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_evicts_oldest() {
        let mut s = Scrollback::default();
        for i in 0..(SCROLLBACK_CAP + 5) {
            s.push(format!("line {i}").into_bytes());
        }
        assert_eq!(s.lines.len(), SCROLLBACK_CAP);
        assert_eq!(s.lines.front().unwrap(), b"line 5");
        assert_eq!(
            s.lines.back().unwrap(),
            format!("line {}", SCROLLBACK_CAP + 4).as_bytes()
        );
    }

    #[test]
    fn a_smaller_size_drops_the_oldest_lines() {
        let mut s = Scrollback::default();
        for i in 0..10 {
            s.push(format!("line {i}").into_bytes());
        }
        let _ = s.take_changed();
        s.set_cap(4);
        assert_eq!(
            s.lines().collect::<Vec<_>>(),
            [&b"line 6"[..], b"line 7", b"line 8", b"line 9"]
        );
        assert!(s.take_changed().is_some());
        s.push(b"line 10".to_vec());
        assert_eq!(s.lines.len(), 4);
        assert_eq!(s.lines.front().unwrap(), b"line 7");
        s.set_cap(20_000);
        for i in 0..15_000 {
            s.push(format!("more {i}").into_bytes());
        }
        assert_eq!(s.lines.len(), 15_004);
    }

    #[test]
    fn the_ring_says_when_its_file_is_behind() {
        let mut s = Scrollback::default();
        assert_eq!(s.take_changed(), None);
        s.load_from_bytes(b"Maren waves.\r\n");
        assert_eq!(s.take_changed(), None, "the file holds what it read");
        s.push(b"Orla nods.".to_vec());
        assert_eq!(
            s.take_changed().map(|snapshot| snapshot.bytes).as_deref(),
            Some(&b"Maren waves.\r\nOrla nods.\r\n"[..])
        );
        assert_eq!(s.take_changed(), None);
        s.mark_changed();
        assert!(s.take_changed().is_some(), "a failed write tries again");
        s.clear();
        assert_eq!(
            s.take_changed().map(|snapshot| snapshot.bytes).as_deref(),
            Some(&b""[..])
        );
    }

    #[tokio::test]
    async fn a_quit_ends_the_open_log_and_writes_the_scrollback() {
        let dir = tempfile::tempdir().expect("a temporary folder");
        let state: crate::app::state::SharedState =
            Arc::new(crate::app::state::AppState::default());
        let _ = state.app_data.set(dir.path().to_path_buf());
        let mut store = vosh_log::LogStore::in_memory().unwrap();
        let open = store.start_session("h", 1, 0).unwrap();
        *state.logs.lock().await = Some(store);
        let session = state.selected_session();
        session
            .scrollback
            .lock()
            .await
            .push(b"Tolliver leaves north.".to_vec());

        on_quit(&state).await;
        let path = crate::disk::paths::scrollback_path(dir.path(), session.id);
        assert_eq!(std::fs::read(path).unwrap(), b"Tolliver leaves north.\r\n");
        let guard = state.logs.lock().await;
        let row = guard.as_ref().unwrap().get_session(open).unwrap().unwrap();
        assert!(row.ended_at_ms.is_some());
    }

    #[tokio::test]
    async fn a_quit_writes_the_scrollback_a_running_pass_already_took() {
        let dir = tempfile::tempdir().expect("a temporary folder");
        let state: crate::app::state::SharedState =
            Arc::new(crate::app::state::AppState::default());
        let _ = state.app_data.set(dir.path().to_path_buf());
        let session = state.selected_session();
        let mut ring = session.scrollback.lock().await;
        ring.push(b"Maren waves.".to_vec());
        // The three minute pass took this copy, and its write has not
        // landed when you quit.
        let in_flight = ring.take_changed().expect("the ring changed");
        drop(ring);

        on_quit(&state).await;
        let path = crate::disk::paths::scrollback_path(dir.path(), session.id);
        assert_eq!(std::fs::read(&path).unwrap(), b"Maren waves.\r\n");
        // The older copy landing late writes nothing.
        session.scrollback.lock().await.push(b"Orla nods.".to_vec());
        on_quit(&state).await;
        assert!(write_scrollback(&path, &in_flight));
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"Maren waves.\r\nOrla nods.\r\n"
        );
    }

    #[tokio::test]
    async fn a_busy_log_store_does_not_hold_up_the_scrollback_on_quit() {
        let dir = tempfile::tempdir().expect("a temporary folder");
        let state: crate::app::state::SharedState =
            Arc::new(crate::app::state::AppState::default());
        let _ = state.app_data.set(dir.path().to_path_buf());
        let session = state.selected_session();
        session
            .scrollback
            .lock()
            .await
            .push(b"Tolliver leaves north.".to_vec());
        // A rebuild holds the log store past the quit's time.
        let busy = state.logs.lock().await;

        let quit = tokio::time::timeout(std::time::Duration::from_millis(200), on_quit(&state));
        assert!(quit.await.is_err(), "the log end waits on the store");
        drop(busy);
        let path = crate::disk::paths::scrollback_path(dir.path(), session.id);
        assert_eq!(std::fs::read(path).unwrap(), b"Tolliver leaves north.\r\n");
    }

    #[test]
    fn a_scrollback_file_is_replaced_whole() {
        let dir = tempfile::tempdir().expect("a temporary folder");
        let path = dir.path().join("scrollback.txt");
        std::fs::write(&path, b"old\r\n").unwrap();
        let mut ring = Scrollback::default();
        ring.push(b"new".to_vec());
        assert!(write_scrollback(&path, &ring.snapshot()));
        assert_eq!(std::fs::read(&path).unwrap(), b"new\r\n");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn the_newer_scrollback_wins_whichever_write_lands_last() {
        let dir = tempfile::tempdir().expect("a temporary folder");
        let mut ring = Scrollback::default();
        ring.push(b"Maren waves.".to_vec());
        let older = ring.snapshot();
        ring.push(b"Orla nods.".to_vec());
        let newer = ring.snapshot();
        for (name, first, second) in [
            ("in-order.txt", &older, &newer),
            ("out-of-order.txt", &newer, &older),
        ] {
            let path = dir.path().join(name);
            assert!(write_scrollback(&path, first));
            assert!(write_scrollback(&path, second));
            assert_eq!(
                std::fs::read(&path).unwrap(),
                b"Maren waves.\r\nOrla nods.\r\n",
                "{name}"
            );
        }
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 2, "no temp file is left: {names:?}");
    }

    #[test]
    fn a_failed_scrollback_write_says_so() {
        let dir = tempfile::tempdir().expect("a temporary folder");
        let path = dir.path().join("missing").join("scrollback.txt");
        let mut ring = Scrollback::default();
        ring.push(b"Tolliver leaves north.".to_vec());
        assert!(!write_scrollback(&path, &ring.snapshot()));
    }

    #[test]
    fn clear_forgets_every_line_and_the_run() {
        let mut s = Scrollback::default();
        s.push(b"first".to_vec());
        s.push(b"second".to_vec());
        s.clear();
        let leftover = &s.dump();
        assert!(leftover.is_empty(), "{leftover:?}");
        let live = &s.dump_live();
        assert!(live.is_empty(), "{live:?}");
        s.push(b"after".to_vec());
        assert_eq!(s.lines.len(), 1);
    }

    #[test]
    fn dump_round_trips_through_load() {
        let mut a = Scrollback::default();
        a.push(b"first".to_vec());
        a.push(b"second".to_vec());
        a.push(b"third".to_vec());
        let bytes = a.dump();

        let mut b = Scrollback::default();
        b.load_from_bytes(&bytes);
        assert_eq!(b.lines.len(), 3);
        assert_eq!(b.lines[0], b"first");
        assert_eq!(b.lines[2], b"third");
    }

    #[test]
    fn load_preserves_blank_lines_and_handles_crlf() {
        let mut s = Scrollback::default();
        s.load_from_bytes(b"alpha\r\nbeta\r\n\r\ngamma\r\n");
        assert_eq!(s.lines.len(), 4);
        assert_eq!(s.lines[0], b"alpha");
        assert_eq!(s.lines[1], b"beta");
        assert_eq!(s.lines[2], b"");
        assert_eq!(s.lines[3], b"gamma");
    }

    #[test]
    fn a_run_of_repeated_lines_keeps_one_line_with_its_count_in_the_order_it_came() {
        let mut s = Scrollback::default();
        s.push(b"You are hungry.".to_vec());
        s.start_run(b"You dodge Quenby's attack.".to_vec(), 1);
        s.join_run(b"(2) You dodge Quenby's attack.".to_vec(), 2);
        // A prompt kept while it shows pinned never showed after the run
        // on screen, so the run comes after it once a line joins.
        s.push(b"[1020/1020hp 800/800mn 930/930mv]".to_vec());
        s.join_run(b"(3) You dodge Quenby's attack.".to_vec(), 3);
        assert_eq!(
            s.lines().collect::<Vec<_>>(),
            [
                &b"You are hungry."[..],
                b"[1020/1020hp 800/800mn 930/930mv]",
                b"(3) You dodge Quenby's attack.",
            ]
        );
        // The next run starts on a line of its own.
        s.start_run(b"You parry Quenby's attack.".to_vec(), 4);
        s.join_run(b"(2) You parry Quenby's attack.".to_vec(), 5);
        assert_eq!(s.lines.len(), 4);
        assert_eq!(s.lines[3], b"(2) You parry Quenby's attack.");
        assert_eq!(s.lines[2], b"(3) You dodge Quenby's attack.");
    }

    #[test]
    fn a_run_whose_line_left_the_ring_starts_again() {
        let mut s = Scrollback::default();
        s.start_run(b"You are hungry.".to_vec(), 1);
        for i in 0..SCROLLBACK_CAP {
            s.push(format!("line {i}").into_bytes());
        }
        s.join_run(b"(2) You are hungry.".to_vec(), 2);
        assert_eq!(s.lines.len(), SCROLLBACK_CAP);
        assert_eq!(s.lines.back().unwrap(), b"(2) You are hungry.");
        assert_eq!(s.lines.front().unwrap(), b"line 1");
        // A restored scrollback holds no run.
        s.load_from_bytes(b"alpha\r\n");
        s.join_run(b"(3) You are hungry.".to_vec(), 3);
        assert_eq!(s.lines.len(), 2);
    }

    #[test]
    fn a_live_dump_marks_the_run_only_while_it_is_the_last_line() {
        let mut s = Scrollback::default();
        s.push(b"You are hungry.".to_vec());
        s.start_run(b"You dodge Quenby's attack.".to_vec(), 7);
        s.join_run(b"(2) You dodge Quenby's attack.".to_vec(), 8);
        let mark = vosh_prompt::stage::mark(8);
        let mut want = b"You are hungry.\r\n".to_vec();
        want.extend(&mark);
        want.extend(b"(2) You dodge Quenby's attack.\r\n");
        assert_eq!(s.dump_live(), want);
        // The file the next launch restores holds no mark.
        assert_eq!(
            s.dump(),
            b"You are hungry.\r\n(2) You dodge Quenby's attack.\r\n"
        );
        // A line kept after the run leaves it unmarked.
        s.push(b"[1020/1020hp 800/800mn 930/930mv]".to_vec());
        assert_eq!(s.dump_live(), s.dump());
    }

    #[test]
    fn round_trip_preserves_blank_rows() {
        let mut a = Scrollback::default();
        a.push(b"first".to_vec());
        a.push(b"".to_vec());
        a.push(b"third".to_vec());
        a.push(b"".to_vec());
        a.push(b"".to_vec());
        a.push(b"sixth".to_vec());
        let bytes = a.dump();
        let mut b = Scrollback::default();
        b.load_from_bytes(&bytes);
        assert_eq!(b.lines.len(), 6);
        assert_eq!(b.lines[0], b"first");
        assert_eq!(b.lines[1], b"");
        assert_eq!(b.lines[2], b"third");
        assert_eq!(b.lines[3], b"");
        assert_eq!(b.lines[4], b"");
        assert_eq!(b.lines[5], b"sixth");
    }
}

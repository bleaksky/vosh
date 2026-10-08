//! Your logs. This module keeps the session log and the scrollback ring,
//! [`forget_passwords`] blanks the lines in that log where you sent a
//! password, and [`retention`] deletes the logs past Keep logs for.
//!
//! The shared log store wraps `vosh_log::LogStore` in an async mutex so
//! the session `io_loop`, the search commands, and the scrollback flush
//! path can all reach the same `SQLite` handle.
//!
//! The ring buffer holds the most recent terminal lines and survives
//! across runs as a plain text scrollback file.

pub(crate) mod forget_passwords;
pub(crate) mod retention;

use std::collections::VecDeque;
use std::sync::Arc;

use tokio::sync::Mutex;

pub(crate) type SharedLogStore = Arc<Mutex<Option<vosh_log::LogStore>>>;

/// Maximum number of terminal lines kept in the persistent scrollback.
const SCROLLBACK_CAP: usize = 10_000;

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

#[derive(Debug, Default)]
pub(crate) struct Scrollback {
    lines: VecDeque<Vec<u8>>,
    /// The run of repeated lines the screen ends on, while Collapse
    /// repeated lines is on.
    run: Option<RingRun>,
}

impl Scrollback {
    /// Forget every line, for Clear scrollback. The next launch restores
    /// nothing from before it.
    pub(crate) fn clear(&mut self) {
        self.lines.clear();
        self.run = None;
    }

    pub(crate) fn push(&mut self, raw_line: Vec<u8>) {
        if self.lines.len() == SCROLLBACK_CAP {
            self.lines.pop_front();
            // The run's line was the oldest, and left with it.
            if self.run.is_some_and(|run| run.after + 1 >= SCROLLBACK_CAP) {
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
    }
}

pub(crate) type SharedScrollback = Arc<Mutex<Scrollback>>;

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

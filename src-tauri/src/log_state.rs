//! Shared log store state. Wraps `vosh_log::LogStore` in an async
//! mutex so the session `io_loop`, the search commands, and the scrollback
//! flush path can all reach the same `SQLite` handle.
//!
//! Also owns a ring buffer of the most recent terminal lines that
//! survives across runs as a plain text scrollback file.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;

pub(crate) type SharedLogStore = Arc<Mutex<Option<vosh_log::LogStore>>>;

/// Maximum number of terminal lines kept in the persistent scrollback.
const SCROLLBACK_CAP: usize = 10_000;

#[derive(Debug, Default)]
pub(crate) struct Scrollback {
    lines: VecDeque<Vec<u8>>,
    /// How many lines the ring ever took, so a line's number names it for
    /// as long as the ring still holds it.
    taken: u64,
    /// The number of the line that shows the run of repeated lines the
    /// screen ends on, while Collapse repeated lines is on.
    run: Option<u64>,
}

impl Scrollback {
    pub(crate) fn push(&mut self, raw_line: Vec<u8>) {
        if self.lines.len() == SCROLLBACK_CAP {
            self.lines.pop_front();
        }
        self.lines.push_back(raw_line);
        self.taken += 1;
    }

    /// Keep `line` as Collapse repeated lines made of it on screen: on its
    /// own while it is off, as the start of a run, or as the run's line
    /// with its count once a line joined the run.
    pub(crate) fn keep(&mut self, line: Vec<u8>, repeat: Option<vosh_prompt::stage::Repeat>) {
        match repeat {
            None => self.push(line),
            Some(vosh_prompt::stage::Repeat::Starts) => self.start_run(line),
            Some(vosh_prompt::stage::Repeat::Joins(_)) => self.join_run(line),
        }
    }

    /// Keep `line`, the first of a run of repeated lines, and note it, so
    /// the lines that join the run take its place.
    pub(crate) fn start_run(&mut self, line: Vec<u8>) {
        self.push(line);
        self.run = Some(self.taken - 1);
    }

    /// Keep `line`, the run of repeated lines with its count, in place of
    /// the run's line, as the screen shows the run once. A prompt kept
    /// after the run while it shows pinned stays where it is. A run whose
    /// line already left the ring starts again.
    pub(crate) fn join_run(&mut self, line: Vec<u8>) {
        let first = self.taken - self.lines.len() as u64;
        let at = self
            .run
            .filter(|&run| run >= first)
            .and_then(|run| usize::try_from(run - first).ok());
        match at.and_then(|at| self.lines.get_mut(at)) {
            Some(kept) => *kept = line,
            None => self.start_run(line),
        }
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

pub(crate) fn scrollback_path(base: &std::path::Path) -> PathBuf {
    base.join("scrollback.txt")
}

pub(crate) fn log_db_path(base: &std::path::Path) -> PathBuf {
    base.join("logs.sqlite")
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
    fn a_run_of_repeated_lines_keeps_one_line_with_its_count() {
        let mut s = Scrollback::default();
        s.push(b"You are hungry.".to_vec());
        s.start_run(b"You dodge Quenby's attack.".to_vec());
        s.join_run(b"(2) You dodge Quenby's attack.".to_vec());
        // A prompt kept while it shows pinned stays after the run.
        s.push(b"[1020/1020hp 800/800mn 930/930mv]".to_vec());
        s.join_run(b"(3) You dodge Quenby's attack.".to_vec());
        assert_eq!(
            s.lines().collect::<Vec<_>>(),
            [
                &b"You are hungry."[..],
                b"(3) You dodge Quenby's attack.",
                b"[1020/1020hp 800/800mn 930/930mv]",
            ]
        );
        // The next run starts on a line of its own.
        s.start_run(b"You parry Quenby's attack.".to_vec());
        s.join_run(b"(2) You parry Quenby's attack.".to_vec());
        assert_eq!(s.lines.len(), 4);
        assert_eq!(s.lines[3], b"(2) You parry Quenby's attack.");
        assert_eq!(s.lines[1], b"(3) You dodge Quenby's attack.");
    }

    #[test]
    fn a_run_whose_line_left_the_ring_starts_again() {
        let mut s = Scrollback::default();
        s.start_run(b"You are hungry.".to_vec());
        for i in 0..SCROLLBACK_CAP {
            s.push(format!("line {i}").into_bytes());
        }
        s.join_run(b"(2) You are hungry.".to_vec());
        assert_eq!(s.lines.len(), SCROLLBACK_CAP);
        assert_eq!(s.lines.back().unwrap(), b"(2) You are hungry.");
        assert_eq!(s.lines.front().unwrap(), b"line 1");
        // A restored scrollback holds no run.
        s.load_from_bytes(b"alpha\r\n");
        s.join_run(b"(3) You are hungry.".to_vec());
        assert_eq!(s.lines.len(), 2);
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

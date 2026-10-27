//! The pace of what your automation sends: the lines triggers, timers,
//! the tick, Lua, plugins and `#walk` send. What you type, your aliases
//! and your macros go past it, as the writing card's hold lets them.
//!
//! A burst of [`BURST`] lines goes at once, and the rest follow at the
//! pace the profile sets, 4 a second unless you change it, the pace
//! Aabahran reads commands at, one each pulse. More than [`RUNAWAY`]
//! waiting means automation sends faster than any game reads, so Vosh
//! drops them all and says so.
//!
//! Aabahran counts the same command sent many times in a row. It warns
//! at 25, holds you at 50 and freezes the character at 75, and a
//! trigger that answers each game line with the same command walks into
//! that at the game's own pace, which no pace stops. So the
//! [`REPEAT_STOP`]th line of automation that repeats the line before it
//! is held back, and so is each one after it, until you type a command
//! or automation sends another line. What you type is never held back.

use std::collections::VecDeque;
use std::time::Duration;

use tokio::time::Instant;

/// How many lines go at once before the pace holds the rest.
pub(crate) const BURST: f64 = 10.0;

/// The lines waiting past which Vosh drops them all.
pub(crate) const RUNAWAY: usize = 200;

/// The line of automation that repeats the lines before it this many
/// times in a row is held back.
pub(crate) const REPEAT_STOP: u32 = 40;

/// The pace a profile sets: `per_sec` lines a second, or none.
pub(crate) fn pace_of(ui: &crate::profile::ui::UiConfig) -> Option<u32> {
    ui.send_pace.then_some(ui.send_pace_per_sec.clamp(1, 20))
}

/// The pace of one connection.
#[derive(Debug)]
pub(crate) struct Pace {
    /// Lines a second, or None while pacing is off.
    per_sec: Option<u32>,
    /// How many lines may go now, up to [`BURST`].
    tokens: f64,
    /// When `tokens` last grew.
    refilled: Option<Instant>,
    /// Whole lines that wait, each with its line end.
    waiting: VecDeque<Vec<u8>>,
    /// The last line that went to the game, as it went, and how many
    /// times in a row.
    last: Vec<u8>,
    repeats: u32,
    /// The repeat stop holds back `last` now.
    held_back: bool,
    /// What the session prints once the pass ends.
    notices: Vec<String>,
}

impl Default for Pace {
    fn default() -> Self {
        Self {
            per_sec: Some(4),
            tokens: BURST,
            refilled: None,
            waiting: VecDeque::new(),
            last: Vec::new(),
            repeats: 0,
            held_back: false,
            notices: Vec::new(),
        }
    }
}

/// The whole lines of `buf`, each with its line end.
fn lines(buf: &[u8]) -> impl Iterator<Item = &[u8]> {
    buf.split_inclusive(|&b| b == b'\n')
}

/// A line without its line end, to compare and to name.
fn text(line: &[u8]) -> &[u8] {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    line.strip_suffix(b"\r").unwrap_or(line)
}

impl Pace {
    /// Take the profile's pace. Lines that wait keep waiting at the new
    /// pace, and go at once when pacing turns off.
    pub(crate) fn set(&mut self, per_sec: Option<u32>) {
        self.per_sec = per_sec.map(|n| n.clamp(1, 20));
    }

    fn refill(&mut self, now: Instant) {
        let Some(per_sec) = self.per_sec else {
            return;
        };
        if let Some(then) = self.refilled {
            let grown = now.saturating_duration_since(then).as_secs_f64() * f64::from(per_sec);
            self.tokens = (self.tokens + grown).min(BURST);
        }
        self.refilled = Some(now);
    }

    /// Count `line` as gone to the game, for the repeat stop.
    fn went(&mut self, line: &[u8]) {
        let line = text(line);
        if line == self.last.as_slice() {
            self.repeats = self.repeats.saturating_add(1);
        } else {
            self.last = line.to_vec();
            self.repeats = 1;
            self.held_back = false;
        }
    }

    /// Lines you typed went out: the count starts again from them, and a
    /// repeat stop lets go.
    pub(crate) fn typed(&mut self, buf: &[u8]) {
        for line in lines(buf) {
            if line.ends_with(b"\n") {
                self.last = text(line).to_vec();
                self.repeats = 1;
                self.held_back = false;
            }
        }
    }

    /// Whether automation's `line` must wait for you, as the repeat stop
    /// says, and note it when it starts.
    fn repeat_stops(&mut self, line: &[u8]) -> bool {
        if text(line) != self.last.as_slice() {
            return false;
        }
        if self.held_back {
            return true;
        }
        if self.repeats + 1 >= REPEAT_STOP {
            self.held_back = true;
            self.notices.push(format!(
                "Vosh held back `{}`, which your triggers, timers or scripts sent {REPEAT_STOP} times in a row. The game freezes a character that sends one command 75 times in a row. Type any command to let them send it again.",
                String::from_utf8_lossy(text(line))
            ));
            return true;
        }
        false
    }

    /// Automation's `buf` at `now`: hands back the bytes that go at once,
    /// and keeps the lines the pace holds. A telnet answer, which ends
    /// no line, is the caller's to send.
    pub(crate) fn offer(&mut self, buf: &[u8], now: Instant) -> Vec<u8> {
        self.refill(now);
        let mut now_out = Vec::new();
        for line in lines(buf) {
            if self.repeat_stops(line) {
                continue;
            }
            self.went(line);
            if self.per_sec.is_none() || (self.waiting.is_empty() && self.tokens >= 1.0) {
                if self.per_sec.is_some() {
                    self.tokens -= 1.0;
                }
                now_out.extend_from_slice(line);
            } else {
                self.waiting.push_back(line.to_vec());
            }
        }
        if self.waiting.len() > RUNAWAY {
            let dropped = self.waiting.len();
            self.waiting.clear();
            self.notices.push(format!(
                "Vosh dropped {dropped} lines your triggers, timers or scripts sent faster than the game reads them."
            ));
        }
        now_out
    }

    /// The lines whose turn came by `now`.
    pub(crate) fn due(&mut self, now: Instant) -> Vec<u8> {
        self.refill(now);
        let mut out = Vec::new();
        while let Some(line) = self.waiting.front() {
            if self.per_sec.is_some() {
                if self.tokens < 1.0 {
                    break;
                }
                self.tokens -= 1.0;
            }
            out.extend_from_slice(line);
            self.waiting.pop_front();
        }
        out
    }

    /// When the next line that waits gets its turn, None while none
    /// waits.
    pub(crate) fn next_due(&self) -> Option<Instant> {
        if self.waiting.is_empty() {
            return None;
        }
        let (Some(per_sec), Some(then)) = (self.per_sec, self.refilled) else {
            return Some(Instant::now());
        };
        let short = (1.0 - self.tokens).max(0.0);
        Some(then + Duration::from_secs_f64(short / f64::from(per_sec)))
    }

    /// How many lines wait.
    pub(crate) fn waiting(&self) -> usize {
        self.waiting.len()
    }

    /// Drop every line that waits, as you asked, and say how many.
    pub(crate) fn drop_waiting(&mut self) -> usize {
        let dropped = self.waiting.len();
        self.waiting.clear();
        if dropped > 0 {
            self.notices.push(if dropped == 1 {
                "Dropped the line that waited to send.".to_string()
            } else {
                format!("Dropped the {dropped} lines that waited to send.")
            });
        }
        dropped
    }

    /// What the session prints, once each.
    pub(crate) fn take_notices(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notices)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::Instant;

    use super::{Pace, BURST, REPEAT_STOP, RUNAWAY};

    fn lines(n: usize, word: &str) -> Vec<u8> {
        (0..n)
            .flat_map(|i| format!("{word} {i}\r\n").into_bytes())
            .collect()
    }

    fn count(buf: &[u8]) -> usize {
        usize::try_from(super::super::socket::lines_in(buf)).unwrap_or(usize::MAX)
    }

    #[test]
    fn a_burst_goes_at_once_and_the_rest_at_the_pace() {
        let t0 = Instant::now();
        let mut pace = Pace::default();
        let now = pace.offer(&lines(14, "say"), t0);
        assert_eq!(count(&now), BURST as usize);
        assert_eq!(pace.waiting(), 4);
        // Four a second, so one every quarter second.
        assert_eq!(pace.due(t0 + Duration::from_millis(200)), Vec::<u8>::new());
        assert_eq!(count(&pace.due(t0 + Duration::from_millis(260))), 1);
        assert_eq!(count(&pace.due(t0 + Duration::from_secs(1))), 3);
        assert_eq!(pace.waiting(), 0);
        assert_eq!(pace.next_due(), None);
    }

    #[test]
    fn with_pacing_off_every_line_goes_at_once() {
        let t0 = Instant::now();
        let mut pace = Pace::default();
        pace.set(None);
        assert_eq!(count(&pace.offer(&lines(50, "say"), t0)), 50);
        // Lines that waited when it turned off go at the next look.
        pace.set(Some(4));
        let _ = pace.offer(&lines(30, "say"), t0);
        let waited = pace.waiting();
        assert!(waited > 0);
        pace.set(None);
        assert_eq!(count(&pace.due(t0)), waited);
        assert_eq!(pace.waiting(), 0);
    }

    #[test]
    fn a_runaway_drops_every_line_that_waits_and_says_so() {
        let t0 = Instant::now();
        let mut pace = Pace::default();
        let _ = pace.offer(&lines(BURST as usize + RUNAWAY + 1, "say"), t0);
        assert_eq!(pace.waiting(), 0);
        let notices = pace.take_notices();
        assert_eq!(notices.len(), 1);
        assert!(
            notices[0].starts_with("Vosh dropped 201 lines"),
            "{notices:?}"
        );
    }

    #[test]
    fn the_same_line_from_automation_stops_at_the_fortieth_until_you_type() {
        let t0 = Instant::now();
        let mut pace = Pace::default();
        pace.set(None);
        let mut sent = 0;
        for _ in 0..60 {
            sent += count(&pace.offer(b"kick\r\n", t0));
        }
        assert_eq!(sent, REPEAT_STOP as usize - 1);
        let notices = pace.take_notices();
        assert_eq!(notices.len(), 1);
        assert!(
            notices[0].starts_with("Vosh held back `kick`"),
            "{notices:?}"
        );
        // A command you type lets it go, and the count starts again.
        pace.typed(b"kick\r\n");
        assert_eq!(count(&pace.offer(b"kick\r\n", t0)), 1);
        // So does another line from automation.
        let mut pace = Pace::default();
        pace.set(None);
        for _ in 0..50 {
            let _ = pace.offer(b"kick\r\nbash\r\n", t0);
        }
        assert_eq!(pace.take_notices(), Vec::<String>::new());
    }

    #[test]
    fn dropping_what_waits_says_how_many() {
        let t0 = Instant::now();
        let mut pace = Pace::default();
        let _ = pace.offer(&lines(13, "say"), t0);
        assert_eq!(pace.drop_waiting(), 3);
        assert_eq!(
            pace.take_notices(),
            ["Dropped the 3 lines that waited to send."]
        );
        assert_eq!(pace.drop_waiting(), 0);
        assert_eq!(pace.take_notices(), Vec::<String>::new());
    }
}

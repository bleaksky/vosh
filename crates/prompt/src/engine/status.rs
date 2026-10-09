//! Miss counting and status: whether Vosh reads your prompt now, for
//! `session://prompt-status` and `#prompt`.

use chrono::{DateTime, FixedOffset};
use serde::Serialize;

use super::{stamp, PromptEngine};
use crate::aabahran::observer;

/// How many pulses in a row without your prompt make it not matching.
pub(crate) const MISSES: u32 = 3;

/// Whether Vosh reads your prompt, for `session://prompt-status` and
/// `#prompt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Nothing reads your prompt in this profile.
    NoCapture,
    Matching,
    /// Three pulses in a row brought no prompt Vosh reads.
    NotMatching,
    /// You turned prompts off in the game, so no prompt comes and none
    /// is missed.
    PromptsOff,
}

/// `session://prompt-status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusReport {
    pub status: Status,
    /// When Vosh last read your prompt, RFC 3339 local time.
    pub last_match_at: Option<String>,
}

/// What counts misses between prompts.
#[derive(Debug, Clone, Default)]
pub(super) struct Misses {
    /// Pulses or sends in a row that brought no prompt Vosh read.
    pub(super) count: u32,
    /// A prompt was read since the pulse started, or since your send.
    pub(super) matched: bool,
    /// A pulse has started this session, so the next one ends it.
    pub(super) in_pulse: bool,
    /// The game sent text since your last send.
    pub(super) text: bool,
    pub(super) last_match_at: Option<DateTime<FixedOffset>>,
    pub(super) reported: Option<StatusReport>,
}

impl PromptEngine {
    /// Note a line you sent: the observer's window opens, and `prompt
    /// off` in it means the reply sets nothing. `text` is what went to
    /// the game, aliases expanded. `at_ms` is milliseconds since the
    /// epoch.
    ///
    /// Returns true when the send started a pulse, on a server that
    /// sends no Char.Vitals.
    pub fn note_send(&mut self, text: &str, at_ms: i64) -> bool {
        self.observer.sent_at = Some(at_ms);
        self.observer.sent_off = text.lines().any(observer::turns_prompts_off);
        self.observer.off_line = false;
        if !self.forsaken() {
            // Elsewhere a miss is a send whose reply brought text but no
            // prompt Vosh read before your next send.
            if self.stage.has_recognizer() && self.misses.text && !self.misses.matched {
                self.misses.count += 1;
            }
            self.misses.text = false;
            self.misses.matched = false;
        }
        let pulse = self.vars.on_send();
        if pulse {
            self.pulse_started();
        }
        pulse
    }

    /// A pulse started. Under the Forsaken Lands rules the one before it
    /// is a miss when it brought no prompt Vosh read, unless you turned
    /// prompts off.
    pub(super) fn pulse_started(&mut self) {
        if !self.forsaken() {
            return;
        }
        let missed = self.misses.in_pulse && !self.misses.matched;
        if missed && self.stage.has_recognizer() && !self.prompts_off {
            self.misses.count += 1;
        }
        self.misses.in_pulse = true;
        self.misses.matched = false;
    }

    /// The game sent a line or a partial.
    pub fn note_text(&mut self) {
        self.misses.text = true;
    }

    /// Whether Vosh reads your prompt now.
    pub fn status(&self) -> Status {
        if self.prompts_off {
            Status::PromptsOff
        } else if !self.stage.has_recognizer() {
            Status::NoCapture
        } else if self.misses.count >= MISSES {
            Status::NotMatching
        } else {
            Status::Matching
        }
    }

    /// When Vosh last read your prompt this session.
    pub fn last_match_at(&self) -> Option<DateTime<FixedOffset>> {
        self.misses.last_match_at
    }

    /// The status and when Vosh last read your prompt, as
    /// `session://prompt-status` carries them.
    pub(crate) fn status_report(&self) -> StatusReport {
        StatusReport {
            status: self.status(),
            last_match_at: self.misses.last_match_at.map(stamp),
        }
    }

    /// The status when it changed since the last call, for one
    /// `session://prompt-status` per socket read.
    pub fn take_status_change(&mut self) -> Option<StatusReport> {
        let report = self.status_report();
        if self.misses.reported.as_ref() == Some(&report) {
            return None;
        }
        self.misses.reported = Some(report.clone());
        Some(report)
    }

    /// Vosh read your prompt at `at`, so your text prompt is on and no
    /// prompt is missed.
    pub fn note_prompt(&mut self, at: DateTime<FixedOffset>) {
        self.prompts_off = false;
        self.misses.count = 0;
        self.misses.matched = true;
        self.misses.last_match_at = Some(at);
    }

    /// You turned prompts off in the game.
    pub fn prompts_off(&self) -> bool {
        self.prompts_off
    }
}

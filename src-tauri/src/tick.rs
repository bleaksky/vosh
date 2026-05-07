//! Per-session tick timer.
//!
//! MUD ticks are periodic server events that regenerate stats and advance
//! quests. Players want to know exactly when one is about to fire so they
//! can time abilities. The tick timer counts down from a configurable
//! interval and optionally resets on a regex match against incoming server
//! output. When the countdown reaches zero the timer cycles and may run an
//! auto-fire command through the input pipeline.

use std::time::Duration;

use regex::Regex;
use serde::Serialize;
use tokio::time::Instant;

/// Default tick interval in seconds. Matches the typical ROM 2.4 tick.
pub(crate) const DEFAULT_INTERVAL_SECS: u64 = 30;

#[derive(Debug, Clone)]
pub(crate) struct TickConfig {
    pub enabled: bool,
    pub interval: Duration,
    pub auto_fire: Option<String>,
    pub sound: bool,
    pub reset_pattern: Option<String>,
}

impl Default for TickConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: Duration::from_secs(DEFAULT_INTERVAL_SECS),
            auto_fire: None,
            sound: true,
            reset_pattern: None,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct TickRuntime {
    pub config: TickConfig,
    /// Compiled form of `config.reset_pattern`. Recompiled when the pattern
    /// changes via the slash command.
    pub reset_regex: Option<Regex>,
    /// Wall-clock instant the timer should fire next. `None` while the
    /// timer is disabled.
    pub next_fire: Option<Instant>,
}

impl TickRuntime {
    pub(crate) fn enable(&mut self, now: Instant) {
        self.config.enabled = true;
        self.next_fire = Some(now + self.config.interval);
    }

    pub(crate) fn disable(&mut self) {
        self.config.enabled = false;
        self.next_fire = None;
    }

    pub(crate) fn reset(&mut self, now: Instant) {
        if self.config.enabled {
            self.next_fire = Some(now + self.config.interval);
        }
    }

    pub(crate) fn set_interval(&mut self, secs: u64, now: Instant) {
        self.config.interval = Duration::from_secs(secs.max(1));
        if self.config.enabled {
            self.next_fire = Some(now + self.config.interval);
        }
    }

    pub(crate) fn set_reset_pattern(
        &mut self,
        pattern: Option<String>,
    ) -> Result<(), regex::Error> {
        let regex = match &pattern {
            Some(p) => Some(Regex::new(p)?),
            None => None,
        };
        self.config.reset_pattern = pattern;
        self.reset_regex = regex;
        Ok(())
    }

    pub(crate) fn check_reset_match(&self, line: &str) -> bool {
        self.reset_regex.as_ref().is_some_and(|r| r.is_match(line))
    }

    pub(crate) fn remaining(&self, now: Instant) -> Option<Duration> {
        let next = self.next_fire?;
        Some(next.saturating_duration_since(now))
    }

    /// Take the next-fire instant if we have already passed it. Returns
    /// `Some(())` to indicate the caller should run the fire side effects
    /// (auto-fire + sound) and reschedule.
    pub(crate) fn try_consume_fire(&mut self, now: Instant) -> bool {
        let Some(next) = self.next_fire else {
            return false;
        };
        if now < next {
            return false;
        }
        self.next_fire = Some(now + self.config.interval);
        true
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TickPayload {
    pub enabled: bool,
    pub interval_ms: u64,
    pub remaining_ms: u64,
    /// True for the emit that corresponds to a tick fire. Used by the
    /// frontend to play the optional beep exactly once per cycle.
    pub fired: bool,
    pub sound: bool,
}

impl TickPayload {
    pub(crate) fn from_runtime(runtime: &TickRuntime, now: Instant, fired: bool) -> Self {
        Self {
            enabled: runtime.config.enabled,
            interval_ms: runtime.config.interval.as_millis() as u64,
            remaining_ms: runtime.remaining(now).map_or(0, |d| d.as_millis() as u64),
            fired,
            sound: runtime.config.sound,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Duration;

    fn now_plus(d: Duration) -> Instant {
        Instant::now() + d
    }

    #[test]
    fn enable_sets_next_fire() {
        let mut t = TickRuntime::default();
        let start = Instant::now();
        t.enable(start);
        let remaining = t.remaining(start).unwrap();
        assert_eq!(remaining.as_secs(), DEFAULT_INTERVAL_SECS);
    }

    #[test]
    fn disable_clears_next_fire() {
        let mut t = TickRuntime::default();
        t.enable(Instant::now());
        t.disable();
        assert!(t.next_fire.is_none());
        assert!(t.remaining(Instant::now()).is_none());
    }

    #[test]
    fn reset_pushes_next_fire() {
        let mut t = TickRuntime::default();
        let start = Instant::now();
        t.enable(start);
        let later = now_plus(Duration::from_secs(10));
        t.reset(later);
        let remaining = t.remaining(later).unwrap();
        assert_eq!(remaining.as_secs(), DEFAULT_INTERVAL_SECS);
    }

    #[test]
    fn try_consume_fire_only_after_due() {
        let mut t = TickRuntime::default();
        let start = Instant::now();
        t.enable(start);
        assert!(!t.try_consume_fire(start));
        let after = start + t.config.interval;
        assert!(t.try_consume_fire(after));
        // Reschedules to one interval ahead.
        let remaining = t.remaining(after).unwrap();
        assert_eq!(remaining.as_secs(), DEFAULT_INTERVAL_SECS);
    }

    #[test]
    fn set_interval_clamps_to_one_second_minimum() {
        let mut t = TickRuntime::default();
        let now = Instant::now();
        t.set_interval(0, now);
        assert_eq!(t.config.interval, Duration::from_secs(1));
    }

    #[test]
    fn reset_pattern_compiles_or_fails() {
        let mut t = TickRuntime::default();
        assert!(t.set_reset_pattern(Some("good (.*)".into())).is_ok());
        assert!(t.check_reset_match("good morning"));
        assert!(t.set_reset_pattern(Some("[bad".into())).is_err());
    }

    #[test]
    fn check_reset_match_false_when_no_pattern() {
        let t = TickRuntime::default();
        assert!(!t.check_reset_match("anything"));
    }
}

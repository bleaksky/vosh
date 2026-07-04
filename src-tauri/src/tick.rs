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
    /// Seconds before the next fire at which the warning echo should
    /// land. None disables the warning.
    pub warn_at_secs: Option<u64>,
    /// Text printed to the terminal as the warning. None falls back to
    /// a sensible default when `warn_at_secs` is set.
    pub warn_message: Option<String>,
    /// Color for the warning text. Accepts standard ANSI names ("red",
    /// "bright-red", "yellow", etc.), hex ("#rrggbb", "#rgb", with or
    /// without the #), or a 256-palette index ("196"). None defaults to
    /// bright-red.
    pub warn_color: Option<String>,
}

impl Default for TickConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: Duration::from_secs(DEFAULT_INTERVAL_SECS),
            auto_fire: None,
            sound: true,
            reset_pattern: None,
            warn_at_secs: None,
            warn_message: None,
            warn_color: None,
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
    /// Last `hour` value observed on a GMCP `World.Time` push. Used to
    /// detect the moment the server's tick fires on MUDs that report
    /// world time (Aabahran does, and many ROM derivatives do as well).
    pub last_world_hour: Option<String>,
    /// Whether the warning echo has already fired for the current cycle.
    /// Resets on tick fire, on `reset()`, and on interval changes.
    pub warned_this_cycle: bool,
}

impl TickRuntime {
    pub(crate) fn enable(&mut self, now: Instant) {
        self.config.enabled = true;
        self.next_fire = Some(now + self.config.interval);
        self.warned_this_cycle = false;
    }

    pub(crate) fn disable(&mut self) {
        self.config.enabled = false;
        self.next_fire = None;
        self.warned_this_cycle = false;
    }

    pub(crate) fn reset(&mut self, now: Instant) {
        if self.config.enabled {
            self.next_fire = Some(now + self.config.interval);
        }
        self.warned_this_cycle = false;
    }

    pub(crate) fn set_interval(&mut self, secs: u64, now: Instant) {
        self.config.interval = Duration::from_secs(secs.max(1));
        if self.config.enabled {
            self.next_fire = Some(now + self.config.interval);
        }
        self.warned_this_cycle = false;
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

    /// Update the last observed world hour. Returns true when the value
    /// actually changed (and the caller should reset the tick). The first
    /// observation primes the state without firing a reset, so the user
    /// does not see a spurious tick the moment they connect.
    pub(crate) fn observe_world_hour(&mut self, hour: &str) -> bool {
        match &self.last_world_hour {
            Some(prev) if prev == hour => false,
            Some(_) => {
                self.last_world_hour = Some(hour.to_string());
                true
            }
            None => {
                self.last_world_hour = Some(hour.to_string());
                false
            }
        }
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
        self.warned_this_cycle = false;
        true
    }

    /// Take the warning slot if the configured threshold is set, the
    /// remaining time has fallen below it, and we haven't already
    /// fired this cycle. Returns true exactly once per cycle.
    pub(crate) fn try_consume_warn(&mut self, now: Instant) -> bool {
        if !self.config.enabled || self.warned_this_cycle {
            return false;
        }
        let Some(secs) = self.config.warn_at_secs else {
            return false;
        };
        let Some(remaining) = self.remaining(now) else {
            return false;
        };
        if remaining.as_secs() <= secs && remaining > Duration::ZERO {
            self.warned_this_cycle = true;
            return true;
        }
        false
    }
}

/// Resolve a color spec into the SGR escape that paints it: an ANSI name,
/// hex (`#rrggbb` / `#rgb`, the `#` optional), or a 256-palette index.
/// Unknown specs fall back to bright red so a misconfigured color still
/// draws attention.
pub(crate) fn warn_color_escape(name: Option<&str>) -> String {
    let lowered = name.unwrap_or("").trim().to_ascii_lowercase();
    let named = match lowered.as_str() {
        "red" => Some("\x1b[31m"),
        "green" => Some("\x1b[32m"),
        "yellow" => Some("\x1b[33m"),
        "blue" => Some("\x1b[34m"),
        "magenta" => Some("\x1b[35m"),
        "cyan" => Some("\x1b[36m"),
        "white" => Some("\x1b[37m"),
        "bright-green" | "bgreen" => Some("\x1b[1;32m"),
        "bright-yellow" | "byellow" => Some("\x1b[1;33m"),
        "bright-blue" | "bblue" => Some("\x1b[1;34m"),
        "bright-magenta" | "bmagenta" => Some("\x1b[1;35m"),
        "bright-cyan" | "bcyan" => Some("\x1b[1;36m"),
        "bright-white" | "bwhite" => Some("\x1b[1;37m"),
        _ => None,
    };
    if let Some(sgr) = named {
        return sgr.to_string();
    }
    // Bare digits read as a 256-palette index ("196"); a # prefix always
    // reads as hex, so "#196" is the color #119966 shorthand instead.
    if !lowered.starts_with('#') {
        if let Ok(idx) = lowered.parse::<u8>() {
            return format!("\x1b[38;5;{idx}m");
        }
    }
    // Hex: #rrggbb or #rgb, the # optional.
    let hex = lowered.strip_prefix('#').unwrap_or(&lowered);
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        let channel = |s: &str| u8::from_str_radix(s, 16).unwrap_or(0);
        let (r, g, b) = (
            channel(&hex[0..2]),
            channel(&hex[2..4]),
            channel(&hex[4..6]),
        );
        return format!("\x1b[38;2;{r};{g};{b}m");
    }
    if hex.len() == 3 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        let channel = |s: &str| u8::from_str_radix(s, 16).unwrap_or(0) * 17;
        let (r, g, b) = (
            channel(&hex[0..1]),
            channel(&hex[1..2]),
            channel(&hex[2..3]),
        );
        return format!("\x1b[38;2;{r};{g};{b}m");
    }
    // Bright red doubles as the unknown-spec fallback so a typo still
    // draws attention.
    "\x1b[1;31m".to_string()
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

    #[test]
    fn observe_world_hour_first_call_primes_without_reset() {
        let mut t = TickRuntime::default();
        assert!(!t.observe_world_hour("9"));
        assert_eq!(t.last_world_hour.as_deref(), Some("9"));
    }

    #[test]
    fn observe_world_hour_returns_true_when_value_changes() {
        let mut t = TickRuntime::default();
        let _ = t.observe_world_hour("9");
        assert!(!t.observe_world_hour("9"));
        assert!(t.observe_world_hour("10"));
        assert_eq!(t.last_world_hour.as_deref(), Some("10"));
    }

    #[test]
    fn warn_color_accepts_names_hex_and_palette_indexes() {
        assert_eq!(warn_color_escape(Some("yellow")), "\x1b[33m");
        assert_eq!(warn_color_escape(Some("#ff8800")), "\x1b[38;2;255;136;0m");
        assert_eq!(warn_color_escape(Some("FF8800")), "\x1b[38;2;255;136;0m");
        assert_eq!(warn_color_escape(Some("#f80")), "\x1b[38;2;255;136;0m");
        assert_eq!(warn_color_escape(Some("196")), "\x1b[38;5;196m");
    }

    #[test]
    fn warn_color_falls_back_to_bright_red_on_typos() {
        assert_eq!(warn_color_escape(Some("chartreuse-ish")), "\x1b[1;31m");
        assert_eq!(warn_color_escape(Some("#ff88")), "\x1b[1;31m");
        assert_eq!(warn_color_escape(None), "\x1b[1;31m");
    }
}

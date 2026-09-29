//! Per-session tick timer.
//!
//! MUD ticks are periodic server events that regenerate stats and advance
//! quests. Players want to know exactly when one is about to land so they
//! can time abilities. The timer counts from the last tick against a
//! configurable interval.
//!
//! The game's own tick decides. A GMCP `World.Time` hour change (after the
//! first, which only primes) or a line that matches the Reset on pattern
//! is a real tick. Once one has come in this session the timer is synced,
//! and the interval is only how long a tick is expected to take. Aabahran
//! picks each tick at random between 25 and 35 seconds, so a local timer
//! that fired on its own would ring early or miss a tick. While synced the
//! local timer never fires or restarts by itself. Each real tick restarts
//! the count and runs the fire side effects (the sound and the Send each
//! tick command) exactly once.
//!
//! Unsynced, before any real tick this session or on a game that sends
//! none, the local timer fires at the interval as it always has. A synced
//! timer that hears nothing for twice the interval fires locally once and
//! drops back to unsynced, so it never freezes when GMCP stops.

use std::time::Duration;

use regex::Regex;
use serde::Serialize;
use tokio::time::Instant;

/// Default tick interval in seconds. Matches the typical ROM 2.4 tick.
pub(crate) const DEFAULT_INTERVAL_SECS: u64 = 30;

/// Real tick signals this close to the tick they follow belong to it. A
/// `World.Time` hour change and a Reset on line for the same tick, or a
/// pattern that matches several lines of one tick, fire once. A real tick
/// this close after a local fire restarts the count without firing again.
pub(crate) const SAME_TICK_WINDOW: Duration = Duration::from_secs(2);

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
    /// When the count last restarted: the last tick, real or local, a
    /// reset, or the moment the timer started. `None` while the timer is
    /// off. The expected tick is one interval after it.
    pub last_tick: Option<Instant>,
    /// Last `hour` value observed on a GMCP `World.Time` push. Used to
    /// detect the moment the server's tick fires on MUDs that report
    /// world time (Aabahran does, and many ROM derivatives do as well).
    pub last_world_hour: Option<String>,
    /// A real tick has come in since the timer started, so the game
    /// decides when it fires.
    pub synced: bool,
    /// When the last real tick signal counted as a tick, for
    /// [`SAME_TICK_WINDOW`].
    pub last_signal: Option<Instant>,
    /// When the local timer last fired by itself, for
    /// [`SAME_TICK_WINDOW`].
    pub last_local_fire: Option<Instant>,
    /// Whether the warning echo has already fired for the current cycle.
    /// Resets on each tick, on `reset()`, and on interval changes.
    pub warned_this_cycle: bool,
    /// A session is connected. A profile switch arms a timer its new
    /// config turns on only while one is.
    pub in_session: bool,
}

/// What the session does for one tick event: the report to emit on
/// `session://tick`, the Send each tick command to run, and the warning
/// line to print.
#[derive(Debug)]
pub(crate) struct TickStep {
    pub payload: TickPayload,
    pub command: Option<String>,
    pub warn_echo: Option<String>,
}

impl TickRuntime {
    /// Start the timer for a new connection. It starts unsynced, and the
    /// first `World.Time` hour of the session primes again, so an hour
    /// that moved while you were away is not a tick.
    pub(crate) fn start_session(&mut self, now: Instant) {
        self.in_session = true;
        self.last_world_hour = None;
        self.forget_sync();
        self.enable(now);
    }

    /// Stop the timer when the connection ends.
    pub(crate) fn end_session(&mut self) {
        self.in_session = false;
        self.disable();
    }

    pub(crate) fn enable(&mut self, now: Instant) {
        if !self.config.enabled {
            self.forget_sync();
        }
        self.config.enabled = true;
        self.last_tick = Some(now);
        self.warned_this_cycle = false;
    }

    pub(crate) fn disable(&mut self) {
        self.config.enabled = false;
        self.stop();
    }

    /// Stop the count and forget the game's tick, leaving the config.
    fn stop(&mut self) {
        self.last_tick = None;
        self.warned_this_cycle = false;
        self.forget_sync();
    }

    fn forget_sync(&mut self) {
        self.synced = false;
        self.last_signal = None;
        self.last_local_fire = None;
    }

    /// Restart the count now, as `#tick reset` asks. A synced timer stays
    /// synced, and the next real tick restarts the count again.
    pub(crate) fn reset(&mut self, now: Instant) {
        if self.config.enabled {
            self.last_tick = Some(now);
        }
        self.warned_this_cycle = false;
    }

    /// Set the interval. Unsynced it restarts the count, as it always
    /// has. Synced the interval is only the expected length, so the count
    /// carries on and the expected tick moves.
    pub(crate) fn set_interval(&mut self, secs: u64, now: Instant) {
        self.config.interval = Duration::from_secs(secs.max(1));
        if self.config.enabled && !self.synced {
            self.last_tick = Some(now);
        }
        self.warned_this_cycle = false;
    }

    pub(crate) fn set_reset_pattern(
        &mut self,
        pattern: Option<String>,
    ) -> Result<(), regex::Error> {
        let regex = compile_reset_pattern(pattern.as_deref())?;
        self.set_compiled_reset_pattern(pattern, regex);
        Ok(())
    }

    /// Install a pattern that [`compile_reset_pattern`] already
    /// compiled, so a caller that checked it first cannot fail here.
    pub(crate) fn set_compiled_reset_pattern(
        &mut self,
        pattern: Option<String>,
        regex: Option<Regex>,
    ) {
        self.config.reset_pattern = pattern;
        self.reset_regex = regex;
    }

    pub(crate) fn check_reset_match(&self, line: &str) -> bool {
        self.reset_regex.as_ref().is_some_and(|r| r.is_match(line))
    }

    /// Update the last observed world hour. Returns true when the value
    /// actually changed, which is a real tick. The first observation
    /// primes the state without a tick, so you do not see a spurious tick
    /// the moment you connect.
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

    /// The expected tick, one interval after the last. `None` while the
    /// timer is off.
    pub(crate) fn next_fire(&self) -> Option<Instant> {
        if !self.config.enabled {
            return None;
        }
        Some(self.last_tick? + self.config.interval)
    }

    /// Time left until the expected tick, zero once it has passed.
    pub(crate) fn remaining(&self, now: Instant) -> Option<Duration> {
        Some(self.next_fire()?.saturating_duration_since(now))
    }

    /// Restart the count on a tick and open a new warning cycle.
    fn restart(&mut self, now: Instant) {
        self.last_tick = Some(now);
        self.warned_this_cycle = false;
    }

    /// Whether the local timer fires by itself now. Unsynced it fires at
    /// the interval. Synced it waits for the game, and fires only when no
    /// real tick has come for twice the interval, then drops back to
    /// unsynced. Firing restarts the count.
    pub(crate) fn try_consume_fire(&mut self, now: Instant) -> bool {
        let Some(last) = self.last_tick.filter(|_| self.config.enabled) else {
            return false;
        };
        let wait = if self.synced {
            self.config.interval * 2
        } else {
            self.config.interval
        };
        if now < last + wait {
            return false;
        }
        if self.synced {
            self.forget_sync();
        }
        self.restart(now);
        self.last_local_fire = Some(now);
        true
    }

    /// Take the warning slot if the configured threshold is set, the
    /// time left before the expected tick has fallen below it, and it
    /// has not fired this cycle. Returns true at most once per cycle, and
    /// never while the tick is overdue.
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

    /// One step of the session loop, four times a second. Fires the local
    /// timer when [`try_consume_fire`](Self::try_consume_fire) says so,
    /// takes the warning, and reports.
    pub(crate) fn poll(&mut self, now: Instant) -> TickStep {
        let fired = self.try_consume_fire(now);
        let warned = self.try_consume_warn(now);
        TickStep {
            payload: TickPayload::from_runtime(self, now, fired),
            command: self.fire_command(fired),
            warn_echo: warned.then(|| warn_echo(&self.config)),
        }
    }

    /// A real tick from the game: a `World.Time` hour change or a line
    /// that matches the Reset on pattern. The first one syncs the timer.
    /// Each restarts the count and fires once. Returns `None` while the
    /// timer is off and for a signal inside [`SAME_TICK_WINDOW`] of the
    /// tick it belongs to. A real tick just after a local fire restarts
    /// the count on the game's tick and reports without firing again.
    pub(crate) fn on_game_tick(&mut self, now: Instant) -> Option<TickStep> {
        if !self.config.enabled || self.last_tick.is_none() {
            return None;
        }
        let within = |at: Option<Instant>| {
            at.is_some_and(|at| now.saturating_duration_since(at) < SAME_TICK_WINDOW)
        };
        if within(self.last_signal) {
            return None;
        }
        let fired = !within(self.last_local_fire);
        self.synced = true;
        self.last_signal = Some(now);
        self.last_local_fire = None;
        self.restart(now);
        Some(TickStep {
            payload: TickPayload::from_runtime(self, now, fired),
            command: self.fire_command(fired),
            warn_echo: None,
        })
    }

    fn fire_command(&self, fired: bool) -> Option<String> {
        if fired {
            self.config.auto_fire.clone()
        } else {
            None
        }
    }
}

/// The warning line for `config`, in its color.
fn warn_echo(config: &TickConfig) -> String {
    let message = config
        .warn_message
        .clone()
        .unwrap_or_else(|| match config.warn_at_secs {
            Some(s) => format!("TICK IN {s}s"),
            None => "TICK INCOMING".to_string(),
        });
    let color = warn_color_escape(config.warn_color.as_deref());
    format!("\r\n{color}{message}\x1b[0m\r\n")
}

/// Compile a Reset on pattern without touching any runtime, so a caller
/// can check it before it changes anything.
pub(crate) fn compile_reset_pattern(pattern: Option<&str>) -> Result<Option<Regex>, regex::Error> {
    pattern.map(Regex::new).transpose()
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
        assert!(t.next_fire().is_none());
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

    // ── The game's tick decides ─────────────────────────────────────

    fn secs(s: f64) -> Duration {
        Duration::from_secs_f64(s)
    }

    /// A 30 second timer that sends `score` on each tick, started at
    /// the moment a session connects.
    fn session(start: Instant) -> TickRuntime {
        let mut t = TickRuntime::default();
        t.config.auto_fire = Some("score".into());
        t.start_session(start);
        t
    }

    /// Poll every 250 ms from `from` up to and not including `to`, the
    /// way the session loop does, and return the instants that fired.
    fn poll_span(t: &mut TickRuntime, from: Instant, to: Instant) -> Vec<Instant> {
        let mut fired = Vec::new();
        let mut at = from;
        while at < to {
            if t.poll(at).payload.fired {
                fired.push(at);
            }
            at += Duration::from_millis(250);
        }
        fired
    }

    #[test]
    fn unsynced_the_local_timer_fires_at_the_interval_as_before() {
        let t0 = Instant::now();
        let mut t = session(t0);
        assert!(!t.synced);
        assert!(poll_span(&mut t, t0, t0 + secs(30.0)).is_empty());
        let step = t.poll(t0 + secs(30.0));
        assert!(step.payload.fired);
        assert_eq!(step.command.as_deref(), Some("score"));
        // The count restarts from the fire.
        assert_eq!(t.remaining(t0 + secs(30.0)), Some(secs(30.0)));
        assert!(!t.poll(t0 + secs(45.0)).payload.fired);
        assert!(t.poll(t0 + secs(60.0)).payload.fired);
        assert!(!t.synced);
    }

    #[test]
    fn the_first_world_hour_only_primes() {
        let t0 = Instant::now();
        let mut t = session(t0);
        assert!(!t.observe_world_hour("9"));
        assert!(!t.synced);
    }

    #[test]
    fn a_game_tick_fires_once_and_syncs_the_timer() {
        let t0 = Instant::now();
        let mut t = session(t0);
        let step = t.on_game_tick(t0 + secs(12.0)).expect("the tick lands");
        assert!(step.payload.fired);
        assert_eq!(step.command.as_deref(), Some("score"));
        assert!(t.synced);
        assert_eq!(t.remaining(t0 + secs(12.0)), Some(secs(30.0)));
    }

    #[test]
    fn once_synced_the_local_timer_no_longer_fires_at_the_interval() {
        let t0 = Instant::now();
        let mut t = session(t0);
        let _ = t.observe_world_hour("9");
        assert!(t.observe_world_hour("10"));
        let tick = t0 + secs(10.0);
        assert!(t.on_game_tick(tick).is_some());
        // Past the interval the timer waits for the game, overdue.
        assert!(poll_span(&mut t, tick, tick + secs(59.0)).is_empty());
        assert!(t.synced);
        assert_eq!(t.remaining(tick + secs(45.0)), Some(Duration::ZERO));
    }

    #[test]
    fn real_ticks_at_25_30_and_35_seconds_each_fire_once_and_restart_the_count() {
        let t0 = Instant::now();
        let mut t = session(t0);
        let mut tick = t0 + secs(4.0);
        assert!(t.on_game_tick(tick).is_some());
        for gap in [25.0, 30.0, 35.0] {
            let next = tick + secs(gap);
            assert!(
                poll_span(&mut t, tick, next).is_empty(),
                "no local fire in a {gap} second tick"
            );
            let step = t.on_game_tick(next).expect("the tick lands");
            assert!(step.payload.fired, "a {gap} second tick fires");
            assert_eq!(step.command.as_deref(), Some("score"));
            assert_eq!(t.remaining(next), Some(secs(30.0)));
            assert_eq!(t.last_tick, Some(next));
            tick = next;
        }
    }

    #[test]
    fn a_world_hour_change_and_a_reset_line_for_one_tick_fire_once() {
        let t0 = Instant::now();
        let mut t = session(t0);
        let tick = t0 + secs(20.0);
        assert!(t.on_game_tick(tick).is_some());
        assert!(t.on_game_tick(tick + secs(1.5)).is_none());
        // The second signal leaves the count where the first put it.
        assert_eq!(t.last_tick, Some(tick));
        // Past the window a new signal is a new tick.
        assert!(t.on_game_tick(tick + secs(2.0)).is_some());
    }

    #[test]
    fn a_pattern_that_matches_three_lines_of_one_tick_fires_once() {
        let t0 = Instant::now();
        let mut t = session(t0);
        let tick = t0 + secs(20.0);
        let fired: Vec<bool> = [0.0, 0.01, 0.02]
            .iter()
            .map(|d| t.on_game_tick(tick + secs(*d)).is_some())
            .collect();
        assert_eq!(fired, [true, false, false]);
    }

    #[test]
    fn a_game_tick_just_after_a_local_fire_restarts_the_count_without_firing_again() {
        let t0 = Instant::now();
        let mut t = session(t0);
        assert!(t.poll(t0 + secs(30.0)).payload.fired);
        let step = t.on_game_tick(t0 + secs(31.0)).expect("the count restarts");
        assert!(!step.payload.fired);
        assert_eq!(step.command, None);
        assert!(t.synced);
        assert_eq!(t.remaining(t0 + secs(31.0)), Some(secs(30.0)));
    }

    #[test]
    fn no_game_tick_for_twice_the_interval_fires_once_locally_and_unsyncs() {
        let t0 = Instant::now();
        let mut t = session(t0);
        let tick = t0 + secs(5.0);
        assert!(t.on_game_tick(tick).is_some());
        assert!(poll_span(&mut t, tick, tick + secs(60.0)).is_empty());
        let step = t.poll(tick + secs(60.0));
        assert!(step.payload.fired);
        assert_eq!(step.command.as_deref(), Some("score"));
        assert!(!t.synced);
        // Back to counting on its own, one interval at a time.
        let fallback = tick + secs(60.0);
        assert_eq!(
            poll_span(&mut t, fallback, fallback + secs(60.25)),
            [fallback + secs(30.0), fallback + secs(60.0)]
        );
        // The next real tick syncs it again.
        assert!(t.on_game_tick(fallback + secs(70.0)).is_some());
        assert!(t.synced);
    }

    #[test]
    fn a_disabled_timer_ignores_game_ticks() {
        let t0 = Instant::now();
        let mut t = session(t0);
        t.disable();
        assert!(t.on_game_tick(t0 + secs(10.0)).is_none());
        assert!(!t.synced);
        assert!(!t.poll(t0 + secs(40.0)).payload.fired);
    }

    #[test]
    fn a_new_connection_starts_unsynced_and_primes_the_world_hour_again() {
        let t0 = Instant::now();
        let mut t = session(t0);
        let _ = t.observe_world_hour("9");
        assert!(t.on_game_tick(t0 + secs(10.0)).is_some());
        assert!(t.synced);
        t.end_session();
        let t1 = t0 + secs(100.0);
        t.start_session(t1);
        assert!(!t.synced);
        assert_eq!(t.last_world_hour, None);
        // The first hour of the new session primes, even when it moved.
        assert!(!t.observe_world_hour("11"));
        assert_eq!(t.remaining(t1), Some(secs(30.0)));
    }

    #[test]
    fn the_warning_prints_once_per_cycle_and_not_again_while_overdue() {
        let t0 = Instant::now();
        let mut t = session(t0);
        t.config.warn_at_secs = Some(5);
        let tick = t0 + secs(3.0);
        assert!(t.on_game_tick(tick).is_some());
        let warned = |t: &mut TickRuntime, from: f64, to: f64| -> usize {
            let mut n = 0;
            let mut at = tick + secs(from);
            while at < tick + secs(to) {
                if t.poll(at).warn_echo.is_some() {
                    n += 1;
                }
                at += Duration::from_millis(250);
            }
            n
        };
        assert_eq!(warned(&mut t, 0.0, 24.0), 0);
        assert_eq!(warned(&mut t, 24.0, 30.0), 1);
        // The game runs late. No second warning while overdue.
        assert_eq!(warned(&mut t, 30.0, 40.0), 0);
        let late = tick + secs(40.0);
        assert!(t.on_game_tick(late).is_some());
        let step = t.poll(late + secs(26.0));
        assert_eq!(
            step.warn_echo.as_deref(),
            Some("\r\n\x1b[1;31mTICK IN 5s\x1b[0m\r\n")
        );
    }

    #[test]
    fn a_manual_reset_while_synced_restarts_the_count_and_stays_synced() {
        let t0 = Instant::now();
        let mut t = session(t0);
        assert!(t.on_game_tick(t0 + secs(5.0)).is_some());
        t.reset(t0 + secs(15.0));
        assert!(t.synced);
        assert_eq!(t.remaining(t0 + secs(15.0)), Some(secs(30.0)));
    }

    #[test]
    fn a_new_interval_while_synced_keeps_the_count() {
        let t0 = Instant::now();
        let mut t = session(t0);
        let tick = t0 + secs(5.0);
        assert!(t.on_game_tick(tick).is_some());
        t.set_interval(40, tick + secs(10.0));
        assert_eq!(t.last_tick, Some(tick));
        assert_eq!(t.remaining(tick + secs(10.0)), Some(secs(30.0)));
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

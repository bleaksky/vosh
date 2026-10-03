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
//! drops back to unsynced, so it never freezes when GMCP stops. A new
//! interval starts that wait again, so a shorter one never makes a game
//! that still ticks look quiet.

use std::time::Duration;

use regex::Regex;
use serde::Serialize;
use tokio::time::Instant;
use tracing::warn;

/// Default tick interval in seconds. Matches the typical ROM 2.4 tick.
pub(crate) const DEFAULT_INTERVAL_SECS: u64 = 30;

/// Real tick signals this close to the tick they follow belong to it. A
/// `World.Time` hour change and a Reset on line for the same tick, or a
/// pattern that matches several lines of one tick, fire once. A real tick
/// this close after a local fire restarts the count without firing again.
pub(crate) const SAME_TICK_WINDOW: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq)]
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
    /// When the interval last changed while a synced count ran. The
    /// fallback waits twice the interval from the later of this and the
    /// last tick, so a new interval alone never fires it.
    pub interval_changed_at: Option<Instant>,
    /// Whether the warning echo has already fired for the current cycle.
    /// Resets each time the count restarts: on each tick, on `reset()`,
    /// and on an interval change while unsynced.
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

    /// Stop the timer when the connection ends. `config.enabled` is your
    /// setting and the profile saves it, so the end of a session leaves
    /// it alone and stops only the count. A save after the game
    /// disconnects, the exit flush among them, then keeps the tick on.
    pub(crate) fn end_session(&mut self) {
        self.in_session = false;
        self.stop();
    }

    /// Take another profile's tick settings mid session, as a live
    /// profile switch, `#profile load`, or `#profile reset` does. The
    /// running count carries across: the last tick, the synced state, the
    /// world hour, and whether this cycle warned. Only the settings
    /// change, so the expected tick moves with a new interval, and the
    /// fallback's wait starts again as it does for
    /// [`set_interval`](Self::set_interval).
    ///
    /// A running tick stays on, whatever the new profile saved. A
    /// connection starts the tick whatever the profile says, and earlier
    /// builds saved it off whenever the game had disconnected, so a
    /// saved off cannot be told from one you chose. You stop the tick
    /// yourself with `#tick disable` or the Tick switch in Settings. A
    /// config that turns the tick on while a session runs starts a
    /// stopped one now. Between sessions the config's setting applies
    /// as saved, and the timer stays stopped until the next connection
    /// starts it.
    pub(crate) fn adopt(
        &mut self,
        mut config: TickConfig,
        reset_regex: Option<Regex>,
        now: Instant,
    ) {
        let was_on = self.config.enabled;
        let was_interval = self.config.interval;
        if self.in_session && was_on && self.last_tick.is_some() {
            config.enabled = true;
        }
        self.config = config;
        self.reset_regex = reset_regex;
        if !self.config.enabled {
            self.stop();
        } else if self.in_session && (!was_on || self.last_tick.is_none()) {
            self.forget_sync();
            self.restart(now);
        } else {
            self.note_interval_change(was_interval, now);
        }
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
        self.interval_changed_at = None;
    }

    /// Restart the count now, as `#tick reset` asks. A synced timer stays
    /// synced, and the next real tick restarts the count again.
    pub(crate) fn reset(&mut self, now: Instant) {
        if self.config.enabled {
            self.last_tick = Some(now);
        }
        self.warned_this_cycle = false;
    }

    /// Set the interval. Unsynced it restarts the count and opens a new
    /// warning cycle, as it always has. Synced the interval is only the
    /// expected length, so the count carries on, the expected tick moves,
    /// and a warning this tick already printed does not print again. A
    /// new interval also starts the fallback's wait again.
    pub(crate) fn set_interval(&mut self, secs: u64, now: Instant) {
        let was_interval = self.config.interval;
        self.config.interval = Duration::from_secs(secs.max(1));
        if self.config.enabled && !self.synced {
            self.restart(now);
        } else {
            self.note_interval_change(was_interval, now);
        }
    }

    /// Start the fallback's wait again when the interval moved while a
    /// synced count runs. The wait is twice the interval, so without
    /// this a shorter interval could put the game's last tick past it
    /// and fire the fallback at once while the game still ticks.
    fn note_interval_change(&mut self, was_interval: Duration, now: Instant) {
        if self.synced && self.config.interval != was_interval {
            self.interval_changed_at = Some(now);
        }
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

    /// Time since the last tick. `None` while the timer is off.
    pub(crate) fn elapsed(&self, now: Instant) -> Option<Duration> {
        if !self.config.enabled {
            return None;
        }
        Some(now.saturating_duration_since(self.last_tick?))
    }

    /// Restart the count on a tick and open a new warning cycle.
    fn restart(&mut self, now: Instant) {
        self.last_tick = Some(now);
        self.warned_this_cycle = false;
        self.interval_changed_at = None;
    }

    /// Whether the local timer fires by itself now. Unsynced it fires at
    /// the interval. Synced it waits for the game, and fires only when no
    /// real tick has come for twice the interval, counted from the later
    /// of the last tick and the last change of the interval, then drops
    /// back to unsynced. Firing restarts the count.
    pub(crate) fn try_consume_fire(&mut self, now: Instant) -> bool {
        let Some(last) = self.last_tick.filter(|_| self.config.enabled) else {
            return false;
        };
        let due = if self.synced {
            let from = self.interval_changed_at.map_or(last, |at| at.max(last));
            from + self.config.interval * 2
        } else {
            last + self.config.interval
        };
        if now < due {
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
    /// time left before the expected tick has reached it, and it has not
    /// fired this cycle. Returns true at most once per cycle, and never
    /// while the tick is overdue.
    ///
    /// The rule is the status line's. It counts the time left in whole
    /// seconds rounded up and warns once that is at or under Warn at,
    /// which is the time left at or under Warn at seconds. So the
    /// warning prints on the report that turns the status line to warn,
    /// never up to a second before it.
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
        if remaining <= Duration::from_secs(secs) && remaining > Duration::ZERO {
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

/// Detect the game's tick from a GMCP `World.Time` push. Aabahran (and
/// most ROM derivatives that ship World.Time) advance the `hour` field
/// every server tick, so an hour change is the tick. Returns the step to
/// deliver when the change counted as a tick.
pub(crate) fn observe_world_time_for_tick(
    tick: &mut TickRuntime,
    msg: &vosh_protocol::gmcp::Message,
    now: Instant,
) -> Option<TickStep> {
    if msg.package != "World.Time" {
        return None;
    }
    let hour_str = match msg.data.as_object()?.get("hour")? {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => return None,
    };
    if tick.observe_world_hour(&hour_str) {
        tick.on_game_tick(now)
    } else {
        None
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
    /// Time left until the expected tick, zero once it has passed.
    pub remaining_ms: u64,
    /// Time since the last tick. It keeps growing past the interval
    /// while the game runs late, so the frontend can show how late.
    pub elapsed_ms: u64,
    /// The expected tick has come and the game's tick has not, the
    /// elapsed time at or past the interval.
    pub overdue: bool,
    /// The game's tick decides when the timer fires.
    pub synced: bool,
    /// True for the emit that corresponds to a tick fire. Used by the
    /// frontend to play the optional beep exactly once per cycle.
    pub fired: bool,
    pub sound: bool,
}

impl TickPayload {
    pub(crate) fn from_runtime(runtime: &TickRuntime, now: Instant, fired: bool) -> Self {
        let millis = |d: Duration| d.as_millis() as u64;
        let elapsed = runtime.elapsed(now);
        Self {
            enabled: runtime.config.enabled,
            interval_ms: millis(runtime.config.interval),
            remaining_ms: runtime.remaining(now).map_or(0, millis),
            elapsed_ms: elapsed.map_or(0, millis),
            overdue: elapsed.is_some_and(|e| e >= runtime.config.interval),
            synced: runtime.synced,
            fired,
            sound: runtime.config.sound,
        }
    }
}

/// Snapshot of the per-session tick timer config. Mirrors
/// `tick::TickConfig` with `Duration` flattened to a `u64` of seconds
/// so the frontend can edit it cleanly. Reset pattern, auto-fire
/// command, warning timer / message / color are all optional — empty
/// means the feature is off.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct TickConfigPayload {
    pub enabled: bool,
    pub interval_secs: u64,
    pub auto_fire: Option<String>,
    pub sound: bool,
    pub reset_pattern: Option<String>,
    pub warn_at_secs: Option<u64>,
    pub warn_message: Option<String>,
    pub warn_color: Option<String>,
}

/// The live tick configuration as `tick_get_config` reads it.
pub(crate) fn tick_config_payload(cfg: &crate::tick::TickConfig) -> TickConfigPayload {
    TickConfigPayload {
        enabled: cfg.enabled,
        interval_secs: cfg.interval.as_secs(),
        auto_fire: cfg.auto_fire.clone(),
        sound: cfg.sound,
        reset_pattern: cfg.reset_pattern.clone(),
        warn_at_secs: cfg.warn_at_secs,
        warn_message: cfg.warn_message.clone(),
        warn_color: cfg.warn_color.clone(),
    }
}

/// The error `tick_set_config` returns for a Reset on pattern that does
/// not compile.
const TICK_RESET_PATTERN_ERROR: &str =
    "Vosh could not read the Reset on pattern. Check it and save again.";

/// Apply a tick configuration from Settings to `tick`. Checks the Reset
/// on pattern before it changes anything, so a pattern that does not
/// compile leaves the running tick exactly as it was and returns a
/// sentence. Routes interval changes through `TickRuntime::set_interval`
/// so the next-fire deadline rebuilds. Other fields are direct
/// assignments. Returns the configuration as it now reads.
pub(crate) fn apply_tick_config(
    tick: &mut crate::tick::TickRuntime,
    config: &TickConfigPayload,
    now: tokio::time::Instant,
) -> Result<TickConfigPayload, String> {
    // Normalize string options: empty / whitespace-only -> None so the
    // persisted state does not carry an empty placeholder.
    let auto_fire = config
        .auto_fire
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let reset_pattern = config
        .reset_pattern
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let warn_message = config
        .warn_message
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let warn_color = config
        .warn_color
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    // Everything that can fail runs before the first change.
    let reset_regex =
        crate::tick::compile_reset_pattern(reset_pattern.as_deref()).map_err(|e| {
            warn!(error = %e, "tick reset pattern did not compile");
            TICK_RESET_PATTERN_ERROR.to_string()
        })?;

    if config.enabled {
        if !tick.config.enabled {
            tick.enable(now);
        }
        tick.set_interval(config.interval_secs, now);
    } else {
        tick.disable();
        // Still record the interval so the user can flip enabled
        // back on without re-typing it.
        tick.config.interval = std::time::Duration::from_secs(config.interval_secs.max(1));
    }
    tick.set_compiled_reset_pattern(reset_pattern.clone(), reset_regex);
    tick.config.auto_fire.clone_from(&auto_fire);
    tick.config.sound = config.sound;
    tick.config.warn_at_secs = config.warn_at_secs.filter(|s| *s > 0);
    tick.config.warn_message.clone_from(&warn_message);
    tick.config.warn_color.clone_from(&warn_color);

    Ok(TickConfigPayload {
        enabled: tick.config.enabled,
        interval_secs: tick.config.interval.as_secs(),
        auto_fire,
        sound: tick.config.sound,
        reset_pattern,
        warn_at_secs: tick.config.warn_at_secs,
        warn_message,
        warn_color,
    })
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
        let leftover = &poll_span(&mut t, t0, t0 + secs(30.0));
        assert!(leftover.is_empty(), "{leftover:?}");
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
        let leftover = &poll_span(&mut t, tick, tick + secs(59.0));
        assert!(leftover.is_empty(), "{leftover:?}");
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
        let leftover = &poll_span(&mut t, tick, tick + secs(60.0));
        assert!(leftover.is_empty(), "{leftover:?}");
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
    fn the_end_of_a_session_stops_the_count_and_keeps_your_tick_setting() {
        let t0 = Instant::now();
        let mut t = session(t0);
        assert!(t.on_game_tick(t0 + secs(10.0)).is_some());
        t.end_session();
        assert!(t.config.enabled, "the saved setting stays on");
        assert!(!t.in_session);
        assert!(!t.synced);
        assert_eq!(t.next_fire(), None);
        assert!(t.on_game_tick(t0 + secs(40.0)).is_none());
    }

    #[test]
    fn a_tick_you_turned_off_stays_off_when_the_session_ends() {
        let t0 = Instant::now();
        let mut t = session(t0);
        t.disable();
        t.end_session();
        assert!(!t.config.enabled);
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

    /// Count the warnings the session loop prints polling every 250 ms
    /// from `from` up to and not including `to`.
    fn warns_between(t: &mut TickRuntime, from: Instant, to: Instant) -> usize {
        let mut n = 0;
        let mut at = from;
        while at < to {
            if t.poll(at).warn_echo.is_some() {
                n += 1;
            }
            at += Duration::from_millis(250);
        }
        n
    }

    #[test]
    fn a_new_interval_while_synced_does_not_warn_again_this_tick() {
        let t0 = Instant::now();
        let mut t = session(t0);
        t.config.warn_at_secs = Some(5);
        let tick = t0 + secs(10.0);
        assert!(t.on_game_tick(tick).is_some());
        assert_eq!(warns_between(&mut t, tick, tick + secs(26.5)), 1);
        // The same interval again, as a Settings save of another field
        // or `#tick interval 30` does, inside the warn window.
        t.set_interval(30, tick + secs(26.5));
        assert_eq!(
            warns_between(&mut t, tick + secs(26.5), tick + secs(33.0)),
            0
        );
        // Overdue, a longer interval puts the expected tick ahead again.
        t.set_interval(35, tick + secs(33.0));
        assert_eq!(
            warns_between(&mut t, tick + secs(33.0), tick + secs(40.0)),
            0
        );
        // The next tick opens a new cycle, and it warns again.
        let next = tick + secs(40.0);
        assert!(t.on_game_tick(next).is_some());
        assert_eq!(warns_between(&mut t, next, next + secs(35.0)), 1);
    }

    #[test]
    fn a_new_interval_unsynced_restarts_the_count_and_the_warning() {
        let t0 = Instant::now();
        let mut t = session(t0);
        t.config.warn_at_secs = Some(5);
        assert_eq!(warns_between(&mut t, t0, t0 + secs(27.0)), 1);
        t.set_interval(30, t0 + secs(27.0));
        assert_eq!(t.last_tick, Some(t0 + secs(27.0)));
        assert_eq!(warns_between(&mut t, t0 + secs(27.0), t0 + secs(57.0)), 1);
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

    // ── A new interval never makes the game look quiet ──────────────

    /// A synced session whose game ticked 5 seconds in, with the time
    /// of that tick.
    fn synced_session(t0: Instant) -> (TickRuntime, Instant) {
        let mut t = session(t0);
        let tick = t0 + secs(5.0);
        assert!(t.on_game_tick(tick).is_some());
        (t, tick)
    }

    #[test]
    fn lowering_the_interval_while_synced_does_not_fire_the_fallback() {
        let t0 = Instant::now();
        let (mut t, tick) = synced_session(t0);
        // 25 seconds into the tick you set Every to 10, as #tick
        // interval and a Settings save do.
        let change = tick + secs(25.0);
        t.set_interval(10, change);
        let leftover = &poll_span(&mut t, change, tick + secs(30.0));
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(t.synced);
        // The game's tick lands on time and fires once.
        let step = t.on_game_tick(tick + secs(30.0)).expect("the tick lands");
        assert!(step.payload.fired);
        assert_eq!(step.command.as_deref(), Some("score"));
        assert!(t.synced);
    }

    #[test]
    fn a_profile_with_a_shorter_interval_does_not_fire_the_fallback() {
        let t0 = Instant::now();
        let (mut t, tick) = synced_session(t0);
        // A switch, #profile load, reset, or an import brings Every 10.
        let mut config = t.config.clone();
        config.interval = secs(10.0);
        let change = tick + secs(25.0);
        t.adopt(config, None, change);
        let leftover = &poll_span(&mut t, change, tick + secs(30.0));
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(t.synced);
        let step = t.on_game_tick(tick + secs(30.0)).expect("the tick lands");
        assert!(step.payload.fired);
        assert!(t.synced);
    }

    #[test]
    fn after_a_shorter_interval_the_fallback_waits_twice_it_from_the_change() {
        let t0 = Instant::now();
        let (mut t, tick) = synced_session(t0);
        let change = tick + secs(25.0);
        t.set_interval(10, change);
        // The game goes quiet. Twice the new interval after the change
        // the timer fires once on its own and drops back to unsynced.
        let leftover = &poll_span(&mut t, change, change + secs(20.0));
        assert!(leftover.is_empty(), "{leftover:?}");
        let step = t.poll(change + secs(20.0));
        assert!(step.payload.fired);
        assert_eq!(step.command.as_deref(), Some("score"));
        assert!(!t.synced);
    }

    #[test]
    fn raising_the_interval_while_synced_does_not_fire_the_fallback() {
        let t0 = Instant::now();
        let (mut t, tick) = synced_session(t0);
        let change = tick + secs(25.0);
        t.set_interval(60, change);
        // The game runs a little late and ticks once.
        let leftover = &poll_span(&mut t, change, tick + secs(35.0));
        assert!(leftover.is_empty(), "{leftover:?}");
        let next = tick + secs(35.0);
        assert!(t.on_game_tick(next).expect("the tick lands").payload.fired);
        // Then it goes quiet, and the fallback waits twice the new one.
        let leftover = &poll_span(&mut t, next, next + secs(120.0));
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(t.synced);
        assert!(t.poll(next + secs(120.0)).payload.fired);
        assert!(!t.synced);
    }

    #[test]
    fn a_profile_with_a_longer_interval_does_not_fire_the_fallback() {
        let t0 = Instant::now();
        let (mut t, tick) = synced_session(t0);
        let mut config = t.config.clone();
        config.interval = secs(60.0);
        let change = tick + secs(25.0);
        t.adopt(config, None, change);
        let leftover = &poll_span(&mut t, change, tick + secs(35.0));
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(t.synced);
        assert!(
            t.on_game_tick(tick + secs(35.0))
                .expect("the tick lands")
                .payload
                .fired
        );
    }

    #[test]
    fn a_new_interval_unsynced_fires_at_the_new_interval_from_the_change() {
        for every in [10, 60] {
            let t0 = Instant::now();
            let mut t = session(t0);
            let change = t0 + secs(25.0);
            t.set_interval(every, change);
            let due = change + Duration::from_secs(every);
            assert!(
                poll_span(&mut t, change, due).is_empty(),
                "no fire before a new {every} second interval runs out"
            );
            assert!(
                t.poll(due).payload.fired,
                "fires at the new {every} seconds"
            );
            assert!(!t.synced);
        }
    }

    // ── The terminal warns when the status line does ────────────────

    /// The status line counts the time left in whole seconds rounded up
    /// and warns once that reaches Warn at (computeTick in
    /// src/lib/stores/tickStore.ts, whose test checks the same cases).
    /// Each case is the time into a 30 second tick and whether the
    /// status line warns there with Warn at 5.
    const WARN_BOUNDARY: [(f64, bool); 6] = [
        (24.0, false),
        (24.75, false),
        (24.999, false),
        (25.0, true),
        (25.001, true),
        (29.75, true),
    ];

    #[test]
    fn the_warning_prints_exactly_when_the_status_line_turns_to_warn() {
        for (into, warns) in WARN_BOUNDARY {
            let t0 = Instant::now();
            let (mut t, tick) = synced_session(t0);
            t.config.warn_at_secs = Some(5);
            assert_eq!(
                t.try_consume_warn(tick + secs(into)),
                warns,
                "{into} seconds into the tick"
            );
        }
    }

    #[test]
    fn the_session_loop_prints_the_warning_on_the_report_that_turns_warn() {
        let t0 = Instant::now();
        let (mut t, tick) = synced_session(t0);
        t.config.warn_at_secs = Some(5);
        let mut warned = Vec::new();
        let mut at = tick;
        while at < tick + secs(30.0) {
            if t.poll(at).warn_echo.is_some() {
                warned.push(at);
            }
            at += Duration::from_millis(250);
        }
        assert_eq!(warned, [tick + secs(25.0)]);
    }

    #[test]
    fn the_report_carries_the_time_since_the_tick_and_whether_it_is_overdue() {
        let t0 = Instant::now();
        let mut t = session(t0);
        let p = t.poll(t0 + secs(12.5)).payload;
        assert_eq!(
            (p.elapsed_ms, p.remaining_ms, p.interval_ms),
            (12_500, 17_500, 30_000)
        );
        assert!(!p.overdue);
        assert!(!p.synced);

        let tick = t0 + secs(14.0);
        let p = t.on_game_tick(tick).expect("the tick lands").payload;
        assert_eq!((p.elapsed_ms, p.remaining_ms), (0, 30_000));
        assert!(p.synced);
        assert!(p.fired);

        // At the expected tick and past it, the report says overdue and
        // keeps counting while the time left holds at zero.
        let p = t.poll(tick + secs(30.0)).payload;
        assert_eq!((p.elapsed_ms, p.remaining_ms), (30_000, 0));
        assert!(p.overdue);
        let p = t.poll(tick + secs(36.75)).payload;
        assert_eq!((p.elapsed_ms, p.remaining_ms), (36_750, 0));
        assert!(p.overdue);
        assert!(p.synced);
        assert!(!p.fired);

        // Off, the report counts nothing.
        t.disable();
        let p = t.poll(tick + secs(40.0)).payload;
        assert!(!p.enabled);
        assert_eq!((p.elapsed_ms, p.remaining_ms), (0, 0));
        assert!(!p.overdue);
        assert!(!p.synced);
    }

    #[test]
    fn the_report_serializes_the_new_fields() {
        let t0 = Instant::now();
        let t = session(t0);
        let json =
            serde_json::to_value(TickPayload::from_runtime(&t, t0 + secs(2.0), false)).unwrap();
        assert_eq!(json["elapsed_ms"], 2_000);
        assert_eq!(json["overdue"], false);
        assert_eq!(json["synced"], false);
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

    /// A Tick block from Settings: off, every minute, with every option
    /// filled in and `reset_pattern` as the Reset on pattern.
    fn tick_payload(reset_pattern: &str) -> super::TickConfigPayload {
        super::TickConfigPayload {
            enabled: false,
            interval_secs: 60,
            auto_fire: Some(" score ".into()),
            sound: false,
            reset_pattern: Some(reset_pattern.into()),
            warn_at_secs: Some(5),
            warn_message: Some("Tick soon".into()),
            warn_color: Some("red".into()),
        }
    }

    /// A running 30 second tick that resets on `^You feel`.
    fn running_tick(now: tokio::time::Instant) -> crate::tick::TickRuntime {
        let mut tick = crate::tick::TickRuntime::default();
        tick.enable(now);
        tick.set_reset_pattern(Some("^You feel".into())).unwrap();
        tick
    }

    #[test]
    fn a_tick_config_with_a_bad_reset_pattern_changes_nothing() {
        let now = tokio::time::Instant::now();
        let mut tick = running_tick(now);
        let before = format!("{:?}", tick.config);
        let next_fire = tick.next_fire();

        let err = super::apply_tick_config(&mut tick, &tick_payload("[bad"), now).unwrap_err();
        assert_eq!(
            err,
            "Vosh could not read the Reset on pattern. Check it and save again."
        );
        // Still on, still every 30 seconds, still on the same clock, and
        // still resetting on the old pattern.
        assert_eq!(format!("{:?}", tick.config), before);
        assert!(tick.config.enabled);
        assert_eq!(tick.config.interval.as_secs(), 30);
        assert_eq!(tick.next_fire(), next_fire);
        assert!(tick.check_reset_match("You feel less tired."));
    }

    #[test]
    fn a_tick_config_that_reads_applies_every_field() {
        let now = tokio::time::Instant::now();
        let mut tick = running_tick(now);

        let saved = super::apply_tick_config(&mut tick, &tick_payload(" ^Dawn "), now).unwrap();
        assert!(!saved.enabled);
        assert_eq!(saved.interval_secs, 60);
        assert_eq!(saved.auto_fire.as_deref(), Some("score"));
        assert_eq!(saved.reset_pattern.as_deref(), Some("^Dawn"));
        assert_eq!(saved.warn_at_secs, Some(5));
        assert!(!tick.config.enabled);
        assert_eq!(tick.next_fire(), None);
        assert_eq!(tick.config.interval.as_secs(), 60);
        assert!(!tick.config.sound);
        assert!(tick.check_reset_match("Dawn breaks."));
        assert!(!tick.check_reset_match("You feel less tired."));

        // Turned back on, the tick runs at the saved interval, and a
        // blank pattern clears the reset.
        let mut on = tick_payload("  ");
        on.enabled = true;
        let saved = super::apply_tick_config(&mut tick, &on, now).unwrap();
        assert!(saved.enabled);
        assert_eq!(saved.reset_pattern, None);
        assert_eq!(
            tick.next_fire(),
            Some(now + std::time::Duration::from_secs(60))
        );
        assert!(!tick.check_reset_match("Dawn breaks."));
    }

    #[test]
    fn a_tick_save_inside_the_warn_window_does_not_warn_twice() {
        let t0 = tokio::time::Instant::now();
        let at = |s: f64| t0 + std::time::Duration::from_secs_f64(s);
        let mut tick = crate::tick::TickRuntime::default();
        tick.start_session(t0);
        tick.config.warn_at_secs = Some(5);
        assert!(tick.on_game_tick(at(1.0)).is_some());
        let mut warns = 0;
        let mut now = 1.0;
        while now < 40.0 {
            if (now - 28.0_f64).abs() < f64::EPSILON {
                // Untick Play a sound in Settings, which saves the whole
                // Tick block at the same interval.
                let mut quiet = tick_payload("");
                quiet.enabled = true;
                quiet.interval_secs = 30;
                quiet.sound = false;
                super::apply_tick_config(&mut tick, &quiet, at(now)).unwrap();
            }
            if tick.poll(at(now)).warn_echo.is_some() {
                warns += 1;
            }
            now += 0.25;
        }
        assert_eq!(warns, 1);
        assert!(tick.synced);
    }

    #[test]
    fn a_tick_save_with_a_shorter_interval_does_not_fire_while_the_game_ticks() {
        let t0 = tokio::time::Instant::now();
        let at = |s: f64| t0 + std::time::Duration::from_secs_f64(s);
        let mut tick = crate::tick::TickRuntime::default();
        tick.start_session(t0);
        tick.config.auto_fire = Some("score".into());
        assert!(tick.on_game_tick(at(1.0)).is_some());
        // 25 seconds into the tick, Settings saves Every 10.
        let mut shorter = tick_payload("");
        shorter.enabled = true;
        shorter.interval_secs = 10;
        super::apply_tick_config(&mut tick, &shorter, at(26.0)).unwrap();
        let mut now = 26.0;
        while now < 31.0 {
            let step = tick.poll(at(now));
            assert!(!step.payload.fired, "no fallback at {now}");
            assert_eq!(step.command, None);
            now += 0.25;
        }
        assert!(tick.synced);
        let step = tick.on_game_tick(at(31.0)).expect("the tick lands");
        assert!(step.payload.fired);
        assert_eq!(step.command.as_deref(), Some("score"));
    }

    fn world_time(hour: serde_json::Value) -> vosh_protocol::gmcp::Message {
        vosh_protocol::gmcp::Message {
            package: "World.Time".into(),
            data: serde_json::json!({ "hour": hour }),
        }
    }

    #[test]
    fn a_world_hour_change_is_the_tick_and_fires_once() {
        let t0 = tokio::time::Instant::now();
        let mut tick = crate::tick::TickRuntime::default();
        tick.config.auto_fire = Some("score".into());
        tick.start_session(t0);
        let at = |s: u64| t0 + std::time::Duration::from_secs(s);

        // The first hour of the session primes.
        assert!(
            super::observe_world_time_for_tick(&mut tick, &world_time(9.into()), at(1)).is_none()
        );
        // The same hour again is no tick.
        assert!(
            super::observe_world_time_for_tick(&mut tick, &world_time(9.into()), at(5)).is_none()
        );
        let step = super::observe_world_time_for_tick(&mut tick, &world_time("10".into()), at(12))
            .expect("the hour moved");
        assert!(step.payload.fired);
        assert_eq!(step.command.as_deref(), Some("score"));
        assert!(tick.synced);
        // A Reset on line for the same tick does not fire again.
        assert!(tick.on_game_tick(at(13)).is_none());
        // Past the interval the timer waits for the next hour.
        assert!(!tick.poll(at(45)).payload.fired);
        let step = super::observe_world_time_for_tick(&mut tick, &world_time(11.into()), at(46))
            .expect("the next tick");
        assert!(step.payload.fired);

        // Other packages and a World.Time without an hour are no tick.
        let other = vosh_protocol::gmcp::Message {
            package: "Char.Vitals".into(),
            data: serde_json::json!({ "hour": 12 }),
        };
        assert!(super::observe_world_time_for_tick(&mut tick, &other, at(80)).is_none());
        let no_hour = vosh_protocol::gmcp::Message {
            package: "World.Time".into(),
            data: serde_json::json!({ "sunlight": "light" }),
        };
        assert!(super::observe_world_time_for_tick(&mut tick, &no_hour, at(80)).is_none());
    }
}

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
//!
//! The profile keeps the settings, [`TickSettings`], since its file saves
//! them. The connection keeps the count, [`TickRuntime`], and each of its
//! methods takes the settings it reads or writes. Every session on a
//! profile counts against its settings, so a change from one session
//! reaches the others through [`follow_in_other_sessions`].

use std::sync::Arc;
use std::time::Duration;

use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use tracing::warn;

use crate::app::state::AppState;
use crate::profile::open::OpenProfile;
use crate::sessions::SessionId;

/// Default tick interval in seconds. Matches the typical ROM 2.4 tick.
pub(crate) const DEFAULT_INTERVAL_SECS: u64 = 30;

/// Real tick signals this close to the tick they follow belong to it. A
/// `World.Time` hour change and a Reset on line for the same tick, or a
/// pattern that matches several lines of one tick, fire once. A real tick
/// this close after a local fire restarts the count without firing again.
pub(crate) const SAME_TICK_WINDOW: Duration = Duration::from_secs(2);

/// The tick settings. The profile file's `[tick]` table, the live timer
/// and the Settings Tick card all read this one shape, so a field keeps
/// the same key on disk and on the wire. An optional field left empty
/// turns its feature off.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct TickConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Whole seconds. Each path into the live timer clamps it to at
    /// least 1.
    #[serde(default = "default_interval_secs")]
    pub interval_secs: u64,
    #[serde(default)]
    pub auto_fire: Option<String>,
    #[serde(default = "default_true")]
    pub sound: bool,
    #[serde(default)]
    pub reset_pattern: Option<String>,
    /// Seconds before the next fire at which the warning echo should
    /// land. None disables the warning.
    #[serde(default)]
    pub warn_at_secs: Option<u64>,
    /// Text printed to the terminal as the warning. None falls back to
    /// a sensible default when `warn_at_secs` is set.
    #[serde(default)]
    pub warn_message: Option<String>,
    /// Color for the warning text. Accepts standard ANSI names ("red",
    /// "bright-red", "yellow", etc.), hex ("#rrggbb", "#rgb", with or
    /// without the #), or a 256-palette index ("196"). None defaults to
    /// bright-red.
    #[serde(default)]
    pub warn_color: Option<String>,
}

impl TickConfig {
    /// How long a tick is expected to take.
    pub(crate) fn interval(&self) -> Duration {
        Duration::from_secs(self.interval_secs)
    }
}

impl Default for TickConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: DEFAULT_INTERVAL_SECS,
            auto_fire: None,
            sound: true,
            reset_pattern: None,
            warn_at_secs: None,
            warn_message: None,
            warn_color: None,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_interval_secs() -> u64 {
    DEFAULT_INTERVAL_SECS
}

/// The tick settings the profile keeps: the `[tick]` table and its
/// compiled Reset on pattern.
#[derive(Debug, Default)]
pub(crate) struct TickSettings {
    pub config: TickConfig,
    /// Compiled form of `config.reset_pattern`. Recompiled when the pattern
    /// changes via the slash command.
    pub reset_regex: Option<Regex>,
}

impl TickSettings {
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
}

/// The running count of one connection.
#[derive(Debug, Default)]
pub(crate) struct TickRuntime {
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
    /// Whether the sun is up in the game, from the latest World.Time,
    /// which day and night themes follow. A drop keeps it,
    /// so the window holds what it showed until World.Time comes again.
    /// None before the first.
    pub daylight: Option<Daylight>,
}

/// The game's day or night, as World.Time says it. Every window hears it
/// on `vosh://daylight-changed`, since only the main window hears GMCP
/// and Settings and Help resolve their theme from it too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Daylight {
    Day,
    Night,
}

/// `vosh://daylight-changed`: the game of a session turned to day or
/// night. Every window hears it, and the page resolves day and night
/// themes from the selected session's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct DaylightPayload {
    pub(crate) phase: Daylight,
}

/// The first game hour of the day where World.Time names no sunlight.
/// Aabahran always names it, and day begins with the hour 6 line, `The
/// day has begun.`, so the fallback agrees with it.
const DAY_FIRST_HOUR: i64 = 6;
/// The first game hour of the night, the hour 19 line, `The night has
/// begun.`.
const NIGHT_FIRST_HOUR: i64 = 19;

impl Daylight {
    /// The day or night a World.Time packet `data` says: its `sunlight`,
    /// where rise, light and set are day and dark is night, as
    /// `isDaytime` in src/shell/daylight.ts reads it, or else its hour.
    /// None when it says neither.
    pub(crate) fn of_world_time(data: &serde_json::Value) -> Option<Self> {
        let sunlight = data
            .get("sunlight")
            .and_then(serde_json::Value::as_str)
            .map(str::to_ascii_lowercase);
        match sunlight.as_deref() {
            Some("dark" | "night") => return Some(Self::Night),
            Some("rise" | "light" | "set" | "day") => return Some(Self::Day),
            _ => {}
        }
        let hour = match data.get("hour")? {
            serde_json::Value::Number(n) => n.as_i64()?,
            serde_json::Value::String(s) => s.trim().parse().ok()?,
            _ => return None,
        };
        Some(if (DAY_FIRST_HOUR..NIGHT_FIRST_HOUR).contains(&hour) {
            Self::Day
        } else {
            Self::Night
        })
    }
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
    /// Start the timer for a new connection, with your switch turned on.
    /// It starts unsynced, and the first `World.Time` hour of the session
    /// primes again, so an hour that moved while you were away is not a
    /// tick.
    pub(crate) fn start_session(&mut self, settings: &mut TickSettings, now: Instant) {
        self.begin_session();
        self.enable(settings, now);
    }

    /// Start the timer for a new connection while another session on the
    /// profile is connected. That session's count already follows your
    /// switch, so it stays as it stands, and the count starts only while
    /// it is on.
    pub(crate) fn join_connected(&mut self, settings: &TickSettings, now: Instant) {
        self.begin_session();
        if settings.config.enabled {
            self.restart(now);
        } else {
            self.stop();
        }
    }

    fn begin_session(&mut self) {
        self.in_session = true;
        self.last_world_hour = None;
        self.forget_sync();
    }

    /// Stop the timer when the connection ends. `config.enabled` is your
    /// setting and the profile saves it, so the end of a session leaves
    /// it alone and stops only the count. A save after the game
    /// disconnects, the exit flush among them, then keeps the tick on.
    pub(crate) fn end_session(&mut self) {
        self.in_session = false;
        self.stop();
    }

    /// Follow the tick settings another profile laid over `settings` mid
    /// session, as a live profile switch, `#profile load`, or `#profile
    /// reset` does. `before` is the config they replaced. The running
    /// count carries across: the last tick, the synced state, the world
    /// hour, and whether this cycle warned. Only the settings change, so
    /// the expected tick moves with a new interval, and the fallback's
    /// wait starts again as it does for
    /// [`set_interval`](Self::set_interval).
    ///
    /// A running tick stays on, whatever the new profile saved. A
    /// connection alone on its profile starts the tick whatever the
    /// profile says, and earlier builds saved it off whenever the game
    /// had disconnected, so a saved off cannot be told from one you
    /// chose. You stop the tick
    /// yourself with `#tick disable` or the Tick switch in Settings. A
    /// config that turns the tick on while a session runs starts a
    /// stopped one now. Between sessions the config's setting applies
    /// as saved, and the timer stays stopped until the next connection
    /// starts it.
    pub(crate) fn adopt(&mut self, settings: &mut TickSettings, before: &TickConfig, now: Instant) {
        if self.in_session && before.enabled && self.last_tick.is_some() {
            settings.config.enabled = true;
        }
        self.follow(settings, before, now);
    }

    /// Follow the tick settings as they read now, which read `before`
    /// until they changed. A switch turned off stops the count, one
    /// turned on while a session runs starts it, and a new interval
    /// moves the expected tick and starts the fallback's wait again as
    /// [`set_interval`](Self::set_interval) does. Every other session on
    /// a profile follows a change to its settings this way.
    pub(crate) fn follow(&mut self, settings: &TickSettings, before: &TickConfig, now: Instant) {
        if !settings.config.enabled {
            self.stop();
        } else if self.in_session && (!before.enabled || self.last_tick.is_none()) {
            self.forget_sync();
            self.restart(now);
        } else {
            self.note_interval_change(settings, before.interval(), now);
        }
    }

    pub(crate) fn enable(&mut self, settings: &mut TickSettings, now: Instant) {
        if !settings.config.enabled {
            self.forget_sync();
        }
        settings.config.enabled = true;
        self.last_tick = Some(now);
        self.warned_this_cycle = false;
    }

    pub(crate) fn disable(&mut self, settings: &mut TickSettings) {
        settings.config.enabled = false;
        self.stop();
    }

    /// Stop the count and forget the game's tick, leaving the settings.
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
    pub(crate) fn reset(&mut self, settings: &TickSettings, now: Instant) {
        if settings.config.enabled {
            self.last_tick = Some(now);
        }
        self.warned_this_cycle = false;
    }

    /// Set the interval. Unsynced it restarts the count and opens a new
    /// warning cycle, as it always has. Synced the interval is only the
    /// expected length, so the count carries on, the expected tick moves,
    /// and a warning this tick already printed does not print again. A
    /// new interval also starts the fallback's wait again.
    pub(crate) fn set_interval(&mut self, settings: &mut TickSettings, secs: u64, now: Instant) {
        let was_interval = settings.config.interval();
        settings.config.interval_secs = secs.max(1);
        if settings.config.enabled && !self.synced {
            self.restart(now);
        } else {
            self.note_interval_change(settings, was_interval, now);
        }
    }

    /// Start the fallback's wait again when the interval moved while a
    /// synced count runs. The wait is twice the interval, so without
    /// this a shorter interval could put the game's last tick past it
    /// and fire the fallback at once while the game still ticks.
    fn note_interval_change(
        &mut self,
        settings: &TickSettings,
        was_interval: Duration,
        now: Instant,
    ) {
        if self.synced && settings.config.interval() != was_interval {
            self.interval_changed_at = Some(now);
        }
    }

    /// Follow the day or night of a World.Time packet `data`. Returns the
    /// new one when it turned.
    pub(crate) fn observe_daylight(&mut self, data: &serde_json::Value) -> Option<Daylight> {
        let now = Daylight::of_world_time(data)?;
        (self.daylight.replace(now) != Some(now)).then_some(now)
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
    pub(crate) fn next_fire(&self, settings: &TickSettings) -> Option<Instant> {
        if !settings.config.enabled {
            return None;
        }
        Some(self.last_tick? + settings.config.interval())
    }

    /// Time left until the expected tick, zero once it has passed.
    pub(crate) fn remaining(&self, settings: &TickSettings, now: Instant) -> Option<Duration> {
        Some(self.next_fire(settings)?.saturating_duration_since(now))
    }

    /// Time since the last tick. `None` while the timer is off.
    pub(crate) fn elapsed(&self, settings: &TickSettings, now: Instant) -> Option<Duration> {
        if !settings.config.enabled {
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
    pub(crate) fn try_consume_fire(&mut self, settings: &TickSettings, now: Instant) -> bool {
        let Some(last) = self.last_tick.filter(|_| settings.config.enabled) else {
            return false;
        };
        let due = if self.synced {
            let from = self.interval_changed_at.map_or(last, |at| at.max(last));
            from + settings.config.interval() * 2
        } else {
            last + settings.config.interval()
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
    pub(crate) fn try_consume_warn(&mut self, settings: &TickSettings, now: Instant) -> bool {
        if !settings.config.enabled || self.warned_this_cycle {
            return false;
        }
        let Some(secs) = settings.config.warn_at_secs else {
            return false;
        };
        let Some(remaining) = self.remaining(settings, now) else {
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
    pub(crate) fn poll(&mut self, settings: &TickSettings, now: Instant) -> TickStep {
        let fired = self.try_consume_fire(settings, now);
        let warned = self.try_consume_warn(settings, now);
        TickStep {
            payload: TickPayload::from_runtime(settings, self, now, fired),
            command: fire_command(settings, fired),
            warn_echo: warned.then(|| warn_echo(&settings.config)),
        }
    }

    /// A real tick from the game: a `World.Time` hour change or a line
    /// that matches the Reset on pattern. The first one syncs the timer.
    /// Each restarts the count and fires once. Returns `None` while the
    /// timer is off and for a signal inside [`SAME_TICK_WINDOW`] of the
    /// tick it belongs to. A real tick just after a local fire restarts
    /// the count on the game's tick and reports without firing again.
    pub(crate) fn on_game_tick(
        &mut self,
        settings: &TickSettings,
        now: Instant,
    ) -> Option<TickStep> {
        if !settings.config.enabled || self.last_tick.is_none() {
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
            payload: TickPayload::from_runtime(settings, self, now, fired),
            command: fire_command(settings, fired),
            warn_echo: None,
        })
    }
}

/// The Send each tick command to run, when the timer fired.
fn fire_command(settings: &TickSettings, fired: bool) -> Option<String> {
    if fired {
        settings.config.auto_fire.clone()
    } else {
        None
    }
}

/// Bring the count of every session on `open` but `session` to the tick
/// settings `open` holds now, which read `before` until `session` changed
/// them. The sessions come from the map before the profile lock, and
/// each one's connection is locked in turn under it, never two at once.
/// Call with no lock held.
pub(crate) async fn follow_in_other_sessions(
    state: &AppState,
    session: SessionId,
    open: &Arc<OpenProfile>,
    before: &TickConfig,
) {
    let others = state.other_sessions(session);
    if others.is_empty() {
        return;
    }
    let p = open.lock().await;
    let now = Instant::now();
    for other in p.players(&others) {
        other.connection.lock().tick.follow(&p.tick, before, now);
    }
}

/// Detect the game's tick from a GMCP `World.Time` push. Aabahran (and
/// most ROM derivatives that ship World.Time) advance the `hour` field
/// every server tick, so an hour change is the tick. Returns the step to
/// deliver when the change counted as a tick.
pub(crate) fn observe_world_time_for_tick(
    settings: &TickSettings,
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
        tick.on_game_tick(settings, now)
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
    pub(crate) fn from_runtime(
        settings: &TickSettings,
        runtime: &TickRuntime,
        now: Instant,
        fired: bool,
    ) -> Self {
        let millis = |d: Duration| d.as_millis() as u64;
        let elapsed = runtime.elapsed(settings, now);
        Self {
            enabled: settings.config.enabled,
            interval_ms: millis(settings.config.interval()),
            remaining_ms: runtime.remaining(settings, now).map_or(0, millis),
            elapsed_ms: elapsed.map_or(0, millis),
            overdue: elapsed.is_some_and(|e| e >= settings.config.interval()),
            synced: runtime.synced,
            fired,
            sound: settings.config.sound,
        }
    }
}

/// The error `tick_set_config` returns for a Reset on pattern that does
/// not compile.
const TICK_RESET_PATTERN_ERROR: &str =
    "Vosh could not read the Reset on pattern. Check it and save again.";

/// Apply a tick configuration from Settings to `settings` and the count
/// `tick`. Checks the Reset on pattern before it changes anything, so a
/// pattern that does not compile leaves the running tick exactly as it
/// was and returns a sentence. Routes interval changes through
/// `TickRuntime::set_interval` so the next-fire deadline rebuilds. Other
/// fields are direct assignments. Returns the configuration as it now
/// reads.
pub(crate) fn apply_tick_config(
    settings: &mut TickSettings,
    tick: &mut TickRuntime,
    config: &TickConfig,
    now: tokio::time::Instant,
) -> Result<TickConfig, String> {
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
        if !settings.config.enabled {
            tick.enable(settings, now);
        }
        tick.set_interval(settings, config.interval_secs, now);
    } else {
        tick.disable(settings);
        // Still record the interval so the user can flip enabled
        // back on without re-typing it.
        settings.config.interval_secs = config.interval_secs.max(1);
    }
    settings.set_compiled_reset_pattern(reset_pattern, reset_regex);
    settings.config.auto_fire = auto_fire;
    settings.config.sound = config.sound;
    settings.config.warn_at_secs = config.warn_at_secs.filter(|s| *s > 0);
    settings.config.warn_message = warn_message;
    settings.config.warn_color = warn_color;

    Ok(settings.config.clone())
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
        let mut s = TickSettings::default();
        let mut t = TickRuntime::default();
        let start = Instant::now();
        t.enable(&mut s, start);
        let remaining = t.remaining(&s, start).unwrap();
        assert_eq!(remaining.as_secs(), DEFAULT_INTERVAL_SECS);
    }

    #[test]
    fn disable_clears_next_fire() {
        let mut s = TickSettings::default();
        let mut t = TickRuntime::default();
        t.enable(&mut s, Instant::now());
        t.disable(&mut s);
        assert!(t.next_fire(&s).is_none());
        assert!(t.remaining(&s, Instant::now()).is_none());
    }

    #[test]
    fn reset_pushes_next_fire() {
        let mut s = TickSettings::default();
        let mut t = TickRuntime::default();
        let start = Instant::now();
        t.enable(&mut s, start);
        let later = now_plus(Duration::from_secs(10));
        t.reset(&s, later);
        let remaining = t.remaining(&s, later).unwrap();
        assert_eq!(remaining.as_secs(), DEFAULT_INTERVAL_SECS);
    }

    #[test]
    fn try_consume_fire_only_after_due() {
        let mut s = TickSettings::default();
        let mut t = TickRuntime::default();
        let start = Instant::now();
        t.enable(&mut s, start);
        assert!(!t.try_consume_fire(&s, start));
        let after = start + s.config.interval();
        assert!(t.try_consume_fire(&s, after));
        // Reschedules to one interval ahead.
        let remaining = t.remaining(&s, after).unwrap();
        assert_eq!(remaining.as_secs(), DEFAULT_INTERVAL_SECS);
    }

    #[test]
    fn set_interval_clamps_to_one_second_minimum() {
        let mut s = TickSettings::default();
        let mut t = TickRuntime::default();
        let now = Instant::now();
        t.set_interval(&mut s, 0, now);
        assert_eq!(s.config.interval(), Duration::from_secs(1));
    }

    #[test]
    fn reset_pattern_compiles_or_fails() {
        let mut s = TickSettings::default();
        assert!(s.set_reset_pattern(Some("good (.*)".into())).is_ok());
        assert!(s.check_reset_match("good morning"));
        assert!(s.set_reset_pattern(Some("[bad".into())).is_err());
    }

    #[test]
    fn check_reset_match_false_when_no_pattern() {
        let s = TickSettings::default();
        assert!(!s.check_reset_match("anything"));
    }

    #[test]
    fn the_game_s_day_and_night_follow_world_time_and_turn_once() {
        use serde_json::json;
        let of = |data: serde_json::Value| Daylight::of_world_time(&data);
        // weather_update, update.c:2326: hour 4 dark, 6 rise, 7 light,
        // 18 set, 19 dark.
        assert_eq!(
            of(json!({"hour": 6, "sunlight": "rise"})),
            Some(Daylight::Day)
        );
        assert_eq!(
            of(json!({"hour": 18, "sunlight": "set"})),
            Some(Daylight::Day)
        );
        assert_eq!(
            of(json!({"hour": 19, "sunlight": "dark"})),
            Some(Daylight::Night)
        );
        // Eternal darkness keeps it dark at noon, update.c:2305.
        assert_eq!(
            of(json!({"hour": 12, "sunlight": "dark"})),
            Some(Daylight::Night)
        );
        // With no sunlight the hour decides.
        assert_eq!(of(json!({"hour": "5"})), Some(Daylight::Night));
        assert_eq!(of(json!({"hour": 6})), Some(Daylight::Day));
        assert_eq!(of(json!({"hour": 19})), Some(Daylight::Night));
        assert_eq!(of(json!({"day": 3})), None);
        let mut t = TickRuntime::default();
        let light = json!({"hour": 14, "sunlight": "light"});
        assert_eq!(t.observe_daylight(&light), Some(Daylight::Day));
        assert_eq!(t.observe_daylight(&light), None, "no turn, no news");
        t.end_session();
        assert_eq!(t.daylight, Some(Daylight::Day), "a drop keeps it");
        let dark = json!({"hour": 19, "sunlight": "dark"});
        assert_eq!(t.observe_daylight(&dark), Some(Daylight::Night));
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
    fn session(start: Instant) -> (TickSettings, TickRuntime) {
        let mut s = TickSettings::default();
        s.config.auto_fire = Some("score".into());
        let mut t = TickRuntime::default();
        t.start_session(&mut s, start);
        (s, t)
    }

    /// Poll every 250 ms from `from` up to and not including `to`, the
    /// way the session loop does, and return the instants that fired.
    fn poll_span(
        s: &TickSettings,
        t: &mut TickRuntime,
        from: Instant,
        to: Instant,
    ) -> Vec<Instant> {
        let mut fired = Vec::new();
        let mut at = from;
        while at < to {
            if t.poll(s, at).payload.fired {
                fired.push(at);
            }
            at += Duration::from_millis(250);
        }
        fired
    }

    #[test]
    fn unsynced_the_local_timer_fires_at_the_interval_as_before() {
        let t0 = Instant::now();
        let (s, mut t) = session(t0);
        assert!(!t.synced);
        let leftover = &poll_span(&s, &mut t, t0, t0 + secs(30.0));
        assert!(leftover.is_empty(), "{leftover:?}");
        let step = t.poll(&s, t0 + secs(30.0));
        assert!(step.payload.fired);
        assert_eq!(step.command.as_deref(), Some("score"));
        // The count restarts from the fire.
        assert_eq!(t.remaining(&s, t0 + secs(30.0)), Some(secs(30.0)));
        assert!(!t.poll(&s, t0 + secs(45.0)).payload.fired);
        assert!(t.poll(&s, t0 + secs(60.0)).payload.fired);
        assert!(!t.synced);
    }

    #[test]
    fn the_first_world_hour_only_primes() {
        let t0 = Instant::now();
        let (_, mut t) = session(t0);
        assert!(!t.observe_world_hour("9"));
        assert!(!t.synced);
    }

    #[test]
    fn a_game_tick_fires_once_and_syncs_the_timer() {
        let t0 = Instant::now();
        let (s, mut t) = session(t0);
        let step = t.on_game_tick(&s, t0 + secs(12.0)).expect("the tick lands");
        assert!(step.payload.fired);
        assert_eq!(step.command.as_deref(), Some("score"));
        assert!(t.synced);
        assert_eq!(t.remaining(&s, t0 + secs(12.0)), Some(secs(30.0)));
    }

    #[test]
    fn once_synced_the_local_timer_no_longer_fires_at_the_interval() {
        let t0 = Instant::now();
        let (s, mut t) = session(t0);
        let _ = t.observe_world_hour("9");
        assert!(t.observe_world_hour("10"));
        let tick = t0 + secs(10.0);
        assert!(t.on_game_tick(&s, tick).is_some());
        // Past the interval the timer waits for the game, overdue.
        let leftover = &poll_span(&s, &mut t, tick, tick + secs(59.0));
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(t.synced);
        assert_eq!(t.remaining(&s, tick + secs(45.0)), Some(Duration::ZERO));
    }

    #[test]
    fn real_ticks_at_25_30_and_35_seconds_each_fire_once_and_restart_the_count() {
        let t0 = Instant::now();
        let (s, mut t) = session(t0);
        let mut tick = t0 + secs(4.0);
        assert!(t.on_game_tick(&s, tick).is_some());
        for gap in [25.0, 30.0, 35.0] {
            let next = tick + secs(gap);
            assert!(
                poll_span(&s, &mut t, tick, next).is_empty(),
                "no local fire in a {gap} second tick"
            );
            let step = t.on_game_tick(&s, next).expect("the tick lands");
            assert!(step.payload.fired, "a {gap} second tick fires");
            assert_eq!(step.command.as_deref(), Some("score"));
            assert_eq!(t.remaining(&s, next), Some(secs(30.0)));
            assert_eq!(t.last_tick, Some(next));
            tick = next;
        }
    }

    #[test]
    fn a_world_hour_change_and_a_reset_line_for_one_tick_fire_once() {
        let t0 = Instant::now();
        let (s, mut t) = session(t0);
        let tick = t0 + secs(20.0);
        assert!(t.on_game_tick(&s, tick).is_some());
        assert!(t.on_game_tick(&s, tick + secs(1.5)).is_none());
        // The second signal leaves the count where the first put it.
        assert_eq!(t.last_tick, Some(tick));
        // Past the window a new signal is a new tick.
        assert!(t.on_game_tick(&s, tick + secs(2.0)).is_some());
    }

    #[test]
    fn a_pattern_that_matches_three_lines_of_one_tick_fires_once() {
        let t0 = Instant::now();
        let (s, mut t) = session(t0);
        let tick = t0 + secs(20.0);
        let fired: Vec<bool> = [0.0, 0.01, 0.02]
            .iter()
            .map(|d| t.on_game_tick(&s, tick + secs(*d)).is_some())
            .collect();
        assert_eq!(fired, [true, false, false]);
    }

    #[test]
    fn a_game_tick_just_after_a_local_fire_restarts_the_count_without_firing_again() {
        let t0 = Instant::now();
        let (s, mut t) = session(t0);
        assert!(t.poll(&s, t0 + secs(30.0)).payload.fired);
        let step = t
            .on_game_tick(&s, t0 + secs(31.0))
            .expect("the count restarts");
        assert!(!step.payload.fired);
        assert_eq!(step.command, None);
        assert!(t.synced);
        assert_eq!(t.remaining(&s, t0 + secs(31.0)), Some(secs(30.0)));
    }

    #[test]
    fn no_game_tick_for_twice_the_interval_fires_once_locally_and_unsyncs() {
        let t0 = Instant::now();
        let (s, mut t) = session(t0);
        let tick = t0 + secs(5.0);
        assert!(t.on_game_tick(&s, tick).is_some());
        let leftover = &poll_span(&s, &mut t, tick, tick + secs(60.0));
        assert!(leftover.is_empty(), "{leftover:?}");
        let step = t.poll(&s, tick + secs(60.0));
        assert!(step.payload.fired);
        assert_eq!(step.command.as_deref(), Some("score"));
        assert!(!t.synced);
        // Back to counting on its own, one interval at a time.
        let fallback = tick + secs(60.0);
        assert_eq!(
            poll_span(&s, &mut t, fallback, fallback + secs(60.25)),
            [fallback + secs(30.0), fallback + secs(60.0)]
        );
        // The next real tick syncs it again.
        assert!(t.on_game_tick(&s, fallback + secs(70.0)).is_some());
        assert!(t.synced);
    }

    #[test]
    fn a_disabled_timer_ignores_game_ticks() {
        let t0 = Instant::now();
        let (mut s, mut t) = session(t0);
        t.disable(&mut s);
        assert!(t.on_game_tick(&s, t0 + secs(10.0)).is_none());
        assert!(!t.synced);
        assert!(!t.poll(&s, t0 + secs(40.0)).payload.fired);
    }

    #[test]
    fn a_new_connection_starts_unsynced_and_primes_the_world_hour_again() {
        let t0 = Instant::now();
        let (mut s, mut t) = session(t0);
        let _ = t.observe_world_hour("9");
        assert!(t.on_game_tick(&s, t0 + secs(10.0)).is_some());
        assert!(t.synced);
        t.end_session();
        let t1 = t0 + secs(100.0);
        t.start_session(&mut s, t1);
        assert!(!t.synced);
        assert_eq!(t.last_world_hour, None);
        // The first hour of the new session primes, even when it moved.
        assert!(!t.observe_world_hour("11"));
        assert_eq!(t.remaining(&s, t1), Some(secs(30.0)));
    }

    #[test]
    fn the_end_of_a_session_stops_the_count_and_keeps_your_tick_setting() {
        let t0 = Instant::now();
        let (s, mut t) = session(t0);
        assert!(t.on_game_tick(&s, t0 + secs(10.0)).is_some());
        t.end_session();
        assert!(s.config.enabled, "the saved setting stays on");
        assert!(!t.in_session);
        assert!(!t.synced);
        assert_eq!(t.next_fire(&s), None);
        assert!(t.on_game_tick(&s, t0 + secs(40.0)).is_none());
    }

    #[test]
    fn a_tick_you_turned_off_stays_off_when_the_session_ends() {
        let t0 = Instant::now();
        let (mut s, mut t) = session(t0);
        t.disable(&mut s);
        t.end_session();
        assert!(!s.config.enabled);
    }

    #[test]
    fn a_connect_beside_a_connected_session_keeps_the_switch_as_it_stands() {
        let t0 = Instant::now();
        let mut s = TickSettings::default();
        s.config.enabled = false;
        let mut t = TickRuntime::default();
        t.join_connected(&s, t0);
        assert!(t.in_session);
        assert_eq!(t.next_fire(&s), None);

        s.config.enabled = true;
        let mut t = TickRuntime::default();
        t.join_connected(&s, t0);
        assert_eq!(t.remaining(&s, t0), Some(secs(30.0)));
    }

    #[test]
    fn the_warning_prints_once_per_cycle_and_not_again_while_overdue() {
        let t0 = Instant::now();
        let (mut s, mut t) = session(t0);
        s.config.warn_at_secs = Some(5);
        let tick = t0 + secs(3.0);
        assert!(t.on_game_tick(&s, tick).is_some());
        let warned = |s: &TickSettings, t: &mut TickRuntime, from: f64, to: f64| -> usize {
            let mut n = 0;
            let mut at = tick + secs(from);
            while at < tick + secs(to) {
                if t.poll(s, at).warn_echo.is_some() {
                    n += 1;
                }
                at += Duration::from_millis(250);
            }
            n
        };
        assert_eq!(warned(&s, &mut t, 0.0, 24.0), 0);
        assert_eq!(warned(&s, &mut t, 24.0, 30.0), 1);
        // The game runs late. No second warning while overdue.
        assert_eq!(warned(&s, &mut t, 30.0, 40.0), 0);
        let late = tick + secs(40.0);
        assert!(t.on_game_tick(&s, late).is_some());
        let step = t.poll(&s, late + secs(26.0));
        assert_eq!(
            step.warn_echo.as_deref(),
            Some("\r\n\x1b[1;31mTICK IN 5s\x1b[0m\r\n")
        );
    }

    /// Count the warnings the session loop prints polling every 250 ms
    /// from `from` up to and not including `to`.
    fn warns_between(s: &TickSettings, t: &mut TickRuntime, from: Instant, to: Instant) -> usize {
        let mut n = 0;
        let mut at = from;
        while at < to {
            if t.poll(s, at).warn_echo.is_some() {
                n += 1;
            }
            at += Duration::from_millis(250);
        }
        n
    }

    #[test]
    fn a_new_interval_while_synced_does_not_warn_again_this_tick() {
        let t0 = Instant::now();
        let (mut s, mut t) = session(t0);
        s.config.warn_at_secs = Some(5);
        let tick = t0 + secs(10.0);
        assert!(t.on_game_tick(&s, tick).is_some());
        assert_eq!(warns_between(&s, &mut t, tick, tick + secs(26.5)), 1);
        // The same interval again, as a Settings save of another field
        // or `#tick interval 30` does, inside the warn window.
        t.set_interval(&mut s, 30, tick + secs(26.5));
        assert_eq!(
            warns_between(&s, &mut t, tick + secs(26.5), tick + secs(33.0)),
            0
        );
        // Overdue, a longer interval puts the expected tick ahead again.
        t.set_interval(&mut s, 35, tick + secs(33.0));
        assert_eq!(
            warns_between(&s, &mut t, tick + secs(33.0), tick + secs(40.0)),
            0
        );
        // The next tick opens a new cycle, and it warns again.
        let next = tick + secs(40.0);
        assert!(t.on_game_tick(&s, next).is_some());
        assert_eq!(warns_between(&s, &mut t, next, next + secs(35.0)), 1);
    }

    #[test]
    fn a_new_interval_unsynced_restarts_the_count_and_the_warning() {
        let t0 = Instant::now();
        let (mut s, mut t) = session(t0);
        s.config.warn_at_secs = Some(5);
        assert_eq!(warns_between(&s, &mut t, t0, t0 + secs(27.0)), 1);
        t.set_interval(&mut s, 30, t0 + secs(27.0));
        assert_eq!(t.last_tick, Some(t0 + secs(27.0)));
        assert_eq!(
            warns_between(&s, &mut t, t0 + secs(27.0), t0 + secs(57.0)),
            1
        );
    }

    #[test]
    fn a_manual_reset_while_synced_restarts_the_count_and_stays_synced() {
        let t0 = Instant::now();
        let (s, mut t) = session(t0);
        assert!(t.on_game_tick(&s, t0 + secs(5.0)).is_some());
        t.reset(&s, t0 + secs(15.0));
        assert!(t.synced);
        assert_eq!(t.remaining(&s, t0 + secs(15.0)), Some(secs(30.0)));
    }

    #[test]
    fn a_new_interval_while_synced_keeps_the_count() {
        let t0 = Instant::now();
        let (mut s, mut t) = session(t0);
        let tick = t0 + secs(5.0);
        assert!(t.on_game_tick(&s, tick).is_some());
        t.set_interval(&mut s, 40, tick + secs(10.0));
        assert_eq!(t.last_tick, Some(tick));
        assert_eq!(t.remaining(&s, tick + secs(10.0)), Some(secs(30.0)));
    }

    // ── A new interval never makes the game look quiet ──────────────

    /// A synced session whose game ticked 5 seconds in, with the time
    /// of that tick.
    fn synced_session(t0: Instant) -> (TickSettings, TickRuntime, Instant) {
        let (s, mut t) = session(t0);
        let tick = t0 + secs(5.0);
        assert!(t.on_game_tick(&s, tick).is_some());
        (s, t, tick)
    }

    #[test]
    fn lowering_the_interval_while_synced_does_not_fire_the_fallback() {
        let t0 = Instant::now();
        let (mut s, mut t, tick) = synced_session(t0);
        // 25 seconds into the tick you set Every to 10, as #tick
        // interval and a Settings save do.
        let change = tick + secs(25.0);
        t.set_interval(&mut s, 10, change);
        let leftover = &poll_span(&s, &mut t, change, tick + secs(30.0));
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(t.synced);
        // The game's tick lands on time and fires once.
        let step = t
            .on_game_tick(&s, tick + secs(30.0))
            .expect("the tick lands");
        assert!(step.payload.fired);
        assert_eq!(step.command.as_deref(), Some("score"));
        assert!(t.synced);
    }

    #[test]
    fn a_profile_with_a_shorter_interval_does_not_fire_the_fallback() {
        let t0 = Instant::now();
        let (mut s, mut t, tick) = synced_session(t0);
        // A switch, #profile load, reset, or an import brings Every 10.
        let before = s.config.clone();
        s.config.interval_secs = 10;
        let change = tick + secs(25.0);
        t.adopt(&mut s, &before, change);
        let leftover = &poll_span(&s, &mut t, change, tick + secs(30.0));
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(t.synced);
        let step = t
            .on_game_tick(&s, tick + secs(30.0))
            .expect("the tick lands");
        assert!(step.payload.fired);
        assert!(t.synced);
    }

    #[test]
    fn after_a_shorter_interval_the_fallback_waits_twice_it_from_the_change() {
        let t0 = Instant::now();
        let (mut s, mut t, tick) = synced_session(t0);
        let change = tick + secs(25.0);
        t.set_interval(&mut s, 10, change);
        // The game goes quiet. Twice the new interval after the change
        // the timer fires once on its own and drops back to unsynced.
        let leftover = &poll_span(&s, &mut t, change, change + secs(20.0));
        assert!(leftover.is_empty(), "{leftover:?}");
        let step = t.poll(&s, change + secs(20.0));
        assert!(step.payload.fired);
        assert_eq!(step.command.as_deref(), Some("score"));
        assert!(!t.synced);
    }

    #[test]
    fn raising_the_interval_while_synced_does_not_fire_the_fallback() {
        let t0 = Instant::now();
        let (mut s, mut t, tick) = synced_session(t0);
        let change = tick + secs(25.0);
        t.set_interval(&mut s, 60, change);
        // The game runs a little late and ticks once.
        let leftover = &poll_span(&s, &mut t, change, tick + secs(35.0));
        assert!(leftover.is_empty(), "{leftover:?}");
        let next = tick + secs(35.0);
        assert!(
            t.on_game_tick(&s, next)
                .expect("the tick lands")
                .payload
                .fired
        );
        // Then it goes quiet, and the fallback waits twice the new one.
        let leftover = &poll_span(&s, &mut t, next, next + secs(120.0));
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(t.synced);
        assert!(t.poll(&s, next + secs(120.0)).payload.fired);
        assert!(!t.synced);
    }

    #[test]
    fn a_profile_with_a_longer_interval_does_not_fire_the_fallback() {
        let t0 = Instant::now();
        let (mut s, mut t, tick) = synced_session(t0);
        let before = s.config.clone();
        s.config.interval_secs = 60;
        let change = tick + secs(25.0);
        t.adopt(&mut s, &before, change);
        let leftover = &poll_span(&s, &mut t, change, tick + secs(35.0));
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(t.synced);
        assert!(
            t.on_game_tick(&s, tick + secs(35.0))
                .expect("the tick lands")
                .payload
                .fired
        );
    }

    // ── A count follows a change from another session ───────────────

    #[test]
    fn following_a_switch_turned_off_stops_the_count() {
        let t0 = Instant::now();
        let (mut s, mut t, _) = synced_session(t0);
        let before = s.config.clone();
        s.config.enabled = false;
        t.follow(&s, &before, t0 + secs(8.0));
        assert_eq!(t.last_tick, None);
        assert!(!t.synced);
        assert!(t.in_session);
    }

    #[test]
    fn following_a_switch_turned_on_starts_the_count_only_in_a_session() {
        let t0 = Instant::now();
        let (mut s, mut t) = session(t0);
        t.disable(&mut s);
        let before = s.config.clone();
        s.config.enabled = true;
        let on = t0 + secs(8.0);
        t.follow(&s, &before, on);
        assert_eq!(t.remaining(&s, on), Some(secs(30.0)));
        // Between connections the count waits for the next one.
        let mut idle = TickRuntime::default();
        idle.follow(&s, &before, on);
        assert_eq!(idle.next_fire(&s), None);
    }

    #[test]
    fn following_a_new_interval_moves_the_expected_tick_and_keeps_the_count() {
        let t0 = Instant::now();
        let (mut s, mut t, tick) = synced_session(t0);
        let before = s.config.clone();
        s.config.interval_secs = 45;
        let change = tick + secs(10.0);
        t.follow(&s, &before, change);
        assert_eq!(t.next_fire(&s), Some(tick + secs(45.0)));
        assert!(t.synced);
        assert_eq!(t.interval_changed_at, Some(change));
    }

    #[test]
    fn a_profile_laid_over_keeps_a_running_tick_on_where_a_change_stops_it() {
        let t0 = Instant::now();
        let (mut s, mut t) = session(t0);
        let before = s.config.clone();
        s.config.enabled = false;
        t.adopt(&mut s, &before, t0 + secs(8.0));
        assert!(s.config.enabled);
        assert_eq!(t.last_tick, Some(t0));

        s.config.enabled = false;
        t.follow(&s, &before, t0 + secs(9.0));
        assert_eq!(t.last_tick, None);
    }

    #[test]
    fn a_new_interval_unsynced_fires_at_the_new_interval_from_the_change() {
        for every in [10, 60] {
            let t0 = Instant::now();
            let (mut s, mut t) = session(t0);
            let change = t0 + secs(25.0);
            t.set_interval(&mut s, every, change);
            let due = change + Duration::from_secs(every);
            assert!(
                poll_span(&s, &mut t, change, due).is_empty(),
                "no fire before a new {every} second interval runs out"
            );
            assert!(
                t.poll(&s, due).payload.fired,
                "fires at the new {every} seconds"
            );
            assert!(!t.synced);
        }
    }

    // ── The terminal warns when the status line does ────────────────

    /// The status line counts the time left in whole seconds rounded up
    /// and warns once that reaches Warn at (computeTick in
    /// src/stores/session/tickStore.ts, whose test checks the same cases).
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
            let (mut s, mut t, tick) = synced_session(t0);
            s.config.warn_at_secs = Some(5);
            assert_eq!(
                t.try_consume_warn(&s, tick + secs(into)),
                warns,
                "{into} seconds into the tick"
            );
        }
    }

    #[test]
    fn the_session_loop_prints_the_warning_on_the_report_that_turns_warn() {
        let t0 = Instant::now();
        let (mut s, mut t, tick) = synced_session(t0);
        s.config.warn_at_secs = Some(5);
        let mut warned = Vec::new();
        let mut at = tick;
        while at < tick + secs(30.0) {
            if t.poll(&s, at).warn_echo.is_some() {
                warned.push(at);
            }
            at += Duration::from_millis(250);
        }
        assert_eq!(warned, [tick + secs(25.0)]);
    }

    #[test]
    fn the_report_carries_the_time_since_the_tick_and_whether_it_is_overdue() {
        let t0 = Instant::now();
        let (mut s, mut t) = session(t0);
        let p = t.poll(&s, t0 + secs(12.5)).payload;
        assert_eq!(
            (p.elapsed_ms, p.remaining_ms, p.interval_ms),
            (12_500, 17_500, 30_000)
        );
        assert!(!p.overdue);
        assert!(!p.synced);

        let tick = t0 + secs(14.0);
        let p = t.on_game_tick(&s, tick).expect("the tick lands").payload;
        assert_eq!((p.elapsed_ms, p.remaining_ms), (0, 30_000));
        assert!(p.synced);
        assert!(p.fired);

        // At the expected tick and past it, the report says overdue and
        // keeps counting while the time left holds at zero.
        let p = t.poll(&s, tick + secs(30.0)).payload;
        assert_eq!((p.elapsed_ms, p.remaining_ms), (30_000, 0));
        assert!(p.overdue);
        let p = t.poll(&s, tick + secs(36.75)).payload;
        assert_eq!((p.elapsed_ms, p.remaining_ms), (36_750, 0));
        assert!(p.overdue);
        assert!(p.synced);
        assert!(!p.fired);

        // Off, the report counts nothing.
        t.disable(&mut s);
        let p = t.poll(&s, tick + secs(40.0)).payload;
        assert!(!p.enabled);
        assert_eq!((p.elapsed_ms, p.remaining_ms), (0, 0));
        assert!(!p.overdue);
        assert!(!p.synced);
    }

    #[test]
    fn the_report_serializes_the_new_fields() {
        let t0 = Instant::now();
        let (s, t) = session(t0);
        let json =
            serde_json::to_value(TickPayload::from_runtime(&s, &t, t0 + secs(2.0), false)).unwrap();
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
    fn tick_payload(reset_pattern: &str) -> super::TickConfig {
        super::TickConfig {
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
    fn running_tick(
        now: tokio::time::Instant,
    ) -> (crate::tick::TickSettings, crate::tick::TickRuntime) {
        let mut s = crate::tick::TickSettings::default();
        let mut tick = crate::tick::TickRuntime::default();
        tick.enable(&mut s, now);
        s.set_reset_pattern(Some("^You feel".into())).unwrap();
        (s, tick)
    }

    #[test]
    fn a_tick_config_with_a_bad_reset_pattern_changes_nothing() {
        let now = tokio::time::Instant::now();
        let (mut s, mut tick) = running_tick(now);
        let before = format!("{:?}", s.config);
        let next_fire = tick.next_fire(&s);

        let err =
            super::apply_tick_config(&mut s, &mut tick, &tick_payload("[bad"), now).unwrap_err();
        assert_eq!(
            err,
            "Vosh could not read the Reset on pattern. Check it and save again."
        );
        // Still on, still every 30 seconds, still on the same clock, and
        // still resetting on the old pattern.
        assert_eq!(format!("{:?}", s.config), before);
        assert!(s.config.enabled);
        assert_eq!(s.config.interval_secs, 30);
        assert_eq!(tick.next_fire(&s), next_fire);
        assert!(s.check_reset_match("You feel less tired."));
    }

    #[test]
    fn a_tick_config_that_reads_applies_every_field() {
        let now = tokio::time::Instant::now();
        let (mut s, mut tick) = running_tick(now);

        let saved =
            super::apply_tick_config(&mut s, &mut tick, &tick_payload(" ^Dawn "), now).unwrap();
        assert!(!saved.enabled);
        assert_eq!(saved.interval_secs, 60);
        assert_eq!(saved.auto_fire.as_deref(), Some("score"));
        assert_eq!(saved.reset_pattern.as_deref(), Some("^Dawn"));
        assert_eq!(saved.warn_at_secs, Some(5));
        assert!(!s.config.enabled);
        assert_eq!(tick.next_fire(&s), None);
        assert_eq!(s.config.interval_secs, 60);
        assert!(!s.config.sound);
        assert!(s.check_reset_match("Dawn breaks."));
        assert!(!s.check_reset_match("You feel less tired."));

        // Turned back on, the tick runs at the saved interval, and a
        // blank pattern clears the reset.
        let mut on = tick_payload("  ");
        on.enabled = true;
        let saved = super::apply_tick_config(&mut s, &mut tick, &on, now).unwrap();
        assert!(saved.enabled);
        assert_eq!(saved.reset_pattern, None);
        assert_eq!(
            tick.next_fire(&s),
            Some(now + std::time::Duration::from_secs(60))
        );
        assert!(!s.check_reset_match("Dawn breaks."));
    }

    #[test]
    fn a_tick_save_inside_the_warn_window_does_not_warn_twice() {
        let t0 = tokio::time::Instant::now();
        let at = |s: f64| t0 + std::time::Duration::from_secs_f64(s);
        let mut s = crate::tick::TickSettings::default();
        let mut tick = crate::tick::TickRuntime::default();
        tick.start_session(&mut s, t0);
        s.config.warn_at_secs = Some(5);
        assert!(tick.on_game_tick(&s, at(1.0)).is_some());
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
                super::apply_tick_config(&mut s, &mut tick, &quiet, at(now)).unwrap();
            }
            if tick.poll(&s, at(now)).warn_echo.is_some() {
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
        let mut s = crate::tick::TickSettings::default();
        let mut tick = crate::tick::TickRuntime::default();
        tick.start_session(&mut s, t0);
        s.config.auto_fire = Some("score".into());
        assert!(tick.on_game_tick(&s, at(1.0)).is_some());
        // 25 seconds into the tick, Settings saves Every 10.
        let mut shorter = tick_payload("");
        shorter.enabled = true;
        shorter.interval_secs = 10;
        super::apply_tick_config(&mut s, &mut tick, &shorter, at(26.0)).unwrap();
        let mut now = 26.0;
        while now < 31.0 {
            let step = tick.poll(&s, at(now));
            assert!(!step.payload.fired, "no fallback at {now}");
            assert_eq!(step.command, None);
            now += 0.25;
        }
        assert!(tick.synced);
        let step = tick.on_game_tick(&s, at(31.0)).expect("the tick lands");
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
        let mut s = crate::tick::TickSettings::default();
        let mut tick = crate::tick::TickRuntime::default();
        s.config.auto_fire = Some("score".into());
        tick.start_session(&mut s, t0);
        let at = |s: u64| t0 + std::time::Duration::from_secs(s);

        // The first hour of the session primes.
        assert!(
            super::observe_world_time_for_tick(&s, &mut tick, &world_time(9.into()), at(1))
                .is_none()
        );
        // The same hour again is no tick.
        assert!(
            super::observe_world_time_for_tick(&s, &mut tick, &world_time(9.into()), at(5))
                .is_none()
        );
        let step =
            super::observe_world_time_for_tick(&s, &mut tick, &world_time("10".into()), at(12))
                .expect("the hour moved");
        assert!(step.payload.fired);
        assert_eq!(step.command.as_deref(), Some("score"));
        assert!(tick.synced);
        // A Reset on line for the same tick does not fire again.
        assert!(tick.on_game_tick(&s, at(13)).is_none());
        // Past the interval the timer waits for the next hour.
        assert!(!tick.poll(&s, at(45)).payload.fired);
        let step =
            super::observe_world_time_for_tick(&s, &mut tick, &world_time(11.into()), at(46))
                .expect("the next tick");
        assert!(step.payload.fired);

        // Other packages and a World.Time without an hour are no tick.
        let other = vosh_protocol::gmcp::Message {
            package: "Char.Vitals".into(),
            data: serde_json::json!({ "hour": 12 }),
        };
        assert!(super::observe_world_time_for_tick(&s, &mut tick, &other, at(80)).is_none());
        let no_hour = vosh_protocol::gmcp::Message {
            package: "World.Time".into(),
            data: serde_json::json!({ "sunlight": "light" }),
        };
        assert!(super::observe_world_time_for_tick(&s, &mut tick, &no_hour, at(80)).is_none());
    }
}

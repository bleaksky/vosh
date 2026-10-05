//! The five alert presets (Alerts Q5), matched from GMCP where the game
//! says it plainly. Each one is on while `ui.enabled_presets` lists its
//! id, and the profile's `[alerts]` table says what it does. All five
//! ship off, so none rings until you turn it on.
//!
//! - Tells you get: Comm.Channel with channel `tell` and direction
//!   `received`, once per sender in 10 seconds.
//! - Your name: a line of the game that holds the name Char.Status gave,
//!   as a whole word with its capital. Lines that start with `You ` are
//!   your own, so they stay quiet.
//! - Being attacked: Char.Combat going from no target to one, quiet when
//!   it names a groupmate other than you as the tank, or when a line of
//!   yours left in the 2 seconds before, since you likely began it.
//! - Low health: the low latch rising, under 20 percent and clear again
//!   at 25, the twin of `nextLow` in src/lib/stores/vitalsStore.ts, held
//!   to it by fixtures/alerts/low-latch.json. It never rings while the
//!   game hides your vitals.
//! - Connection: the session itself, at a drop while you play, when a
//!   redial reaches the game's prompt, and when Vosh stops trying.

use std::time::Duration;

use tokio::time::Instant;
use vosh_protocol::gmcp::Message;

use super::{Alert, AlertParts};
use crate::loadouts::presets::PRESETS_OFF;
use crate::profile::live::Profile;

pub(crate) const TELLS: &str = "alert_tells";
pub(crate) const NAME: &str = "alert_name";
pub(crate) const ATTACKED: &str = "alert_attacked";
pub(crate) const LOW_HEALTH: &str = "alert_low_health";
pub(crate) const CONNECTION: &str = "alert_connection";

/// Every alert preset, in the order the Alerts category lists them.
pub(crate) const PRESETS: [&str; 5] = [TELLS, NAME, ATTACKED, LOW_HEALTH, CONNECTION];

/// Below this percent your health turns low, as `LEDGER_LOW_ENTER`.
const LOW_ENTER: i64 = 20;
/// A low health leaves the state only at this percent, as
/// `LEDGER_LOW_EXIT`, so regen across the line never flickers.
const LOW_EXIT: i64 = 25;

/// A fight that starts this soon after a line of yours left stays quiet.
const YOUR_FIGHT: Duration = Duration::from_secs(2);

/// What the presets follow on one connection. It sits on the session's
/// [`Connection`](crate::session::connection::Connection), since the
/// line pipeline changes it, and starts over at each connect.
#[derive(Debug, Default)]
pub(crate) struct Watch {
    /// Your name, from Char.Status. Your name stays quiet until it comes,
    /// and a drop clears it.
    name: Option<String>,
    /// The low latch on your health.
    low: bool,
    /// Whom you fight, from the latest Char.Combat.
    target: Option<String>,
}

impl Watch {
    /// A connection opened or ended, so nothing it followed carries on.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// What a GMCP packet rings. `last_line` is when a line of yours last
    /// left for the game.
    pub(crate) fn gmcp(
        &mut self,
        p: &Profile,
        msg: &Message,
        last_line: Option<Instant>,
        now: Instant,
    ) -> Option<Alert> {
        match msg.package.as_str() {
            "Char.Status" => {
                if let Some(name) = msg.data.get("name").and_then(|v| v.as_str()) {
                    let name = name.trim();
                    if !name.is_empty() {
                        self.name = Some(name.to_string());
                    }
                }
                None
            }
            "Comm.Channel" => tell(p, msg),
            "Char.Combat" => {
                let target = msg
                    .data
                    .get("target")
                    .and_then(|v| v.as_str())
                    .filter(|t| !t.is_empty())
                    .map(str::to_string);
                let started = self.target.is_none() && target.is_some();
                let tank = msg
                    .data
                    .get("tank")
                    .and_then(|t| t.get("name"))
                    .and_then(|v| v.as_str());
                self.target.clone_from(&target);
                let parts = parts(p, ATTACKED)?;
                if !started {
                    return None;
                }
                // A groupmate tanks, as when autoassist pulls you in. With
                // your name not known yet no tank can be told apart.
                if tank.is_some() && tank != self.name.as_deref() {
                    return None;
                }
                if last_line.is_some_and(|at| now.duration_since(at) < YOUR_FIGHT) {
                    return None;
                }
                let target = target?;
                Some(preset(
                    ATTACKED,
                    format!("{} attacked you", capitalized(&target)),
                    None,
                    parts,
                ))
            }
            _ => None,
        }
    }

    /// What a line of the game that is not your prompt rings: your name.
    pub(crate) fn line(&self, p: &Profile, plain: &str) -> Option<Alert> {
        let name = self.name.as_deref()?;
        if plain.starts_with("You ") || !names(plain, name) {
            return None;
        }
        let parts = parts(p, NAME)?;
        Some(preset(
            NAME,
            "Someone named you".into(),
            Some(plain.to_string()),
            parts,
        ))
    }

    /// Follow your health, `hp` of `maxhp`, or `hidden` while the game
    /// hides your vitals, and ring when it turns low.
    pub(crate) fn health(
        &mut self,
        p: &Profile,
        hp: i64,
        maxhp: i64,
        hidden: bool,
    ) -> Option<Alert> {
        let was = self.low;
        self.low = !hidden && next_low(was, hp, maxhp);
        if was || !self.low {
            return None;
        }
        let parts = parts(p, LOW_HEALTH)?;
        Some(preset(
            LOW_HEALTH,
            format!("Health at {}%", vital_percent(hp, maxhp)),
            None,
            parts,
        ))
    }
}

/// Your health as the vitals panes read it from `vars`: `hp` and
/// `maxhp` from the fresh prompt values a capture or Lua set, or else
/// from Char.Vitals, and whether the game hides your vitals, by the
/// packet's own flag or by what Vosh worked out and told the panes.
/// None before either source gave a value.
pub(crate) fn health(vars: &vosh_prompt::values::Vars) -> Option<(i64, i64, bool)> {
    let vitals = vars.gmcp().vitals();
    let fresh = vars.prompt_vars();
    let pick = |key: &str, gmcp: Option<i64>| {
        fresh
            .get(key)
            .and_then(|v| v.trim().parse::<i64>().ok())
            .or(gmcp)
    };
    let hp = pick("hp", vitals.as_ref().and_then(|v| v.hp));
    let maxhp = pick("maxhp", vitals.as_ref().and_then(|v| v.maxhp));
    let hidden = vitals.as_ref().is_some_and(|v| v.hidden) || vars.reported().vitals();
    if hp.is_none() && maxhp.is_none() && !hidden {
        return None;
    }
    Some((hp.unwrap_or(0), maxhp.unwrap_or(0), hidden))
}

/// A tell you got, from Comm.Channel.
fn tell(p: &Profile, msg: &Message) -> Option<Alert> {
    let field = |key: &str| msg.data.get(key).and_then(|v| v.as_str());
    if field("channel") != Some("tell") || field("direction") != Some("received") {
        return None;
    }
    let parts = parts(p, TELLS)?;
    let speaker = field("speaker")
        .filter(|s| !s.is_empty())
        .unwrap_or("someone");
    Some(Alert {
        cap: format!("preset:{TELLS}:{speaker}"),
        ..preset(
            TELLS,
            format!("Tell from {speaker}"),
            field("text").map(str::to_string),
            parts,
        )
    })
}

fn preset(id: &str, title: String, words: Option<String>, parts: AlertParts) -> Alert {
    Alert {
        cap: format!("preset:{id}"),
        source: format!("preset:{id}"),
        title,
        words,
        parts,
        owner: None,
    }
}

/// What the preset `id` does while `p` has it on, or None while it is
/// off. A preset the `[alerts]` table names no parts for posts a banner.
pub(crate) fn parts(p: &Profile, id: &str) -> Option<AlertParts> {
    let list = &p.ui.enabled_presets;
    if !list.iter().any(|on| on == id) || list.iter().any(|on| on == PRESETS_OFF) {
        return None;
    }
    Some(p.alerts.get(id).cloned().unwrap_or(AlertParts {
        banner: true,
        ..AlertParts::default()
    }))
}

/// Whether `line` holds `name` as a whole word, with its capital.
fn names(line: &str, name: &str) -> bool {
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '\'');
    line.match_indices(name).any(|(at, _)| {
        let before = line[..at].chars().next_back();
        let after = line[at + name.len()..].chars().next();
        !word(before) && !(after.is_some_and(char::is_alphanumeric))
    })
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Whole percent, 0 to 100, and 0 with no max, as `vitalPercent`, which
/// rounds a half up.
pub(crate) fn vital_percent(current: i64, max: i64) -> i64 {
    if max <= 0 || current <= 0 {
        return 0;
    }
    let pct = current.saturating_mul(200).saturating_add(max) / max.saturating_mul(2);
    pct.min(100)
}

/// The next low latch for one vital, as `nextLow`: low under 20 percent,
/// and once low, until 25. A vital with no max is never low.
pub(crate) fn next_low(was_low: bool, current: i64, max: i64) -> bool {
    if max <= 0 {
        return false;
    }
    let pct = vital_percent(current, max);
    if was_low {
        pct < LOW_EXIT
    } else {
        pct < LOW_ENTER
    }
}

#[cfg(test)]
mod tests;

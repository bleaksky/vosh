//! The five alert presets, matched from GMCP where the game
//! says it plainly. Each one is on while `ui.enabled_presets` lists its
//! id, and the profile's `[alerts]` table says what it does. All five
//! ship off, so none rings until you turn it on. A preset that is off
//! still raises its alert, with nothing on, so the row of a session you
//! are not looking at takes the dot all the same.
//!
//! - Tells you get: Comm.Channel with channel `tell` and direction
//!   `received`, once per sender in 10 seconds.
//! - Your name: a line of the game that holds the name Char.Status gave,
//!   as a whole word with its capital. Lines that start with `You ` are
//!   your own, so they stay quiet.
//! - Being attacked: Char.Combat going from no target to one, quiet when
//!   it names a groupmate other than you as the tank, or when a line of
//!   yours left in the 2 seconds before, since you likely began it. The
//!   game names a tank only in your group, you among it, and as you see
//!   them, so a form or a conceal names you by a short description. The
//!   last Group.Info says who else is in it.
//! - Low health: the low latch rising, under 20 percent and clear again
//!   at 25, the twin of `nextLow` in src/stores/gmcp/vitalsStore.ts, held
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
pub(crate) struct PresetWatch {
    /// Your name, from Char.Status. Your name stays quiet until it comes,
    /// and a drop clears it.
    name: Option<String>,
    /// The low latch on your health.
    low: bool,
    /// Whom you fight, from the latest Char.Combat.
    target: Option<String>,
    /// Your group, from the latest Group.Info.
    group: Group,
}

/// Your group as the latest Group.Info gives it.
#[derive(Debug, Default)]
enum Group {
    /// No Group.Info yet, or the game hides it under lamented tears.
    #[default]
    Unknown,
    /// You fight alone, `Group.Info {}` (gmcp.c:796), so you are your own
    /// tank whatever the game calls you.
    Alone,
    /// The members as you see them, you among them.
    Members(Vec<String>),
}

impl Group {
    fn of(data: &serde_json::Value) -> Self {
        if data.get("hidden").and_then(serde_json::Value::as_bool) == Some(true) {
            return Self::Unknown;
        }
        match data.get("members").and_then(serde_json::Value::as_array) {
            Some(members) => Self::Members(
                members
                    .iter()
                    .filter_map(|m| m.get("name").and_then(serde_json::Value::as_str))
                    .map(str::to_string)
                    .collect(),
            ),
            None if data.as_object().is_some_and(serde_json::Map::is_empty) => Self::Alone,
            None => Self::Unknown,
        }
    }

    /// Whether `tank`, whom the target hits, is a groupmate other than
    /// you, `name`, as far as the group tells.
    fn other_tanks(&self, tank: &str, name: Option<&str>) -> bool {
        if name == Some(tank) {
            return false;
        }
        match self {
            // With no group known, a tank by another name is someone
            // else, and with your name not known yet no tank can be told
            // apart.
            Self::Unknown => true,
            Self::Alone => false,
            Self::Members(members) => members.iter().any(|member| member == tank),
        }
    }
}

/// What the connection alert says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Link {
    /// The link dropped while you played.
    Lost,
    /// A redial reached the game's prompt, so you can log in.
    Ready,
    /// Vosh stopped trying.
    Stopped,
}

impl PresetWatch {
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
            // Being attacked reads the group whether its alert is on or
            // off, since a fight that starts on you marks a row either way.
            "Group.Info" => {
                self.group = Group::of(&msg.data);
                None
            }
            "Char.Combat" => {
                let target = msg
                    .data
                    .get("target")
                    .and_then(|v| v.as_str())
                    .filter(|t| !t.is_empty());
                let started = self.target.is_none() && target.is_some();
                let tank = msg
                    .data
                    .get("tank")
                    .and_then(|t| t.get("name"))
                    .and_then(|v| v.as_str());
                // Each round of a fight sends the same target, which this
                // keeps without a copy.
                if self.target.as_deref() != target {
                    self.target = target.map(str::to_string);
                }
                let parts = heard(p, ATTACKED);
                if !started {
                    return None;
                }
                // A groupmate tanks, as when autoassist pulls you in.
                if tank.is_some_and(|tank| self.group.other_tanks(tank, self.name.as_deref())) {
                    return None;
                }
                if last_line.is_some_and(|at| now.duration_since(at) < YOUR_FIGHT) {
                    return None;
                }
                let target = target?;
                Some(preset(
                    ATTACKED,
                    format!("{} attacked you", capitalized(target)),
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
        let parts = heard(p, NAME);
        if plain.starts_with("You ") || !names(plain, name) {
            return None;
        }
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
        let parts = heard(p, LOW_HEALTH);
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

/// What the connection preset raises for `link`, with nothing on while
/// it is off. Each turn of the link counts under a cap of its own, so a
/// redial that reaches the prompt 3 seconds after the drop still rings.
pub(crate) fn connection(p: &Profile, link: Link) -> Alert {
    let (key, title) = match link {
        Link::Lost => ("lost", "Connection lost"),
        Link::Ready => ("ready", "Ready to log in"),
        Link::Stopped => ("stopped", "Vosh stopped trying"),
    };
    Alert {
        cap: format!("preset:{CONNECTION}:{key}"),
        ..preset(CONNECTION, title.into(), None, heard(p, CONNECTION))
    }
}

/// A tell you got, from Comm.Channel.
fn tell(p: &Profile, msg: &Message) -> Option<Alert> {
    let field = |key: &str| msg.data.get(key).and_then(|v| v.as_str());
    if field("channel") != Some("tell") || field("direction") != Some("received") {
        return None;
    }
    let parts = heard(p, TELLS);
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

/// What the preset `id` does in `p`: its parts while it is on, and
/// nothing while it is off, so its alert only marks a row.
fn heard(p: &Profile, id: &str) -> AlertParts {
    parts(p, id).unwrap_or_default()
}

/// Whether `line` holds `name` as a whole word, with its capital. A
/// quote may open right before it, as in `$n yells '$t'`, and a
/// possessive may follow, as in `Orla's`.
fn names(line: &str, name: &str) -> bool {
    let word = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
    line.match_indices(name).any(|(at, _)| {
        let before = line[..at].chars().next_back();
        let after = line[at + name.len()..].chars().next();
        !word(before) && !word(after)
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

//! Alerts that reach you while you look elsewhere (the Alerts and Scenes
//! review, Q1 to Q6 and Q19, with Sessions Q10). An alert comes from a
//! trigger's alert table, from one of the five alert presets, or from
//! `mud.alert` in Lua, and each one rings the same way.
//!
//! - The line pipeline and the GMCP handler work out which alerts a line
//!   or a packet raises, under the profile and connection locks, and hand
//!   them on in the step's [`crate::script::ApplyResult`].
//! - [`ring`] takes them once those locks let go. It asks [`focus`]
//!   whether you look at the session, which takes the session map, holds
//!   each to the 10 second cap the session keeps, posts the banner,
//!   bounces the Dock or flashes the taskbar, and tells the page through
//!   `session://alert`, which plays the tone.
//! - [`banner`] is where a banner goes, the system or, in a test build,
//!   a list the test reads, so no test ever posts one.

pub(crate) mod banner;
pub(crate) mod focus;

#[cfg(target_os = "macos")]
mod mac;

#[cfg(not(target_os = "macos"))]
mod desktop;

use std::collections::HashMap;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tokio::time::Instant;
pub(crate) use vosh_automation::alert::{AlertParts, Attention};

use crate::app::events;
use crate::app::state::SharedState;
use crate::sessions::Session;

/// How long an alert waits before it rings again under the same key: a
/// trigger, a preset, a tell from one sender, or one title from one
/// piece of Lua.
pub(crate) const CAP: Duration = Duration::from_secs(10);

/// One alert, worked out under the locks, that rings once they let go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Alert {
    /// What the 10 second cap counts it under, such as `trigger:visitor`
    /// or `preset:alert_tells:Tolliver`.
    pub(crate) cap: String,
    /// Where it came from, for the page: `trigger:<name>`,
    /// `preset:<id>` or `lua:<owner>`.
    pub(crate) source: String,
    /// The banner's title, such as `Tell from Tolliver`.
    pub(crate) title: String,
    /// What a banner shows under the title with Title and words, the line
    /// a trigger matched or what was said.
    pub(crate) words: Option<String>,
    pub(crate) parts: AlertParts,
    /// The owner tag of the Lua that raised it, such as
    /// `plugin:vitals_alert`, so turning the plugin off ends its alerts.
    pub(crate) owner: Option<String>,
}

impl Alert {
    /// The alert of the trigger `trigger` that matched `line`.
    pub(crate) fn of_trigger(alert: &vosh_automation::trigger::TriggerAlert, line: &str) -> Self {
        Self {
            cap: format!("trigger:{}", alert.trigger),
            source: format!("trigger:{}", alert.trigger),
            title: alert.trigger.clone(),
            words: Some(line.to_string()),
            parts: alert.parts.clone(),
            owner: None,
        }
    }
}

/// When each key last rang in one session, for the 10 second cap. The
/// session keeps it under a leaf lock, taken alone once the profile and
/// connection locks let go.
#[derive(Debug, Default)]
pub(crate) struct Caps(HashMap<String, Instant>);

impl Caps {
    /// Whether `key` may ring at `now`, and if so, mark that it rang.
    pub(crate) fn allow(&mut self, key: &str, now: Instant) -> bool {
        self.0.retain(|_, at| now.duration_since(*at) < CAP);
        if self.0.contains_key(key) {
            return false;
        }
        self.0.insert(key.to_string(), now);
        true
    }
}

/// `session://alert`: an alert rang. The page plays `sound`, shows its
/// own notice when `notice` says so, and marks the session's row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct AlertPayload {
    pub(crate) title: String,
    /// The session as its row reads, which the banner names under the
    /// title.
    pub(crate) label: Option<String>,
    /// What was said or the line that matched, with Title and words on.
    pub(crate) words: Option<String>,
    /// The tone the page plays, or None when the alert has none or Vosh
    /// played a system sound in its place.
    pub(crate) sound: Option<String>,
    /// A system banner went out.
    pub(crate) banner: bool,
    /// Vosh is in front and you look at another session, so the page
    /// shows a notice of its own in place of a banner.
    pub(crate) notice: bool,
    pub(crate) source: String,
    pub(crate) owner: Option<String>,
}

/// Ring each of `alerts` that `session` raised, with no lock held. Each
/// one follows the focus rule, then the 10 second cap, and what rings
/// posts its banner, asks for attention and tells the page.
pub(crate) fn ring<R: tauri::Runtime>(app: &AppHandle<R>, session: &Session, alerts: Vec<Alert>) {
    if alerts.is_empty() {
        return;
    }
    let Some(state) = app.try_state::<SharedState>() else {
        return;
    };
    let seen = focus::seen(&state, session.id);
    let label = session.label();
    let now = Instant::now();
    for alert in alerts {
        let Some(fate) = focus::fate(&alert.parts, seen) else {
            continue;
        };
        if !session.allow_alert(&alert.cap, now) {
            continue;
        }
        let words = alert.parts.words.then(|| alert.words.clone()).flatten();
        let played = state.banners.post(
            app,
            &banner::Banner {
                session: session.id,
                title: alert.title.clone(),
                label: label.clone(),
                words: words.clone(),
                owner: alert.owner.clone(),
            },
            &fate,
        );
        session.emit(
            app,
            events::ALERT,
            &AlertPayload {
                title: alert.title,
                label: label.clone(),
                words,
                sound: if played { None } else { fate.sound.clone() },
                banner: fate.banner,
                notice: fate.notice,
                source: alert.source,
                owner: alert.owner,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_rings_once_in_ten_seconds_and_each_key_counts_alone() {
        let mut caps = Caps::default();
        let start = Instant::now();
        assert!(caps.allow("preset:alert_tells:Tolliver", start));
        assert!(!caps.allow(
            "preset:alert_tells:Tolliver",
            start + Duration::from_secs(9)
        ));
        assert!(caps.allow("preset:alert_tells:Maren", start + Duration::from_secs(9)));
        assert!(caps.allow("preset:alert_tells:Tolliver", start + CAP));
    }
}

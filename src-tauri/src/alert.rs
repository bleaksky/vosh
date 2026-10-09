//! Alerts that reach you while you look elsewhere. An alert comes from a
//! trigger's alert table, from one of the five alert presets, or from
//! `mud.alert` in Lua, and each one rings the same way.
//!
//! - The line pipeline and the GMCP handler work out which alerts a line
//!   or a packet raises, under the profile and connection locks, and hand
//!   them on in the step's [`crate::script::ApplyResult`], as the Lua does
//!   for `mud.alert`. A plugin that turns off, stops or loads again ends
//!   its alerts through [`end_owner`].
//! - [`ring`] takes them once those locks let go. It asks [`focus`]
//!   whether you look at the session, which takes the session map, holds
//!   each to the 10 second cap the session keeps, posts the banner,
//!   bounces the Dock or flashes the taskbar, and tells the page through
//!   `session://alert`, which plays the tone. An alert that rings nothing
//!   still tells the page through `session://mark` in a session you are
//!   not looking at, once for each such alert with its source, so its row
//!   counts and names what waits.
//! - [`presets`] matches the five presets from GMCP, the text and the
//!   link, and keeps the low latch, the Rust twin of `nextLow`. A preset
//!   that is off still raises its alerts, with nothing on, so they mark.
//! - [`banner`] is where a banner goes, the system or, in a test build,
//!   a list the test reads, so no test ever posts one.

pub(crate) mod banner;
pub(crate) mod focus;
pub(crate) mod presets;

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
    /// Where it came from, for the page: `trigger:<name>`, with the
    /// unit separator and the group after the name for a trigger in a
    /// group, `preset:<id>` or `lua:<owner>`.
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
    /// The alert of the trigger `trigger` that matched `line`. Two groups
    /// may each hold a trigger of one name, so the key of one in a group
    /// carries the group too, and each rings under its own cap.
    pub(crate) fn of_trigger(alert: &vosh_automation::trigger::TriggerAlert, line: &str) -> Self {
        let key = match &alert.group {
            Some(group) => format!("trigger:{}\u{1f}{group}", alert.trigger),
            None => format!("trigger:{}", alert.trigger),
        };
        Self {
            cap: key.clone(),
            source: key,
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

    /// Forget every key of the Lua `owner`, whose alerts ended.
    pub(crate) fn forget_owner(&mut self, owner: &str) {
        let prefix = format!("lua:{owner}:");
        self.0.retain(|key, _| !key.starts_with(&prefix));
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

/// `session://alerts-ended`: the Lua `owner` turned off, stopped or
/// loaded again, so its notices go and its banners are taken back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct AlertsEnded {
    pub(crate) owner: String,
}

/// The alert `mud.alert` raised for the Lua of `owner`, whose tag, such
/// as `plugin:vitals_alert`, it carries.
pub(crate) fn of_lua(
    owner: &vosh_script::Owner,
    title: String,
    text: Option<String>,
    parts: AlertParts,
) -> Alert {
    let tag = owner.tag();
    Alert {
        cap: format!("lua:{tag}:{title}"),
        source: format!("lua:{tag}"),
        title,
        words: text,
        parts,
        owner: Some(tag),
    }
}

/// `session://mark`: something for you happened in a session behind and
/// rang nothing. One goes out for each such alert, so the row counts them.
#[derive(Serialize)]
struct Mark {
    /// Where the alert came from, as [`Alert::source`] reads.
    source: String,
}

/// Ring each of `alerts` that `session` raised, with no lock held. Each
/// one follows the focus rule, then the 10 second cap, and what rings
/// posts its banner, asks for attention and tells the page. In a session
/// other than the selected one, each alert that rings nothing, since it
/// is off or quiet or the cap holds it back, still marks the row.
pub(crate) fn ring<R: tauri::Runtime>(app: &AppHandle<R>, session: &Session, alerts: Vec<Alert>) {
    if alerts.is_empty() {
        return;
    }
    let Some(state) = app.try_state::<SharedState>() else {
        return;
    };
    let seen = focus::seen(&state, session.id);
    let behind = state.selected_session().id != session.id;
    let label = session.label(&state.other_sessions(session.id));
    let now = Instant::now();
    for alert in alerts {
        let fate = focus::fate(&alert.parts, seen).filter(|_| session.allow_alert(&alert.cap, now));
        let Some(fate) = fate else {
            if behind {
                let source = alert.source;
                session.emit(app, events::MARK, &Mark { source });
            }
            continue;
        };
        let words = alert.parts.words.then(|| alert.words.clone()).flatten();
        let banner = banner::Banner {
            session: session.id,
            title: alert.title.clone(),
            label: label.clone(),
            words: words.clone(),
            owner: alert.owner.clone(),
        };
        let payload = AlertPayload {
            title: alert.title,
            label: label.clone(),
            words,
            sound: fate.sound.clone(),
            banner: fate.banner,
            notice: fate.notice,
            source: alert.source,
            owner: alert.owner,
        };
        let (to, id) = (app.clone(), session.id);
        state.banners.post(app, banner, fate, move |played| {
            // A system sound played in place of the page's tone.
            let payload = if played {
                AlertPayload {
                    sound: None,
                    ..payload
                }
            } else {
                payload
            };
            crate::sessions::emit_for(&to, id, events::ALERT, &payload);
        });
    }
}

/// End the alerts of the Lua `owner` in `session`: its caps go, its
/// banners still showing are taken back, and the page drops its notices.
pub(crate) fn end_owner<R: tauri::Runtime>(app: &AppHandle<R>, session: &Session, owner: &str) {
    session.forget_alert_owner(owner);
    if let Some(state) = app.try_state::<SharedState>() {
        state.banners.withdraw(session.id, owner);
    }
    session.emit(
        app,
        events::ALERTS_ENDED,
        &AlertsEnded {
            owner: owner.to_string(),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lua_owner_whose_alerts_end_rings_again_at_once() {
        let mut caps = Caps::default();
        let now = Instant::now();
        assert!(caps.allow("lua:plugin:vitals_alert:Health low", now));
        assert!(caps.allow("lua:plugin:other:Health low", now));
        caps.forget_owner("plugin:vitals_alert");
        assert!(caps.allow("lua:plugin:vitals_alert:Health low", now));
        assert!(!caps.allow("lua:plugin:other:Health low", now));
    }

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

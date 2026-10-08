//! Whether you look at a session, which decides what an alert does.
//! Vosh counts as in front while any of its windows has focus, Settings
//! and Help included, and a session counts as in front only while Vosh
//! does and the session is the one selected. Rust keeps the windows
//! that have focus from `WindowEvent::Focused`, so the session decides
//! alone.

use std::collections::BTreeSet;
use std::sync::Mutex;

use super::{AlertParts, Attention};
use crate::app::state::AppState;
use crate::sessions::SessionId;

/// The windows of Vosh that have focus, by label. A leaf lock, held for
/// a copy.
#[derive(Debug, Default)]
pub(crate) struct Focus(Mutex<BTreeSet<String>>);

impl Focus {
    /// The window `label` gained or lost focus, or closed.
    pub(crate) fn set(&self, label: &str, focused: bool) {
        let mut windows = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if focused {
            windows.insert(label.to_string());
        } else {
            windows.remove(label);
        }
    }

    /// Whether any window of Vosh has focus.
    pub(crate) fn front(&self) -> bool {
        !self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
    }
}

/// How you see a session as an alert from it rings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Seen {
    /// Vosh is in front and the session is selected, so you see it.
    Looking,
    /// Vosh is in front and you look at another session.
    Behind,
    /// Vosh is in the background.
    Away,
}

/// How you see the session `id` now. Takes the session map, so call it
/// with no profile or connection held.
pub(crate) fn seen(state: &AppState, id: SessionId) -> Seen {
    if !state.focus.front() {
        return Seen::Away;
    }
    if state.selected_session().id == id {
        Seen::Looking
    } else {
        Seen::Behind
    }
}

/// What an alert does once the focus rule has spoken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fate {
    /// Post a system banner.
    pub(crate) banner: bool,
    /// The tone to play.
    pub(crate) sound: Option<String>,
    /// Bounce the Dock or flash the taskbar.
    pub(crate) attention: Option<Attention>,
    /// The page shows a notice of its own, with Show, in place of a
    /// banner over the window you read.
    pub(crate) notice: bool,
}

/// What an alert with `parts` does while you see its session as `seen`,
/// or None when it stays quiet.
///
/// - In the background, every part it has rings.
/// - In front on another session, its tone plays and the page shows a
///   notice in the corner, with no banner and no bounce.
/// - Looking at its session, it rings only when Only while you are not
///   looking at its session is off, with its banner and its tone. The
///   Dock bounce does nothing while Vosh is in front, so it asks none.
pub(crate) fn fate(parts: &AlertParts, seen: Seen) -> Option<Fate> {
    if parts.is_silent() {
        return None;
    }
    match seen {
        Seen::Away => Some(Fate {
            banner: parts.banner,
            sound: parts.sound.clone(),
            attention: parts.attention,
            notice: false,
        }),
        Seen::Behind => Some(Fate {
            banner: false,
            sound: parts.sound.clone(),
            attention: None,
            notice: true,
        }),
        Seen::Looking if parts.background => None,
        Seen::Looking => Some(Fate {
            banner: parts.banner,
            sound: parts.sound.clone(),
            attention: None,
            notice: false,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_part() -> AlertParts {
        AlertParts {
            banner: true,
            sound: Some("chime".into()),
            attention: Some(Attention::Until),
            background: true,
            words: false,
        }
    }

    #[test]
    fn vosh_is_in_front_while_any_window_has_focus() {
        let focus = Focus::default();
        assert!(!focus.front());
        focus.set("settings", true);
        focus.set("main", true);
        focus.set("settings", false);
        assert!(focus.front());
        focus.set("main", false);
        assert!(!focus.front());
    }

    #[test]
    fn in_the_background_every_part_rings() {
        let fate = fate(&every_part(), Seen::Away).expect("it rings");
        assert!(fate.banner && !fate.notice);
        assert_eq!(fate.attention, Some(Attention::Until));
        assert_eq!(fate.sound.as_deref(), Some("chime"));
    }

    #[test]
    fn a_session_behind_plays_its_tone_and_the_page_shows_a_notice() {
        let fate = fate(&every_part(), Seen::Behind).expect("it rings");
        assert!(!fate.banner && fate.notice);
        assert_eq!(fate.attention, None);
        assert_eq!(fate.sound.as_deref(), Some("chime"));
    }

    #[test]
    fn the_session_you_look_at_rings_only_with_the_switch_off() {
        assert_eq!(fate(&every_part(), Seen::Looking), None);
        let always = AlertParts {
            background: false,
            ..every_part()
        };
        let fate = fate(&always, Seen::Looking).expect("it rings");
        assert!(fate.banner && !fate.notice);
        assert_eq!(fate.attention, None);
    }

    #[test]
    fn an_alert_with_no_part_on_never_rings() {
        let quiet = AlertParts {
            background: false,
            ..AlertParts::default()
        };
        for seen in [Seen::Looking, Seen::Behind, Seen::Away] {
            assert_eq!(fate(&quiet, seen), None);
        }
    }

    #[test]
    fn a_session_counts_as_in_front_only_while_vosh_is_and_it_is_selected() {
        // A selection shows the session's grid, which other tests read.
        let _grid = crate::native::grid::lock_shared_grid_for_test();
        let state = AppState::default();
        let one = state.selected_session();
        let two = state.open_session(one.profile());
        assert_eq!(seen(&state, one.id), Seen::Away);
        state.focus.set("help", true);
        assert_eq!(seen(&state, one.id), Seen::Looking);
        assert_eq!(seen(&state, two.id), Seen::Behind);
        assert_eq!(state.select_session(two.id), Ok(()));
        assert_eq!(seen(&state, one.id), Seen::Behind);
        assert_eq!(seen(&state, two.id), Seen::Looking);
        state.focus.set("help", false);
        assert_eq!(seen(&state, two.id), Seen::Away);
    }
}

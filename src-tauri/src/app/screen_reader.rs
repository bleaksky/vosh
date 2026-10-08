//! Whether a screen reader runs as Vosh starts. When one does and Read
//! new game lines is off in the profile in front, launch says once in
//! the terminal where that setting lives, see [`add_launch_notice`].
//!
//! macOS asks `NSWorkspace` whether `VoiceOver` is on. Windows asks for the
//! screen reader flag that Narrator, NVDA and JAWS set. Linux has no
//! signal that holds across desktops, since Orca sets a GNOME key only,
//! so there the check says no reader and launch prints nothing.

use crate::app::state::SharedState;

/// The line launch prints when a reader runs and the feed is off.
#[cfg(target_os = "macos")]
const NOTICE: &str =
    "VoiceOver is on. To hear the game, turn on Read new game lines in Settings under Accessibility.";
#[cfg(not(target_os = "macos"))]
const NOTICE: &str = "A screen reader is on. To hear the game, turn on Read new game lines in Settings under Accessibility.";

/// Add [`NOTICE`] to the launch notices when `running` says a screen
/// reader runs and the profile in front has Read new game lines off.
/// Launch passes [`running`], and tests pass a fake.
pub(crate) async fn add_launch_notice(state: &SharedState, running: impl FnOnce() -> bool) {
    let feed_on = state
        .selected_session()
        .lock_profile()
        .await
        .ui
        .screen_reader;
    if !feed_on && running() {
        state.add_launch_notices(vec![NOTICE.to_string()]);
    }
}

/// Whether `VoiceOver`, the screen reader macOS ships, is on.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
pub(crate) fn running() -> bool {
    use objc2::runtime::{AnyClass, AnyObject, Bool, Sel};
    let Some(class) = AnyClass::get("NSWorkspace") else {
        return false;
    };
    // SAFETY: sharedWorkspace and isVoiceOverEnabled are NSWorkspace
    // methods, the second since macOS 10.13, and the respondsToSelector:
    // check keeps an older system from an unrecognized selector.
    unsafe {
        let workspace: *mut AnyObject = objc2::msg_send![class, sharedWorkspace];
        if workspace.is_null() {
            return false;
        }
        let getter: Sel = objc2::sel!(isVoiceOverEnabled);
        let reachable: Bool = objc2::msg_send![workspace, respondsToSelector: getter];
        if !reachable.as_bool() {
            return false;
        }
        let on: Bool = objc2::msg_send![workspace, isVoiceOverEnabled];
        on.as_bool()
    }
}

/// Whether a screen reader has set the system flag for one.
#[cfg(windows)]
#[allow(unsafe_code)]
pub(crate) fn running() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETSCREENREADER};
    let mut on: i32 = 0;
    // SAFETY: SPI_GETSCREENREADER writes one BOOL through the pointer,
    // which points at `on`.
    let ok = unsafe { SystemParametersInfoW(SPI_GETSCREENREADER, 0, (&raw mut on).cast(), 0) };
    ok != 0 && on != 0
}

/// No signal holds across Linux desktops, so no reader counts as running.
#[cfg(not(any(target_os = "macos", windows)))]
pub(crate) fn running() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{add_launch_notice, NOTICE};
    use crate::app::state::{AppState, SharedState};

    async fn notices(feed_on: bool, reader: bool) -> Vec<String> {
        let state: SharedState = Arc::new(AppState::default());
        state
            .selected_session()
            .lock_profile()
            .await
            .ui
            .screen_reader = feed_on;
        add_launch_notice(&state, || reader).await;
        state.take_launch_notices()
    }

    #[tokio::test]
    async fn a_reader_with_the_feed_off_adds_one_notice() {
        assert_eq!(notices(false, true).await, vec![NOTICE.to_string()]);
    }

    #[tokio::test]
    async fn the_feed_on_or_no_reader_adds_none() {
        assert_eq!(notices(true, true).await, Vec::<String>::new());
        assert_eq!(notices(false, false).await, Vec::<String>::new());
        assert_eq!(notices(true, false).await, Vec::<String>::new());
    }

    /// The real check answers without a crash on the system that runs
    /// the tests, whatever it answers.
    #[test]
    fn the_system_check_answers() {
        let _ = super::running();
    }

    #[test]
    fn the_notice_names_the_setting_and_where_it_lives() {
        assert!(NOTICE.ends_with(
            "is on. To hear the game, turn on Read new game lines in Settings under Accessibility."
        ));
        #[cfg(target_os = "macos")]
        assert!(NOTICE.starts_with("VoiceOver is on."));
        #[cfg(not(target_os = "macos"))]
        assert!(NOTICE.starts_with("A screen reader is on."));
    }
}

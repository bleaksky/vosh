//! The commands for the terminal pane on either renderer. The page writes
//! its own text through them, reads back the native grid's cursor and
//! screen, loads the saved scrollback as a pane mounts, and reports the
//! ground your highlight colors must read on.

use tauri::State;
use vosh_automation::trigger::readable;

use crate::app::state::SharedState;

/// Write text the webview drew itself, such as your typed echo or an
/// error notice. The native grid takes it too, as it takes every session
/// write, so it keeps the same content as xterm whichever renderer shows,
/// and the session closes the open row, since that text now follows it.
/// The webview calls it for every such write, on either renderer.
///
/// `after` is the newest output of the prompt stage xterm took before the
/// text, while xterm shows. While the native grid shows it is null, and
/// the grid names its own as it takes the text. The session can hear of
/// the text after it sent later output, since your echo and your line
/// reach it by two calls, and that output stays open.
#[tauri::command]
pub(crate) async fn terminal_local_write(
    state: State<'_, SharedState>,
    text: String,
    after: Option<u64>,
) -> Result<(), String> {
    #[cfg(native_surface)]
    let taken = {
        let taken = crate::native::grid::feed_local(text.as_bytes());
        crate::native::surface::request_redraw();
        taken
    };
    // With no grid, text whose renderer named nothing lands after
    // everything.
    #[cfg(not(native_surface))]
    let taken = {
        let _ = &text;
        u64::MAX
    };
    let session = state.selected_session();
    if let Some(handle) = session.slot.lock().await.as_ref() {
        let _ = handle.local_write(after.unwrap_or(taken));
    }
    Ok(())
}

/// You started or stopped selecting text or reading back in xterm. While
/// you do, a clock piece in your design does not repaint your prompt in
/// the text, so the row under your selection or above your reading never
/// moves. The native grid holds its own selection and scroll,
/// which the session reads itself.
#[tauri::command]
pub(crate) fn terminal_reader_busy(state: State<'_, SharedState>, busy: bool) {
    state
        .selected_session()
        .reader_busy
        .store(busy, std::sync::atomic::Ordering::Release);
}

/// Where the native grid's cursor sits and where the open region starts,
/// so the webview can map a pointer to a piece of your prompt while the
/// native renderer draws the terminal. Lines count from the
/// top of the live screen. Null before the grid exists. xterm reads its
/// own buffer and marker instead.
#[cfg(native_surface)]
#[tauri::command]
pub(crate) fn terminal_cursor() -> Option<crate::native::grid::regions::CursorReport> {
    crate::native::grid::cursor_report()
}

/// No native grid on this build, so there is nothing to report.
#[cfg(not(native_surface))]
#[tauri::command]
pub(crate) fn terminal_cursor() -> Option<()> {
    None
}

/// The native grid's live screen as text, row by row, so the prompt card
/// can find the line the game sent while the profile reads no prompt and
/// no row is open. Null before the grid exists. xterm reads its own
/// buffer instead.
#[cfg(native_surface)]
#[tauri::command]
pub(crate) fn terminal_screen_rows() -> Option<crate::native::grid::regions::ScreenRows> {
    crate::native::grid::screen_rows()
}

/// No native grid on this build, so there is nothing to read.
#[cfg(not(native_surface))]
#[tauri::command]
pub(crate) fn terminal_screen_rows() -> Option<()> {
    None
}

#[tauri::command]
pub(crate) async fn scrollback_load(
    state: State<'_, SharedState>,
    feed_native: bool,
) -> Result<ScrollbackLoad, String> {
    let session = state.selected_session();
    let sb = session.scrollback.lock().await;
    // With the run of repeated lines the screen ends on marked, so a pane
    // that loads it during the run rewrites the count in place.
    let bytes = sb.dump_live();
    // The native grid is fed only live output, so the persisted scrollback
    // would be missing there. The live pane asks us to seed it, and only
    // the first ask per process lands. A reloaded page asks again while
    // the grid still holds everything. The seed is claimed even when the
    // scrollback is empty, since the grid then gets every line live.
    #[cfg(native_surface)]
    let seeded_native = feed_native && crate::native::grid::claim_seed() && !bytes.is_empty();
    #[cfg(native_surface)]
    if seeded_native {
        crate::native::grid::feed_local(&bytes);
        crate::native::surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    let seeded_native = {
        let _ = feed_native;
        false
    };
    Ok(ScrollbackLoad {
        bytes,
        seeded_native,
    })
}

/// Clear scrollback in the terminal menu. Vosh forgets the lines it keeps
/// for the next launch, and on the native renderer the grid drops its
/// history too. The xterm renderer clears its own buffer. The session log
/// keeps every line.
#[tauri::command]
pub(crate) async fn scrollback_clear(state: State<'_, SharedState>) -> Result<(), String> {
    let session = state.selected_session();
    session.scrollback.lock().await.clear();
    #[cfg(native_surface)]
    {
        crate::native::grid::clear_history();
        crate::native::surface::request_redraw();
    }
    Ok(())
}

/// The persisted scrollback for a mounting terminal, and whether this call
/// also wrote it into the native grid. The page mirrors its restored banner
/// into the grid only when the seed landed here, so a reloaded page, whose
/// grid already holds the history and the first banner, adds no second one.
#[derive(Debug, serde::Serialize)]
pub(crate) struct ScrollbackLoad {
    pub bytes: Vec<u8>,
    pub seeded_native: bool,
}

/// Keep highlight colors readable. `background` is the theme's terminal
/// background as `#rrggbb` while the setting is on, and `None` while it is
/// off. A background that does not read turns lifting off too.
#[tauri::command]
pub(crate) fn highlight_ground_set(background: Option<String>) {
    crate::session::highlight_ground::set(background.as_deref().and_then(readable::parse_hex));
}

#[cfg(test)]
mod tests {
    use super::ScrollbackLoad;

    #[test]
    fn scrollback_load_names_the_fields_the_page_reads() {
        let load = ScrollbackLoad {
            bytes: vec![104, 105],
            seeded_native: true,
        };
        assert_eq!(
            serde_json::to_value(&load).unwrap(),
            serde_json::json!({ "bytes": [104, 105], "seeded_native": true })
        );
    }
}

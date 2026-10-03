//! The commands for the app's windows. Opening Settings and Help,
//! quitting, the notices launch leaves for the main window, the theme
//! ground a new window opens on, a window's answer when a quit asks for
//! the writes it holds, and the macOS menu bar's state and Copy.

use tauri::{AppHandle, Manager, State};

use crate::app::exit::ANSWERS;
use crate::app::state::SharedState;
use crate::app::windows::{open_aux_window, record, DRESSED_WINDOWS, HELP_WINDOW, SETTINGS_WINDOW};
use crate::app_menu::MenuState;

/// What launch has to tell you, for the main window to show once in the
/// terminal and as a toast.
#[tauri::command]
pub(crate) fn launch_notices_take(state: State<'_, SharedState>) -> Vec<String> {
    state.take_launch_notices()
}

/// Open (or focus, if already open) the standalone settings window,
/// where the React entry renders `SettingsApp`.
#[tauri::command]
pub(crate) async fn open_settings_window(app: AppHandle) -> Result<(), String> {
    open_aux_window(&app, &SETTINGS_WINDOW)
}

/// Open (or focus, if already open) the Help window, where the React
/// entry renders `HelpApp`. The page that asked leaves the topic or the
/// search it should land on (src/lib/helpLink.ts).
#[tauri::command]
pub(crate) async fn open_help_window(app: AppHandle) -> Result<(), String> {
    open_aux_window(&app, &HELP_WINDOW)
}

/// Cleanly exit the app. Surfaces a "quit" event first so any window
/// can flush state, then calls `app.exit(0)`. Used by the post-
/// migration prompt to take the user out of the legacy-mode session
/// in one click; on relaunch the Path B startup hook picks up the
/// new catalog.
#[tauri::command]
pub(crate) async fn app_quit(app: AppHandle) -> Result<(), String> {
    // No explicit persist here: `app.exit` raises `RunEvent::ExitRequested`,
    // whose handler asks the windows for their pending writes and then
    // flushes the profile exactly once (with a timeout), see app/exit.rs.
    app.exit(0);
    Ok(())
}

/// A theme paint in a window reports the ground and appearance a new
/// window should open on. An open Settings or Help window takes the
/// ground now, so a theme change while it is open leaves no old color
/// under it.
#[tauri::command]
pub(crate) fn window_backdrop_set(
    app: AppHandle,
    background: Option<String>,
    appearance: Option<String>,
) -> Result<(), String> {
    let backdrop = record(background.as_deref(), appearance.as_deref())?;
    for label in DRESSED_WINDOWS {
        if let Some(window) = app.get_webview_window(label) {
            backdrop.redress(&window.as_ref().window());
        }
    }
    Ok(())
}

/// A window answers the quit request once it has sent what it held.
#[tauri::command]
pub(crate) fn pending_writes_flushed<R: tauri::Runtime>(window: tauri::WebviewWindow<R>) {
    ANSWERS.answer(window.label());
}

/// Mirror the page's state in the menu. Sync, so it runs on the main
/// thread and the menu setters run inline.
#[tauri::command]
pub(crate) fn menu_set_state(app: AppHandle, state: MenuState) {
    #[cfg(target_os = "macos")]
    crate::app_menu::apply_state(&app, &state);
    #[cfg(not(target_os = "macos"))]
    let _ = (app, state);
}

/// Edit, then Copy, from the main window. `terminal` is true when the
/// page holds no text selection of its own, and then a native terminal
/// selection wins. Anything else copies the way the system would, from
/// the focused field or the page selection.
#[tauri::command]
pub(crate) fn menu_copy(app: AppHandle, terminal: bool) {
    #[cfg(target_os = "macos")]
    {
        if terminal && crate::term_grid::selection_text().is_some_and(|t| !t.is_empty()) {
            crate::native_surface::request_copy();
        } else {
            let _ = app.run_on_main_thread(crate::app_menu::system_copy);
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, terminal);
}

use std::sync::Arc;

use tracing_subscriber::EnvFilter;

mod affects;
mod alert;
mod app;
mod color;
mod disk;
mod import;
mod input;
mod ipc;
mod loadouts;
mod logs;
// Off macOS the native grid builds only for the tests, which read it, so
// what only the renderer calls goes unused there.
#[cfg(any(native_surface, test))]
#[cfg_attr(not(native_surface), allow(dead_code))]
mod native;
mod output;
mod profile;
mod prompt;
mod script;
mod session;
mod sessions;
#[cfg(test)]
mod tests;
mod tick;
mod writing;

use app::state::{AppState, SharedState};
use app::system_fonts::handle_font_uri;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let state: SharedState = Arc::new(AppState::default());

    let builder = tauri::Builder::default();
    // The macOS menu bar (app/menu.rs). Windows and Linux get no menu, so
    // their frameless windows never grow a native menubar.
    #[cfg(target_os = "macos")]
    let builder = builder
        .menu(app::menu::build)
        .on_menu_event(app::menu::on_event);
    // A click on a toast of an installed Windows Vosh starts it again.
    // The guard hands that start to the Vosh that runs, which selects the
    // session of the newest banner. It goes first, before the other
    // plugins.
    #[cfg(windows)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        app::windows::second_start(app);
    }));
    // Alert banners on Windows and Linux. macOS posts through a module of
    // Vosh's own, see alert/mac.rs.
    #[cfg(not(target_os = "macos"))]
    let builder = builder.plugin(tauri_plugin_notification::init());

    builder
        // Serves the regular face of a system font family. fontLoader.ts
        // mints @font-face blocks whose URL carries the family in its
        // path (font://localhost/<family>, or http://font.localhost/
        // <family> on Windows) so the webview can render user-installed
        // fonts WebKit otherwise refuses to match. The lookup and the
        // file read run on the blocking pool, never on the main thread.
        .register_asynchronous_uri_scheme_protocol("font", |_ctx, request, responder| {
            let uri = request.uri().clone();
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(handle_font_uri(&uri));
            });
        })
        .on_window_event(app::windows::on_window_event)
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                // Persist only geometry. DECORATIONS would override the
                // frameless setting in tauri.conf on every restart, and
                // VISIBLE conflicts with our deliberate "open hidden, show
                // after first paint" reveal in shell/useUiConfigFollow.ts.
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED
                        | tauri_plugin_window_state::StateFlags::FULLSCREEN,
                )
                .build(),
        )
        .manage(state.clone())
        .setup(move |app| {
            app::launch::setup(app, &state);
            Ok(())
        })
        .invoke_handler(ipc::handler())
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(app::exit::on_run_event);
}

use std::sync::Arc;

use tracing_subscriber::EnvFilter;

mod affect_full;
mod affects_snapshot;
mod app;
mod app_menu;
#[cfg(native_surface)]
mod cell_render;
mod disk;
mod fonts;
mod import;
mod input;
mod ipc;
mod loadouts;
mod logs;
#[cfg(native_surface)]
mod native_surface;
mod output;
mod preset_rollout;
mod profile;
mod prompt;
mod prompt_migration;
mod script;
mod session;
#[cfg(native_surface)]
mod term_grid;
#[cfg(test)]
mod tests;
mod tick;
mod tintin_import;

use app::state::{AppState, SharedState};
use fonts::handle_font_uri;

// Callers still reach the loadout code by the paths of loadout.rs and
// loadout_store.rs, until they point at loadouts/.
mod loadout {
    pub(crate) use crate::loadouts::catalog::GlobalCatalog;
    pub(crate) use crate::loadouts::set::{Loadout, LoadoutSet};
}
mod loadout_store {
    #[cfg(test)]
    pub(crate) use crate::loadouts::catalog::UNREAD_CATALOG_NOTICE;
    pub(crate) use crate::loadouts::catalog::{
        catalog_path, load_global_catalog, path_b_mode_active, save_global_catalog,
    };
    pub(crate) use crate::loadouts::gating::apply_effective_state;
    pub(crate) use crate::loadouts::presets::{
        adopt_catalog_presets, first_catalog_presets, profile_preset_lists, PRESETS_OFF,
    };
    #[cfg(test)]
    pub(crate) use crate::loadouts::set::{load_loadout_set, UNREAD_LOADOUTS_NOTICE};
    pub(crate) use crate::loadouts::set::{loadouts_path, save_loadout_set};
    #[cfg(test)]
    pub(crate) use crate::loadouts::wizard::apply::legacy_dir;
    pub(crate) use crate::loadouts::{load_path_b_at_launch, LoadoutStoreError};
}

// Callers still reach the wizard planner by the path of migration.rs,
// until they point at loadouts/wizard/.
use loadouts::wizard::plan as migration;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let state: SharedState = Arc::new(AppState::default());

    let builder = tauri::Builder::default();
    // The macOS menu bar (app_menu.rs). Windows and Linux get no menu, so
    // their frameless windows never grow a native menubar.
    #[cfg(target_os = "macos")]
    let builder = builder
        .menu(app_menu::build)
        .on_menu_event(app_menu::on_event);

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
                // after first paint" reveal in App.tsx.
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

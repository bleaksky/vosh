use std::sync::Arc;

use tracing_subscriber::EnvFilter;

mod affect_full;
mod affects_snapshot;
mod app;
mod app_menu;
#[cfg(native_surface)]
mod cell_render;
mod characters;
#[cfg(test)]
mod config_golden_tests;
mod connection;
mod disk;
#[cfg(test)]
mod fake_mud_tests;
mod fonts;
mod forget_passwords;
mod gmcp_bind;
mod hidden_input;
mod highlight_ground;
mod import;
mod input;
mod ipc;
#[cfg(test)]
mod ipc_contract_tests;
#[cfg(all(test, native_surface))]
mod latency_tests;
mod line_accumulator;
mod loadout;
mod loadout_store;
mod loadouts;
mod log_state;
mod migration;
#[cfg(native_surface)]
mod native_surface;
mod preset_rollout;
mod profile;
mod profile_config;
mod profile_set;
mod prompt;
mod prompt_migration;
mod room_block;
mod script_state;
mod session;
#[cfg(native_surface)]
mod term_grid;
#[cfg(test)]
mod tests;
#[cfg(all(test, native_surface))]
mod throughput_tests;
mod tick;
mod tintin_import;
#[cfg(test)]
mod upgrade_order_tests;

use app::state::{AppState, SharedState};
use fonts::handle_font_uri;

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
        .invoke_handler(tauri::generate_handler![
            ipc::native_surface::native_surface_set_bounds,
            ipc::native_surface::native_surface_scroll,
            ipc::native_surface::native_surface_copy,
            ipc::native_surface::native_surface_select_all,
            ipc::native_surface::native_surface_set_theme,
            ipc::native_surface::native_surface_set_font,
            ipc::terminal::terminal_local_write,
            ipc::terminal::terminal_cursor,
            ipc::terminal::terminal_reader_busy,
            ipc::terminal::terminal_screen_rows,
            ipc::native_surface::native_surface_find,
            ipc::native_surface::native_surface_find_clear,
            ipc::native_surface::native_surface_set_visible,
            ipc::native_surface::native_surface_pointer,
            ipc::native_surface::native_surface_ready,
            ipc::native_surface::native_surface_wheel,
            ipc::native_surface::native_surface_set_cell_metrics,
            ipc::native_surface::native_surface_set_bright_bold,
            ipc::native_surface::native_surface_set_divider_color,
            ipc::native_surface::native_surface_set_tokens,
            ipc::native_surface::native_surface_set_prompt_bands,
            ipc::native_surface::native_surface_set_prompt_reach,
            ipc::native_surface::native_surface_set_blink_text,
            ipc::session::session_connect,
            ipc::session::session_send_input,
            ipc::session::session_send_masked,
            ipc::session::session_set_window_size,
            ipc::session::session_disconnect,
            ipc::automation::triggers_list,
            ipc::session::target_get,
            ipc::automation::triggers_export,
            ipc::automation::triggers_import,
            ipc::automation::aliases_export,
            ipc::automation::aliases_import,
            ipc::automation::presets_install,
            ipc::automation::presets_remove,
            ipc::logs::logs_list_sessions,
            ipc::logs::logs_search_page,
            ipc::logs::logs_export,
            ipc::terminal::scrollback_load,
            ipc::ui_config::ui_get_config,
            ipc::ui_config::ui_set_config,
            ipc::updater::updater_check,
            ipc::updater::updater_install_and_relaunch,
            ipc::profiles::profiles_list,
            ipc::profiles::profile_create,
            ipc::profiles::profile_delete,
            ipc::profiles::profile_rename,
            ipc::profiles::profile_duplicate,
            ipc::profiles::profile_switch,
            ipc::profiles::profile_resolve_match,
            ipc::wizard::migration_analyze,
            ipc::wizard::migration_apply,
            ipc::windows::app_quit,
            ipc::windows::launch_notices_take,
            ipc::windows::pending_writes_flushed,
            ipc::loadouts::loadouts_get_state,
            ipc::loadouts::loadouts_set_active,
            ipc::tick::tick_get_config,
            ipc::tick::tick_set_config,
            ipc::profiles::profile_get_scope,
            ipc::profiles::profile_set_scope,
            ipc::windows::open_settings_window,
            ipc::windows::open_help_window,
            ipc::windows::window_backdrop_set,
            ipc::terminal::highlight_ground_set,
            ipc::panes::pane_layout_get,
            ipc::panes::pane_layout_set,
            ipc::affects::tracked_affects_set,
            ipc::characters::profile_detail_get,
            ipc::panes::pane_layout_reset,
            ipc::characters::profile_set_login,
            ipc::characters::profile_set_world,
            ipc::characters::session_identity_get,
            ipc::affects::affects_snapshot_get,
            ipc::affects::affect_full_get,
            ipc::prompt::hidden_get,
            ipc::prompt::prompt_show_get,
            ipc::prompt::prompt_last_seen,
            ipc::prompt::prompt_config_get,
            ipc::prompt::prompt_config_set,
            ipc::prompt::prompt_card_open,
            ipc::prompt::prompt_code_reader_set,
            ipc::prompt::prompt_designs_list,
            ipc::prompt::prompt_compile,
            ipc::prompt::prompt_candidates,
            ipc::prompt::prompt_capture_check,
            ipc::prompt::prompt_capture_from_line,
            ipc::prompt::prompt_render,
            ipc::prompt::prompt_render_many,
            ipc::prompt::prompt_preview_set,
            ipc::prompt::prompt_edit,
            ipc::prompt::prompt_describe,
            ipc::prompt::prompt_forms,
            ipc::prompt::prompt_line_triggers,
            ipc::prompt::prompt_state_get,
            ipc::prompt::prompt_watch,
            ipc::prompt::prompt_gags_without_reader,
            ipc::characters::profile_export_file,
            ipc::ui_config::ui_set_theme,
            ipc::affects::ui_set_affects_display,
            ipc::ui_config::ui_get_chat_colors,
            ipc::ui_config::ui_set_chat_color,
            ipc::ui_config::ui_reset_chat_colors,
            ipc::ui_config::fonts_list,
            ipc::automation::macros_list,
            ipc::automation::macros_set,
            ipc::automation::macros_delete,
            ipc::automation::macros_groups_list,
            ipc::automation::timers_list,
            ipc::automation::timers_set,
            ipc::automation::timers_delete,
            ipc::automation::import_detect,
            ipc::automation::import_apply,
            ipc::windows::menu_set_state,
            ipc::windows::menu_copy,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(app::exit::on_run_event);
}

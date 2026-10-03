use std::sync::Arc;

use tracing_subscriber::EnvFilter;

mod affect_full;
mod affects_snapshot;
mod app;
mod app_menu;
#[cfg(native_surface)]
mod cell_render;
mod characters;
mod commands;
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
use commands::{
    app_quit, loadouts_get_state, loadouts_set_active, logs_export, logs_list_sessions,
    logs_search_page, migration_analyze, migration_apply, open_help_window, open_settings_window,
    profile_create, profile_delete, profile_duplicate, profile_get_scope, profile_rename,
    profile_resolve_match, profile_set_scope, profile_switch, profiles_list, tick_get_config,
    tick_set_config, updater_check, updater_install_and_relaunch,
};
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
            logs_list_sessions,
            logs_search_page,
            logs_export,
            ipc::terminal::scrollback_load,
            ipc::ui_config::ui_get_config,
            ipc::ui_config::ui_set_config,
            updater_check,
            updater_install_and_relaunch,
            profiles_list,
            profile_create,
            profile_delete,
            profile_rename,
            profile_duplicate,
            profile_switch,
            profile_resolve_match,
            migration_analyze,
            migration_apply,
            app_quit,
            commands::launch_notices_take,
            app::exit::pending_writes_flushed,
            loadouts_get_state,
            loadouts_set_active,
            tick_get_config,
            tick_set_config,
            profile_get_scope,
            profile_set_scope,
            open_settings_window,
            open_help_window,
            app::windows::window_backdrop_set,
            ipc::terminal::highlight_ground_set,
            ipc::panes::pane_layout_get,
            ipc::panes::pane_layout_set,
            ipc::affects::tracked_affects_set,
            characters::profile_detail_get,
            ipc::panes::pane_layout_reset,
            characters::profile_set_login,
            characters::profile_set_world,
            characters::session_identity_get,
            ipc::affects::affects_snapshot_get,
            ipc::affects::affect_full_get,
            commands::hidden_get,
            commands::prompt_show_get,
            prompt::last_seen::prompt_last_seen,
            prompt::prompt_config_get,
            prompt::prompt_config_set,
            prompt::prompt_card_open,
            prompt::prompt_code_reader_set,
            prompt::prompt_designs_list,
            prompt::prompt_compile,
            prompt::prompt_candidates,
            prompt::prompt_capture_check,
            prompt::prompt_capture_from_line,
            prompt::prompt_render,
            prompt::prompt_render_many,
            prompt::prompt_preview_set,
            prompt::prompt_edit,
            prompt::prompt_describe,
            prompt::prompt_forms,
            prompt::prompt_line_triggers,
            prompt::prompt_state_get,
            prompt::prompt_watch,
            commands::prompt_gags_without_reader,
            characters::profile_export_file,
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
            app_menu::menu_set_state,
            app_menu::menu_copy,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(app::exit::on_run_event);
}

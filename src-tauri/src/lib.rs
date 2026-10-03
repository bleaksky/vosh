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
    aliases_export, aliases_import, app_quit, import_apply, import_detect, loadouts_get_state,
    loadouts_set_active, logs_export, logs_list_sessions, logs_search_page, macros_delete,
    macros_groups_list, macros_list, macros_set, migration_analyze, migration_apply,
    native_surface_copy, native_surface_find, native_surface_find_clear, native_surface_pointer,
    native_surface_ready, native_surface_scroll, native_surface_set_bounds,
    native_surface_set_bright_bold, native_surface_set_cell_metrics,
    native_surface_set_divider_color, native_surface_set_font, native_surface_set_theme,
    native_surface_set_visible, native_surface_wheel, open_help_window, open_settings_window,
    presets_install, presets_remove, profile_create, profile_delete, profile_duplicate,
    profile_get_scope, profile_rename, profile_resolve_match, profile_set_scope, profile_switch,
    profiles_list, scrollback_load, session_connect, session_disconnect, session_send_input,
    session_send_masked, session_set_window_size, target_get, tick_get_config, tick_set_config,
    timers_delete, timers_list, timers_set, triggers_export, triggers_import, triggers_list,
    ui_get_config, ui_set_config, updater_check, updater_install_and_relaunch,
};
use fonts::{fonts_list, handle_font_uri};

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
            native_surface_set_bounds,
            native_surface_scroll,
            native_surface_copy,
            commands::native_surface_select_all,
            native_surface_set_theme,
            native_surface_set_font,
            commands::terminal_local_write,
            commands::terminal_cursor,
            commands::terminal_reader_busy,
            commands::terminal_screen_rows,
            native_surface_find,
            native_surface_find_clear,
            native_surface_set_visible,
            native_surface_pointer,
            native_surface_ready,
            native_surface_wheel,
            native_surface_set_cell_metrics,
            native_surface_set_bright_bold,
            native_surface_set_divider_color,
            commands::native_surface_set_tokens,
            commands::native_surface_set_prompt_bands,
            commands::native_surface_set_prompt_reach,
            commands::native_surface_set_blink_text,
            session_connect,
            session_send_input,
            session_send_masked,
            session_set_window_size,
            session_disconnect,
            triggers_list,
            target_get,
            triggers_export,
            triggers_import,
            aliases_export,
            aliases_import,
            presets_install,
            presets_remove,
            logs_list_sessions,
            logs_search_page,
            logs_export,
            scrollback_load,
            ui_get_config,
            ui_set_config,
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
            highlight_ground::highlight_ground_set,
            commands::pane_layout_get,
            commands::pane_layout_set,
            commands::tracked_affects_set,
            characters::profile_detail_get,
            characters::pane_layout_reset,
            characters::profile_set_login,
            characters::profile_set_world,
            characters::session_identity_get,
            affects_snapshot::affects_snapshot_get,
            affect_full::affect_full_get,
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
            commands::ui_set_theme,
            commands::ui_set_affects_display,
            commands::ui_get_chat_colors,
            commands::ui_set_chat_color,
            commands::ui_reset_chat_colors,
            fonts_list,
            macros_list,
            macros_set,
            macros_delete,
            macros_groups_list,
            timers_list,
            timers_set,
            timers_delete,
            import_detect,
            import_apply,
            app_menu::menu_set_state,
            app_menu::menu_copy,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(app::exit::on_run_event);
}

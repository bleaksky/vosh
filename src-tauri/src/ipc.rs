//! Every command the page calls, as thin wrappers by topic, and the one
//! list in [`handler`] that registers them.

pub(crate) mod affects;
pub(crate) mod alerts;
pub(crate) mod automation;
pub(crate) mod characters;
pub(crate) mod loadouts;
pub(crate) mod logs;
pub(crate) mod native_surface;
pub(crate) mod panes;
pub(crate) mod profiles;
pub(crate) mod prompt;
pub(crate) mod scripts;
pub(crate) mod session;
pub(crate) mod snoop;
pub(crate) mod terminal;
pub(crate) mod tick;
pub(crate) mod ui_config;
pub(crate) mod updater;
pub(crate) mod vitals;
pub(crate) mod windows;
pub(crate) mod wizard;

/// Your Downloads folder, where Export to Downloads saves a profile or a
/// plugin, or the sentence a command returns when the system names none.
pub(crate) fn downloads_dir<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<std::path::PathBuf, String> {
    use tauri::Manager;
    app.path()
        .download_dir()
        .map_err(|_| "Vosh could not find your Downloads folder.".to_string())
}

/// Every command the page can invoke, routed by its function name. The
/// IPC contract test reads this list, so a command the page calls and
/// this list leaves out fails a test, not the app.
pub(crate) fn handler() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        native_surface::native_surface_set_bounds,
        native_surface::native_surface_scroll,
        native_surface::native_surface_copy,
        native_surface::native_surface_select_all,
        native_surface::native_surface_set_theme,
        native_surface::native_surface_set_font,
        terminal::terminal_local_write,
        terminal::terminal_cursor,
        terminal::terminal_reader_busy,
        terminal::terminal_screen_rows,
        native_surface::native_surface_find,
        native_surface::native_surface_find_clear,
        native_surface::native_surface_pointer,
        native_surface::native_surface_ready,
        native_surface::native_surface_wheel,
        native_surface::native_surface_set_cell_metrics,
        native_surface::native_surface_set_bright_bold,
        native_surface::native_surface_set_divider_color,
        native_surface::native_surface_set_tokens,
        native_surface::native_surface_set_prompt_bands,
        native_surface::native_surface_set_prompt_reach,
        native_surface::native_surface_set_blink_text,
        session::session_open,
        session::session_select,
        session::session_close,
        session::session_rename,
        session::session_move,
        session::session_set_address,
        session::sessions_list,
        session::session_connect,
        session::session_send_input,
        session::session_send_masked,
        session::session_walk_stop,
        session::session_set_window_size,
        session::session_disconnect,
        session::session_reconnect_now,
        session::session_reconnect_cancel,
        session::reconnect_get,
        session::reconnect_set,
        automation::triggers_list,
        session::target_get,
        automation::triggers_export,
        automation::triggers_import,
        automation::aliases_export,
        automation::aliases_import,
        automation::presets_install,
        automation::presets_remove,
        logs::logs_list_sessions,
        logs::logs_search_page,
        logs::logs_export,
        terminal::scrollback_load,
        terminal::scrollback_clear,
        ui_config::ui_get_config,
        ui_config::ui_set_fields,
        updater::updater_check,
        updater::updater_install_and_relaunch,
        profiles::profiles_list,
        profiles::profile_create,
        profiles::profile_delete,
        profiles::profile_rename,
        profiles::profile_duplicate,
        profiles::profile_switch,
        profiles::profile_hold_edits,
        profiles::profile_resolve_match,
        wizard::migration_analyze,
        wizard::migration_apply,
        windows::app_quit,
        windows::launch_notices_take,
        windows::pending_writes_flushed,
        loadouts::loadouts_get_state,
        loadouts::loadouts_set_active,
        tick::tick_get_config,
        tick::tick_set_config,
        tick::daylight_get,
        alerts::alert_presets_get,
        alerts::alert_presets_set,
        alerts::alerts_permission,
        alerts::alerts_ask_permission,
        alerts::alerts_open_settings,
        profiles::profile_get_scope,
        profiles::profile_set_scope,
        windows::open_settings_window,
        windows::open_help_window,
        windows::window_backdrop_set,
        terminal::highlight_ground_set,
        panes::pane_layout_get,
        panes::pane_layout_set,
        affects::tracked_affects_set,
        characters::profile_detail_get,
        panes::pane_layout_reset,
        panes::lua_panes_get,
        snoop::snoop_get,
        characters::profile_set_login,
        characters::profile_set_world,
        characters::session_identity_get,
        affects::affects_snapshot_get,
        affects::affect_full_get,
        vitals::vitals_snapshot_get,
        vitals::vitals_text_watch,
        prompt::hidden_get,
        prompt::prompt_show_get,
        prompt::prompt_last_seen,
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
        prompt::prompt_gags_without_reader,
        characters::profile_export_file,
        characters::profile_import_read,
        characters::profile_import_apply,
        ui_config::ui_set_theme,
        affects::ui_set_affects_display,
        ui_config::ui_get_chat_colors,
        ui_config::ui_set_chat_color,
        ui_config::ui_reset_chat_colors,
        ui_config::fonts_list,
        automation::macros_list,
        automation::macros_set,
        automation::macros_delete,
        automation::macros_groups_list,
        automation::groups_list,
        automation::groups_set_enabled,
        automation::timers_list,
        automation::timers_set,
        automation::timers_delete,
        automation::import_detect,
        automation::import_apply,
        windows::menu_set_state,
        windows::menu_copy,
        scripts::lua_output_get,
        scripts::lua_output_clear,
        scripts::lua_run,
        scripts::plugins_list,
        scripts::plugin_read,
        scripts::plugin_create,
        scripts::plugin_save,
        scripts::plugin_set_enabled,
        scripts::plugin_reload,
        scripts::plugin_reveal,
        scripts::plugin_install_check,
        scripts::plugin_install,
        scripts::plugin_export,
        scripts::plugin_remove,
    ]
}

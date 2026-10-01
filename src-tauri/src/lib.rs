use std::sync::Arc;

use tauri::Manager;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use vosh_log::LogStore;
use vosh_map::MapStore;

// macOS-only: WKWebView ignores the HTML `spellcheck` attribute
// until continuous spell-checking is enabled at the NSView level.
// The context-menu "Check Spelling While Typing" item works, which
// means the action `toggleContinuousSpellChecking:` is dispatchable
// through the responder chain. We mirror that path: query
// isContinuousSpellCheckingEnabled first, then send the toggle
// action only if it is off, so we never flip it back off. All
// sends are gated with respondsToSelector: — earlier unguarded
// sends of NSTextView-only selectors crashed the app at launch.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn enable_macos_spellcheck(window: &tauri::WebviewWindow) -> Result<(), tauri::Error> {
    use objc2::runtime::{AnyObject, Bool, Sel};
    window.with_webview(|webview| {
        let raw = webview.inner().cast::<AnyObject>();
        if raw.is_null() {
            tracing::warn!("macos spellcheck: webview.inner() was null");
            return;
        }
        unsafe {
            let setter: Sel = objc2::sel!(setContinuousSpellCheckingEnabled:);
            let getter: Sel = objc2::sel!(isContinuousSpellCheckingEnabled);
            let toggler: Sel = objc2::sel!(toggleContinuousSpellChecking:);
            let r_set: Bool = objc2::msg_send![raw, respondsToSelector: setter];
            let r_get: Bool = objc2::msg_send![raw, respondsToSelector: getter];
            let r_tog: Bool = objc2::msg_send![raw, respondsToSelector: toggler];
            tracing::info!(
                set = r_set.as_bool(),
                get = r_get.as_bool(),
                toggle = r_tog.as_bool(),
                "macos spellcheck: selectors reachable on WKWebView"
            );
            if r_set.as_bool() {
                let _: () = objc2::msg_send![raw, setContinuousSpellCheckingEnabled: true];
                tracing::info!("macos spellcheck: setContinuousSpellCheckingEnabled:YES sent");
                return;
            }
            if r_tog.as_bool() {
                let enabled: Bool = if r_get.as_bool() {
                    objc2::msg_send![raw, isContinuousSpellCheckingEnabled]
                } else {
                    Bool::NO
                };
                if enabled.as_bool() {
                    tracing::info!("macos spellcheck: already enabled, no toggle needed");
                } else {
                    let _: () = objc2::msg_send![raw, toggleContinuousSpellChecking: raw];
                    tracing::info!("macos spellcheck: toggleContinuousSpellChecking: sent");
                }
            } else {
                tracing::warn!("macos spellcheck: no reachable setter or toggle on WKWebView");
            }
        }
    })
}

mod affect_full;
mod affects_snapshot;
mod app_menu;
#[cfg(native_surface)]
mod cell_render;
mod characters;
mod commands;
mod connection;
mod exit_flush;
#[cfg(test)]
mod fake_mud_tests;
mod fonts;
mod forget_passwords;
mod gmcp_bind;
mod hidden_input;
mod import;
mod input;
#[cfg(test)]
mod ipc_contract_tests;
#[cfg(all(test, native_surface))]
mod latency_tests;
mod launch;
mod line_accumulator;
mod list_events;
mod loadout;
mod loadout_store;
mod log_state;
mod map_state;
mod migration;
#[cfg(native_surface)]
mod native_surface;
mod plugins;
mod preset_rollout;
mod profile;
mod profile_config;
mod profile_set;
mod prompt_commands;
mod prompt_lookup;
mod prompt_migration;
mod script_state;
mod session;
#[cfg(native_surface)]
mod term_grid;
mod tick;
mod tintin_import;
mod window_backdrop;

use commands::{
    aliases_export, aliases_groups_list, aliases_import, aliases_set_group_enabled, app_quit,
    app_version, dock_layout_get, dock_layout_set, import_apply, import_detect, loadouts_get_state,
    loadouts_set_active, logs_export, logs_list_sessions, logs_search, logs_search_page,
    macros_delete, macros_groups_list, macros_list, macros_set, macros_set_group_enabled,
    map_set_avoid, map_set_note, map_walk_to, migration_analyze, migration_apply,
    native_surface_copy, native_surface_find, native_surface_find_clear, native_surface_pointer,
    native_surface_ready, native_surface_scroll, native_surface_set_bounds,
    native_surface_set_bright_bold, native_surface_set_cell_metrics,
    native_surface_set_divider_color, native_surface_set_font, native_surface_set_theme,
    native_surface_set_visible, native_surface_wheel, open_help_window, open_settings_window,
    plugins_list, plugins_reload, plugins_set_enabled, presets_install, presets_remove,
    profile_create, profile_delete, profile_duplicate, profile_export, profile_get_scope,
    profile_import, profile_rename, profile_resolve_match, profile_set_metadata, profile_set_scope,
    profile_switch, profiles_list, scrollback_load, session_connect, session_disconnect,
    session_send, session_send_input, session_send_masked, session_set_window_size, target_get,
    tick_get_config, tick_set_config, timers_delete, timers_list, timers_set, triggers_export,
    triggers_groups_list, triggers_import, triggers_list, triggers_set_group_enabled,
    ui_get_config, ui_set_config, updater_check, updater_install_and_relaunch, AppState,
    SharedState,
};
use fonts::{fonts_list, handle_font_uri};
use map_state::MapState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
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
        // Closing the main window should take every auxiliary window
        // (settings, etc.) down with it. Tauri only exits the process
        // when the LAST window closes, so without this the settings
        // popup hangs around alone after the user closes the main
        // client.
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                let app = window.app_handle();
                for (label, w) in app.webview_windows() {
                    if label != "main" {
                        let _ = w.close();
                    }
                }
            }
        })
        .plugin(tauri_plugin_shell::init())
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
            // Read the font list for Appearance while the app starts,
            // on the blocking pool, never here on the main thread.
            #[cfg(target_os = "macos")]
            fonts::warm_font_cache();
            match open_map_store(app) {
                Ok(store) => {
                    let map = state.map.clone();
                    tauri::async_runtime::block_on(async move {
                        let mut guard = map.lock().await;
                        *guard = Some(MapState::new(store));
                    });
                }
                Err(e) => {
                    error!(error = %e, "map store failed to open; map features disabled");
                }
            }
            if let Ok(path) = app.path().app_data_dir() {
                migrate_from_mudclient_dir(&path);
                // How full each affect was cast, per character, for the
                // Affects pane's gauges. Read when the game names you.
                state
                    .affect_full
                    .set_path(path.join(affect_full::FILE_NAME));

                // The profile set and the active profile, then the
                // shared catalog and loadouts in loadout mode. See
                // launch.rs.
                let launched = tauri::async_runtime::block_on(launch::load(&state, &path));
                if launched.loadout_mode {
                    crate::input::PATH_B_ACTIVE.store(true, std::sync::atomic::Ordering::Release);
                }
                if launched.wizard_unfinished {
                    // The next launch writes the wizard journal again, over
                    // anything this session would save.
                    crate::commands::MIGRATION_RELAUNCH_PENDING
                        .store(true, std::sync::atomic::Ordering::Release);
                }
                match open_log_store(&path) {
                    Ok(store) => {
                        // Searches read through a second connection so
                        // they never wait on, or hold up, the session
                        // loop's appends. Without it they share the
                        // writer.
                        let reader = match open_log_store(&path) {
                            Ok(reader) => Some(reader),
                            Err(e) => {
                                tracing::warn!(error = %e, "log reader failed to open; searches share the writer");
                                None
                            }
                        };
                        let logs = state.logs.clone();
                        let log_reader = state.log_reader.clone();
                        tauri::async_runtime::block_on(async move {
                            *logs.lock().await = Some(store);
                            *log_reader.lock().await = reader;
                        });
                    }
                    Err(e) => {
                        error!(error = %e, "log store failed to open; logging disabled");
                    }
                }
                let scrollback_path = log_state::scrollback_path(&path);
                if let Ok(bytes) = std::fs::read(&scrollback_path) {
                    let scrollback = state.scrollback.clone();
                    tauri::async_runtime::block_on(async move {
                        let mut sb = scrollback.lock().await;
                        sb.load_from_bytes(&bytes);
                    });
                    info!(path = %scrollback_path.display(), "loaded scrollback");
                }

                let plugins_dir = path.join("plugins");
                let _ = std::fs::create_dir_all(&plugins_dir);
                seed_example_plugins(&plugins_dir);
                let plugins_handle = state.plugins.clone();
                let profile_handle = state.profile.clone();
                tauri::async_runtime::block_on(async move {
                    let mut mgr = plugins_handle.lock().await;
                    mgr.set_plugins_dir(plugins_dir.clone());
                    if let Err(e) = mgr.discover() {
                        error!(error = %e, "plugin discovery failed");
                    }
                    let enabled = {
                        let p = profile_handle.lock().await;
                        p.plugins.enabled.clone()
                    };
                    mgr.set_enabled(enabled.clone());
                    for name in &enabled {
                        match mgr.read_entry(name) {
                            Ok(code) => {
                                let mut p = profile_handle.lock().await;
                                crate::script_state::snapshot_vars(&p.script, &p.vars);
                                match p.script.load_script(&format!("plugin:{name}"), code) {
                                    Ok(outcome) => {
                                        let _ = crate::script_state::apply_actions(&mut p, outcome);
                                        info!(name = %name, "loaded plugin");
                                    }
                                    Err(e) => {
                                        error!(name = %name, error = %e, "plugin script error");
                                    }
                                }
                            }
                            Err(e) => {
                                error!(name = %name, error = %e, "plugin entry missing");
                            }
                        }
                    }
                });
            }
            #[cfg(target_os = "macos")]
            {
                for (_, window) in app.webview_windows() {
                    let _ = enable_macos_spellcheck(&window);
                }
            }
            // Tier 3: install the native terminal surface over the webview
            // in the main window. See native_surface.
            #[cfg(native_surface)]
            {
                if let Some(main) = app.get_webview_window("main") {
                    let _ = native_surface::install_probe(&main);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_version,
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
            session_connect,
            session_send,
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
            map_walk_to,
            map_set_note,
            map_set_avoid,
            profile_export,
            profile_import,
            logs_list_sessions,
            logs_search,
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
            profile_set_metadata,
            profile_resolve_match,
            migration_analyze,
            migration_apply,
            app_quit,
            commands::launch_notices_take,
            exit_flush::pending_writes_flushed,
            loadouts_get_state,
            loadouts_set_active,
            tick_get_config,
            tick_set_config,
            profile_get_scope,
            profile_set_scope,
            plugins_list,
            plugins_set_enabled,
            plugins_reload,
            open_settings_window,
            open_help_window,
            window_backdrop::window_backdrop_set,
            dock_layout_get,
            dock_layout_set,
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
            prompt_lookup::prompt_last_seen,
            prompt_commands::prompt_config_get,
            prompt_commands::prompt_config_set,
            prompt_commands::prompt_card_open,
            prompt_commands::prompt_code_reader_set,
            prompt_commands::prompt_designs_list,
            prompt_commands::prompt_compile,
            prompt_commands::prompt_candidates,
            prompt_commands::prompt_capture_check,
            prompt_commands::prompt_capture_from_line,
            prompt_commands::prompt_render,
            prompt_commands::prompt_render_many,
            prompt_commands::prompt_preview_set,
            prompt_commands::prompt_edit,
            prompt_commands::prompt_describe,
            prompt_commands::prompt_forms,
            prompt_commands::prompt_line_triggers,
            prompt_commands::prompt_state_get,
            prompt_commands::prompt_watch,
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
            aliases_groups_list,
            aliases_set_group_enabled,
            triggers_groups_list,
            triggers_set_group_enabled,
            macros_groups_list,
            macros_set_group_enabled,
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
        .run(|app_handle, event| {
            // Backstop flush: slash-command and Lua edits ride a debounced
            // persist that may not have fired when the user quits (Cmd+Q,
            // window close). Write the profile out before the process
            // ends so nothing authored this session is lost. Before that
            // write, an exit request asks the open windows for the edits
            // they hold back (the Settings autosave, a pane width, the
            // field you are typing in) and waits a short, bounded time
            // for them (exit_flush.rs). Matches both exit events because
            // macOS quit paths that go through NSApplication terminate
            // can deliver Exit without a preceding ExitRequested. The
            // exit flow keeps the write to exactly once when both arrive.
            match event {
                tauri::RunEvent::ExitRequested { code, api, .. } => {
                    // Tauri does not let a restart be held.
                    let can_hold = code != Some(tauri::RESTART_EXIT_CODE);
                    let windows = app_handle
                        .webview_windows()
                        .into_keys()
                        .filter(|label| exit_flush::holds_writes(label))
                        .count();
                    match exit_flush::exit_requested(can_hold, windows) {
                        exit_flush::ExitStep::AskWindows => {
                            api.prevent_exit();
                            let app = app_handle.clone();
                            let code = code.unwrap_or(0);
                            tauri::async_runtime::spawn(async move {
                                exit_flush::ask_windows_to_flush(&app).await;
                                app.exit(code);
                            });
                        }
                        exit_flush::ExitStep::Hold => api.prevent_exit(),
                        exit_flush::ExitStep::Flush => flush_profile_on_exit(app_handle),
                        exit_flush::ExitStep::Done => {}
                    }
                }
                tauri::RunEvent::Exit => {
                    let step = exit_flush::exit();
                    if step == exit_flush::ExitStep::Flush {
                        flush_profile_on_exit(app_handle);
                    }
                }
                _ => {}
            }
        });
}

/// Write the live profile once on the way out. The exit flow in
/// [`exit_flush`] decides when, so this runs exactly once.
fn flush_profile_on_exit(app_handle: &tauri::AppHandle) {
    // The affect fulls are a cache of their own, written whatever
    // becomes of the profile.
    app_handle
        .state::<commands::SharedState>()
        .affect_full
        .flush();
    // Honor a #profile reset/load: the in-memory profile is
    // deliberately diverged from disk; do not write it back.
    if commands::AUTO_PERSIST_SUPPRESSED.load(std::sync::atomic::Ordering::Acquire) {
        info!("exit flush: skipped, persist suppressed by profile reset or load");
        return;
    }
    info!("exit flush: persisting profile");
    let state: commands::SharedState = app_handle.state::<commands::SharedState>().inner().clone();
    // Bounded: a wedged Lua trigger holding the profile lock
    // must not turn quit into a hang. The timeout cuts the
    // lock waits; the file writes themselves are sync and
    // small.
    let flush = commands::persist_profile(app_handle, &state);
    let outcome = tauri::async_runtime::block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(3), flush).await
    });
    match outcome {
        Ok(()) => info!("exit flush: done"),
        Err(_) => {
            tracing::warn!("exit flush: timed out after 3s, exiting without it");
        }
    }
}

/// One-shot rename migration: when the bundle identifier flipped from
/// `com.aabahran.mudclient` to `com.aabahran.vosh`, the macOS/Windows/Linux
/// app-data directory moved with it. On first run after the rename, find
/// the old directory next to the new one and recursively copy its
/// contents over so saved profile, scrollback, maps, logs, and plugins
/// survive the rebrand. Skips if the new directory already has its own
/// data (so we never clobber a real fresh install).
fn migrate_from_mudclient_dir(new_dir: &std::path::Path) {
    let Some(parent) = new_dir.parent() else {
        return;
    };
    let Some(new_name) = new_dir.file_name().and_then(|s| s.to_str()) else {
        return;
    };
    // Replace the trailing "vosh" segment with "mudclient". The
    // identifier change is the only diff between the two paths.
    let Some(old_name) = new_name
        .strip_suffix("vosh")
        .map(|prefix| format!("{prefix}mudclient"))
    else {
        return;
    };
    let old_dir = parent.join(&old_name);
    if !old_dir.exists() {
        return;
    }
    let migrated_flag = new_dir.join(".migrated-from-mudclient");
    if migrated_flag.exists() {
        return;
    }
    // Don't overwrite a real install. If the new dir already has a
    // profile or any of the core data files, the user has already used
    // the renamed build — leave them alone.
    let occupied = [
        "profile.toml",
        "scrollback.bin",
        "maps.sqlite",
        "logs.sqlite",
    ]
    .iter()
    .any(|name| new_dir.join(name).exists());
    if occupied {
        let _ = std::fs::create_dir_all(new_dir);
        let _ = std::fs::write(&migrated_flag, "skipped: new dir already populated\n");
        return;
    }
    if let Err(e) = std::fs::create_dir_all(new_dir) {
        error!(error = %e, "failed to create new app data dir for migration");
        return;
    }
    match copy_dir_recursive(&old_dir, new_dir) {
        Ok(count) => {
            info!(
                from = %old_dir.display(),
                to = %new_dir.display(),
                files = count,
                "migrated app data from prior mudclient install",
            );
            let _ = std::fs::write(&migrated_flag, format!("copied {count} files\n"));
        }
        Err(e) => {
            error!(error = %e, "app data migration failed");
        }
    }
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<usize> {
    let mut count = 0;
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            count += copy_dir_recursive(&from, &to)?;
        } else if file_type.is_file() {
            std::fs::copy(&from, &to)?;
            count += 1;
        }
        // Skip symlinks and other special entries; the mudclient app
        // data dir never contained any.
    }
    Ok(count)
}

fn open_map_store(app: &tauri::App) -> Result<MapStore, Box<dyn std::error::Error>> {
    let dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&dir)?;
    let scripts_dir = dir.join("scripts");
    if !scripts_dir.exists() {
        std::fs::create_dir_all(&scripts_dir)?;
    }
    let path = dir.join("maps.sqlite");
    info!(path = %path.display(), "opening map store");
    Ok(MapStore::open(&path)?)
}

fn open_log_store(dir: &std::path::Path) -> Result<LogStore, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;
    let path = log_state::log_db_path(dir);
    info!(path = %path.display(), "opening log store");
    Ok(LogStore::open(&path)?)
}

/// Drop the example plugins shipped with the app into the user's plugins
/// directory if they're not already there. Lets a fresh install show
/// something usable in the Plugins fieldset without manual setup.
fn seed_example_plugins(plugins_dir: &std::path::Path) {
    const EXAMPLES: &[(&str, &[(&str, &str)])] = &[(
        "vitals_alert",
        &[
            (
                "manifest.toml",
                include_str!("../../plugins/vitals_alert/manifest.toml"),
            ),
            (
                "main.lua",
                include_str!("../../plugins/vitals_alert/main.lua"),
            ),
        ],
    )];
    for (name, files) in EXAMPLES {
        let dir = plugins_dir.join(name);
        if dir.exists() {
            continue;
        }
        if let Err(e) = std::fs::create_dir_all(&dir) {
            error!(plugin = %name, error = %e, "failed to seed plugin directory");
            continue;
        }
        for (filename, contents) in *files {
            let path = dir.join(filename);
            if let Err(e) = std::fs::write(&path, contents) {
                error!(plugin = %name, file = %filename, error = %e, "failed to seed plugin file");
            }
        }
        info!(plugin = %name, "seeded example plugin");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_version_matches_cargo_pkg_version() {
        assert_eq!(app_version(), env!("CARGO_PKG_VERSION"));
    }
}

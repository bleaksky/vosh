use std::sync::Arc;

use tauri::Manager;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use vosh_log::LogStore;
use vosh_map::MapStore;

mod commands;
mod connection;
mod fonts;
mod gmcp_bind;
mod import;
mod input;
mod line_accumulator;
mod loadout;
mod loadout_store;
mod log_state;
mod map_state;
mod migration;
mod plugins;
mod profile;
mod profile_config;
mod profile_set;
mod script_state;
mod session;
mod tick;
mod tintin_import;

use commands::{
    aliases_export, aliases_groups_list, aliases_import, aliases_set_group_enabled, app_version,
    dock_layout_get, dock_layout_set, import_apply, import_detect, logs_export, logs_list_sessions,
    logs_search, macros_delete, macros_groups_list, macros_list, macros_set,
    macros_set_group_enabled, map_area_snapshot, map_set_avoid, map_set_note, map_walk_to,
    open_settings_window, plugins_list, plugins_reload, plugins_set_enabled, presets_install,
    presets_remove, profile_create, profile_delete, profile_duplicate, profile_export,
    profile_get_scope, profile_import, profile_rename, profile_resolve_match, profile_set_metadata,
    profile_set_scope, profile_switch, profiles_list, scrollback_load, session_connect,
    session_disconnect, session_send, session_send_input, session_set_window_size, target_get,
    triggers_export, triggers_groups_list, triggers_import, triggers_list,
    triggers_set_group_enabled, ui_get_config, ui_set_config, updater_check,
    updater_install_and_relaunch, AppState, SharedState,
};
use fonts::{fonts_list, handle_font_uri};
use map_state::MapState;
use profile_config::ProfileConfig;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let state: SharedState = Arc::new(AppState::default());

    tauri::Builder::default()
        // Serves font files by family name. Frontend mints @font-face
        // blocks pointing at font://<family> so the webview can render
        // user-installed fonts WebKit otherwise refuses to match.
        .register_uri_scheme_protocol("font", |_ctx, request| {
            handle_font_uri(request.uri())
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

                // Load (or migrate from the legacy single-file layout)
                // the named-profile collection. Then load whichever
                // profile the index marks as active and apply it to
                // the live in-memory state, finally overlaying the
                // shared global.toml (theme / font / dock_layout /
                // keep_last / auto_update) so those UI prefs are
                // consistent across all profiles.
                match profile_set::ProfileSet::load_or_migrate(path.clone()) {
                    Ok(set) => {
                        let active_path = set.active_path();
                        let global_path = set.global_path();
                        if active_path.exists() {
                            match ProfileConfig::load(&active_path) {
                                Ok(snapshot) => {
                                    let profile = state.profile.clone();
                                    tauri::async_runtime::block_on(async move {
                                        let mut p = profile.lock().await;
                                        let warnings = snapshot.apply_to(&mut p);
                                        for w in warnings {
                                            info!(warning = %w, "profile apply warning");
                                        }
                                    });
                                    info!(
                                        path = %active_path.display(),
                                        active = %set.active_name(),
                                        "loaded profile",
                                    );
                                }
                                Err(e) => {
                                    error!(error = %e, "failed to load active profile at startup");
                                }
                            }
                        }
                        if global_path.exists() {
                            match profile_config::GlobalConfig::load(&global_path) {
                                Ok(global) => {
                                    let profile = state.profile.clone();
                                    tauri::async_runtime::block_on(async move {
                                        let mut p = profile.lock().await;
                                        global.apply_to(&mut p);
                                    });
                                    info!(path = %global_path.display(), "loaded global config");
                                }
                                Err(e) => {
                                    error!(error = %e, "failed to load global.toml at startup");
                                }
                            }
                        }
                        let profile_set = state.profile_set.clone();
                        tauri::async_runtime::block_on(async move {
                            let mut guard = profile_set.lock().await;
                            *guard = Some(set);
                        });
                    }
                    Err(e) => {
                        error!(error = %e, "failed to load profile set; using in-memory defaults");
                    }
                }
                match open_log_store(&path) {
                    Ok(store) => {
                        let logs = state.logs.clone();
                        tauri::async_runtime::block_on(async move {
                            let mut guard = logs.lock().await;
                            *guard = Some(store);
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
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_version,
            session_connect,
            session_send,
            session_send_input,
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
            map_area_snapshot,
            map_walk_to,
            map_set_note,
            map_set_avoid,
            profile_export,
            profile_import,
            logs_list_sessions,
            logs_search,
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
            profile_get_scope,
            profile_set_scope,
            plugins_list,
            plugins_set_enabled,
            plugins_reload,
            open_settings_window,
            dock_layout_get,
            dock_layout_set,
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
            import_detect,
            import_apply,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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

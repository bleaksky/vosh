use std::sync::Arc;

use vosh_log::LogStore;
use vosh_map::MapStore;
use tauri::Manager;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

mod commands;
mod connection;
mod fonts;
mod gmcp_bind;
mod input;
mod line_accumulator;
mod log_state;
mod map_state;
mod plugins;
mod profile;
mod profile_config;
mod script_state;
mod session;
mod tick;
mod tintin_import;

use commands::{
    aliases_export, aliases_import, app_version, dock_layout_get, dock_layout_set, logs_export,
    logs_list_sessions, logs_search, macros_delete, macros_list, macros_set, map_area_snapshot,
    map_set_avoid, map_set_note, map_walk_to, open_settings_window, presets_install,
    presets_remove, profile_export, profile_import, scrollback_load, session_connect,
    session_disconnect, session_send, session_send_input, target_get, triggers_export,
    triggers_import, plugins_list, plugins_reload, plugins_set_enabled, triggers_list,
    ui_get_config, ui_set_config, updater_check, AppState, SharedState,
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
                let toml_path = path.join("profile.toml");
                if toml_path.exists() {
                    match ProfileConfig::load(&toml_path) {
                        Ok(snapshot) => {
                            let profile = state.profile.clone();
                            tauri::async_runtime::block_on(async move {
                                let mut p = profile.lock().await;
                                let warnings = snapshot.apply_to(&mut p);
                                for w in warnings {
                                    info!(warning = %w, "profile apply warning");
                                }
                            });
                            info!(path = %toml_path.display(), "loaded profile");
                        }
                        Err(e) => {
                            error!(error = %e, "failed to load profile.toml at startup");
                        }
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
    let parent = match new_dir.parent() {
        Some(p) => p,
        None => return,
    };
    let new_name = match new_dir.file_name().and_then(|s| s.to_str()) {
        Some(s) => s,
        None => return,
    };
    // Replace the trailing "vosh" segment with "mudclient". The
    // identifier change is the only diff between the two paths.
    let Some(old_name) = new_name.strip_suffix("vosh").map(|prefix| format!("{prefix}mudclient")) else {
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
    let occupied = ["profile.toml", "scrollback.bin", "maps.sqlite", "logs.sqlite"]
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

fn copy_dir_recursive(
    src: &std::path::Path,
    dst: &std::path::Path,
) -> std::io::Result<usize> {
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

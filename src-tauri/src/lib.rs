use std::sync::Arc;

use mudclient_log::LogStore;
use mudclient_map::MapStore;
use tauri::Manager;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

mod commands;
mod connection;
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
    app_version, dock_layout_get, dock_layout_set, logs_export, logs_list_sessions, logs_search,
    map_area_snapshot, map_set_avoid, map_set_note, map_walk_to, open_settings_window,
    presets_install, presets_remove, profile_export, profile_import, scrollback_load,
    session_connect, session_disconnect, session_send, session_send_input, triggers_export,
    triggers_import, plugins_list, plugins_reload, plugins_set_enabled, triggers_list,
    ui_get_config, ui_set_config, updater_check, AppState, SharedState,
};
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
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_window_state::Builder::default().build())
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
            triggers_export,
            triggers_import,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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

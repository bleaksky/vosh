use std::sync::Arc;

use mudclient_map::MapStore;
use tauri::Manager;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

mod commands;
mod connection;
mod gmcp_bind;
mod input;
mod line_accumulator;
mod map_state;
mod profile;
mod profile_config;
mod script_state;
mod session;
mod tick;
mod tintin_import;

use commands::{
    app_version, map_area_snapshot, map_set_avoid, map_set_note, map_walk_to, profile_export,
    profile_import, session_connect, session_disconnect, session_send, session_send_input,
    triggers_export, triggers_import, triggers_list, AppState, SharedState,
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
            map_area_snapshot,
            map_walk_to,
            map_set_note,
            map_set_avoid,
            profile_export,
            profile_import,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_version_matches_cargo_pkg_version() {
        assert_eq!(app_version(), env!("CARGO_PKG_VERSION"));
    }
}

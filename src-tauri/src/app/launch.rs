//! lib.rs hands [`setup`] to the app's setup hook. It runs every step the
//! app takes as it starts, in order. [`load`] is what launch loads from
//! the app data folder before any window opens: the profile set with the
//! active profile, then the shared catalog and the loadouts when you use
//! loadout mode. Tests run [`load`] to relaunch over a folder of their
//! own.

use std::path::Path;
use std::sync::atomic::Ordering;

use tauri::Manager;
use tracing::{error, info};
use vosh_log::LogStore;

use crate::app::state::SharedState;
use crate::disk::paths;
use crate::loadouts;
use crate::loadouts::catalog::{lay_catalog_over, loadout_mode_on, save_global_catalog};
use crate::loadouts::presets::{adopt_catalog_presets, profile_preset_lists};
use crate::loadouts::wizard::journal::{self, WizardRun};
use crate::profile::file::load_at_launch;
use crate::profile::set::ProfileSet;

/// Every startup step, in order, as the app's setup hook runs them.
pub(crate) fn setup(app: &tauri::App, state: &SharedState) {
    // Read the font list for Appearance while the app starts,
    // on the blocking pool, never here on the main thread.
    #[cfg(target_os = "macos")]
    crate::app::system_fonts::warm_font_cache();
    // The data folder and the scripts folder in it, where
    // `#script load` finds Lua files.
    if let Err(e) = create_scripts_dir(app) {
        error!(error = %e, "scripts folder could not be created");
    }
    if let Ok(path) = app.path().app_data_dir() {
        // How full each affect was cast, per character, for the
        // Affects pane's gauges. Read when the game names you.
        state.affect_full.set_path(paths::affect_full_path(&path));

        // The profile set and the active profile, then the
        // shared catalog and loadouts in loadout mode. See `load`.
        tauri::async_runtime::block_on(load(state, &path));
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
        let scrollback_path = paths::scrollback_path(&path);
        if let Ok(bytes) = std::fs::read(&scrollback_path) {
            let scrollback = state.scrollback.clone();
            tauri::async_runtime::block_on(async move {
                let mut sb = scrollback.lock().await;
                sb.load_from_bytes(&bytes);
            });
            info!(path = %scrollback_path.display(), "loaded scrollback");
        }

        let plugins_dir = paths::plugins_dir(&path);
        let _ = std::fs::create_dir_all(&plugins_dir);
        crate::app::plugins::seed_example_plugins(&plugins_dir);
        tauri::async_runtime::block_on(crate::app::plugins::load_enabled_plugins(
            app.handle(),
            state,
            plugins_dir,
        ));
    }
    #[cfg(target_os = "macos")]
    {
        for (_, window) in app.webview_windows() {
            let _ = crate::app::windows::enable_macos_spellcheck(&window);
        }
    }
    // The native terminal surface goes under the main window's webview.
    // See native/surface.rs.
    #[cfg(native_surface)]
    {
        if let Some(main) = app.get_webview_window("main") {
            let _ = crate::native::surface::install(&main);
        }
    }
}

/// Everything launch loads, in order, from `app_data`, which it keeps as
/// the state's app data folder first. A shared catalog wizard run that
/// stopped partway finishes first, so nothing loads a file it had yet to
/// write. Then launch reads profiles.toml, once, and runs the one time
/// upgrades over the set, see [`crate::disk::upgrades::run`]. Then the
/// profiles load, see [`load_profiles`], and loadout mode starts when
/// catalog.toml is on disk, see [`load_loadout_mode`]. Launch then turns
/// on [`AppState::loadout_mode`]. While the run stays unfinished, the
/// prompt capture move, the preset rollouts and loadout mode wait, since
/// a profile file may still hold its items under their old group names
/// and would lay them over the catalog for every character. The session
/// runs on the active profile file alone, and launch turns on
/// [`AppState::relaunch_pending`], which holds every save and every
/// profile switch until the next launch finishes the run. When
/// profiles.toml does not read, the upgrades and the profiles wait for
/// the next launch, and the session runs on the defaults.
///
/// [`AppState::loadout_mode`]: crate::app::state::AppState::loadout_mode
/// [`AppState::relaunch_pending`]: crate::app::state::AppState::relaunch_pending
pub(crate) async fn load(state: &SharedState, app_data: &Path) {
    // Every command and `#profile` or `#script` line finds its files
    // under it from here on. Launch runs once, so the folder never moves.
    let _ = state.app_data.set(app_data.to_path_buf());
    let run = journal::finish_wizard_run(app_data);
    state.add_launch_notices(run.notices());
    let wizard_settled = run != WizardRun::Unfinished;
    match ProfileSet::load_or_migrate(app_data.to_path_buf()) {
        Ok(mut set) => {
            let notices = crate::disk::upgrades::run(&mut set, app_data, wizard_settled).await;
            state.add_launch_notices(notices);
            load_profiles(state, set).await;
        }
        Err(e) => {
            error!(error = %e, "failed to load profile set; skipping the upgrades and using in-memory defaults");
        }
    }
    if !wizard_settled {
        // The next launch writes the wizard journal again, over
        // anything this session would save.
        state.relaunch_pending.store(true, Ordering::Release);
        return;
    }
    if load_loadout_mode(state, app_data).await {
        state.loadout_mode.store(true, Ordering::Release);
    }
}

/// Load whichever profile `set`, the profile set launch read, marks as
/// active into the live profile, and overlay the shared global.toml
/// (theme, font, dock layout, keep last, auto update) so those UI prefs
/// stay the same across every profile.
pub(crate) async fn load_profiles(state: &SharedState, mut set: ProfileSet) {
    // What an earlier session left to tell you, once.
    state.add_launch_notices(set.take_notices());
    // A file that does not read keeps the defaults in its place for this
    // session, and no save writes over it. The notices tell you so once
    // the main window shows.
    let notices = {
        let mut p = state.profile.lock().await;
        let notices = load_at_launch(&set, &mut p);
        let mut c = state.connection.lock().await;
        let table = p.prompt.clone();
        crate::prompt::take_config(&mut p, &mut c, table);
        notices
    };
    state.add_launch_notices(notices);
    state.note_active_profile(set.active_name());
    *state.profile_set.lock().await = Some(set);
}

/// Loadout mode startup, after [`load_profiles`], see [`load`]. When
/// catalog.toml is on disk, read it and loadouts.toml, have the catalog
/// adopt the presets once when it holds no list yet, then lay the catalog
/// and the active loadouts over the live profile, see
/// [`lay_catalog_over`]. Returns true when loadout mode is live.
pub(crate) async fn load_loadout_mode(state: &SharedState, app_data: &Path) -> bool {
    if !loadout_mode_on(app_data) {
        return false;
    }
    let (mut catalog, set) = match loadouts::load_at_launch(app_data) {
        Ok(files) => files,
        // A file that does not read keeps the session on the profile
        // files alone. Both files are held so no save writes a catalog
        // without your shared items, and the notices tell you so.
        Err(notices) => {
            error!(
                "catalog.toml or loadouts.toml failed to load; falling back to per-profile state"
            );
            state.add_launch_notices(notices);
            return false;
        }
    };
    // The catalog owns which presets are on, with the preset triggers. An
    // older catalog takes every preset any profile file had on, once, and
    // saves it so a launch as another character keeps it. A profile file
    // that does not read is left out, and the notices name it.
    let preset_lists = if catalog.enabled_presets.is_none() {
        state
            .profile_set
            .lock()
            .await
            .as_ref()
            .map(profile_preset_lists)
    } else {
        None
    };
    let presets_moved = {
        let mut p = state.profile.lock().await;
        adopt_catalog_presets(&mut catalog, &mut p, preset_lists.as_ref())
    };
    if let Some(lists) = &preset_lists {
        state.add_launch_notices(lists.unread_notices(presets_moved));
    }
    if presets_moved {
        match save_global_catalog(app_data, &catalog) {
            Ok(()) => info!("moved the enabled presets into catalog.toml"),
            Err(e) => {
                error!(error = %e, "failed to save the enabled presets to catalog.toml");
            }
        }
    }
    // adopt_catalog_presets left the live preset list equal to the
    // catalog's, or the catalog with none, so the overlay leaves it as is.
    lay_catalog_over(&mut *state.profile.lock().await, &catalog, Some(&set));
    *state.global_catalog.lock().await = Some(catalog);
    *state.loadout_set.lock().await = Some(set);
    info!("loaded catalog.toml and loadouts.toml");
    true
}

/// Create the app data folder and the `scripts` folder in it, where
/// `#script load` finds Lua files. The map store's opener did this until
/// D3 retired the store. maps.sqlite stays on disk as it is, and nothing
/// reads or writes it.
fn create_scripts_dir(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(paths::scripts_dir(&dir))?;
    Ok(())
}

fn open_log_store(dir: &std::path::Path) -> Result<LogStore, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;
    let path = paths::log_db_path(dir);
    info!(path = %path.display(), "opening log store");
    Ok(LogStore::open(&path)?)
}

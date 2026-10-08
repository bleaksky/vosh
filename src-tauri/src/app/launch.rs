//! lib.rs hands [`setup`] to the app's setup hook. It runs every step the
//! app takes as it starts, in order. [`load`] is what launch loads from
//! the app data folder before any window opens: the profile set with the
//! sessions you had and the active profile, then the shared catalog and
//! the loadouts when you use loadout mode. [`start_selected`] then gives
//! the selected session its scrollback and plugins, and a restored
//! session opens its own on its first selection, see [`open_restored`].
//! Tests run [`load`] to relaunch over a folder of their own.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::{AppHandle, Manager};
use tracing::{error, info};
use vosh_log::LogStore;

use crate::app::events::{broadcast, broadcast_profile_ui, PROFILE_SWITCHED};
use crate::app::state::SharedState;
use crate::disk::paths;
use crate::disk::save::PERSIST_LOCK;
use crate::loadouts;
use crate::loadouts::catalog::{lay_catalog_over, loadout_mode_on, save_global_catalog};
use crate::loadouts::presets::{adopt_catalog_presets, profile_preset_lists};
use crate::loadouts::wizard::journal::{self, WizardRun};
use crate::profile::file::load_at_launch;
use crate::profile::open::{lock_both, OpenProfile};
use crate::profile::set::ProfileSet;
use crate::sessions::Session;

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
        state.affect_file.set_path(paths::affect_full_path(&path));

        // The profile set and the active profile, then the
        // shared catalog and loadouts in loadout mode. See `load`.
        tauri::async_runtime::block_on(load(state, &path));
        match open_log_store(&path) {
            Ok(mut store) => {
                // A crash left these open. They end at their last line,
                // before any connection opens a log.
                if let Err(e) = store.end_crashed_sessions() {
                    tracing::warn!(error = %e, "could not end the logs a crash left open");
                }
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
                // Keep logs for, now and once a day.
                crate::logs::retention::start(state);
            }
            Err(e) => {
                error!(error = %e, "log store failed to open; logging disabled");
            }
        }
        tauri::async_runtime::block_on(start_selected(app.handle(), state, &path));
        // The scrollback that changed, every few minutes.
        crate::logs::start_saving_scrollback(state);
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
    // A click on an alert banner selects the session it names.
    crate::alert::banner::install(app.handle());
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

/// Restore the sessions `set`, the profile set launch read, lists, none
/// connected, see [`Sessions::restore`]. Then load whichever profile it
/// marks as active into the profile the selected session plays, which
/// takes its name, and overlay the shared global.toml (theme, font, dock
/// layout, keep last, auto update) so those UI prefs stay the same across
/// every profile. The session takes the profile's tick settings and
/// `[prompt]` table. With no list, the one session the app starts with
/// plays the active profile.
///
/// [`Sessions::restore`]: crate::sessions::Sessions::restore
pub(crate) async fn load_profiles(state: &SharedState, mut set: ProfileSet) {
    let (sessions, selected) = set.sessions();
    state.restore_sessions(sessions, selected);
    let session = state.selected_session();
    // What an earlier session left to tell you, once.
    state.add_launch_notices(set.take_notices());
    // A file that does not read keeps the defaults in its place for this
    // session, and no save writes over it. The notices tell you so once
    // the main window shows.
    let notices = {
        let mut p = session.lock_profile().await;
        p.set_name(set.active_name());
        let tick_before = p.tick.config.clone();
        let notices = load_at_launch(&set, &mut p);
        let mut c = session.connection.lock();
        crate::profile::switch::hand_to_connection(&mut p, &mut c, &tick_before);
        notices
    };
    state.add_launch_notices(notices);
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
    let session = state.selected_session();
    let presets_moved = {
        let mut p = session.lock_profile().await;
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
    {
        let mut p = session.lock_profile().await;
        let gate = set.for_profile(p.name.as_deref());
        lay_catalog_over(&mut p, &catalog, Some(&gate));
    }
    *state.global_catalog.lock().await = Some(catalog);
    *state.loadout_set.lock().await = Some(set);
    info!("loaded catalog.toml and loadouts.toml");
    true
}

/// Start the session launch selected on what it kept: its scrollback,
/// then the plugins its profile turns on, once the example plugins are in
/// the plugins folder.
pub(crate) async fn start_selected<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    app_data: &Path,
) {
    let session = state.selected_session();
    read_scrollback(&session, app_data).await;
    let plugins_dir = paths::plugins_dir(app_data);
    let _ = std::fs::create_dir_all(&plugins_dir);
    crate::app::plugins::seed_example_plugins(&plugins_dir);
    crate::app::plugins::load_enabled_plugins(app, &session, plugins_dir).await;
}

/// Hand `session` what the profile it plays holds for a session that
/// starts on it, new or restored: its connection takes the tick settings
/// and the `[prompt]` table, and its Lua engine loads the plugins the
/// profile turns on, as the first session's does at launch.
pub(crate) async fn start_on_profile<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    session: &Arc<Session>,
) {
    {
        let mut p = session.lock_profile().await;
        let tick_before = p.tick.config.clone();
        let mut c = session.connection.lock();
        crate::profile::switch::hand_to_connection(&mut p, &mut c, &tick_before);
    }
    if let Some(app_data) = state.app_data.get() {
        let plugins_dir = paths::plugins_dir(app_data);
        crate::app::plugins::load_enabled_plugins(app, session, plugins_dir).await;
    }
}

/// Open the profile a restored session last played the first time you
/// select it, or join it when another session plays it, and start the
/// session on it, see [`start_on_profile`]. A profile the set no longer
/// lists gives way to the active one. The session's grid then takes the
/// lines its scrollback file kept. Does nothing for a session whose
/// profile is open. A profile that does not open leaves the session
/// waiting, and the error says why, as it does for a session that
/// closed.
pub(crate) async fn open_restored<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    session: &Arc<Session>,
) -> Result<(), String> {
    {
        // No other step opens or closes a profile until the session plays
        // it.
        let _persist_guard = PERSIST_LOCK.lock().await;
        // A session that closed opens nothing, since no close would close
        // the profile it opened. The close takes the session out of the
        // map under this lock.
        state.session(Some(session.id))?;
        let waiting = session.profile();
        if state.is_open(&waiting) {
            return Ok(());
        }
        let name = {
            let set = state.loaded_profile_set().await?;
            waiting
                .name()
                .filter(|name| set.get(name).is_some())
                .unwrap_or_else(|| set.active_name().to_string())
        };
        let open = crate::profile::switch::open_or_join(state, &name).await?;
        let _both = lock_both(&waiting, &open).await;
        session.play(open);
    }
    start_on_profile(app, state, session).await;
    if let Some(app_data) = state.app_data.get() {
        read_scrollback(session, app_data).await;
    }
    // The grid takes the lines once, as the selected session's does when
    // the page loads its scrollback.
    #[cfg(any(native_surface, test))]
    if crate::native::grid::claim_seed(session.id) {
        let bytes = session.scrollback.lock().await.dump_live();
        if !bytes.is_empty() {
            crate::native::grid::feed_local(session.id, &bytes);
            #[cfg(native_surface)]
            crate::native::surface::request_redraw();
        }
    }
    Ok(())
}

/// Select the session `id` names, and show its grid in the place of the
/// grid that showed. A session launch restored opens its profile and
/// reads its scrollback the first time, see [`open_restored`], and
/// profiles.toml then keeps it as the selected one. Every window hears
/// the rows with the new selection, the one a banner click makes among
/// them, and then what the selection brings to the front, see
/// [`show_selection`].
pub(crate) async fn select_session<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    id: crate::sessions::SessionId,
) -> Result<(), String> {
    let front = state.selected_session().profile();
    state.select_session(id)?;
    let selected = state.session(Some(id))?;
    let opened = open_restored(app, state, &selected).await;
    {
        let _persist_guard = PERSIST_LOCK.lock().await;
        crate::profile::set::save_sessions(state).await;
        crate::sessions::broadcast_sessions(app, state);
    }
    show_selection(app, state, &front).await;
    opened
}

/// Tell every window what the selected session brings to the front, once
/// the selection moved from a session that played `front`. The windows
/// show the profile the selected session plays, so when it plays another
/// one every window takes its panes, tracked affects, tick settings and
/// the rest, then hears its name on `vosh://profile-switched`, as after a
/// switch. Settings › Characters hears who the session is logged in as.
/// With the native surface the page hears how far back the grid that
/// now shows sits, so the depth chip and the copy in xterm follow it.
/// Call it once profiles.toml names the new active profile, which
/// Characters reads again on the switch.
pub(crate) async fn show_selection<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    front: &Arc<OpenProfile>,
) {
    #[cfg(native_surface)]
    crate::native::surface::report_scroll();
    let selected = state.selected_session();
    let plays = selected.profile();
    if !Arc::ptr_eq(&plays, front) {
        broadcast_profile_ui(app, state).await;
        broadcast(app, PROFILE_SWITCHED, &plays.name());
    }
    crate::session::identity::broadcast_session_identity(app, state, &selected).await;
}

/// Read the lines the scrollback file of `session` kept into its ring.
async fn read_scrollback(session: &Session, app_data: &Path) {
    let path = paths::scrollback_path(app_data, session.id);
    if let Ok(bytes) = std::fs::read(&path) {
        session.scrollback.lock().await.load_from_bytes(&bytes);
        info!(path = %path.display(), "loaded scrollback");
    }
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

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
    use tauri::{App, Manager};

    use crate::app::state::{AppState, SharedState};
    use crate::disk::paths;
    use crate::ipc::session::{
        session_close, session_move, session_open, session_rename, session_select,
        session_set_address,
    };
    use crate::profile::file::ProfileConfig;
    use crate::profile::live::Profile;
    use crate::profile::set::{ProfileSet, SessionEntry, DEFAULT_PROFILE_NAME};
    use crate::sessions::SessionId;

    /// Launch over `root` as the app does, with a mock app that holds the
    /// state.
    async fn launch(root: &Path) -> (SharedState, App<MockRuntime>) {
        let state: SharedState = Arc::new(AppState::default());
        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .expect("a mock app");
        app.manage::<SharedState>(state.clone());
        super::load(&state, root).await;
        super::start_selected(app.handle(), &state, root).await;
        (state, app)
    }

    /// Each session's id, name, profile and whether it is selected, with
    /// none connected.
    fn rows(state: &SharedState) -> Vec<(SessionId, Option<String>, Option<String>, bool)> {
        let rows = state.session_rows();
        assert!(rows.iter().all(|row| !row.connected), "{rows:?}");
        rows.into_iter()
            .map(|row| (row.id, row.name, row.profile, row.selected))
            .collect()
    }

    fn open_names(state: &SharedState) -> Vec<String> {
        state
            .open_profiles()
            .iter()
            .filter_map(|open| open.name())
            .collect()
    }

    /// What the plugins printed in the session `id` as they loaded.
    fn plugin_lines(state: &SharedState, id: SessionId) -> String {
        let session = state.session(Some(id)).expect("the session");
        let lines = session.launch_lua_lines.lock().expect("the lines");
        lines.concat()
    }

    /// What the scrollback ring of the session `id` holds.
    async fn kept(state: &SharedState, id: SessionId) -> String {
        let session = state.session(Some(id)).expect("the session");
        let bytes = session.scrollback.lock().await.dump();
        String::from_utf8(bytes).expect("text")
    }

    fn shows(id: SessionId, text: &str) -> bool {
        crate::native::grid::screen_rows(id)
            .is_some_and(|screen| screen.rows.iter().any(|row| row.contains(text)))
    }

    /// Write the session `n`'s scrollback file in `root`.
    fn scrollback(root: &Path, n: u32, line: &str) {
        let path = paths::scrollback_path(root, SessionId::numbered(n));
        std::fs::write(path, format!("{line}\r\n")).expect("the scrollback");
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn three_sessions_restore_unconnected_and_each_opens_its_profile_when_first_selected() {
        // A selection shows the session's grid, which other tests read.
        let _grid = crate::native::grid::lock_shared_grid_for_test();
        let dir = tempfile::tempdir().expect("a folder");
        let root = dir.path();
        let mut set = ProfileSet::load_or_migrate(root.to_path_buf()).expect("a set");
        set.create("Healer").expect("Healer");
        let entry = |n, name: Option<&str>, profile: &str| SessionEntry {
            id: SessionId::numbered(n),
            name: name.map(str::to_string),
            host: None,
            port: None,
            tls: false,
            profile: profile.into(),
        };
        let (one, two, three) = (
            SessionId::numbered(1),
            SessionId::numbered(2),
            SessionId::numbered(3),
        );
        let entries = vec![
            entry(1, Some("Main"), DEFAULT_PROFILE_NAME),
            entry(2, None, "Healer"),
            entry(3, None, "Healer"),
        ];
        set.keep_sessions(Some("Healer"), entries, Some(two))
            .expect("the list saves");
        // Healer turns on a plugin that says so as it loads.
        let mut healer = Profile::default();
        healer.plugins.enabled = vec!["hello".into()];
        ProfileConfig::from_profile(&healer)
            .save(&set.profile_path("Healer"))
            .expect("Healer's file");
        let plugin = paths::plugins_dir(root).join("hello");
        std::fs::create_dir_all(&plugin).expect("the plugin folder");
        std::fs::write(plugin.join("manifest.toml"), "[plugin]\nname = \"hello\"\n")
            .expect("the manifest");
        std::fs::write(plugin.join("main.lua"), "print('Healer is here')\n").expect("the script");
        for (n, line) in [(1, "Line one"), (2, "Line two"), (3, "Line three")] {
            scrollback(root, n, line);
        }

        let (state, app) = launch(root).await;
        let healer = Some("Healer".to_string());
        assert_eq!(
            rows(&state),
            [
                (
                    one,
                    Some("Main".into()),
                    Some(DEFAULT_PROFILE_NAME.into()),
                    false
                ),
                (two, None, healer.clone(), true),
                (three, None, healer.clone(), false),
            ]
        );
        // Only the selected session's profile is open, with its plugins
        // and its scrollback.
        assert_eq!(open_names(&state), ["Healer"]);
        assert!(plugin_lines(&state, two).contains("Healer is here"));
        assert_eq!(plugin_lines(&state, three), "");
        assert!(kept(&state, two).await.contains("Line two"));
        assert_eq!(kept(&state, one).await, "");

        // The first selection of a session opens its profile and seeds its
        // grid from its scrollback, and one on an open profile joins it.
        session_select(app.handle().clone(), app.state(), one)
            .await
            .expect("the first session opens");
        assert_eq!(open_names(&state), ["Healer", DEFAULT_PROFILE_NAME]);
        assert!(kept(&state, one).await.contains("Line one"));
        assert!(shows(one, "Line one"));
        session_select(app.handle().clone(), app.state(), three)
            .await
            .expect("the third session joins Healer");
        assert_eq!(open_names(&state), ["Healer", DEFAULT_PROFILE_NAME]);
        let plays = |id| state.session(Some(id)).expect("a session").profile();
        assert!(Arc::ptr_eq(&plays(three), &plays(two)));
        assert!(plugin_lines(&state, three).contains("Healer is here"));
        assert!(shows(three, "Line three"));
        // A new session takes the next number.
        let four = session_open(app.handle().clone(), app.state(), None)
            .await
            .expect("a new session");
        assert_eq!(four, SessionId::numbered(4));
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn a_session_that_closes_before_its_first_selection_opens_no_profile() {
        // A selection shows the session's grid, which other tests read.
        let _grid = crate::native::grid::lock_shared_grid_for_test();
        let dir = tempfile::tempdir().expect("a folder");
        let root = dir.path();
        let mut set = ProfileSet::load_or_migrate(root.to_path_buf()).expect("a set");
        set.create("Healer").expect("Healer");
        let entry = |n, profile: &str| SessionEntry {
            id: SessionId::numbered(n),
            name: None,
            host: None,
            port: None,
            tls: false,
            profile: profile.into(),
        };
        let entries = vec![entry(1, DEFAULT_PROFILE_NAME), entry(2, "Healer")];
        set.keep_sessions(Some(DEFAULT_PROFILE_NAME), entries, None)
            .expect("the list saves");

        let (state, app) = launch(root).await;
        let two = state
            .session(Some(SessionId::numbered(2)))
            .expect("the second session");
        // A close takes the session out of the map first and then waits
        // for its connection to end. A selection that lands in that wait
        // opens no profile.
        state.close_session(two.id).expect("the close");
        assert_eq!(
            super::open_restored(app.handle(), &state, &two).await,
            Err(crate::sessions::NO_SUCH_SESSION.to_string())
        );
        assert_eq!(open_names(&state), [DEFAULT_PROFILE_NAME]);
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn a_moved_order_and_where_each_session_dials_survive_a_relaunch() {
        // A selection shows the session's grid, which other tests read.
        let _grid = crate::native::grid::lock_shared_grid_for_test();
        let dir = tempfile::tempdir().expect("a folder");
        let root = dir.path();
        let (_, app) = launch(root).await;
        let first = SessionId::FIRST;
        let two = session_open(app.handle().clone(), app.state(), None)
            .await
            .expect("a second session");
        let host = "play.theforsakenlands.com".to_string();
        session_set_address(app.handle().clone(), app.state(), two, host, 1825, true)
            .await
            .expect("the address");
        session_move(app.handle().clone(), app.state(), two, 0)
            .await
            .expect("the move");

        let (state, _app) = launch(root).await;
        let rows: Vec<_> = state
            .session_rows()
            .into_iter()
            .map(|row| (row.id, row.host, row.port, row.tls, row.selected))
            .collect();
        let host = Some("play.theforsakenlands.com".to_string());
        assert_eq!(
            rows,
            [
                (two, host, Some(1825), true, false),
                (first, None, None, false, true)
            ]
        );
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn an_index_with_no_list_gives_one_session_and_a_name_survives_a_relaunch() {
        // A selection shows the session's grid, which other tests read.
        let _grid = crate::native::grid::lock_shared_grid_for_test();
        let dir = tempfile::tempdir().expect("a folder");
        let root = dir.path();
        // profiles.toml as 0.8.1 writes it, with Healer active.
        let written = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../fixtures/config/profiles.full.toml"
        );
        std::fs::copy(written, paths::profiles_index_path(root)).expect("the index");
        scrollback(root, 1, "Line one");
        let healer = Some("Healer".to_string());

        let (state, app) = launch(root).await;
        let first = SessionId::FIRST;
        assert_eq!(rows(&state), [(first, None, healer.clone(), true)]);
        assert!(kept(&state, first).await.contains("Line one"));
        let two = session_open(app.handle().clone(), app.state(), Some("Ranger".into()))
            .await
            .expect("a second session");
        session_rename(app.handle().clone(), app.state(), two, Some("Alt".into()))
            .await
            .expect("the rename");

        let (state, app) = launch(root).await;
        let ranger = Some("Ranger".to_string());
        assert_eq!(
            rows(&state),
            [
                (first, None, healer.clone(), true),
                (two, Some("Alt".into()), ranger, false),
            ]
        );
        // A profile renamed before the session opens it opens under its
        // new name.
        crate::profile::set::rename_profile(&state, "Ranger", "Scout")
            .await
            .expect("the profile rename");
        session_select(app.handle().clone(), app.state(), two)
            .await
            .expect("the second session opens Scout");
        assert_eq!(open_names(&state), ["Healer", "Scout"]);
        // Closing a session takes its scrollback file with it.
        scrollback(root, 2, "Line two");
        session_close(app.handle().clone(), app.state(), two)
            .await
            .expect("the second session closes");
        assert!(!paths::scrollback_path(root, two).exists());
        assert!(paths::scrollback_path(root, first).exists());
    }
}

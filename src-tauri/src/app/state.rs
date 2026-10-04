//! The state the whole app shares. Tauri holds one [`AppState`] for
//! every command, window and session.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use tokio::sync::{MappedMutexGuard, Mutex, MutexGuard};

use crate::app::plugins::SharedPluginManager;
use crate::logs::SharedLogStore;
use crate::profile::live::Profile;
use crate::sessions::{Session, SessionId, Sessions, NO_SUCH_SESSION};

/// What every command, window and session shares. The sessions, the live
/// profile, the profile set, the log store, the plugins, the catalog and
/// loadouts of loadout mode, the affect fulls and the app data folder.
/// The generations that turn away a write read before the profile was
/// replaced live here too, with the counter that settles a burst of
/// changes into one save, the flags that hold the saves back and the flag
/// that says loadout mode is live.
pub(crate) struct AppState {
    /// The sessions and the one selected. See [`crate::sessions`] for
    /// where its lock sits.
    sessions: std::sync::Mutex<Sessions>,
    pub(crate) profile: Arc<Mutex<Profile>>,
    pub(crate) logs: SharedLogStore,
    /// A second connection to the same log database for the read
    /// commands. The session loop appends through `logs`, and a search
    /// can read the whole log, so reads take their own lock and never
    /// hold up the live session. WAL lets both run at once.
    pub(crate) log_reader: SharedLogStore,
    pub(crate) plugins: SharedPluginManager,
    /// Catalog of named profiles. Loaded (or migrated from the legacy
    /// single-file layout) once at startup; commands mutate it under
    /// this mutex.
    pub(crate) profile_set: Arc<Mutex<Option<crate::profile::set::ProfileSet>>>,
    /// How full each affect was cast, per character, for the Affects
    /// pane's gauges. See [`crate::affects::full`].
    pub(crate) affect_full: crate::affects::full::AffectFull,
    /// The shared catalog of loadout mode. `Some` when the app started
    /// up with `catalog.toml` present (loadout mode), `None` in per
    /// profile mode. Mutated alongside the live `Profile` so on-disk
    /// state stays in step with in-memory edits.
    pub(crate) global_catalog: Arc<Mutex<Option<crate::loadouts::catalog::GlobalCatalog>>>,
    /// The loadouts of loadout mode. Same `Some`/`None` semantics as
    /// `global_catalog`. The active subset drives which catalog groups
    /// the runtime gates on (see [`crate::loadouts::gating::apply_loadout_state`]).
    pub(crate) loadout_set: Arc<Mutex<Option<crate::loadouts::set::LoadoutSet>>>,
    /// Sentences launch has to tell you, such as a profile file Vosh
    /// could not read and will not save over. Kept until the main window
    /// takes them through `launch_notices_take`, since launch runs before
    /// any window listens.
    pub(crate) launch_notices: std::sync::Mutex<Vec<String>>,
    /// The active profile's name, kept beside the profile set so an event
    /// can name it without waiting for that lock. Set at launch, on a
    /// switch and on a rename. None before any profile loads.
    pub(crate) active_profile: std::sync::Mutex<Option<String>>,
    /// Counts the times the live profile's panes have been replaced: a
    /// wholesale replace of the UI config, or a pane reset. It moves under
    /// the profile lock in the same step that swaps them, so a pane tree
    /// and the generation read with it always belong together. A pane
    /// layout write carries the generation of the tree it was edited from,
    /// and `pane_layout_set` refuses one from before a swap so it cannot
    /// land on the new profile.
    panes_generation: AtomicU64,
    /// Counts the times the live profile's whole UI config has been
    /// replaced: a profile switch, an import, `#profile load` and `reset`.
    /// It moves under the profile lock in the same step that swaps the
    /// config. `ui_get_config` hands it out with the config, and a whole
    /// config save carries back the one it was read at, so `ui_set_config`
    /// refuses a copy from before a replace rather than write the old
    /// profile's values over the new one.
    ui_config_generation: AtomicU64,
    /// Debounce generation for `mark_profile_dirty`: each mark bumps it,
    /// and the delayed persist only fires if no newer mark arrived while
    /// waiting.
    pub(crate) profile_dirty_gen: AtomicU64,
    /// Set by `#profile reset` / `#profile load`: the in-memory profile is
    /// deliberately diverged from disk, so the passive flushes (debounce,
    /// exit) must not write it. Cleared by the next durable change.
    pub(crate) auto_persist_suppressed: AtomicBool,
    /// Set by `migration_apply` once catalog.toml / loadouts.toml are
    /// written: the session is in the post-migration window where the live
    /// Profile is still pre-migration state and must not be persisted.
    /// Launch sets it too when it could not finish a wizard run that
    /// stopped partway, since the next launch writes the run's journal
    /// again over anything the session saved (see `app::launch::load`).
    /// Deliberately in-process (not a disk sniff): catalog.toml existing
    /// while `global_catalog` is None also describes a corrupt catalog
    /// falling back to legacy mode at startup, and that session must keep
    /// persisting normally.
    pub(crate) relaunch_pending: AtomicBool,
    /// Set when loadout mode is live: launch loaded catalog.toml, or the
    /// wizard wrote it this session. The catalog owns your aliases,
    /// triggers and macros and saves on its own, so `#profile save`,
    /// `load` and `reset` only echo.
    pub(crate) loadout_mode: AtomicBool,
    /// The app data folder, which holds every file Vosh keeps. Launch
    /// sets it once, see [`crate::app::launch::load`]. It stays unset
    /// only when launch could not resolve the folder, and then nothing
    /// loads from it.
    pub(crate) app_data: OnceLock<PathBuf>,
}

impl AppState {
    /// The session map, held for one step that takes no other lock.
    fn sessions(&self) -> std::sync::MutexGuard<'_, Sessions> {
        self.sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The selected session, which a command that names no session acts
    /// on. Take it before any other lock.
    pub(crate) fn selected_session(&self) -> Arc<Session> {
        self.sessions().selected()
    }

    /// The session a command acts on: the one `id` names, or the selected
    /// session when it names none. A session Vosh does not hold is an
    /// error, in a sentence. Take it before any other lock.
    pub(crate) fn session(&self, id: Option<SessionId>) -> Result<Arc<Session>, String> {
        let sessions = self.sessions();
        match id {
            None => Ok(sessions.selected()),
            Some(id) => sessions.get(id).ok_or_else(|| NO_SUCH_SESSION.to_string()),
        }
    }

    /// Add a session after the others, see [`Sessions::open`]. Take it
    /// before any other lock.
    pub(crate) fn open_session(&self) -> Arc<Session> {
        self.sessions().open()
    }

    /// Select the session `id` names. The commands that name no session
    /// act on it from then on, and its native grid shows. A session Vosh
    /// does not hold is an error, in a sentence, and the selection stays.
    pub(crate) fn select_session(&self, id: SessionId) -> Result<(), String> {
        if self.sessions().select(id) {
            Ok(())
        } else {
            Err(NO_SUCH_SESSION.to_string())
        }
    }

    /// Keep `notices` for the main window to show.
    pub(crate) fn add_launch_notices(&self, notices: Vec<String>) {
        if notices.is_empty() {
            return;
        }
        self.launch_notices
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend(notices);
    }

    /// Note which profile is active, for the events that name it.
    pub(crate) fn note_active_profile(&self, name: &str) {
        *self
            .active_profile
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(name.to_string());
    }

    /// The active profile's name, as [`AppState::note_active_profile`]
    /// last kept it.
    pub(crate) fn active_profile(&self) -> Option<String> {
        self.active_profile
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// The profile set, locked, once startup has loaded it. Before that,
    /// the error every profile command returns.
    pub(crate) async fn loaded_profile_set(
        &self,
    ) -> Result<MappedMutexGuard<'_, crate::profile::set::ProfileSet>, &'static str> {
        MutexGuard::try_map(self.profile_set.lock().await, Option::as_mut)
            .map_err(|_| PROFILES_NOT_LOADED)
    }

    /// Hand over the notices kept so far, once. A second call gets none.
    pub(crate) fn take_launch_notices(&self) -> Vec<String> {
        std::mem::take(
            &mut *self
                .launch_notices
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    /// Advance the panes generation. Call with the profile lock held, in
    /// the step that replaces the live panes.
    pub(crate) fn bump_panes_generation(&self) {
        self.panes_generation.fetch_add(1, Ordering::AcqRel);
    }

    /// Note that the live UI config was replaced wholesale, panes included.
    /// Advances the UI config and panes generations. Call with the profile
    /// lock held, in the step that swaps the config.
    pub(crate) fn note_ui_config_replaced(&self) {
        self.bump_panes_generation();
        self.ui_config_generation.fetch_add(1, Ordering::AcqRel);
    }

    /// Read the UI config generation. Call with the profile lock held.
    pub(crate) fn ui_config_generation(&self) -> u64 {
        self.ui_config_generation.load(Ordering::Acquire)
    }

    /// Read the panes generation. Call with the profile lock held.
    pub(crate) fn panes_generation(&self) -> u64 {
        self.panes_generation.load(Ordering::Acquire)
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            sessions: std::sync::Mutex::new(Sessions::default()),
            profile: Arc::new(Mutex::new(Profile::default())),
            logs: SharedLogStore::default(),
            log_reader: SharedLogStore::default(),
            plugins: SharedPluginManager::default(),
            profile_set: Arc::new(Mutex::new(None)),
            affect_full: crate::affects::full::AffectFull::default(),
            global_catalog: Arc::new(Mutex::new(None)),
            loadout_set: Arc::new(Mutex::new(None)),
            launch_notices: std::sync::Mutex::new(Vec::new()),
            active_profile: std::sync::Mutex::new(None),
            panes_generation: AtomicU64::new(0),
            ui_config_generation: AtomicU64::new(0),
            profile_dirty_gen: AtomicU64::new(0),
            auto_persist_suppressed: AtomicBool::new(false),
            relaunch_pending: AtomicBool::new(false),
            loadout_mode: AtomicBool::new(false),
            app_data: OnceLock::new(),
        }
    }
}

pub(crate) type SharedState = Arc<AppState>;

/// The error a profile command returns before startup has loaded the
/// profile set.
pub(crate) const PROFILES_NOT_LOADED: &str = "Vosh has not loaded your profiles yet.";

/// The error a command that needs the app data folder returns when launch
/// could not resolve it.
pub(crate) const NO_APP_DATA: &str = "Vosh could not find its data folder at launch.";

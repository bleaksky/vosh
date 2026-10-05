//! The state the whole app shares. Tauri holds one [`AppState`] for
//! every command, window and session.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use tokio::sync::{MappedMutexGuard, Mutex, MutexGuard};

use crate::app::plugins::SharedPluginManager;
use crate::logs::SharedLogStore;
use crate::profile::live::Profile;
use crate::profile::open::OpenProfile;
use crate::sessions::{Session, SessionId, SessionRow, Sessions, NO_SUCH_SESSION};

/// What every command, window and session shares. The sessions with the
/// profiles they play, the profile set, the log store, the plugins, the
/// catalog and loadouts of loadout mode, the file of the affect fulls and
/// the app data folder. The generation that turns away a pane layout
/// write read before the profile was replaced lives here too, with the
/// flag that holds the saves back until a relaunch and the flag that says
/// loadout mode is live.
pub(crate) struct AppState {
    /// The sessions, the one selected and the profiles they play. See
    /// [`crate::sessions`] for where its lock sits.
    sessions: std::sync::Mutex<Sessions>,
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
    /// The file each session's affect fulls are kept in, per character,
    /// for the Affects pane's gauges. See [`crate::affects::full`].
    pub(crate) affect_file: crate::affects::full::FullFile,
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
    /// Counts the times the live profile's panes have been replaced: a
    /// wholesale replace of the UI config, or a pane reset. It moves under
    /// the profile lock in the same step that swaps them, so a pane tree
    /// and the generation read with it always belong together. A pane
    /// layout write carries the generation of the tree it was edited from,
    /// and `pane_layout_set` refuses one from before a swap so it cannot
    /// land on the new profile.
    panes_generation: AtomicU64,
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

    /// Every session, in order.
    pub(crate) fn all_sessions(&self) -> Vec<Arc<Session>> {
        self.sessions().in_order().0
    }

    /// Every session but `id`. A step that works on the sessions of one
    /// profile takes them here before any other lock, then picks the ones
    /// that play it once it holds the profile, through
    /// [`ProfileGuard::players`](crate::profile::open::ProfileGuard::players).
    pub(crate) fn other_sessions(&self, id: SessionId) -> Vec<Arc<Session>> {
        self.sessions().others(id)
    }

    /// Every session's row, in order. Each row is read once the map lets
    /// go, so take it with no profile held.
    pub(crate) fn session_rows(&self) -> Vec<SessionRow> {
        let (list, selected) = self.sessions().in_order();
        list.iter()
            .map(|session| session.row(session.id == selected))
            .collect()
    }

    /// Add a session after the others that plays `profile`, see
    /// [`Sessions::open`]. Take it before any other lock.
    pub(crate) fn open_session(&self, profile: Arc<OpenProfile>) -> Arc<Session> {
        self.sessions().open(profile)
    }

    /// Take the session `id` out of the map, see [`Sessions::close`].
    /// A session Vosh does not hold, or the only one, is an error, in a
    /// sentence.
    pub(crate) fn close_session(&self, id: SessionId) -> Result<Arc<Session>, String> {
        self.sessions().close(id).map_err(str::to_string)
    }

    /// The open profile named `name`, while a session plays it.
    pub(crate) fn open_profile(&self, name: &str) -> Option<Arc<OpenProfile>> {
        self.sessions().profile(name)
    }

    /// Keep `profile`, named `name`, open for a session to play, see
    /// [`Sessions::add_profile`].
    pub(crate) fn add_open_profile(&self, name: &str, profile: Profile) -> Arc<OpenProfile> {
        self.sessions().add_profile(name, profile)
    }

    /// The profiles the sessions play, in the order they opened.
    pub(crate) fn open_profiles(&self) -> Vec<Arc<OpenProfile>> {
        self.sessions().profiles()
    }

    /// How many sessions play `open`. Every step that opens or closes a
    /// session or moves one to another profile holds
    /// [`PERSIST_LOCK`](crate::disk::save::PERSIST_LOCK), so under it the
    /// count stays as read.
    pub(crate) fn players(&self, open: &Arc<OpenProfile>) -> usize {
        self.sessions().players(open)
    }

    /// Close `open` when no session plays it, see
    /// [`Sessions::close_unplayed`].
    pub(crate) fn close_unplayed(&self, open: &Arc<OpenProfile>) -> bool {
        self.sessions().close_unplayed(open)
    }

    /// Whether `open` is one of the profiles the sessions play, see
    /// [`Sessions::is_open`].
    pub(crate) fn is_open(&self, open: &Arc<OpenProfile>) -> bool {
        self.sessions().is_open(open)
    }

    /// The profiles restored sessions wait on under the name `name`, see
    /// [`Sessions::waiting_on`].
    pub(crate) fn waiting_on(&self, name: &str) -> Vec<Arc<OpenProfile>> {
        self.sessions().waiting_on(name)
    }

    /// Put the sessions profiles.toml lists in place of the one the app
    /// starts with, see [`Sessions::restore`]. Launch calls it before it
    /// loads a profile.
    pub(crate) fn restore_sessions(
        &self,
        entries: &[crate::profile::set::SessionEntry],
        selected: Option<SessionId>,
    ) {
        self.sessions().restore(entries, selected);
    }

    /// Hold `set` as launch does once it read it, with the selected
    /// session on its active profile, for a test.
    #[cfg(test)]
    pub(crate) async fn set_profiles(&self, set: crate::profile::set::ProfileSet) {
        self.selected_profile().await.set_name(set.active_name());
        *self.profile_set.lock().await = Some(set);
    }

    /// The selected session's profile, locked, for a test.
    #[cfg(test)]
    pub(crate) async fn selected_profile(&self) -> crate::profile::open::ProfileGuard {
        self.selected_session().lock_profile().await
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

    /// The name of the profile the selected session plays, for the
    /// events that name it. None before any profile loads.
    pub(crate) fn active_profile(&self) -> Option<String> {
        self.selected_session().profile().name()
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

    /// Read the panes generation. Call with the profile lock held.
    pub(crate) fn panes_generation(&self) -> u64 {
        self.panes_generation.load(Ordering::Acquire)
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            sessions: std::sync::Mutex::new(Sessions::default()),
            logs: SharedLogStore::default(),
            log_reader: SharedLogStore::default(),
            plugins: SharedPluginManager::default(),
            profile_set: Arc::new(Mutex::new(None)),
            affect_file: crate::affects::full::FullFile::default(),
            global_catalog: Arc::new(Mutex::new(None)),
            loadout_set: Arc::new(Mutex::new(None)),
            launch_notices: std::sync::Mutex::new(Vec::new()),
            panes_generation: AtomicU64::new(0),
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

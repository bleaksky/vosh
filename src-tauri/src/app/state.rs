//! The state the whole app shares. Tauri holds one [`AppState`] for
//! every command, window and session.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use tokio::sync::{MappedMutexGuard, Mutex, MutexGuard};

use crate::app::plugins::SharedPluginManager;
use crate::logs::SharedLogStore;
use crate::profile::live::Profile;
use crate::profile::open::{OpenProfile, ProfileGuard};
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
    /// Counts the log searches the view started. A search reads on while
    /// the count is its own, so the next keystroke's search stops it.
    pub(crate) log_searches: AtomicU64,
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
    /// Counts the times the panes in front have been replaced: a
    /// wholesale replace of the UI config, a pane reset, or a selection
    /// that brought another profile to the front. It moves in the same
    /// step that swaps them, under the profile lock or with the selection
    /// under the session map, so a pane tree and the generation read with
    /// it always belong together. A pane layout write carries the
    /// generation of the tree it was edited from, and `pane_layout_set`
    /// refuses one from before a swap so it cannot land on the new
    /// profile.
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
    /// The windows of Vosh that have focus, which the focus rule of the
    /// alerts reads. See [`crate::alert::focus`].
    pub(crate) focus: crate::alert::focus::Focus,
    /// Where alert banners go, the system's, or in a test build a list
    /// the test reads. See [`crate::alert::banner`].
    pub(crate) banners: crate::alert::banner::Banners,
    /// In a test build, the clock the redial waits on while a test holds
    /// one: each wait goes to the test, which ends it.
    #[cfg(test)]
    pub(crate) redial_clock: std::sync::Mutex<Option<RedialClock>>,
    /// In a test build, the gate a redial that connected waits at before
    /// it takes the slot, while a test holds one. See
    /// [`crate::session::reconnect::hold_try`].
    #[cfg(test)]
    pub(crate) redial_gate: std::sync::Mutex<Option<RedialGate>>,
}

/// Where a redial in a test build sends each wait: the session, how long
/// it would wait, and the sender the test ends the wait with.
#[cfg(test)]
pub(crate) type RedialClock = tokio::sync::mpsc::UnboundedSender<(
    SessionId,
    std::time::Duration,
    tokio::sync::oneshot::Sender<()>,
)>;

/// Where a redial in a test build that connected sends the sender that
/// lets it go on.
#[cfg(test)]
pub(crate) type RedialGate = std::sync::mpsc::Sender<std::sync::mpsc::Sender<()>>;

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

    /// Move the session `id` to the place `to` in the list, see
    /// [`Sessions::move_to`]. A session Vosh does not hold is an error, in
    /// a sentence.
    pub(crate) fn move_session(&self, id: SessionId, to: usize) -> Result<(), String> {
        self.sessions().move_to(id, to).map_err(str::to_string)
    }

    /// Take the session `id` out of the map, see [`Sessions::close`].
    /// A session Vosh does not hold, or the only one, is an error, in a
    /// sentence. A close that hands the selection to a session on another
    /// profile moves the panes generation, as a selection does.
    pub(crate) fn close_session(&self, id: SessionId) -> Result<Arc<Session>, String> {
        let mut sessions = self.sessions();
        let front = sessions.selected().profile();
        let closed = sessions.close(id).map_err(str::to_string)?;
        self.follow_front(&sessions, &front);
        Ok(closed)
    }

    /// The open profile named `name`, while a session plays it or
    /// Settings holds unsaved edits on it.
    pub(crate) fn open_profile(&self, name: &str) -> Option<Arc<OpenProfile>> {
        self.sessions().profile(name)
    }

    /// The profile a Settings command edits: the open profile `profile`
    /// names, or the selected session's when it names none. It finds the
    /// profile in the session map, so take it before any other lock. A
    /// command that takes the loadouts before the profile finds it here
    /// first.
    ///
    /// A name no open profile has is an error, in a sentence. Settings
    /// names only a profile it showed, and a profile it holds unsaved
    /// edits on stays open after its last session leaves, see
    /// [`AppState::hold_edits`], so the profile closed before the page
    /// held it. The sentence says the change did not save.
    pub(crate) fn edited_profile(&self, profile: Option<String>) -> Result<EditedProfile, String> {
        match profile {
            None => Ok(EditedProfile::Selected(self.selected_session())),
            Some(name) => self
                .open_profile(&name)
                .map(EditedProfile::Named)
                .ok_or_else(|| profile_closed(&name)),
        }
    }

    /// The profile a Settings command edits, see
    /// [`AppState::edited_profile`], locked. Take it before any other
    /// lock.
    pub(crate) async fn lock_named(&self, profile: Option<String>) -> Result<ProfileGuard, String> {
        Ok(self.edited_profile(profile)?.lock().await)
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

    /// Hold the open profile `name` names for the unsaved edits Settings
    /// keeps on it, or let go with None, see [`Sessions::hold_edits`].
    pub(crate) fn hold_edits(&self, name: Option<&str>) -> Option<Arc<OpenProfile>> {
        self.sessions().hold_edits(name)
    }

    /// The profile Settings holds unsaved edits on.
    pub(crate) fn edit_hold(&self) -> Option<Arc<OpenProfile>> {
        self.sessions().edit_hold()
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
    pub(crate) async fn selected_profile(&self) -> ProfileGuard {
        self.selected_session().lock_profile().await
    }

    /// Select the session `id` names. The commands that name no session
    /// act on it from then on, and its native grid shows. A session Vosh
    /// does not hold is an error, in a sentence, and the selection stays.
    /// A selection that brings another profile to the front moves the
    /// panes generation, see [`AppState::follow_front`].
    pub(crate) fn select_session(&self, id: SessionId) -> Result<(), String> {
        let mut sessions = self.sessions();
        let front = sessions.selected().profile();
        if !sessions.select(id) {
            return Err(NO_SUCH_SESSION.to_string());
        }
        self.follow_front(&sessions, &front);
        Ok(())
    }

    /// Move the panes generation when the selected session in `sessions`
    /// plays another profile than `front`, the one in front before. Every
    /// window then takes that profile's panes, as after a switch, so a
    /// pane layout write edited from the tree of the profile that showed
    /// is refused. Call it with the session map held, in the step that
    /// moved the selection, so a write that finds the new selection reads
    /// the new generation.
    fn follow_front(&self, sessions: &Sessions, front: &Arc<OpenProfile>) {
        if !Arc::ptr_eq(&sessions.selected().profile(), front) {
            self.bump_panes_generation();
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

    /// Whether `open` is the profile in front, the one the selected
    /// session plays, which every window shows. An event that carries
    /// one profile's settings or lists goes out only while that profile
    /// is in front, so a change in a session on it counts whichever
    /// session that is. A profile behind loses nothing, since a selection
    /// that brings it to the front sends its settings, see
    /// [`crate::app::launch::show_selection`]. Take it with no profile
    /// held.
    pub(crate) fn in_front(&self, open: &Arc<OpenProfile>) -> bool {
        Arc::ptr_eq(&self.selected_session().profile(), open)
    }

    /// The name of the profile the selected session plays, for a test.
    /// None before any profile loads.
    #[cfg(test)]
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

    /// Advance the panes generation. Call it in the step that replaces
    /// the panes in front, with the profile lock held, or with the session
    /// map held as a selection moves, see [`AppState::follow_front`].
    pub(crate) fn bump_panes_generation(&self) {
        self.panes_generation.fetch_add(1, Ordering::AcqRel);
    }

    /// Read the panes generation. Call with the profile lock held.
    pub(crate) fn panes_generation(&self) -> u64 {
        self.panes_generation.load(Ordering::Acquire)
    }
}

/// What a Settings command says when the profile it names closed before
/// it could save, see [`AppState::edited_profile`].
fn profile_closed(name: &str) -> String {
    format!(
        "{} closed before Vosh could save this change.",
        crate::profile::set::display_name(name)
    )
}

/// The profile a Settings command edits, as the session map found it
/// before the command took any other lock, see
/// [`AppState::edited_profile`].
pub(crate) enum EditedProfile {
    /// The selected session's. The lock reads which profile the session
    /// plays, since a switch may move it first.
    Selected(Arc<Session>),
    /// The open profile the command named.
    Named(Arc<OpenProfile>),
}

impl EditedProfile {
    /// Lock the profile.
    pub(crate) async fn lock(&self) -> ProfileGuard {
        match self {
            Self::Selected(session) => session.lock_profile().await,
            Self::Named(open) => open.lock().await,
        }
    }

    /// The profile's name in the profile set, None before launch loads
    /// one.
    pub(crate) fn name(&self) -> Option<String> {
        match self {
            Self::Selected(session) => session.profile().name(),
            Self::Named(open) => open.name(),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            sessions: std::sync::Mutex::new(Sessions::default()),
            logs: SharedLogStore::default(),
            log_reader: SharedLogStore::default(),
            log_searches: AtomicU64::new(0),
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
            focus: crate::alert::focus::Focus::default(),
            banners: crate::alert::banner::Banners::default(),
            #[cfg(test)]
            redial_clock: std::sync::Mutex::new(None),
            #[cfg(test)]
            redial_gate: std::sync::Mutex::new(None),
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

#[cfg(test)]
mod tests {
    use crate::profile::set::DEFAULT_PROFILE_NAME;

    #[test]
    fn a_profile_that_closed_reads_by_the_name_settings_shows() {
        let state = super::AppState::default();
        let closed = |name: &str| state.edited_profile(Some(name.into())).err();
        assert_eq!(
            closed(DEFAULT_PROFILE_NAME).as_deref(),
            Some("Default closed before Vosh could save this change.")
        );
        assert_eq!(
            closed("Build").as_deref(),
            Some("Build closed before Vosh could save this change.")
        );
    }
}

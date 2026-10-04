//! The state the whole app shares. Tauri holds one [`AppState`] for
//! every command, window and session.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use tokio::sync::{MappedMutexGuard, Mutex, MutexGuard};

use crate::app::plugins::SharedPluginManager;
use crate::logs::{SharedLogStore, SharedScrollback};
use crate::profile::live::Profile;
use crate::script::SharedTimers;
use crate::session::connection::Connection;
use crate::session::SessionHandle;

/// What every command, window and session shares. The one session slot,
/// the live profile, what the connection shares with the commands, the
/// profile set, the log store and scrollback, the plugins and Lua timers,
/// the catalog and loadouts of loadout mode, the app data folder, and what
/// the app keeps about the live connection: its host and port, the
/// character logged in, the terminal size and the last affects. The
/// generations that turn away a write read before the profile was replaced
/// live here too, with the counter that settles a burst of changes into
/// one save, the flags that hold the saves back and the flag that says
/// loadout mode is live.
pub(crate) struct AppState {
    pub(crate) session: Mutex<Option<SessionHandle>>,
    pub(crate) profile: Arc<Mutex<Profile>>,
    /// Your target and the room list, which belong to the connection. The
    /// session loop holds a handle to them. See [`Connection`] for where
    /// its lock sits.
    pub(crate) connection: Arc<Mutex<Connection>>,
    pub(crate) lua_timers: SharedTimers,
    pub(crate) logs: SharedLogStore,
    /// A second connection to the same log database for the read
    /// commands. The session loop appends through `logs`, and a search
    /// can read the whole log, so reads take their own lock and never
    /// hold up the live session. WAL lets both run at once.
    pub(crate) log_reader: SharedLogStore,
    pub(crate) scrollback: SharedScrollback,
    pub(crate) plugins: SharedPluginManager,
    /// Catalog of named profiles. Loaded (or migrated from the legacy
    /// single-file layout) once at startup; commands mutate it under
    /// this mutex.
    pub(crate) profile_set: Arc<Mutex<Option<crate::profile::set::ProfileSet>>>,
    /// Last terminal size reported by the frontend, kept across the
    /// no-session window so a fresh `session_connect` can seed the
    /// telnet `Negotiator` with the real (cols, rows) instead of the
    /// 80×24 default. Without this cache the server's first NAWS
    /// reply carried 80 cols and wrapped early output (login banner,
    /// `who`, motd) until the user nudged the window. Stored under a
    /// std Mutex because the critical section is two integer copies
    /// — async overhead is not worth it.
    pub(crate) window_size: std::sync::Mutex<(u16, u16)>,
    /// Live connection target (host, port). Set when
    /// `session_connect` succeeds, cleared on disconnect. Read by the
    /// Char.Status-driven auto-switch path so the resolver knows
    /// which connection's profile to pick. Stored under a std mutex
    /// because the work inside the lock is just a clone.
    pub(crate) current_connection: std::sync::Mutex<Option<(String, u16)>>,
    /// Last character name observed via Char.Status or Char.Name on
    /// the current session. Cleared on disconnect. Used to suppress
    /// duplicate resolver calls when the MUD re-sends Char.Status on
    /// every vitals update.
    pub(crate) current_character: std::sync::Mutex<Option<String>>,
    /// The last Char.Affects list of this connection, for a window that
    /// opens between ticks. Cleared on connect and when the session
    /// ends.
    pub(crate) last_affects: crate::affects::snapshot::AffectsSnapshot,
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
    /// The terminal lines the plugins printed as they loaded at launch,
    /// their `[lua]` errors and stops among them. Launch runs before any
    /// window listens, so they wait for the first connect or the first
    /// line you type, see [`crate::app::plugins::show_launch_lines`].
    pub(crate) launch_lua_lines: std::sync::Mutex<Vec<String>>,
    /// The active profile's name, kept beside the profile set so an event
    /// can name it without waiting for that lock. Set at launch, on a
    /// switch and on a rename. None before any profile loads.
    pub(crate) active_profile: std::sync::Mutex<Option<String>>,
    /// The prompt card watches your prompt, so `session://prompt-state`
    /// follows each prompt Vosh reads.
    pub(crate) prompt_watch: std::sync::atomic::AtomicBool,
    /// You are selecting text in xterm or reading back in its split, as
    /// the webview last said. A clock repaint of your prompt waits while
    /// it holds, so the row you select or read never moves.
    pub(crate) reader_busy: std::sync::atomic::AtomicBool,
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
            session: Mutex::new(None),
            profile: Arc::new(Mutex::new(Profile::default())),
            connection: Arc::new(Mutex::new(Connection::default())),
            lua_timers: SharedTimers::default(),
            logs: SharedLogStore::default(),
            log_reader: SharedLogStore::default(),
            scrollback: SharedScrollback::default(),
            plugins: SharedPluginManager::default(),
            profile_set: Arc::new(Mutex::new(None)),
            // Default matches `Negotiator::default()` so any code path
            // that bypasses `session_set_window_size` (e.g. an early
            // automated connect from a script) still gets a sensible
            // baseline.
            window_size: std::sync::Mutex::new((80, 24)),
            current_connection: std::sync::Mutex::new(None),
            current_character: std::sync::Mutex::new(None),
            last_affects: crate::affects::snapshot::AffectsSnapshot::default(),
            affect_full: crate::affects::full::AffectFull::default(),
            global_catalog: Arc::new(Mutex::new(None)),
            loadout_set: Arc::new(Mutex::new(None)),
            launch_notices: std::sync::Mutex::new(Vec::new()),
            launch_lua_lines: std::sync::Mutex::new(Vec::new()),
            active_profile: std::sync::Mutex::new(None),
            prompt_watch: std::sync::atomic::AtomicBool::new(false),
            reader_busy: std::sync::atomic::AtomicBool::new(false),
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

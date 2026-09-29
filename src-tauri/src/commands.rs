//! Tauri commands invoked by the frontend.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::Mutex;
use tracing::warn;
use vosh_log::{SearchHit, SearchOptions, SearchPage, SessionRow};
use vosh_trigger::Trigger;

use crate::input;
use crate::list_events::{broadcast_list_changes, ListChanges, ListRevisions};
use crate::log_state::{SharedLogStore, SharedScrollback};

/// Send an event to every webview window. `AppHandle::emit` routes via
/// the global listener pool, which has been observed to skip late-attached
/// listeners in sibling webviews (the main window misses settings-window
/// updates). Iterating the live window map and emitting to each one
/// guarantees delivery to both the main and settings webviews.
/// Debounce generation for `mark_profile_dirty`: each mark bumps it, and
/// the delayed persist only fires if no newer mark arrived while waiting.
static PROFILE_DIRTY_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Counts the times the live profile's UI config has been replaced
/// wholesale (a profile switch). It moves under the profile lock in the
/// same step that swaps the config, so a pane tree and the generation
/// read with it always belong together. A pane layout write carries the
/// generation of the tree it was edited from, and `pane_layout_set`
/// refuses one from before a swap so it cannot land on the new profile.
static PANES_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Advance [`PANES_GENERATION`]. Call with the profile lock held, in the
/// step that replaces the live UI config.
pub(crate) fn bump_panes_generation() {
    PANES_GENERATION.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
}

/// Read [`PANES_GENERATION`]. Call with the profile lock held.
pub(crate) fn panes_generation() -> u64 {
    PANES_GENERATION.load(std::sync::atomic::Ordering::Acquire)
}

/// Set by `#profile reset` / `#profile load`: the in-memory profile is
/// deliberately diverged from disk, so the passive flushes (debounce,
/// exit) must not write it. Cleared by the next durable change.
pub(crate) static AUTO_PERSIST_SUPPRESSED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
/// Set by `migration_apply` once catalog.toml / loadouts.toml are
/// written: the session is in the post-migration window where the live
/// Profile is still pre-migration state and must not be persisted.
/// Deliberately in-process (not a disk sniff): catalog.toml existing
/// while `state.global_catalog` is None also describes a corrupt
/// catalog falling back to legacy mode at startup, and that session
/// must keep persisting normally.
pub(crate) static MIGRATION_RELAUNCH_PENDING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Serializes every write of a profile file: the active profile's
/// persist, Settings edits to an inactive profile's file, a profile
/// switch from its flush through loading the next file, and the
/// rename, delete and copy of a profile file. `write_with_backup` uses
/// a fixed .tmp name per target, so two writers on one file would break
/// its atomic write, and a write racing a rename or a switch would land
/// on a file the other side already moved or read. Take it before the
/// profile set lock, never while holding it.
pub(crate) static PERSIST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Record that durable profile state changed (slash commands, Lua
/// mutations) and persist shortly after the burst settles. Keeps disk
/// writes off latency-sensitive paths while guaranteeing the change
/// reaches profile.toml/catalog.toml within a couple of seconds; the
/// exit hook flushes immediately as a backstop.
pub(crate) fn mark_profile_dirty(app: &AppHandle) {
    AUTO_PERSIST_SUPPRESSED.store(false, std::sync::atomic::Ordering::Release);
    schedule_profile_persist(app);
}

/// Persist shortly after the burst settles, like `mark_profile_dirty`,
/// but without counting as consent to save a profile that `#profile
/// reset` or `#profile load` left diverged from disk. While that holds,
/// the write is skipped and the change waits in memory for the next
/// durable change or an explicit `#profile save`. For incidental edits
/// such as a pane layout drag.
pub(crate) fn schedule_profile_persist(app: &AppHandle) {
    use std::sync::atomic::Ordering;
    let gen = PROFILE_DIRTY_GEN.fetch_add(1, Ordering::AcqRel) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if PROFILE_DIRTY_GEN.load(Ordering::Acquire) != gen {
            return; // a newer mark restarted the clock
        }
        if AUTO_PERSIST_SUPPRESSED.load(Ordering::Acquire) {
            return; // a #profile reset/load intervened
        }
        let shared: SharedState = app.state::<SharedState>().inner().clone();
        persist_profile(&app, &shared).await;
    });
}

/// Act on what a run of input lines asked of the saved profile. Every
/// path that runs lines through the input pipeline calls this after it
/// releases the profile lock, so they all save alike.
///
/// Slash commands (#alias, #trigger, #var, #endrec, #import-tintin,
/// ...) and durable Lua actions mutate the profile but historically
/// never persisted, so anything authored this way vanished on restart
/// unless an unrelated persisting command happened to run later.
/// `#profile reset` and `#profile load` are deliberate exceptions:
/// reset blanks the LIVE profile only (the help documents `#profile
/// save` as the explicit write and `load` as the undo), so
/// auto-persisting it would wipe the on-disk profile, and in Path B the
/// shared catalog. They also suppress the passive flushes (exit,
/// debounce) until the next durable change says the in-memory state is
/// wanted again. The pipeline itself says when one replaced the profile
/// (see [`crate::input::run_line`]), so a `#profile load` whose file does
/// not read suppresses nothing, and spelling variants ("#profile  reset",
/// "# profile load") cannot slip past into the dirty mark and persist the
/// just-blanked profile.
pub(crate) async fn settle_line_effects(app: &AppHandle, effects: crate::input::LineEffects) {
    if effects.replaced {
        AUTO_PERSIST_SUPPRESSED.store(true, std::sync::atomic::Ordering::Release);
        // Every window drops its copy of the old panes and tracked
        // affects, so a later panel edit cannot write them back.
        let shared: SharedState = app.state::<SharedState>().inner().clone();
        broadcast_profile_ui(app, &shared).await;
    }
    // A durable change after the replace counts as wanting the live
    // state saved, the way a line typed after `#profile reset` does.
    if effects.dirty {
        mark_profile_dirty(app);
    }
}

pub(crate) fn broadcast<S: serde::Serialize + Clone>(app: &AppHandle, event: &str, payload: &S) {
    for win in app.webview_windows().values() {
        if let Err(e) = win.emit(event, payload.clone()) {
            warn!(error = %e, window = %win.label(), event, "broadcast failed");
        }
    }
}
use crate::map_state::SharedMap;
use crate::plugins::{PluginRecord, SharedPluginManager};
use crate::profile::{Macro, Profile, Timer};
use crate::profile_config::{
    hand_out_shared, share_custom_themes, strip_global_fields, DockEntryPersist, GlobalConfig,
    HeldCustomThemes, PaneLayoutPersist, ProfileConfig, SharedLayer,
};
use crate::script_state;
use crate::script_state::SharedTimers;
use crate::session::{self, OutputPayload, SessionHandle, TargetPayload};

/// Application-wide state. Phase 1 carries a single optional session and one
/// profile. Phase 5 widens this to a session map; Phase 9 widens to multiple
/// profiles.
pub(crate) struct AppState {
    pub(crate) session: Mutex<Option<SessionHandle>>,
    pub(crate) profile: Arc<Mutex<Profile>>,
    pub(crate) map: SharedMap,
    pub(crate) script_timers: SharedTimers,
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
    pub(crate) profile_set: Arc<Mutex<Option<crate::profile_set::ProfileSet>>>,
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
    pub(crate) last_affects: crate::affects_snapshot::AffectsSnapshot,
    /// Path B authoring catalog. `Some` when the app started up with
    /// `catalog.toml` present (Path B mode); `None` in legacy per-
    /// profile mode. Mutated alongside the live `Profile` so on-disk
    /// state stays in step with in-memory edits.
    pub(crate) global_catalog: Arc<Mutex<Option<crate::loadout::GlobalCatalog>>>,
    /// Path B loadout collection. Same `Some`/`None` semantics as
    /// `global_catalog`. The active subset drives which catalog groups
    /// the runtime gates on (see [`crate::loadout_store::apply_loadout_state`]).
    pub(crate) loadout_set: Arc<Mutex<Option<crate::loadout::LoadoutSet>>>,
    /// Sentences launch has to tell you, such as a profile file Vosh
    /// could not read and will not save over. Kept until the main window
    /// takes them through `launch_notices_take`, since launch runs before
    /// any window listens.
    pub(crate) launch_notices: std::sync::Mutex<Vec<String>>,
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

    /// Hand over the notices kept so far, once. A second call gets none.
    pub(crate) fn take_launch_notices(&self) -> Vec<String> {
        std::mem::take(
            &mut *self
                .launch_notices
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
}

/// What launch has to tell you, for the main window to show once in the
/// terminal and as a toast.
#[tauri::command]
pub(crate) fn launch_notices_take(state: State<'_, SharedState>) -> Vec<String> {
    state.take_launch_notices()
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            session: Mutex::new(None),
            profile: Arc::new(Mutex::new(Profile::default())),
            map: SharedMap::default(),
            script_timers: SharedTimers::default(),
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
            last_affects: crate::affects_snapshot::AffectsSnapshot::default(),
            global_catalog: Arc::new(Mutex::new(None)),
            loadout_set: Arc::new(Mutex::new(None)),
            launch_notices: std::sync::Mutex::new(Vec::new()),
        }
    }
}

pub(crate) type SharedState = Arc<AppState>;

/// The error a profile command returns before startup has loaded the
/// profile set.
pub(crate) const PROFILES_NOT_LOADED: &str = "Vosh has not loaded your profiles yet.";

/// Snapshot the live profile and write it to the active profile's file
/// under `<app_data_dir>/profiles/<active>.toml`. Failures are logged
/// but not surfaced — callers don't want a UI toggle to fail because
/// the disk is full mid-flight, and the in-memory state is still
/// correct for the rest of the session.
pub(crate) async fn persist_profile(app: &AppHandle, state: &SharedState) {
    // Serialize whole-persist runs. The debounced dirty-persist and the
    // exit-time flush can overlap each other or an inline command
    // persist, and Settings can write an inactive profile's file.
    let _persist_guard = PERSIST_LOCK.lock().await;
    persist_profile_locked(app, state).await;
}

/// The body of [`persist_profile`]. Call with [`PERSIST_LOCK`] held.
async fn persist_profile_locked(app: &AppHandle, state: &SharedState) {
    let app_data = app.path().app_data_dir().ok();
    persist_state(state, app_data.as_deref()).await;
}

/// [`persist_profile_locked`] over the app data folder `app_data`, so a
/// test can run it over a folder of its own. Call with [`PERSIST_LOCK`]
/// held. A file Vosh could not read at launch is never written, see
/// [`crate::profile_config::hold_unread`].
pub(crate) async fn persist_state(state: &SharedState, app_data: Option<&std::path::Path>) {
    // Path B branch. When `state.global_catalog` is `Some`, the user is
    // post-migration: authored items live in catalog.toml and the live
    // Profile is the cache. Write the live aliases / triggers / macros
    // back to the catalog plus the loadout set, and every other setting
    // to the active profile file; the per-profile branch below is
    // skipped entirely.
    if state.global_catalog.lock().await.is_some() {
        if let Some(dir) = app_data {
            persist_path_b(state, dir).await;
        }
        return;
    }

    // Post-migration window: migration_apply has written catalog.toml
    // but `state.global_catalog` only loads at the next launch. Running
    // the legacy branch here would write the live pre-migration items
    // back into the profile file the migration just took them out of —
    // with pre-retag group names that would overlay and corrupt the
    // catalog on relaunch. Persist nothing until the restart completes
    // the migration.
    if MIGRATION_RELAUNCH_PENDING.load(std::sync::atomic::Ordering::Acquire) {
        tracing::debug!("persist skipped: post-migration window before relaunch");
        return;
    }

    // Resolve the active profile's path + global path via ProfileSet
    // if it's been loaded; fall back to the legacy single-file path
    // if ProfileSet is somehow missing (shouldn't happen post-
    // startup; defensive path for very early calls before setup()
    // finishes).
    let (per_profile_path, global_path, scope) = {
        let guard = state.profile_set.lock().await;
        if let Some(set) = guard.as_ref() {
            (
                Some(set.active_path()),
                Some(set.global_path()),
                Some(*set.scope()),
            )
        } else {
            let Some(dir) = app_data else {
                return;
            };
            (Some(dir.join("profile.toml")), None, None)
        }
    };

    let (mut per_profile_snapshot, global_snapshot) = {
        let p = state.profile.lock().await;
        let scope = scope.unwrap_or_default();
        (
            ProfileConfig::from_profile(&p),
            GlobalConfig::from_profile(&p, &scope),
        )
    };

    // Strip the global-scoped fields out of the per-profile snapshot
    // so they don't get duplicated. Honors the per-category scope
    // map (categories marked Profile-scoped stay in the per-profile
    // file).
    if let Some(scope) = scope.as_ref() {
        strip_global_fields(&mut per_profile_snapshot, scope);
    }

    if let Some(p) = per_profile_path.as_ref() {
        if let Err(e) = per_profile_snapshot.save(p) {
            warn!(error = %e, path = %p.display(), "auto-save per-profile failed");
        }
    }
    if let Some(g) = global_path.as_ref() {
        if let Err(e) = global_snapshot.save(g) {
            warn!(error = %e, path = %g.display(), "auto-save global failed");
        }
    }
}

/// Path B persistence. Snapshots the live `Profile`'s authored items
/// into `catalog.toml` and the in-memory `LoadoutSet` into
/// `loadouts.toml`, both via the same atomic-write-with-backup
/// pipeline the per-profile branch uses. Falls through to the legacy
/// `global.toml` write so theme / font / `dock_layout` edits land on
/// the same path in both modes.
async fn persist_path_b(state: &SharedState, dir: &std::path::Path) {
    // A catalog that has not taken the enabled presets yet waits for a
    // launch that reads a profile file (see
    // `loadout_store::adopt_catalog_presets`), so a save leaves the list
    // out rather than write the live profile's list alone.
    let presets_waiting = state
        .global_catalog
        .lock()
        .await
        .as_ref()
        .is_some_and(|c| c.enabled_presets.is_none());
    // Catalog. Pull aliases / triggers / macros directly from the live
    // Profile. The catalog is the authoritative source in Path B mode
    // so an overwrite here is correct — anything the user typed via
    // #alias / Settings made it into Profile and now into the file.
    let (catalog, global_snapshot, scope) = {
        let p = state.profile.lock().await;
        // The enabled presets ride along, since the preset triggers they
        // name live in the catalog too.
        let mut catalog = crate::loadout::GlobalCatalog::from_profile(&p);
        if presets_waiting {
            catalog.enabled_presets = None;
        }
        let scope = state
            .profile_set
            .lock()
            .await
            .as_ref()
            .map(|s| *s.scope())
            .unwrap_or_default();
        let global = GlobalConfig::from_profile(&p, &scope);
        (catalog, global, scope)
    };
    // `scope` is consumed below when stripping global-scoped fields out
    // of the per-profile snapshot. Holding the binding here so the
    // catalog/loadout writes can run without holding the profile-set
    // lock alongside them.

    // Snapshot the current LoadoutSet from state. We do not mutate the
    // active list here; that gets driven by future loadout_set_active
    // commands. Just persist whatever the runtime currently holds.
    let set_snapshot = state.loadout_set.lock().await.clone();

    if let Err(e) = crate::loadout_store::save_global_catalog(dir, &catalog) {
        warn!(error = %e, "Path B catalog auto-save failed");
    }
    if let Some(set) = set_snapshot {
        if let Err(e) = crate::loadout_store::save_loadout_set(dir, &set) {
            warn!(error = %e, "Path B loadout set auto-save failed");
        }
    }

    let (global_path, per_profile_path) = {
        let guard = state.profile_set.lock().await;
        match guard.as_ref() {
            Some(set) => (Some(set.global_path()), Some(set.active_path())),
            None => (None, None),
        }
    };
    if let Some(g) = global_path {
        if let Err(e) = global_snapshot.save(&g) {
            warn!(error = %e, path = %g.display(), "Path B global auto-save failed");
        }
    }
    // Per-profile snapshot. Before this, Path B only wrote catalog /
    // loadouts / global, which meant every UI field outside the five
    // scope-controlled ones (tracked_affects, theme_terminal_colors,
    // vitals config, custom themes, paste pacing, moons position,
    // chip style, side-panels fill, split-divider color, dock layout
    // when its scope is profile, ...) silently dropped on every quit.
    // Writing a per-profile file lets these persist per-loadout the
    // same way legacy mode does. The catalog stays authoritative for
    // aliases / triggers / macros, so we blank those out of the
    // per-profile snapshot before saving — otherwise a launch as an
    // older profile would lay its stale copy over the catalog, bringing
    // back a deleted item or overriding fresher catalog edits. The
    // catalog owns the enabled presets too. The file keeps a copy of the
    // shared list, which the next load replaces with the catalog's.
    if let Some(p) = per_profile_path {
        let mut per_profile_snapshot = {
            let live = state.profile.lock().await;
            ProfileConfig::from_profile(&live)
        };
        per_profile_snapshot.clear_catalog_items();
        // The disabled-group lists STAY: they are where the Settings
        // group checkboxes persist in Path B mode. Clearing them here
        // (as this used to) meant group toggles could not survive a
        // restart at all — the catalog has no field for them and the
        // startup rebuild recomputed them from loadout enabled_groups.
        // Known limit: when an active loadout DOES declare
        // enabled_groups, apply_loadout_state stays authoritative and
        // overwrites these lists on the next loadout change. A separate
        // durable-checkbox field is the follow-up fix for that cohort.
        strip_global_fields(&mut per_profile_snapshot, &scope);
        if let Err(e) = per_profile_snapshot.save(&p) {
            warn!(
                error = %e,
                path = %p.display(),
                "Path B per-profile auto-save failed",
            );
        }
    }
    // Mirror the new catalog into `state.global_catalog` so subsequent
    // reads see the latest write without going back to disk.
    *state.global_catalog.lock().await = Some(catalog);
}

#[tauri::command]
pub(crate) async fn session_connect(
    app: AppHandle,
    state: State<'_, SharedState>,
    host: String,
    port: u16,
    tls: bool,
) -> Result<(), String> {
    // Take any existing handle out under a brief lock and drop the lock
    // before doing the long-running connect. This lets `session_disconnect`
    // run concurrently to cancel a hung connect attempt.
    let old = {
        let mut current = state.session.lock().await;
        current.take()
    };
    if let Some(handle) = old {
        handle.shutdown().await;
    }

    // Clear session-scoped variables on reconnect; profile-scoped survive.
    state.profile.lock().await.vars.clear_session();

    // Remember the live connection target so the Char.Status-driven
    // auto-switch path can re-resolve against it once the MUD tells us
    // who we logged in as. Cleared in `session_disconnect`. Reset the
    // last-known character at the same time so a reconnect to a
    // different account triggers a fresh resolve.
    if let Ok(mut g) = state.current_connection.lock() {
        *g = Some((host.clone(), port));
    }
    if let Ok(mut g) = state.current_character.lock() {
        *g = None;
    }
    // The old session cleared the list as it ended. A new connection
    // starts with none until the MUD sends its own.
    state.last_affects.clear();

    let scrollback_path = tauri::Manager::path(&app)
        .app_data_dir()
        .ok()
        .map(|dir| crate::log_state::scrollback_path(&dir));

    // Seed the negotiator with the most recently reported terminal
    // size so the initial `DO NAWS` reply during the handshake
    // carries the correct cols/rows. The default of (80, 24) is
    // applied only when the frontend never called
    // `session_set_window_size` before this connect.
    let initial_size = state.window_size.lock().map_or((80, 24), |g| *g);
    let target = (host.clone(), port);

    let spawned = session::spawn(
        app.clone(),
        host,
        port,
        tls,
        state.profile.clone(),
        state.map.clone(),
        state.script_timers.clone(),
        state.logs.clone(),
        state.scrollback.clone(),
        scrollback_path,
        initial_size,
    )
    .await;
    let handle = match spawned {
        Ok(handle) => handle,
        Err(e) => {
            // Surface the disconnected state so the UI does not stay stuck
            // on "connecting...". The frontend listens for session://state.
            let _ = app.emit(
                "session://state",
                crate::session::StatePayload::Disconnected {
                    reason: Some(e.to_string()),
                },
            );
            // Nothing reached the target, so nobody is logged in there.
            // A connect that raced this one keeps its own target.
            if let Ok(mut g) = state.current_connection.lock() {
                if g.as_ref() == Some(&target) {
                    *g = None;
                }
            }
            crate::characters::broadcast_session_identity(&app, state.inner()).await;
            return Err(e.to_string());
        }
    };

    {
        let mut current = state.session.lock().await;
        if let Some(prev) = current.take() {
            // A concurrent connect raced us. Shut down our old handle.
            prev.shutdown().await;
        }
        *current = Some(handle);
    }
    crate::characters::broadcast_session_identity(&app, state.inner()).await;
    Ok(())
}

#[tauri::command]
pub(crate) async fn session_send(
    state: State<'_, SharedState>,
    bytes: Vec<u8>,
) -> Result<(), String> {
    let current = state.session.lock().await;
    let Some(handle) = current.as_ref() else {
        return Err("not connected".to_string());
    };
    if !handle.send(bytes) {
        return Err("session task gone".to_string());
    }
    Ok(())
}

/// global.toml as a switch reads it, for `#profile reset` and `#profile
/// load` to lay back over the config they swap in. Holds the persist
/// lock for the read, so a save cannot move the file aside midway. None
/// before startup loads the profile set.
async fn read_shared_layer(state: &SharedState) -> Option<SharedLayer> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    let guard = state.profile_set.lock().await;
    let set = guard.as_ref()?;
    Some(SharedLayer::read(&set.global_path(), *set.scope()))
}

/// [`read_shared_layer`] for a path other than typed input, when one of
/// `lines` is a `#profile reset` or `#profile load` that acts. A timer,
/// the tick auto-fire command, and `mud.input` then keep the shared
/// settings across it the way typed input does. Call before taking the
/// profile lock.
pub(crate) async fn shared_layer_for_lines<'a>(
    app: &AppHandle,
    lines: impl IntoIterator<Item = &'a str>,
) -> Option<SharedLayer> {
    let replaces = lines.into_iter().any(crate::input::may_replace_profile);
    if !replaces {
        return None;
    }
    let state: SharedState = app.state::<SharedState>().inner().clone();
    read_shared_layer(&state).await
}

#[tauri::command]
pub(crate) async fn session_send_input(
    app: AppHandle,
    state: State<'_, SharedState>,
    line: String,
) -> Result<(), String> {
    // `#profile reset` and `#profile load` replace the live profile
    // wholesale, panes and tracked affects included. Path B turns them
    // into echoes, so there they change nothing.
    let mut effects = crate::input::LineEffects::default();
    // The profile file they read holds none of the shared settings, so
    // global.toml goes back over the result the way a switch lays it.
    let shared_layer = if crate::input::may_replace_profile(&line) {
        read_shared_layer(state.inner()).await
    } else {
        None
    };
    let (mut result, target_after, script_apply, lists) = {
        let mut profile = state.profile.lock().await;
        let lists_before = ListRevisions::of(&profile);
        let before_name = profile.target.name.clone();
        let before_idx = profile.target.room_idx;
        let before_keys = profile.target.quick_keys.clone();
        let ran = match &shared_layer {
            Some(layer) => layer.keep_across(&mut profile, |p| input::run_line(p, &line)),
            None => input::run_line(&mut profile, &line),
        };
        // Only a reset, or a load that read its file, replaced the
        // profile. A load that failed leaves it for the saves to write.
        effects.note(&line, ran.replaced);
        if ran.replaced {
            bump_panes_generation();
        }
        let result = ran.result;
        let after_name = profile.target.name.clone();
        let after_idx = profile.target.room_idx;
        let after_keys = profile.target.quick_keys.clone();
        let changed =
            before_name != after_name || before_idx != after_idx || before_keys != after_keys;
        let payload = if changed {
            Some(TargetPayload {
                name: after_name,
                room_idx: after_idx,
                quick_keys: after_keys,
            })
        } else {
            None
        };
        // Run any Lua bodies queued by script-bodied aliases that
        // fired during expansion. The actions they produce (sends,
        // echoes, var sets, etc.) fold into the same ScriptOutcome
        // pipeline that the Lua-registered triggers use, and the
        // resulting ApplyResult is appended to this input's
        // bytes / echo so the user sees one coherent response.
        let script_apply = if result.scripts.is_empty() {
            None
        } else {
            let mut combined = vosh_script::ScriptOutcome::default();
            for call in &result.scripts {
                match script_state::eval_with_captures(
                    &mut profile.script,
                    &call.body,
                    &call.captures,
                    "alias-script",
                ) {
                    Ok(o) => combined.actions.extend(o.actions),
                    Err(err) => {
                        tracing::warn!(error = %err, "alias script eval failed");
                    }
                }
            }
            Some(script_state::apply_actions(&mut profile, combined))
        };
        let lists = ListChanges::since(lists_before, &profile);
        (result, payload, script_apply, lists)
    };
    // #trigger, #alias, and the Lua they run change the lists an open
    // Settings page shows, so tell it.
    broadcast_list_changes(&app, lists);

    effects.note_script(script_apply.as_ref().is_some_and(|a| a.durable_changed));
    settle_line_effects(&app, effects).await;

    if let Some(apply) = script_apply {
        // Lua actions append AFTER the alias's template output (which
        // is currently empty for script-bodied aliases since the
        // body owns the response). Echoes go through the same local
        // echo path as the alias's own echoes; send bytes append to
        // the wire payload.
        result.echo.extend(apply.echoes);
        result.bytes.extend(apply.send_bytes);
    }

    if let Some(payload) = target_after {
        let _ = app.emit("session://target", payload);
    }

    if !result.echo.is_empty() {
        let mut buf = Vec::new();
        for line in &result.echo {
            buf.extend_from_slice(line.as_bytes());
            buf.extend_from_slice(b"\r\n");
        }
        let _ = app.emit("session://output", OutputPayload::from_bytes(&buf));
    }

    if result.bytes.is_empty() {
        return Ok(());
    }

    let current = state.session.lock().await;
    let Some(handle) = current.as_ref() else {
        let _ = app.emit(
            "session://output",
            OutputPayload::from_bytes(b"\r\n[not connected]\r\n"),
        );
        return Ok(());
    };
    if !handle.send(result.bytes) {
        return Err("session task gone".to_string());
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn session_disconnect(
    app: AppHandle,
    state: State<'_, SharedState>,
) -> Result<(), String> {
    {
        let mut current = state.session.lock().await;
        if let Some(handle) = current.take() {
            handle.shutdown().await;
        }
    }
    if let Ok(mut g) = state.current_connection.lock() {
        *g = None;
    }
    if let Ok(mut g) = state.current_character.lock() {
        *g = None;
    }
    crate::characters::broadcast_session_identity(&app, state.inner()).await;
    Ok(())
}

/// Inform the session of a new terminal size. The backend updates the
/// telnet negotiator and, when NAWS has already been negotiated with
/// the server, pushes a NAWS subnegotiation so the MUD re-wraps its
/// output at the new column count. No-op when not connected.
#[tauri::command]
pub(crate) async fn session_set_window_size(
    state: State<'_, SharedState>,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    // Always cache the size — even when no session exists, so the
    // next `session_connect` can seed the negotiator with the real
    // dimensions instead of the 80×24 default. Without this the
    // server's first NAWS reply (during the early handshake) would
    // carry the wrong size and wrap early output until the next
    // user-driven resize triggered a fresh subneg.
    if let Ok(mut guard) = state.window_size.lock() {
        *guard = (cols, rows);
    }
    let current = state.session.lock().await;
    if let Some(handle) = current.as_ref() {
        if !handle.set_window_size(cols, rows) {
            return Err("session task gone".into());
        }
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn triggers_list(state: State<'_, SharedState>) -> Result<Vec<Trigger>, String> {
    let p = state.profile.lock().await;
    Ok(p.triggers.list())
}

/// Snapshot of the current target state + configured quick-keys.
/// Frontend uses this to seed the `TargetBar` on mount before any
/// `session://target` events fire.
#[tauri::command]
pub(crate) async fn target_get(state: State<'_, SharedState>) -> Result<TargetPayload, String> {
    let p = state.profile.lock().await;
    Ok(TargetPayload {
        name: p.target.name.clone(),
        room_idx: p.target.room_idx,
        quick_keys: p.target.quick_keys.clone(),
    })
}

/// Snapshot of every keyboard macro binding. Used by the Settings
/// macros tab to render the existing list and by Input.tsx (via the
/// same payload) to seed its in-memory binding lookup before any
/// `vosh://macros-changed` event fires.
/// Detect which import format a file uses, based on content sniffing.
/// Frontend extension-checks first; this is the fallback. Returns
/// `null` when nothing recognized so the UI can ask the user.
#[tauri::command]
pub(crate) async fn import_detect(text: String) -> Result<Option<String>, String> {
    Ok(crate::import::detect_format(&text).map(|f| match f {
        crate::import::ImportFormat::Mushclient => "mushclient".to_string(),
        crate::import::ImportFormat::Mudlet => "mudlet".to_string(),
        crate::import::ImportFormat::Gmud => "gmud".to_string(),
        crate::import::ImportFormat::Cmud => "cmud".to_string(),
    }))
}

#[derive(serde::Serialize)]
pub(crate) struct ImportSummary {
    pub aliases: usize,
    pub triggers: usize,
    pub macros: usize,
    pub vars: usize,
    pub unsupported: Vec<(String, String)>,
    pub unparsed: Vec<String>,
    pub rejected: Vec<String>,
}

/// Parse + apply an import file to the live profile. The format
/// string is one of `mushclient` / `mudlet` / `gmud`; pass an
/// empty string to auto-detect. Aliases / triggers / macros / vars
/// merge into the existing stores (overwrite on name collision).
/// Returns a summary so the UI can report what landed and what
/// did not.
#[tauri::command]
pub(crate) async fn import_apply(
    app: AppHandle,
    state: State<'_, SharedState>,
    format: String,
    text: String,
) -> Result<ImportSummary, String> {
    let fmt = match format.as_str() {
        "mushclient" => crate::import::ImportFormat::Mushclient,
        "mudlet" => crate::import::ImportFormat::Mudlet,
        "gmud" => crate::import::ImportFormat::Gmud,
        "cmud" => crate::import::ImportFormat::Cmud,
        "" => crate::import::detect_format(&text)
            .ok_or_else(|| "could not detect import format".to_string())?,
        other => return Err(format!("unknown import format: {other}")),
    };
    let report = crate::import::parse(fmt, &text);
    let mut rejected: Vec<String> = Vec::new();
    let mut macros_changed = false;
    let macros_snapshot: Vec<Macro>;
    let lists;
    {
        let mut p = state.profile.lock().await;
        let lists_before = ListRevisions::of(&p);
        for alias in &report.aliases {
            p.aliases.set(alias.clone());
        }
        for trigger in &report.triggers {
            if let Err(e) = p.triggers.set(trigger.clone()) {
                rejected.push(format!("trigger `{}` rejected: {e}", trigger.name));
            }
        }
        for m in &report.macros {
            if let Some(existing) = p.macros.iter_mut().find(|x| x.key == m.key) {
                existing.command.clone_from(&m.command);
            } else {
                p.macros.push(m.clone());
            }
            macros_changed = true;
        }
        for (k, v) in &report.vars {
            p.vars.set(vosh_vars::Scope::Profile, k.clone(), v.clone());
        }
        macros_snapshot = p.macros.clone();
        lists = ListChanges::since(lists_before, &p);
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    if macros_changed {
        broadcast(&app, "vosh://macros-changed", &macros_snapshot);
    }
    broadcast_list_changes(&app, lists);
    Ok(ImportSummary {
        aliases: report.aliases.len(),
        triggers: report.triggers.len() - rejected.len(),
        macros: report.macros.len(),
        vars: report.vars.len(),
        unsupported: report.unsupported,
        unparsed: report.unparsed,
        rejected,
    })
}

#[tauri::command]
pub(crate) async fn macros_list(state: State<'_, SharedState>) -> Result<Vec<Macro>, String> {
    let p = state.profile.lock().await;
    Ok(p.macros.clone())
}

/// Set or replace a binding by key. Empty `command` is rejected;
/// callers that want to unbind should use `macros_delete`.
/// Re-binding an existing key overwrites the prior command. `enabled`
/// turns the binding on or off without unbinding it. Absent keeps an
/// existing binding's state and makes a new binding on.
#[tauri::command]
pub(crate) async fn macros_set(
    app: AppHandle,
    state: State<'_, SharedState>,
    key: String,
    command: String,
    group: Option<String>,
    enabled: Option<bool>,
) -> Result<Vec<Macro>, String> {
    let key = key.trim().to_string();
    let command = command.trim().to_string();
    if key.is_empty() {
        return Err("key cannot be empty".into());
    }
    if command.is_empty() {
        return Err("command cannot be empty".into());
    }
    // Normalize the group: empty / whitespace-only -> None so the
    // wire format does not persist an empty group string.
    let group = group
        .map(|g| g.trim().to_string())
        .filter(|g| !g.is_empty());
    let updated = {
        let mut p = state.profile.lock().await;
        if let Some(existing) = p.macros.iter_mut().find(|m| m.key == key) {
            existing.command = command;
            existing.group = group;
            if let Some(enabled) = enabled {
                existing.enabled = enabled;
            }
        } else {
            p.macros.push(Macro {
                key,
                command,
                group,
                enabled: enabled.unwrap_or(true),
            });
        }
        p.macros.clone()
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast(&app, "vosh://macros-changed", &updated);
    Ok(updated)
}

/// Remove a binding by key. No-op when the key is not bound.
#[tauri::command]
pub(crate) async fn macros_delete(
    app: AppHandle,
    state: State<'_, SharedState>,
    key: String,
) -> Result<Vec<Macro>, String> {
    let updated = {
        let mut p = state.profile.lock().await;
        p.macros.retain(|m| m.key != key);
        p.macros.clone()
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast(&app, "vosh://macros-changed", &updated);
    Ok(updated)
}

/// List every interval timer, in stored order.
#[tauri::command]
pub(crate) async fn timers_list(state: State<'_, SharedState>) -> Result<Vec<Timer>, String> {
    let p = state.profile.lock().await;
    Ok(p.timers.clone())
}

/// Create or update an interval timer. A `None` id creates a new timer
/// (assigned the next free id); an existing id updates in place. The
/// interval is clamped to at least one second. Returns the full list.
#[tauri::command]
pub(crate) async fn timers_set(
    app: AppHandle,
    state: State<'_, SharedState>,
    id: Option<u32>,
    name: String,
    interval_secs: u32,
    command: String,
    enabled: bool,
) -> Result<Vec<Timer>, String> {
    let name = name.trim().to_string();
    let command = command.trim().to_string();
    if command.is_empty() {
        return Err("command cannot be empty".into());
    }
    let interval_secs = interval_secs.max(1);
    let updated = {
        let mut p = state.profile.lock().await;
        match id.and_then(|wanted| p.timers.iter_mut().find(|t| t.id == wanted)) {
            Some(existing) => {
                existing.name = name;
                existing.interval_secs = interval_secs;
                existing.command = command;
                existing.enabled = enabled;
            }
            None => {
                let next_id = p.timers.iter().map(|t| t.id).max().unwrap_or(0) + 1;
                p.timers.push(Timer {
                    id: next_id,
                    name,
                    interval_secs,
                    command,
                    enabled,
                });
            }
        }
        p.timers.clone()
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast(&app, "vosh://timers-changed", &updated);
    Ok(updated)
}

/// Remove a timer by id. No-op when the id is not present.
#[tauri::command]
pub(crate) async fn timers_delete(
    app: AppHandle,
    state: State<'_, SharedState>,
    id: u32,
) -> Result<Vec<Timer>, String> {
    let updated = {
        let mut p = state.profile.lock().await;
        p.timers.retain(|t| t.id != id);
        p.timers.clone()
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast(&app, "vosh://timers-changed", &updated);
    Ok(updated)
}

/// One entry in a groups-list response: name + whether the group is
/// currently enabled. Used by every per-type groups list endpoint so
/// the frontend can render the toggle UI from a single shape.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct GroupState {
    pub name: String,
    pub enabled: bool,
}

#[tauri::command]
pub(crate) async fn aliases_groups_list(
    state: State<'_, SharedState>,
) -> Result<Vec<GroupState>, String> {
    let p = state.profile.lock().await;
    Ok(p.aliases
        .groups()
        .into_iter()
        .map(|(name, enabled)| GroupState { name, enabled })
        .collect())
}

#[tauri::command]
pub(crate) async fn aliases_set_group_enabled(
    app: AppHandle,
    state: State<'_, SharedState>,
    group: String,
    enabled: bool,
) -> Result<(), String> {
    {
        let mut p = state.profile.lock().await;
        p.aliases.set_group_enabled(group.trim(), enabled);
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast(&app, "vosh://alias-groups-changed", &group);
    Ok(())
}

#[tauri::command]
pub(crate) async fn triggers_groups_list(
    state: State<'_, SharedState>,
) -> Result<Vec<GroupState>, String> {
    let p = state.profile.lock().await;
    Ok(p.triggers
        .groups()
        .into_iter()
        .map(|(name, enabled)| GroupState { name, enabled })
        .collect())
}

#[tauri::command]
pub(crate) async fn triggers_set_group_enabled(
    app: AppHandle,
    state: State<'_, SharedState>,
    group: String,
    enabled: bool,
) -> Result<(), String> {
    {
        let mut p = state.profile.lock().await;
        p.triggers.set_group_enabled(group.trim(), enabled);
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast(&app, "vosh://trigger-groups-changed", &group);
    Ok(())
}

#[tauri::command]
pub(crate) async fn macros_groups_list(
    state: State<'_, SharedState>,
) -> Result<Vec<GroupState>, String> {
    let p = state.profile.lock().await;
    let mut names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for m in &p.macros {
        if let Some(g) = &m.group {
            if !g.is_empty() {
                names.insert(g.clone());
            }
        }
    }
    Ok(names
        .into_iter()
        .map(|n| {
            let enabled = !p.disabled_macro_groups.contains(&n);
            GroupState { name: n, enabled }
        })
        .collect())
}

#[tauri::command]
pub(crate) async fn macros_set_group_enabled(
    app: AppHandle,
    state: State<'_, SharedState>,
    group: String,
    enabled: bool,
) -> Result<(), String> {
    let group = group.trim().to_string();
    if group.is_empty() {
        return Ok(());
    }
    {
        let mut p = state.profile.lock().await;
        if enabled {
            p.disabled_macro_groups.remove(&group);
        } else {
            p.disabled_macro_groups.insert(group.clone());
        }
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast(&app, "vosh://macro-groups-changed", &group);
    Ok(())
}

#[tauri::command]
pub(crate) async fn triggers_export(state: State<'_, SharedState>) -> Result<String, String> {
    let p = state.profile.lock().await;
    p.triggers.export_json().map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn triggers_import(
    app: AppHandle,
    state: State<'_, SharedState>,
    json: String,
) -> Result<usize, String> {
    let count = {
        let mut p = state.profile.lock().await;
        p.triggers.import_json(&json).map_err(|e| e.to_string())?
    };
    // The editor's save path lands here: persist, or the "saved" state
    // lives only in memory and vanishes on restart. Broadcast so the
    // group checkboxes resync (the import may add or drop groups).
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    // Empty string, not unit: the frontend listener types this
    // payload as string. Empty means "more than one group changed".
    broadcast(&app, "vosh://trigger-groups-changed", &"");
    broadcast_list_changes(&app, ListChanges::TRIGGERS);
    Ok(count)
}

/// Dump every alias to a pretty JSON array. Mirrors `triggers_export`
/// so the settings window can treat triggers and aliases with the
/// same `JsonTab` component.
#[tauri::command]
pub(crate) async fn aliases_export(state: State<'_, SharedState>) -> Result<String, String> {
    let p = state.profile.lock().await;
    let aliases: Vec<vosh_alias::Alias> = p.aliases.list().into_iter().cloned().collect();
    serde_json::to_string_pretty(&aliases).map_err(|e| e.to_string())
}

/// Replace the entire alias store with the JSON-decoded list. Returns
/// the count installed. Invalid JSON or wrong shape rejects without
/// touching the store.
#[tauri::command]
pub(crate) async fn aliases_import(
    app: AppHandle,
    state: State<'_, SharedState>,
    json: String,
) -> Result<usize, String> {
    let parsed: Vec<vosh_alias::Alias> = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let count = parsed.len();
    {
        let mut p = state.profile.lock().await;
        let mut store = vosh_alias::AliasStore::new();
        for alias in parsed {
            store.set(alias);
        }
        // The disabled-groups set is user state about GROUPS, not items;
        // replacing the store without carrying it over silently
        // re-enabled every disabled group on each editor save.
        store.set_disabled_groups(p.aliases.disabled_groups());
        p.aliases = store;
    }
    // Same persistence rule as triggers_import: the editor saves here.
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    // Empty string, not unit: the frontend listener types this
    // payload as string. Empty means "more than one group changed".
    broadcast(&app, "vosh://alias-groups-changed", &"");
    broadcast_list_changes(&app, ListChanges::ALIASES);
    Ok(count)
}

#[tauri::command]
pub(crate) fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Tier 3 native renderer (macOS). The frontend reports the terminal
/// pane's screen rectangle (CSS pixels, top-left origin, relative to the
/// window) and device pixel ratio so the native wgpu surface can track
/// it. NSView/Metal must be touched on the main thread, so the work is
/// dispatched there. A no-op on other platforms and when the surface is
/// not installed.
#[tauri::command]
pub(crate) fn native_surface_set_bounds(
    app: AppHandle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    dpr: f64,
) {
    #[cfg(native_surface)]
    {
        let _ = app.run_on_main_thread(move || {
            crate::native_surface::set_bounds(x, y, width, height, dpr);
        });
    }
    #[cfg(not(native_surface))]
    {
        let _ = (&app, x, y, width, height, dpr);
    }
}

/// Tier 3 native renderer, underlay mode (macOS). The webview sits above
/// the surface and receives every click, so the page forwards pointer
/// events over the terminal here. `x` and `y` are CSS px from the pane's
/// top-left corner. `kind` is "down", "drag", "up", "move", "leave", or
/// "middle". `open` carries the Cmd modifier for opening links. The work
/// runs on the main thread, which the renderer requires. A no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_pointer(app: AppHandle, kind: String, x: f64, y: f64, open: bool) {
    #[cfg(native_surface)]
    {
        let _ = app.run_on_main_thread(move || {
            crate::native_surface::forward_pointer(&kind, x, y, open);
        });
    }
    #[cfg(not(native_surface))]
    {
        let _ = (&app, kind, x, y, open);
    }
}

/// Tier 3 native renderer: true once the surface installed and its GPU came
/// up. The page leaves the terminal pane transparent only after this, so a
/// failed install falls back to xterm. False elsewhere.
#[tauri::command]
pub(crate) fn native_surface_ready() -> bool {
    #[cfg(native_surface)]
    {
        crate::native_surface::is_ready()
    }
    #[cfg(not(native_surface))]
    {
        false
    }
}

/// Tier 3 native renderer, underlay mode (macOS): a wheel delta forwarded
/// from the page. Positive reveals older lines. A no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_wheel(app: AppHandle, delta_y: f64) {
    #[cfg(native_surface)]
    {
        let _ = app.run_on_main_thread(move || {
            crate::native_surface::forward_wheel(delta_y);
        });
    }
    #[cfg(not(native_surface))]
    {
        let _ = (&app, delta_y);
    }
}

/// Tier 3 native renderer (macOS): copy the current selection to the
/// clipboard. Used by the Cmd+C / Ctrl+C path; a no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_copy() {
    #[cfg(native_surface)]
    crate::native_surface::request_copy();
}

/// Tier 3 native renderer: select everything in the grid, scrollback
/// included, for the terminal menu's Select all and Cmd+A on an empty
/// command line. Repaints; a no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_select_all() {
    #[cfg(native_surface)]
    {
        crate::term_grid::select_all();
        crate::native_surface::request_redraw();
    }
}

/// Parse a `#rrggbb` (or `rrggbb`) hex color.
#[cfg(native_surface)]
fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim().trim_start_matches('#');
    if s.len() < 6 {
        return None;
    }
    Some((
        u8::from_str_radix(&s[0..2], 16).ok()?,
        u8::from_str_radix(&s[2..4], 16).ok()?,
        u8::from_str_radix(&s[4..6], 16).ok()?,
    ))
}

/// Tier 3 native renderer (macOS): set the surface theme colors so the
/// background, foreground, and selection follow the active Vosh theme.
/// Colors are `#rrggbb`. A no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_set_theme(
    background: String,
    foreground: String,
    selection: String,
    ansi: Vec<String>,
) {
    #[cfg(native_surface)]
    {
        if let (Some(bg), Some(fg), Some(sel)) = (
            parse_hex(&background),
            parse_hex(&foreground),
            parse_hex(&selection),
        ) {
            crate::cell_render::set_theme(bg, fg, sel);
            let palette: Vec<(u8, u8, u8)> = ansi.iter().filter_map(|s| parse_hex(s)).collect();
            if palette.len() == 16 {
                crate::cell_render::set_palette(&palette);
            }
            crate::native_surface::request_redraw();
        }
    }
    #[cfg(not(native_surface))]
    {
        let _ = (background, foreground, selection, ansi);
    }
}

/// Tier 3 native renderer: apply the split divider color setting to the
/// surface renderer (hex or `rgb()`/`rgba()`; None restores the default).
#[tauri::command]
pub(crate) fn native_surface_set_divider_color(color: Option<String>) {
    #[cfg(native_surface)]
    {
        let parsed = color
            .as_deref()
            .and_then(crate::cell_render::parse_css_color);
        crate::cell_render::set_divider_color(parsed);
        crate::native_surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    {
        let _ = color;
    }
}

/// Tier 3 native renderer: the chrome colors the page derives with its
/// theme tokens, as CSS colors (hex, or `rgb()`/`rgba()` with alpha). The
/// split divider, the selection, every find match, the current match, a
/// hovered link, and the scrollbar thumb. Each call replaces the whole
/// set, and a missing or unreadable color falls back to one derived from
/// the terminal palette. The divider setting still wins over `divider`.
#[tauri::command]
pub(crate) fn native_surface_set_tokens(
    divider: Option<String>,
    selection: Option<String>,
    find_match: Option<String>,
    current_match: Option<String>,
    link: Option<String>,
    scrollbar: Option<String>,
) {
    #[cfg(native_surface)]
    {
        let parse = |v: Option<String>| v.as_deref().and_then(crate::cell_render::parse_css_color);
        crate::cell_render::set_tokens(crate::cell_render::ChromeTokens {
            divider: parse(divider),
            selection: parse(selection),
            find_match: parse(find_match),
            current_match: parse(current_match),
            link: parse(link),
            scrollbar: parse(scrollbar),
        });
        crate::native_surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    {
        let _ = (
            divider,
            selection,
            find_match,
            current_match,
            link,
            scrollbar,
        );
    }
}

/// Tier 3 native renderer (macOS): toggle drawing bright (ANSI 8-15) colored
/// text with the bold font weight. A no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_set_bright_bold(on: bool) {
    #[cfg(native_surface)]
    {
        crate::cell_render::set_bright_bold(on);
        crate::native_surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    {
        let _ = on;
    }
}

/// Tier 3 native renderer (macOS): report xterm's device cell size so the
/// surface grid matches the webview's spacing exactly instead of deriving it
/// from font metrics. `char_height` is xterm's device glyph box, which it
/// centers in a cell taller than the box, so the surface can put its
/// baseline in the same place at every line height. A no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_set_cell_metrics(width: u32, height: u32, char_height: Option<u32>) {
    #[cfg(native_surface)]
    crate::native_surface::set_cell_metrics(width, height, char_height.unwrap_or(0));
    #[cfg(not(native_surface))]
    {
        let _ = (width, height, char_height);
    }
}

/// Tier 3 native renderer (macOS): hide or show the surface so a DOM overlay
/// (dropdown, menu, modal) that would be occluded by the opaque surface
/// shows through. xterm renders the same content behind it. A no-op
/// elsewhere.
#[tauri::command]
pub(crate) fn native_surface_set_visible(visible: bool) {
    #[cfg(native_surface)]
    crate::native_surface::set_visible(visible);
    #[cfg(not(native_surface))]
    {
        let _ = visible;
    }
}

/// Tier 3 native renderer (macOS): echo locally-sent input into the grid so
/// the user sees their own commands (xterm gets the same bytes via
/// onLocalEcho). `text` is the already-styled echo line. A no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_echo(text: String) {
    #[cfg(native_surface)]
    {
        crate::term_grid::feed_bytes(text.as_bytes());
        crate::native_surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    {
        let _ = text;
    }
}

/// Tier 3 native renderer (macOS): search the grid and step to the next (or
/// previous) match, scrolling it into view and highlighting all matches.
/// Returns `[current, total]` (1-based; `[0, 0]` when no match). A no-op
/// returning `[0, 0]` elsewhere.
#[tauri::command]
pub(crate) fn native_surface_find(
    query: String,
    regex: bool,
    case_sensitive: bool,
    whole_word: bool,
    forward: bool,
) -> (usize, usize) {
    #[cfg(native_surface)]
    {
        let result = crate::term_grid::find_run(&query, regex, case_sensitive, whole_word, forward);
        crate::native_surface::request_redraw();
        result
    }
    #[cfg(not(native_surface))]
    {
        let _ = (query, regex, case_sensitive, whole_word, forward);
        (0, 0)
    }
}

/// Tier 3 native renderer (macOS): clear the find highlight. A no-op
/// elsewhere.
#[tauri::command]
pub(crate) fn native_surface_find_clear() {
    #[cfg(native_surface)]
    {
        crate::term_grid::find_clear();
        crate::native_surface::request_redraw();
    }
}

/// Tier 3 native renderer (macOS): rebuild the surface atlas at a new font
/// family and size (CSS px) so it matches the configured Vosh font. A no-op
/// elsewhere.
#[tauri::command]
pub(crate) fn native_surface_set_font(family: String, size: u32) {
    #[cfg(native_surface)]
    crate::native_surface::request_set_font(family, size);
    #[cfg(not(native_surface))]
    {
        let _ = (family, size);
    }
}

/// Tier 3 native renderer (macOS): keyboard scroll. `kind` is "pageup",
/// "pagedown", "bottom", or "toggle". Toggle opens or closes the split
/// the way a middle click does: scrolled back it snaps to the live
/// tail, at the tail it pages up into scrollback. Scrolls the grid and
/// repaints; a no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_scroll(kind: String) {
    #[cfg(native_surface)]
    {
        match kind.as_str() {
            "pageup" => crate::term_grid::scroll_page(true),
            "pagedown" => crate::term_grid::scroll_page(false),
            "bottom" => crate::term_grid::scroll_to_bottom(),
            "toggle" => {
                let (offset, _) = crate::term_grid::scroll_metrics();
                if offset > 0 {
                    crate::term_grid::scroll_to_bottom();
                } else {
                    crate::term_grid::scroll_page(true);
                }
            }
            _ => {}
        }
        crate::native_surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    {
        let _ = kind;
    }
}

/// Read the persistent dock layout. Returns the same shape as the
/// frontend `DockEntry` (id + zone) so the layout editor can render
/// from it directly.
#[tauri::command]
pub(crate) async fn dock_layout_get(
    state: State<'_, SharedState>,
) -> Result<Vec<DockEntryPersist>, String> {
    let p = state.profile.lock().await;
    Ok(p.ui.dock_layout.clone())
}

/// Replace the persistent dock layout. Persists to profile.toml and
/// broadcasts `vosh://dock-layout-changed` so other open windows
/// (specifically the main window) can re-apply without a relaunch.
#[tauri::command]
pub(crate) async fn dock_layout_set(
    app: AppHandle,
    state: State<'_, SharedState>,
    entries: Vec<DockEntryPersist>,
) -> Result<(), String> {
    {
        let mut p = state.profile.lock().await;
        p.ui.dock_layout.clone_from(&entries);
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    if let Err(e) = app.emit("vosh://dock-layout-changed", &entries) {
        warn!(error = %e, "failed to broadcast dock-layout-changed");
    }
    Ok(())
}

/// A pane tree as the frontend receives it: the layout plus the
/// [`PANES_GENERATION`] it was read at. The generation never reaches
/// disk, and an inactive profile's tree carries none, since no pane
/// layout write can target it.
#[derive(Clone, serde::Serialize)]
pub(crate) struct PaneLayoutEnvelope {
    #[serde(flatten)]
    pub(crate) layout: PaneLayoutPersist,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) generation: Option<u64>,
}

/// The active profile's pane layout and its generation. Call with the
/// profile lock held.
pub(crate) fn pane_layout_envelope(p: &Profile) -> PaneLayoutEnvelope {
    PaneLayoutEnvelope {
        layout: p.ui.pane_layout(),
        generation: Some(panes_generation()),
    }
}

/// Hand every window the active profile's panes and tracked affects.
/// For the paths that replace the live UI config wholesale (a profile
/// switch, an import, `#profile load` and `reset`), which must also
/// bump the pane generation under the profile lock as they swap.
pub(crate) async fn broadcast_profile_ui(app: &AppHandle, state: &SharedState) {
    let (panes, tracked) = {
        let p = state.profile.lock().await;
        (pane_layout_envelope(&p), p.ui.tracked_affects.clone())
    };
    broadcast(app, "vosh://pane-layout-changed", &panes);
    broadcast(app, "vosh://tracked-affects-changed", &tracked);
}

/// Read a profile's pane layout, the active one when `profile` is
/// absent. A profile that has never saved one gets a tree migrated from
/// its dock layout (or the default), with nothing written to disk until
/// the first edit. An inactive profile's tree comes from its file and
/// carries no generation.
#[tauri::command]
pub(crate) async fn pane_layout_get(
    state: State<'_, SharedState>,
    profile: Option<String>,
) -> Result<PaneLayoutEnvelope, String> {
    if let Some(name) = profile.as_deref() {
        let shared: SharedState = state.inner().clone();
        if let Some(layout) = crate::characters::inactive_pane_layout(&shared, name).await? {
            return Ok(PaneLayoutEnvelope {
                layout,
                generation: None,
            });
        }
    }
    let p = state.profile.lock().await;
    Ok(pane_layout_envelope(&p))
}

/// Replace the active profile's pane layout and broadcast the
/// sanitized tree as `vosh://pane-layout-changed` to every window.
/// Splitter drags land here several times a second even after the
/// frontend debounce, so the disk write goes through the debounced
/// `mark_profile_dirty` rather than rotating a backup per drag step.
/// A profile switch or quit flushes it right away.
///
/// `generation` is the one the edited tree was read at. A write made
/// against a profile that has since been swapped out is refused and
/// returns false, and the caller reads the current tree again. An
/// untagged write (a tree that never came from the backend) applies.
#[tauri::command]
pub(crate) async fn pane_layout_set(
    app: AppHandle,
    state: State<'_, SharedState>,
    layout: PaneLayoutPersist,
    generation: Option<u64>,
) -> Result<bool, String> {
    let mut layout = layout;
    layout.sanitize();
    let current = {
        let mut p = state.profile.lock().await;
        let current = panes_generation();
        if generation.is_some_and(|g| g != current) {
            return Ok(false);
        }
        p.ui.panes = Some(layout.clone());
        current
    };
    // A layout tweak after `#profile reset` must not save the blanked
    // profile, so this schedules without clearing the suppression.
    schedule_profile_persist(&app);
    broadcast(
        &app,
        "vosh://pane-layout-changed",
        &PaneLayoutEnvelope {
            layout,
            generation: Some(current),
        },
    );
    Ok(true)
}

/// The Settings window's default size, the approved boards' 880×600.
const SETTINGS_SIZE: (f64, f64) = (880.0, 600.0);
/// The smallest Settings window whose two column layouts still fit.
const SETTINGS_MIN_SIZE: (f64, f64) = (820.0, 560.0);

/// The logical size a Settings window should take when the window state
/// plugin restored it at `restored`, or None when it already fits. A
/// side under the minimum, saved by an older and smaller Settings
/// window, goes back to the default. The system does not apply the
/// minimum to a size set from code, so this has to.
fn settings_window_fit(restored: (f64, f64)) -> Option<(f64, f64)> {
    let (width, height) = restored;
    let (min_width, min_height) = SETTINGS_MIN_SIZE;
    if width >= min_width && height >= min_height {
        return None;
    }
    Some((
        if width < min_width {
            SETTINGS_SIZE.0
        } else {
            width
        },
        if height < min_height {
            SETTINGS_SIZE.1
        } else {
            height
        },
    ))
}

/// Open (or focus, if already open) the standalone settings window.
/// The settings window is a separate webview pointed at the same
/// frontend bundle with `?view=settings`, so the React entry can
/// branch and render the `SettingsApp` instead of the main `App`.
/// Both windows share the same Rust backend state.
#[tauri::command]
pub(crate) async fn open_settings_window(app: AppHandle) -> Result<(), String> {
    if let Some(existing) = app.get_webview_window("settings") {
        existing.show().map_err(|e| e.to_string())?;
        existing.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }
    let builder = WebviewWindowBuilder::new(&app, "settings", WebviewUrl::App("index.html?view=settings".into()))
            .title("Settings")
            .inner_size(SETTINGS_SIZE.0, SETTINGS_SIZE.1)
            .min_inner_size(SETTINGS_MIN_SIZE.0, SETTINGS_MIN_SIZE.1)
            .resizable(true)
            .transparent(true)
            // Stay hidden until the React app calls show() on first render
            // so the user never sees the unstyled default state.
            .visible(false)
            // Disable Tauri's OS file-drop handler. When enabled it
            // intercepts HTML5 drag-and-drop inside the webview, which
            // can break overlay drag interactions.
            .disable_drag_drop_handler();
    // macOS gives Settings the main window's titled frame: native
    // traffic lights over the sidebar at the same centers, a hidden
    // title, and the system's corners and rim. Windows and Linux stay
    // frameless, and the page draws its own window controls.
    #[cfg(target_os = "macos")]
    let builder = builder
        .decorations(true)
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true);
    #[cfg(not(target_os = "macos"))]
    let builder = builder.decorations(false);
    let window = builder.build().map_err(|e| e.to_string())?;
    if let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) {
        let current = size.to_logical::<f64>(scale);
        if let Some((width, height)) = settings_window_fit((current.width, current.height)) {
            let _ = window.set_size(tauri::LogicalSize::new(width, height));
        }
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn profile_export(state: State<'_, SharedState>) -> Result<String, String> {
    let p = state.profile.lock().await;
    let snapshot = ProfileConfig::from_profile(&p);
    snapshot.to_toml().map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn profile_import(
    app: AppHandle,
    state: State<'_, SharedState>,
    toml: String,
) -> Result<Vec<String>, String> {
    let snapshot = ProfileConfig::from_toml(&toml).map_err(|e| e.to_string())?;
    let applied = {
        let mut p = state.profile.lock().await;
        let applied = snapshot.apply_to(&mut p);
        bump_panes_generation();
        applied
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    // The import replaced the panes and tracked affects too, and the
    // main window would otherwise write its old tree back.
    broadcast_profile_ui(&app, &shared).await;
    Ok(applied)
}

// ============================================================
// Named profile collection (multi-profile support, Stage 1).
//
// Persistent layout under <app_data_dir>:
//     profiles.toml        — index (active + entries)
//     profiles/<name>.toml — per-profile snapshot
// AppState.profile_set holds the live ProfileSet behind a Mutex.
// ============================================================

#[derive(serde::Serialize)]
pub(crate) struct ProfilesListPayload {
    pub active: String,
    pub profiles: Vec<crate::profile_set::ProfileEntry>,
}

#[tauri::command]
pub(crate) async fn profiles_list(
    state: State<'_, SharedState>,
) -> Result<ProfilesListPayload, String> {
    let guard = state.profile_set.lock().await;
    let Some(set) = guard.as_ref() else {
        return Err(PROFILES_NOT_LOADED.into());
    };
    Ok(ProfilesListPayload {
        active: set.active_name().to_string(),
        profiles: set.list().to_vec(),
    })
}

/// Write the live profile to its file before a copy of `source` reads
/// that file, when `source` is the live profile. Call with
/// [`PERSIST_LOCK`] held across this and the copy, so the copy reads
/// what the flush wrote and no persist rewrites the source mid copy.
async fn flush_before_copy(app: &AppHandle, shared: &SharedState, source: &str) {
    let copying_live = shared
        .profile_set
        .lock()
        .await
        .as_ref()
        .is_some_and(|set| set.active_name() == source);
    // The live profile can run two seconds ahead of its file. After
    // `#profile reset` or `load` it is deliberately diverged, and the
    // copy takes the file as it stands.
    if copying_live && !AUTO_PERSIST_SUPPRESSED.load(std::sync::atomic::Ordering::Acquire) {
        persist_profile_locked(app, shared).await;
    }
}

/// Create a profile with `auto_match` as its login claim, starting as a
/// copy of `copy_from` when given. Returns the new entry and does not
/// switch. The claim takes nothing from other profiles, so a caller
/// that wants the character for itself follows with
/// `profile_set_login`.
#[tauri::command]
pub(crate) async fn profile_create(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
    copy_from: Option<String>,
    auto_match: Option<crate::profile_set::AutoMatch>,
) -> Result<crate::profile_set::ProfileEntry, String> {
    let shared: SharedState = state.inner().clone();
    let _persist_guard = PERSIST_LOCK.lock().await;
    if let Some(source) = copy_from.as_deref() {
        flush_before_copy(&app, &shared, source).await;
    }
    let entry = {
        let mut guard = state.profile_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err(PROFILES_NOT_LOADED.into());
        };
        set.create_from(&name, copy_from.as_deref(), auto_match)
            .map_err(|e| e.to_string())?
    };
    broadcast(&app, "vosh://profiles-changed", &entry.name);
    Ok(entry)
}

#[tauri::command]
pub(crate) async fn profile_delete(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
) -> Result<(), String> {
    {
        let _persist_guard = PERSIST_LOCK.lock().await;
        let mut guard = state.profile_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err(PROFILES_NOT_LOADED.into());
        };
        set.delete(&name).map_err(|e| e.to_string())?;
    }
    broadcast(&app, "vosh://profiles-changed", &name);
    Ok(())
}

#[tauri::command]
pub(crate) async fn profile_rename(
    app: AppHandle,
    state: State<'_, SharedState>,
    old: String,
    new: String,
) -> Result<(), String> {
    {
        let _persist_guard = PERSIST_LOCK.lock().await;
        let mut guard = state.profile_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err(PROFILES_NOT_LOADED.into());
        };
        set.rename(&old, &new).map_err(|e| e.to_string())?;
    }
    broadcast(&app, "vosh://profiles-changed", &new);
    Ok(())
}

/// Copy `source` under a new name without its login claim. Duplicating
/// the live profile writes it first, so the copy holds your latest
/// changes.
#[tauri::command]
pub(crate) async fn profile_duplicate(
    app: AppHandle,
    state: State<'_, SharedState>,
    source: String,
    new: String,
) -> Result<(), String> {
    {
        let _persist_guard = PERSIST_LOCK.lock().await;
        flush_before_copy(&app, state.inner(), &source).await;
        let mut guard = state.profile_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err(PROFILES_NOT_LOADED.into());
        };
        set.duplicate(&source, &new).map_err(|e| e.to_string())?;
    }
    broadcast(&app, "vosh://profiles-changed", &new);
    Ok(())
}

/// Read the per-category scope map. Frontend uses this to render
/// the toggle row in the Profiles tab.
#[tauri::command]
pub(crate) async fn profile_get_scope(
    state: State<'_, SharedState>,
) -> Result<crate::profile_set::ScopeConfig, String> {
    let guard = state.profile_set.lock().await;
    let Some(set) = guard.as_ref() else {
        return Err(PROFILES_NOT_LOADED.into());
    };
    Ok(*set.scope())
}

/// Update the per-category scope map. After the index is updated,
/// persist the active profile so values move to the correct file
/// (a category flipped Global -> Profile lands in the per-profile
/// file on next save; Profile -> Global lands in global.toml).
///
/// Turning the theme category global also folds the custom themes the
/// other profile files hold into the shared list and clears them from
/// those files, or a switch to one of those profiles would lose them.
/// When the list grows, `vosh://custom-themes-changed` carries it to
/// every window.
///
/// Turning a category per profile first copies the shared values into
/// every other profile file that holds none of its own, since the save
/// drops them from global.toml.
#[tauri::command]
pub(crate) async fn profile_set_scope(
    app: AppHandle,
    state: State<'_, SharedState>,
    scope: crate::profile_set::ScopeConfig,
) -> Result<(), String> {
    // Held from the scope change through the persist, so no other
    // profile file write lands between the moves and the save.
    let persist_guard = PERSIST_LOCK.lock().await;
    let shared: SharedState = state.inner().clone();
    let gained = change_scope_locked(&shared, scope).await?;
    persist_profile_locked(&app, &shared).await;
    drop(persist_guard);
    if let Some(list) = gained {
        broadcast(&app, "vosh://custom-themes-changed", &list);
    }
    broadcast(&app, "vosh://profiles-changed", &"scope");
    Ok(())
}

/// Why a category cannot stop being shared between `migration_apply`
/// and the relaunch that finishes it. The shared values would have to
/// reach profile files that nothing may write in that window.
const SCOPE_MIGRATION_PENDING: &str =
    "Restart Vosh to finish the move to loadouts, then turn this off.";

/// Why the shared categories cannot change while Vosh holds a file it
/// could not read at launch. The live profile holds the defaults where
/// that file's settings belong, and a change would hand those defaults
/// to the other profiles or share them with every character.
fn scope_refusal_for_unread(set: &crate::profile_set::ProfileSet) -> Option<String> {
    use crate::profile_config::is_unread;
    if is_unread(&set.global_path()) {
        return Some(
            "Vosh could not read global.toml, so it will not change which settings every \
             character shares. Fix the file and restart Vosh."
                .to_string(),
        );
    }
    if is_unread(&set.active_path()) {
        return Some(format!(
            "Vosh could not read the {} profile file, so it will not change which settings \
             every character shares. Fix the file or switch to another profile.",
            crate::profile_set::display_name(set.active_name())
        ));
    }
    None
}

/// The body of [`profile_set_scope`] up to its save. Call with
/// [`PERSIST_LOCK`] held. Returns the live custom themes when turning the
/// theme category global added to them.
async fn change_scope_locked(
    state: &SharedState,
    scope: crate::profile_set::ScopeConfig,
) -> Result<Option<Vec<crate::profile_config::CustomTheme>>, String> {
    use crate::profile_set::Scope;
    let migration_pending = MIGRATION_RELAUNCH_PENDING.load(std::sync::atomic::Ordering::Acquire);
    let before = {
        let guard = state.profile_set.lock().await;
        let set = guard.as_ref().ok_or(PROFILES_NOT_LOADED)?;
        if let Some(refusal) = scope_refusal_for_unread(set) {
            return Err(refusal);
        }
        *set.scope()
    };
    // Every other profile file holds the defaults for a shared category,
    // and the save below drops the category from global.toml, so each
    // file takes the shared values first or that profile opens with the
    // defaults. The live profile holds the shared values.
    if let Some(stopped) = before.stopped_sharing(&scope) {
        if migration_pending {
            return Err(SCOPE_MIGRATION_PENDING.into());
        }
        let values = {
            let p = state.profile.lock().await;
            GlobalConfig::from_profile(&p, &stopped)
        };
        let guard = state.profile_set.lock().await;
        let set = guard.as_ref().ok_or(PROFILES_NOT_LOADED)?;
        hand_out_shared(set, &values)?;
    }
    let (held, global_path) = {
        let mut guard = state.profile_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err(PROFILES_NOT_LOADED.into());
        };
        let theme_was_global = matches!(set.scope().theme, Scope::Global);
        set.set_scope(scope).map_err(|e| e.to_string())?;
        // Nothing may write profile files while a migration relaunch is
        // pending. The next launch moves the themes instead.
        let theme_turned_global =
            !theme_was_global && matches!(scope.theme, Scope::Global) && !migration_pending;
        let held =
            theme_turned_global.then(|| HeldCustomThemes::find(set, Some(set.active_name())));
        (held, set.global_path())
    };
    let mut gained = None;
    if let Some(held) = held {
        let mut p = state.profile.lock().await;
        match share_custom_themes(held, &scope, &global_path, &mut p) {
            Ok(true) => gained = Some(p.ui.custom_themes.clone()),
            Ok(false) => {}
            Err(e) => warn!(error = %e, "custom themes stayed in their profile files"),
        }
    }
    Ok(gained)
}

/// Replace a profile's description and login claim. The claim follows
/// the login toggle's rules, and the result names every profile that
/// lost a character to it. See [`crate::profile_set::ProfileSet::set_metadata`].
#[tauri::command]
pub(crate) async fn profile_set_metadata(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
    description: Option<String>,
    auto_match: Option<crate::profile_set::AutoMatch>,
) -> Result<crate::profile_set::LoginClaim, String> {
    let claim = {
        let mut guard = state.profile_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err(PROFILES_NOT_LOADED.into());
        };
        set.set_metadata(&name, description, auto_match)
            .map_err(|e| e.to_string())?
    };
    broadcast(&app, "vosh://profiles-changed", &name);
    Ok(claim)
}

/// Given a connect target, find the first profile whose `auto_match`
/// claims it. Returns the profile name or null. The frontend calls
/// this right before invoking `session_connect` so a matching
/// profile can be switched to ahead of the connection.
#[tauri::command]
pub(crate) async fn profile_resolve_match(
    state: State<'_, SharedState>,
    host: String,
    port: u16,
    character: Option<String>,
) -> Result<Option<String>, String> {
    let guard = state.profile_set.lock().await;
    let Some(set) = guard.as_ref() else {
        return Ok(None);
    };
    Ok(set.resolve_match(&host, port, character.as_deref()))
}

/// Lay loadout mode's catalog and active loadouts over the live profile
/// `p`, right after a switch loaded a profile file into it. The catalog
/// is the authoritative source for aliases, triggers, and macros in
/// loadout mode, and the profile file holds none of them, so it fills
/// the stores. The group state of `set` then applies to the result.
fn lay_catalog_over(
    p: &mut crate::profile::Profile,
    catalog: &crate::loadout::GlobalCatalog,
    set: Option<&crate::loadout::LoadoutSet>,
) {
    // The per-profile file just restored this profile's group checkbox
    // state into the live stores; carry it across the catalog rebuild
    // (the rebuilt stores would otherwise start with everything
    // enabled).
    let alias_disabled = p.aliases.disabled_groups();
    let trigger_disabled = p.triggers.disabled_groups();
    let mut aliases = vosh_alias::AliasStore::new();
    for a in &catalog.aliases {
        aliases.set(a.clone());
    }
    aliases.set_disabled_groups(alias_disabled);
    p.aliases = aliases;
    let mut triggers = vosh_trigger::TriggerStore::new();
    for t in &catalog.triggers {
        if let Err(e) = triggers.set(t.clone()) {
            warn!(error = %e, "catalog trigger rejected during profile switch");
        }
    }
    triggers.set_disabled_groups(trigger_disabled);
    p.triggers = triggers;
    p.macros.clone_from(&catalog.macros);
    // The presets that are on belong to the catalog with the preset
    // triggers, so the profile's own list gives way to it.
    if let Some(list) = &catalog.enabled_presets {
        p.ui.enabled_presets.clone_from(list);
    }
    if let Some(set) = set {
        crate::loadout_store::apply_effective_state(set, p);
    }
}

/// The files a switch to a profile loads: its own file, None for a
/// profile that never saved one, and global.toml, None before the first
/// save.
struct SwitchFiles {
    per_profile: Option<ProfileConfig>,
    global: Option<GlobalConfig>,
}

/// Read the files a switch to `name` loads, and only then point the
/// index at it. A file that does not read changes nothing, so the index
/// keeps naming the profile the live state holds and the next persist
/// still writes that profile to its own file.
fn open_profile_for_switch(
    set: &mut crate::profile_set::ProfileSet,
    name: &str,
) -> Result<SwitchFiles, String> {
    use crate::profile_set::{display_name, ProfileSetError};
    if set.get(name).is_none() {
        return Err(ProfileSetError::NotFound(name.to_string()).to_string());
    }
    let refused = |what: &str| {
        format!(
            "Vosh could not open the {} profile because it could not read {what}. You are \
             still using the {} profile.",
            display_name(name),
            display_name(set.active_name()),
        )
    };
    let path = set.profile_path(name);
    let per_profile = if path.exists() {
        match ProfileConfig::load(&path) {
            Ok(config) => Some(config),
            Err(e) => {
                warn!(error = %e, path = %path.display(), "profile file unreadable at switch");
                return Err(refused("the profile file"));
            }
        }
    } else {
        None
    };
    let global_path = set.global_path();
    // Only the categories the scope shares, so a value global.toml still
    // holds from before cannot cover the one the profile file owns.
    let global = match GlobalConfig::load_shared(&global_path, set.scope()) {
        Ok(config) => config,
        Err(e) => {
            warn!(error = %e, path = %global_path.display(), "global config unreadable at switch");
            return Err(refused("global.toml, which holds your shared settings"));
        }
    };
    let leaving = set.active_path();
    set.switch(name).map_err(|e| e.to_string())?;
    // Both files read, and the live profile is about to hold what they
    // say, so the saves may write them again. The file of the profile you
    // left no longer stands behind the live profile, and every other write
    // to it reads it first, so a file that did not read at launch is safe
    // from here on.
    for path in [&leaving, &path, &global_path] {
        crate::profile_config::release_unread(path);
    }
    Ok(SwitchFiles {
        per_profile,
        global,
    })
}

/// Steps 2 and 3 of a switch, after the flush of the outgoing profile.
/// Call with [`PERSIST_LOCK`] held. Loads the incoming profile's file
/// and global.toml, points the index at it, then lays both over the
/// live profile, and in loadout mode the catalog and the loadouts too.
/// Either every step lands or none does, and a save that waits on the
/// lock finds the live profile whole.
async fn switch_live_profile(state: &SharedState, name: &str) -> Result<(), String> {
    // Step 2: read the incoming files, then flip the active pointer in
    // the index.
    let SwitchFiles {
        per_profile,
        global,
    } = {
        let mut guard = state.profile_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err(PROFILES_NOT_LOADED.into());
        };
        open_profile_for_switch(set, name)?
    };

    // In loadout mode the profile file holds no aliases, triggers, or
    // macros, so the catalog fills the stores in the same step. Were a
    // save to find the stores empty in between, it would write an empty
    // catalog.
    let catalog = state.global_catalog.lock().await.clone();
    let loadouts = state.loadout_set.lock().await.clone();

    // Step 3: apply the per-profile file (or defaults) and then overlay
    // global.toml so theme/font/keep-last/auto-update/dock_layout
    // survive the switch.
    {
        let mut p = state.profile.lock().await;
        match per_profile {
            Some(snap) => {
                snap.apply_to(&mut p);
            }
            None => {
                let default = ProfileConfig::default();
                default.apply_to(&mut p);
            }
        }
        if let Some(g) = global {
            g.apply_to(&mut p);
        }
        if let Some(catalog) = &catalog {
            lay_catalog_over(&mut p, catalog, loadouts.as_ref());
        }
        // Under the same lock as the swap, so a pane layout write edited
        // from the old profile's tree is refused from here on.
        bump_panes_generation();
    }
    Ok(())
}

/// Shared body for switching the active profile. The
/// `profile_switch` Tauri command and the Char.Status auto-switch
/// path in `handle_char_known_for_auto_switch` both call this so the
/// persist + load + flip sequence stays identical. An error is a
/// sentence for you, and leaves the index and the live profile on the
/// profile you were using.
pub(crate) async fn apply_profile_switch(
    app: &AppHandle,
    state: &SharedState,
    name: &str,
) -> Result<(), String> {
    let app_data = app.path().app_data_dir().ok();
    switch_profile(state, app_data.as_deref(), name).await?;

    // Hand every window the new profile's panes and tracked affects
    // from here rather than leaving each window to re-fetch. The main
    // window's broadcast after its re-fetch diffs against its own last
    // snapshot, so it can skip a list that Settings changed meanwhile.
    // These go out before profile-switched so the stores already hold
    // the new values when windows react to the switch.
    broadcast_profile_ui(app, state).await;

    broadcast(app, "vosh://profile-switched", &name);
    Ok(())
}

/// Steps 1 to 3 of [`apply_profile_switch`] over the app data folder
/// `app_data`, so a test can run them over a folder of its own.
async fn switch_profile(
    state: &SharedState,
    app_data: Option<&std::path::Path>,
    name: &str,
) -> Result<(), String> {
    // Hold the persist lock from the flush through loading the next
    // file, so a Settings write to the incoming profile's file lands
    // either before the load reads it or after the switch made the
    // profile live, never in between.
    let _persist_guard = PERSIST_LOCK.lock().await;

    // Step 1: snapshot + write the CURRENT active profile so user
    // changes since the last persist are not lost on switch. Skipped
    // after a #profile reset/load: the live profile is deliberately
    // diverged from disk and a passive switch (the GMCP Char.Status
    // auto-switch reaches here too) must not write it back.
    if !AUTO_PERSIST_SUPPRESSED.load(std::sync::atomic::Ordering::Acquire) {
        persist_state(state, app_data).await;
    }

    switch_live_profile(state, name).await
}

#[tauri::command]
pub(crate) async fn profile_switch(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
) -> Result<(), String> {
    let shared: SharedState = state.inner().clone();
    apply_profile_switch(&app, &shared, &name).await
}

/// Called by the session GMCP handler when Char.Status or Char.Name
/// reports a character name. Suppresses duplicate observations so the
/// resolver does not re-run on every Char.Status tick, then resolves
/// (host, port, character) against the profile set. When the resolved
/// profile differs from the currently-active one, swap to it and
/// announce on the terminal so the user knows the active profile
/// changed. Either way, a new name updates the session identity.
pub(crate) async fn handle_char_known_for_auto_switch(
    app: &AppHandle,
    state: &SharedState,
    character: &str,
) {
    let trimmed = character.trim();
    if trimmed.is_empty() {
        return;
    }
    // Short-circuit on duplicate observations. Char.Status is sent on
    // every vitals update, so without this gate the resolver would
    // run every tick.
    let should_resolve = {
        let Ok(mut guard) = state.current_character.lock() else {
            return;
        };
        if guard.as_deref() == Some(trimmed) {
            false
        } else {
            *guard = Some(trimmed.to_string());
            true
        }
    };
    if !should_resolve {
        return;
    }
    auto_switch_for_character(app, state, trimmed).await;
    crate::characters::broadcast_session_identity(app, state).await;
}

/// Switch to the profile that claims `character` on the live
/// connection, when that is not the active one already.
async fn auto_switch_for_character(app: &AppHandle, state: &SharedState, character: &str) {
    let Some(new_name) = auto_switch_target(state, character).await else {
        return;
    };
    // A switch that fails leaves the live profile and the index as they
    // were, and says so on the terminal, since nothing else would tell
    // you the login kept the old profile.
    let line = match apply_profile_switch(app, state, &new_name).await {
        Ok(()) => auto_switch_line(&new_name),
        Err(e) => {
            warn!(error = %e, "auto profile switch failed");
            auto_switch_failed_line(&e)
        }
    };
    let _ = app.emit(
        "session://output",
        OutputPayload::from_bytes(line.as_bytes()),
    );
}

/// The profile that `character` logging in on the live connection
/// should load, when that is not the active one already.
async fn auto_switch_target(state: &SharedState, character: &str) -> Option<String> {
    let (host, port) = state
        .current_connection
        .lock()
        .ok()
        .and_then(|g| g.clone())?;
    let guard = state.profile_set.lock().await;
    let set = guard.as_ref()?;
    set.resolve_match(&host, port, Some(character))
        .filter(|name| name != set.active_name())
}

/// The terminal line that says a login switched the profile, in the
/// yellow that tick warnings use.
fn auto_switch_line(profile: &str) -> String {
    format!(
        "\r\n\x1b[33mVosh switched to the {} profile.\x1b[0m\r\n",
        crate::profile_set::display_name(profile)
    )
}

/// The terminal line that says a login switch did not happen, in the
/// same yellow. `error` is the sentence the switch returned.
fn auto_switch_failed_line(error: &str) -> String {
    format!("\r\n\x1b[33m{error}\x1b[0m\r\n")
}

#[tauri::command]
pub(crate) async fn map_walk_to(
    state: State<'_, SharedState>,
    target_id: i64,
) -> Result<(), String> {
    let path = {
        let guard = state.map.lock().await;
        let Some(map) = guard.as_ref() else {
            return Err("map not ready".to_string());
        };
        let Some(current) = map.current_room_id else {
            return Err("not in a known room".to_string());
        };
        map.store
            .find_path(current, target_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "no known path".to_string())?
    };
    if path.is_empty() {
        return Ok(());
    }
    let mut bytes = Vec::with_capacity(path.iter().map(|s| s.len() + 2).sum());
    for dir in path {
        bytes.extend_from_slice(dir.as_bytes());
        bytes.extend_from_slice(b"\r\n");
    }
    let session = state.session.lock().await;
    let Some(handle) = session.as_ref() else {
        return Err("not connected".to_string());
    };
    if !handle.send(bytes) {
        return Err("session task gone".to_string());
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn map_set_note(
    state: State<'_, SharedState>,
    room_id: i64,
    notes: String,
) -> Result<(), String> {
    let mut guard = state.map.lock().await;
    let Some(map) = guard.as_mut() else {
        return Err("map not ready".to_string());
    };
    map.store
        .set_note(room_id, &notes)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn map_set_avoid(
    state: State<'_, SharedState>,
    room_id: i64,
    avoid: bool,
) -> Result<(), String> {
    let mut guard = state.map.lock().await;
    let Some(map) = guard.as_mut() else {
        return Err("map not ready".to_string());
    };
    map.store
        .set_avoid(room_id, avoid)
        .map_err(|e| e.to_string())
}

/// Run `read` on the log store's read connection, or on the writer when
/// the read connection did not open. None when neither is open.
async fn read_logs<T>(state: &AppState, read: impl FnOnce(&vosh_log::LogStore) -> T) -> Option<T> {
    {
        let guard = state.log_reader.lock().await;
        if let Some(store) = guard.as_ref() {
            return Some(read(store));
        }
    }
    let guard = state.logs.lock().await;
    guard.as_ref().map(read)
}

#[tauri::command]
pub(crate) async fn logs_list_sessions(
    state: State<'_, SharedState>,
    limit: usize,
    hide_local: Option<bool>,
) -> Result<Vec<SessionRow>, String> {
    read_logs(&state, |store| {
        store.list_sessions(limit, hide_local.unwrap_or(false))
    })
    .await
    .unwrap_or_else(|| Ok(Vec::new()))
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn logs_search(
    state: State<'_, SharedState>,
    pattern: String,
    case_sensitive: bool,
    max_results: usize,
    session_id: Option<i64>,
) -> Result<Vec<SearchHit>, String> {
    let opts = SearchOptions {
        case_sensitive,
        max_results,
        session_id,
        ..SearchOptions::default()
    };
    read_logs(&state, |store| store.search(&pattern, &opts))
        .await
        .unwrap_or_else(|| Ok(Vec::new()))
        .map_err(|e| e.to_string())
}

/// One page of the Settings log view: the newest `max_results` matches
/// older than `before_line_id`, oldest first, and with `with_total` the
/// number of lines in that scope that match. The view leaves out
/// sessions to this machine with `hide_local`. A pattern the regex
/// engine cannot read comes back as an error starting `regex:`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn logs_search_page(
    state: State<'_, SharedState>,
    pattern: String,
    case_sensitive: bool,
    max_results: usize,
    session_id: Option<i64>,
    before_line_id: Option<i64>,
    hide_local: bool,
    with_total: bool,
) -> Result<SearchPage, String> {
    let opts = SearchOptions {
        case_sensitive,
        max_results,
        session_id,
        before_line_id,
        hide_local,
    };
    read_logs(&state, |store| {
        store.search_page(&pattern, &opts, with_total)
    })
    .await
    .unwrap_or_else(|| {
        Ok(SearchPage {
            hits: Vec::new(),
            total: with_total.then_some(0),
        })
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn logs_export(
    state: State<'_, SharedState>,
    session_id: i64,
    with_ansi: bool,
) -> Result<String, String> {
    read_logs(&state, |store| store.export_session(session_id, with_ansi))
        .await
        .ok_or_else(|| "log store not ready".to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn scrollback_load(
    state: State<'_, SharedState>,
    feed_native: bool,
) -> Result<ScrollbackLoad, String> {
    let sb = state.scrollback.lock().await;
    let bytes = sb.dump();
    // The native grid is fed only live output, so the persisted scrollback
    // would be missing there. The live pane asks us to seed it, and only
    // the first ask per process lands. A reloaded page asks again while
    // the grid still holds everything. The seed is claimed even when the
    // scrollback is empty, since the grid then gets every line live.
    #[cfg(native_surface)]
    let seeded_native = feed_native && crate::term_grid::claim_seed() && !bytes.is_empty();
    #[cfg(native_surface)]
    if seeded_native {
        crate::term_grid::feed_bytes(&bytes);
        crate::native_surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    let seeded_native = {
        let _ = feed_native;
        false
    };
    Ok(ScrollbackLoad {
        bytes,
        seeded_native,
    })
}

/// The persisted scrollback for a mounting terminal, and whether this call
/// also wrote it into the native grid. The page mirrors its restored banner
/// into the grid only when the seed landed here, so a reloaded page, whose
/// grid already holds the history and the first banner, adds no second one.
#[derive(Debug, serde::Serialize)]
pub(crate) struct ScrollbackLoad {
    pub bytes: Vec<u8>,
    pub seeded_native: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct UiConfigPayload {
    pub theme: String,
    pub follow_system_appearance: bool,
    pub light_theme: String,
    pub dark_theme: String,
    pub auto_update: bool,
    pub font_family: String,
    pub font_size: u32,
    pub terminal_line_height: String,
    pub tracked_affects: Vec<crate::profile_config::TrackedAffect>,
    pub enabled_presets: Vec<String>,
    pub keep_last_command: bool,
    pub theme_terminal_colors: Option<bool>,
    pub bright_bold: bool,
    pub terminal_base_ansi: Option<Vec<String>>,
    pub custom_themes: Vec<crate::profile_config::CustomTheme>,
    pub split_divider_color: Option<String>,
    pub input_echo_color: Option<String>,
    pub echo_macros: bool,
    pub side_panels_fill_height: bool,
    pub paste_line_delay_ms: u32,
    pub spellcheck_prompt: bool,
    pub input_cursor_style: String,
    pub prompt_template_enabled: bool,
    pub prompt_template: String,
    pub vitals: crate::profile_config::VitalsConfig,
    pub vitals_density: String,
    pub vitals_values: String,
    pub vitals_meter: String,
    pub vitals_warn_thirds: bool,
    pub moons_position: String,
    pub chip_style: String,
}

impl UiConfigPayload {
    /// The snapshot `ui_get_config` hands the frontend.
    pub(crate) fn from_ui(ui: &crate::profile_config::UiConfig) -> Self {
        Self {
            theme: ui.theme.clone(),
            follow_system_appearance: ui.follow_system_appearance,
            light_theme: ui.light_theme.clone(),
            dark_theme: ui.dark_theme.clone(),
            auto_update: ui.auto_update,
            font_family: ui.font_family.clone(),
            font_size: ui.font_size,
            terminal_line_height: ui.terminal_line_height.clone(),
            tracked_affects: ui.tracked_affects.clone(),
            enabled_presets: ui.enabled_presets.clone(),
            keep_last_command: ui.keep_last_command,
            theme_terminal_colors: ui.theme_terminal_colors,
            bright_bold: ui.bright_bold,
            terminal_base_ansi: ui.terminal_base_ansi.clone(),
            custom_themes: ui.custom_themes.clone(),
            split_divider_color: ui.split_divider_color.clone(),
            input_echo_color: ui.input_echo_color.clone(),
            echo_macros: ui.echo_macros,
            side_panels_fill_height: ui.side_panels_fill_height,
            paste_line_delay_ms: ui.paste_line_delay_ms,
            spellcheck_prompt: ui.spellcheck_prompt,
            input_cursor_style: ui.input_cursor_style.clone(),
            prompt_template_enabled: ui.prompt_template_enabled,
            prompt_template: ui.prompt_template.clone(),
            vitals: ui.vitals.clone(),
            vitals_density: ui.vitals_density.clone(),
            vitals_values: ui.vitals_values.clone(),
            vitals_meter: ui.vitals_meter.clone(),
            vitals_warn_thirds: ui.vitals_warn_thirds,
            moons_position: ui.moons_position.clone(),
            chip_style: ui.chip_style.clone(),
        }
    }

    /// Write every field onto the live UI config, normalizing as it
    /// goes. `ui_set_config` calls this, and each Settings tab saves the
    /// whole snapshot, so a field left out here would reset on the next
    /// save from any tab. `dock_layout` stays out on purpose because it
    /// travels through `dock_layout_get` and `dock_layout_set`.
    pub(crate) fn apply_to(self, ui: &mut crate::profile_config::UiConfig) {
        let UiConfigPayload {
            theme,
            follow_system_appearance,
            light_theme,
            dark_theme,
            auto_update,
            font_family,
            font_size,
            terminal_line_height,
            tracked_affects,
            enabled_presets,
            keep_last_command,
            theme_terminal_colors,
            bright_bold,
            terminal_base_ansi,
            custom_themes,
            split_divider_color,
            input_echo_color,
            echo_macros,
            side_panels_fill_height,
            paste_line_delay_ms,
            spellcheck_prompt,
            input_cursor_style,
            prompt_template_enabled,
            prompt_template,
            vitals,
            vitals_density,
            vitals_values,
            vitals_meter,
            vitals_warn_thirds,
            moons_position,
            chip_style,
        } = self;
        ui.theme = theme;
        ui.follow_system_appearance = follow_system_appearance;
        // An empty light theme falls back to Vellum. An empty dark theme
        // stays empty, which the frontend reads as the current theme.
        ui.light_theme = match light_theme.trim() {
            "" => "vellum".to_string(),
            id => id.to_string(),
        };
        ui.dark_theme = dark_theme.trim().to_string();
        ui.auto_update = auto_update;
        ui.font_family = font_family;
        ui.font_size = font_size.clamp(6, 64);
        ui.terminal_line_height =
            crate::profile_config::coerce_terminal_line_height(terminal_line_height);
        ui.tracked_affects = crate::profile_config::normalize_tracked_affects(tracked_affects);
        ui.enabled_presets = enabled_presets
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        ui.enabled_presets.sort();
        ui.enabled_presets.dedup();
        ui.keep_last_command = keep_last_command;
        ui.theme_terminal_colors = theme_terminal_colors;
        ui.bright_bold = bright_bold;
        ui.terminal_base_ansi = terminal_base_ansi;
        ui.custom_themes = custom_themes;
        // Empty strings get normalized to None so the picker can clear
        // back to the theme default by submitting "".
        ui.split_divider_color = split_divider_color.and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        ui.input_echo_color = input_echo_color.and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        ui.echo_macros = echo_macros;
        ui.side_panels_fill_height = side_panels_fill_height;
        // Clamp to a sane range so a malformed input cannot freeze the
        // paste indicator (0–10s per line is plenty).
        ui.paste_line_delay_ms = paste_line_delay_ms.min(10_000);
        ui.spellcheck_prompt = spellcheck_prompt;
        // Coerce an unknown caret shape (hand-edited profile.toml, or a
        // value from a newer build) back to the default so the input
        // row always paints something.
        ui.input_cursor_style = match input_cursor_style.as_str() {
            "block_outline" | "half_block" | "underline" | "underline_thick" | "pipe"
            | "pipe_thick" => input_cursor_style,
            _ => "block".to_string(),
        };
        ui.prompt_template_enabled = prompt_template_enabled;
        ui.prompt_template = prompt_template;
        ui.vitals_density = crate::profile_config::coerce_vitals_density(vitals_density);
        ui.vitals_values = crate::profile_config::coerce_vitals_values(vitals_values);
        ui.vitals_meter = crate::profile_config::coerce_vitals_meter(vitals_meter);
        ui.vitals_warn_thirds = vitals_warn_thirds;
        // Normalize vitals glyphs + width. Empty glyph strings would
        // render zero-width bars; collapse to the default in that case
        // so the user cannot accidentally hide the bar via a typo.
        // Also coerce unknown layout / percent_color values back to
        // the defaults so a hand-edited profile.toml typo does not
        // break the panel render.
        let mut v = vitals;
        if v.bar_filled.is_empty() {
            v.bar_filled = "▰".to_string();
        }
        if v.bar_empty.is_empty() {
            v.bar_empty = "▱".to_string();
        }
        v.bar_width = v.bar_width.clamp(4, 60);
        // Every layout the settings picker offers has to be listed
        // here. The four Ember layouts (ember/ledger, gauges, pips,
        // strip) join the legacy stacked / inline pair; anything else
        // is a hand-edited typo and falls back to the default. This
        // list went stale once already when gauges / pips / strip
        // shipped, which silently reset every pick to the ledger.
        v.layout = crate::profile_config::coerce_vitals_layout(v.layout);
        if v.percent_color != "fill" && v.percent_color != "gradient" {
            v.percent_color = "fill".to_string();
        }
        // Legacy `bar_style: "spark"` migrates to solid + history
        // layout. The spark mode was reframed as a layout that wraps
        // any bar style with a braille trend grid below.
        if v.bar_style == "spark" {
            v.bar_style = "solid".to_string();
            v.bar_layout = "with_history".to_string();
        }
        if v.bar_style != "solid" && v.bar_style != "track" && v.bar_style != "ramped" {
            v.bar_style = "solid".to_string();
        }
        if v.bar_layout != "plain" && v.bar_layout != "with_history" {
            v.bar_layout = "plain".to_string();
        }
        ui.vitals = v;
        // Coerce an unknown moons_position value back to "right-edge"
        // so a hand-edited profile.toml typo cannot leave the status
        // bar rendering moons in an unrecognized slot.
        ui.moons_position = match moons_position.as_str() {
            "before-time" | "after-time" | "right-edge" => moons_position,
            _ => "right-edge".to_string(),
        };
        // Same coercion for chip_style — an unknown variant from a
        // hand-edited profile.toml falls back to the default rather
        // than letting the frontend render a chip with no style.
        ui.chip_style = match chip_style.as_str() {
            "value_only" | "caption_value" | "icon_value" => chip_style,
            _ => "value_only".to_string(),
        };
    }
}

#[tauri::command]
pub(crate) async fn ui_get_config(
    state: State<'_, SharedState>,
) -> Result<UiConfigPayload, String> {
    let p = state.profile.lock().await;
    Ok(UiConfigPayload::from_ui(&p.ui))
}

#[tauri::command]
pub(crate) async fn ui_set_config(
    app: AppHandle,
    state: State<'_, SharedState>,
    config: UiConfigPayload,
) -> Result<(), String> {
    {
        let mut p = state.profile.lock().await;
        config.apply_to(&mut p.ui);
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    Ok(())
}

/// Replace a profile's tracked affects without touching the rest of
/// its UI config, so an editor outside Settings cannot write a stale
/// snapshot over other fields. Returns the normalized list.
///
/// With no `profile`, or the active one, the live profile takes the
/// list, persists, and broadcasts it as `vosh://tracked-affects-changed`
/// to every window. An inactive `profile` has its file rewritten
/// instead, and only `vosh://profile-changed` goes out, since the
/// tracked affects event would hand another profile's list to the main
/// window's store.
#[tauri::command]
pub(crate) async fn tracked_affects_set(
    app: AppHandle,
    state: State<'_, SharedState>,
    list: Vec<crate::profile_config::TrackedAffect>,
    profile: Option<String>,
) -> Result<Vec<crate::profile_config::TrackedAffect>, String> {
    let list = crate::profile_config::normalize_tracked_affects(list);
    let shared: SharedState = state.inner().clone();
    if let Some(name) = profile.as_deref() {
        let written = crate::characters::edit_inactive_profile(&shared, name, |_, config| {
            config.ui.tracked_affects.clone_from(&list);
        })
        .await?;
        if written.is_some() {
            crate::characters::broadcast_profile_changed(&app, name);
            return Ok(list);
        }
    }
    {
        let mut p = state.profile.lock().await;
        p.ui.tracked_affects.clone_from(&list);
    }
    persist_profile(&app, &shared).await;
    broadcast(&app, "vosh://tracked-affects-changed", &list);
    if let Some(active) = crate::characters::active_name(&shared).await {
        crate::characters::broadcast_profile_changed(&app, &active);
    }
    Ok(list)
}

/// Replace the active profile's theme choice without touching the rest
/// of the UI config. The main window's palette picks a theme while the
/// Settings window may hold its own full snapshot, so a whole config
/// write from one would overwrite the other's newer fields. The caller
/// applies and broadcasts the theme itself. While follow system
/// appearance is on, a pick fills the light or dark slot instead, so the
/// caller also sends the pair.
#[tauri::command]
pub(crate) async fn ui_set_theme(
    app: AppHandle,
    state: State<'_, SharedState>,
    theme: String,
    light_theme: Option<String>,
    dark_theme: Option<String>,
) -> Result<(), String> {
    {
        let mut p = state.profile.lock().await;
        if !apply_theme_pick(&mut p.ui, theme, light_theme, dark_theme) {
            return Ok(());
        }
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    Ok(())
}

/// Write a theme pick onto the live UI config. A missing or blank pair
/// entry leaves that slot alone. Returns whether anything changed, so an
/// unchanged pick skips the save.
fn apply_theme_pick(
    ui: &mut crate::profile_config::UiConfig,
    theme: String,
    light_theme: Option<String>,
    dark_theme: Option<String>,
) -> bool {
    let mut changed = false;
    let mut set = |slot: &mut String, value: String| {
        if !value.is_empty() && *slot != value {
            *slot = value;
            changed = true;
        }
    };
    set(&mut ui.theme, theme);
    if let Some(v) = light_theme {
        set(&mut ui.light_theme, v);
    }
    if let Some(v) = dark_theme {
        set(&mut ui.dark_theme, v);
    }
    changed
}

/// Bulk-install a set of preset triggers. Each trigger should already
/// have its `preset` field set to the preset id; this command
/// validates and inserts them so the engine starts matching
/// immediately. Returns the number installed.
#[tauri::command]
pub(crate) async fn presets_install(
    app: AppHandle,
    state: State<'_, SharedState>,
    triggers: Vec<Trigger>,
) -> Result<usize, String> {
    let mut installed = 0usize;
    {
        let mut p = state.profile.lock().await;
        for mut t in triggers {
            // The startup re-install overwrites same-named presets so
            // pattern/template updates land, but the group is the user's
            // organization: carry it over so putting a preset into a group
            // survives relaunch.
            if t.group.is_none() {
                if let Some(existing) = p.triggers.get(&t.name) {
                    t.group.clone_from(&existing.group);
                }
            }
            p.triggers.set(t).map_err(|e| e.to_string())?;
            installed += 1;
        }
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    if installed > 0 {
        broadcast_list_changes(&app, ListChanges::TRIGGERS);
    }
    Ok(installed)
}

/// Remove every trigger tagged with the given preset id. Returns the
/// number removed.
#[tauri::command]
pub(crate) async fn presets_remove(
    app: AppHandle,
    state: State<'_, SharedState>,
    preset_id: String,
) -> Result<usize, String> {
    let removed = {
        let mut p = state.profile.lock().await;
        p.triggers.remove_by_preset(&preset_id)
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    if removed > 0 {
        broadcast_list_changes(&app, ListChanges::TRIGGERS);
    }
    Ok(removed)
}

#[derive(serde::Serialize)]
pub(crate) struct UpdateCheckResult {
    pub available: bool,
    pub version: Option<String>,
    pub notes: Option<String>,
}

#[derive(serde::Serialize)]
pub(crate) struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub entry: String,
    pub dir: String,
    pub enabled: bool,
}

impl From<&PluginRecord> for PluginInfo {
    fn from(p: &PluginRecord) -> Self {
        Self {
            name: p.manifest.name.clone(),
            version: p.manifest.version.clone(),
            description: p.manifest.description.clone(),
            author: p.manifest.author.clone(),
            entry: p.manifest.entry.clone(),
            dir: p.dir.display().to_string(),
            enabled: p.enabled,
        }
    }
}

#[tauri::command]
pub(crate) async fn plugins_list(state: State<'_, SharedState>) -> Result<Vec<PluginInfo>, String> {
    let mut mgr = state.plugins.lock().await;
    mgr.discover().map_err(|e| e.to_string())?;
    Ok(mgr.list().iter().map(PluginInfo::from).collect())
}

#[tauri::command]
pub(crate) async fn plugins_set_enabled(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
    enabled: bool,
) -> Result<bool, String> {
    let body = if enabled {
        let mgr = state.plugins.lock().await;
        if !mgr.list().iter().any(|p| p.manifest.name == name) {
            return Err(format!("plugin `{name}` not found"));
        }
        Some(mgr.read_entry(&name).map_err(|e| e.to_string())?)
    } else {
        None
    };

    {
        let mut mgr = state.plugins.lock().await;
        if !mgr.mark_enabled(&name, enabled) {
            return Err(format!("plugin `{name}` not found"));
        }
        let mut p = state.profile.lock().await;
        p.plugins.enabled = mgr.enabled_names();
    }

    let mut lists = ListChanges::default();
    if let Some(code) = body {
        let mut p = state.profile.lock().await;
        crate::script_state::snapshot_vars(&p.script, &p.vars);
        let outcome = p
            .script
            .load_script(&format!("plugin:{name}"), code)
            .map_err(|e| e.to_string())?;
        lists = crate::script_state::apply_actions(&mut p, outcome).lists;
    }
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast_list_changes(&app, lists);
    Ok(enabled)
}

#[tauri::command]
pub(crate) async fn plugins_reload(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
) -> Result<(), String> {
    let code = {
        let mgr = state.plugins.lock().await;
        mgr.read_entry(&name).map_err(|e| e.to_string())?
    };
    let lists = {
        let mut p = state.profile.lock().await;
        crate::script_state::snapshot_vars(&p.script, &p.vars);
        let outcome = p
            .script
            .load_script(&format!("plugin:{name}"), code)
            .map_err(|e| e.to_string())?;
        crate::script_state::apply_actions(&mut p, outcome).lists
    };
    broadcast_list_changes(&app, lists);
    Ok(())
}

#[tauri::command]
pub(crate) async fn updater_check(app: AppHandle) -> Result<UpdateCheckResult, String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => Ok(UpdateCheckResult {
            available: true,
            version: Some(update.version.clone()),
            notes: update.body.clone(),
        }),
        Ok(None) => Ok(UpdateCheckResult {
            available: false,
            version: None,
            notes: None,
        }),
        Err(e) => Err(e.to_string()),
    }
}

/// Read-only Path B migration preview. Walks the current profile set,
/// loads each per-profile [`ProfileConfig`] off disk, and runs the
/// analyzer in [`crate::migration`]. Returns the full plan: every
/// auto-resolved item, every conflict (one entry per name with two or
/// more diverging variants), and the per-source-profile loadouts the
/// migration would generate. Nothing is written to disk; the wizard
/// uses this for the preview pane only. The companion
/// [`migration_apply`] command commits the plan once the user picks
/// winners for any conflicts. Refused while a profile file did not read
/// at launch, or while catalog.toml or loadouts.toml is on disk, see
/// [`crate::loadout_store::migration_refusal`].
#[tauri::command]
pub(crate) async fn migration_analyze(
    app: AppHandle,
    state: State<'_, SharedState>,
) -> Result<crate::migration::MigrationPlan, String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    analyze_migration(&state, &app_data).await
}

/// [`migration_analyze`] over the app data folder `app_data`, so a test
/// can run it over a folder of its own.
async fn analyze_migration(
    state: &SharedState,
    app_data: &std::path::Path,
) -> Result<crate::migration::MigrationPlan, String> {
    if let Some(reason) = crate::loadout_store::migration_refusal(app_data) {
        return Err(reason.into());
    }
    let guard = state.profile_set.lock().await;
    let Some(set) = guard.as_ref() else {
        return Err(PROFILES_NOT_LOADED.into());
    };
    let sources = migration_sources(set)?;
    Ok(crate::migration::analyze_profiles(&sources.profiles))
}

/// What the shared catalog wizard reads, see [`migration_sources`].
struct MigrationSources {
    /// Every profile in index order with what its file holds.
    profiles: Vec<(String, ProfileConfig)>,
    /// The enabled preset list of each profile that saved a file.
    preset_lists: Vec<Vec<String>>,
    /// The file of every profile, in index order.
    files: Vec<MigrationFile>,
}

/// The file of one profile, as the wizard read it.
struct MigrationFile {
    name: String,
    path: std::path::PathBuf,
    /// What the file held, or None for a profile that never saved one.
    text: Option<String>,
}

/// Every profile in index order with what its file holds, for the shared
/// catalog wizard, the enabled preset list of each profile that saved a
/// file, and the text of each file. A profile that never saved a file
/// brings the defaults and no preset list, the way launch leaves it out.
/// A file that does not read stops the wizard, since the catalog would
/// miss its items. So does a file Vosh could not read at launch, since
/// the wizard rewrites every profile file and Vosh never saves over one
/// of those.
fn migration_sources(set: &crate::profile_set::ProfileSet) -> Result<MigrationSources, String> {
    let mut sources = MigrationSources {
        profiles: Vec::with_capacity(set.list().len()),
        preset_lists: Vec::new(),
        files: Vec::new(),
    };
    for entry in set.list() {
        let path = set.profile_path(&entry.name);
        if crate::profile_config::is_unread(&path) {
            return Err(format!(
                "Vosh could not read the {} profile file when it started, so it will not change \
                 the file. Restart Vosh and try again.",
                crate::profile_set::display_name(&entry.name)
            ));
        }
        let text = if path.exists() {
            Some(
                std::fs::read_to_string(&path)
                    .map_err(|e| crate::profile_config::ConfigError::from(e).to_string())?,
            )
        } else {
            None
        };
        let cfg = match &text {
            Some(text) => {
                let cfg = ProfileConfig::from_toml(text).map_err(|e| e.to_string())?;
                sources.preset_lists.push(cfg.ui.enabled_presets.clone());
                cfg
            }
            None => ProfileConfig::default(),
        };
        sources.profiles.push((entry.name.clone(), cfg));
        sources.files.push(MigrationFile {
            name: entry.name.clone(),
            path,
            text,
        });
    }
    Ok(sources)
}

/// One conflict resolution from the wizard. Identifies a single
/// conflicted item (kind + name) and the source profile whose variant
/// should win. Resolutions not present in the list fall back to the
/// first variant in the conflict (the analyzer iterates source
/// profiles in index order, so this is deterministic).
#[derive(Debug, serde::Deserialize)]
pub(crate) struct ConflictResolution {
    pub kind: crate::migration::ItemKind,
    pub name: String,
    pub source_profile: String,
}

/// Commit the Path B migration. Re-runs the analyzer, applies the
/// user's per-conflict resolutions (or the first-variant default for
/// any missing resolution), copies every existing per-profile file into
/// `profiles/legacy/`, writes `catalog.toml` + `loadouts.toml`, takes the
/// aliases, triggers, and macros out of each profile file, which keeps
/// every other setting, and asks for a relaunch so the startup hook
/// picks up Path B mode. The previously-active profile name (from the
/// index) becomes the sole initial active loadout so the user's first
/// post-restart session keeps the same authoring set live. A write that
/// fails puts back every file the run changed, so you stay in per
/// profile mode and can run it again. Refused while a profile file did
/// not read at launch, or while catalog.toml or loadouts.toml is on
/// disk, see [`crate::loadout_store::migration_refusal`].
#[tauri::command]
pub(crate) async fn migration_apply(
    app: AppHandle,
    state: State<'_, SharedState>,
    resolutions: Vec<ConflictResolution>,
) -> Result<(), String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    apply_migration(&state, &app_data, &resolutions, || {
        // Path B is now on disk but the live session still holds the
        // pre-migration profile. Block every persist until the relaunch
        // loads the catalog, and flip the input layer into Path B mode
        // so the legacy #profile trio stops writing files.
        MIGRATION_RELAUNCH_PENDING.store(true, std::sync::atomic::Ordering::Release);
        crate::input::PATH_B_ACTIVE.store(true, std::sync::atomic::Ordering::Release);
    })
    .await?;

    // Returning Ok rather than calling `app.restart()` here. Restart
    // is fragile in dev mode: it tears down the binary out from under
    // the `tauri dev` watcher and leaves the next process trying to
    // load a frontend whose Vite dev server may have been killed
    // with the parent, ending in a hidden window with no JS reveal.
    // The frontend shows a "migration complete, please relaunch"
    // banner and offers an explicit [Quit Vosh] button (handled
    // separately by app_quit) that cleanly exits the process. The
    // user re-opens Vosh and the Path B startup hook picks the new
    // catalog up. Path B mode is durable on disk either way.
    let _ = app.emit("vosh://migration-applied", &());
    Ok(())
}

/// [`migration_apply`] over the app data folder `app_data`, so a test
/// can run it over a folder of its own. `written` runs once catalog.toml,
/// loadouts.toml, and every profile file are on disk. A write that fails
/// puts back every file the run changed and skips `written`, unless
/// catalog.toml stays on disk.
async fn apply_migration(
    state: &SharedState,
    app_data: &std::path::Path,
    resolutions: &[ConflictResolution],
    written: impl FnOnce(),
) -> Result<(), String> {
    // Every save of a profile file takes this lock, so none lands
    // between the read of a file below and its rewrite without the items.
    let _persist_guard = PERSIST_LOCK.lock().await;
    if let Some(reason) = crate::loadout_store::migration_refusal(app_data) {
        return Err(reason.into());
    }

    // Re-load sources from disk — the analyze call has to walk the
    // same set the user just previewed, but a few seconds may have
    // passed and we want the fresh snapshot rather than caching across
    // commands.
    let (previously_active, sources) = {
        let guard = state.profile_set.lock().await;
        let Some(set) = guard.as_ref() else {
            return Err(PROFILES_NOT_LOADED.into());
        };
        (set.active_name().to_string(), migration_sources(set)?)
    };
    let live_presets = state.profile.lock().await.ui.enabled_presets.clone();

    let plan = crate::migration::analyze_profiles(&sources.profiles);
    let mut catalog = plan.auto_resolved;
    // The catalog owns which presets are on. It takes the list here, by
    // the rule launch uses, so the first launch in loadout mode keeps on
    // every preset any character had on and nothing more.
    catalog.enabled_presets = Some(crate::loadout_store::first_catalog_presets(
        &sources.preset_lists,
        &live_presets,
    ));
    for conflict in &plan.conflicts {
        let chosen_source = resolutions
            .iter()
            .find(|r| r.kind == conflict.kind && r.name == conflict.name)
            .map_or_else(
                || conflict.variants[0].source_profile.as_str(),
                |r| r.source_profile.as_str(),
            );
        let chosen = conflict
            .variants
            .iter()
            .find(|v| v.source_profile == chosen_source)
            .ok_or_else(|| {
                format!(
                    "resolution for `{}` points to unknown source `{}`",
                    conflict.name, chosen_source
                )
            })?;
        match &chosen.item {
            crate::migration::ItemPayload::Alias { item } => catalog.aliases.push(item.clone()),
            crate::migration::ItemPayload::Trigger { item } => catalog.triggers.push(item.clone()),
            crate::migration::ItemPayload::Macro { item } => catalog.macros.push(item.clone()),
        }
    }

    let mut loadout_set = crate::loadout::LoadoutSet {
        loadouts: plan.loadouts,
        active: Vec::new(),
        dormant: false,
    };
    if loadout_set
        .loadouts
        .iter()
        .any(|l| l.name == previously_active)
    {
        loadout_set.active.push(previously_active);
    }

    // Each profile file stays where it is and keeps every setting of its
    // profile, its timers, variables, tick, panels, theme, and vitals
    // among them, since loadout mode reads them from there at launch and
    // on a switch. Only the aliases, triggers, and macros leave it, as
    // the catalog holds them now. A file that kept them would lay its
    // copies, with their old group names, over the catalog at launch.
    // Its group checkbox lists name the catalog groups its loadout
    // leaves off, see `migration::profile_file_for_catalog`, and a
    // profile that never saved a file gets one when it has lists to
    // keep. The file keeps its own enabled preset list, which loadout
    // mode replaces with the catalog's at every load. Everything is built
    // before the first write, so a file that does not serialize changes
    // nothing.
    let mut kept = Vec::with_capacity(sources.files.len());
    for file in &sources.files {
        let mut config = match &file.text {
            Some(text) => ProfileConfig::from_toml(text).map_err(|e| e.to_string())?,
            None => ProfileConfig::default(),
        };
        let own = crate::loadout::Loadout::empty(file.name.as_str());
        let loadout = loadout_set.get(&file.name).unwrap_or(&own);
        crate::migration::profile_file_for_catalog(&mut config, &catalog, loadout);
        let lists = !config.disabled_alias_groups.is_empty()
            || !config.disabled_trigger_groups.is_empty()
            || !config.disabled_macro_groups.is_empty();
        if file.text.is_some() || lists {
            kept.push((file, config.to_toml().map_err(|e| e.to_string())?));
        }
    }

    // A full copy of each file first, so the files as they were wait in
    // profiles/legacy before anything changes. A copy an earlier run
    // left there moves to a backup beside it.
    let legacy_dir = app_data.join("profiles").join("legacy");
    for file in &sources.files {
        let Some(text) = &file.text else {
            continue;
        };
        let name = file.path.file_name().unwrap_or_default();
        crate::profile_config::write_with_backup(&legacy_dir.join(name), text).map_err(|e| {
            format!(
                "Vosh could not copy {} into profiles/legacy and changed nothing ({e}).",
                name.to_string_lossy()
            )
        })?;
    }

    // Then the catalog, the loadouts, and each profile file without its
    // items. A write that fails puts back every file this run changed,
    // so Vosh stays in per profile mode and the wizard can run again. The
    // copies in legacy stay.
    let mut touched = Vec::new();
    if let Err((what, e)) =
        write_shared_catalog(app_data, &catalog, &loadout_set, &kept, &mut touched)
    {
        crate::profile_config::put_back(&touched);
        let restored = touched.iter().all(|(path, before)| match before {
            Some(text) => std::fs::read_to_string(path).ok().as_deref() == Some(text.as_str()),
            None => !path.exists(),
        });
        // A catalog left on disk starts loadout mode at the next launch,
        // so the live profile must not save until then.
        if crate::loadout_store::path_b_mode_active(app_data) {
            written();
        }
        return Err(if restored {
            format!(
                "Vosh could not save {what} ({e}), so it put back every file it changed. Your \
                 profiles work as before, and you can try again."
            )
        } else {
            format!(
                "Vosh could not save {what} ({e}) and could not put back every file it changed. \
                 A full copy of each profile file waits in profiles/legacy. Quit Vosh and open \
                 it again."
            )
        });
    }
    written();
    Ok(())
}

/// Save what the shared catalog wizard built, catalog.toml, then
/// loadouts.toml, then each profile file in `kept` with its new text.
/// Notes in `touched` each file it is about to write with what the file
/// held before, None for a file that was not there, so a failure can put
/// them back. On a failure, returns the file that did not save, as words
/// for you, and the error.
fn write_shared_catalog(
    app_data: &std::path::Path,
    catalog: &crate::loadout::GlobalCatalog,
    loadouts: &crate::loadout::LoadoutSet,
    kept: &[(&MigrationFile, String)],
    touched: &mut Vec<(std::path::PathBuf, Option<String>)>,
) -> Result<(), (String, String)> {
    // The wizard refuses to run while either file is on disk.
    touched.push((crate::loadout_store::catalog_path(app_data), None));
    crate::loadout_store::save_global_catalog(app_data, catalog)
        .map_err(|e| ("catalog.toml".to_string(), e.to_string()))?;
    touched.push((crate::loadout_store::loadouts_path(app_data), None));
    crate::loadout_store::save_loadout_set(app_data, loadouts)
        .map_err(|e| ("loadouts.toml".to_string(), e.to_string()))?;
    for (file, text) in kept {
        touched.push((file.path.clone(), file.text.clone()));
        crate::profile_config::write_with_backup(&file.path, text).map_err(|e| {
            (
                format!(
                    "the {} profile file",
                    crate::profile_set::display_name(&file.name)
                ),
                e.to_string(),
            )
        })?;
    }
    Ok(())
}

/// Cleanly exit the app. Surfaces a "quit" event first so any window
/// can flush state, then calls `app.exit(0)`. Used by the post-
/// migration prompt to take the user out of the legacy-mode session
/// in one click; on relaunch the Path B startup hook picks up the
/// new catalog.
#[tauri::command]
pub(crate) async fn app_quit(app: AppHandle) -> Result<(), String> {
    // No explicit persist here: `app.exit` raises `RunEvent::ExitRequested`,
    // whose handler asks the windows for their pending writes and then
    // flushes the profile exactly once (with a timeout), see exit_flush.rs.
    app.exit(0);
    Ok(())
}

/// One loadout as the frontend cares about it: the user-visible
/// identifying fields, the `enabled_groups` list (chips for the picker),
/// and the auto-match block.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct LoadoutSummary {
    pub name: String,
    pub description: Option<String>,
    pub enabled_groups: Vec<String>,
    pub auto_match: Option<crate::profile_set::AutoMatch>,
}

/// Shape returned by [`loadouts_get_state`]. Carries the active list,
/// the full loadout summaries, and a `path_b_active` flag so the
/// frontend can decide whether to render the Loadouts tab at all.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct LoadoutsState {
    pub path_b_active: bool,
    pub active: Vec<String>,
    pub loadouts: Vec<LoadoutSummary>,
}

/// Snapshot the current Path B loadout state for the Settings UI.
/// In legacy mode returns `path_b_active: false` plus empty lists so
/// the frontend can hide the Loadouts tab. In Path B mode the
/// active list and every loadout's summary come from the
/// `state.loadout_set` mutex.
#[tauri::command]
pub(crate) async fn loadouts_get_state(
    state: State<'_, SharedState>,
) -> Result<LoadoutsState, String> {
    let guard = state.loadout_set.lock().await;
    let Some(set) = guard.as_ref() else {
        return Ok(LoadoutsState {
            path_b_active: false,
            active: Vec::new(),
            loadouts: Vec::new(),
        });
    };
    let summaries: Vec<LoadoutSummary> = set
        .loadouts
        .iter()
        .map(|l| LoadoutSummary {
            name: l.name.clone(),
            description: l.description.clone(),
            enabled_groups: l.enabled_groups.clone(),
            auto_match: l.auto_match.clone(),
        })
        .collect();
    Ok(LoadoutsState {
        path_b_active: true,
        active: set.active.clone(),
        loadouts: summaries,
    })
}

/// Replace the active-loadouts list and reapply group state: the
/// union rule while loadouts are active, full dormancy when the user
/// deactivates everything. Persists the loadout set to disk and emits
/// a state-changed event so other windows (e.g. a future `TopBar`
/// checklist) see the update.
#[tauri::command]
pub(crate) async fn loadouts_set_active(
    app: AppHandle,
    state: State<'_, SharedState>,
    active: Vec<String>,
) -> Result<(), String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    {
        let mut guard = state.loadout_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err("Path B not active".into());
        };
        // Filter to known loadout names. A stale name (e.g. from a
        // future-truncated payload) is silently dropped rather than
        // returning an error.
        set.active = active
            .into_iter()
            .filter(|n| set.loadouts.iter().any(|l| &l.name == n))
            .collect();
        // Deactivate-all is the documented kill switch ("Activate none
        // to keep the catalog dormant"). Recorded as an explicit flag:
        // an empty active list on its own is ambiguous with "loadouts
        // have no opinion", and the other apply points (startup,
        // profile switch) must be able to re-impose dormancy.
        set.dormant = set.active.is_empty();
        let snapshot = set.clone();
        let mut p = state.profile.lock().await;
        crate::loadout_store::apply_effective_state(&snapshot, &mut p);
        if let Err(e) = crate::loadout_store::save_loadout_set(&app_data, &snapshot) {
            warn!(error = %e, "loadouts.toml save failed");
        }
    }
    // The recomputed (or dormant) disabled lists live in the profile
    // snapshot on disk; queue a persist so a crash before the exit
    // flush cannot leave loadouts.toml and per-profile state
    // disagreeing. Also clears any stale persist suppression — this is
    // a durable change the user asked for.
    mark_profile_dirty(&app);
    let _ = app.emit("vosh://loadouts-changed", &());
    Ok(())
}

/// Snapshot of the per-session tick timer config. Mirrors
/// `tick::TickConfig` with `Duration` flattened to a `u64` of seconds
/// so the frontend can edit it cleanly. Reset pattern, auto-fire
/// command, warning timer / message / color are all optional — empty
/// means the feature is off.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct TickConfigPayload {
    pub enabled: bool,
    pub interval_secs: u64,
    pub auto_fire: Option<String>,
    pub sound: bool,
    pub reset_pattern: Option<String>,
    pub warn_at_secs: Option<u64>,
    pub warn_message: Option<String>,
    pub warn_color: Option<String>,
}

/// Read the live tick configuration.
#[tauri::command]
pub(crate) async fn tick_get_config(
    state: State<'_, SharedState>,
) -> Result<TickConfigPayload, String> {
    let p = state.profile.lock().await;
    let cfg = &p.tick.config;
    Ok(TickConfigPayload {
        enabled: cfg.enabled,
        interval_secs: cfg.interval.as_secs(),
        auto_fire: cfg.auto_fire.clone(),
        sound: cfg.sound,
        reset_pattern: cfg.reset_pattern.clone(),
        warn_at_secs: cfg.warn_at_secs,
        warn_message: cfg.warn_message.clone(),
        warn_color: cfg.warn_color.clone(),
    })
}

/// Apply a new tick configuration through [`apply_tick_config`], which
/// changes every field or none. Persists the active profile and
/// broadcasts `vosh://tick-config-changed` only after the whole
/// configuration applied.
#[tauri::command]
pub(crate) async fn tick_set_config(
    app: AppHandle,
    state: State<'_, SharedState>,
    config: TickConfigPayload,
) -> Result<TickConfigPayload, String> {
    let snapshot = {
        let mut p = state.profile.lock().await;
        apply_tick_config(&mut p.tick, &config, tokio::time::Instant::now())?
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast(&app, "vosh://tick-config-changed", &snapshot);
    Ok(snapshot)
}

/// The error `tick_set_config` returns for a Reset on pattern that does
/// not compile.
const TICK_RESET_PATTERN_ERROR: &str =
    "Vosh could not read the Reset on pattern. Check it and save again.";

/// Apply a tick configuration from Settings to `tick`. Checks the Reset
/// on pattern before it changes anything, so a pattern that does not
/// compile leaves the running tick exactly as it was and returns a
/// sentence. Routes interval changes through `TickRuntime::set_interval`
/// so the next-fire deadline rebuilds. Other fields are direct
/// assignments. Returns the configuration as it now reads.
fn apply_tick_config(
    tick: &mut crate::tick::TickRuntime,
    config: &TickConfigPayload,
    now: tokio::time::Instant,
) -> Result<TickConfigPayload, String> {
    // Normalize string options: empty / whitespace-only -> None so the
    // persisted state does not carry an empty placeholder.
    let auto_fire = config
        .auto_fire
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let reset_pattern = config
        .reset_pattern
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let warn_message = config
        .warn_message
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let warn_color = config
        .warn_color
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    // Everything that can fail runs before the first change.
    let reset_regex =
        crate::tick::compile_reset_pattern(reset_pattern.as_deref()).map_err(|e| {
            warn!(error = %e, "tick reset pattern did not compile");
            TICK_RESET_PATTERN_ERROR.to_string()
        })?;

    if config.enabled {
        if !tick.config.enabled {
            tick.enable(now);
        }
        tick.set_interval(config.interval_secs, now);
    } else {
        tick.disable();
        // Still record the interval so the user can flip enabled
        // back on without re-typing it.
        tick.config.interval = std::time::Duration::from_secs(config.interval_secs.max(1));
    }
    tick.set_compiled_reset_pattern(reset_pattern.clone(), reset_regex);
    tick.config.auto_fire.clone_from(&auto_fire);
    tick.config.sound = config.sound;
    tick.config.warn_at_secs = config.warn_at_secs.filter(|s| *s > 0);
    tick.config.warn_message.clone_from(&warn_message);
    tick.config.warn_color.clone_from(&warn_color);

    Ok(TickConfigPayload {
        enabled: tick.config.enabled,
        interval_secs: tick.config.interval.as_secs(),
        auto_fire,
        sound: tick.config.sound,
        reset_pattern,
        warn_at_secs: tick.config.warn_at_secs,
        warn_message,
        warn_color,
    })
}

/// Download + install the pending update and restart the app. Errors
/// surface to the frontend; the relaunch is a hard exit so any UI
/// confirmation has to happen before this call returns.
#[tauri::command]
pub(crate) async fn updater_install_and_relaunch(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no update available".to_string())?;
    // Progress callbacks are no-ops at this stage; can be wired to
    // session://event later for a download progress bar.
    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::{settings_window_fit, ScrollbackLoad, UiConfigPayload};
    use crate::profile_config::{ProfileConfig, UiConfig};
    use crate::profile_set::tests::james_like_set;
    use crate::profile_set::{ProfileSet, DEFAULT_PROFILE_NAME};

    /// Send `ui` the way Settings does: out through `ui_get_config`,
    /// across the JSON bridge, and back through `ui_set_config` onto a
    /// fresh config.
    fn through_payload(ui: &UiConfig) -> UiConfig {
        let json = serde_json::to_string(&UiConfigPayload::from_ui(ui)).unwrap();
        let payload: UiConfigPayload = serde_json::from_str(&json).unwrap();
        let mut out = UiConfig::default();
        payload.apply_to(&mut out);
        out
    }

    /// Save `ui` to a profile file and read it back.
    fn through_toml(ui: &UiConfig) -> UiConfig {
        let config = ProfileConfig {
            ui: ui.clone(),
            ..ProfileConfig::default()
        };
        ProfileConfig::from_toml(&config.to_toml().unwrap())
            .unwrap()
            .ui
    }

    #[test]
    fn follow_system_appearance_round_trips() {
        let ui = UiConfig {
            follow_system_appearance: true,
            ..UiConfig::default()
        };
        assert!(through_payload(&ui).follow_system_appearance);
        assert!(through_toml(&ui).follow_system_appearance);
        assert!(!through_payload(&UiConfig::default()).follow_system_appearance);
    }

    #[test]
    fn light_theme_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.light_theme, "vellum");
        ui.light_theme = "classic-vivid".into();
        assert_eq!(through_payload(&ui).light_theme, "classic-vivid");
        assert_eq!(through_toml(&ui).light_theme, "classic-vivid");
        // A blank pick saves as the default light theme.
        ui.light_theme = "  ".into();
        assert_eq!(through_payload(&ui).light_theme, "vellum");
    }

    #[test]
    fn dark_theme_round_trips() {
        let mut ui = UiConfig::default();
        // Unset until the first save, so the frontend can seed it from
        // the current theme.
        assert_eq!(ui.dark_theme, "");
        assert_eq!(through_payload(&ui).dark_theme, "");
        ui.dark_theme = "nord".into();
        assert_eq!(through_payload(&ui).dark_theme, "nord");
        assert_eq!(through_toml(&ui).dark_theme, "nord");
    }

    #[test]
    fn terminal_line_height_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.terminal_line_height, "default");
        for id in ["compact", "default", "loose"] {
            ui.terminal_line_height = id.into();
            assert_eq!(through_payload(&ui).terminal_line_height, id);
            assert_eq!(through_toml(&ui).terminal_line_height, id);
        }
        ui.terminal_line_height = "roomy".into();
        assert_eq!(through_payload(&ui).terminal_line_height, "default");
    }

    #[test]
    fn a_theme_pick_writes_only_what_it_names() {
        let mut ui = UiConfig::default();
        assert!(super::apply_theme_pick(&mut ui, "nord".into(), None, None));
        assert_eq!(ui.theme, "nord");
        assert_eq!(ui.light_theme, "vellum");
        assert_eq!(ui.dark_theme, "");

        // A pick while following the system fills the dark slot.
        assert!(super::apply_theme_pick(
            &mut ui,
            "nord".into(),
            Some("vellum".into()),
            Some("tokyo-night".into()),
        ));
        assert_eq!(ui.theme, "nord");
        assert_eq!(ui.dark_theme, "tokyo-night");

        // The same pick again changes nothing, and a blank slot is left alone.
        assert!(!super::apply_theme_pick(
            &mut ui,
            "nord".into(),
            Some(String::new()),
            Some("tokyo-night".into()),
        ));
        assert_eq!(ui.light_theme, "vellum");
    }

    #[test]
    fn vitals_density_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.vitals_density, "rows");
        for id in ["rows", "line"] {
            ui.vitals_density = id.into();
            assert_eq!(through_payload(&ui).vitals_density, id);
            assert_eq!(through_toml(&ui).vitals_density, id);
        }
        // An unknown density saves as rows.
        ui.vitals_density = "grid".into();
        assert_eq!(through_payload(&ui).vitals_density, "rows");
    }

    #[test]
    fn vitals_values_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.vitals_values, "current-max");
        for id in ["current-max", "current", "percent"] {
            ui.vitals_values = id.into();
            assert_eq!(through_payload(&ui).vitals_values, id);
            assert_eq!(through_toml(&ui).vitals_values, id);
        }
        // An unknown form saves as current and max.
        ui.vitals_values = "both".into();
        assert_eq!(through_payload(&ui).vitals_values, "current-max");
    }

    #[test]
    fn vitals_meter_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.vitals_meter, "line");
        for id in ["line", "bar", "none"] {
            ui.vitals_meter = id.into();
            assert_eq!(through_payload(&ui).vitals_meter, id);
            assert_eq!(through_toml(&ui).vitals_meter, id);
        }
        // An unknown meter saves as the line.
        ui.vitals_meter = "gauge".into();
        assert_eq!(through_payload(&ui).vitals_meter, "line");
    }

    #[test]
    fn vitals_warn_thirds_round_trips() {
        let mut ui = UiConfig::default();
        assert!(!ui.vitals_warn_thirds);
        assert!(!through_payload(&ui).vitals_warn_thirds);
        ui.vitals_warn_thirds = true;
        assert!(through_payload(&ui).vitals_warn_thirds);
        assert!(through_toml(&ui).vitals_warn_thirds);
    }

    #[test]
    fn a_profile_without_the_vitals_options_loads_the_defaults() {
        let ui = ProfileConfig::from_toml("[ui]\ntheme = \"nord\"\n")
            .unwrap()
            .ui;
        assert_eq!(ui.vitals_values, "current-max");
        assert_eq!(ui.vitals_meter, "line");
        assert!(!ui.vitals_warn_thirds);
    }

    #[test]
    fn a_profile_without_the_vitals_density_loads_rows() {
        let ui = ProfileConfig::from_toml("[ui]\ntheme = \"nord\"\n")
            .unwrap()
            .ui;
        assert_eq!(ui.vitals_density, "rows");
    }

    #[test]
    fn a_profile_without_the_appearance_fields_loads_the_defaults() {
        let ui = ProfileConfig::from_toml("[ui]\ntheme = \"nord\"\n")
            .unwrap()
            .ui;
        assert_eq!(ui.theme, "nord");
        assert!(!ui.follow_system_appearance);
        assert_eq!(ui.light_theme, "vellum");
        assert_eq!(ui.dark_theme, "");
        assert_eq!(ui.terminal_line_height, "default");
    }

    #[test]
    fn auto_switch_line_names_the_profile_in_a_sentence() {
        assert_eq!(
            super::auto_switch_line("Erelei"),
            "\r\n\x1b[33mVosh switched to the Erelei profile.\x1b[0m\r\n"
        );
        assert_eq!(
            super::auto_switch_line("default"),
            "\r\n\x1b[33mVosh switched to the Default profile.\x1b[0m\r\n"
        );
    }

    /// App state over James's profile set in `dir`, with `default` live
    /// and tracking Sanctuary.
    async fn switch_state(dir: &std::path::Path) -> super::SharedState {
        let state: super::SharedState = std::sync::Arc::new(super::AppState::default());
        state.profile.lock().await.ui.tracked_affects = vec![affect("Sanctuary")];
        *state.profile_set.lock().await = Some(james_like_set(dir));
        state
    }

    fn affect(name: &str) -> crate::profile_config::TrackedAffect {
        crate::profile_config::TrackedAffect {
            name: name.into(),
            label: None,
        }
    }

    async fn live_affects(state: &super::SharedState) -> Vec<String> {
        let p = state.profile.lock().await;
        p.ui.tracked_affects
            .iter()
            .map(|t| t.name.clone())
            .collect()
    }

    async fn active(state: &super::SharedState) -> String {
        let guard = state.profile_set.lock().await;
        guard.as_ref().unwrap().active_name().to_string()
    }

    fn healer_file(dir: &std::path::Path) -> std::path::PathBuf {
        ProfileSet::load_or_migrate(dir.to_path_buf())
            .unwrap()
            .profile_path("Healer")
    }

    const UNREADABLE: &str = "tracked = = [\n";

    #[tokio::test]
    async fn a_switch_loads_the_named_profile() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let mut config = ProfileConfig::default();
        config.ui.tracked_affects = vec![affect("Haste")];
        config.save(&healer_file(dir.path())).unwrap();

        super::switch_live_profile(&state, "Healer").await.unwrap();
        assert_eq!(active(&state).await, "Healer");
        assert_eq!(live_affects(&state).await, ["Haste"]);
        let reloaded = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert_eq!(reloaded.active_name(), "Healer");
    }

    #[tokio::test]
    async fn a_profile_file_that_does_not_read_keeps_the_live_profile() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        std::fs::write(healer_file(dir.path()), UNREADABLE).unwrap();

        let err = super::switch_live_profile(&state, "Healer")
            .await
            .unwrap_err();
        assert_eq!(
            err,
            "Vosh could not open the Healer profile because it could not read the profile \
             file. You are still using the Default profile."
        );
        // The index still names the live profile, in memory and on
        // disk, so the next persist writes it to its own file.
        assert_eq!(active(&state).await, DEFAULT_PROFILE_NAME);
        let reloaded = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert_eq!(reloaded.active_name(), DEFAULT_PROFILE_NAME);
        assert_eq!(live_affects(&state).await, ["Sanctuary"]);
        // The file that did not read stays as it was.
        assert_eq!(
            std::fs::read_to_string(healer_file(dir.path())).unwrap(),
            UNREADABLE
        );
    }

    #[tokio::test]
    async fn a_global_file_that_does_not_read_keeps_the_live_profile() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        let mut config = ProfileConfig::default();
        config.ui.tracked_affects = vec![affect("Haste")];
        config.save(&healer_file(dir.path())).unwrap();
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        std::fs::write(set.global_path(), UNREADABLE).unwrap();

        let err = super::switch_live_profile(&state, "Healer")
            .await
            .unwrap_err();
        assert_eq!(
            err,
            "Vosh could not open the Healer profile because it could not read global.toml, \
             which holds your shared settings. You are still using the Default profile."
        );
        assert_eq!(active(&state).await, DEFAULT_PROFILE_NAME);
        let reloaded = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert_eq!(reloaded.active_name(), DEFAULT_PROFILE_NAME);
        assert_eq!(live_affects(&state).await, ["Sanctuary"]);
    }

    #[tokio::test]
    async fn a_login_switch_to_a_file_that_does_not_read_keeps_the_live_profile() {
        let dir = tempfile::tempdir().unwrap();
        let state = switch_state(dir.path()).await;
        std::fs::write(healer_file(dir.path()), UNREADABLE).unwrap();
        *state.current_connection.lock().unwrap() =
            Some(("play.theforsakenlands.com".into(), 1848));

        // Caelaor logging in picks Healer, whose file does not read.
        let target = super::auto_switch_target(&state, "Caelaor").await;
        assert_eq!(target.as_deref(), Some("Healer"));
        let err = super::switch_live_profile(&state, "Healer")
            .await
            .unwrap_err();
        assert_eq!(
            super::auto_switch_failed_line(&err),
            "\r\n\x1b[33mVosh could not open the Healer profile because it could not read \
             the profile file. You are still using the Default profile.\x1b[0m\r\n"
        );
        assert_eq!(active(&state).await, DEFAULT_PROFILE_NAME);
        assert_eq!(live_affects(&state).await, ["Sanctuary"]);
        // Erelei belongs to the live profile, so nothing switches.
        assert_eq!(super::auto_switch_target(&state, "Erelei").await, None);
    }

    /// Launch over the profile set in `dir` the way lib.rs runs it, and
    /// hand back the app state with the notices launch kept.
    async fn launch_state(dir: &std::path::Path) -> super::SharedState {
        let state: super::SharedState = std::sync::Arc::new(super::AppState::default());
        crate::launch::load_profiles(&state, dir).await;
        assert!(state.profile_set.lock().await.is_some());
        state
    }

    /// The save a Settings edit, a slash command debounce, or quit runs.
    async fn persist(state: &super::SharedState, dir: &std::path::Path) {
        let _persist_guard = super::PERSIST_LOCK.lock().await;
        super::persist_state(state, Some(dir)).await;
    }

    async fn change_scope(state: &super::SharedState) -> Result<(), String> {
        let _persist_guard = super::PERSIST_LOCK.lock().await;
        let scope = crate::profile_set::ScopeConfig {
            theme: crate::profile_set::Scope::Profile,
            ..crate::profile_set::ScopeConfig::default()
        };
        super::change_scope_locked(state, scope).await.map(|_| ())
    }

    fn read(path: &std::path::Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    #[tokio::test]
    async fn a_profile_file_that_did_not_read_at_launch_is_never_saved_over() {
        let dir = tempfile::tempdir().unwrap();
        let set = james_like_set(dir.path());
        std::fs::write(set.active_path(), UNREADABLE).unwrap();

        let state = launch_state(dir.path()).await;
        assert_eq!(
            state.take_launch_notices(),
            [
                "Vosh could not read the Default profile file, so it will not save over it. Fix \
              the file or switch to another profile."
            ]
        );
        // You tell once. A second take finds nothing.
        assert!(state.take_launch_notices().is_empty());

        // The app keeps running on the defaults, and an edit saves.
        state.profile.lock().await.ui.tracked_affects = vec![affect("Haste")];
        persist(&state, dir.path()).await;
        assert_eq!(read(&set.active_path()), UNREADABLE);
        // global.toml read, so the shared settings still save.
        assert!(set.global_path().exists());

        // Changing what every character shares would spread the defaults.
        assert_eq!(
            change_scope(&state).await.unwrap_err(),
            "Vosh could not read the Default profile file, so it will not change which settings \
             every character shares. Fix the file or switch to another profile."
        );
        assert_eq!(read(&set.active_path()), UNREADABLE);
    }

    #[tokio::test]
    async fn a_switch_that_reads_its_files_lets_the_saves_resume() {
        let dir = tempfile::tempdir().unwrap();
        let set = james_like_set(dir.path());
        std::fs::write(set.active_path(), UNREADABLE).unwrap();
        let mut healer = ProfileConfig::default();
        healer.ui.tracked_affects = vec![affect("Haste")];
        healer.save(&set.profile_path("Healer")).unwrap();
        let state = launch_state(dir.path()).await;

        // A switch saves the profile you leave first, and that save
        // leaves the file that did not read alone.
        persist(&state, dir.path()).await;
        assert_eq!(read(&set.active_path()), UNREADABLE);
        super::switch_live_profile(&state, "Healer").await.unwrap();
        assert_eq!(live_affects(&state).await, ["Haste"]);
        state.profile.lock().await.ui.tracked_affects = vec![affect("Fly")];
        persist(&state, dir.path()).await;

        let saved = ProfileConfig::load(&set.profile_path("Healer")).unwrap();
        assert_eq!(saved.ui.tracked_affects[0].name, "Fly");
        // The file that did not read was never written.
        assert_eq!(read(&set.profile_path(DEFAULT_PROFILE_NAME)), UNREADABLE);
    }

    #[tokio::test]
    async fn global_toml_that_did_not_read_at_launch_is_never_saved_over() {
        let dir = tempfile::tempdir().unwrap();
        let set = james_like_set(dir.path());
        std::fs::write(set.global_path(), UNREADABLE).unwrap();

        let state = launch_state(dir.path()).await;
        assert_eq!(
            state.take_launch_notices(),
            [crate::profile_config::UNREAD_GLOBAL_NOTICE]
        );
        {
            let mut p = state.profile.lock().await;
            p.ui.theme = "nord".into();
            p.ui.tracked_affects = vec![affect("Fly")];
        }
        persist(&state, dir.path()).await;

        assert_eq!(read(&set.global_path()), UNREADABLE);
        // The profile file read, so what it owns still saves.
        let saved = ProfileConfig::load(&set.active_path()).unwrap();
        assert_eq!(saved.ui.tracked_affects[0].name, "Fly");
        assert_eq!(
            change_scope(&state).await.unwrap_err(),
            "Vosh could not read global.toml, so it will not change which settings every \
             character shares. Fix the file and restart Vosh."
        );
        assert_eq!(read(&set.global_path()), UNREADABLE);
    }

    #[tokio::test]
    async fn a_rename_keeps_the_file_that_did_not_read_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = james_like_set(dir.path());
        set.switch("Healer").unwrap();
        std::fs::write(set.active_path(), UNREADABLE).unwrap();
        let state = launch_state(dir.path()).await;

        {
            let mut guard = state.profile_set.lock().await;
            guard.as_mut().unwrap().rename("Healer", "Cleric").unwrap();
        }
        persist(&state, dir.path()).await;
        assert_eq!(read(&set.profile_path("Cleric")), UNREADABLE);
    }

    /// A Tick block from Settings: off, every minute, with every option
    /// filled in and `reset_pattern` as the Reset on pattern.
    fn tick_payload(reset_pattern: &str) -> super::TickConfigPayload {
        super::TickConfigPayload {
            enabled: false,
            interval_secs: 60,
            auto_fire: Some(" score ".into()),
            sound: false,
            reset_pattern: Some(reset_pattern.into()),
            warn_at_secs: Some(5),
            warn_message: Some("Tick soon".into()),
            warn_color: Some("red".into()),
        }
    }

    /// A running 30 second tick that resets on `^You feel`.
    fn running_tick(now: tokio::time::Instant) -> crate::tick::TickRuntime {
        let mut tick = crate::tick::TickRuntime::default();
        tick.enable(now);
        tick.set_reset_pattern(Some("^You feel".into())).unwrap();
        tick
    }

    #[test]
    fn a_tick_config_with_a_bad_reset_pattern_changes_nothing() {
        let now = tokio::time::Instant::now();
        let mut tick = running_tick(now);
        let before = format!("{:?}", tick.config);
        let next_fire = tick.next_fire;

        let err = super::apply_tick_config(&mut tick, &tick_payload("[bad"), now).unwrap_err();
        assert_eq!(
            err,
            "Vosh could not read the Reset on pattern. Check it and save again."
        );
        // Still on, still every 30 seconds, still on the same clock, and
        // still resetting on the old pattern.
        assert_eq!(format!("{:?}", tick.config), before);
        assert!(tick.config.enabled);
        assert_eq!(tick.config.interval.as_secs(), 30);
        assert_eq!(tick.next_fire, next_fire);
        assert!(tick.check_reset_match("You feel less tired."));
    }

    #[test]
    fn a_tick_config_that_reads_applies_every_field() {
        let now = tokio::time::Instant::now();
        let mut tick = running_tick(now);

        let saved = super::apply_tick_config(&mut tick, &tick_payload(" ^Dawn "), now).unwrap();
        assert!(!saved.enabled);
        assert_eq!(saved.interval_secs, 60);
        assert_eq!(saved.auto_fire.as_deref(), Some("score"));
        assert_eq!(saved.reset_pattern.as_deref(), Some("^Dawn"));
        assert_eq!(saved.warn_at_secs, Some(5));
        assert!(!tick.config.enabled);
        assert_eq!(tick.next_fire, None);
        assert_eq!(tick.config.interval.as_secs(), 60);
        assert!(!tick.config.sound);
        assert!(tick.check_reset_match("Dawn breaks."));
        assert!(!tick.check_reset_match("You feel less tired."));

        // Turned back on, the tick runs at the saved interval, and a
        // blank pattern clears the reset.
        let mut on = tick_payload("  ");
        on.enabled = true;
        let saved = super::apply_tick_config(&mut tick, &on, now).unwrap();
        assert!(saved.enabled);
        assert_eq!(saved.reset_pattern, None);
        assert_eq!(
            tick.next_fire,
            Some(now + std::time::Duration::from_secs(60))
        );
        assert!(!tick.check_reset_match("Dawn breaks."));
    }

    #[test]
    fn settings_window_keeps_a_size_that_fits() {
        assert_eq!(settings_window_fit((880.0, 600.0)), None);
        assert_eq!(settings_window_fit((820.0, 560.0)), None);
        assert_eq!(settings_window_fit((1200.0, 900.0)), None);
    }

    #[test]
    fn settings_window_grows_a_side_left_under_the_minimum() {
        // The old Settings window opened at 780×640.
        assert_eq!(settings_window_fit((780.0, 640.0)), Some((880.0, 640.0)));
        assert_eq!(settings_window_fit((900.0, 420.0)), Some((900.0, 600.0)));
        assert_eq!(settings_window_fit((520.0, 420.0)), Some((880.0, 600.0)));
    }

    #[test]
    fn scrollback_load_names_the_fields_the_page_reads() {
        let load = ScrollbackLoad {
            bytes: vec![104, 105],
            seeded_native: true,
        };
        assert_eq!(
            serde_json::to_value(&load).unwrap(),
            serde_json::json!({ "bytes": [104, 105], "seeded_native": true })
        );
    }

    mod scope {
        use std::sync::Arc;

        use super::super::{change_scope_locked, AppState, SharedState, PERSIST_LOCK};
        use crate::profile::Profile;
        use crate::profile_config::{
            strip_global_fields, CustomTheme, GlobalConfig, ProfileConfig, TrackedAffect, UiConfig,
        };
        use crate::profile_set::tests::james_like_set;
        use crate::profile_set::{ProfileSet, Scope, ScopeConfig, DEFAULT_PROFILE_NAME};

        fn theme(id: &str, background: &str) -> CustomTheme {
            CustomTheme {
                id: id.into(),
                label: id.into(),
                xterm: [("background".to_string(), background.to_string())]
                    .into_iter()
                    .collect(),
                ..CustomTheme::default()
            }
        }

        fn ids(themes: &[CustomTheme]) -> Vec<&str> {
            themes.iter().map(|t| t.id.as_str()).collect()
        }

        /// The live profile with every shared setting off its default.
        fn shared_profile() -> Profile {
            let mut profile = Profile::default();
            profile.ui.theme = "night-ink".into();
            profile.ui.follow_system_appearance = true;
            profile.ui.light_theme = "classic-vivid".into();
            profile.ui.dark_theme = "night-ink".into();
            profile.ui.custom_themes = vec![theme("night-ink", "#000000")];
            profile.ui.font_family = "Iosevka".into();
            profile.ui.font_size = 16;
            profile.ui.terminal_line_height = "loose".into();
            profile.ui.keep_last_command = true;
            profile.ui.auto_update = true;
            profile
        }

        /// Mirror `persist_profile` for the active profile.
        fn persist(set: &ProfileSet, profile: &Profile) {
            let mut snapshot = ProfileConfig::from_profile(profile);
            strip_global_fields(&mut snapshot, set.scope());
            snapshot.save(&set.active_path()).unwrap();
            GlobalConfig::from_profile(profile, set.scope())
                .save(&set.global_path())
                .unwrap();
        }

        /// Mirror a switch. The active profile file loads first, then the
        /// shared part of global.toml over it.
        fn load(set: &ProfileSet) -> Profile {
            let mut profile = Profile::default();
            let path = set.active_path();
            if path.exists() {
                ProfileConfig::load(&path).unwrap().apply_to(&mut profile);
            }
            if let Some(global) =
                GlobalConfig::load_shared(&set.global_path(), set.scope()).unwrap()
            {
                global.apply_to(&mut profile);
            }
            profile
        }

        fn file(set: &ProfileSet, name: &str) -> ProfileConfig {
            ProfileConfig::load(&set.profile_path(name)).unwrap()
        }

        fn per_profile() -> ScopeConfig {
            ScopeConfig {
                theme: Scope::Profile,
                font: Scope::Profile,
                keep_last_command: Scope::Profile,
                auto_update: Scope::Profile,
                ..ScopeConfig::default()
            }
        }

        /// Default is live and shares everything. Healer saved its file
        /// while everything was shared, so it holds the defaults. Test-Prompt
        /// saved its own theme, font, and custom theme before they were
        /// shared, under the id the live custom theme holds.
        async fn three_profiles(dir: &std::path::Path) -> SharedState {
            let set = james_like_set(dir);
            let live = shared_profile();
            persist(&set, &live);

            let mut healer = ProfileConfig::default();
            healer.ui.tracked_affects = vec![TrackedAffect {
                name: "Fly".into(),
                label: None,
            }];
            healer.save(&set.profile_path("Healer")).unwrap();

            let mut prompt = ProfileConfig::default();
            prompt.ui.theme = "night-ink".into();
            prompt.ui.custom_themes = vec![theme("night-ink", "#ffffff")];
            prompt.ui.font_size = 13;
            prompt.save(&set.profile_path("Test-Prompt")).unwrap();

            let state: SharedState = Arc::new(AppState::default());
            *state.profile.lock().await = live;
            *state.profile_set.lock().await = Some(set);
            state
        }

        /// Mirror `profile_set_scope`. The persist that follows the change
        /// runs under the same lock.
        async fn set_scope(state: &SharedState, scope: ScopeConfig) {
            let _persist_guard = PERSIST_LOCK.lock().await;
            change_scope_locked(state, scope).await.unwrap();
            let live = state.profile.lock().await;
            let guard = state.profile_set.lock().await;
            persist(guard.as_ref().unwrap(), &live);
        }

        #[tokio::test]
        async fn turning_sharing_off_hands_the_shared_settings_to_every_profile() {
            let dir = tempfile::tempdir().unwrap();
            let state = three_profiles(dir.path()).await;
            set_scope(&state, per_profile()).await;

            let mut guard = state.profile_set.lock().await;
            let set = guard.as_mut().unwrap();
            // global.toml no longer holds the categories you turned off.
            let global = GlobalConfig::load(&set.global_path()).unwrap();
            assert!(global.theme.is_none());
            assert!(global.custom_themes.is_none());
            assert!(global.font_size.is_none());
            assert!(global.keep_last_command.is_none());
            assert!(global.auto_update.is_none());

            // Healer held none of its own, so it takes every shared value
            // and keeps what it owns.
            let healer = file(set, "Healer").ui;
            assert_eq!(healer.theme, "night-ink");
            assert!(healer.follow_system_appearance);
            assert_eq!(healer.light_theme, "classic-vivid");
            assert_eq!(healer.dark_theme, "night-ink");
            assert_eq!(ids(&healer.custom_themes), ["night-ink"]);
            assert_eq!(healer.font_family, "Iosevka");
            assert_eq!(healer.font_size, 16);
            assert_eq!(healer.terminal_line_height, "loose");
            assert!(healer.keep_last_command);
            assert!(healer.auto_update);
            assert_eq!(healer.tracked_affects.len(), 1);

            // Test-Prompt keeps its own theme and font, and its own custom
            // theme moves to a fresh id beside the shared one.
            let prompt = file(set, "Test-Prompt").ui;
            assert_eq!(ids(&prompt.custom_themes), ["night-ink", "night-ink-2"]);
            assert_eq!(prompt.custom_themes[1], {
                let mut own = theme("night-ink-2", "#ffffff");
                own.label = "night-ink (Test-Prompt)".into();
                own
            });
            assert_eq!(prompt.theme, "night-ink-2");
            assert_eq!(prompt.font_size, 13);
            assert_eq!(prompt.font_family, UiConfig::default().font_family);
            assert!(prompt.keep_last_command);

            // A switch to Healer shows what it showed while shared.
            set.switch("Healer").unwrap();
            let healer = load(set);
            assert_eq!(healer.ui.theme, "night-ink");
            assert_eq!(healer.ui.font_size, 16);
            assert!(healer.ui.keep_last_command);
            // The live profile kept its values in its own file.
            set.switch(DEFAULT_PROFILE_NAME).unwrap();
            let live = load(set);
            assert_eq!(live.ui.theme, "night-ink");
            assert_eq!(live.ui.font_size, 16);
        }

        #[tokio::test]
        async fn sharing_again_after_turning_it_off_keeps_every_theme() {
            let dir = tempfile::tempdir().unwrap();
            let state = three_profiles(dir.path()).await;
            set_scope(&state, per_profile()).await;
            set_scope(&state, ScopeConfig::default()).await;

            let live = state.profile.lock().await;
            assert_eq!(ids(&live.ui.custom_themes), ["night-ink", "night-ink-2"]);
            let guard = state.profile_set.lock().await;
            let set = guard.as_ref().unwrap();
            let global = GlobalConfig::load(&set.global_path()).unwrap();
            assert_eq!(
                ids(&global.custom_themes.unwrap()),
                ["night-ink", "night-ink-2"]
            );
            // Test-Prompt still points at its own theme for the next time
            // you turn sharing off.
            assert_eq!(file(set, "Test-Prompt").ui.theme, "night-ink-2");
        }

        #[tokio::test]
        async fn a_profile_that_never_saved_takes_the_shared_settings() {
            let dir = tempfile::tempdir().unwrap();
            let state = three_profiles(dir.path()).await;
            state
                .profile_set
                .lock()
                .await
                .as_mut()
                .unwrap()
                .create("Bard")
                .unwrap();
            set_scope(
                &state,
                ScopeConfig {
                    font: Scope::Profile,
                    ..ScopeConfig::default()
                },
            )
            .await;

            let guard = state.profile_set.lock().await;
            let set = guard.as_ref().unwrap();
            let bard = file(set, "Bard").ui;
            assert_eq!(bard.font_size, 16);
            assert_eq!(bard.terminal_line_height, "loose");
            // The theme is still shared, so the file keeps the defaults.
            assert!(bard.custom_themes.is_empty());
        }

        #[tokio::test]
        async fn a_file_vosh_cannot_read_keeps_the_settings_shared() {
            let dir = tempfile::tempdir().unwrap();
            let state = three_profiles(dir.path()).await;
            let (healer_path, global_path) = {
                let guard = state.profile_set.lock().await;
                let set = guard.as_ref().unwrap();
                std::fs::write(set.profile_path("Test-Prompt"), "theme = [").unwrap();
                (set.profile_path("Healer"), set.global_path())
            };
            let healer_before = std::fs::read_to_string(&healer_path).unwrap();
            let global_before = std::fs::read_to_string(&global_path).unwrap();

            let refused = {
                let _persist_guard = PERSIST_LOCK.lock().await;
                change_scope_locked(&state, per_profile()).await
            };

            let message = refused.unwrap_err();
            assert_eq!(
                message,
                "Vosh could not read the Test-Prompt profile file, so these settings stay the same for every character."
            );
            let guard = state.profile_set.lock().await;
            assert_eq!(guard.as_ref().unwrap().scope().theme, Scope::Global);
            assert_eq!(
                std::fs::read_to_string(&healer_path).unwrap(),
                healer_before
            );
            assert_eq!(
                std::fs::read_to_string(&global_path).unwrap(),
                global_before
            );
        }

        #[test]
        fn stopped_sharing_names_only_the_categories_turned_off() {
            let shared = ScopeConfig::default();
            assert!(shared.stopped_sharing(&shared).is_none());
            let stopped = shared.stopped_sharing(&per_profile()).unwrap();
            assert_eq!(stopped.theme, Scope::Global);
            assert_eq!(stopped.font, Scope::Global);
            assert_eq!(stopped.dock_layout, Scope::Profile);
            assert!(per_profile().stopped_sharing(&shared).is_none());
        }

        #[tokio::test]
        async fn turning_sharing_on_writes_no_other_profile_file() {
            let dir = tempfile::tempdir().unwrap();
            let state = three_profiles(dir.path()).await;
            let healer_before = {
                let guard = state.profile_set.lock().await;
                std::fs::read_to_string(guard.as_ref().unwrap().profile_path("Healer")).unwrap()
            };
            set_scope(&state, ScopeConfig::default()).await;
            let guard = state.profile_set.lock().await;
            let healer_after =
                std::fs::read_to_string(guard.as_ref().unwrap().profile_path("Healer")).unwrap();
            assert_eq!(healer_after, healer_before);
        }
    }

    #[tokio::test]
    async fn a_switch_in_loadout_mode_keeps_the_catalog_presets() {
        use std::sync::Arc;
        let state: super::SharedState = Arc::new(super::AppState::default());
        *state.global_catalog.lock().await = Some(crate::loadout::GlobalCatalog {
            enabled_presets: Some(vec!["healing_basics".into()]),
            ..crate::loadout::GlobalCatalog::default()
        });
        // The switch just loaded Healer's file, with its own older list.
        state.profile.lock().await.ui.enabled_presets =
            vec!["healing_basics".into(), "potion_labels".into()];
        let catalog = state.global_catalog.lock().await.clone().unwrap();
        super::lay_catalog_over(&mut *state.profile.lock().await, &catalog, None);
        assert_eq!(
            state.profile.lock().await.ui.enabled_presets,
            vec!["healing_basics".to_string()]
        );
    }

    #[tokio::test]
    async fn a_loadout_save_leaves_the_presets_to_a_catalog_still_waiting() {
        use std::sync::Arc;
        let dir = tempfile::tempdir().unwrap();
        let state: super::SharedState = Arc::new(super::AppState::default());
        *state.profile_set.lock().await = Some(james_like_set(dir.path()));
        // No profile file read at launch, so the catalog took no list and
        // the live profile kept its own.
        *state.global_catalog.lock().await = Some(crate::loadout::GlobalCatalog::default());
        state.profile.lock().await.ui.enabled_presets = vec!["healing_basics".into()];

        persist(&state, dir.path()).await;
        let saved = crate::loadout_store::load_global_catalog(dir.path()).unwrap();
        assert_eq!(saved.enabled_presets, None);

        // Once the catalog holds a list, the saves keep it current.
        state
            .global_catalog
            .lock()
            .await
            .as_mut()
            .unwrap()
            .enabled_presets = Some(Vec::new());
        persist(&state, dir.path()).await;
        let saved = crate::loadout_store::load_global_catalog(dir.path()).unwrap();
        assert_eq!(saved.enabled_presets, Some(vec!["healing_basics".into()]));
    }

    /// The shared catalog wizard and the loadout mode files it writes.
    mod shared_catalog {
        use super::{james_like_set, launch_state, persist, read, UNREADABLE};
        use crate::loadout::{GlobalCatalog, Loadout, LoadoutSet};
        use crate::loadout_store::{
            catalog_path, load_path_b_at_launch, loadouts_path, save_global_catalog,
            save_loadout_set,
        };
        use crate::profile_config::ProfileConfig;
        use crate::profile_set::ProfileSet;

        const HELD: &str = "Vosh could not read your shared catalog at launch, so it will not \
                            build a new one over it. Fix catalog.toml or loadouts.toml and \
                            restart Vosh.";

        /// Save `name`'s file with one alias, the way a profile in per
        /// profile mode holds its own items.
        fn write_alias(set: &ProfileSet, name: &str, alias: &str) {
            let mut config = ProfileConfig::default();
            config
                .aliases
                .push(vosh_alias::Alias::new(alias, "kick %1"));
            config.save(&set.profile_path(name)).unwrap();
        }

        /// A catalog with your shared alias and a loadout per character,
        /// and profile files that hold no items, as loadout mode keeps
        /// them.
        fn loadout_mode(set: &ProfileSet, dir: &std::path::Path) {
            for entry in set.list() {
                ProfileConfig::default()
                    .save(&set.profile_path(&entry.name))
                    .unwrap();
            }
            let mut catalog = GlobalCatalog::default();
            catalog
                .aliases
                .push(vosh_alias::Alias::new("kk", "kick %1"));
            save_global_catalog(dir, &catalog).unwrap();
            let loadouts = LoadoutSet {
                loadouts: vec![Loadout::empty("default")],
                active: vec!["default".into()],
                dormant: false,
            };
            save_loadout_set(dir, &loadouts).unwrap();
        }

        async fn refused(state: &super::super::SharedState, dir: &std::path::Path) -> String {
            let analyze = super::super::analyze_migration(state, dir)
                .await
                .unwrap_err();
            let apply = super::super::apply_migration(state, dir, &[], || {})
                .await
                .unwrap_err();
            assert_eq!(analyze, apply);
            apply
        }

        /// Quit and open Vosh again as `name`, the way lib.rs launches,
        /// with the shared catalog and loadouts when they are on disk.
        async fn relaunch_as(dir: &std::path::Path, name: &str) -> super::super::SharedState {
            ProfileSet::load_or_migrate(dir.to_path_buf())
                .unwrap()
                .switch(name)
                .unwrap();
            let state = launch_state(dir).await;
            crate::launch::load_loadout_mode(&state, dir).await;
            state
        }

        fn macro_on(key: &str, command: &str) -> crate::profile::Macro {
            crate::profile::Macro {
                key: key.into(),
                command: command.into(),
                group: None,
                enabled: true,
            }
        }

        fn macro_keys(macros: &[crate::profile::Macro]) -> Vec<&str> {
            macros.iter().map(|m| m.key.as_str()).collect()
        }

        #[tokio::test]
        async fn a_loadout_save_leaves_the_macros_to_the_catalog() {
            let dir = tempfile::tempdir().unwrap();
            let set = james_like_set(dir.path());
            loadout_mode(&set, dir.path());
            let state = relaunch_as(dir.path(), crate::profile_set::DEFAULT_PROFILE_NAME).await;
            assert!(state.global_catalog.lock().await.is_some());
            state
                .profile
                .lock()
                .await
                .macros
                .push(macro_on("f1", "look"));
            persist(&state, dir.path()).await;
            let saved = crate::loadout_store::load_global_catalog(dir.path()).unwrap();
            assert_eq!(macro_keys(&saved.macros), ["f1"]);
            assert!(ProfileConfig::load(&set.active_path())
                .unwrap()
                .macros
                .is_empty());

            // You delete the macro while you play Healer.
            let state = relaunch_as(dir.path(), "Healer").await;
            state.profile.lock().await.macros.clear();
            persist(&state, dir.path()).await;

            // Back on Default, the macro stays deleted.
            let state = relaunch_as(dir.path(), crate::profile_set::DEFAULT_PROFILE_NAME).await;
            assert!(state.profile.lock().await.macros.is_empty());
        }

        #[tokio::test]
        async fn a_catalog_that_does_not_read_is_held_and_never_replaced() {
            let dir = tempfile::tempdir().unwrap();
            let set = james_like_set(dir.path());
            loadout_mode(&set, dir.path());
            std::fs::write(catalog_path(dir.path()), UNREADABLE).unwrap();
            let loadouts = read(&loadouts_path(dir.path()));

            // Launch tells you and runs on the profile files alone.
            let state = launch_state(dir.path()).await;
            assert_eq!(
                load_path_b_at_launch(dir.path()).unwrap_err(),
                [crate::loadout_store::UNREAD_CATALOG_NOTICE]
            );

            // The wizard would build the catalog from profile files that
            // hold none of your shared items.
            assert_eq!(refused(&state, dir.path()).await, HELD);
            persist(&state, dir.path()).await;
            assert!(save_global_catalog(dir.path(), &GlobalCatalog::default()).is_err());
            assert!(save_loadout_set(dir.path(), &LoadoutSet::default()).is_err());

            assert_eq!(read(&catalog_path(dir.path())), UNREADABLE);
            assert_eq!(read(&loadouts_path(dir.path())), loadouts);
            // The profile files stay where they are.
            assert!(set.active_path().exists());
            assert!(!dir.path().join("profiles").join("legacy").exists());
        }

        #[tokio::test]
        async fn a_loadouts_file_that_does_not_read_holds_the_catalog_too() {
            let dir = tempfile::tempdir().unwrap();
            let set = james_like_set(dir.path());
            loadout_mode(&set, dir.path());
            std::fs::write(loadouts_path(dir.path()), UNREADABLE).unwrap();
            let catalog = read(&catalog_path(dir.path()));

            let state = launch_state(dir.path()).await;
            assert_eq!(
                load_path_b_at_launch(dir.path()).unwrap_err(),
                [crate::loadout_store::UNREAD_LOADOUTS_NOTICE]
            );
            assert_eq!(refused(&state, dir.path()).await, HELD);
            assert!(save_global_catalog(dir.path(), &GlobalCatalog::default()).is_err());
            assert_eq!(read(&catalog_path(dir.path())), catalog);
            assert_eq!(read(&loadouts_path(dir.path())), UNREADABLE);
        }

        #[tokio::test]
        async fn the_wizard_never_builds_over_a_catalog_you_already_use() {
            let dir = tempfile::tempdir().unwrap();
            let set = james_like_set(dir.path());
            loadout_mode(&set, dir.path());
            let catalog = read(&catalog_path(dir.path()));
            let state = launch_state(dir.path()).await;
            assert!(load_path_b_at_launch(dir.path()).is_ok());

            assert_eq!(
                refused(&state, dir.path()).await,
                "You already have a shared catalog, so Vosh will not build another one over it."
            );
            assert_eq!(read(&catalog_path(dir.path())), catalog);
            assert!(set.active_path().exists());
        }

        #[tokio::test]
        async fn the_wizard_never_saves_over_a_loadouts_file_it_did_not_read() {
            let dir = tempfile::tempdir().unwrap();
            let set = james_like_set(dir.path());
            write_alias(&set, "Healer", "hh");
            save_loadout_set(dir.path(), &LoadoutSet::default()).unwrap();
            let loadouts = read(&loadouts_path(dir.path()));
            let state = launch_state(dir.path()).await;

            assert_eq!(
                refused(&state, dir.path()).await,
                "Vosh found loadouts.toml from an earlier shared catalog and will not save over \
                 it. Move the file out of the Vosh folder to build a new catalog."
            );
            assert!(!catalog_path(dir.path()).exists());
            assert_eq!(read(&loadouts_path(dir.path())), loadouts);
        }

        #[tokio::test]
        async fn the_wizard_builds_a_catalog_once() {
            let dir = tempfile::tempdir().unwrap();
            let set = james_like_set(dir.path());
            write_alias(&set, "Healer", "hh");
            let state = launch_state(dir.path()).await;

            let plan = super::super::analyze_migration(&state, dir.path())
                .await
                .unwrap();
            assert_eq!(plan.auto_resolved.aliases.len(), 1);
            let mut written = false;
            super::super::apply_migration(&state, dir.path(), &[], || written = true)
                .await
                .unwrap();
            assert!(written);
            let (catalog, _) = load_path_b_at_launch(dir.path()).unwrap();
            assert_eq!(catalog.aliases[0].name, "hh");
            // The alias left the profile file, and the copy in legacy
            // still holds it.
            let healer = set.profile_path("Healer");
            assert!(ProfileConfig::load(&healer).unwrap().aliases.is_empty());
            let legacy = healer.parent().unwrap().join("legacy").join("Healer.toml");
            assert_eq!(ProfileConfig::load(&legacy).unwrap().aliases[0].name, "hh");
            // A profile that never saved a file gets one that keeps the
            // Healer alias off for it, as it had no such alias.
            let prompt = ProfileConfig::load(&set.profile_path("Test-Prompt")).unwrap();
            assert_eq!(prompt.disabled_alias_groups, ["Healer"]);
            assert!(!legacy.with_file_name("Test-Prompt.toml").exists());

            // A second run in the same session would read the profile
            // files, which hold no items now, and write that over the
            // catalog.
            let before = read(&catalog_path(dir.path()));
            refused(&state, dir.path()).await;
            assert_eq!(read(&catalog_path(dir.path())), before);
            assert!(ProfileConfig::load(&healer).unwrap().aliases.is_empty());
        }

        #[tokio::test]
        async fn the_wizard_never_rewrites_a_profile_file_it_held_at_launch() {
            let dir = tempfile::tempdir().unwrap();
            let set = james_like_set(dir.path());
            write_alias(&set, "Healer", "hh");
            std::fs::write(set.active_path(), UNREADABLE).unwrap();
            let state = launch_state(dir.path()).await;
            // You fix the file in an editor while Vosh runs.
            let mut config = ProfileConfig::default();
            config.aliases.push(vosh_alias::Alias::new("kk", "kick %1"));
            let fixed = config.to_toml().unwrap();
            std::fs::write(set.active_path(), &fixed).unwrap();

            assert_eq!(
                refused(&state, dir.path()).await,
                "Vosh could not read the Default profile file when it started, so it will not \
                 change the file. Restart Vosh and try again."
            );
            assert_eq!(read(&set.active_path()), fixed);
            assert!(!catalog_path(dir.path()).exists());
            assert!(!loadouts_path(dir.path()).exists());
            assert!(!dir.path().join("profiles").join("legacy").exists());
        }

        /// Save `name`'s file with `list` as its enabled presets.
        fn write_presets(set: &ProfileSet, name: &str, list: &[&str]) {
            let mut config = ProfileConfig::default();
            config.ui.enabled_presets = list.iter().map(|s| (*s).to_string()).collect();
            config.save(&set.profile_path(name)).unwrap();
        }

        #[tokio::test]
        async fn the_wizard_keeps_off_a_preset_every_character_had_off() {
            let dir = tempfile::tempdir().unwrap();
            let set = james_like_set(dir.path());
            // Both characters turned the potion labels off.
            write_presets(
                &set,
                crate::profile_set::DEFAULT_PROFILE_NAME,
                &["healing_basics"],
            );
            write_presets(&set, "Healer", &["healing_basics", "herb_labels"]);
            let state = launch_state(dir.path()).await;
            super::super::apply_migration(&state, dir.path(), &[], || {})
                .await
                .unwrap();

            let on = vec!["healing_basics".to_string(), "herb_labels".to_string()];
            let (catalog, _) = load_path_b_at_launch(dir.path()).unwrap();
            assert_eq!(catalog.enabled_presets, Some(on.clone()));

            // The first launch in loadout mode, as either character, keeps
            // the potion labels off.
            for name in [crate::profile_set::DEFAULT_PROFILE_NAME, "Healer"] {
                let state = relaunch_as(dir.path(), name).await;
                assert_eq!(state.profile.lock().await.ui.enabled_presets, on);
            }
        }

        /// Everything one character keeps in per profile mode. `n` makes
        /// every value differ between characters, so a profile that comes
        /// back with the defaults or with another character's values
        /// fails the comparison.
        fn character(name: &str, n: u32, presets: &[&str]) -> ProfileConfig {
            use crate::profile_config::{CustomTheme, PaneLayoutPersist, TrackedAffect};
            let pick = |options: &[&str]| options[n as usize % options.len()].to_string();
            let trigger = |what: &str, pattern: &str, group: Option<&str>| vosh_trigger::Trigger {
                name: format!("{name} {what}"),
                patterns: vec![vosh_trigger::TriggerPattern {
                    pattern: pattern.into(),
                    enabled: true,
                }],
                priority: 0,
                enabled: true,
                actions: vec![vosh_trigger::TriggerAction::Send {
                    template: format!("say {what} {n}"),
                }],
                preset: None,
                group: group.map(String::from),
                target: vosh_trigger::TriggerTarget::Line,
            };
            let mut config = ProfileConfig::default();
            config.connection.host = format!("{}.example", name.to_lowercase());
            config.connection.port = 4000 + n as u16;
            config.connection.tls = n % 2 == 1;
            let mut combat = vosh_alias::Alias::new(format!("{name} bash"), "bash %1");
            combat.group = Some("combat".into());
            config.aliases = vec![
                vosh_alias::Alias::new(format!("{name} kick"), format!("kick {n}")),
                combat,
            ];
            config.triggers = vec![
                trigger("greet", "^hi$", None),
                trigger("flee", "^You flee", Some("combat")),
            ];
            config.macros = vec![
                macro_on(&format!("f{n}"), &format!("cast {n}")),
                crate::profile::Macro {
                    group: Some("combat".into()),
                    ..macro_on(&format!("ctrl+{n}"), "flee")
                },
            ];
            config.timers = vec![crate::profile::Timer {
                id: n,
                name: format!("drink {n}"),
                interval_secs: 60 + n,
                command: format!("drink {name}"),
                enabled: n % 2 == 0,
            }];
            config
                .profile_vars
                .insert("target".into(), format!("orc {n}"));
            config.tick.enabled = n % 2 == 0;
            config.tick.interval_secs = 30 + u64::from(n);
            config.tick.auto_fire = Some(format!("stand {n}"));
            config.tick.sound = n % 2 == 0;
            config.tick.reset_pattern = Some(format!("^The day {n} has begun"));
            config.tick.warn_at_secs = Some(u64::from(n));
            config.tick.warn_message = Some(format!("tick {n} soon"));
            config.tick.warn_color = Some("#ff0000".into());
            config.plugins.enabled = vec![format!("plugin {n}")];
            let ui = &mut config.ui;
            ui.enabled_presets = presets.iter().map(|s| (*s).to_string()).collect();
            ui.vitals_values = pick(&["current-max", "current", "percent"]);
            ui.vitals_meter = pick(&["line", "bar", "none"]);
            ui.vitals_density = pick(&["rows", "line"]);
            ui.vitals_warn_thirds = n % 2 == 1;
            ui.vitals.show_delta = n % 2 == 0;
            ui.tracked_affects = vec![
                TrackedAffect {
                    name: format!("Sanctuary {n}"),
                    label: Some(format!("S{n}")),
                },
                TrackedAffect {
                    name: format!("Haste {n}"),
                    label: None,
                },
            ];
            ui.panes = Some(PaneLayoutPersist {
                panel_open: n % 2 == 1,
                panel_width: Some(300 + 20 * n),
                ..PaneLayoutPersist::default_layout()
            });
            ui.custom_themes = vec![CustomTheme {
                id: format!("night-ink-{n}"),
                label: format!("Night ink {n}"),
                ..CustomTheme::default()
            }];
            ui.theme = format!("night-ink-{n}");
            ui.chip_style = pick(&["value_only", "caption", "icon"]);
            ui.moons_position = pick(&["right-edge", "left-edge"]);
            ui.paste_line_delay_ms = 10 * n;
            ui.prompt_template_enabled = true;
            ui.prompt_template = format!("<%h hp {n}>");
            config
        }

        /// What `config` holds besides the items and the preset list the
        /// shared catalog owns, and the group checkbox lists, which name
        /// the catalog groups in loadout mode. As TOML, so it compares
        /// every field.
        fn settings(mut config: ProfileConfig) -> String {
            config.clear_catalog_items();
            config.ui.enabled_presets.clear();
            config.disabled_alias_groups.clear();
            config.disabled_trigger_groups.clear();
            config.disabled_macro_groups.clear();
            config.to_toml().unwrap()
        }

        /// What a save writes to the file of the live profile `p`.
        fn saved_settings(p: &crate::profile::Profile, set: &ProfileSet) -> String {
            let mut config = ProfileConfig::from_profile(p);
            crate::profile_config::strip_global_fields(&mut config, set.scope());
            settings(config)
        }

        /// The aliases, triggers, and macros that are on in `p`.
        fn items_on(p: &crate::profile::Profile) -> Vec<String> {
            let on = |group: Option<&str>, off: &[String]| {
                group.is_none_or(|g| g.is_empty() || !off.iter().any(|o| o == g))
            };
            let alias_off = p.aliases.disabled_groups();
            let trigger_off = p.triggers.disabled_groups();
            let macro_off: Vec<String> = p.disabled_macro_groups.iter().cloned().collect();
            let mut items: Vec<String> = p
                .aliases
                .list()
                .into_iter()
                .filter(|a| a.enabled && on(a.group.as_deref(), &alias_off))
                .map(|a| format!("alias {}", a.name))
                .chain(
                    p.triggers
                        .list()
                        .into_iter()
                        .filter(|t| t.enabled && on(t.group.as_deref(), &trigger_off))
                        .map(|t| format!("trigger {}", t.name)),
                )
                .chain(
                    p.macros
                        .iter()
                        .filter(|m| m.enabled && on(m.group.as_deref(), &macro_off))
                        .map(|m| format!("macro {}", m.key)),
                )
                .collect();
            items.sort();
            items
        }

        /// Every alias, trigger, and macro in the three lists, one JSON
        /// line each, sorted. The item types do not implement `PartialEq`.
        fn item_rows(
            aliases: &[vosh_alias::Alias],
            triggers: &[vosh_trigger::Trigger],
            macros: &[crate::profile::Macro],
        ) -> Vec<String> {
            let mut rows: Vec<String> = aliases
                .iter()
                .map(|a| serde_json::to_string(a).unwrap())
                .chain(triggers.iter().map(|t| serde_json::to_string(t).unwrap()))
                .chain(macros.iter().map(|m| serde_json::to_string(m).unwrap()))
                .collect();
            rows.sort();
            rows
        }

        #[tokio::test]
        async fn the_wizard_keeps_every_setting_of_every_profile() {
            use crate::profile_set::{Scope, ScopeConfig, DEFAULT_PROFILE_NAME};
            let dir = tempfile::tempdir().unwrap();
            let mut set = james_like_set(dir.path());
            // Your themes and panels differ per character, the way James
            // keeps them.
            set.set_scope(ScopeConfig {
                theme: Scope::Profile,
                dock_layout: Scope::Profile,
                ..ScopeConfig::default()
            })
            .unwrap();
            let names = [DEFAULT_PROFILE_NAME, "Healer", "Test-Prompt"];
            let presets: [&[&str]; 3] = [
                &["healing_basics"],
                &["healing_basics", "herb_labels"],
                &["potion_labels"],
            ];
            let mut originals = Vec::new();
            for (n, name) in names.iter().enumerate() {
                let path = set.profile_path(name);
                character(name, n as u32 + 1, presets[n])
                    .save(&path)
                    .unwrap();
                originals.push(read(&path));
            }

            // What each character holds in per profile mode.
            let mut before = Vec::new();
            for name in names {
                let state = relaunch_as(dir.path(), name).await;
                assert!(state.global_catalog.lock().await.is_none());
                let p = state.profile.lock().await;
                assert_eq!(items_on(&p).len(), 6, "{name}");
                before.push((
                    settings(ProfileConfig::from_profile(&p)),
                    items_on(&p),
                    saved_settings(&p, &set),
                ));
            }

            // You build the catalog while you play Default.
            let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
            super::super::apply_migration(&state, dir.path(), &[], || {})
                .await
                .unwrap();

            let (catalog, loadouts) = load_path_b_at_launch(dir.path()).unwrap();
            assert_eq!(loadouts.active, [DEFAULT_PROFILE_NAME]);
            // Every item sits in the catalog once.
            let mut in_catalog: Vec<String> = catalog
                .aliases
                .iter()
                .map(|a| format!("alias {}", a.name))
                .chain(
                    catalog
                        .triggers
                        .iter()
                        .map(|t| format!("trigger {}", t.name)),
                )
                .chain(catalog.macros.iter().map(|m| format!("macro {}", m.key)))
                .collect();
            in_catalog.sort();
            let mut every_item: Vec<String> =
                before.iter().flat_map(|(_, on, _)| on.clone()).collect();
            every_item.sort();
            assert_eq!(in_catalog, every_item);
            let shared_presets = vec![
                "healing_basics".to_string(),
                "herb_labels".to_string(),
                "potion_labels".to_string(),
            ];
            assert_eq!(catalog.enabled_presets, Some(shared_presets.clone()));

            let legacy = dir.path().join("profiles").join("legacy");
            for (n, name) in names.iter().enumerate() {
                let path = set.profile_path(name);
                // A full copy of the file you had waits in legacy.
                assert_eq!(read(&legacy.join(format!("{name}.toml"))), originals[n]);
                // The file stays with every setting but the items.
                let kept = ProfileConfig::load(&path).unwrap();
                assert!(kept.aliases.is_empty(), "{name}");
                assert!(kept.triggers.is_empty(), "{name}");
                assert!(kept.macros.is_empty(), "{name}");
                assert_eq!(
                    kept.connection.host,
                    format!("{}.example", name.to_lowercase())
                );
                assert_eq!(kept.connection.port, 4001 + n as u16);
                // The rewrite kept a backup of the file beside it.
                let backup = format!("{name}.toml.bak.");
                let backups = std::fs::read_dir(path.parent().unwrap())
                    .unwrap()
                    .filter_map(Result::ok)
                    .filter(|e| e.file_name().to_string_lossy().starts_with(&backup))
                    .count();
                assert!(backups >= 1, "{name}");
            }

            // Quit, then open Vosh as each character with its own loadout.
            for (n, name) in names.iter().enumerate() {
                let mut loadouts = crate::loadout_store::load_loadout_set(dir.path()).unwrap();
                loadouts.active = vec![(*name).to_string()];
                save_loadout_set(dir.path(), &loadouts).unwrap();
                let state = relaunch_as(dir.path(), name).await;
                assert!(state.global_catalog.lock().await.is_some(), "{name}");
                {
                    let p = state.profile.lock().await;
                    assert_eq!(
                        settings(ProfileConfig::from_profile(&p)),
                        before[n].0,
                        "{name}"
                    );
                    assert_eq!(items_on(&p), before[n].1, "{name}");
                    assert_eq!(p.ui.enabled_presets, shared_presets, "{name}");
                    // The live stores hold each catalog item once, as the
                    // catalog has it.
                    let aliases: Vec<_> = p.aliases.list().into_iter().cloned().collect();
                    assert_eq!(
                        item_rows(&aliases, &p.triggers.list(), &p.macros),
                        item_rows(&catalog.aliases, &catalog.triggers, &catalog.macros),
                        "{name}"
                    );
                }

                // The first save in loadout mode.
                persist(&state, dir.path()).await;
                let saved = ProfileConfig::load(&set.profile_path(name)).unwrap();
                assert!(saved.aliases.is_empty(), "{name}");
                assert!(saved.triggers.is_empty(), "{name}");
                assert!(saved.macros.is_empty(), "{name}");
                assert_eq!(settings(saved), before[n].2, "{name}");
                let (saved_catalog, _) = load_path_b_at_launch(dir.path()).unwrap();
                assert_eq!(
                    item_rows(
                        &saved_catalog.aliases,
                        &saved_catalog.triggers,
                        &saved_catalog.macros
                    ),
                    item_rows(&catalog.aliases, &catalog.triggers, &catalog.macros),
                    "{name}"
                );
            }
        }

        #[tokio::test]
        async fn each_loadout_turns_on_the_items_its_character_shared() {
            use crate::profile_set::DEFAULT_PROFILE_NAME;
            let dir = tempfile::tempdir().unwrap();
            let set = james_like_set(dir.path());
            let save = |name: &str, aliases: &[(&str, &str)], combat: &[(&str, &str)]| {
                let mut config = ProfileConfig::default();
                for (alias, expansion) in aliases {
                    config
                        .aliases
                        .push(vosh_alias::Alias::new(*alias, *expansion));
                }
                for (alias, expansion) in combat {
                    let mut a = vosh_alias::Alias::new(*alias, *expansion);
                    a.group = Some("combat".into());
                    config.aliases.push(a);
                }
                config.save(&set.profile_path(name)).unwrap();
            };
            // Test-Prompt began as a copy of Default. Both have kk and
            // bash as they are, each changed cc its own way, and each
            // added one alias of its own.
            save(
                DEFAULT_PROFILE_NAME,
                &[("kk", "kick %1"), ("cc", "cast a"), ("dd", "dig")],
                &[("bash", "bash %1")],
            );
            save(
                "Test-Prompt",
                &[("kk", "kick %1"), ("cc", "cast b"), ("tp", "prompt")],
                &[("bash", "bash %1")],
            );
            save("Healer", &[("hh", "heal %1")], &[]);

            let names = [DEFAULT_PROFILE_NAME, "Healer", "Test-Prompt"];
            let mut before = Vec::new();
            for name in names {
                let state = relaunch_as(dir.path(), name).await;
                before.push(items_on(&*state.profile.lock().await));
            }

            let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
            super::super::apply_migration(&state, dir.path(), &[], || {})
                .await
                .unwrap();
            let (catalog, _) = load_path_b_at_launch(dir.path()).unwrap();
            let group = |alias: &str| {
                let found = catalog.aliases.iter().find(|a| a.name == alias).unwrap();
                found.group.clone().unwrap()
            };
            // What both characters had sits in a group of its own.
            assert_eq!(group("kk"), "default+Test-Prompt");
            assert_eq!(group("cc"), "default+Test-Prompt");
            assert_eq!(group("bash"), "default+Test-Prompt.combat");
            assert_eq!(group("dd"), "default");
            assert_eq!(group("tp"), "Test-Prompt");

            for (n, name) in names.iter().enumerate() {
                let mut loadouts = crate::loadout_store::load_loadout_set(dir.path()).unwrap();
                loadouts.active = vec![(*name).to_string()];
                save_loadout_set(dir.path(), &loadouts).unwrap();
                let state = relaunch_as(dir.path(), name).await;
                assert_eq!(items_on(&*state.profile.lock().await), before[n], "{name}");
            }
        }

        #[tokio::test]
        async fn each_loadout_keeps_off_the_groups_its_character_had_off() {
            use crate::profile_set::DEFAULT_PROFILE_NAME;
            let dir = tempfile::tempdir().unwrap();
            let mut set = james_like_set(dir.path());
            set.create("Bard").unwrap();
            // Default has everything on.
            character(DEFAULT_PROFILE_NAME, 1, &[])
                .save(&set.profile_path(DEFAULT_PROFILE_NAME))
                .unwrap();
            // Healer turned its combat group off in every list.
            let mut healer = character("Healer", 2, &[]);
            healer.disabled_alias_groups = vec!["combat".into()];
            healer.disabled_trigger_groups = vec!["combat".into()];
            healer.disabled_macro_groups = vec!["combat".into()];
            healer.save(&set.profile_path("Healer")).unwrap();
            // The Bard has no aliases, triggers, or macros of its own, and
            // Test-Prompt never saved a file.
            let mut bard = ProfileConfig::default();
            bard.profile_vars.insert("target".into(), "rat".into());
            bard.save(&set.profile_path("Bard")).unwrap();

            let names = [DEFAULT_PROFILE_NAME, "Healer", "Test-Prompt", "Bard"];
            let mut before = Vec::new();
            for name in names {
                let state = relaunch_as(dir.path(), name).await;
                before.push(items_on(&*state.profile.lock().await));
            }
            assert_eq!(before[0].len(), 6);
            assert_eq!(before[1].len(), 3);
            assert!(before[2].is_empty());
            assert!(before[3].is_empty());

            let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
            super::super::apply_migration(&state, dir.path(), &[], || {})
                .await
                .unwrap();
            let (_, loadouts) = load_path_b_at_launch(dir.path()).unwrap();
            let groups = |name: &str| loadouts.get(name).unwrap().enabled_groups.clone();
            assert_eq!(groups("Healer"), ["Healer"]);
            assert!(groups("Test-Prompt").is_empty());
            assert!(groups("Bard").is_empty());
            // The Settings group checkboxes of each file say the same.
            let healer = ProfileConfig::load(&set.profile_path("Healer")).unwrap();
            assert_eq!(
                healer.disabled_alias_groups,
                ["Healer.combat", "default", "default.combat"]
            );
            let bard = ProfileConfig::load(&set.profile_path("Bard")).unwrap();
            assert_eq!(bard.profile_vars.get("target").unwrap(), "rat");
            assert_eq!(
                bard.disabled_trigger_groups,
                ["Healer", "Healer.combat", "default", "default.combat"]
            );

            // With its own loadout on, and with that loadout on beside
            // another character's, each character has on what it had.
            for (n, name) in names.iter().enumerate() {
                for other in ["", "Bard"] {
                    let mut loadouts = crate::loadout_store::load_loadout_set(dir.path()).unwrap();
                    loadouts.active = vec![(*name).to_string()];
                    if !other.is_empty() && other != *name {
                        loadouts.active.push(other.to_string());
                    }
                    save_loadout_set(dir.path(), &loadouts).unwrap();
                    let state = relaunch_as(dir.path(), name).await;
                    assert_eq!(items_on(&*state.profile.lock().await), before[n], "{name}");
                    persist(&state, dir.path()).await;
                    let state = relaunch_as(dir.path(), name).await;
                    assert_eq!(items_on(&*state.profile.lock().await), before[n], "{name}");
                }
            }
        }

        #[tokio::test]
        async fn a_wizard_that_cannot_finish_puts_every_file_back() {
            use crate::profile_set::DEFAULT_PROFILE_NAME;
            let dir = tempfile::tempdir().unwrap();
            let mut set = james_like_set(dir.path());
            set.create("Bard").unwrap();
            set.create("Rich").unwrap();
            // Test-Prompt never saved a file, and every other character
            // holds its own items and settings.
            for (n, name) in [DEFAULT_PROFILE_NAME, "Healer", "Bard", "Rich"]
                .iter()
                .enumerate()
            {
                character(name, n as u32 + 1, &[])
                    .save(&set.profile_path(name))
                    .unwrap();
            }
            let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
            // Each file as launch left it, with its custom themes moved to
            // global.toml.
            let files: Vec<(std::path::PathBuf, Option<String>)> = set
                .list()
                .iter()
                .map(|entry| {
                    let path = set.profile_path(&entry.name);
                    let text = path.exists().then(|| read(&path));
                    (path, text)
                })
                .collect();

            // Something stops the rewrite of the Bard file, after the
            // catalog and the files before it saved, the way a full disk
            // or a lock on the file would.
            let blocked = set.profile_path("Bard").with_extension("toml.tmp");
            std::fs::create_dir(&blocked).unwrap();
            let mut written = false;
            let err = super::super::apply_migration(&state, dir.path(), &[], || written = true)
                .await
                .unwrap_err();
            assert!(
                err.starts_with("Vosh could not save the Bard profile file ("),
                "{err}"
            );
            assert!(
                err.ends_with(
                    "), so it put back every file it changed. Your profiles work as before, \
                     and you can try again."
                ),
                "{err}"
            );

            // Nothing changed, so Vosh stays in per profile mode and its
            // saves go on.
            assert!(!written);
            assert!(!catalog_path(dir.path()).exists());
            assert!(!loadouts_path(dir.path()).exists());
            for (path, text) in &files {
                match text {
                    Some(text) => assert_eq!(&read(path), text, "{}", path.display()),
                    None => assert!(!path.exists(), "{}", path.display()),
                }
            }
            let state = relaunch_as(dir.path(), "Bard").await;
            assert!(state.global_catalog.lock().await.is_none());
            {
                let p = state.profile.lock().await;
                assert_eq!(items_on(&p).len(), 6);
                let kept = ProfileConfig::from_profile(&p);
                assert_eq!(kept.profile_vars.get("target").unwrap(), "orc 3");
            }

            // Once the file saves again, the wizard runs.
            std::fs::remove_dir(&blocked).unwrap();
            super::super::apply_migration(&state, dir.path(), &[], || written = true)
                .await
                .unwrap();
            assert!(written);
            assert!(catalog_path(dir.path()).exists());
            let bard = ProfileConfig::load(&set.profile_path("Bard")).unwrap();
            assert!(bard.aliases.is_empty());
            assert_eq!(bard.profile_vars.get("target").unwrap(), "orc 3");
        }

        /// Build the catalog over Default, Healer, and Test-Prompt, each set
        /// up as `character` sets it up, and open Vosh again as Default.
        /// Returns the live state and the items each character had on,
        /// in that order.
        async fn converted_three(
            dir: &std::path::Path,
        ) -> (super::super::SharedState, Vec<Vec<String>>) {
            use crate::profile_set::DEFAULT_PROFILE_NAME;
            let set = james_like_set(dir);
            let names = [DEFAULT_PROFILE_NAME, "Healer", "Test-Prompt"];
            for (n, name) in names.iter().enumerate() {
                character(name, n as u32 + 1, &[])
                    .save(&set.profile_path(name))
                    .unwrap();
            }
            let mut before = Vec::new();
            for name in names {
                let state = relaunch_as(dir, name).await;
                before.push(items_on(&*state.profile.lock().await));
            }
            let state = relaunch_as(dir, DEFAULT_PROFILE_NAME).await;
            super::super::apply_migration(&state, dir, &[], || {})
                .await
                .unwrap();
            (relaunch_as(dir, DEFAULT_PROFILE_NAME).await, before)
        }

        /// The catalog items the live stores of `state` hold.
        async fn live_rows(state: &super::super::SharedState) -> Vec<String> {
            let p = state.profile.lock().await;
            let aliases: Vec<_> = p.aliases.list().into_iter().cloned().collect();
            item_rows(&aliases, &p.triggers.list(), &p.macros)
        }

        #[tokio::test]
        async fn a_save_right_after_a_switch_keeps_the_catalog() {
            let dir = tempfile::tempdir().unwrap();
            let (state, _) = converted_three(dir.path()).await;
            let (catalog, _) = load_path_b_at_launch(dir.path()).unwrap();
            let rows = item_rows(&catalog.aliases, &catalog.triggers, &catalog.macros);
            assert_eq!(rows.len(), 18);

            // You switch to Healer while a save waits for the switch to
            // let go of the lock.
            {
                let _persist_guard = super::super::PERSIST_LOCK.lock().await;
                super::super::switch_live_profile(&state, "Healer")
                    .await
                    .unwrap();
            }
            persist(&state, dir.path()).await;

            let (saved, _) = load_path_b_at_launch(dir.path()).unwrap();
            assert_eq!(
                item_rows(&saved.aliases, &saved.triggers, &saved.macros),
                rows
            );
            assert_eq!(live_rows(&state).await, rows);
        }
    }
}

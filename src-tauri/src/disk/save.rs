//! The save engine. It writes the live profile to the active profile's
//! file and global.toml, and in loadout mode to catalog.toml and
//! loadouts.toml too. A durable change saves a couple of seconds after
//! its burst settles, and [`PERSIST_LOCK`] keeps two writes off one file.
//!
//! The lock order. A step that holds two of these locks at once takes
//! them in this order, so no two tasks wait on each other for good.
//!
//! 1. [`PERSIST_LOCK`]. Nothing waits for it while holding another lock,
//!    so `#profile save`, which runs under the profile lock, only tries
//!    it.
//! 2. The loadouts and the plugin manager in [`AppState`].
//! 3. The profile.
//! 4. The profile set. The save in loadout mode reads the sharing scope
//!    from it while it holds the profile, so a step that holds the set
//!    never waits for the profile.
//!
//! The session slot comes before every lock the session task takes, and
//! the log before the log reader. docs/architecture.md says why.
//!
//! [`AppState`]: crate::app::state::AppState

use tauri::{AppHandle, Manager};
use tracing::warn;

use crate::app::events::{broadcast, line_effect_events};
use crate::app::state::{SharedState, AUTO_PERSIST_SUPPRESSED, MIGRATION_RELAUNCH_PENDING};
use crate::profile::Profile;
use crate::profile_config::{strip_global_fields, GlobalConfig, ProfileConfig};

/// Debounce generation for `mark_profile_dirty`: each mark bumps it, and
/// the delayed persist only fires if no newer mark arrived while waiting.
static PROFILE_DIRTY_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

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
pub(crate) fn mark_profile_dirty<R: tauri::Runtime>(app: &AppHandle<R>) {
    AUTO_PERSIST_SUPPRESSED.store(false, std::sync::atomic::Ordering::Release);
    schedule_profile_persist(app);
}

/// Persist shortly after the burst settles, like `mark_profile_dirty`,
/// but without counting as consent to save a profile that `#profile
/// reset` or `#profile load` left diverged from disk. While that holds,
/// the write is skipped and the change waits in memory for the next
/// durable change or an explicit `#profile save`. For incidental edits
/// such as a pane layout drag.
pub(crate) fn schedule_profile_persist<R: tauri::Runtime>(app: &AppHandle<R>) {
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
pub(crate) async fn settle_line_effects<R: tauri::Runtime>(
    app: &AppHandle<R>,
    effects: crate::input::LineEffects,
) {
    if effects.replaced {
        AUTO_PERSIST_SUPPRESSED.store(true, std::sync::atomic::Ordering::Release);
    }
    if effects.replaced || effects.tick_changed {
        let shared: SharedState = app.state::<SharedState>().inner().clone();
        let events = {
            let p = shared.profile.lock().await;
            line_effect_events(&effects, &p)
        };
        for (event, payload) in events {
            broadcast(app, event, &payload);
        }
    }
    // A durable change after the replace counts as wanting the live
    // state saved, the way a line typed after `#profile reset` does.
    if effects.dirty {
        mark_profile_dirty(app);
    }
}

/// Snapshot the live profile and write it to the active profile's file
/// under `<app_data_dir>/profiles/<active>.toml`. Failures are logged
/// but not surfaced — callers don't want a UI toggle to fail because
/// the disk is full mid-flight, and the in-memory state is still
/// correct for the rest of the session.
pub(crate) async fn persist_profile<R: tauri::Runtime>(app: &AppHandle<R>, state: &SharedState) {
    // Serialize whole-persist runs. The debounced dirty-persist and the
    // exit-time flush can overlap each other or an inline command
    // persist, and Settings can write an inactive profile's file.
    let _persist_guard = PERSIST_LOCK.lock().await;
    persist_profile_locked(app, state).await;
}

/// When [`save_then_broadcast`] saves the live profile. Each command
/// names the policy it has always had.
#[derive(Debug, Clone, Copy)]
pub(crate) enum SavePolicy {
    /// Save at once, through [`persist_profile`].
    Now,
    /// Save at once, unless `#profile reset` or `#profile load` left the
    /// live profile apart from disk and no durable change has wanted it
    /// saved since ([`AUTO_PERSIST_SUPPRESSED`]).
    NowUnlessHeld,
    /// Save once the burst settles, through [`schedule_profile_persist`],
    /// which keeps that hold. For edits that land several times a second.
    SoonUnlessHeld,
}

/// Save the live profile by `policy`, then send `event` with `payload`
/// to every window. A command that changed one part of the profile ends
/// this way, so the save always comes before the event.
pub(crate) async fn save_then_broadcast<R: tauri::Runtime, S: serde::Serialize + ?Sized>(
    app: &AppHandle<R>,
    state: &SharedState,
    policy: SavePolicy,
    event: &str,
    payload: &S,
) {
    match policy {
        SavePolicy::Now => persist_profile(app, state).await,
        SavePolicy::NowUnlessHeld => {
            if !AUTO_PERSIST_SUPPRESSED.load(std::sync::atomic::Ordering::Acquire) {
                persist_profile(app, state).await;
            }
        }
        SavePolicy::SoonUnlessHeld => schedule_profile_persist(app),
    }
    broadcast(app, event, payload);
}

/// The body of [`persist_profile`]. Call with [`PERSIST_LOCK`] held.
pub(crate) async fn persist_profile_locked<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
) {
    let app_data = app.path().app_data_dir().ok();
    persist_state(state, app_data.as_deref()).await;
}

/// [`persist_profile_locked`] over the app data folder `app_data`, so a
/// test can run it over a folder of its own. Call with [`PERSIST_LOCK`]
/// held. A file Vosh could not read at launch is never written, see
/// [`crate::profile_config::hold_unread`].
pub(crate) async fn persist_state(state: &SharedState, app_data: Option<&std::path::Path>) {
    persist_state_with(state, app_data, &MIGRATION_RELAUNCH_PENDING).await;
}

/// [`persist_state`] with `relaunch_pending` in place of
/// [`MIGRATION_RELAUNCH_PENDING`], so a test can save while a relaunch
/// is pending without touching the flag every other test reads.
pub(crate) async fn persist_state_with(
    state: &SharedState,
    app_data: Option<&std::path::Path>,
    relaunch_pending: &std::sync::atomic::AtomicBool,
) {
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
    // the migration. The same holds after a launch that could not finish
    // a wizard run that stopped partway.
    if relaunch_pending.load(std::sync::atomic::Ordering::Acquire) {
        tracing::debug!("persist skipped: post-migration window before relaunch");
        return;
    }

    // Resolve the active profile's path and global.toml through the
    // ProfileSet. Without one, launch could not read profiles.toml (or
    // has not run yet), so nothing names the file to write and the save
    // writes nothing. A root profile.toml written here would replace
    // profiles/default.toml at the next launch that finds no
    // profiles.toml, see `ProfileSet::load_or_migrate`.
    let (per_profile_path, global_path, scope) = {
        let guard = state.profile_set.lock().await;
        let Some(set) = guard.as_ref() else {
            tracing::debug!("persist skipped: the profile set is not loaded");
            return;
        };
        (set.active_path(), set.global_path(), *set.scope())
    };

    let (per_profile_snapshot, global_snapshot) = {
        let p = state.profile.lock().await;
        (
            active_profile_file(&p, Some(&scope)),
            GlobalConfig::from_profile(&p, &scope),
        )
    };

    if let Err(e) = per_profile_snapshot.save(&per_profile_path) {
        warn!(error = %e, path = %per_profile_path.display(), "auto-save per-profile failed");
    }
    if let Err(e) = global_snapshot.save(&global_path) {
        warn!(error = %e, path = %global_path.display(), "auto-save global failed");
    }
}

/// What a save in per profile mode writes to the file of the active
/// profile `p`. The fields `scope` keeps global go to global.toml, so
/// they are left out here and never saved twice. Honors the scope of each
/// category, and a category kept per profile stays in the file.
pub(crate) fn active_profile_file(
    p: &Profile,
    scope: Option<&crate::profile_set::ScopeConfig>,
) -> ProfileConfig {
    let mut snapshot = ProfileConfig::from_profile(p);
    if let Some(scope) = scope {
        strip_global_fields(&mut snapshot, scope);
    }
    snapshot
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

#[cfg(test)]
pub(crate) mod tests {
    use crate::app::state::AppState;
    use crate::profile_config::ProfileConfig;
    use crate::profile_set::tests::james_like_set;

    pub(crate) fn affect(name: &str) -> crate::profile_config::TrackedAffect {
        crate::profile_config::TrackedAffect {
            name: name.into(),
            label: None,
        }
    }

    pub(crate) async fn live_affects(state: &super::SharedState) -> Vec<String> {
        let p = state.profile.lock().await;
        p.ui.tracked_affects
            .iter()
            .map(|t| t.name.clone())
            .collect()
    }

    pub(crate) const UNREADABLE: &str = "tracked = = [\n";

    /// Launch over the profile set in `dir` the way app/launch.rs runs
    /// it, and hand back the app state with the notices launch kept.
    pub(crate) async fn launch_state(dir: &std::path::Path) -> super::SharedState {
        let state: super::SharedState = std::sync::Arc::new(AppState::default());
        crate::app::launch::load_profiles(&state, dir).await;
        assert!(state.profile_set.lock().await.is_some());
        state
    }

    /// The save a Settings edit, a slash command debounce, or quit runs.
    pub(crate) async fn persist(state: &super::SharedState, dir: &std::path::Path) {
        let _persist_guard = super::PERSIST_LOCK.lock().await;
        super::persist_state(state, Some(dir)).await;
    }

    async fn change_scope(state: &super::SharedState) -> Result<(), String> {
        let _persist_guard = super::PERSIST_LOCK.lock().await;
        let scope = crate::profile_set::ScopeConfig {
            theme: crate::profile_set::Scope::Profile,
            ..crate::profile_set::ScopeConfig::default()
        };
        crate::profile::shared::change_scope_locked(state, scope)
            .await
            .map(|_| ())
    }

    pub(crate) fn read(path: &std::path::Path) -> String {
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
        let leftover = &state.take_launch_notices();
        assert!(leftover.is_empty(), "{leftover:?}");

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

    /// Bug 5. A save after a launch that could not read profiles.toml
    /// writes no root profile.toml, since a launch that finds no
    /// profiles.toml moves that file over your Default profile.
    #[tokio::test]
    async fn a_save_after_profiles_toml_did_not_read_keeps_your_default_profile() {
        let dir = tempfile::tempdir().unwrap();
        let set = james_like_set(dir.path());
        let mut default = ProfileConfig::default();
        default.ui.tracked_affects = vec![affect("Sanctuary")];
        default.save(&set.active_path()).unwrap();
        let index = dir.path().join("profiles.toml");
        std::fs::write(&index, UNREADABLE).unwrap();

        // Launch cannot read the index, so the session runs on the
        // defaults. You change a setting and Vosh saves.
        let state: super::SharedState = std::sync::Arc::new(AppState::default());
        crate::app::launch::load_profiles(&state, dir.path()).await;
        assert!(state.profile_set.lock().await.is_none());
        state.profile.lock().await.ui.tracked_affects = vec![affect("Haste")];
        persist(&state, dir.path()).await;
        assert!(!dir.path().join("profile.toml").exists());

        // You delete profiles.toml to recover and launch again.
        std::fs::remove_file(&index).unwrap();
        let state = launch_state(dir.path()).await;
        assert_eq!(live_affects(&state).await, ["Sanctuary"]);
        let saved = ProfileConfig::load(&set.active_path()).unwrap();
        assert_eq!(saved.ui.tracked_affects[0].name, "Sanctuary");
    }

    #[tokio::test]
    async fn a_loadout_save_leaves_the_presets_to_a_catalog_still_waiting() {
        use std::sync::Arc;
        let dir = tempfile::tempdir().unwrap();
        let state: super::SharedState = Arc::new(AppState::default());
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
}

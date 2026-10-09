//! The save engine. It writes each open profile to its own file and
//! global.toml, and in loadout mode to catalog.toml and loadouts.toml
//! too, then lays what it wrote to the files every profile shares over
//! the other open profiles. A durable change saves its profile a couple
//! of seconds after its burst settles, and [`PERSIST_LOCK`] keeps two
//! writes off one file.
//!
//! The lock order. A step that holds two of these locks at once takes
//! them in this order, so no two tasks wait on each other for good.
//!
//! 1. A session's slot, which `disconnect` holds while the task it ends
//!    takes the locks below.
//! 2. [`PERSIST_LOCK`]. Nothing else waits for it while holding another
//!    lock, so `#profile save`, which runs under the profile lock, only
//!    tries it.
//! 3. The turn [`broadcast_sessions`] holds while it reads the session
//!    rows and sends them. Under it the rows take the session map and
//!    then each session's connection, one at a time.
//! 4. The session map in [`AppState`]. Its holders take no other lock
//!    of this list.
//! 5. The loadouts in [`AppState`].
//! 6. The profiles the sessions play, the one that opened first first.
//! 7. The profile set. The save in loadout mode reads the sharing scope
//!    from it while it holds the profile, so a step that holds the set
//!    never waits for the profile.
//! 8. A session's connection, one at a time.
//!
//! The catalog and the plugin manager are only ever held alone, and the
//! log comes before the log reader. docs/architecture.md says why.
//!
//! [`AppState`]: crate::app::state::AppState
//! [`broadcast_sessions`]: crate::sessions::broadcast_sessions

use std::sync::Arc;

use tauri::{AppHandle, Manager};
use tracing::warn;

use crate::app::events::{broadcast, line_effect_events};
use crate::app::state::SharedState;
use crate::loadouts::catalog::{lay_catalog_change_over, GlobalCatalog};
use crate::profile::file::ProfileConfig;
use crate::profile::live::Profile;
use crate::profile::open::OpenProfile;
use crate::profile::shared::{strip_global_fields, GlobalConfig};
use crate::sessions::Session;

/// Serializes every write of a profile file: an open profile's
/// persist, Settings edits to an inactive profile's file, a profile
/// switch from its flush through loading the next file, and the
/// rename, delete and copy of a profile file. `write_with_backup` uses
/// a fixed .tmp name per target, so two writers on one file would break
/// its atomic write, and a write racing a rename or a switch would land
/// on a file the other side already moved or read. Take it before the
/// profile set lock, never while holding it.
pub(crate) static PERSIST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Record that durable state of `open` changed (slash commands, Lua
/// mutations) and persist it shortly after the burst settles. Keeps disk
/// writes off latency-sensitive paths while guaranteeing the change
/// reaches its file or catalog.toml within a couple of seconds; the exit
/// hook flushes immediately as a backstop.
pub(crate) fn mark_profile_dirty<R: tauri::Runtime>(app: &AppHandle<R>, open: &Arc<OpenProfile>) {
    open.hold(false);
    schedule_profile_persist(app, open);
}

/// Persist shortly after the burst settles, like `mark_profile_dirty`,
/// but without counting as consent to save a profile that `#profile
/// reset` or `#profile load` left diverged from disk. While that holds,
/// the write is skipped and the change waits in memory for the next
/// durable change or an explicit `#profile save`. For incidental edits
/// such as a pane layout drag. The count and the hold are those of
/// `open`, so a burst on one profile never holds back another's save.
pub(crate) fn schedule_profile_persist<R: tauri::Runtime>(
    app: &AppHandle<R>,
    open: &Arc<OpenProfile>,
) {
    let shared: SharedState = app.state::<SharedState>().inner().clone();
    let open = open.clone();
    let gen = open.mark();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if open.marks() != gen {
            return; // a newer mark restarted the clock
        }
        if open.held() {
            return; // a #profile reset/load intervened
        }
        persist_profile(&shared, &open).await;
    });
}

/// Act on what a run of input lines in `session` asked of `open`, the
/// profile they ran under, which the caller read while it held the lock,
/// since a switch may move the session once it lets go. Every path that
/// runs lines through the input pipeline calls this after it releases the
/// profile lock and the connection's, so they all save alike.
///
/// Slash commands (#alias, #trigger, #var, #endrec, #import-tintin,
/// ...) and durable Lua actions change the profile without saving it, so
/// this marks it dirty and the debounced save writes it. Without that,
/// what you author this way would vanish on restart.
/// `#profile reset` and `#profile load` are deliberate exceptions:
/// reset blanks the LIVE profile only (the help documents `#profile
/// save` as the explicit write and `load` as the undo), so
/// auto-persisting it would wipe the on-disk profile, and in loadout
/// mode the shared catalog. They also suppress the passive flushes (exit,
/// debounce) until the next durable change says the in-memory state is
/// wanted again. The pipeline itself says when one replaced the profile
/// (see [`crate::input::run_line`]), so a `#profile load` whose file does
/// not read suppresses nothing, and spelling variants ("#profile  reset",
/// "# profile load") cannot slip past into the dirty mark and persist the
/// just-blanked profile.
///
/// `replaced_by` names which of the two last laid a profile over. Every
/// other session on the profile then takes the new tick settings and
/// `[prompt]` table, and prints a line that says which session did it.
pub(crate) async fn settle_line_effects<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: &Session,
    open: &Arc<OpenProfile>,
    effects: crate::input::LineEffects,
    replaced_by: Option<crate::input::ProfileReplace>,
) {
    let shared: SharedState = app.state::<SharedState>().inner().clone();
    if effects.replaced {
        open.hold(true);
    }
    // A line that laid a profile over handed its own connection the new
    // tick settings and `[prompt]` table as it ran, and every other
    // session on the profile takes them now. Otherwise those sessions
    // follow a change to the tick settings and take what you chose in the
    // table.
    if let Some(how) = replaced_by {
        let before = effects.tick_before.as_ref();
        crate::input::profile::hand_to_other_sessions(app, &shared, session, open, how, before)
            .await;
        crate::input::keep_profile_echo_mark(&shared, open).await;
    } else {
        if let Some(before) = effects.tick_before.as_ref() {
            crate::tick::follow_in_other_sessions(&shared, session.id, open, before).await;
        }
        if let Some(chosen) = effects.prompt.as_ref() {
            crate::prompt::choose_in_other_sessions(&shared, session.id, open, chosen).await;
        }
    }
    // The windows hear the settings of the profile in front alone, so a
    // replace or a `#tick` in a session on a profile behind waits for the
    // selection that brings it to the front.
    if (effects.replaced || effects.tick_before.is_some()) && shared.in_front(open) {
        let events = {
            let p = open.lock().await;
            line_effect_events(&shared, &effects, &p)
        };
        for (event, payload) in events {
            broadcast(app, event, &payload);
        }
    }
    // A durable change after the replace counts as wanting the live
    // state saved, the way a line typed after `#profile reset` does.
    if effects.dirty {
        mark_profile_dirty(app, open);
    }
}

/// Snapshot `open` and write it to its own file under
/// `<app_data_dir>/profiles/<name>.toml`. Failures are logged but not
/// surfaced — callers don't want a UI toggle to fail because the disk is
/// full mid-flight, and the in-memory state is still correct for the
/// rest of the session.
pub(crate) async fn persist_profile(state: &SharedState, open: &Arc<OpenProfile>) {
    // Serialize whole-persist runs. The debounced dirty-persist and the
    // exit-time flush can overlap each other or an inline command
    // persist, and Settings can write an inactive profile's file.
    let _persist_guard = PERSIST_LOCK.lock().await;
    persist_state(state, open).await;
}

/// When [`save_then_broadcast`] saves a profile. Each command names the
/// policy it has always had.
#[derive(Debug, Clone, Copy)]
pub(crate) enum SavePolicy {
    /// Save at once, through [`persist_profile`].
    Now,
    /// Save at once, unless `#profile reset` or `#profile load` left the
    /// profile apart from disk and no durable change has wanted it
    /// saved since ([`OpenProfile::held`]).
    ///
    /// [`OpenProfile::held`]: crate::profile::open::OpenProfile::held
    NowUnlessHeld,
    /// Save once the burst settles, through [`schedule_profile_persist`],
    /// which keeps that hold. For edits that land several times a second.
    SoonUnlessHeld,
}

/// Save `open`, the profile a command changed, by `policy`, then send
/// `event` with `payload` to every window while `open` is the profile in
/// front, see [`AppState::in_front`]. A command that changed one part of
/// the profile ends this way, so the save always comes before the event.
///
/// [`AppState::in_front`]: crate::app::state::AppState::in_front
pub(crate) async fn save_then_broadcast<R: tauri::Runtime, S: serde::Serialize + ?Sized>(
    app: &AppHandle<R>,
    state: &SharedState,
    open: &Arc<OpenProfile>,
    policy: SavePolicy,
    event: &str,
    payload: &S,
) {
    save_by(app, state, open, policy).await;
    if state.in_front(open) {
        broadcast(app, event, payload);
    }
}

/// Save `open` by `policy`, for a command that tells no window of the
/// change but the ones its caller picks.
pub(crate) async fn save_by<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    open: &Arc<OpenProfile>,
    policy: SavePolicy,
) {
    match policy {
        SavePolicy::Now => persist_profile(state, open).await,
        SavePolicy::NowUnlessHeld => {
            if !open.held() {
                persist_profile(state, open).await;
            }
        }
        SavePolicy::SoonUnlessHeld => schedule_profile_persist(app, open),
    }
}

/// The body of [`persist_profile`]. Call with [`PERSIST_LOCK`] held. A
/// file Vosh could not read at launch is never written, see
/// [`crate::disk::atomic::hold_unread`]. A profile that closed saved as
/// it closed, and its file may have moved since, so it writes nothing.
pub(crate) async fn persist_state(state: &SharedState, open: &Arc<OpenProfile>) {
    if !state
        .open_profiles()
        .iter()
        .any(|kept| Arc::ptr_eq(kept, open))
    {
        return;
    }
    let name = open.name();
    // Loadout mode branch. When `state.global_catalog` is `Some`, the user is
    // post-migration: authored items live in catalog.toml and the live
    // Profile is the cache. Write the live aliases / triggers / macros
    // back to the catalog plus the loadout set, and every other setting
    // to the profile's own file; the per-profile branch below is
    // skipped entirely.
    if state.global_catalog.lock().await.is_some() {
        if let Some(dir) = state.app_data.get() {
            persist_loadout_mode(state, open, name.as_deref(), dir).await;
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
    if state
        .relaunch_pending
        .load(std::sync::atomic::Ordering::Acquire)
    {
        tracing::debug!("persist skipped: post-migration window before relaunch");
        return;
    }

    // Resolve the profile's path and global.toml through the
    // ProfileSet. Without one, launch could not read profiles.toml (or
    // has not run yet), so nothing names the file to write and the save
    // writes nothing. A root profile.toml written here would replace
    // profiles/default.toml at the next launch that finds no
    // profiles.toml, see `ProfileSet::load_or_migrate`.
    let (per_profile_path, global_path, scope) = {
        let guard = state.profile_set.lock().await;
        let (Some(set), Some(name)) = (guard.as_ref(), name) else {
            tracing::debug!("persist skipped: the profile set is not loaded");
            return;
        };
        (set.profile_path(&name), set.global_path(), *set.scope())
    };

    let (per_profile_snapshot, global_snapshot) = {
        let p = open.lock().await;
        (
            active_profile_file(&p, Some(&scope)),
            GlobalConfig::from_profile(&p, &scope),
        )
    };

    if let Err(e) = per_profile_snapshot.save(&per_profile_path) {
        warn!(error = %e, path = %per_profile_path.display(), "auto-save per-profile failed");
    }
    match global_snapshot.save(&global_path) {
        Ok(()) => lay_save_over_others(state, open, None, Some(&global_snapshot)).await,
        Err(e) => warn!(error = %e, path = %global_path.display(), "auto-save global failed"),
    }
}

/// Lay what the save of `saved` wrote to the files every profile shares
/// over each other open profile, so a later save from one of them writes
/// back no older copy. `catalog` is the change the save made to the
/// catalog, from the copy before it to the one it wrote, which each
/// profile takes through its own loadout stack, and `shared` is what it
/// wrote to global.toml. Call with [`PERSIST_LOCK`] held, once the save
/// let go of `saved`. Each profile is locked in turn, one at a time.
async fn lay_save_over_others(
    state: &SharedState,
    saved: &Arc<OpenProfile>,
    catalog: Option<(&GlobalCatalog, &GlobalCatalog)>,
    shared: Option<&GlobalConfig>,
) {
    let others: Vec<_> = state
        .open_profiles()
        .into_iter()
        .filter(|open| !Arc::ptr_eq(open, saved))
        .collect();
    if others.is_empty() {
        return;
    }
    let loadouts = state.loadout_set.lock().await.clone();
    for open in others {
        let mut p = open.lock().await;
        if let Some((before, after)) = catalog {
            let gate = loadouts
                .as_ref()
                .map(|set| set.for_profile(p.name.as_deref()));
            lay_catalog_change_over(&mut p, before, after, gate.as_deref());
        }
        if let Some(shared) = shared {
            shared.apply_to(&mut p);
        }
    }
}

/// What a save in per profile mode writes to the file of the profile
/// `p`. The fields `scope` keeps global go to global.toml, so
/// they are left out here and never saved twice. Honors the scope of each
/// category, and a category kept per profile stays in the file.
pub(crate) fn active_profile_file(
    p: &Profile,
    scope: Option<&crate::profile::shared::ScopeConfig>,
) -> ProfileConfig {
    let mut snapshot = ProfileConfig::from_profile(p);
    if let Some(scope) = scope {
        strip_global_fields(&mut snapshot, scope);
    }
    snapshot
}

/// Saves in loadout mode. Snapshots the authored items of `open`, which
/// `name` names once a profile set loaded, into `catalog.toml` and the in-memory `LoadoutSet` into
/// `loadouts.toml`, both via the same atomic-write-with-backup
/// pipeline the per-profile branch uses. Falls through to the legacy
/// `global.toml` write so theme / font / `dock_layout` edits land on
/// the same path in both modes. The other open profiles then take the
/// change to the catalog and what global.toml holds, see
/// [`lay_save_over_others`].
async fn persist_loadout_mode(
    state: &SharedState,
    open: &Arc<OpenProfile>,
    name: Option<&str>,
    dir: &std::path::Path,
) {
    let before = state.global_catalog.lock().await.clone();
    // A catalog that has not taken the enabled presets yet waits for a
    // launch that reads a profile file (see
    // `loadouts::presets::adopt_catalog_presets`), so a save leaves the list
    // out rather than write the live profile's list alone.
    let presets_waiting = before.as_ref().is_some_and(|c| c.enabled_presets.is_none());
    // Catalog. Pull aliases / triggers / macros directly from the live
    // Profile. The catalog is the authoritative source in loadout mode
    // so an overwrite here is correct — anything the user typed via
    // #alias / Settings made it into Profile and now into the file.
    let (catalog, global_snapshot, scope) = {
        let p = open.lock().await;
        // The enabled presets ride along, since the preset triggers they
        // name live in the catalog too.
        let mut catalog = crate::loadouts::catalog::GlobalCatalog::from_profile(&p);
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

    // The switch in `loadouts::set::set_active_loadouts` sets the active
    // list, and this save writes whatever the set holds.
    let set_snapshot = state.loadout_set.lock().await.clone();

    if let Err(e) = crate::loadouts::catalog::save_global_catalog(dir, &catalog) {
        warn!(error = %e, "catalog auto-save failed");
    }
    if let Some(set) = set_snapshot {
        if let Err(e) = crate::loadouts::set::save_loadout_set(dir, &set) {
            warn!(error = %e, "loadout set auto-save failed");
        }
    }

    let (global_path, per_profile_path) = {
        let guard = state.profile_set.lock().await;
        match guard.as_ref() {
            Some(set) => (
                Some(set.global_path()),
                name.map(|name| set.profile_path(name)),
            ),
            None => (None, None),
        }
    };
    let mut shared = None;
    if let Some(g) = global_path {
        match global_snapshot.save(&g) {
            Ok(()) => shared = Some(&global_snapshot),
            Err(e) => {
                warn!(error = %e, path = %g.display(), "loadout mode global auto-save failed");
            }
        }
    }
    // The per-profile file keeps every UI setting outside the shared
    // categories (tracked_affects, theme_terminal_colors, the vitals
    // config, paste pacing, the moons position, the chip style, the dock
    // layout when its scope is profile, ...), so loadout mode writes it
    // too, as per profile mode does. The catalog owns the aliases,
    // triggers, and macros, so they are left out of the file. Otherwise a
    // launch as a profile saved earlier would lay its older copy over the
    // catalog, bringing back a deleted item or undoing a newer edit. The
    // catalog owns the enabled presets too. The file keeps a copy of the
    // shared list, which the next load replaces with the catalog's.
    if let Some(p) = per_profile_path {
        let mut per_profile_snapshot = {
            let live = open.lock().await;
            ProfileConfig::from_profile(&live)
        };
        per_profile_snapshot.clear_catalog_items();
        // The disabled group lists stay, since they are where the
        // Settings group checkboxes persist in loadout mode, and the
        // catalog has no field for them. When an active loadout declares
        // enabled_groups, the loadouts impose the group state and
        // replace these lists at the next apply, see `loadouts::gating`.
        strip_global_fields(&mut per_profile_snapshot, &scope);
        if let Err(e) = per_profile_snapshot.save(&p) {
            warn!(
                error = %e,
                path = %p.display(),
                "loadout mode per-profile auto-save failed",
            );
        }
    }
    // The other open profiles take the change, then the new catalog goes
    // into `state.global_catalog`, the copy the next save compares with
    // and a switch lays over the next profile.
    let changed = before.as_ref().filter(|before| **before != catalog);
    lay_save_over_others(
        state,
        open,
        changed.map(|before| (before, &catalog)),
        shared,
    )
    .await;
    *state.global_catalog.lock().await = Some(catalog);
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::app::state::AppState;
    use crate::profile::file::ProfileConfig;
    use crate::profile::set::ProfileSet;
    use crate::profile::tests::james_like_set;

    pub(crate) fn affect(name: &str) -> crate::profile::ui::TrackedAffect {
        crate::profile::ui::TrackedAffect {
            name: name.into(),
            label: None,
        }
    }

    pub(crate) async fn live_affects(state: &super::SharedState) -> Vec<String> {
        let p = state.selected_profile().await;
        p.ui.tracked_affects
            .iter()
            .map(|t| t.name.clone())
            .collect()
    }

    pub(crate) const UNREADABLE: &str = "tracked = = [\n";

    /// Load the profiles in `dir` the way launch loads them after its
    /// upgrades, and hand back the app state with the notices launch
    /// kept.
    pub(crate) async fn launch_state(dir: &std::path::Path) -> super::SharedState {
        let state: super::SharedState = std::sync::Arc::new(AppState::default());
        let set = ProfileSet::load_or_migrate(dir.to_path_buf()).unwrap();
        crate::app::launch::load_profiles(&state, set).await;
        assert!(state.profile_set.lock().await.is_some());
        state.app_data.set(dir.to_path_buf()).unwrap();
        state
    }

    /// The save a Settings edit, a slash command debounce, or quit runs,
    /// of the profile the selected session plays.
    pub(crate) async fn persist(state: &super::SharedState) {
        let _persist_guard = super::PERSIST_LOCK.lock().await;
        super::persist_state(state, &state.selected_session().profile()).await;
    }

    async fn change_scope(state: &super::SharedState) -> Result<(), String> {
        let _persist_guard = super::PERSIST_LOCK.lock().await;
        let scope = crate::profile::shared::ScopeConfig {
            theme: crate::profile::shared::Scope::Profile,
            ..crate::profile::shared::ScopeConfig::default()
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
            state.take_launch_messages(),
            [
                "Vosh could not read the Default profile file, so it will not save over it. Fix \
              the file or switch to another profile."
            ]
        );
        // You tell once. A second take finds nothing.
        let leftover = &state.take_launch_messages();
        assert!(leftover.is_empty(), "{leftover:?}");

        // The app keeps running on the defaults, and an edit saves.
        state.selected_profile().await.ui.tracked_affects = vec![affect("Haste")];
        persist(&state).await;
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
            state.take_launch_messages(),
            [crate::profile::file::UNREAD_GLOBAL_NOTICE]
        );
        {
            let mut p = state.selected_profile().await;
            p.ui.theme = "nord".into();
            p.ui.tracked_affects = vec![affect("Fly")];
        }
        persist(&state).await;

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
        persist(&state).await;
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
        crate::app::launch::load(&state, dir.path()).await;
        assert!(state.profile_set.lock().await.is_none());
        state.selected_profile().await.ui.tracked_affects = vec![affect("Haste")];
        persist(&state).await;
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
        state.app_data.set(dir.path().to_path_buf()).unwrap();
        state.set_profiles(james_like_set(dir.path())).await;
        // No profile file read at launch, so the catalog took no list and
        // the live profile kept its own.
        *state.global_catalog.lock().await =
            Some(crate::loadouts::catalog::GlobalCatalog::default());
        state.selected_profile().await.ui.enabled_presets = vec!["healing_basics".into()];

        persist(&state).await;
        let saved = crate::loadouts::catalog::load_global_catalog(dir.path()).unwrap();
        assert_eq!(saved.enabled_presets, None);

        // Once the catalog holds a list, the saves keep it current.
        state
            .global_catalog
            .lock()
            .await
            .as_mut()
            .unwrap()
            .enabled_presets = Some(Vec::new());
        persist(&state).await;
        let saved = crate::loadouts::catalog::load_global_catalog(dir.path()).unwrap();
        assert_eq!(saved.enabled_presets, Some(vec!["healing_basics".into()]));
    }
}

//! What launch loads from the app data folder before any window opens:
//! the profile set with the active profile, then the shared catalog and
//! the loadouts when you use loadout mode. lib.rs runs [`load`] from its
//! setup hook, and tests run it to relaunch over a folder of their own.

use std::path::Path;

use tracing::{error, info};

use crate::commands::SharedState;
use crate::loadout_store::WizardRun;
use crate::{loadout_store, profile_config, profile_set};

/// What [`load`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Launch {
    /// Loadout mode is live, so the caller flips the input layer over.
    pub(crate) loadout_mode: bool,
    /// A shared catalog wizard run is still not done, see
    /// [`WizardRun::Unfinished`]. The caller holds every save and every
    /// profile switch until the next launch finishes the run.
    pub(crate) wizard_unfinished: bool,
}

/// Everything launch loads, in order. A shared catalog wizard run that
/// stopped partway finishes first, so nothing loads a file it had yet to
/// write. Then the prompt capture triggers move into the profiles, once,
/// see [`crate::prompt_migration`], and each preset a build adds comes
/// on once, see [`crate::preset_rollout`]. Then the profiles load, see
/// [`load_profiles`], and loadout mode starts when catalog.toml is on
/// disk, see [`load_loadout_mode`]. While the run stays unfinished, the
/// move and loadout mode wait, since a profile file may still hold its
/// items under their old group names and would lay them over the catalog
/// for every character. The session runs on the active profile file
/// alone.
pub(crate) async fn load(state: &SharedState, app_data: &Path) -> Launch {
    let run = loadout_store::finish_wizard_run(app_data);
    state.add_launch_notices(run.notices());
    let relaunch_pending =
        crate::commands::MIGRATION_RELAUNCH_PENDING.load(std::sync::atomic::Ordering::Acquire);
    if run != WizardRun::Unfinished && !relaunch_pending {
        // Before any profile loads, so the live profile reads the files
        // as the move left them. It writes inactive profile files too.
        let _persist = crate::commands::PERSIST_LOCK.lock().await;
        state.add_launch_notices(crate::prompt_migration::run(app_data));
        crate::preset_rollout::run(app_data);
    }
    load_profiles(state, app_data).await;
    if run == WizardRun::Unfinished {
        return Launch {
            loadout_mode: false,
            wizard_unfinished: true,
        };
    }
    Launch {
        loadout_mode: load_loadout_mode(state, app_data).await,
        wizard_unfinished: false,
    }
}

/// Load (or migrate from the legacy single-file layout) the named
/// profile collection. Then load whichever profile the index marks as
/// active into the live profile, and overlay the shared global.toml
/// (theme, font, dock layout, keep last, auto update) so those UI prefs
/// stay the same across every profile.
pub(crate) async fn load_profiles(state: &SharedState, app_data: &Path) {
    let mut set = match profile_set::ProfileSet::load_or_migrate(app_data.to_path_buf()) {
        Ok(set) => set,
        Err(e) => {
            error!(error = %e, "failed to load profile set; using in-memory defaults");
            return;
        }
    };
    // What an earlier session left to tell you, once.
    state.add_launch_notices(set.take_notices());
    // Before any profile loads, move the custom themes older profile
    // files still hold into global.toml, which owns the list from here
    // on. It writes only files it read, so a file that does not read
    // stays as it is.
    match profile_config::migrate_custom_themes(&set) {
        Ok(0) => {}
        Ok(files) => {
            info!(files, "moved custom themes into global.toml");
        }
        Err(e) => {
            error!(error = %e, "failed to move custom themes into global.toml");
        }
    }
    // A file that does not read keeps the defaults in its place for this
    // session, and no save writes over it. The notices tell you so once
    // the main window shows.
    let notices = {
        let mut p = state.profile.lock().await;
        profile_config::load_at_launch(&set, &mut p)
    };
    state.add_launch_notices(notices);
    state.note_active_profile(set.active_name());
    *state.profile_set.lock().await = Some(set);
}

/// Loadout mode startup, after [`load_profiles`], see [`load`]. When catalog.toml is
/// present, start from the catalog (shared defaults) and overlay the
/// triggers, aliases, and macros of the profile file ON TOP. Same-name
/// entries from the profile file win, and new names are added. Before
/// this, the catalog overlay outright replaced per-profile state, which
/// silently wiped any trigger or alias a user authored against their
/// profile file. Loadouts still apply on top to gate catalog groups by
/// the active `enabled_groups` set. Returns true when loadout mode is
/// live, so the caller can flip the input layer over.
pub(crate) async fn load_loadout_mode(state: &SharedState, app_data: &Path) -> bool {
    if !loadout_store::path_b_mode_active(app_data) {
        return false;
    }
    let (mut catalog, set) = match loadout_store::load_path_b_at_launch(app_data) {
        Ok(files) => files,
        // A file that does not read keeps the session on the profile
        // files alone. Both files are held so no save writes a catalog
        // without your shared items, and the notices tell you so.
        Err(notices) => {
            error!("Path B files present but failed to load; falling back to per-profile state");
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
            .map(loadout_store::profile_preset_lists)
    } else {
        None
    };
    let presets_moved = {
        let mut p = state.profile.lock().await;
        loadout_store::adopt_catalog_presets(&mut catalog, &mut p, preset_lists.as_ref())
    };
    if let Some(lists) = &preset_lists {
        state.add_launch_notices(lists.unread_notices(presets_moved));
    }
    if presets_moved {
        match loadout_store::save_global_catalog(app_data, &catalog) {
            Ok(()) => info!("moved the enabled presets into catalog.toml"),
            Err(e) => {
                error!(error = %e, "failed to save the enabled presets to catalog.toml");
            }
        }
    }
    {
        let mut p = state.profile.lock().await;
        // Snapshot what the per-profile load just put into the live
        // stores so we can replay it on top of the catalog.
        let per_profile_aliases: Vec<_> = p.aliases.list().into_iter().cloned().collect();
        let per_profile_triggers: Vec<_> = p.triggers.list();
        let per_profile_macros = p.macros.clone();
        // The per-profile file also restored the user's group checkbox
        // state; carry it across the catalog rebuild or every group
        // comes back enabled.
        let alias_disabled = p.aliases.disabled_groups();
        let trigger_disabled = p.triggers.disabled_groups();

        // Catalog first.
        let mut aliases = vosh_automation::alias::AliasStore::new();
        for a in &catalog.aliases {
            aliases.set(a.clone());
        }
        // Per-profile overrides by name.
        for a in per_profile_aliases {
            aliases.set(a);
        }
        aliases.set_disabled_groups(alias_disabled);
        p.aliases = aliases;

        let mut triggers = vosh_trigger::TriggerStore::new();
        for t in &catalog.triggers {
            if let Err(e) = triggers.set(t.clone()) {
                info!(error = %e, "catalog trigger rejected at startup");
            }
        }
        for t in per_profile_triggers {
            if let Err(e) = triggers.set(t) {
                info!(error = %e, "per-profile trigger rejected at startup");
            }
        }
        triggers.set_disabled_groups(trigger_disabled);
        p.triggers = triggers;

        // Macros: catalog defaults, per-profile overrides by `key` (the
        // canonical keypress identifier). Per-profile entries with no
        // catalog match are simply appended.
        let mut macros = catalog.macros.clone();
        for m in per_profile_macros {
            macros.retain(|x| x.key != m.key);
            macros.push(m);
        }
        p.macros = macros;

        loadout_store::apply_effective_state(&set, &mut p);
    }
    *state.global_catalog.lock().await = Some(catalog);
    *state.loadout_set.lock().await = Some(set);
    info!("loaded Path B catalog + loadout set");
    true
}

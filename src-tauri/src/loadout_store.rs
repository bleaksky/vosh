//! Runtime adapter between the [`crate::loadout`] data model and the
//! live runtime stores. Phase B2.
//!
//! ## What this module provides
//!
//! Three concerns sit here together because they are the boundary
//! where Path B's static catalog + loadout types meet the runtime:
//!
//!   1. **Persistence**: `<app_data_dir>/catalog.toml` and
//!      `<app_data_dir>/loadouts.toml` load and save, both routed
//!      through the same atomic backup pipeline that `profile.toml`
//!      and `global.toml` use ([`crate::profile_config::write_with_backup`]).
//!   2. **Mode detection**: [`path_b_mode_active`] returns true iff
//!      `catalog.toml` exists. This is the trigger `AppState` reads
//!      at startup to decide whether to source items from the catalog
//!      (Path B) or from each per-profile file (legacy).
//!   3. **Runtime apply**: [`apply_loadout_state`] takes a
//!      [`LoadoutSet`] plus the live [`AliasStore`] / [`TriggerStore`]
//!      / [`Profile`] handles and writes each store's
//!      `disabled_groups` from the union of every active loadout's
//!      `enabled_groups`. The macro store has no wrapper so the
//!      profile's `disabled_macro_groups` set is updated in place.
//!
//! ## Apply semantics
//!
//! `disabled_groups` is the user's durable Settings-checkbox state in
//! BOTH modes. Loadouts only impose group state when at least one
//! active loadout actually declares `enabled_groups`: then the disabled
//! set becomes every cataloged group NOT in the union of active
//! loadouts' `enabled_groups`. When no active loadout declares any
//! groups, the loadouts have no opinion and the user's checkbox state
//! is left untouched (and persists via the per-profile snapshot).
//! Ungrouped items (whose `group` is `None` or empty) are never
//! disabled because the store already treats them as always-on.
//!
//! ## Phase B2 scope
//!
//! `AppState` integration and the Tauri command surface
//! (`loadout_list`, `loadout_set_active`, etc.) land in a follow-up
//! commit so the runtime data path can be reviewed first. The
//! module-level `dead_code` allow exists for the same reason as in
//! [`crate::loadout`] and [`crate::migration`]: no callers yet.
#![allow(dead_code)]

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::loadout::{GlobalCatalog, LoadoutSet};
use crate::profile::Profile;
use crate::profile_config::write_with_backup;

/// Filename of the global catalog inside the app data directory.
const CATALOG_FILE: &str = "catalog.toml";
/// Filename of the loadout collection inside the app data directory.
const LOADOUTS_FILE: &str = "loadouts.toml";

#[derive(Debug, Error)]
pub(crate) enum LoadoutStoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("toml parse error: {0}")]
    Deserialize(#[from] toml::de::Error),
}

/// Path to `catalog.toml` under the given app data directory.
pub(crate) fn catalog_path(app_data: &Path) -> PathBuf {
    app_data.join(CATALOG_FILE)
}

/// Path to `loadouts.toml` under the given app data directory.
pub(crate) fn loadouts_path(app_data: &Path) -> PathBuf {
    app_data.join(LOADOUTS_FILE)
}

/// True iff `catalog.toml` exists at the app data root. The wizard in
/// Phase B3 is what writes that file the first time; until then this
/// returns false and `AppState` stays on the legacy per-profile path.
pub(crate) fn path_b_mode_active(app_data: &Path) -> bool {
    catalog_path(app_data).exists()
}

/// Load the global catalog. Missing file yields an empty catalog
/// rather than an error so first-launch and Path-A-only installs do
/// not have to special-case absent state.
pub(crate) fn load_global_catalog(app_data: &Path) -> Result<GlobalCatalog, LoadoutStoreError> {
    let path = catalog_path(app_data);
    if !path.exists() {
        return Ok(GlobalCatalog::default());
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(toml::from_str(&text)?)
}

/// Persist the global catalog atomically with a rolling backup. See
/// [`crate::profile_config::write_with_backup`] for the rename and
/// retention guarantees.
pub(crate) fn save_global_catalog(
    app_data: &Path,
    catalog: &GlobalCatalog,
) -> Result<(), LoadoutStoreError> {
    let text = toml::to_string_pretty(catalog)?;
    write_with_backup(&catalog_path(app_data), &text)?;
    Ok(())
}

/// Load the loadout collection. Missing file yields an empty
/// `LoadoutSet` (no loadouts, no active stack) so callers can treat
/// "fresh install" and "user wiped their loadouts" identically.
pub(crate) fn load_loadout_set(app_data: &Path) -> Result<LoadoutSet, LoadoutStoreError> {
    let path = loadouts_path(app_data);
    if !path.exists() {
        return Ok(LoadoutSet::default());
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(toml::from_str(&text)?)
}

/// Persist the loadout collection atomically with a rolling backup.
pub(crate) fn save_loadout_set(app_data: &Path, set: &LoadoutSet) -> Result<(), LoadoutStoreError> {
    let text = toml::to_string_pretty(set)?;
    write_with_backup(&loadouts_path(app_data), &text)?;
    Ok(())
}

/// Apply whatever group state the loadout set actually calls for:
/// explicit dormancy wins, otherwise the union rules run (including
/// the no-opinion guard). Every apply point (startup, profile switch,
/// active-list change) routes through here so the deactivate-all kill
/// switch cannot be undone by a later rebuild.
pub(crate) fn apply_effective_state(set: &LoadoutSet, profile: &mut Profile) {
    if set.dormant {
        apply_dormant_state(profile);
    } else {
        apply_loadout_state(set, profile);
    }
}

/// Disable every group in every store: the deactivate-all "keep the
/// catalog dormant" kill switch. Persisted like any checkbox state, so
/// dormancy survives restart (and the no-opinion guard in
/// `apply_loadout_state` will not undo it).
pub(crate) fn apply_dormant_state(profile: &mut Profile) {
    let alias_groups: Vec<String> = profile
        .aliases
        .groups()
        .into_iter()
        .map(|(name, _)| name)
        .filter(|n| !n.is_empty())
        .collect();
    profile.aliases.set_disabled_groups(alias_groups);
    let trigger_groups: Vec<String> = profile
        .triggers
        .groups()
        .into_iter()
        .map(|(name, _)| name)
        .filter(|n| !n.is_empty())
        .collect();
    profile.triggers.set_disabled_groups(trigger_groups);
    profile.disabled_macro_groups = profile
        .macros
        .iter()
        .filter_map(|m| m.group.clone())
        .filter(|g| !g.is_empty())
        .collect();
}

/// Write the per-store `disabled_groups` on a live `Profile` from the
/// active loadouts' union.
///
/// For each store the rule is: gather every group name that appears
/// on at least one item; the disabled set is that universe minus the
/// [`LoadoutSet::effective_enabled_groups`] output. Ungrouped items
/// stay live because the stores already treat empty / `None` groups
/// as always-on.
///
/// This is idempotent and does not touch the items themselves, only
/// the per-store bookkeeping the runtime gates on.
pub(crate) fn apply_loadout_state(set: &LoadoutSet, profile: &mut Profile) {
    let enabled: HashSet<String> = set.effective_enabled_groups().into_iter().collect();

    // No active loadout declares any enabled_groups: the loadouts have no
    // opinion about groups, so leave the user's Settings checkbox state
    // alone. The old behavior treated the empty union as "disable every
    // group", which force-disabled everything at startup for users whose
    // loadouts do not manage groups at all — and made the Settings group
    // checkboxes impossible to persist.
    if enabled.is_empty() {
        return;
    }

    // Aliases. `AliasStore::groups()` returns (name, enabled), we
    // only need the names to build the universe.
    let alias_groups: HashSet<String> = profile
        .aliases
        .groups()
        .into_iter()
        .map(|(name, _)| name)
        .filter(|n| !n.is_empty())
        .collect();
    let alias_disabled: Vec<String> = alias_groups.difference(&enabled).cloned().collect();
    profile.aliases.set_disabled_groups(alias_disabled);

    // Triggers, same shape.
    let trigger_groups: HashSet<String> = profile
        .triggers
        .groups()
        .into_iter()
        .map(|(name, _)| name)
        .filter(|n| !n.is_empty())
        .collect();
    let trigger_disabled: Vec<String> = trigger_groups.difference(&enabled).cloned().collect();
    profile.triggers.set_disabled_groups(trigger_disabled);

    // Macros. No wrapper store, the profile owns the set directly.
    let macro_groups: HashSet<String> = profile
        .macros
        .iter()
        .filter_map(|m| m.group.as_ref())
        .filter(|g| !g.is_empty())
        .cloned()
        .collect();
    profile.disabled_macro_groups = macro_groups
        .difference(&enabled)
        .cloned()
        .collect::<BTreeSet<String>>();
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use vosh_alias::Alias;
    use vosh_trigger::{Trigger, TriggerAction, TriggerPattern, TriggerTarget};

    use crate::loadout::Loadout;
    use crate::profile::Macro;

    fn make_trigger(name: &str, pattern: &str, group: Option<&str>) -> Trigger {
        Trigger {
            name: name.to_string(),
            patterns: vec![TriggerPattern {
                pattern: pattern.to_string(),
                enabled: true,
            }],
            priority: 0,
            enabled: true,
            actions: vec![TriggerAction::Send {
                template: "noop".to_string(),
            }],
            preset: None,
            group: group.map(String::from),
            target: TriggerTarget::Line,
        }
    }

    fn alias_with_group(name: &str, expansion: &str, group: Option<&str>) -> Alias {
        let mut a = Alias::new(name, expansion);
        a.group = group.map(String::from);
        a
    }

    fn tmpdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "vosh-loadout-store-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn path_b_mode_active_false_when_catalog_missing() {
        let dir = tmpdir();
        assert!(!path_b_mode_active(&dir));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn path_b_mode_active_true_after_save() {
        let dir = tmpdir();
        save_global_catalog(&dir, &GlobalCatalog::default()).unwrap();
        assert!(path_b_mode_active(&dir));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_missing_files_yields_defaults() {
        let dir = tmpdir();
        let catalog = load_global_catalog(&dir).unwrap();
        let set = load_loadout_set(&dir).unwrap();
        assert!(catalog.aliases.is_empty());
        assert!(catalog.triggers.is_empty());
        assert!(catalog.macros.is_empty());
        assert!(set.loadouts.is_empty());
        assert!(set.active.is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn loadout_set_round_trips_through_disk() {
        let dir = tmpdir();
        let mut set = LoadoutSet::default();
        let mut warrior = Loadout::empty("warrior");
        warrior.enabled_groups = vec!["combat-melee".into(), "wartools".into()];
        set.loadouts = vec![warrior];
        set.active = vec!["warrior".into()];

        save_loadout_set(&dir, &set).unwrap();
        let loaded = load_loadout_set(&dir).unwrap();
        assert_eq!(loaded.active, vec!["warrior".to_string()]);
        assert_eq!(loaded.loadouts.len(), 1);
        assert_eq!(loaded.loadouts[0].name, "warrior");
        assert_eq!(
            loaded.loadouts[0].enabled_groups,
            vec!["combat-melee", "wartools"]
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn catalog_round_trips_through_disk() {
        let dir = tmpdir();
        let mut catalog = GlobalCatalog::default();
        let mut alias = Alias::new("kk", "kick %1");
        alias.group = Some("combat".into());
        catalog.aliases.push(alias);
        save_global_catalog(&dir, &catalog).unwrap();
        let loaded = load_global_catalog(&dir).unwrap();
        assert_eq!(loaded.aliases.len(), 1);
        assert_eq!(loaded.aliases[0].name, "kk");
        assert_eq!(loaded.aliases[0].group.as_deref(), Some("combat"));
        fs::remove_dir_all(&dir).ok();
    }

    fn profile_with_items(
        aliases: Vec<Alias>,
        triggers: Vec<Trigger>,
        macros: Vec<Macro>,
    ) -> Profile {
        let mut p = Profile {
            macros,
            ..Profile::default()
        };
        for a in aliases {
            p.aliases.set(a);
        }
        for t in triggers {
            p.triggers.set(t).unwrap();
        }
        p
    }

    #[test]
    fn apply_with_no_declared_groups_leaves_state_untouched() {
        // When nothing is active (or no active loadout declares
        // enabled_groups), the loadouts have no opinion: nothing gets
        // disabled and the user's checkbox state stands. The old
        // semantics disabled every group here, which force-disabled
        // everything at startup for users whose loadouts do not manage
        // groups.
        let mut profile = profile_with_items(
            vec![alias_with_group("kk", "kick %1", Some("combat"))],
            vec![make_trigger("dot", r"burning", Some("buffs"))],
            vec![Macro {
                key: "F1".into(),
                command: "north".into(),
                group: Some("movement".into()),
            }],
        );

        let set = LoadoutSet::default();
        apply_loadout_state(&set, &mut profile);

        assert!(profile.aliases.disabled_groups().is_empty());
        assert!(profile.triggers.disabled_groups().is_empty());
        assert!(profile.disabled_macro_groups.is_empty());
    }

    #[test]
    fn apply_with_one_active_loadout_enables_its_groups_only() {
        let mut profile = profile_with_items(
            vec![
                alias_with_group("kk", "kick %1", Some("combat")),
                alias_with_group("smelt", "smelt iron ore", Some("crafting")),
            ],
            vec![make_trigger("dot", r"burning", Some("buffs"))],
            vec![Macro {
                key: "F1".into(),
                command: "north".into(),
                group: Some("movement".into()),
            }],
        );

        let mut set = LoadoutSet::default();
        let mut warrior = Loadout::empty("warrior");
        warrior.enabled_groups = vec!["combat".into(), "movement".into()];
        set.loadouts = vec![warrior];
        set.active = vec!["warrior".into()];

        apply_loadout_state(&set, &mut profile);

        let alias_disabled = profile.aliases.disabled_groups();
        assert!(!alias_disabled.contains(&"combat".to_string()));
        assert!(alias_disabled.contains(&"crafting".to_string()));
        assert!(profile
            .triggers
            .disabled_groups()
            .contains(&"buffs".to_string()));
        assert!(!profile.disabled_macro_groups.contains("movement"));
    }

    #[test]
    fn apply_unions_multiple_active_loadouts() {
        let mut profile = profile_with_items(
            vec![
                alias_with_group("kk", "kick %1", Some("combat")),
                alias_with_group("smelt", "smelt iron ore", Some("crafting")),
                alias_with_group("hb", "say hello there", Some("social")),
            ],
            vec![],
            vec![],
        );

        let mut warrior = Loadout::empty("warrior");
        warrior.enabled_groups = vec!["combat".into()];
        let mut crafter = Loadout::empty("crafter");
        crafter.enabled_groups = vec!["crafting".into()];
        let set = LoadoutSet {
            dormant: false,
            loadouts: vec![warrior, crafter],
            active: vec!["warrior".into(), "crafter".into()],
        };

        apply_loadout_state(&set, &mut profile);

        let disabled = profile.aliases.disabled_groups();
        assert!(!disabled.contains(&"combat".to_string()));
        assert!(!disabled.contains(&"crafting".to_string()));
        assert!(disabled.contains(&"social".to_string()));
    }

    #[test]
    fn apply_leaves_checkbox_state_alone_when_no_loadout_declares_groups() {
        // Loadouts with empty enabled_groups have no opinion about
        // groups: the user's Settings checkbox state must survive both
        // startup and loadout activation. The old semantics treated the
        // empty union as "disable every group", which made group
        // checkboxes impossible to persist for users whose loadouts do
        // not manage groups.
        let mut profile = profile_with_items(
            vec![
                alias_with_group("kk", "kick %1", Some("combat")),
                alias_with_group("hh", "heal %1", Some("heals")),
            ],
            vec![],
            vec![],
        );
        profile
            .aliases
            .set_disabled_groups(vec!["combat".to_string()]);
        let set = LoadoutSet {
            dormant: false,
            loadouts: vec![Loadout::empty("default"), Loadout::empty("Healer")],
            active: vec!["default".into(), "Healer".into()],
        };
        apply_loadout_state(&set, &mut profile);
        assert_eq!(
            profile.aliases.disabled_groups(),
            vec!["combat".to_string()]
        );
    }

    #[test]
    fn apply_still_authoritative_when_a_loadout_declares_groups() {
        // A loadout that DOES declare enabled_groups keeps the original
        // semantics: the disabled set becomes the universe minus the
        // union, overriding checkbox state.
        let mut profile = profile_with_items(
            vec![
                alias_with_group("kk", "kick %1", Some("combat")),
                alias_with_group("hh", "heal %1", Some("heals")),
            ],
            vec![],
            vec![],
        );
        profile
            .aliases
            .set_disabled_groups(vec!["heals".to_string()]);
        let mut warrior = Loadout::empty("warrior");
        warrior.enabled_groups = vec!["heals".into()];
        let set = LoadoutSet {
            dormant: false,
            loadouts: vec![warrior],
            active: vec!["warrior".into()],
        };
        apply_loadout_state(&set, &mut profile);
        assert_eq!(
            profile.aliases.disabled_groups(),
            vec!["combat".to_string()]
        );
    }

    #[test]
    fn dormant_state_disables_every_group_in_every_store() {
        // Deactivating the last loadout is an explicit "make the catalog
        // dormant" request. apply_loadout_state's no-opinion guard would
        // leave everything running, so loadouts_set_active calls this
        // instead: disable the full group universe across all three
        // stores.
        let mut profile = profile_with_items(
            vec![
                alias_with_group("kk", "kick %1", Some("combat")),
                alias_with_group("ungrouped", "look", None),
            ],
            vec![make_trigger("dot", r"burning", Some("buffs"))],
            vec![Macro {
                key: "F1".into(),
                command: "north".into(),
                group: Some("movement".into()),
            }],
        );

        apply_dormant_state(&mut profile);

        assert_eq!(
            profile.aliases.disabled_groups(),
            vec!["combat".to_string()]
        );
        assert_eq!(
            profile.triggers.disabled_groups(),
            vec!["buffs".to_string()]
        );
        assert!(profile.disabled_macro_groups.contains("movement"));
    }

    #[test]
    fn effective_state_honors_dormant_over_empty_active() {
        // dormant=true with active=[] must disable everything even
        // though apply_loadout_state alone treats the empty union as
        // no-opinion. This is what re-imposes the kill switch at
        // startup and across profile switches.
        let mut profile = profile_with_items(
            vec![alias_with_group("kk", "kick %1", Some("combat"))],
            vec![],
            vec![],
        );
        let set = LoadoutSet {
            dormant: true,
            ..Default::default()
        };
        apply_effective_state(&set, &mut profile);
        assert_eq!(
            profile.aliases.disabled_groups(),
            vec!["combat".to_string()]
        );
    }

    #[test]
    fn dormant_flag_round_trips_through_disk() {
        let dir = tmpdir();
        let set = LoadoutSet {
            dormant: true,
            ..Default::default()
        };
        save_loadout_set(&dir, &set).unwrap();
        assert!(load_loadout_set(&dir).unwrap().dormant);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn apply_is_idempotent() {
        // Running apply twice in a row produces identical state. Catches
        // accidental accumulation bugs where the function appended to
        // disabled_groups instead of replacing it.
        let mut profile = profile_with_items(
            vec![alias_with_group("kk", "kick %1", Some("combat"))],
            vec![],
            vec![],
        );
        let mut warrior = Loadout::empty("warrior");
        warrior.enabled_groups = vec!["combat".into()];
        let set = LoadoutSet {
            dormant: false,
            loadouts: vec![warrior],
            active: vec!["warrior".into()],
        };

        apply_loadout_state(&set, &mut profile);
        let first = profile.aliases.disabled_groups();
        apply_loadout_state(&set, &mut profile);
        let second = profile.aliases.disabled_groups();
        assert_eq!(first, second);
    }

    #[test]
    fn apply_ignores_ungrouped_items() {
        // Items with no group must never appear in disabled_groups (the
        // stores treat empty group as always-on independently, but the
        // apply function should not surface "" into the set either).
        let mut profile = profile_with_items(
            vec![Alias::new("loose", "look")], // group: None
            vec![],
            vec![],
        );
        let set = LoadoutSet::default();
        apply_loadout_state(&set, &mut profile);
        assert!(profile.aliases.disabled_groups().is_empty());
    }
}

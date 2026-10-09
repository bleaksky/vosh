//! The group state the loadouts lay over the live profile.
//! [`apply_loadout_state`] takes a [`LoadoutSet`] plus the live
//! [`AliasStore`] / [`TriggerStore`] / [`Profile`] handles and writes
//! each store's `disabled_groups` from the union of every active
//! loadout's `enabled_groups`. The macro store has no wrapper so the
//! profile's `disabled_macro_groups` set is updated in place.
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
//! [`AliasStore`]: vosh_automation::alias::AliasStore
//! [`TriggerStore`]: vosh_automation::trigger::TriggerStore

use std::collections::{BTreeSet, HashSet};

use super::presets::hold_profile_keys;
use super::set::LoadoutSet;
use crate::profile::live::Profile;

/// What the loadouts decide about one group of the catalog, while they
/// decide it: on or off, and the active loadouts that decide. Every
/// apply point lays this state over the group again, so a change made
/// to it by hand lasts only until the next switch, launch or change
/// another open profile saves to the catalog, and the switch on the
/// group heading in Settings waits while it holds.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct LoadoutHold {
    /// Whether the loadouts turn the group on.
    pub(crate) on: bool,
    /// The active loadouts that decide, in the order they are on: the
    /// ones that list the group when they turn it on, and every one that
    /// lists any group when they leave it off. Empty while every loadout
    /// is off and the catalog is dormant.
    pub(crate) by: Vec<String>,
}

/// What `set` decides about the catalog group `group`, the way
/// [`apply_effective_state`] lays it over the stores, or None while the
/// loadouts have no opinion and the group stays as you left it.
pub(crate) fn loadout_hold(set: &LoadoutSet, group: &str) -> Option<LoadoutHold> {
    if set.dormant {
        return Some(LoadoutHold {
            on: false,
            by: Vec::new(),
        });
    }
    let enabled = set.effective_enabled_groups();
    if enabled.is_empty() {
        return None;
    }
    let on = enabled.iter().any(|g| g == group);
    let mut by: Vec<String> = Vec::new();
    for name in &set.active {
        let Some(loadout) = set.get(name) else {
            continue;
        };
        let decides = if on {
            loadout.enabled_groups.iter().any(|g| g == group)
        } else {
            !loadout.enabled_groups.is_empty()
        };
        if decides && !by.contains(name) {
            by.push(name.clone());
        }
    }
    Some(LoadoutHold { on, by })
}

/// What `set` decides about a group no loadout lists, such as one you
/// just named in Settings: off while any active loadout lists groups or
/// the catalog is dormant, so it goes off at the next launch, profile
/// switch or Loadouts save, and None while the loadouts have no opinion.
/// Vosh never adds the group to a loadout for you, since a loadout says
/// which groups a character plays and a new group may belong to another
/// character.
pub(crate) fn new_group_hold(set: &LoadoutSet) -> Option<LoadoutHold> {
    if set.dormant {
        return Some(LoadoutHold {
            on: false,
            by: Vec::new(),
        });
    }
    if set.effective_enabled_groups().is_empty() {
        return None;
    }
    let mut by: Vec<String> = Vec::new();
    for name in &set.active {
        let lists = set
            .get(name)
            .is_some_and(|loadout| !loadout.enabled_groups.is_empty());
        if lists && !by.contains(name) {
            by.push(name.clone());
        }
    }
    Some(LoadoutHold { on: false, by })
}

/// Apply whatever group state the loadout set actually calls for:
/// explicit dormancy wins, otherwise the union rules run (including
/// the no-opinion guard). Every apply point (startup, profile switch,
/// active-list change) routes through here so the deactivate-all kill
/// switch cannot be undone by a later rebuild. The preset macros then
/// take the hold of the macro groups as they now stand, see
/// [`hold_profile_keys`].
pub(crate) fn apply_effective_state(set: &LoadoutSet, profile: &mut Profile) {
    if set.dormant {
        apply_dormant_state(profile);
    } else {
        apply_loadout_state(set, profile);
    }
    hold_profile_keys(profile);
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

    use vosh_automation::alias::Alias;
    use vosh_automation::trigger::{Trigger, TriggerAction};

    use crate::loadouts::set::Loadout;
    use crate::profile::live::Macro;

    fn make_trigger(name: &str, pattern: &str, group: Option<&str>) -> Trigger {
        Trigger {
            group: group.map(String::from),
            ..Trigger::new(
                name,
                pattern,
                TriggerAction::Send {
                    template: "noop".to_string(),
                },
            )
        }
    }

    fn alias_with_group(name: &str, expansion: &str, group: Option<&str>) -> Alias {
        let mut a = Alias::new(name, expansion);
        a.group = group.map(String::from);
        a
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

    fn set_of(active: &[&str], dormant: bool) -> LoadoutSet {
        let mut healer = Loadout::empty("Healer");
        healer.enabled_groups = vec!["heals".into(), "combat".into()];
        let mut warrior = Loadout::empty("Warrior");
        warrior.enabled_groups = vec!["combat".into()];
        LoadoutSet {
            active: active.iter().map(|n| (*n).to_string()).collect(),
            dormant,
            loadouts: vec![healer, warrior, Loadout::empty("Quiet")],
            ..Default::default()
        }
    }

    #[test]
    fn a_new_group_is_held_off_as_a_group_no_loadout_lists() {
        let both = set_of(&["Healer", "Warrior", "Quiet"], false);
        assert_eq!(new_group_hold(&both), loadout_hold(&both, "maren"));
        assert_eq!(
            new_group_hold(&both),
            Some(LoadoutHold {
                on: false,
                by: vec!["Healer".into(), "Warrior".into()],
            })
        );
        assert_eq!(new_group_hold(&set_of(&["Quiet"], false)), None);
        assert_eq!(
            new_group_hold(&set_of(&[], true)),
            loadout_hold(&set_of(&[], true), "maren")
        );
    }

    #[test]
    fn a_hold_names_the_loadouts_that_decide_a_group() {
        let hold = |set: &LoadoutSet, group| loadout_hold(set, group);
        let both = set_of(&["Healer", "Warrior", "Quiet"], false);
        assert_eq!(
            hold(&both, "combat"),
            Some(LoadoutHold {
                on: true,
                by: vec!["Healer".into(), "Warrior".into()],
            })
        );
        assert_eq!(
            hold(&both, "heals"),
            Some(LoadoutHold {
                on: true,
                by: vec!["Healer".into()],
            })
        );
        // A group no active loadout lists stays off, and every loadout
        // that lists a group decides that. Quiet lists none.
        assert_eq!(
            hold(&both, "loot"),
            Some(LoadoutHold {
                on: false,
                by: vec!["Healer".into(), "Warrior".into()],
            })
        );
    }

    #[test]
    fn no_hold_while_the_loadouts_have_no_opinion() {
        assert_eq!(loadout_hold(&LoadoutSet::default(), "combat"), None);
        assert_eq!(loadout_hold(&set_of(&["Quiet"], false), "combat"), None);
        assert_eq!(loadout_hold(&set_of(&[], false), "combat"), None);
    }

    #[test]
    fn dormant_holds_every_group_off_with_no_loadout_to_name() {
        let off = Some(LoadoutHold {
            on: false,
            by: Vec::new(),
        });
        assert_eq!(loadout_hold(&set_of(&[], true), "combat"), off);
        assert_eq!(loadout_hold(&set_of(&["Healer"], true), "heals"), off);
    }

    #[test]
    fn a_hold_says_what_an_apply_lays_over_the_group() {
        // Every hold agrees with what apply_effective_state does to the
        // group, whatever the switch said before.
        let groups = ["combat", "heals", "loot"];
        for (active, dormant) in [
            (&["Healer"][..], false),
            (&["Warrior", "Quiet"][..], false),
            (&[][..], true),
        ] {
            let set = set_of(active, dormant);
            let mut profile = profile_with_items(
                groups
                    .iter()
                    .map(|g| alias_with_group(g, "x", Some(g)))
                    .collect(),
                Vec::new(),
                Vec::new(),
            );
            for g in groups {
                profile.aliases.set_group_enabled(g, g == "loot");
            }
            apply_effective_state(&set, &mut profile);
            for g in groups {
                let hold = loadout_hold(&set, g).expect("the loadouts decide");
                assert_eq!(
                    profile.aliases.is_group_enabled(g),
                    hold.on,
                    "{active:?} {g}"
                );
            }
        }
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
                enabled: true,
                preset: None,
            }],
        );

        let set = LoadoutSet::default();
        apply_loadout_state(&set, &mut profile);

        let leftover = &profile.aliases.disabled_groups();
        assert!(leftover.is_empty(), "{leftover:?}");
        let leftover = &profile.triggers.disabled_groups();
        assert!(leftover.is_empty(), "{leftover:?}");
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
                enabled: true,
                preset: None,
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
                enabled: true,
                preset: None,
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
            ..Default::default()
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
        let leftover = &profile.aliases.disabled_groups();
        assert!(leftover.is_empty(), "{leftover:?}");
    }
}

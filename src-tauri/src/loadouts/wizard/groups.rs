//! The catalog group each item of the wizard plan lands in, and what
//! each profile file keeps of the groups.
//!
//! ## Group names
//!
//! The catalog is shared, and each profile keeps its own group checkbox
//! lists, so every item sits in a group that is on for exactly the
//! profiles that had it on. What each profile has of an item is its
//! role: off (it lacks the item, or turned it off while another profile
//! had it on), on without a folder, or in one of its folders, whose
//! checkbox then decides. Items with the same role for every profile
//! share a group. Items every profile has on without a folder need none.
//!
//! A group whose items sit in folder `combat` takes that name, one group
//! per folder: the one the most profiles have in their `combat` folder,
//! on a tie the one of the first profile.
//! Another group of the folder adds the profiles that have its items,
//! such as `combat (Healer)`, and a group of items that were in no
//! folder is named for those profiles alone, such as `(Healer)`. A name
//! taken twice, or one that is also a folder name, gets a number, such as
//! `combat (Healer) 2`.
//!
//! One folder of one profile can so land in several catalog groups. Each
//! profile file keeps a folder map that names them (see
//! [`crate::profile::file::GroupFolders`]), so `#group combat off` still
//! turns off exactly what that profile had in its combat folder. Each
//! kind keeps its own groups, as it keeps its own checkbox list.
//!
//! The derived loadout for each profile names every group on for it, so
//! turning on loadout `default` turns on what the default profile had
//! on, the items it shared included, and nothing it did not have.
//!
//! A preset trigger follows the list of presets that are on, which every
//! profile shares in loadout mode, so it is on for every profile, the
//! ones whose file lacks it included, since a launch installs every
//! preset that is on. It stays in a folder only for a profile that had
//! it there. A preset the library no longer has installs for no one, so
//! its trigger stays off for a profile whose file lacks it.

use std::collections::{BTreeMap, BTreeSet};

use crate::profile::file::ProfileConfig;

use super::plan::{preset_on, CatalogItem, Entry};

/// Every folder any profile puts an item of any kind in. A group named
/// for the profiles that have it never takes one of these names, so
/// `#group` with the name of a folder never reaches it.
pub(super) fn every_folder(profiles: &[(String, ProfileConfig)]) -> BTreeSet<String> {
    let mut folders = BTreeSet::new();
    for (_, config) in profiles {
        let groups = config
            .aliases
            .iter()
            .map(|a| a.group.as_deref())
            .chain(config.triggers.iter().map(|t| t.group.as_deref()))
            .chain(config.macros.iter().map(|m| m.group.as_deref()));
        folders.extend(groups.flatten().filter(|g| !g.is_empty()).map(String::from));
    }
    folders
}

/// What one profile has of an item.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Role {
    /// It lacks the item, or turned the item off while another profile
    /// had it on, so the item's group stays off for it.
    Off,
    /// It has the item on without a folder, so the item's group stays on
    /// for it.
    On,
    /// It has the item in this folder, whose checkbox decides, and
    /// `#group` with the folder's name turns the item's group on or off.
    Folder(String),
}

/// The folder a copy sits in, None for none.
pub(super) fn folder_of<T: CatalogItem>(item: &T) -> Option<String> {
    item.group().filter(|g| !g.is_empty()).map(String::from)
}

/// What each profile has of `entry`, in profile order.
fn roles<T: CatalogItem>(profiles: &[(String, ProfileConfig)], entry: &Entry<T>) -> Vec<Role> {
    let any_on = entry.copies.iter().any(|(_, item)| item.enabled());
    (0..profiles.len())
        .map(|n| {
            let copy = entry
                .copies
                .iter()
                .find(|(holder, _)| *holder == n)
                .map(|(_, item)| item);
            if entry.preset {
                // A launch installs every preset that is on, whatever the
                // file held, and turns its triggers on. A launch also takes
                // out the triggers of a preset that is off, so only a
                // preset its holder had on can sit in its folder. A preset
                // the library no longer has installs for no one, so it
                // stays off for a profile whose file lacks it.
                let config = &profiles[n].1;
                return match copy {
                    Some(item)
                        if item
                            .preset()
                            .is_some_and(|id| preset_on(&config.ui.enabled_presets, id)) =>
                    {
                        folder_of(item).map_or(Role::On, Role::Folder)
                    }
                    None if !entry.in_library => Role::Off,
                    _ => Role::On,
                };
            }
            match copy {
                None => Role::Off,
                // It comes over turned on for another profile, so its
                // group keeps it off for this one.
                Some(item) if any_on && !item.enabled() => Role::Off,
                Some(item) => folder_of(item).map_or(Role::On, Role::Folder),
            }
        })
        .collect()
}

/// Where the items of one kind land, see [`plan_kind`].
pub(super) struct KindPlan {
    /// The catalog group of each entry, None for one every profile has
    /// on without a folder.
    pub(super) groups: Vec<Option<String>>,
    /// Whether each entry comes over turned on: when any profile had it
    /// on, since its group decides for whom.
    pub(super) enabled: Vec<bool>,
    /// The groups of this kind off for each profile, sorted.
    pub(super) off: Vec<Vec<String>>,
    /// The groups of this kind on for each profile, sorted.
    pub(super) on: Vec<Vec<String>>,
    /// The folder map of this kind for each profile.
    pub(super) folders: Vec<BTreeMap<String, Vec<String>>>,
}

/// Put each entry of one kind in its catalog group, see the module docs,
/// and work out what each profile file keeps of the groups.
pub(super) fn plan_kind<T: CatalogItem>(
    profiles: &[(String, ProfileConfig)],
    entries: &[Entry<T>],
    reserved: &BTreeSet<String>,
) -> KindPlan {
    let roles: Vec<Vec<Role>> = entries.iter().map(|e| roles(profiles, e)).collect();
    let names = name_groups(profiles, &roles, reserved);
    let folders: BTreeSet<&String> = names
        .keys()
        .flatten()
        .filter_map(|role| match role {
            Role::Folder(folder) => Some(folder),
            _ => None,
        })
        .collect();
    let mut plan = KindPlan {
        groups: roles.iter().map(|r| names.get(r).cloned()).collect(),
        enabled: entries
            .iter()
            .map(|e| e.preset || e.copies.iter().any(|(_, item)| item.enabled()))
            .collect(),
        off: Vec::new(),
        on: Vec::new(),
        folders: Vec::new(),
    };
    for (n, (_, config)) in profiles.iter().enumerate() {
        let (mut off, mut on) = (Vec::new(), Vec::new());
        for (roles, group) in &names {
            let is_off = match &roles[n] {
                Role::Off => true,
                Role::On => false,
                Role::Folder(folder) => T::groups_off(config).contains(folder),
            };
            if is_off {
                off.push(group.clone());
            } else {
                on.push(group.clone());
            }
        }
        off.sort();
        on.sort();
        let mut map = BTreeMap::new();
        for folder in &folders {
            let mut mine: Vec<String> = names
                .iter()
                .filter(|(roles, _)| roles[n] == Role::Folder((*folder).clone()))
                .map(|(_, group)| group.clone())
                .collect();
            mine.sort();
            // A folder that is its catalog group of the same name needs
            // no entry, and neither does one no group bears the name of.
            let same = mine.len() == 1 && mine[0] == **folder;
            let named = names.values().any(|group| group == *folder);
            if !same && (!mine.is_empty() || named) {
                map.insert((*folder).clone(), mine);
            }
        }
        plan.off.push(off);
        plan.on.push(on);
        plan.folders.push(map);
    }
    plan
}

/// How many profiles have a group in a folder, then how early the first
/// of them comes, for the group that takes the folder's own name.
type Rank = (usize, std::cmp::Reverse<usize>);

/// The catalog group name of each set of roles `roles` holds, but the
/// one every profile has on without a folder, which needs no group. See
/// the module docs.
fn name_groups(
    profiles: &[(String, ProfileConfig)],
    roles: &[Vec<Role>],
    reserved: &BTreeSet<String>,
) -> BTreeMap<Vec<Role>, String> {
    let mut distinct: Vec<&Vec<Role>> = Vec::new();
    for r in roles {
        if !r.iter().all(|role| *role == Role::On) && !distinct.contains(&r) {
            distinct.push(r);
        }
    }
    let base = |r: &[Role]| {
        r.iter().find_map(|role| match role {
            Role::Folder(folder) => Some(folder.clone()),
            _ => None,
        })
    };
    // Each folder's own name goes to the group of it the most profiles
    // have in that folder, on a tie the one whose first such profile
    // comes first.
    let mut main: BTreeMap<String, (Rank, &Vec<Role>)> = BTreeMap::new();
    for r in &distinct {
        let Some(folder) = base(r) else {
            continue;
        };
        let mine = |role: &Role| *role == Role::Folder(folder.clone());
        let count = r.iter().filter(|role| mine(role)).count();
        let first = r.iter().position(mine).unwrap_or(usize::MAX);
        let rank = (count, std::cmp::Reverse(first));
        if main.get(&folder).map_or(true, |(best, _)| rank > *best) {
            main.insert(folder, (rank, r));
        }
    }
    let mut names: BTreeMap<Vec<Role>, String> = BTreeMap::new();
    for (folder, (_, r)) in &main {
        names.insert((*r).clone(), folder.clone());
    }
    let mut taken: BTreeSet<String> = names.values().cloned().collect();
    for r in &distinct {
        if names.contains_key(*r) {
            continue;
        }
        let who: Vec<&str> = profiles
            .iter()
            .zip(r.iter())
            .filter(|(_, role)| **role != Role::Off)
            .map(|((name, _), _)| name.as_str())
            .collect();
        let stem = match base(r) {
            Some(folder) => format!("{folder} ({})", who.join(", ")),
            None => format!("({})", who.join(", ")),
        };
        let mut name = stem.clone();
        let mut n = 2;
        while taken.contains(&name) || reserved.contains(&name) {
            name = format!("{stem} {n}");
            n += 1;
        }
        taken.insert(name.clone());
        names.insert((*r).clone(), name);
    }
    names
}

#[cfg(test)]
mod tests {
    use vosh_automation::alias::Alias;
    use vosh_automation::trigger::Trigger;

    use super::super::plan::tests::{
        alias_group, analyze, file_after, grouped, profile_with, trigger, trigger_named,
    };
    use super::super::plan::ItemPayload;
    use crate::profile::file::ProfileConfig;
    use crate::profile::live::Macro;

    #[test]
    fn every_variant_of_a_conflict_lands_in_the_group_of_all_its_holders() {
        let plan = analyze(&[
            (
                "default".into(),
                profile_with(vec![Alias::new("kk", "kick %1")], vec![], vec![]),
            ),
            ("bard".into(), profile_with(vec![], vec![], vec![])),
            (
                "warrior".into(),
                profile_with(vec![grouped("kk", "kick 1.", "combat")], vec![], vec![]),
            ),
        ]);
        let ItemPayload::Alias { item } = &plan.conflicts[0].variants[1].item else {
            panic!("an alias conflict");
        };
        // Whichever version you keep, it stays on for both of them, in
        // the warrior's combat folder, and off for the bard.
        assert_eq!(item.group.as_deref(), Some("combat"));
        assert_eq!(plan.loadouts[0].enabled_groups, ["combat"]);
        let leftover = &plan.loadouts[1].enabled_groups;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(plan.loadouts[2].enabled_groups, ["combat"]);
        assert_eq!(file_after(&plan, "bard").disabled_alias_groups, ["combat"]);
        // Neither the default profile nor the bard had a combat folder, so
        // `#group combat` turns nothing on or off for them.
        for name in ["default", "bard"] {
            let folders = file_after(&plan, name).group_folders;
            assert_eq!(folders.aliases["combat"], Vec::<String>::new());
        }
        assert!(file_after(&plan, "warrior").group_folders.is_empty());
    }

    #[test]
    fn loadouts_carry_enabled_groups_per_source() {
        let plan = analyze(&[(
            "default".into(),
            profile_with(
                vec![
                    Alias::new("kk", "kick %1"),
                    grouped("punch", "punch %1", "combat"),
                ],
                vec![],
                vec![],
            ),
        )]);
        assert_eq!(plan.loadouts.len(), 1);
        let loadout = &plan.loadouts[0];
        assert_eq!(loadout.name, "default");
        // The folder keeps its name, and the ungrouped alias needs none.
        assert_eq!(loadout.enabled_groups, ["combat"]);
        assert_eq!(alias_group(&plan, "punch").as_deref(), Some("combat"));
        assert_eq!(alias_group(&plan, "kk"), None);
    }

    #[test]
    fn a_loadout_leaves_out_a_group_its_profile_had_off() {
        let mut flee = trigger("flee", "^You flee", "flee");
        flee.group = Some("combat".into());
        let mut cfg = profile_with(
            vec![
                grouped("punch", "punch %1", "combat"),
                grouped("sanc", "cast sanctuary", "buffs"),
            ],
            vec![flee],
            vec![],
        );
        cfg.disabled_alias_groups = vec!["combat".into(), "buffs".into()];
        let plan = analyze(&[("default".into(), cfg)]);
        // Buffs was off in the only list that has it. The combat triggers
        // were on, so combat stays in for them.
        assert_eq!(plan.loadouts[0].enabled_groups, ["combat"]);
        let file = file_after(&plan, "default");
        assert_eq!(file.disabled_alias_groups, ["buffs", "combat"]);
        let leftover = &file.disabled_trigger_groups;
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn each_kind_keeps_its_own_group_state_under_one_name() {
        // The Healer had its loot alias on and its auto loot trigger off,
        // both in a group named loot.
        let mut loot_trigger = trigger("autoloot", "^You killed", "get all corpse");
        loot_trigger.group = Some("loot".into());
        let mut cfg = profile_with(
            vec![grouped("loot", "get all corpse", "loot")],
            vec![loot_trigger],
            vec![],
        );
        cfg.disabled_trigger_groups = vec!["loot".into()];
        let plan = analyze(&[("Healer".into(), cfg)]);

        let file = file_after(&plan, "Healer");
        // The trigger group stays off. It used to come on with the alias
        // group of the same name, so the trigger looted every kill.
        let leftover = &file.disabled_alias_groups;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(file.disabled_trigger_groups, ["loot"]);
    }

    #[test]
    fn a_shared_item_one_holder_had_off_lands_apart_from_the_ones_on() {
        // Both characters have the same auto loot and flee triggers. The
        // Healer keeps its loot group off.
        let mut loot = trigger("autoloot", "^You killed", "get all corpse");
        loot.group = Some("loot".into());
        let flee = trigger("flee", "^You flee", "flee");
        let default = profile_with(vec![], vec![loot.clone(), flee.clone()], vec![]);
        let mut healer = profile_with(vec![], vec![loot, flee], vec![]);
        healer.disabled_trigger_groups = vec!["loot".into()];
        let plan = analyze(&[("default".into(), default), ("Healer".into(), healer)]);

        assert!(plan.conflicts.is_empty());
        let group = |name: &str| {
            let found = plan.auto_resolved.triggers.iter().find(|t| t.name == name);
            found.unwrap().group.clone()
        };
        // Both keep auto loot in their loot folder, which the Healer has
        // off, and flee is on for both without one.
        assert_eq!(group("autoloot").as_deref(), Some("loot"));
        assert_eq!(group("flee"), None);
        let leftover = &file_after(&plan, "default").disabled_trigger_groups;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(
            file_after(&plan, "Healer").disabled_trigger_groups,
            ["loot"]
        );
        let leftover = &plan.loadouts[1].enabled_groups;
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn an_item_one_holder_turned_off_stays_off_for_it() {
        let mut off = Alias::new("kk", "kick %1");
        off.enabled = false;
        let f1 = Macro {
            key: "f1".into(),
            command: "cast heal".into(),
            group: None,
            enabled: true,
            preset: None,
        };
        let f1_off = Macro {
            enabled: false,
            ..f1.clone()
        };
        let plan = analyze(&[
            (
                "default".into(),
                profile_with(vec![Alias::new("kk", "kick %1")], vec![], vec![f1]),
            ),
            (
                "Healer".into(),
                profile_with(vec![off], vec![], vec![f1_off]),
            ),
        ]);
        // Only whether it is on differs, so there is nothing to settle.
        assert!(plan.conflicts.is_empty());
        let kk = &plan.auto_resolved.aliases[0];
        assert!(kk.enabled);
        assert_eq!(kk.group.as_deref(), Some("(default)"));
        assert!(plan.auto_resolved.macros[0].enabled);
        let healer = file_after(&plan, "Healer");
        assert_eq!(healer.disabled_alias_groups, ["(default)"]);
        assert_eq!(healer.disabled_macro_groups, ["(default)"]);
    }

    #[test]
    fn an_item_no_holder_had_on_stays_as_each_left_it() {
        let mut off = Alias::new("kk", "kick %1");
        off.enabled = false;
        let mut cfg = profile_with(
            vec![off, grouped("punch", "punch %1", "combat")],
            vec![],
            vec![],
        );
        cfg.disabled_alias_groups = vec!["combat".into()];
        let plan = analyze(&[("Healer".into(), cfg)]);
        let alias = |name: &str| {
            let found = plan.auto_resolved.aliases.iter().find(|a| a.name == name);
            found.unwrap().clone()
        };
        // The one you turned off stays off, and the one in a folder you
        // had off stays on inside that folder, which stays off.
        assert!(!alias("kk").enabled);
        assert_eq!(alias("kk").group, None);
        assert!(alias("punch").enabled);
        assert_eq!(alias("punch").group.as_deref(), Some("combat"));
        assert_eq!(
            file_after(&plan, "Healer").disabled_alias_groups,
            ["combat"]
        );
        let leftover = &plan.loadouts[0].enabled_groups;
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn an_item_you_turned_off_stays_in_the_group_you_kept_on() {
        let mut dirt = grouped("dirt", "dirt %1", "combat");
        dirt.enabled = false;
        let plan = analyze(&[(
            "Healer".into(),
            profile_with(
                vec![grouped("bash", "bash %1", "combat"), dirt],
                vec![],
                vec![],
            ),
        )]);
        // Both stay in combat, which stays on, and dirt stays off by its
        // own switch, as you left them.
        for alias in &plan.auto_resolved.aliases {
            assert_eq!(alias.group.as_deref(), Some("combat"));
            assert_eq!(alias.enabled, alias.name == "bash");
        }
        let leftover = &file_after(&plan, "Healer").disabled_alias_groups;
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn a_folder_two_characters_filled_differently_keeps_its_name_for_both() {
        // Both have flee in combat, and only the default profile has bash
        // there too.
        let plan = analyze(&[
            (
                "default".into(),
                profile_with(
                    vec![
                        grouped("flee", "flee", "combat"),
                        grouped("bash", "bash %1", "combat"),
                    ],
                    vec![],
                    vec![],
                ),
            ),
            (
                "Healer".into(),
                profile_with(vec![grouped("flee", "flee", "combat")], vec![], vec![]),
            ),
        ]);
        // The group both have takes the folder's name, and the other is
        // named for the profile that has it.
        assert_eq!(alias_group(&plan, "flee").as_deref(), Some("combat"));
        assert_eq!(
            alias_group(&plan, "bash").as_deref(),
            Some("combat (default)")
        );
        // `#group combat` for the default profile turns both on and off.
        // For the Healer it is the combat group alone, which needs no
        // entry.
        let default = file_after(&plan, "default");
        assert_eq!(
            default.group_folders.aliases["combat"],
            ["combat", "combat (default)"]
        );
        assert!(file_after(&plan, "Healer").group_folders.is_empty());
        assert_eq!(
            file_after(&plan, "Healer").disabled_alias_groups,
            ["combat (default)"]
        );
    }

    #[test]
    fn a_name_two_groups_of_one_kind_would_share_gets_a_number() {
        // Each alias sits in the default combat folder. The warrior has
        // dig in combat too, kk without a folder, and xx in its loot
        // folder. kk and xx both land in a group named for both profiles.
        let plan = analyze(&[
            (
                "default".into(),
                profile_with(
                    vec![
                        grouped("dig", "dig", "combat"),
                        grouped("kk", "kick %1", "combat"),
                        grouped("xx", "xx", "combat"),
                    ],
                    vec![],
                    vec![],
                ),
            ),
            (
                "warrior".into(),
                profile_with(
                    vec![
                        grouped("dig", "dig", "combat"),
                        Alias::new("kk", "kick %1"),
                        grouped("xx", "xx", "loot"),
                    ],
                    vec![],
                    vec![],
                ),
            ),
        ]);
        assert_eq!(alias_group(&plan, "dig").as_deref(), Some("combat"));
        assert_eq!(
            alias_group(&plan, "kk").as_deref(),
            Some("combat (default, warrior)")
        );
        assert_eq!(
            alias_group(&plan, "xx").as_deref(),
            Some("combat (default, warrior) 2")
        );
        let warrior = file_after(&plan, "warrior").group_folders;
        assert_eq!(warrior.aliases["loot"], ["combat (default, warrior) 2"]);
        assert!(!warrior.aliases.contains_key("combat"));
        let default = file_after(&plan, "default").group_folders;
        assert_eq!(
            default.aliases["combat"],
            [
                "combat",
                "combat (default, warrior)",
                "combat (default, warrior) 2"
            ]
        );
    }

    #[test]
    fn a_group_named_for_profiles_never_takes_a_folder_name() {
        // The Healer keeps its own aliases in a folder named (default),
        // and kk, on for the default profile alone, would take that name.
        let plan = analyze(&[
            (
                "default".into(),
                profile_with(vec![Alias::new("kk", "kick %1")], vec![], vec![]),
            ),
            (
                "Healer".into(),
                profile_with(
                    vec![grouped("hl", "cast heal", "(default)")],
                    vec![],
                    vec![],
                ),
            ),
        ]);
        assert_eq!(alias_group(&plan, "hl").as_deref(), Some("(default)"));
        assert_eq!(alias_group(&plan, "kk").as_deref(), Some("(default) 2"));
    }

    /// A trigger of the healing basics preset, as a profile file holds it.
    fn heal_preset(pattern: &str) -> Trigger {
        Trigger {
            preset: Some("healing_basics".into()),
            ..trigger("heal 1", pattern, "HEAL")
        }
    }

    #[test]
    fn a_preset_trigger_stays_on_for_every_profile_that_lacked_it() {
        // Default has healing basics on and its trigger in its file. The
        // Healer saved its file before the preset came out, with an older
        // pattern, and the Bard never saved a file.
        let default = profile_with(vec![], vec![heal_preset("^You heal")], vec![]);
        let healer = profile_with(vec![], vec![heal_preset("^You are healed")], vec![]);
        let plan = analyze(&[
            ("default".into(), default),
            ("Healer".into(), healer),
            ("Bard".into(), ProfileConfig::default()),
        ]);
        // A launch installs the library version either way, so there is
        // nothing to ask you, and no group of any profile gates it.
        assert!(plan.conflicts.is_empty());
        assert_eq!(plan.auto_resolved.triggers[0].group, None);
        for name in ["default", "Healer", "Bard"] {
            let leftover = &file_after(&plan, name).disabled_trigger_groups;
            assert!(leftover.is_empty(), "{leftover:?}");
        }
    }

    #[test]
    fn a_preset_the_library_no_longer_has_stays_off_for_profiles_that_lack_it() {
        // An older build left the Healer the trigger of a preset this
        // build's library no longer has, so no launch installs it for a
        // profile whose file lacks it.
        let old = Trigger {
            preset: Some("old_labels".into()),
            ..trigger("old 1", "^old$", "OLD")
        };
        let plan = analyze(&[
            ("default".into(), ProfileConfig::default()),
            (
                "Healer".into(),
                profile_with(vec![], vec![old, heal_preset("^You heal")], vec![]),
            ),
            ("Bard".into(), ProfileConfig::default()),
        ]);
        assert!(plan.conflicts.is_empty());
        // It used to sit in no group, on for every character.
        assert_eq!(
            trigger_named(&plan, "old 1").group.as_deref(),
            Some("(Healer)")
        );
        for name in ["default", "Bard"] {
            let off = file_after(&plan, name).disabled_trigger_groups;
            assert_eq!(off, ["(Healer)"], "{name}");
        }
        let leftover = &file_after(&plan, "Healer").disabled_trigger_groups;
        assert!(leftover.is_empty(), "{leftover:?}");
        // A preset the library has still comes on for every profile.
        assert_eq!(trigger_named(&plan, "heal 1").group, None);
    }

    #[test]
    fn a_preset_trigger_a_profile_kept_off_by_its_group_stays_off_for_it() {
        let mut labelled = heal_preset("^You heal");
        labelled.group = Some("labels".into());
        let mut healer = profile_with(vec![], vec![labelled], vec![]);
        healer.disabled_trigger_groups = vec!["labels".into()];
        let plan = analyze(&[
            (
                "default".into(),
                profile_with(vec![], vec![heal_preset("^You heal")], vec![]),
            ),
            ("Healer".into(), healer),
            ("Bard".into(), ProfileConfig::default()),
        ]);
        // It keeps the Healer's labels folder, which the Healer has off,
        // and every other profile has on.
        let group = plan.auto_resolved.triggers[0].group.clone();
        assert_eq!(group.as_deref(), Some("labels"));
        assert_eq!(
            file_after(&plan, "Healer").disabled_trigger_groups,
            ["labels"]
        );
        for name in ["default", "Bard"] {
            let file = file_after(&plan, name);
            let leftover = &file.disabled_trigger_groups;
            assert!(leftover.is_empty(), "{leftover:?}");
            // Neither had a labels folder, so `#group labels` finds none.
            assert_eq!(file.group_folders.triggers["labels"], Vec::<String>::new());
        }
    }

    #[test]
    fn a_preset_trigger_off_in_its_own_list_gates_nothing() {
        // The Healer turned healing basics off, and its file still holds
        // the trigger in a group it keeps off. A launch as the Healer
        // takes the trigger out, so that group never kept it off.
        let mut labelled = heal_preset("^You heal");
        labelled.group = Some("labels".into());
        let mut healer = profile_with(vec![], vec![labelled], vec![]);
        healer.disabled_trigger_groups = vec!["labels".into()];
        healer.ui.enabled_presets = vec!["potion_labels".into()];
        let plan = analyze(&[
            (
                "default".into(),
                profile_with(vec![], vec![heal_preset("^You heal")], vec![]),
            ),
            ("Healer".into(), healer),
        ]);
        assert_eq!(plan.auto_resolved.triggers[0].group, None);
        let leftover = &file_after(&plan, "Healer").disabled_trigger_groups;
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn a_preset_trigger_in_a_group_every_profile_kept_on_keeps_that_group() {
        let mut labelled = heal_preset("^You heal");
        labelled.group = Some("labels".into());
        let plan = analyze(&[
            (
                "default".into(),
                profile_with(vec![], vec![labelled], vec![]),
            ),
            ("Healer".into(), ProfileConfig::default()),
        ]);
        assert_eq!(
            plan.auto_resolved.triggers[0].group.as_deref(),
            Some("labels")
        );
        // Each loadout turns it on, so an active loadout keeps it on.
        for loadout in &plan.loadouts {
            assert_eq!(loadout.enabled_groups, ["labels"]);
        }
        let leftover = &file_after(&plan, "Healer").disabled_trigger_groups;
        assert!(leftover.is_empty(), "{leftover:?}");
    }
}

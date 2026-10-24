//! Renaming a group, from the menu on a group heading in Settings. A
//! group is one name across triggers, aliases, macros and timers, as
//! `#group` and the loadouts treat it, so a rename moves the items of
//! the group in every list. Each list carries the group's state to the
//! new name, unless the new name is a group that list already holds,
//! which keeps its own state. The preset edit rows and the folder map
//! that name the group follow it.
//!
//! In loadout mode the shared catalog holds the triggers, aliases and
//! macros, while each profile file keeps the groups it turned off and its
//! folder map. So every other profile follows the rename in those, the
//! open ones live and the rest in their files, and the loadouts follow
//! it too. Timers stay in the profile file, so they rename there alone.

use std::collections::BTreeSet;

use crate::loadouts::preset_edits::PresetEdits;
use crate::loadouts::set::LoadoutSet;
use crate::profile::file::{GroupFolders, ProfileConfig};
use crate::profile::live::Profile;

/// For each list, triggers, aliases, macros then timers, whether it
/// holds the group.
pub(crate) type InLists = [bool; 4];

/// Which lists hold `group`.
pub(crate) fn lists_holding(p: &Profile, group: &str) -> InLists {
    [
        p.triggers.holds_group(group),
        p.aliases.holds_group(group),
        p.macros.iter().any(|m| m.group.as_deref() == Some(group)),
        p.timers.iter().any(|t| t.group.as_deref() == Some(group)),
    ]
}

/// The names both groups hold in a list that knows an item by its group
/// and its name, which a rename would leave twice in one group. Macros
/// are known by their key and timers by their id, so they never clash.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Clashes {
    pub(crate) triggers: Vec<String>,
    pub(crate) aliases: Vec<String>,
}

pub(crate) fn clashes(p: &Profile, from: &str, to: &str) -> Clashes {
    Clashes {
        triggers: p.triggers.group_clashes(from, to),
        aliases: p.aliases.group_clashes(from, to),
    }
}

/// `names` as one list in prose: `ds`, `ds and res`, or `ds, res and gt`.
fn listed(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// What Settings says when it refuses a rename for `clashes`, or None
/// when there are none.
pub(crate) fn clash_message(clashes: &Clashes, from: &str, to: &str) -> Option<String> {
    let part = |names: &[String], one: &str, many: &str| match names.len() {
        0 => None,
        1 => Some(format!("{one} named {}", listed(names))),
        _ => Some(format!("{many} named {}", listed(names))),
    };
    let parts: Vec<String> = [
        part(&clashes.aliases, "an alias", "aliases"),
        part(&clashes.triggers, "a trigger", "triggers"),
    ]
    .into_iter()
    .flatten()
    .collect();
    if parts.is_empty() {
        return None;
    }
    let count = clashes.aliases.len() + clashes.triggers.len();
    let fix = if count == 1 {
        "Rename one of them first."
    } else {
        "Rename one of each first."
    };
    Some(format!(
        "{from} and {to} both have {}. {fix}",
        parts.join(", and ")
    ))
}

/// `off` with the state of `from` carried to `to`, as
/// [`vosh_automation::alias::AliasStore::rename_group_state`] does for
/// the stores. `kept` says the list held `to` already.
fn rename_off(off: &mut BTreeSet<String>, from: &str, to: &str, kept: bool) {
    let was_off = off.remove(from);
    if kept {
        return;
    }
    if was_off {
        off.insert(to.to_string());
    } else {
        off.remove(to);
    }
}

/// [`rename_off`] over a list a profile file keeps, sorted once each.
fn rename_off_list(off: &mut Vec<String>, from: &str, to: &str, kept: bool) -> bool {
    let mut set: BTreeSet<String> = off.iter().cloned().collect();
    rename_off(&mut set, from, to, kept);
    let next: Vec<String> = set.into_iter().collect();
    let changed = next != *off;
    *off = next;
    changed
}

/// The folder map with `from` renamed to `to` wherever a folder stands
/// for it, once each. Returns whether it changed.
fn rename_in_folders(folders: &mut GroupFolders, from: &str, to: &str) -> bool {
    let mut changed = false;
    for map in [
        &mut folders.aliases,
        &mut folders.triggers,
        &mut folders.macros,
    ] {
        for groups in map.values_mut() {
            let Some(at) = groups.iter().position(|g| g == from) else {
                continue;
            };
            if groups.iter().any(|g| g == to) {
                groups.remove(at);
            } else {
                groups[at] = to.to_string();
            }
            changed = true;
        }
    }
    changed
}

/// Your edits to the preset triggers that put one in `from` put it in
/// `to` instead. Returns whether any changed.
fn rename_in_preset_edits(edits: &mut PresetEdits, from: &str, to: &str) -> bool {
    let from_value = toml::Value::String(from.to_string());
    let mut changed = false;
    for edit in edits.values_mut() {
        for rows in edit.triggers.values_mut() {
            if let Some(row) = rows.get_mut("group") {
                if row.value == from_value {
                    row.value = toml::Value::String(to.to_string());
                    changed = true;
                }
            }
        }
    }
    changed
}

/// Rename `from` to `to` in every list of `p`, after [`clashes`] found
/// none. Returns how many items moved and which lists moved any.
pub(crate) fn rename_in_profile(p: &mut Profile, from: &str, to: &str) -> (usize, InLists) {
    let kept = lists_holding(p, to);
    let triggers = p.triggers.rename_group(from, to).unwrap_or(0);
    let aliases = p.aliases.rename_group(from, to).unwrap_or(0);
    let mut macros = 0;
    for m in &mut p.macros {
        if m.group.as_deref() == Some(from) {
            m.group = Some(to.to_string());
            macros += 1;
        }
    }
    if macros > 0 {
        rename_off(&mut p.disabled_macro_groups, from, to, kept[2]);
        p.macro_group_toggles = p.macro_group_toggles.wrapping_add(1);
        crate::loadouts::presets::hold_profile_keys(p);
    }
    let mut timers = 0;
    for t in &mut p.timers {
        if t.group.as_deref() == Some(from) {
            t.group = Some(to.to_string());
            timers += 1;
        }
    }
    if timers > 0 {
        rename_off(&mut p.disabled_timer_groups, from, to, kept[3]);
    }
    rename_in_preset_edits(&mut p.preset_edits, from, to);
    rename_in_folders(&mut p.group_folders, from, to);
    let moved = triggers + aliases + macros + timers;
    if moved > 0 {
        p.group_toggles = p.group_toggles.wrapping_add(1);
    }
    (moved, [triggers > 0, aliases > 0, macros > 0, timers > 0])
}

/// Follow a rename the shared catalog took in another open profile `p`:
/// the groups it keeps off in each catalog list in `moved`, where `kept`
/// says the new name was a group already, and its folder map.
pub(crate) fn follow_in_profile(
    p: &mut Profile,
    from: &str,
    to: &str,
    moved: InLists,
    kept: InLists,
) {
    if moved[0] {
        p.triggers.rename_group_state(from, to, kept[0]);
    }
    if moved[1] {
        p.aliases.rename_group_state(from, to, kept[1]);
    }
    if moved[2] {
        rename_off(&mut p.disabled_macro_groups, from, to, kept[2]);
        p.macro_group_toggles = p.macro_group_toggles.wrapping_add(1);
        crate::loadouts::presets::hold_profile_keys(p);
    }
    rename_in_folders(&mut p.group_folders, from, to);
    p.group_toggles = p.group_toggles.wrapping_add(1);
}

/// [`follow_in_profile`] for the file of a profile that is not open.
/// Returns whether the file changed and needs a save.
pub(crate) fn follow_in_file(
    config: &mut ProfileConfig,
    from: &str,
    to: &str,
    moved: InLists,
    kept: InLists,
) -> bool {
    let mut changed = false;
    if moved[0] {
        changed |= rename_off_list(&mut config.disabled_trigger_groups, from, to, kept[0]);
    }
    if moved[1] {
        changed |= rename_off_list(&mut config.disabled_alias_groups, from, to, kept[1]);
    }
    if moved[2] {
        changed |= rename_off_list(&mut config.disabled_macro_groups, from, to, kept[2]);
    }
    changed |= rename_in_folders(&mut config.group_folders, from, to);
    changed
}

/// The loadouts that list `from` list `to` in its place. A merge into a
/// group that was already one leaves out `from` alone, so `to` keeps the
/// loadouts it had. Returns whether any loadout changed.
pub(crate) fn follow_in_loadouts(set: &mut LoadoutSet, from: &str, to: &str, merge: bool) -> bool {
    let mut changed = false;
    for loadout in &mut set.loadouts {
        let groups = &mut loadout.enabled_groups;
        let Some(at) = groups.iter().position(|g| g == from) else {
            continue;
        };
        if merge || groups.iter().any(|g| g == to) {
            groups.remove(at);
        } else {
            groups[at] = to.to_string();
        }
        changed = true;
    }
    changed
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use vosh_automation::alias::Alias;
    use vosh_automation::trigger::{Trigger, TriggerAction};

    use super::{
        clash_message, clashes, follow_in_file, follow_in_loadouts, rename_in_profile, Clashes,
    };
    use crate::loadouts::preset_edits::{EditRow, PresetEdit};
    use crate::loadouts::set::{Loadout, LoadoutSet};
    use crate::profile::file::ProfileConfig;
    use crate::profile::live::{Macro, Profile, Timer};
    use crate::script::{set_list_group, GroupList};

    fn trigger(group: &str, name: &str) -> Trigger {
        Trigger {
            group: Some(group.into()),
            ..Trigger::new(name, "^x$", TriggerAction::Gag)
        }
    }

    /// A combat group in every list and a loot group of triggers.
    fn grouped() -> Profile {
        let mut p = Profile::default();
        p.triggers.set(trigger("combat", "flee")).unwrap();
        p.triggers.set(trigger("loot", "loot")).unwrap();
        p.aliases.set(Alias::new("kk", "kick").with_group("combat"));
        p.macros.push(Macro {
            key: "F1".into(),
            command: "bash".into(),
            group: Some("combat".into()),
            enabled: true,
            preset: None,
        });
        p.timers.push(Timer {
            id: 1,
            name: String::new(),
            interval_secs: 30,
            command: "rescue".into(),
            enabled: true,
            group: Some("combat".into()),
        });
        p
    }

    #[test]
    fn a_rename_moves_the_group_in_every_list_and_keeps_each_state() {
        let mut p = grouped();
        set_list_group(&mut p, GroupList::Triggers, "combat", false);
        set_list_group(&mut p, GroupList::Macros, "combat", false);
        let (items, moved) = rename_in_profile(&mut p, "combat", "fighting");
        assert_eq!(items, 4);
        assert_eq!(moved, [true; 4]);
        assert_eq!(
            p.triggers.groups(),
            [("fighting".to_string(), false), ("loot".to_string(), true)]
        );
        assert_eq!(p.aliases.groups(), [("fighting".to_string(), true)]);
        assert_eq!(
            p.disabled_macro_groups.iter().collect::<Vec<_>>(),
            ["fighting"]
        );
        assert_eq!(p.timers[0].group.as_deref(), Some("fighting"));
    }

    #[test]
    fn a_merge_keeps_the_state_of_the_group_it_joins_in_each_list() {
        let mut p = grouped();
        set_list_group(&mut p, GroupList::Triggers, "combat", false);
        set_list_group(&mut p, GroupList::Aliases, "combat", false);
        rename_in_profile(&mut p, "combat", "loot");
        // Triggers held loot, which stays on. Aliases did not, so theirs
        // takes the off state of combat.
        assert_eq!(p.triggers.groups(), [("loot".to_string(), true)]);
        assert_eq!(p.aliases.groups(), [("loot".to_string(), false)]);
    }

    #[test]
    fn the_preset_edits_and_the_folders_follow_the_group() {
        let mut p = grouped();
        let row = |value: &str| EditRow {
            value: toml::Value::String(value.into()),
            was: toml::Value::String("combat_base".into()),
            seen: None,
        };
        p.preset_edits.insert(
            "potion_labels".into(),
            PresetEdit {
                colors: BTreeMap::new(),
                triggers: BTreeMap::from([(
                    "potion.quaff".into(),
                    BTreeMap::from([("group".into(), row("combat"))]),
                )]),
            },
        );
        p.group_folders.triggers = BTreeMap::from([
            ("fight".into(), vec!["combat".into(), "loot".into()]),
            ("old".into(), vec!["combat".into()]),
        ]);
        rename_in_profile(&mut p, "combat", "loot");
        let edited = &p.preset_edits["potion_labels"].triggers["potion.quaff"]["group"];
        assert_eq!(edited.value, toml::Value::String("loot".into()));
        assert_eq!(p.group_folders.triggers["fight"], ["loot"]);
        assert_eq!(p.group_folders.triggers["old"], ["loot"]);
    }

    #[test]
    fn a_refusal_names_what_both_groups_hold() {
        let mut p = grouped();
        p.triggers.set(trigger("loot", "flee")).unwrap();
        p.aliases.set(Alias::new("kk", "kick").with_group("loot"));
        p.aliases.set(Alias::new("bs", "bash").with_group("loot"));
        p.aliases.set(Alias::new("bs", "bash").with_group("combat"));
        let found = clashes(&p, "combat", "loot");
        assert_eq!(
            found,
            Clashes {
                triggers: vec!["flee".into()],
                aliases: vec!["bs".into(), "kk".into()],
            }
        );
        assert_eq!(
            clash_message(&found, "combat", "loot").as_deref(),
            Some("combat and loot both have aliases named bs and kk, and a trigger named flee. Rename one of each first.")
        );
        let one = Clashes {
            triggers: Vec::new(),
            aliases: vec!["kk".into()],
        };
        assert_eq!(
            clash_message(&one, "combat", "loot").as_deref(),
            Some("combat and loot both have an alias named kk. Rename one of them first.")
        );
        assert_eq!(clash_message(&Clashes::default(), "combat", "loot"), None);
    }

    #[test]
    fn the_loadouts_follow_a_rename_and_a_merge_keeps_their_own() {
        let mut set = LoadoutSet::default();
        set.loadouts.push(Loadout {
            enabled_groups: vec!["combat".into(), "travel".into()],
            ..Loadout::empty("Tolliver")
        });
        set.loadouts.push(Loadout {
            enabled_groups: vec!["combat".into(), "loot".into()],
            ..Loadout::empty("Maren")
        });
        assert!(follow_in_loadouts(&mut set, "combat", "fighting", false));
        assert_eq!(set.loadouts[0].enabled_groups, ["fighting", "travel"]);
        assert_eq!(set.loadouts[1].enabled_groups, ["fighting", "loot"]);
        // A merge into travel leaves the loadout that lacked travel as it
        // stands, less the old name.
        assert!(follow_in_loadouts(&mut set, "fighting", "travel", true));
        assert_eq!(set.loadouts[0].enabled_groups, ["travel"]);
        assert_eq!(set.loadouts[1].enabled_groups, ["loot"]);
        assert!(!follow_in_loadouts(&mut set, "combat", "travel", true));
    }

    #[test]
    fn a_closed_profile_file_follows_in_the_catalog_lists_alone() {
        let mut config = ProfileConfig {
            disabled_trigger_groups: vec!["combat".into(), "loot".into()],
            disabled_alias_groups: vec!["combat".into()],
            disabled_timer_groups: vec!["combat".into()],
            ..ProfileConfig::default()
        };
        config.group_folders.aliases = BTreeMap::from([("fight".into(), vec!["combat".into()])]);
        let moved = [true, true, false, false];
        assert!(follow_in_file(
            &mut config,
            "combat",
            "fighting",
            moved,
            [false; 4]
        ));
        assert_eq!(config.disabled_trigger_groups, ["fighting", "loot"]);
        assert_eq!(config.disabled_alias_groups, ["fighting"]);
        // Timers stay in each profile's own file, so its timers keep theirs.
        assert_eq!(config.disabled_timer_groups, ["combat"]);
        assert_eq!(config.group_folders.aliases["fight"], ["fighting"]);
        // A file that names neither group stays as it is, with no save.
        let mut untouched = ProfileConfig::default();
        assert!(!follow_in_file(
            &mut untouched,
            "combat",
            "fighting",
            moved,
            [false; 4]
        ));
    }
}

//! Migration analyzer for loadout mode, the "global catalog +
//! loadouts" model. Reads every existing per-profile [`ProfileConfig`] and
//! produces a [`MigrationPlan`] without touching disk.
//!
//! ## What it does
//!
//! Each alias and macro name that one or more profiles hold becomes one
//! catalog item. When the copies differ only in their folder and in
//! whether they are on, which the catalog groups carry (see groups.rs), the
//! item is **auto-resolved** with no question. When the copies differ in
//! anything else, the item is a **conflict**, and the wizard shows each
//! version with its profile and asks which to keep, since the name is
//! what you type or press and the catalog holds one of each. Each version
//! says whether its profile had it on, and when exactly one version is on
//! anywhere, the wizard keeps that one unless you pick another.
//!
//! A trigger's name is only a label, so each version of a trigger stays,
//! and the profiles that share one version share its copy. The version
//! of the first profile keeps the name, and each other one adds the
//! profiles that have it, such as `greet (Healer)`. The copies of a
//! preset trigger always fold into one, since a launch installs the
//! library version.
//!
//! ## Scope
//!
//! This module produces the plan only. The `migration_apply` command
//! applies it once you confirm the conflict resolutions in the wizard.
//! It copies each profile file into `profiles/legacy/`, writes
//! `catalog.toml` and `loadouts.toml`, and takes the aliases, triggers,
//! and macros out of each profile file, which keeps every other setting
//! of its profile. The `migration_analyze` Tauri command consumes
//! [`analyze_profiles`] for the preview.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use vosh_automation::alias::Alias;
use vosh_automation::trigger::Trigger;

use crate::loadouts::catalog::GlobalCatalog;
use crate::loadouts::set::Loadout;
use crate::profile::file::{GroupFolders, ProfileConfig};
use crate::profile::live::Macro;

use super::groups::{every_folder, folder_of, plan_kind, KindPlan};

/// Which kind of item a conflict or auto-resolved entry refers to.
/// The frontend wizard renders different summaries per kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ItemKind {
    Alias,
    Trigger,
    Macro,
}

/// One variant of a (possibly conflicted) item. Carries the source
/// profile so the user knows where each variant came from.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Variant {
    pub source_profile: String,
    /// The version with its catalog group and the on state the catalog
    /// gives the version you keep.
    pub item: ItemPayload,
    /// Whether its profile had this copy switched on, so the preview can
    /// show which version was on.
    pub switched_on: bool,
}

/// Serializable union of the three item types. The wizard renders
/// each payload differently (an alias shows its expansion, a trigger
/// shows its first pattern + action summary, a macro shows its key +
/// command), so we keep the full body around rather than a synopsis.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ItemPayload {
    Alias { item: Alias },
    Trigger { item: Trigger },
    Macro { item: Macro },
}

/// One name with two or more non-equivalent variants from different
/// source profiles. Surfaces to the wizard for explicit resolution.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Conflict {
    pub kind: ItemKind,
    pub name: String,
    pub variants: Vec<Variant>,
    /// The profile whose version the wizard keeps unless you pick another.
    /// When exactly one version is switched on anywhere, it is the first
    /// profile that has that version on, since the others fire nothing.
    /// Otherwise it is the first profile.
    pub default_source: String,
}

/// Result of analyzing the existing profiles.
/// [`super::apply::apply_migration`] writes it to disk once you resolve
/// the conflicts.
#[derive(Debug, Clone, Serialize, Default)]
pub(crate) struct MigrationPlan {
    /// Items that need no user decision — single source, or every
    /// source agreed on the content. Already carry their catalog group
    /// (see groups.rs).
    pub auto_resolved: GlobalCatalog,
    /// Aliases and macros with diverging variants. The wizard asks the
    /// user to pick the one to keep. A trigger keeps every version.
    pub conflicts: Vec<Conflict>,
    /// One loadout per source profile, with `enabled_groups` naming
    /// every catalog group on for that profile. Connection defaults,
    /// tick config, and `profile_vars` stay in the profile file.
    pub loadouts: Vec<Loadout>,
    /// Names of source profiles the plan covered. Useful for the
    /// wizard summary header.
    pub source_profiles: Vec<String>,
    /// The enabled preset list the catalog takes, in the
    /// `enabled_presets` shape. Every character shares it in loadout
    /// mode. The caller fills it, see
    /// [`crate::loadouts::presets::first_catalog_presets`].
    pub shared_presets: Vec<String>,
    /// Each source profile's own enabled preset list, in the order of
    /// `source_profiles`, so the preview can say which characters gain or
    /// lose a preset. A profile that never saved a file holds the
    /// defaults, the empty list, which turns every preset on.
    pub profile_presets: Vec<Vec<String>>,
    /// What each profile file keeps of the catalog groups, by profile.
    /// See [`profile_file_for_catalog`].
    #[serde(skip)]
    pub files: BTreeMap<String, FileGroups>,
}

/// The group checkbox lists and the folder map one profile file keeps in
/// loadout mode.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FileGroups {
    /// The catalog alias groups off for the profile, sorted.
    pub(crate) disabled_alias_groups: Vec<String>,
    /// The catalog trigger groups off for the profile, sorted.
    pub(crate) disabled_trigger_groups: Vec<String>,
    /// The catalog macro groups off for the profile, sorted.
    pub(crate) disabled_macro_groups: Vec<String>,
    /// The catalog groups each folder of the profile became.
    pub(crate) folders: GroupFolders,
}

/// Walk every source profile, gather each alias, trigger, and macro into
/// one catalog item, put each in its catalog group, classify each as
/// auto-resolved or conflicted, and emit the plan.
///
/// `profiles` is an ordered list so determinism is preserved: group
/// names list profiles in this order, and when every variant agrees,
/// the copy of the FIRST profile in it becomes the catalog entry.
/// `library` holds the id of every preset in the library this build
/// installs from, see src/lib/presets.ts.
pub(crate) fn analyze_profiles(
    profiles: &[(String, ProfileConfig)],
    library: &[&str],
) -> MigrationPlan {
    let mut plan = MigrationPlan {
        source_profiles: profiles.iter().map(|(name, _)| name.clone()).collect(),
        ..MigrationPlan::default()
    };
    let reserved = every_folder(profiles);

    let aliases = keyed_entries(profiles, |c| &c.aliases);
    let triggers = trigger_entries(profiles, library);
    let macros = keyed_entries(profiles, |c| &c.macros);
    let alias_plan = plan_kind(profiles, &aliases, &reserved);
    let trigger_plan = plan_kind(profiles, &triggers, &reserved);
    let macro_plan = plan_kind(profiles, &macros, &reserved);

    for (n, (name, _)) in profiles.iter().enumerate() {
        plan.files.insert(
            name.clone(),
            FileGroups {
                disabled_alias_groups: alias_plan.off[n].clone(),
                disabled_trigger_groups: trigger_plan.off[n].clone(),
                disabled_macro_groups: macro_plan.off[n].clone(),
                folders: GroupFolders {
                    aliases: alias_plan.folders[n].clone(),
                    triggers: trigger_plan.folders[n].clone(),
                    macros: macro_plan.folders[n].clone(),
                },
            },
        );
        let mut loadout = Loadout::empty(name);
        for group in [&alias_plan, &trigger_plan, &macro_plan]
            .iter()
            .flat_map(|kind| &kind.on[n])
        {
            if !loadout.enabled_groups.contains(group) {
                loadout.enabled_groups.push(group.clone());
            }
        }
        plan.loadouts.push(loadout);
    }

    resolve(
        profiles,
        aliases,
        &alias_plan,
        ItemKind::Alias,
        &mut plan.auto_resolved.aliases,
        &mut plan.conflicts,
    );
    resolve(
        profiles,
        triggers,
        &trigger_plan,
        ItemKind::Trigger,
        &mut plan.auto_resolved.triggers,
        &mut plan.conflicts,
    );
    resolve(
        profiles,
        macros,
        &macro_plan,
        ItemKind::Macro,
        &mut plan.auto_resolved.macros,
        &mut plan.conflicts,
    );
    plan
}

/// What the wizard reads and rewrites on an alias, a trigger, or a
/// macro.
pub(super) trait CatalogItem: Clone + Serialize {
    /// The name that identifies it in its store: the alias name, the
    /// trigger name, or the macro key.
    fn key(&self) -> &str;
    fn group(&self) -> Option<&str>;
    fn set_group(&mut self, group: Option<String>);
    fn enabled(&self) -> bool;
    fn set_enabled(&mut self, enabled: bool);
    /// The group checkbox list of this kind in `config`, the groups it
    /// has off.
    fn groups_off(config: &ProfileConfig) -> &[String];
    /// The preset a preset trigger belongs to.
    fn preset(&self) -> Option<&str> {
        None
    }
    /// How this copy ranks among the copies of its key in `config`. The
    /// wizard keeps the last copy of the highest rank. An alias or a
    /// trigger store keeps the last copy of a name, so every copy ranks
    /// the same.
    fn rank_in(&self, _config: &ProfileConfig) -> u8 {
        0
    }
    fn payload(self) -> ItemPayload;
}

impl CatalogItem for Alias {
    fn key(&self) -> &str {
        &self.name
    }
    fn group(&self) -> Option<&str> {
        self.group.as_deref()
    }
    fn set_group(&mut self, group: Option<String>) {
        self.group = group;
    }
    fn enabled(&self) -> bool {
        self.enabled
    }
    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
    fn groups_off(config: &ProfileConfig) -> &[String] {
        &config.disabled_alias_groups
    }
    fn payload(self) -> ItemPayload {
        ItemPayload::Alias { item: self }
    }
}

impl CatalogItem for Trigger {
    fn key(&self) -> &str {
        &self.name
    }
    fn group(&self) -> Option<&str> {
        self.group.as_deref()
    }
    fn set_group(&mut self, group: Option<String>) {
        self.group = group;
    }
    fn enabled(&self) -> bool {
        self.enabled
    }
    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
    fn groups_off(config: &ProfileConfig) -> &[String] {
        &config.disabled_trigger_groups
    }
    fn preset(&self) -> Option<&str> {
        self.preset.as_deref()
    }
    fn payload(self) -> ItemPayload {
        ItemPayload::Trigger { item: self }
    }
}

impl CatalogItem for Macro {
    fn key(&self) -> &str {
        &self.key
    }
    fn group(&self) -> Option<&str> {
        self.group.as_deref()
    }
    fn set_group(&mut self, group: Option<String>) {
        self.group = group;
    }
    fn enabled(&self) -> bool {
        self.enabled
    }
    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
    fn groups_off(config: &ProfileConfig) -> &[String] {
        &config.disabled_macro_groups
    }
    /// A profile keeps every copy of a key, and the input bar fires the
    /// last one that is on and whose group is on. Next comes the last one
    /// that is on, which fires once you turn its group on, then the last.
    fn rank_in(&self, config: &ProfileConfig) -> u8 {
        let group_on = folder_of(self).map_or(true, |g| !Self::groups_off(config).contains(&g));
        match (self.enabled, group_on) {
            (true, true) => 2,
            (true, false) => 1,
            (false, _) => 0,
        }
    }
    fn payload(self) -> ItemPayload {
        ItemPayload::Macro { item: self }
    }
}

/// One catalog item: the copy of it each profile that holds it has, in
/// profile order, by profile index.
pub(super) struct Entry<T> {
    pub(super) copies: Vec<(usize, T)>,
    /// A preset trigger, which the shared preset list turns on.
    pub(super) preset: bool,
    /// A preset trigger whose preset the library still has, so a launch
    /// installs it for every profile, the ones whose file lacks it too.
    pub(super) in_library: bool,
}

/// The items of one kind in `configs`, one entry per name. Of the copies
/// of a name in one file, the last one of the highest rank wins, see
/// [`CatalogItem::rank_in`]. A store keeps the last copy of a name, and
/// the input bar fires the last macro of a key that is on.
fn keyed_entries<T: CatalogItem>(
    profiles: &[(String, ProfileConfig)],
    items: impl Fn(&ProfileConfig) -> &[T],
) -> Vec<Entry<T>> {
    let mut by_key: BTreeMap<String, Entry<T>> = BTreeMap::new();
    for (n, (_, config)) in profiles.iter().enumerate() {
        let mut last: BTreeMap<&str, &T> = BTreeMap::new();
        for item in items(config) {
            let outranked = last
                .get(item.key())
                .is_some_and(|kept| kept.rank_in(config) > item.rank_in(config));
            if !outranked {
                last.insert(item.key(), item);
            }
        }
        for (key, item) in last {
            by_key
                .entry(key.to_string())
                .or_insert_with(|| Entry {
                    copies: Vec::new(),
                    preset: false,
                    in_library: false,
                })
                .copies
                .push((n, item.clone()));
        }
    }
    by_key.into_values().collect()
}

/// The triggers in `configs`, in the order the catalog keeps them. A
/// name whose every copy is a preset trigger is one preset entry, since
/// a launch installs the library version whichever copy the catalog
/// keeps, and puts it last among the triggers of its priority. The
/// others come in an order that keeps the order each profile's store
/// runs them in, see [`merge_run`], since every trigger that matches a
/// line fires in that order. `library` names the presets the library
/// still has.
///
/// The copies of a trigger that are the same apart from their folder and
/// whether they are on share an entry. A trigger's name is only a label,
/// so each other version stays as an entry of its own, where an alias or
/// a macro, whose name is what you type or press, can keep only one. So
/// does a copy whose place in its profile's order no shared copy can
/// take. The first entry of a name keeps it, and each other one adds the
/// profiles that have it, such as `greet (Healer)`, with a number when
/// that name is taken too.
fn trigger_entries(profiles: &[(String, ProfileConfig)], library: &[&str]) -> Vec<Entry<Trigger>> {
    let mut presets: Vec<Entry<Trigger>> = keyed_entries(profiles, |c| &c.triggers)
        .into_iter()
        .filter(|e| e.copies.iter().all(|(_, t)| t.preset.is_some()))
        .collect();
    for entry in &mut presets {
        entry.preset = true;
        entry.in_library = entry
            .copies
            .iter()
            .any(|(_, t)| t.preset().is_some_and(|id| library.contains(&id)));
    }
    let preset_names: BTreeSet<&str> = presets.iter().map(|e| e.copies[0].1.key()).collect();
    let mut order: Vec<(usize, Entry<Trigger>)> = Vec::new();
    let mut created = 0;
    for (n, (_, config)) in profiles.iter().enumerate() {
        let run: Vec<&Trigger> = run_order(&config.triggers)
            .into_iter()
            .filter(|t| !preset_names.contains(t.name.as_str()))
            .collect();
        order = merge_run(order, n, &run, &mut created);
    }
    // The first entry made for a name belongs to the first profile that
    // has the name, and keeps it.
    let mut by_age: Vec<usize> = (0..order.len()).collect();
    by_age.sort_by_key(|i| order[*i].0);
    let mut taken: BTreeSet<String> = preset_names.iter().map(|n| (*n).to_string()).collect();
    let mut renamed = Vec::new();
    for i in by_age {
        let name = order[i].1.copies[0].1.name.clone();
        if taken.insert(name) {
            continue;
        }
        renamed.push(i);
    }
    for i in renamed {
        let entry = &mut order[i].1;
        let who: Vec<&str> = entry
            .copies
            .iter()
            .map(|(holder, _)| profiles[*holder].0.as_str())
            .collect();
        let stem = format!("{} ({})", entry.copies[0].1.name, who.join(", "));
        let mut name = stem.clone();
        let mut n = 2;
        while taken.contains(&name) {
            name = format!("{stem} {n}");
            n += 1;
        }
        taken.insert(name.clone());
        for (_, trigger) in &mut entry.copies {
            trigger.name.clone_from(&name);
        }
    }
    let mut entries: Vec<Entry<Trigger>> = order.into_iter().map(|(_, entry)| entry).collect();
    entries.extend(presets);
    entries
}

/// The triggers of one profile file in the order its store runs them. A
/// store keeps the last copy of a name, in the place of that copy, and
/// runs a higher priority first, keeping the order within one priority.
fn run_order(triggers: &[Trigger]) -> Vec<&Trigger> {
    let mut run: Vec<&Trigger> = Vec::new();
    for trigger in triggers {
        run.retain(|t| t.name != trigger.name);
        run.push(trigger);
    }
    run.sort_by_key(|t| std::cmp::Reverse(t.priority));
    run
}

/// Merge `run`, the triggers of profile `holder` in the order its store
/// runs them, into `order`, the catalog so far with the age of each
/// entry. The longest run of copies that `order` already holds in the
/// same order, the same in name and content, joins those entries, and
/// every other copy becomes a new entry in its place, so the catalog
/// keeps the order of every profile merged so far.
fn merge_run(
    order: Vec<(usize, Entry<Trigger>)>,
    holder: usize,
    run: &[&Trigger],
    created: &mut usize,
) -> Vec<(usize, Entry<Trigger>)> {
    let keys: Vec<String> = run.iter().map(|t| content_key(*t)).collect();
    let held: Vec<String> = order
        .iter()
        .map(|(_, e)| content_key(&e.copies[0].1))
        .collect();
    let (n, m) = (keys.len(), held.len());
    // common[i][j] is the length of the longest common run of keys[i..]
    // and held[j..].
    let mut common = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            common[i][j] = if keys[i] == held[j] {
                common[i + 1][j + 1] + 1
            } else {
                common[i + 1][j].max(common[i][j + 1])
            };
        }
    }
    let mut rest = order.into_iter();
    let mut merged = Vec::with_capacity(n + m);
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && keys[i] == held[j] {
            if let Some(mut entry) = rest.next() {
                entry.1.copies.push((holder, run[i].clone()));
                merged.push(entry);
            }
            i += 1;
            j += 1;
        } else if j < m && (i == n || common[i][j + 1] >= common[i + 1][j]) {
            merged.extend(rest.next());
            j += 1;
        } else {
            merged.push((
                *created,
                Entry {
                    copies: vec![(holder, run[i].clone())],
                    preset: false,
                    in_library: false,
                },
            ));
            *created += 1;
            i += 1;
        }
    }
    merged
}

/// What a copy holds apart from its folder and whether it is on, which
/// the catalog group carries.
fn content_key<T: CatalogItem>(item: &T) -> String {
    let mut item = item.clone();
    item.set_group(None);
    item.set_enabled(true);
    serde_json::to_string(&item).unwrap_or_default()
}

/// True when `list`, an `enabled_presets` list, has `preset` on. An
/// empty list means the defaults, and every preset in the library is on
/// by default, as `presets_on_in_any` in `loadouts/presets.rs` relies on too.
pub(super) fn preset_on(list: &[String], preset: &str) -> bool {
    list.is_empty() || list.iter().any(|id| id == preset)
}

/// Give each copy of each entry its catalog group and on state, then
/// collapse every entry whose copies agree into `auto`, the copy of the
/// first profile that holds it, and hand the others to `conflicts`. The
/// copies of a preset trigger always collapse, since a launch installs
/// the version the library holds, whichever one the catalog keeps.
fn resolve<T: CatalogItem>(
    profiles: &[(String, ProfileConfig)],
    entries: Vec<Entry<T>>,
    plan: &KindPlan,
    kind: ItemKind,
    auto: &mut Vec<T>,
    conflicts: &mut Vec<Conflict>,
) {
    for (n, mut entry) in entries.into_iter().enumerate() {
        // Whether each profile had its copy on, before the copies take
        // the on state of the catalog.
        let switched_on: Vec<bool> = entry.copies.iter().map(|(_, i)| i.enabled()).collect();
        for (_, item) in &mut entry.copies {
            item.set_group(plan.groups[n].clone());
            item.set_enabled(plan.enabled[n]);
        }
        let first = serde_json::to_string(&entry.copies[0].1).ok();
        let agree = entry
            .copies
            .iter()
            .all(|(_, item)| serde_json::to_string(item).ok() == first);
        if entry.preset || agree {
            auto.push(entry.copies.swap_remove(0).1);
            continue;
        }
        // Copies of one version hold the same text once they share the
        // catalog group and on state.
        let versions_on: BTreeSet<String> = entry
            .copies
            .iter()
            .zip(&switched_on)
            .filter(|(_, on)| **on)
            .filter_map(|((_, item), _)| serde_json::to_string(item).ok())
            .collect();
        let default_holder = match versions_on.len() {
            1 => entry
                .copies
                .iter()
                .zip(&switched_on)
                .find(|(_, on)| **on)
                .map_or(entry.copies[0].0, |((holder, _), _)| *holder),
            _ => entry.copies[0].0,
        };
        conflicts.push(Conflict {
            kind,
            name: entry.copies[0].1.key().to_string(),
            default_source: profiles[default_holder].0.clone(),
            variants: entry
                .copies
                .into_iter()
                .zip(switched_on)
                .map(|((holder, item), switched_on)| Variant {
                    source_profile: profiles[holder].0.clone(),
                    switched_on,
                    item: item.payload(),
                })
                .collect(),
        });
    }
}

/// Make `config` the file `profile` keeps in loadout mode, once the
/// catalog holds every item. The aliases, triggers, and macros leave the
/// file. Each group checkbox list names every catalog group of its kind
/// that is off for the profile, built from that kind alone, and the
/// folder map names the catalog groups each of its folders became (see
/// [`FileGroups`]). While no active loadout declares any groups, the
/// lists keep the profile to what it had on. Without them a profile with
/// no items of its own, or with every group off, would turn on every
/// other character's items.
pub(crate) fn profile_file_for_catalog(
    config: &mut ProfileConfig,
    profile: &str,
    plan: &MigrationPlan,
) {
    config.clear_catalog_items();
    let groups = plan.files.get(profile).cloned().unwrap_or_default();
    config.disabled_alias_groups = groups.disabled_alias_groups;
    config.disabled_trigger_groups = groups.disabled_trigger_groups;
    config.disabled_macro_groups = groups.disabled_macro_groups;
    config.group_folders = groups.folders;
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use vosh_automation::alias::Alias;
    use vosh_automation::trigger::{Trigger, TriggerAction};

    // The group naming tests in groups.rs build their plans with the
    // helpers marked pub(in super::super).

    pub(in super::super) fn trigger(name: &str, pattern: &str, replacement: &str) -> Trigger {
        Trigger::new(
            name,
            pattern,
            TriggerAction::Replace {
                template: replacement.to_string(),
            },
        )
    }

    pub(in super::super) fn profile_with(
        aliases: Vec<Alias>,
        triggers: Vec<Trigger>,
        macros: Vec<Macro>,
    ) -> ProfileConfig {
        ProfileConfig {
            aliases,
            triggers,
            macros,
            ..ProfileConfig::default()
        }
    }

    pub(in super::super) fn grouped(name: &str, expansion: &str, group: &str) -> Alias {
        let mut alias = Alias::new(name, expansion);
        alias.group = Some(group.into());
        alias
    }

    /// The file `profile` keeps after the wizard.
    pub(in super::super) fn file_after(plan: &MigrationPlan, profile: &str) -> ProfileConfig {
        let mut file = ProfileConfig::default();
        profile_file_for_catalog(&mut file, profile, plan);
        file
    }

    pub(in super::super) fn alias_group(plan: &MigrationPlan, name: &str) -> Option<String> {
        let found = plan.auto_resolved.aliases.iter().find(|a| a.name == name);
        found.unwrap().group.clone()
    }

    /// The preset library these tests install from.
    const LIBRARY: &[&str] = &["healing_basics", "potion_labels", "herb_labels"];

    pub(in super::super) fn analyze(profiles: &[(String, ProfileConfig)]) -> MigrationPlan {
        analyze_profiles(profiles, LIBRARY)
    }

    #[test]
    fn unique_items_pass_through_as_auto_resolved() {
        let kk = Alias::new("kk", "kick %1");
        let plan = analyze(&[(
            "default".into(),
            profile_with(vec![kk.clone()], vec![], vec![]),
        )]);
        assert_eq!(plan.conflicts.len(), 0);
        assert_eq!(plan.auto_resolved.aliases.len(), 1);
        // Every profile has it on without a folder, so it needs no group.
        assert_eq!(plan.auto_resolved.aliases[0].group, None);
    }

    #[test]
    fn identical_aliases_across_profiles_auto_resolve() {
        let kk = Alias::new("kk", "kick %1");
        let plan = analyze(&[
            (
                "default".into(),
                profile_with(vec![kk.clone()], vec![], vec![]),
            ),
            (
                "warrior".into(),
                profile_with(vec![kk.clone()], vec![], vec![]),
            ),
        ]);
        assert_eq!(plan.conflicts.len(), 0);
        // One catalog entry, not two, on for both without a group.
        assert_eq!(plan.auto_resolved.aliases.len(), 1);
        assert_eq!(plan.auto_resolved.aliases[0].group, None);
        for loadout in &plan.loadouts {
            let leftover = &loadout.enabled_groups;
            assert!(leftover.is_empty(), "{leftover:?}");
        }
    }

    #[test]
    fn diverging_aliases_surface_as_conflict() {
        let kk_a = Alias::new("kk", "kick %1");
        let kk_b = Alias::new("kk", "kick 1.");
        let plan = analyze(&[
            (
                "default".into(),
                profile_with(vec![kk_a.clone()], vec![], vec![]),
            ),
            (
                "warrior".into(),
                profile_with(vec![kk_b.clone()], vec![], vec![]),
            ),
        ]);
        assert_eq!(plan.auto_resolved.aliases.len(), 0);
        assert_eq!(plan.conflicts.len(), 1);
        let conflict = &plan.conflicts[0];
        assert_eq!(conflict.kind, ItemKind::Alias);
        assert_eq!(conflict.name, "kk");
        assert_eq!(conflict.variants.len(), 2);
        // Both source profiles are represented and tagged
        // distinguishably so the wizard can show provenance.
        let sources: Vec<_> = conflict
            .variants
            .iter()
            .map(|v| v.source_profile.as_str())
            .collect();
        assert!(sources.contains(&"default"));
        assert!(sources.contains(&"warrior"));
    }

    #[test]
    fn aliases_that_differ_only_in_their_lua_body_surface_as_conflict() {
        let plain = Alias::new("bash", "bash %1");
        let mut scripted = plain.clone();
        scripted.script = Some("mud.send('bash ' .. args[2])".into());
        let plan = analyze(&[
            ("default".into(), profile_with(vec![plain], vec![], vec![])),
            (
                "warrior".into(),
                profile_with(vec![scripted], vec![], vec![]),
            ),
        ]);
        // One of them used to stand in for both, so the warrior lost its
        // script without a word.
        let leftover = &plan.auto_resolved.aliases;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.conflicts[0].name, "bash");
    }

    fn switched_off(mut alias: Alias) -> Alias {
        alias.enabled = false;
        alias
    }

    /// Which profile each variant of the first conflict came from, with
    /// whether its profile had that copy on.
    fn variants_on(conflict: &Conflict) -> Vec<(&str, bool)> {
        conflict
            .variants
            .iter()
            .map(|v| (v.source_profile.as_str(), v.switched_on))
            .collect()
    }

    #[test]
    fn a_conflict_defaults_to_the_one_version_switched_on() {
        // Default and the Bard keep their kk off, and only the Healer's
        // version is on.
        let f1 = |command: &str, enabled: bool| Macro {
            key: "f1".into(),
            command: command.into(),
            group: None,
            enabled,
        };
        let plan = analyze(&[
            (
                "default".into(),
                profile_with(
                    vec![switched_off(Alias::new("kk", "kick %1"))],
                    vec![],
                    vec![f1("cast heal", false)],
                ),
            ),
            (
                "Healer".into(),
                profile_with(
                    vec![Alias::new("kk", "kick 1.")],
                    vec![],
                    vec![f1("cast sanctuary", true)],
                ),
            ),
            (
                "Bard".into(),
                profile_with(
                    vec![switched_off(Alias::new("kk", "kick %1"))],
                    vec![],
                    vec![f1("cast sanctuary", true)],
                ),
            ),
        ]);
        let kk = plan.conflicts.iter().find(|c| c.name == "kk").unwrap();
        // The pick used to be the first profile, whose version fired
        // nothing, so the Healer lost the kk it used.
        assert_eq!(kk.default_source, "Healer");
        assert_eq!(
            variants_on(kk),
            [("default", false), ("Healer", true), ("Bard", false)]
        );
        // The version you keep comes over on, in the group of the Healer.
        let ItemPayload::Alias { item } = &kk.variants[0].item else {
            panic!("an alias conflict");
        };
        assert!(item.enabled);
        assert_eq!(item.group.as_deref(), Some("(Healer)"));
        // Two profiles have the one version that is on, and the first of
        // them is the pick.
        let f1 = plan.conflicts.iter().find(|c| c.name == "f1").unwrap();
        assert_eq!(f1.default_source, "Healer");
        assert_eq!(
            variants_on(f1),
            [("default", false), ("Healer", true), ("Bard", true)]
        );
    }

    #[test]
    fn a_conflict_with_two_versions_on_or_none_defaults_to_the_first_profile() {
        let plan = analyze(&[
            (
                "default".into(),
                profile_with(vec![Alias::new("kk", "kick %1")], vec![], vec![]),
            ),
            (
                "Healer".into(),
                profile_with(
                    vec![
                        Alias::new("kk", "kick 1."),
                        switched_off(Alias::new("dd", "dirt 1.")),
                    ],
                    vec![],
                    vec![],
                ),
            ),
            (
                "Bard".into(),
                profile_with(
                    vec![switched_off(Alias::new("dd", "dirt %1"))],
                    vec![],
                    vec![],
                ),
            ),
        ]);
        for conflict in &plan.conflicts {
            let first = conflict.variants[0].source_profile.as_str();
            assert_eq!(conflict.default_source, first, "{}", conflict.name);
        }
        let dd = plan.conflicts.iter().find(|c| c.name == "dd").unwrap();
        assert_eq!(variants_on(dd), [("Healer", false), ("Bard", false)]);
    }

    fn macro_in(key: &str, command: &str, group: Option<&str>, enabled: bool) -> Macro {
        Macro {
            key: key.into(),
            command: command.into(),
            group: group.map(String::from),
            enabled,
        }
    }

    #[test]
    fn a_key_bound_twice_keeps_the_copy_the_input_bar_fires() {
        // The input bar fires the last copy of a key that is on and whose
        // group is on. The wizard used to keep the last copy, whatever
        // its state, so F1 went quiet and F2 cast the wrong spell.
        let mut cfg = profile_with(
            vec![],
            vec![],
            vec![
                macro_in("f1", "cast heal", None, true),
                macro_in("f1", "cast armor", None, false),
                macro_in("f2", "cast bless", Some("combat"), true),
                macro_in("f2", "cast curse", Some("loot"), true),
                macro_in("f3", "flee", None, false),
                macro_in("f3", "recall", None, false),
            ],
        );
        cfg.disabled_macro_groups = vec!["loot".into()];
        let plan = analyze(&[("Healer".into(), cfg)]);
        let command = |key: &str| {
            let found = plan.auto_resolved.macros.iter().find(|m| m.key == key);
            let found = found.unwrap_or_else(|| panic!("no macro {key}"));
            (found.command.as_str(), found.enabled, found.group.clone())
        };
        assert_eq!(command("f1"), ("cast heal", true, None));
        assert_eq!(command("f2"), ("cast bless", true, Some("combat".into())));
        // No copy fires, so the last one stays, off.
        assert_eq!(command("f3"), ("recall", false, None));
        assert_eq!(plan.auto_resolved.macros.len(), 3);
    }

    pub(in super::super) fn trigger_named<'a>(plan: &'a MigrationPlan, name: &str) -> &'a Trigger {
        let found = plan.auto_resolved.triggers.iter().find(|t| t.name == name);
        found.unwrap_or_else(|| panic!("no trigger {name}"))
    }

    #[test]
    fn divergent_triggers_keep_each_version() {
        let plan = analyze(&[
            (
                "default".into(),
                profile_with(vec![], vec![trigger("greet", "^hi$", "HELLO")], vec![]),
            ),
            (
                "warrior".into(),
                profile_with(vec![], vec![trigger("greet", "^hi$", "GREETINGS")], vec![]),
            ),
        ]);
        // A trigger's name is only a label, so both versions stay, each
        // on for the profile that had it. The wizard used to keep one for
        // both of them.
        assert!(plan.conflicts.is_empty());
        let default = trigger_named(&plan, "greet");
        assert_eq!(default.group.as_deref(), Some("(default)"));
        assert!(matches!(
            &default.actions[0],
            TriggerAction::Replace { template } if template == "HELLO"
        ));
        let warrior = trigger_named(&plan, "greet (warrior)");
        assert_eq!(warrior.group.as_deref(), Some("(warrior)"));
        assert!(matches!(
            &warrior.actions[0],
            TriggerAction::Replace { template } if template == "GREETINGS"
        ));
    }

    #[test]
    fn profiles_that_share_a_version_of_a_trigger_share_its_copy() {
        let hello = trigger("greet", "^hi$", "HELLO");
        let mut in_folder = hello.clone();
        in_folder.group = Some("social".into());
        let plan = analyze(&[
            ("default".into(), profile_with(vec![], vec![hello], vec![])),
            (
                "Healer".into(),
                profile_with(vec![], vec![in_folder], vec![]),
            ),
            (
                "Bard".into(),
                profile_with(vec![], vec![trigger("greet", "^hi$", "HI")], vec![]),
            ),
            (
                "Rich".into(),
                profile_with(vec![], vec![trigger("greet (Bard)", "^yo$", "YO")], vec![]),
            ),
        ]);
        assert!(plan.conflicts.is_empty());
        assert_eq!(plan.auto_resolved.triggers.len(), 3);
        assert_eq!(
            trigger_named(&plan, "greet").group.as_deref(),
            Some("social")
        );
        // The name the Bard's version would take is a trigger of its own.
        let bard = trigger_named(&plan, "greet (Bard) 2");
        assert_eq!(bard.group.as_deref(), Some("(Bard)"));
        assert_eq!(
            trigger_named(&plan, "greet (Bard)").group.as_deref(),
            Some("(Rich)")
        );
    }

    fn with_priority(mut t: Trigger, priority: i32) -> Trigger {
        t.priority = priority;
        t
    }

    fn names(triggers: &[Trigger]) -> Vec<&str> {
        triggers.iter().map(|t| t.name.as_str()).collect()
    }

    /// The triggers of `profile` in the order its store runs them after
    /// the wizard, from the catalog.
    fn run_order(plan: &MigrationPlan, profile: &str) -> Vec<String> {
        let off = file_after(plan, profile).disabled_trigger_groups;
        let mut store = vosh_automation::trigger::TriggerStore::new();
        for t in &plan.auto_resolved.triggers {
            store.set(t.clone()).unwrap();
        }
        store.set_disabled_groups(off);
        store
            .list()
            .into_iter()
            .filter(|t| store.is_group_enabled(t.group.as_deref().unwrap_or("")))
            .map(|t| t.actions_summary())
            .collect()
    }

    trait Summary {
        fn actions_summary(&self) -> String;
    }

    impl Summary for Trigger {
        fn actions_summary(&self) -> String {
            match &self.actions[0] {
                TriggerAction::Replace { template } => template.clone(),
                other => format!("{other:?}"),
            }
        }
    }

    #[test]
    fn triggers_keep_the_order_the_file_had() {
        // Both match the same line at the same priority, and the file
        // stands up before it bashes.
        let plan = analyze(&[(
            "default".into(),
            profile_with(
                vec![],
                vec![
                    trigger("zz stand", "^You are knocked down", "stand"),
                    trigger("aa bash", "^You are knocked down", "bash"),
                    with_priority(trigger("mm first", "^x", "first"), 5),
                ],
                vec![],
            ),
        )]);
        // They used to come over in name order, so bash ran first. The
        // catalog lists them in the order the store runs them.
        assert_eq!(
            names(&plan.auto_resolved.triggers),
            ["mm first", "zz stand", "aa bash"]
        );
        assert_eq!(run_order(&plan, "default"), ["first", "stand", "bash"]);
    }

    #[test]
    fn each_character_keeps_its_own_trigger_order() {
        let stand = trigger("stand", "^You are knocked down", "stand");
        let bash = trigger("bash", "^You are knocked down", "bash");
        let flee = trigger("flee", "^You are knocked down", "flee");
        let plan = analyze(&[
            (
                "default".into(),
                profile_with(vec![], vec![stand.clone(), bash.clone()], vec![]),
            ),
            (
                "Healer".into(),
                profile_with(vec![], vec![bash, flee.clone(), stand.clone()], vec![]),
            ),
            (
                "Bard".into(),
                profile_with(vec![], vec![flee, stand], vec![]),
            ),
        ]);
        assert!(plan.conflicts.is_empty());
        assert_eq!(run_order(&plan, "default"), ["stand", "bash"]);
        assert_eq!(run_order(&plan, "Healer"), ["bash", "flee", "stand"]);
        assert_eq!(run_order(&plan, "Bard"), ["flee", "stand"]);
        // No one order suits both default and the Healer, so one of them
        // keeps a copy of its own.
        assert_eq!(plan.auto_resolved.triggers.len(), 4);
    }

    #[test]
    fn identical_triggers_collapse_even_when_only_group_differs() {
        let t = trigger("greet", "^hi$", "HELLO");
        let mut in_folder = t.clone();
        in_folder.group = Some("social".into());
        let plan = analyze(&[
            (
                "default".into(),
                profile_with(vec![], vec![t.clone()], vec![]),
            ),
            (
                "warrior".into(),
                profile_with(vec![], vec![in_folder], vec![]),
            ),
        ]);
        assert_eq!(plan.conflicts.len(), 0);
        assert_eq!(plan.auto_resolved.triggers.len(), 1);
    }

    #[test]
    fn a_profile_file_turns_off_every_catalog_group_its_loadout_leaves_off() {
        let plan = analyze(&[
            (
                "default".into(),
                profile_with(
                    vec![
                        Alias::new("kk", "kick %1"),
                        grouped("punch", "punch %1", "combat"),
                    ],
                    vec![],
                    vec![],
                ),
            ),
            ("bard".into(), profile_with(vec![], vec![], vec![])),
        ]);
        let mut file = profile_with(vec![Alias::new("kk", "kick %1")], vec![], vec![]);
        file.disabled_alias_groups = vec!["combat".into()];
        profile_file_for_catalog(&mut file, "bard", &plan);
        let leftover = &file.aliases;
        assert!(leftover.is_empty(), "{leftover:?}");
        // kk was on for the default profile without a folder, so its
        // group is named for that profile.
        assert_eq!(alias_group(&plan, "kk").as_deref(), Some("(default)"));
        assert_eq!(file.disabled_alias_groups, ["(default)", "combat"]);
        let leftover = &file.disabled_trigger_groups;
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn source_profiles_listed_in_iteration_order() {
        let plan = analyze(&[
            ("default".into(), profile_with(vec![], vec![], vec![])),
            ("aabahran".into(), profile_with(vec![], vec![], vec![])),
            ("warrior".into(), profile_with(vec![], vec![], vec![])),
        ]);
        assert_eq!(plan.source_profiles, vec!["default", "aabahran", "warrior"]);
    }
}

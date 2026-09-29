//! Migration analyzer for the Path B "global catalog + loadouts"
//! model. Reads every existing per-profile [`ProfileConfig`] and
//! produces a [`MigrationPlan`] without touching disk.
//!
//! ## What it does
//!
//! For each (alias / trigger / macro) name that appears in two or
//! more source profiles:
//!
//!   - If every variant is byte-equivalent, the item is
//!     **auto-resolved** — a single catalog entry collapses every
//!     copy. No user input needed.
//!   - If the variants differ, the item is a **conflict**. The
//!     plan retains every variant alongside its source profile so
//!     the migration wizard can show the user "default has `kk =
//!     kick %1` and aabahran-erelei has `kk = kick 1.`, which wins,
//!     or keep both?".
//!
//! Items unique to one profile pass through into `auto_resolved`
//! as-is.
//!
//! ## Group names
//!
//! Every item lands in a catalog group named for the profiles that had
//! it on, joined with `+` in index order, then a dot and the group the
//! first of them had it in, when it had one. So `default.combat` holds
//! what only the default profile had on in its combat group, and
//! `default+Healer` holds what both had on without a group. An item no
//! profile had on, in a group it had off or turned off itself, takes the
//! names of the profiles that held it, and its group is off for every
//! profile. Every item in one catalog group is on for the same profiles,
//! so the group checkbox lists each profile file keeps (see
//! [`profile_file_for_catalog`]) turn on exactly what each profile had
//! on. Each kind keeps its own groups, as it keeps its own checkbox
//! list. A name two groups of one kind would share gets a number after
//! it, such as `default.combat 2`.
//!
//! The derived loadout for each profile names every group on for it, so
//! turning on loadout `default` turns on what the default profile had
//! on, the items it shared included, and nothing it did not have.
//!
//! A preset trigger follows the list of presets that are on, which every
//! profile shares in loadout mode, so it counts as on for every profile,
//! the ones whose file lacks it included, since a launch installs every
//! preset that is on. It keeps the group it had, most often none, and
//! gets a group named for profiles only where a profile had it in a
//! group it kept off.
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
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use vosh_alias::Alias;
use vosh_trigger::Trigger;

use crate::loadout::{GlobalCatalog, Loadout};
use crate::profile::Macro;
use crate::profile_config::ProfileConfig;

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
    pub item: ItemPayload,
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
}

/// Result of analyzing the existing profiles. Phase B2 turns this
/// into actual on-disk state once the user resolves conflicts.
#[derive(Debug, Clone, Serialize, Default)]
pub(crate) struct MigrationPlan {
    /// Items that need no user decision — single source, or every
    /// source agreed on the content. Already carry their catalog group
    /// (see module docs).
    pub auto_resolved: GlobalCatalog,
    /// Items with diverging variants. The wizard asks the user to
    /// pick one or rename-and-keep-all.
    pub conflicts: Vec<Conflict>,
    /// One loadout per source profile, with `enabled_groups` naming
    /// every catalog group on for that profile. Connection defaults,
    /// tick config, and `profile_vars` stay in the profile file.
    pub loadouts: Vec<Loadout>,
    /// Names of source profiles the plan covered. Useful for the
    /// wizard summary header.
    pub source_profiles: Vec<String>,
    /// Each catalog group of each kind with the profiles it is on for,
    /// which the group checkbox lists of each profile file follow. See
    /// [`profile_file_for_catalog`].
    #[serde(skip)]
    pub groups: CatalogGroups,
}

/// Walk every source profile, bucket items by (kind, name), classify
/// each bucket as auto-resolved or conflicted, and emit the plan.
///
/// `profiles` is an ordered list so determinism is preserved: when
/// every variant agrees, the FIRST profile in the iteration order
/// becomes the canonical source (i.e. the auto-resolved entry
/// inherits its group prefix).
pub(crate) fn analyze_profiles(profiles: &[(String, ProfileConfig)]) -> MigrationPlan {
    let mut plan = MigrationPlan {
        source_profiles: profiles.iter().map(|(name, _)| name.clone()).collect(),
        ..MigrationPlan::default()
    };

    // Bucket aliases / triggers / macros by name across every
    // profile, tagging each variant with its source. Each bucket is
    // a Vec of (source_profile, item) where item has its `group` and
    // on state rewritten per the migration rule, the same for every
    // variant in the bucket.
    let mut aliases_by_name: BTreeMap<String, Vec<(String, Alias)>> = BTreeMap::new();
    let mut triggers_by_name: BTreeMap<String, Vec<(String, Trigger)>> = BTreeMap::new();
    let mut macros_by_key: BTreeMap<String, Vec<(String, Macro)>> = BTreeMap::new();

    for (profile_name, cfg) in profiles {
        for alias in &cfg.aliases {
            aliases_by_name
                .entry(alias.name.clone())
                .or_default()
                .push((profile_name.clone(), alias.clone()));
        }
        for trigger in &cfg.triggers {
            triggers_by_name
                .entry(trigger.name.clone())
                .or_default()
                .push((profile_name.clone(), trigger.clone()));
        }
        for mac in &cfg.macros {
            macros_by_key
                .entry(mac.key.clone())
                .or_default()
                .push((profile_name.clone(), mac.clone()));
        }
    }
    plan.groups = CatalogGroups {
        aliases: group_buckets(profiles, &mut aliases_by_name),
        triggers: group_buckets(profiles, &mut triggers_by_name),
        macros: group_buckets(profiles, &mut macros_by_key),
    };

    // Classify each bucket.
    for (name, variants) in aliases_by_name {
        if let Some(canonical) = collapse(&variants) {
            plan.auto_resolved.aliases.push(canonical);
        } else {
            plan.conflicts.push(Conflict {
                kind: ItemKind::Alias,
                name,
                variants: variants
                    .into_iter()
                    .map(|(src, item)| Variant {
                        source_profile: src,
                        item: ItemPayload::Alias { item },
                    })
                    .collect(),
            });
        }
    }
    for (name, variants) in triggers_by_name {
        // A launch installs the version of a preset trigger the library
        // holds, whichever one the catalog keeps, so there is nothing to
        // ask you.
        let presets = variants.iter().all(|(_, t)| t.preset.is_some());
        let canonical = if presets {
            variants.first().map(|(_, t)| t.clone())
        } else {
            collapse(&variants)
        };
        if let Some(canonical) = canonical {
            plan.auto_resolved.triggers.push(canonical);
        } else {
            plan.conflicts.push(Conflict {
                kind: ItemKind::Trigger,
                name,
                variants: variants
                    .into_iter()
                    .map(|(src, item)| Variant {
                        source_profile: src,
                        item: ItemPayload::Trigger { item },
                    })
                    .collect(),
            });
        }
    }
    for (key, variants) in macros_by_key {
        if let Some(canonical) = collapse(&variants) {
            plan.auto_resolved.macros.push(canonical);
        } else {
            plan.conflicts.push(Conflict {
                kind: ItemKind::Macro,
                name: key,
                variants: variants
                    .into_iter()
                    .map(|(src, item)| Variant {
                        source_profile: src,
                        item: ItemPayload::Macro { item },
                    })
                    .collect(),
            });
        }
    }

    plan.loadouts = profiles
        .iter()
        .map(|(name, _)| derive_loadout(name, &plan.groups))
        .collect();

    plan
}

/// Each catalog group of one kind, with the profiles it is on for.
pub(crate) type GroupsOn = BTreeMap<String, BTreeSet<String>>;

/// The catalog groups of each kind, with the profiles each is on for.
/// Each kind stands alone, the way each kind keeps its own group
/// checkbox list, so a group name one profile had on for aliases and off
/// for triggers stays on for the aliases and off for the triggers.
#[derive(Debug, Clone, Default)]
pub(crate) struct CatalogGroups {
    pub(crate) aliases: GroupsOn,
    pub(crate) triggers: GroupsOn,
    pub(crate) macros: GroupsOn,
}

impl CatalogGroups {
    /// Every group of any kind that is on for `profile`, once each, the
    /// aliases first, then the triggers, then the macros.
    fn on_for(&self, profile: &str) -> Vec<String> {
        let mut on: Vec<String> = Vec::new();
        for groups in [&self.aliases, &self.triggers, &self.macros] {
            for (group, profiles) in groups {
                if profiles.contains(profile) && !on.contains(group) {
                    on.push(group.clone());
                }
            }
        }
        on
    }
}

/// What the wizard reads and rewrites on an alias, a trigger, or a
/// macro.
trait CatalogItem: Clone + Serialize {
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
}

impl CatalogItem for Alias {
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
}

impl CatalogItem for Trigger {
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
}

impl CatalogItem for Macro {
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
}

/// Where one name of one kind lands in the catalog, see
/// [`group_buckets`].
struct Placement {
    /// The group it asks for, None for a preset trigger that stays
    /// without one.
    group: Option<String>,
    /// The profiles it is on for.
    on: BTreeSet<String>,
    /// True when some profile had the item turned on. It comes over
    /// turned on, and its group decides whom it is on for. An item every
    /// profile turned off comes over turned off, stays off in any group,
    /// and joins a group of its name whomever that is on for.
    enabled: bool,
}

/// True when `list`, an `enabled_presets` list, has `preset` on. An
/// empty list means the defaults, and every preset in the library is on
/// by default, as `presets_on_in_any` in `loadout_store.rs` relies on too.
fn preset_on(list: &[String], preset: &str) -> bool {
    list.is_empty() || list.iter().any(|id| id == preset)
}

/// Where the variants of one name land, from each holder's copy. A store
/// keeps the last copy of a name, so a later copy in one file wins.
fn place<T: CatalogItem>(
    profiles: &[(String, ProfileConfig)],
    variants: &[(String, T)],
) -> Placement {
    let presets = variants.iter().all(|(_, item)| item.preset().is_some());
    // The holders in index order, the group each copy sits in, whether
    // that group is on for its holder, whether the copy is on, and for a
    // preset trigger, whether its holder kept it off by its group.
    let mut holders: Vec<&str> = Vec::new();
    let mut group_of: BTreeMap<&str, Option<&str>> = BTreeMap::new();
    let mut group_on: BTreeSet<&str> = BTreeSet::new();
    let mut enabled: BTreeSet<&str> = BTreeSet::new();
    let mut kept_off: BTreeSet<&str> = BTreeSet::new();
    for (source, item) in variants {
        if !holders.contains(&source.as_str()) {
            holders.push(source);
        }
        let config = profiles
            .iter()
            .find(|(name, _)| name == source)
            .map(|(_, config)| config);
        let group = item.group().filter(|g| !g.is_empty());
        group_of.insert(source, group);
        let off = group.is_some_and(|g| {
            config.is_some_and(|config| T::groups_off(config).iter().any(|o| o == g))
        });
        if off {
            group_on.remove(source.as_str());
        } else {
            group_on.insert(source);
        }
        if item.enabled() {
            enabled.insert(source);
        } else {
            enabled.remove(source.as_str());
        }
        // A launch takes out the triggers of a preset that is off, so
        // only a preset its holder had on can have been off by a group.
        let preset_was_on = item
            .preset()
            .zip(config)
            .is_some_and(|(preset, config)| preset_on(&config.ui.enabled_presets, preset));
        if off && preset_was_on {
            kept_off.insert(source);
        } else {
            kept_off.remove(source.as_str());
        }
    }
    let first_group = || {
        holders
            .first()
            .and_then(|h| group_of.get(h).copied())
            .flatten()
    };
    if presets {
        // A launch installs every preset that is on, whatever the file
        // held, so a preset trigger is on for every profile but one that
        // kept it off by its group, and comes over turned on.
        let on: Vec<&str> = profiles
            .iter()
            .map(|(name, _)| name.as_str())
            .filter(|name| !kept_off.contains(name))
            .collect();
        if on.len() == profiles.len() {
            return Placement {
                group: first_group().map(str::to_string),
                on: on.iter().map(|p| (*p).to_string()).collect(),
                enabled: true,
            };
        }
        let named_for = if on.is_empty() { &holders } else { &on };
        return Placement {
            group: Some(group_name(named_for, &group_of)),
            on: on.iter().map(|p| (*p).to_string()).collect(),
            enabled: true,
        };
    }
    let any_enabled = !enabled.is_empty();
    // An item some profile had on is on for each holder whose copy was
    // on in a group it had on. An item every profile turned off would be
    // on, once you turn it on, where its group was on.
    let on: Vec<&str> = holders
        .iter()
        .copied()
        .filter(|h| group_on.contains(h) && (!any_enabled || enabled.contains(h)))
        .collect();
    let named_for = if on.is_empty() { &holders } else { &on };
    Placement {
        group: Some(group_name(named_for, &group_of)),
        on: on.iter().map(|p| (*p).to_string()).collect(),
        enabled: any_enabled,
    }
}

/// The group named for `profiles`, joined with `+`, then a dot and the
/// group the first of them that holds the item had it in, when it had
/// one.
fn group_name(profiles: &[&str], group_of: &BTreeMap<&str, Option<&str>>) -> String {
    let prefix = profiles.join("+");
    let group = profiles
        .iter()
        .find_map(|p| group_of.get(p).copied())
        .flatten();
    match group {
        Some(g) => format!("{prefix}.{g}"),
        None => prefix,
    }
}

/// Put every variant in each bucket of one kind in its catalog group,
/// and return each catalog group with the profiles it is on for. The
/// group is named for the profiles that had the item on, see the module
/// docs, so every item in one group is on for the same profiles. An item
/// on for any profile comes over turned on, since its group decides for
/// whom, and an item every profile turned off comes over turned off. The
/// holders of an item used to share one group whatever each had on, so
/// an item a profile had off came on for it with the others.
fn group_buckets<T: CatalogItem>(
    profiles: &[(String, ProfileConfig)],
    buckets: &mut BTreeMap<String, Vec<(String, T)>>,
) -> GroupsOn {
    let placements: Vec<Placement> = buckets
        .values()
        .map(|variants| place(profiles, variants))
        .collect();
    // The items that decide whom a group is on for claim their names
    // first, so an item turned off everywhere never pushes one of them
    // to a numbered name.
    let mut groups = GroupsOn::new();
    let mut names: Vec<Option<String>> = placements
        .iter()
        .map(|p| {
            let group = p.group.clone().filter(|_| p.enabled)?;
            Some(claim_group(&mut groups, group, p.on.clone()))
        })
        .collect();
    for (placement, name) in placements.iter().zip(names.iter_mut()) {
        if let (None, false, Some(group)) = (&name, placement.enabled, &placement.group) {
            groups
                .entry(group.clone())
                .or_insert_with(|| placement.on.clone());
            *name = Some(group.clone());
        }
    }
    for ((variants, placement), name) in buckets.values_mut().zip(&placements).zip(names) {
        for (_, item) in variants.iter_mut() {
            item.set_group(name.clone());
            item.set_enabled(placement.enabled);
        }
    }
    groups
}

/// Record `name` as a group on for `on`, and return the name it takes.
/// A name another group of this kind already holds, on for other
/// profiles, gets a number after it, so no group is on for two sets of
/// profiles.
fn claim_group(groups: &mut GroupsOn, name: String, on: BTreeSet<String>) -> String {
    let mut candidate = name.clone();
    let mut n = 2;
    loop {
        match groups.get(&candidate) {
            None => {
                groups.insert(candidate.clone(), on);
                return candidate;
            }
            Some(existing) if *existing == on => return candidate,
            Some(_) => {
                candidate = format!("{name} {n}");
                n += 1;
            }
        }
    }
}

/// Collapse the variants of one name into one catalog item when every
/// variant is the same in every field, or None when they differ and the
/// wizard asks you which to keep. The variants of one name already share
/// their catalog group and on state, see [`group_buckets`].
fn collapse<T: CatalogItem>(variants: &[(String, T)]) -> Option<T> {
    let first = &variants.first()?.1;
    let text = serde_json::to_string(first).ok()?;
    variants[1..]
        .iter()
        .all(|(_, other)| serde_json::to_string(other).ok().as_deref() == Some(text.as_str()))
        .then(|| first.clone())
}

/// Build a loadout for one source profile. `enabled_groups` names every
/// catalog group of any kind that is on for the profile (see
/// [`group_buckets`]), so turning the loadout on turns on what the
/// profile had on. A loadout names groups for every kind at once, so a group
/// name on for one kind and off for another stays in.
fn derive_loadout(profile_name: &str, groups: &CatalogGroups) -> Loadout {
    // The variables, tick, and connection stay in the profile file, which
    // loadout mode loads them from. No runtime code reads them from a
    // loadout, so a copy here would only go stale beside the file.
    let mut loadout = Loadout::empty(profile_name);
    loadout.enabled_groups = groups.on_for(profile_name);
    loadout
}

/// Make `config` the file `profile` keeps in loadout mode, once the
/// catalog holds every item. The aliases, triggers, and macros leave the
/// file. Each group checkbox list names every catalog group of its kind
/// that is off for the profile (see [`group_buckets`]), built from that
/// kind alone. While no active loadout declares any groups, the lists keep
/// the profile to what it had on. Without them a profile with no items
/// of its own, or with every group off, would turn on every other
/// character's items.
pub(crate) fn profile_file_for_catalog(
    config: &mut ProfileConfig,
    profile: &str,
    groups: &CatalogGroups,
) {
    config.clear_catalog_items();
    config.disabled_alias_groups = groups_left_off(&groups.aliases, profile);
    config.disabled_trigger_groups = groups_left_off(&groups.triggers, profile);
    config.disabled_macro_groups = groups_left_off(&groups.macros, profile);
}

/// Each group in `groups` that is off for `profile`, sorted.
fn groups_left_off(groups: &GroupsOn, profile: &str) -> Vec<String> {
    groups
        .iter()
        .filter(|(_, on)| !on.contains(profile))
        .map(|(group, _)| group.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vosh_alias::Alias;
    use vosh_trigger::{Trigger, TriggerAction, TriggerPattern};

    fn single_pattern(p: &str) -> Vec<TriggerPattern> {
        vec![TriggerPattern {
            pattern: p.to_string(),
            enabled: true,
        }]
    }

    fn trigger(name: &str, pattern: &str, replacement: &str) -> Trigger {
        Trigger {
            name: name.to_string(),
            patterns: single_pattern(pattern),
            priority: 0,
            enabled: true,
            actions: vec![TriggerAction::Replace {
                template: replacement.to_string(),
            }],
            preset: None,
            group: None,
            target: vosh_trigger::TriggerTarget::Line,
        }
    }

    fn profile_with(
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

    #[test]
    fn unique_items_pass_through_as_auto_resolved() {
        let kk = Alias::new("kk", "kick %1");
        let plan = analyze_profiles(&[(
            "default".into(),
            profile_with(vec![kk.clone()], vec![], vec![]),
        )]);
        assert_eq!(plan.conflicts.len(), 0);
        assert_eq!(plan.auto_resolved.aliases.len(), 1);
        // Migrated alias picks up the source-profile group tag.
        assert_eq!(
            plan.auto_resolved.aliases[0].group.as_deref(),
            Some("default")
        );
    }

    #[test]
    fn identical_aliases_across_profiles_auto_resolve() {
        let kk = Alias::new("kk", "kick %1");
        let plan = analyze_profiles(&[
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
        // One catalog entry, not two, in a group that names both
        // profiles, which both loadouts turn on.
        assert_eq!(plan.auto_resolved.aliases.len(), 1);
        assert_eq!(
            plan.auto_resolved.aliases[0].group.as_deref(),
            Some("default+warrior")
        );
        for loadout in &plan.loadouts {
            assert_eq!(loadout.enabled_groups, ["default+warrior"]);
        }
    }

    #[test]
    fn every_variant_of_a_conflict_lands_in_the_group_of_all_its_holders() {
        let mut kk = Alias::new("kk", "kick 1.");
        kk.group = Some("combat".into());
        let plan = analyze_profiles(&[
            (
                "default".into(),
                profile_with(vec![Alias::new("kk", "kick %1")], vec![], vec![]),
            ),
            ("bard".into(), profile_with(vec![], vec![], vec![])),
            ("warrior".into(), profile_with(vec![kk], vec![], vec![])),
        ]);
        let ItemPayload::Alias { item } = &plan.conflicts[0].variants[1].item else {
            panic!("an alias conflict");
        };
        // The first holder had kk ungrouped, so whichever version you
        // keep, it stays on for both of them and off for the bard.
        assert_eq!(item.group.as_deref(), Some("default+warrior"));
        assert!(plan.loadouts[1].enabled_groups.is_empty());
        assert_eq!(plan.loadouts[2].enabled_groups, ["default+warrior"]);
    }

    #[test]
    fn diverging_aliases_surface_as_conflict() {
        let kk_a = Alias::new("kk", "kick %1");
        let kk_b = Alias::new("kk", "kick 1.");
        let plan = analyze_profiles(&[
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
        let plan = analyze_profiles(&[
            ("default".into(), profile_with(vec![plain], vec![], vec![])),
            (
                "warrior".into(),
                profile_with(vec![scripted], vec![], vec![]),
            ),
        ]);
        // One of them used to stand in for both, so the warrior lost its
        // script without a word.
        assert!(plan.auto_resolved.aliases.is_empty());
        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.conflicts[0].name, "bash");
    }

    #[test]
    fn divergent_triggers_surface_as_conflict() {
        let plan = analyze_profiles(&[
            (
                "default".into(),
                profile_with(vec![], vec![trigger("greet", "^hi$", "HELLO")], vec![]),
            ),
            (
                "warrior".into(),
                profile_with(vec![], vec![trigger("greet", "^hi$", "GREETINGS")], vec![]),
            ),
        ]);
        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.conflicts[0].kind, ItemKind::Trigger);
        assert_eq!(plan.conflicts[0].name, "greet");
    }

    #[test]
    fn identical_triggers_collapse_even_when_only_group_differs() {
        // The post-retag group always differs across source profiles
        // by construction. That difference must not surface as a
        // conflict — the comparison strips it before equality.
        let t = trigger("greet", "^hi$", "HELLO");
        let plan = analyze_profiles(&[
            (
                "default".into(),
                profile_with(vec![], vec![t.clone()], vec![]),
            ),
            (
                "warrior".into(),
                profile_with(vec![], vec![t.clone()], vec![]),
            ),
        ]);
        assert_eq!(plan.conflicts.len(), 0);
        assert_eq!(plan.auto_resolved.triggers.len(), 1);
    }

    #[test]
    fn loadouts_carry_enabled_groups_per_source() {
        let kk = Alias::new("kk", "kick %1");
        let mut combat_alias = Alias::new("punch", "punch %1");
        combat_alias.group = Some("combat".into());
        let plan = analyze_profiles(&[(
            "default".into(),
            profile_with(vec![kk, combat_alias], vec![], vec![]),
        )]);
        assert_eq!(plan.loadouts.len(), 1);
        let loadout = &plan.loadouts[0];
        assert_eq!(loadout.name, "default");
        // Ungrouped items contribute the bare profile name; items
        // pre-grouped as `combat` contribute `default.combat`.
        assert!(loadout.enabled_groups.contains(&"default".to_string()));
        assert!(loadout
            .enabled_groups
            .contains(&"default.combat".to_string()));
    }

    #[test]
    fn a_loadout_leaves_out_a_group_its_profile_had_off() {
        let mut punch = Alias::new("punch", "punch %1");
        punch.group = Some("combat".into());
        let mut sanc = Alias::new("sanc", "cast sanctuary");
        sanc.group = Some("buffs".into());
        let mut flee = trigger("flee", "^You flee", "flee");
        flee.group = Some("combat".into());
        let mut cfg = profile_with(vec![punch, sanc], vec![flee], vec![]);
        cfg.disabled_alias_groups = vec!["combat".into(), "buffs".into()];
        let plan = analyze_profiles(&[("default".into(), cfg)]);
        // Buffs was off in the only list that has it. The combat triggers
        // were on, so combat stays in for them.
        assert_eq!(plan.loadouts[0].enabled_groups, ["default.combat"]);
    }

    #[test]
    fn a_profile_file_turns_off_every_catalog_group_its_loadout_leaves_off() {
        let mut punch = Alias::new("punch", "punch %1");
        punch.group = Some("combat".into());
        let plan = analyze_profiles(&[
            (
                "default".into(),
                profile_with(vec![Alias::new("kk", "kick %1"), punch], vec![], vec![]),
            ),
            ("bard".into(), profile_with(vec![], vec![], vec![])),
        ]);
        let mut file = profile_with(vec![Alias::new("kk", "kick %1")], vec![], vec![]);
        file.disabled_alias_groups = vec!["combat".into()];
        profile_file_for_catalog(&mut file, "bard", &plan.groups);
        assert!(file.aliases.is_empty());
        assert_eq!(file.disabled_alias_groups, ["default", "default.combat"]);
        assert!(file.disabled_trigger_groups.is_empty());
    }

    #[test]
    fn each_kind_keeps_its_own_group_state_under_one_name() {
        // The Healer had its loot alias on and its auto loot trigger off,
        // both in a group named loot.
        let mut loot_alias = Alias::new("loot", "get all corpse");
        loot_alias.group = Some("loot".into());
        let mut loot_trigger = trigger("autoloot", "^You killed", "get all corpse");
        loot_trigger.group = Some("loot".into());
        let mut cfg = profile_with(vec![loot_alias], vec![loot_trigger], vec![]);
        cfg.disabled_trigger_groups = vec!["loot".into()];
        let plan = analyze_profiles(&[("Healer".into(), cfg)]);

        let mut file = ProfileConfig::default();
        profile_file_for_catalog(&mut file, "Healer", &plan.groups);
        // The trigger group stays off. It used to come on with the alias
        // group of the same name, so the trigger looted every kill.
        assert!(file.disabled_alias_groups.is_empty());
        assert_eq!(file.disabled_trigger_groups, ["Healer.loot"]);
    }

    /// The group checkbox lists `profile` keeps after the wizard.
    fn file_after(plan: &MigrationPlan, profile: &str) -> ProfileConfig {
        let mut file = ProfileConfig::default();
        profile_file_for_catalog(&mut file, profile, &plan.groups);
        file
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
        let plan = analyze_profiles(&[("default".into(), default), ("Healer".into(), healer)]);

        assert!(plan.conflicts.is_empty());
        let group = |name: &str| {
            let found = plan.auto_resolved.triggers.iter().find(|t| t.name == name);
            found.unwrap().group.clone().unwrap()
        };
        // Only the default profile had auto loot on, so its group names
        // only that profile, and it used to name both.
        assert_eq!(group("autoloot"), "default.loot");
        assert_eq!(group("flee"), "default+Healer");
        assert!(file_after(&plan, "default")
            .disabled_trigger_groups
            .is_empty());
        assert_eq!(
            file_after(&plan, "Healer").disabled_trigger_groups,
            ["default.loot"]
        );
        assert_eq!(plan.loadouts[1].enabled_groups, ["default+Healer"]);
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
        };
        let f1_off = Macro {
            enabled: false,
            ..f1.clone()
        };
        let plan = analyze_profiles(&[
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
        assert_eq!(kk.group.as_deref(), Some("default"));
        assert!(plan.auto_resolved.macros[0].enabled);
        let healer = file_after(&plan, "Healer");
        assert_eq!(healer.disabled_alias_groups, ["default"]);
        assert_eq!(healer.disabled_macro_groups, ["default"]);
    }

    #[test]
    fn an_item_no_holder_had_on_stays_as_each_left_it() {
        let mut off = Alias::new("kk", "kick %1");
        off.enabled = false;
        let mut punch = Alias::new("punch", "punch %1");
        punch.group = Some("combat".into());
        let mut cfg = profile_with(vec![off, punch], vec![], vec![]);
        cfg.disabled_alias_groups = vec!["combat".into()];
        let plan = analyze_profiles(&[("Healer".into(), cfg)]);
        let alias = |name: &str| {
            let found = plan.auto_resolved.aliases.iter().find(|a| a.name == name);
            found.unwrap().clone()
        };
        // The one you turned off stays off, and the one in a group you
        // had off stays on inside that group, which stays off.
        assert!(!alias("kk").enabled);
        assert_eq!(alias("kk").group.as_deref(), Some("Healer"));
        assert!(alias("punch").enabled);
        assert_eq!(alias("punch").group.as_deref(), Some("Healer.combat"));
        assert_eq!(
            file_after(&plan, "Healer").disabled_alias_groups,
            ["Healer.combat"]
        );
        assert_eq!(plan.loadouts[0].enabled_groups, ["Healer"]);
    }

    #[test]
    fn an_item_you_turned_off_stays_in_the_group_you_kept_on() {
        let mut bash = Alias::new("bash", "bash %1");
        bash.group = Some("combat".into());
        let mut dirt = Alias::new("dirt", "dirt %1");
        dirt.group = Some("combat".into());
        dirt.enabled = false;
        let plan = analyze_profiles(&[(
            "Healer".into(),
            profile_with(vec![bash, dirt], vec![], vec![]),
        )]);
        // Both stay in Healer.combat, which stays on, and dirt stays off
        // by its own switch, as you left them.
        for alias in &plan.auto_resolved.aliases {
            assert_eq!(alias.group.as_deref(), Some("Healer.combat"));
            assert_eq!(alias.enabled, alias.name == "bash");
        }
        assert!(file_after(&plan, "Healer").disabled_alias_groups.is_empty());
    }

    #[test]
    fn a_name_two_groups_of_one_kind_would_share_gets_a_number() {
        // Both hold dig and kk. Each had dig off its own way, the default
        // profile by turning it off and the warrior by its group, and both
        // had kk on. Both land under default+warrior, which cannot be on
        // for no one and for both at once.
        let mut dig = Alias::new("dig", "dig");
        dig.enabled = false;
        let mut dig_in_off_group = Alias::new("dig", "dig");
        dig_in_off_group.group = Some("mining".into());
        let mut warrior = profile_with(
            vec![dig_in_off_group, Alias::new("kk", "kick %1")],
            vec![],
            vec![],
        );
        warrior.disabled_alias_groups = vec!["mining".into()];
        let plan = analyze_profiles(&[
            (
                "default".into(),
                profile_with(vec![dig, Alias::new("kk", "kick %1")], vec![], vec![]),
            ),
            ("warrior".into(), warrior),
        ]);
        let group = |name: &str| {
            let found = plan.auto_resolved.aliases.iter().find(|a| a.name == name);
            found.unwrap().group.clone().unwrap()
        };
        assert_eq!(group("dig"), "default+warrior");
        assert_eq!(group("kk"), "default+warrior 2");
        for name in ["default", "warrior"] {
            assert_eq!(
                file_after(&plan, name).disabled_alias_groups,
                ["default+warrior"]
            );
        }
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
        let plan = analyze_profiles(&[
            ("default".into(), default),
            ("Healer".into(), healer),
            ("Bard".into(), ProfileConfig::default()),
        ]);
        // A launch installs the library version either way, so there is
        // nothing to ask you, and no group of any profile gates it.
        assert!(plan.conflicts.is_empty());
        assert_eq!(plan.auto_resolved.triggers[0].group, None);
        for name in ["default", "Healer", "Bard"] {
            assert!(file_after(&plan, name).disabled_trigger_groups.is_empty());
        }
    }

    #[test]
    fn a_preset_trigger_a_profile_kept_off_by_its_group_stays_off_for_it() {
        let mut labelled = heal_preset("^You heal");
        labelled.group = Some("labels".into());
        let mut healer = profile_with(vec![], vec![labelled], vec![]);
        healer.disabled_trigger_groups = vec!["labels".into()];
        let plan = analyze_profiles(&[
            (
                "default".into(),
                profile_with(vec![], vec![heal_preset("^You heal")], vec![]),
            ),
            ("Healer".into(), healer),
            ("Bard".into(), ProfileConfig::default()),
        ]);
        let group = plan.auto_resolved.triggers[0].group.clone();
        assert_eq!(group.as_deref(), Some("default+Bard"));
        assert_eq!(
            file_after(&plan, "Healer").disabled_trigger_groups,
            ["default+Bard"]
        );
        assert!(file_after(&plan, "Bard").disabled_trigger_groups.is_empty());
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
        let plan = analyze_profiles(&[
            (
                "default".into(),
                profile_with(vec![], vec![heal_preset("^You heal")], vec![]),
            ),
            ("Healer".into(), healer),
        ]);
        assert_eq!(plan.auto_resolved.triggers[0].group, None);
        assert!(file_after(&plan, "Healer")
            .disabled_trigger_groups
            .is_empty());
    }

    #[test]
    fn a_preset_trigger_in_a_group_every_profile_kept_on_keeps_that_group() {
        let mut labelled = heal_preset("^You heal");
        labelled.group = Some("labels".into());
        let plan = analyze_profiles(&[
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
        assert!(file_after(&plan, "Healer")
            .disabled_trigger_groups
            .is_empty());
    }

    #[test]
    fn a_loadout_leaves_vars_tick_and_connection_to_the_profile_file() {
        let mut cfg = profile_with(vec![], vec![], vec![]);
        cfg.connection.host = "aabahran.example".into();
        cfg.connection.port = 4000;
        cfg.tick.interval_secs = 45;
        cfg.profile_vars.insert("target".into(), "orc".into());
        let plan = analyze_profiles(&[("default".into(), cfg)]);
        let loadout = &plan.loadouts[0];
        let empty = Loadout::empty("default");
        assert_eq!(loadout.connection.host, empty.connection.host);
        assert_eq!(loadout.connection.port, empty.connection.port);
        assert_eq!(loadout.tick.interval_secs, empty.tick.interval_secs);
        assert!(loadout.profile_vars.is_empty());
    }

    #[test]
    fn source_profiles_listed_in_iteration_order() {
        let plan = analyze_profiles(&[
            ("default".into(), profile_with(vec![], vec![], vec![])),
            ("aabahran".into(), profile_with(vec![], vec![], vec![])),
            ("warrior".into(), profile_with(vec![], vec![], vec![])),
        ]);
        assert_eq!(plan.source_profiles, vec!["default", "aabahran", "warrior"]);
    }
}

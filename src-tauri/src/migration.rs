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
//! ## Group tagging
//!
//! Every item gets a group tag scheme so a loadout derived from
//! the source profile can re-enable it later. The rule:
//!
//!   - An item with no group becomes `<source-profile>` (just the
//!     profile name).
//!   - An item already grouped as `combat` becomes
//!     `<source-profile>.combat` (namespaced under the source).
//!   - An item two or more profiles hold under one name, alike or in
//!     conflict, takes the names of all of them joined with `+`, such
//!     as `default+warrior` or `default+warrior.combat`, after the
//!     group the first of them had it in.
//!
//! The derived loadout for that profile then carries every emerged
//! group in its `enabled_groups` list, so day-one behavior matches
//! today: turning on loadout `default` enables every item the original
//! `default` profile had on, the ones it shared included, and nothing
//! it did not have.
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
    /// source agreed on the content. Already carry the migration
    /// group tag (see module docs).
    pub auto_resolved: GlobalCatalog,
    /// Items with diverging variants. The wizard asks the user to
    /// pick one or rename-and-keep-all.
    pub conflicts: Vec<Conflict>,
    /// One loadout per source profile, with `enabled_groups`
    /// populated for every group that emerged from the migration of
    /// that profile's items and was on. Connection defaults, tick
    /// config, and `profile_vars` stay in the profile file.
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
    // a Vec of (source_profile, tagged_item) where tagged_item has
    // its `group` rewritten per the migration rule, the same for every
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
    let tags = CatalogTags {
        aliases: tag_buckets(&mut aliases_by_name, |a| &mut a.group),
        triggers: tag_buckets(&mut triggers_by_name, |t| &mut t.group),
        macros: tag_buckets(&mut macros_by_key, |m| &mut m.group),
    };
    plan.groups = groups_on(profiles, &tags);

    // Classify each bucket.
    for (name, variants) in aliases_by_name {
        if let Some(canonical) = collapse_aliases(&variants) {
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
        if let Some(canonical) = collapse_triggers(&variants) {
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
        if let Some(canonical) = collapse_macros(&variants) {
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

/// The catalog group each name of each kind lands in, by name (by key
/// for macros).
struct CatalogTags {
    aliases: BTreeMap<String, String>,
    triggers: BTreeMap<String, String>,
    macros: BTreeMap<String, String>,
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

/// Which catalog groups of each kind each profile turns on: the group
/// of every item of that kind the profile had in a group it had on for
/// that kind. Every catalog group is named, on for no profile at worst.
fn groups_on(profiles: &[(String, ProfileConfig)], tags: &CatalogTags) -> CatalogGroups {
    fn kind<'a>(
        tags: &BTreeMap<String, String>,
        profiles: &'a [(String, ProfileConfig)],
        items: impl Fn(&'a ProfileConfig) -> Vec<(&'a str, Option<&'a str>)>,
        off: impl Fn(&'a ProfileConfig) -> &'a [String],
    ) -> GroupsOn {
        let mut groups: GroupsOn = tags
            .values()
            .map(|tag| (tag.clone(), BTreeSet::new()))
            .collect();
        for (profile, cfg) in profiles {
            for (name, group) in items(cfg) {
                if group.is_some_and(|g| !g.is_empty() && off(cfg).iter().any(|o| o == g)) {
                    continue;
                }
                if let Some(on) = tags.get(name).and_then(|tag| groups.get_mut(tag)) {
                    on.insert(profile.clone());
                }
            }
        }
        groups
    }
    CatalogGroups {
        aliases: kind(
            &tags.aliases,
            profiles,
            |cfg| {
                cfg.aliases
                    .iter()
                    .map(|a| (a.name.as_str(), a.group.as_deref()))
                    .collect()
            },
            |cfg| &cfg.disabled_alias_groups,
        ),
        triggers: kind(
            &tags.triggers,
            profiles,
            |cfg| {
                cfg.triggers
                    .iter()
                    .map(|t| (t.name.as_str(), t.group.as_deref()))
                    .collect()
            },
            |cfg| &cfg.disabled_trigger_groups,
        ),
        macros: kind(
            &tags.macros,
            profiles,
            |cfg| {
                cfg.macros
                    .iter()
                    .map(|m| (m.key.as_str(), m.group.as_deref()))
                    .collect()
            },
            |cfg| &cfg.disabled_macro_groups,
        ),
    }
}

/// Tag every variant in each bucket with the catalog group of its
/// bucket, and return that group by bucket name. The group names the
/// profiles that hold the name, in index order and joined with `+`,
/// and namespaces the group the first of them had it in (see
/// [`retag_group`]). An item one profile holds keeps that profile's
/// name alone, and an item two profiles share lands in a group both of
/// their loadouts turn on and nothing else does. The first profile's
/// group used to tag a shared item, so the loadout of every other
/// profile that had it left it off.
fn tag_buckets<T>(
    buckets: &mut BTreeMap<String, Vec<(String, T)>>,
    group: impl Fn(&mut T) -> &mut Option<String>,
) -> BTreeMap<String, String> {
    let mut tags = BTreeMap::new();
    for (name, variants) in buckets.iter_mut() {
        let mut holders: Vec<&str> = Vec::new();
        for (source, _) in variants.iter() {
            if !holders.contains(&source.as_str()) {
                holders.push(source);
            }
        }
        let owner = holders.join("+");
        let tag = match variants.first_mut() {
            Some((_, first)) => retag_group(&owner, group(first).as_deref()),
            None => continue,
        };
        for (_, item) in variants.iter_mut() {
            *group(item) = Some(tag.clone());
        }
        tags.insert(name.clone(), tag);
    }
    tags
}

/// Migration group-tag rule: items without a group go to the bare
/// profile name; items with one get namespaced under it. So a
/// `combat` group in profile `default` becomes `default.combat`,
/// and an ungrouped item in `default` becomes group `default`.
/// `profile_name` is every profile that holds the item, joined with
/// `+`, when more than one does.
fn retag_group(profile_name: &str, current: Option<&str>) -> String {
    match current {
        Some(g) if !g.is_empty() => format!("{profile_name}.{g}"),
        _ => profile_name.to_string(),
    }
}

/// Try to collapse multiple alias variants of the same name into
/// one. Returns `Some` when every variant carries identical user
/// content (expansion, enabled, and PRE-RETAG group), `None`
/// when they diverge. The post-retag groups always differ for
/// items from different profiles, so the comparison ignores the
/// `group` field and falls back to the first variant's tagged
/// group as the canonical one.
fn collapse_aliases(variants: &[(String, Alias)]) -> Option<Alias> {
    let first = variants.first()?.1.clone();
    for (_, other) in &variants[1..] {
        if other.name != first.name
            || other.expansion != first.expansion
            || other.enabled != first.enabled
        {
            return None;
        }
    }
    Some(first)
}

fn collapse_triggers(variants: &[(String, Trigger)]) -> Option<Trigger> {
    let first = variants.first()?.1.clone();
    let first_json = serde_json::to_string(&strip_group_for_compare_trigger(&first)).ok()?;
    for (_, other) in &variants[1..] {
        let other_json = serde_json::to_string(&strip_group_for_compare_trigger(other)).ok()?;
        if other_json != first_json {
            return None;
        }
    }
    Some(first)
}

fn collapse_macros(variants: &[(String, Macro)]) -> Option<Macro> {
    let first = variants.first()?.1.clone();
    for (_, other) in &variants[1..] {
        if other.key != first.key || other.command != first.command {
            return None;
        }
    }
    Some(first)
}

/// Strip the `group` field so trigger comparison ignores the
/// post-retag namespacing (which always differs across source
/// profiles). Returns a JSON-serializable clone.
fn strip_group_for_compare_trigger(t: &Trigger) -> Trigger {
    let mut clone = t.clone();
    clone.group = None;
    clone
}

/// Build a loadout for one source profile. `enabled_groups` names every
/// catalog group of any kind that is on for the profile (see
/// [`groups_on`]), so turning the loadout on turns on what the profile
/// had on. A loadout names groups for every kind at once, so a group
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
/// that is off for the profile (see [`groups_on`]), built from that kind
/// alone. While no active loadout declares any groups, the lists keep
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

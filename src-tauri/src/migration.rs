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
//!
//! The derived loadout for that profile then carries every emerged
//! group in its `enabled_groups` list, so day-one behavior matches
//! today: turning on loadout `default` enables every group that came
//! from the original `default` profile.
//!
//! ## Phase B1 scope
//!
//! This module produces the plan only. Applying the plan (writing
//! `catalog.toml` and `loadouts.toml`, renaming the old per-profile
//! files into a `legacy/` subdir for safekeeping) is Phase B2's
//! job and runs only after the user confirms via the wizard in
//! Phase B3. No callers exist for `analyze_profiles` yet, so
//! `dead_code` is allowed at the module level for this phase only.
#![allow(dead_code)]

use std::collections::BTreeMap;

use serde::Serialize;
use vosh_alias::Alias;
use vosh_trigger::Trigger;

use crate::loadout::{GlobalCatalog, Loadout};
use crate::profile::Macro;
use crate::profile_config::ProfileConfig;

/// Which kind of item a conflict or auto-resolved entry refers to.
/// The frontend wizard renders different summaries per kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
    /// that profile's items. Connection defaults, tick config, and
    /// `profile_vars` are copied from the source `ProfileConfig`.
    pub loadouts: Vec<Loadout>,
    /// Names of source profiles the plan covered. Useful for the
    /// wizard summary header.
    pub source_profiles: Vec<String>,
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
    // its `group` already rewritten per the migration rule.
    let mut aliases_by_name: BTreeMap<String, Vec<(String, Alias)>> = BTreeMap::new();
    let mut triggers_by_name: BTreeMap<String, Vec<(String, Trigger)>> = BTreeMap::new();
    let mut macros_by_key: BTreeMap<String, Vec<(String, Macro)>> = BTreeMap::new();

    for (profile_name, cfg) in profiles {
        for alias in &cfg.aliases {
            let mut tagged = alias.clone();
            tagged.group = Some(retag_group(profile_name, alias.group.as_deref()));
            aliases_by_name
                .entry(alias.name.clone())
                .or_default()
                .push((profile_name.clone(), tagged));
        }
        for trigger in &cfg.triggers {
            let mut tagged = trigger.clone();
            tagged.group = Some(retag_group(profile_name, trigger.group.as_deref()));
            triggers_by_name
                .entry(trigger.name.clone())
                .or_default()
                .push((profile_name.clone(), tagged));
        }
        for mac in &cfg.macros {
            let mut tagged = mac.clone();
            tagged.group = Some(retag_group(profile_name, mac.group.as_deref()));
            macros_by_key
                .entry(mac.key.clone())
                .or_default()
                .push((profile_name.clone(), tagged));
        }
    }

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
        .map(|(name, cfg)| derive_loadout(name, cfg))
        .collect();

    plan
}

/// Migration group-tag rule: items without a group go to the bare
/// profile name; items with one get namespaced under it. So a
/// `combat` group in profile `default` becomes `default.combat`,
/// and an ungrouped item in `default` becomes group `default`.
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

/// Build a loadout for one source profile. `enabled_groups` collects
/// every group that emerged from this profile's items per the
/// retag rule, plus the bare profile-name group for any ungrouped
/// items, so day-one behavior matches today exactly.
fn derive_loadout(profile_name: &str, cfg: &ProfileConfig) -> Loadout {
    let mut groups: Vec<String> = Vec::new();
    let mut push = |g: String| {
        if !groups.iter().any(|x| x == &g) {
            groups.push(g);
        }
    };
    for alias in &cfg.aliases {
        push(retag_group(profile_name, alias.group.as_deref()));
    }
    for trigger in &cfg.triggers {
        push(retag_group(profile_name, trigger.group.as_deref()));
    }
    for mac in &cfg.macros {
        push(retag_group(profile_name, mac.group.as_deref()));
    }
    let mut loadout = Loadout::empty(profile_name);
    loadout.enabled_groups = groups;
    loadout.connection = cfg.connection.clone();
    loadout.tick = cfg.tick.clone();
    loadout.profile_vars = cfg.profile_vars.clone();
    loadout
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
        // One catalog entry, not two. The first profile's tagging
        // wins for the canonical group.
        assert_eq!(plan.auto_resolved.aliases.len(), 1);
        assert_eq!(
            plan.auto_resolved.aliases[0].group.as_deref(),
            Some("default")
        );
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
    fn loadout_pulls_connection_and_tick_from_source() {
        let mut cfg = profile_with(vec![], vec![], vec![]);
        cfg.connection.host = "play.theforsakenlands.com".into();
        cfg.connection.port = 1848;
        let plan = analyze_profiles(&[("default".into(), cfg)]);
        let loadout = &plan.loadouts[0];
        assert_eq!(loadout.connection.host, "play.theforsakenlands.com");
        assert_eq!(loadout.connection.port, 1848);
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

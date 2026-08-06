//! Path B data model: global item catalog + loadouts.
//!
//! ## Concept
//!
//! Today every alias / trigger / macro lives inside a specific
//! profile, and switching profiles swaps the whole authored content
//! base. Path B inverts that:
//!
//!   - **[`GlobalCatalog`]** holds every item the user ever defined.
//!     Items are gated for effective enable/disable by their `group`
//!     field — the same per-store `disabled_groups` machinery added
//!     in v0.3.0.
//!   - **[`Loadout`]** is the user-facing "profile": a named set of
//!     groups to enable plus the per-character runtime state (vars,
//!     tick config, connection defaults, auto-match).
//!   - **[`LoadoutSet`]** holds every loadout the user has plus a
//!     list of currently-active ones. Multiple loadouts can stack:
//!     the runtime enables the union of `enabled_groups` across
//!     every currently-active loadout (stack-by-union).
//!
//! ## Phase B1 scope
//!
//! This module ships the types only. The runtime keeps using the
//! per-profile system; nothing here drives behavior yet. Phase B2
//! wires the runtime to read from a `LoadoutSet` and apply its
//! union to the per-store `disabled_groups` sets. Items here have
//! no callers until then, so `dead_code` is allowed at the module
//! level for this phase only.
#![allow(dead_code)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use vosh_alias::Alias;
use vosh_trigger::Trigger;

use crate::profile::Macro;
use crate::profile_config::{ConnectionConfig, TickPersistConfig};
use crate::profile_set::AutoMatch;

/// The global catalog. Every alias, trigger, macro lives here as a
/// flat list with its `group` tag carrying the loadout association.
/// Persisted at `<app_data_dir>/catalog.toml` once Phase B2 wires
/// it as the authoritative source.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct GlobalCatalog {
    #[serde(default)]
    pub aliases: Vec<Alias>,
    #[serde(default)]
    pub triggers: Vec<Trigger>,
    #[serde(default)]
    pub macros: Vec<Macro>,
}

/// One named loadout. Replaces today's "profile" concept from the
/// user's perspective. A loadout has no items of its own — it only
/// references groups in the global catalog and carries the
/// session-coupled state (vars, tick, connection defaults,
/// auto-match) that does NOT cleanly cross-share between
/// characters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Loadout {
    pub name: String,
    /// Optional free-form description shown in the loadout picker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Same auto-match shape as today's per-profile auto-match
    /// (host + port + characters list). The connect-time resolver
    /// walks every loadout in turn looking for a hit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_match: Option<AutoMatch>,
    /// Group names whose items become effective when this loadout
    /// is active. When any active loadout declares groups here, the
    /// runtime gates every alias/trigger/macro on "is your group in
    /// the union of every active loadout's `enabled_groups`?" —
    /// items in an unselected group pass through (alias), don't
    /// fire (trigger), or no-op (macro). When no active loadout
    /// declares any groups, the loadout has no opinion and the
    /// Settings group checkboxes govern instead.
    #[serde(default)]
    pub enabled_groups: Vec<String>,
    /// Per-loadout profile-scoped vars. Stay separated from the
    /// global catalog because vars are state, not authored content
    /// — `$target` should differ across characters even when
    /// they share the same alias library.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub profile_vars: BTreeMap<String, String>,
    /// Per-loadout tick config (same per-loadout rationale).
    #[serde(default)]
    pub tick: TickPersistConfig,
    /// Default host / port / TLS for the connect form when this
    /// loadout is the currently-active picker target.
    #[serde(default)]
    pub connection: ConnectionConfig,
}

impl Loadout {
    /// Build a minimal loadout for a given name. Migrations and the
    /// "+ new loadout" UI both go through this so the default
    /// shape stays consistent.
    pub(crate) fn empty(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            auto_match: None,
            enabled_groups: Vec::new(),
            profile_vars: BTreeMap::new(),
            tick: TickPersistConfig::default(),
            connection: ConnectionConfig::default(),
        }
    }
}

/// Persisted top-level loadout collection. Saved to
/// `<app_data_dir>/loadouts.toml`. The `active` list is the
/// currently-stacked set — Phase B2's runtime gates the catalog on
/// the union of every active loadout's `enabled_groups`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct LoadoutSet {
    /// Loadouts currently considered active. Stack-by-union
    /// semantics. An empty list on its own carries no opinion about
    /// group state; deliberate deactivate-all dormancy is recorded
    /// in `dormant`.
    #[serde(default)]
    pub active: Vec<String>,
    /// True when the user explicitly deactivated every loadout (the
    /// "keep the catalog dormant" kill switch). A flag of its own
    /// because an empty `active` list is ambiguous: never-used
    /// loadout sets also have one, and for those the Settings group
    /// checkboxes govern. Honored at every apply point so dormancy
    /// survives restarts and profile switches.
    #[serde(default)]
    pub dormant: bool,
    /// Every loadout the user has authored.
    #[serde(default)]
    pub loadouts: Vec<Loadout>,
}

impl LoadoutSet {
    /// Look up a loadout by name. Used by the resolver and the
    /// commands surface.
    pub(crate) fn get(&self, name: &str) -> Option<&Loadout> {
        self.loadouts.iter().find(|l| l.name == name)
    }

    /// The effective enabled-group set across every currently-active
    /// loadout. Phase B2 uses this output to compute each store's
    /// `disabled_groups` complement at switch time.
    pub(crate) fn effective_enabled_groups(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for active_name in &self.active {
            let Some(loadout) = self.get(active_name) else {
                continue;
            };
            for g in &loadout.enabled_groups {
                if !out.iter().any(|x| x == g) {
                    out.push(g.clone());
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_enabled_groups_unions_active_loadouts() {
        let mut set = LoadoutSet::default();
        let mut warrior = Loadout::empty("warrior");
        warrior.enabled_groups = vec!["combat-melee".into(), "wartools".into()];
        let mut warmaster = Loadout::empty("warmaster");
        warmaster.enabled_groups = vec!["wm-comm".into(), "wm-watchcommands".into()];
        let mut ranger = Loadout::empty("ranger");
        ranger.enabled_groups = vec!["combat-ranged".into()];
        set.loadouts = vec![warrior, warmaster, ranger];
        set.active = vec!["warrior".into(), "warmaster".into()];

        let union = set.effective_enabled_groups();
        assert!(union.contains(&"combat-melee".to_string()));
        assert!(union.contains(&"wartools".to_string()));
        assert!(union.contains(&"wm-comm".to_string()));
        assert!(union.contains(&"wm-watchcommands".to_string()));
        // Inactive loadout's groups do not leak in.
        assert!(!union.contains(&"combat-ranged".to_string()));
    }

    #[test]
    fn effective_enabled_groups_dedupes_overlap() {
        // Two active loadouts both listing the same group should not
        // produce a duplicate in the effective set.
        let mut set = LoadoutSet::default();
        let mut a = Loadout::empty("a");
        a.enabled_groups = vec!["combat".into(), "buffs".into()];
        let mut b = Loadout::empty("b");
        b.enabled_groups = vec!["combat".into(), "social".into()];
        set.loadouts = vec![a, b];
        set.active = vec!["a".into(), "b".into()];
        let union = set.effective_enabled_groups();
        assert_eq!(union.iter().filter(|g| *g == "combat").count(), 1);
        // Order is first-active-first per the dedup walk.
        assert_eq!(union, vec!["combat", "buffs", "social"]);
    }

    #[test]
    fn effective_enabled_groups_empty_when_no_active() {
        let set = LoadoutSet::default();
        assert!(set.effective_enabled_groups().is_empty());
    }

    #[test]
    fn missing_active_loadout_is_silently_skipped() {
        // A stale name in `active` (e.g. left over from a delete)
        // does not blow up — it just contributes nothing.
        let mut set = LoadoutSet::default();
        let mut a = Loadout::empty("a");
        a.enabled_groups = vec!["combat".into()];
        set.loadouts = vec![a];
        set.active = vec!["a".into(), "ghost".into()];
        assert_eq!(set.effective_enabled_groups(), vec!["combat"]);
    }

    #[test]
    fn loadout_set_round_trips_through_toml() {
        let mut set = LoadoutSet::default();
        let mut warrior = Loadout::empty("warrior");
        warrior.description = Some("Melee main".into());
        warrior.enabled_groups = vec!["combat-melee".into(), "wartools".into()];
        warrior.connection.host = "play.theforsakenlands.com".into();
        warrior.connection.port = 1848;
        set.loadouts = vec![warrior];
        set.active = vec!["warrior".into()];

        let text = toml::to_string_pretty(&set).unwrap();
        let parsed: LoadoutSet = toml::from_str(&text).unwrap();
        assert_eq!(parsed.active, set.active);
        assert_eq!(parsed.loadouts.len(), 1);
        assert_eq!(parsed.loadouts[0].name, "warrior");
        assert_eq!(
            parsed.loadouts[0].description.as_deref(),
            Some("Melee main")
        );
        assert_eq!(
            parsed.loadouts[0].enabled_groups,
            vec!["combat-melee", "wartools"]
        );
    }
}

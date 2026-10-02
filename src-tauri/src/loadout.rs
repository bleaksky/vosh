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
//!   - **[`Loadout`]** is a named set of groups to enable. The
//!     per-character state (vars, tick config, timers, UI settings)
//!     stays in each profile file, which loadout mode loads as per
//!     profile mode does.
//!   - **[`LoadoutSet`]** holds every loadout the user has plus a
//!     list of currently-active ones. Multiple loadouts can stack:
//!     the runtime enables the union of `enabled_groups` across
//!     every currently-active loadout (stack-by-union).

use serde::{Deserialize, Serialize};
use vosh_automation::alias::Alias;
use vosh_trigger::Trigger;

use crate::profile::Macro;
use crate::profile_set::AutoMatch;

/// The global catalog. Every alias, trigger, macro lives here as a
/// flat list with its `group` tag carrying the loadout association.
/// Persisted at `<app_data_dir>/catalog.toml` once Phase B2 wires
/// it as the authoritative source.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct GlobalCatalog {
    #[serde(default)]
    pub aliases: Vec<Alias>,
    /// Every trigger. Room triggers go under `room_triggers` on disk, so
    /// an older build still reads the file (D14), see
    /// [`crate::profile_config::trigger_lists`].
    #[serde(flatten, with = "crate::profile_config::trigger_lists")]
    pub triggers: Vec<Trigger>,
    #[serde(default)]
    pub macros: Vec<Macro>,
    /// The trigger presets that are on, in the `ui.enabled_presets`
    /// shape. The preset triggers live in `triggers` above, which every
    /// profile shares, so the list that says which presets are on is
    /// shared too. `None` in a catalog written before the list moved
    /// here. Startup then takes the active profile's list once, see
    /// [`crate::loadout_store::adopt_catalog_presets`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled_presets: Option<Vec<String>>,
}

impl GlobalCatalog {
    /// The catalog as the live profile holds it: its aliases, triggers,
    /// macros, and enabled presets. Path B persistence writes this.
    pub(crate) fn from_profile(profile: &crate::profile::Profile) -> Self {
        Self {
            aliases: profile.aliases.list().into_iter().cloned().collect(),
            triggers: profile.triggers.list(),
            macros: profile.macros.clone(),
            enabled_presets: Some(profile.ui.enabled_presets.clone()),
        }
    }
}

/// One named loadout. A loadout has no items of its own — it only
/// references groups in the global catalog. Each character's vars,
/// tick and connection live in its profile file. A loadouts.toml that
/// an older build wrote with `profile_vars`, `tick` or `connection`
/// tables still loads, and the next save leaves them out (D12).
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
        let leftover = &set.effective_enabled_groups();
        assert!(leftover.is_empty(), "{leftover:?}");
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
    fn catalog_enabled_presets_round_trip_and_older_files_read_as_none() {
        // A catalog written before the list moved here.
        let older: GlobalCatalog = toml::from_str("").unwrap();
        assert_eq!(older.enabled_presets, None);
        // Nothing to write until startup fills it, so the file stays as
        // an older build wrote it.
        let text = toml::to_string_pretty(&GlobalCatalog::default()).unwrap();
        assert!(!text.contains("enabled_presets"));

        // An empty list means the default presets, and stays apart from
        // a catalog that never took a list.
        for list in [vec![], vec!["healing_basics".to_string()]] {
            let catalog = GlobalCatalog {
                enabled_presets: Some(list.clone()),
                ..GlobalCatalog::default()
            };
            let text = toml::to_string_pretty(&catalog).unwrap();
            let parsed: GlobalCatalog = toml::from_str(&text).unwrap();
            assert_eq!(parsed.enabled_presets, Some(list));
        }
    }

    #[test]
    fn catalog_from_profile_carries_the_enabled_presets() {
        let mut profile = crate::profile::Profile::default();
        profile.ui.enabled_presets = vec!["healing_basics".into(), "potion_labels".into()];
        let catalog = GlobalCatalog::from_profile(&profile);
        assert_eq!(
            catalog.enabled_presets.as_deref(),
            Some(&["healing_basics".to_string(), "potion_labels".to_string()][..])
        );
    }

    #[test]
    fn loadout_set_round_trips_through_toml() {
        let mut set = LoadoutSet::default();
        let mut warrior = Loadout::empty("warrior");
        warrior.description = Some("Melee main".into());
        warrior.enabled_groups = vec!["combat-melee".into(), "wartools".into()];
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

    /// A loadouts.toml from before D12 carries the vars, tick and
    /// connection tables every save used to write. It still loads, with
    /// the rest of each loadout intact, and a save leaves the tables out.
    #[test]
    fn a_file_with_the_old_loadout_tables_still_loads() {
        let older = r#"active = ["warrior"]
dormant = false

[[loadouts]]
name = "warrior"
description = "Melee main"
enabled_groups = ["combat-melee"]

[loadouts.profile_vars]
target = "orc"

[loadouts.tick]
enabled = true
interval_secs = 30
sound = true

[loadouts.connection]
host = "play.theforsakenlands.com"
port = 1848
tls = false
"#;
        let set: LoadoutSet = toml::from_str(older).unwrap();
        assert_eq!(set.active, ["warrior"]);
        let warrior = set.get("warrior").unwrap();
        assert_eq!(warrior.description.as_deref(), Some("Melee main"));
        assert_eq!(warrior.enabled_groups, ["combat-melee"]);

        let text = toml::to_string_pretty(&set).unwrap();
        for table in ["profile_vars", "tick", "connection"] {
            assert!(!text.contains(table), "{table} is written again");
        }
    }
}

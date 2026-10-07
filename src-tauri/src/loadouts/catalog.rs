//! catalog.toml, which holds every alias, trigger, and macro the
//! characters share in loadout mode. Vosh runs in loadout mode while the
//! file is on disk.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::Hash;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tracing::warn;
use vosh_automation::alert::AlertParts;
use vosh_automation::alias::{Alias, AliasStore};
use vosh_automation::trigger::{Trigger, TriggerStore};

use super::gating::apply_effective_state;
use super::preset_edits::PresetEdits;
use super::presets::hold_taken_keys;
use super::set::LoadoutSet;
use super::LoadoutStoreError;
use crate::disk::atomic::write_with_backup;
use crate::disk::paths::catalog_path;
use crate::profile::live::{Macro, Profile};

/// The global catalog. Every alias, trigger, macro lives here as a
/// flat list with its `group` tag carrying the loadout association.
/// Saved at `<app_data>/catalog.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub(crate) struct GlobalCatalog {
    #[serde(default)]
    pub aliases: Vec<Alias>,
    /// Every trigger. Room triggers go under `room_triggers` on disk, so
    /// an older build still reads the file (D14), see
    /// [`crate::profile::file::trigger_lists`].
    #[serde(flatten, with = "crate::profile::file::trigger_lists")]
    pub triggers: Vec<Trigger>,
    #[serde(default)]
    pub macros: Vec<Macro>,
    /// The trigger presets that are on, in the `ui.enabled_presets`
    /// shape. The preset triggers live in `triggers` above, which every
    /// profile shares, so the list that says which presets are on is
    /// shared too. `None` in a catalog written before the list moved
    /// here. Startup then takes the active profile's list once, see
    /// [`super::presets::adopt_catalog_presets`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled_presets: Option<Vec<String>>,
    /// What each alert preset does, by preset id, the `[alerts]` table a
    /// profile file holds in per profile mode. It moves here with
    /// `enabled_presets`, since the list says which of them ring. Left
    /// out while it holds none.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub alerts: BTreeMap<String, AlertParts>,
    /// Your edits to the presets, the `[preset_edits]` table a profile
    /// file holds in per profile mode. Every character shares the preset
    /// triggers here, so they share one set of edits too. Left out while
    /// it holds none.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub preset_edits: PresetEdits,
}

impl GlobalCatalog {
    /// The catalog as the live profile holds it: its aliases, triggers,
    /// macros, enabled presets, alert presets and preset edits. A save in
    /// loadout mode
    /// writes this.
    pub(crate) fn from_profile(profile: &crate::profile::live::Profile) -> Self {
        Self {
            aliases: profile.aliases.list().into_iter().cloned().collect(),
            triggers: profile.triggers.list(),
            macros: profile.macros.clone(),
            enabled_presets: Some(profile.ui.enabled_presets.clone()),
            alerts: profile.alerts.clone(),
            preset_edits: profile.preset_edits.clone(),
        }
    }
}

/// Lay loadout mode's catalog and active loadouts over the live profile
/// `p`, right after launch or a switch loaded a profile file into it. The
/// catalog fills the stores, and the aliases, triggers, and macros the
/// profile file still holds go on top, so a switch keeps them the way a
/// restart does. An item of the file wins over the catalog item of the
/// same name, or for a macro the same key and preset, so your macro on a
/// key sits beside the preset macro there, which is then held off, see
/// [`hold_taken_keys`]. The group state of `set` then applies to the
/// result.
pub(crate) fn lay_catalog_over(p: &mut Profile, catalog: &GlobalCatalog, set: Option<&LoadoutSet>) {
    // What the profile file just put into the live stores, to lay over
    // the catalog.
    let per_profile_aliases: Vec<_> = p.aliases.list().into_iter().cloned().collect();
    let per_profile_triggers: Vec<_> = p.triggers.list();
    let per_profile_macros = p.macros.clone();
    // The per-profile file just restored this profile's group checkbox
    // state into the live stores; carry it across the catalog rebuild
    // (the rebuilt stores would otherwise start with everything
    // enabled).
    let alias_disabled = p.aliases.disabled_groups();
    let trigger_disabled = p.triggers.disabled_groups();
    let mut aliases = AliasStore::new();
    for a in &catalog.aliases {
        aliases.set(a.clone());
    }
    for a in per_profile_aliases {
        aliases.set(a);
    }
    aliases.set_disabled_groups(alias_disabled);
    p.aliases = aliases;
    let mut triggers = TriggerStore::new();
    for t in &catalog.triggers {
        if let Err(e) = triggers.set(t.clone()) {
            warn!(error = %e, "catalog trigger rejected");
        }
    }
    for t in per_profile_triggers {
        if let Err(e) = triggers.set(t) {
            warn!(error = %e, "per-profile trigger rejected");
        }
    }
    triggers.set_disabled_groups(trigger_disabled);
    p.triggers = triggers;
    let mut macros = catalog.macros.clone();
    for m in per_profile_macros {
        macros.retain(|x| x.key != m.key || x.preset != m.preset);
        macros.push(m);
    }
    hold_taken_keys(&mut macros);
    p.macros = macros;
    // The presets that are on belong to the catalog with the preset
    // triggers, so the profile's own list gives way to it.
    if let Some(list) = &catalog.enabled_presets {
        p.ui.enabled_presets.clone_from(list);
        // So do what each alert preset does and your edits to them all.
        p.alerts.clone_from(&catalog.alerts);
        p.preset_edits.clone_from(&catalog.preset_edits);
    }
    if let Some(set) = set {
        apply_effective_state(set, p);
    }
}

/// Lay over `p`, an open profile, what changed in the catalog from
/// `before` to `after`, which a save from another open profile just
/// wrote. An item `after` drops leaves `p` and one it adds or changes
/// comes in, so `p` keeps the edits it has yet to save and the stops Vosh
/// put on the items that did not change. A macro is known by its key and
/// preset, as your macro and a preset macro can share a key. The group
/// state of `set`, the loadouts as `p` gates on them, then applies, since
/// the change may bring a group.
pub(crate) fn lay_catalog_change_over(
    p: &mut Profile,
    before: &GlobalCatalog,
    after: &GlobalCatalog,
    set: Option<&LoadoutSet>,
) {
    let (gone, came) = changes(&before.aliases, &after.aliases, |a| a.name.as_str());
    for name in gone {
        p.aliases.remove(name);
    }
    for alias in came {
        p.aliases.set(alias.clone());
    }
    let (gone, came) = changes(&before.triggers, &after.triggers, |t| t.name.as_str());
    for name in gone {
        p.triggers.remove(name);
    }
    for trigger in came {
        if let Err(e) = p.triggers.set(trigger.clone()) {
            warn!(error = %e, "catalog trigger rejected");
        }
    }
    let (gone, came) = changes(&before.macros, &after.macros, |m| {
        (m.preset.as_deref(), m.key.as_str())
    });
    p.macros
        .retain(|m| !gone.contains(&(m.preset.as_deref(), m.key.as_str())));
    for changed in came {
        match p
            .macros
            .iter_mut()
            .find(|m| m.key == changed.key && m.preset == changed.preset)
        {
            Some(m) => m.clone_from(changed),
            None => p.macros.push(changed.clone()),
        }
    }
    hold_taken_keys(&mut p.macros);
    if after.enabled_presets != before.enabled_presets {
        if let Some(list) = &after.enabled_presets {
            p.ui.enabled_presets.clone_from(list);
        }
    }
    if after.alerts != before.alerts {
        p.alerts.clone_from(&after.alerts);
    }
    if after.preset_edits != before.preset_edits {
        p.preset_edits.clone_from(&after.preset_edits);
    }
    if let Some(set) = set {
        apply_effective_state(set, p);
    }
}

/// The keys of the items of `before` that `after` lacks, and the items of
/// `after` that `before` lacks or holds otherwise, each item known by
/// `key`.
fn changes<'a, T: PartialEq, K: Eq + Hash>(
    before: &'a [T],
    after: &'a [T],
    key: impl Fn(&'a T) -> K,
) -> (Vec<K>, Vec<&'a T>) {
    let was: HashMap<K, &T> = before.iter().map(|item| (key(item), item)).collect();
    let came = after
        .iter()
        .filter(|item| was.get(&key(item)) != Some(item))
        .collect();
    let now: HashSet<K> = after.iter().map(&key).collect();
    let gone = was.into_keys().filter(|k| !now.contains(k)).collect();
    (gone, came)
}

/// True when `catalog.toml` is in the app data folder, which is what
/// puts Vosh in loadout mode. The wizard writes that file the first
/// time, and until then Vosh runs in per profile mode.
pub(crate) fn loadout_mode_on(app_data: &Path) -> bool {
    catalog_path(app_data).exists()
}

/// Load the global catalog. Missing file yields an empty catalog
/// rather than an error so a first launch and a per profile install do
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
/// [`crate::disk::atomic::write_with_backup`] for the rename and
/// retention guarantees.
pub(crate) fn save_global_catalog(
    app_data: &Path,
    catalog: &GlobalCatalog,
) -> Result<(), LoadoutStoreError> {
    let text = toml::to_string_pretty(catalog)?;
    write_with_backup(&catalog_path(app_data), &text)?;
    Ok(())
}

/// What Vosh tells you at launch when catalog.toml does not read.
pub(crate) const UNREAD_CATALOG_NOTICE: &str =
    "Vosh could not read catalog.toml, which holds your shared aliases, triggers, and macros, so \
     they are off and Vosh will not save over it. Fix the file and restart Vosh.";

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use crate::loadouts::tests::tmpdir;

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
        let mut profile = crate::profile::live::Profile::default();
        profile.ui.enabled_presets = vec!["healing_basics".into(), "potion_labels".into()];
        let catalog = GlobalCatalog::from_profile(&profile);
        assert_eq!(
            catalog.enabled_presets.as_deref(),
            Some(&["healing_basics".to_string(), "potion_labels".to_string()][..])
        );
    }

    #[test]
    fn the_overlay_hands_the_catalog_presets_to_the_profile() {
        let catalog = GlobalCatalog {
            enabled_presets: Some(vec!["healing_basics".into()]),
            ..GlobalCatalog::default()
        };
        // A switch just loaded the Healer file, with its own older list.
        let mut p = Profile::default();
        p.ui.enabled_presets = vec!["healing_basics".into(), "potion_labels".into()];
        lay_catalog_over(&mut p, &catalog, None);
        assert_eq!(p.ui.enabled_presets, ["healing_basics"]);
    }

    #[test]
    fn the_alert_presets_move_with_the_list_of_presets_that_are_on() {
        let tells = AlertParts {
            banner: true,
            ..Default::default()
        };
        let catalog = GlobalCatalog {
            enabled_presets: Some(vec!["alert_tells".into()]),
            alerts: std::collections::BTreeMap::from([("alert_tells".into(), tells.clone())]),
            ..GlobalCatalog::default()
        };
        let mut p = Profile::default();
        p.alerts.insert("alert_name".into(), AlertParts::default());
        lay_catalog_over(&mut p, &catalog, None);
        assert_eq!(p.alerts, catalog.alerts);
        assert_eq!(GlobalCatalog::from_profile(&p).alerts, catalog.alerts);
        // A change another open profile saved reaches this one.
        let after = GlobalCatalog {
            alerts: std::collections::BTreeMap::new(),
            ..catalog.clone()
        };
        lay_catalog_change_over(&mut p, &catalog, &after, None);
        assert!(p.alerts.is_empty(), "{:?}", p.alerts);
        // A catalog that never took the list leaves the profile's own.
        let mut q = Profile::default();
        q.alerts.insert("alert_tells".into(), tells);
        lay_catalog_over(&mut q, &GlobalCatalog::default(), None);
        assert_eq!(q.alerts.len(), 1);
    }

    #[test]
    fn the_preset_edits_move_with_the_list_of_presets_that_are_on() {
        use crate::loadouts::preset_edits::{EditRow, PresetEdit};
        let off = PresetEdit {
            triggers: std::collections::BTreeMap::from([(
                "buff.sanctuary".into(),
                std::collections::BTreeMap::from([(
                    "enabled".into(),
                    EditRow {
                        value: false.into(),
                        was: true.into(),
                        seen: None,
                    },
                )]),
            )]),
            ..PresetEdit::default()
        };
        let catalog = GlobalCatalog {
            enabled_presets: Some(vec!["disarm_buff_fade".into()]),
            preset_edits: std::collections::BTreeMap::from([("disarm_buff_fade".into(), off)]),
            ..GlobalCatalog::default()
        };
        let mut p = Profile::default();
        lay_catalog_over(&mut p, &catalog, None);
        assert_eq!(p.preset_edits, catalog.preset_edits);
        assert_eq!(
            GlobalCatalog::from_profile(&p).preset_edits,
            catalog.preset_edits
        );
        // A change another open profile saved reaches this one.
        let after = GlobalCatalog {
            preset_edits: std::collections::BTreeMap::new(),
            ..catalog.clone()
        };
        lay_catalog_change_over(&mut p, &catalog, &after, None);
        assert!(p.preset_edits.is_empty(), "{:?}", p.preset_edits);
    }

    /// A macro of yours on `key`, or one the preset `preset` added.
    fn bind(key: &str, command: &str, preset: Option<&str>) -> Macro {
        Macro {
            key: key.into(),
            command: command.into(),
            group: None,
            enabled: true,
            preset: preset.map(String::from),
        }
    }

    /// What each macro of `p` sends and whether it is on.
    fn sends(p: &Profile) -> Vec<(&str, bool)> {
        p.macros
            .iter()
            .map(|m| (m.command.as_str(), m.enabled))
            .collect()
    }

    /// A catalog with Numpad movement on, two of its macros for short.
    fn numpad_catalog() -> GlobalCatalog {
        let numpad = Some("numpad_movement");
        GlobalCatalog {
            macros: vec![bind("Numpad8", "n", numpad), bind("Numpad3", "d", numpad)],
            ..GlobalCatalog::default()
        }
    }

    #[test]
    fn your_macro_in_the_file_sits_beside_the_preset_macro_on_its_key() {
        // A switch just loaded a file that still holds your Numpad3.
        let mut p = Profile::default();
        p.macros.push(bind("Numpad3", "rec", None));
        lay_catalog_over(&mut p, &numpad_catalog(), None);
        // The preset's d stays, held off while rec keeps the key.
        assert_eq!(sends(&p), [("n", true), ("d", false), ("rec", true)]);
    }

    #[test]
    fn a_change_to_your_macro_leaves_the_preset_macro_on_its_key() {
        let before = numpad_catalog();
        let mut p = Profile::default();
        lay_catalog_over(&mut p, &before, None);
        // Another open profile binds rec to Numpad3 and saves, which
        // holds d off.
        let mut after = before.clone();
        after.macros.push(bind("Numpad3", "rec", None));
        hold_taken_keys(&mut after.macros);
        lay_catalog_change_over(&mut p, &before, &after, None);
        assert_eq!(sends(&p), [("n", true), ("d", false), ("rec", true)]);
        // It deletes rec again, and d takes the key back.
        lay_catalog_change_over(&mut p, &after, &before, None);
        assert_eq!(sends(&p), [("n", true), ("d", true)]);
    }

    #[test]
    fn loadout_mode_off_when_catalog_missing() {
        let dir = tmpdir();
        assert!(!loadout_mode_on(&dir));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn loadout_mode_on_after_catalog_save() {
        let dir = tmpdir();
        save_global_catalog(&dir, &GlobalCatalog::default()).unwrap();
        assert!(loadout_mode_on(&dir));
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
}

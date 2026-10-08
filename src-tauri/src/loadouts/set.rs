//! loadouts.toml, which holds the loadouts you have and which of them
//! are on for each profile, and the switch that turns them on and off.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tracing::warn;

use super::gating::apply_effective_state;
use super::LoadoutStoreError;
use crate::app::events::{broadcast, MACRO_GROUPS_CHANGED};
use crate::app::state::{SharedState, NO_APP_DATA};
use crate::disk::atomic::write_with_backup;
use crate::disk::paths::loadouts_path;
use crate::profile::login_match::AutoMatch;
use crate::profile::open::OpenProfile;

/// One named loadout. A loadout has no items of its own — it only
/// references groups in the global catalog. Each character's vars,
/// tick and connection live in its profile file. A loadouts.toml that
/// an older build wrote with `profile_vars`, `tick` or `connection`
/// tables still loads, and the next save leaves them out, since nothing
/// reads them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Loadout {
    pub name: String,
    /// Optional free-form description shown in the loadout picker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// A world and character list in the profile `auto_match` shape.
    /// Nothing reads it and nothing matches on it. It stays a
    /// stored value so a hand edited loadouts.toml that carries one
    /// keeps it across a save.
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

/// Persisted top-level loadout collection. Saved at
/// `<app_data>/loadouts.toml`. The `active` list is the stacked set of
/// every profile that keeps none of its own in `profiles`. The runtime
/// gates the catalog on the union of every active loadout's
/// `enabled_groups`, see [`super::gating`].
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
    /// The stacks of the profiles that keep their own, by profile name,
    /// see [`LoadoutSet::set_stack`]. Left out of the file while empty,
    /// and an older build, which reads `active` and `dormant` above as
    /// the one stack, passes over it, so a rollback still loads the
    /// file.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub profiles: BTreeMap<String, Stack>,
}

/// The stack of active loadouts a profile keeps of its own, with its
/// dormant switch, in the shape of the top-level `active` and `dormant`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Stack {
    #[serde(default)]
    pub active: Vec<String>,
    #[serde(default)]
    pub dormant: bool,
}

impl LoadoutSet {
    /// The set as the profile `profile` gates on it, with the stack it
    /// keeps of its own in place of the top-level one. A profile with
    /// none, and the defaults before launch names a profile (None), take
    /// the set as it is.
    pub(crate) fn for_profile(&self, profile: Option<&str>) -> Cow<'_, LoadoutSet> {
        match profile.and_then(|name| self.profiles.get(name)) {
            Some(own) => Cow::Owned(LoadoutSet {
                active: own.active.clone(),
                dormant: own.dormant,
                loadouts: self.loadouts.clone(),
                profiles: BTreeMap::new(),
            }),
            None => Cow::Borrowed(self),
        }
    }

    /// Make `stack` the one the profile `profile` gates on. A profile
    /// that keeps a stack of its own changes that. One with none changes
    /// the top-level stack while no other profile in `open`, the names of
    /// the profiles the sessions play, reads it, so one session writes
    /// loadouts.toml as an older build reads it. Once another open profile
    /// reads the top-level stack, the profile takes a stack of its own and
    /// leaves that one as it is.
    pub(crate) fn set_stack(&mut self, profile: Option<&str>, open: &[String], stack: Stack) {
        if let Some(name) = profile {
            let reads_top = |other: &String| other != name && !self.profiles.contains_key(other);
            if self.profiles.contains_key(name) || open.iter().any(reads_top) {
                self.profiles.insert(name.to_string(), stack);
                return;
            }
        }
        self.active = stack.active;
        self.dormant = stack.dormant;
    }

    /// Look up a loadout by name. Used by the resolver and the
    /// commands surface.
    pub(crate) fn get(&self, name: &str) -> Option<&Loadout> {
        self.loadouts.iter().find(|l| l.name == name)
    }

    /// The effective enabled-group set across every currently-active
    /// loadout. [`super::gating`] turns it into each store's
    /// `disabled_groups` complement at launch, at a profile switch, and
    /// when the active list changes.
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

/// Load the loadout collection. Missing file yields an empty
/// `LoadoutSet` (no loadouts, no active stack) so callers can treat
/// "fresh install" and "user wiped their loadouts" identically.
pub(crate) fn load_loadout_set(app_data: &Path) -> Result<LoadoutSet, LoadoutStoreError> {
    let path = loadouts_path(app_data);
    if !path.exists() {
        return Ok(LoadoutSet::default());
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(toml::from_str(&text)?)
}

/// Persist the loadout collection atomically with a rolling backup.
pub(crate) fn save_loadout_set(app_data: &Path, set: &LoadoutSet) -> Result<(), LoadoutStoreError> {
    let text = toml::to_string_pretty(set)?;
    write_with_backup(&loadouts_path(app_data), &text)?;
    Ok(())
}

/// What Vosh tells you at launch when loadouts.toml does not read.
pub(crate) const UNREAD_LOADOUTS_NOTICE: &str =
    "Vosh could not read loadouts.toml, so your shared aliases, triggers, and macros are off and \
     Vosh will not save over it or catalog.toml. Fix the file and restart Vosh.";

/// The part of [`loadouts_set_active`] that runs under the loadout and
/// profile locks: take the new active list as the stack of the profile
/// `profile` names, which a session must play, or else of the one the
/// selected session plays, see [`LoadoutSet::set_stack`], lay the group
/// state it imposes over that profile, and save loadouts.toml in the app
/// data folder. Returns that profile, for the command to queue its save,
/// so a test can run this against a mock app and a scratch folder. When
/// the switch turned a macro group on or off in the profile in front,
/// every window hears it once the locks are released, since the command
/// line keeps its own map of the macro keys that fire.
///
/// [`loadouts_set_active`]: crate::ipc::loadouts::loadouts_set_active
pub(crate) async fn set_active_loadouts<R: tauri::Runtime>(
    app: &AppHandle<R>,
    active: Vec<String>,
    profile: Option<String>,
) -> Result<Arc<OpenProfile>, String> {
    let state: SharedState = app.state::<SharedState>().inner().clone();
    let app_data = state.app_data.get().ok_or(NO_APP_DATA)?;
    // Read from the session map before the loadouts lock.
    let edited = state.edited_profile(profile)?;
    let open: Vec<String> = state
        .open_profiles()
        .iter()
        .filter_map(|open| open.name())
        .collect();
    let (open, macro_groups_changed) = {
        let mut guard = state.loadout_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err("loadout mode is off".into());
        };
        // Filter to known loadout names. A stale name (e.g. from a
        // future-truncated payload) is silently dropped rather than
        // returning an error.
        let active: Vec<String> = active
            .into_iter()
            .filter(|n| set.loadouts.iter().any(|l| &l.name == n))
            .collect();
        // Deactivate-all is the documented kill switch ("Activate none
        // to keep the catalog dormant"). Recorded as an explicit flag:
        // an empty active list on its own is ambiguous with "loadouts
        // have no opinion", and the other apply points (startup,
        // profile switch) must be able to re-impose dormancy.
        let stack = Stack {
            dormant: active.is_empty(),
            active,
        };
        let mut p = edited.lock().await;
        let name = p.name.clone();
        set.set_stack(name.as_deref(), &open, stack);
        let macro_groups_before = p.disabled_macro_groups.clone();
        apply_effective_state(&set.for_profile(name.as_deref()), &mut p);
        if let Err(e) = save_loadout_set(app_data, set) {
            warn!(error = %e, "loadouts.toml save failed");
        }
        (
            p.open().clone(),
            p.disabled_macro_groups != macro_groups_before,
        )
    };
    if macro_groups_changed && state.in_front(&open) {
        broadcast(app, MACRO_GROUPS_CHANGED, &"");
    }
    Ok(open)
}

/// Carry the stack the profile `old` keeps of its own over to `new`, as
/// a rename moves the profile, or drop it when `new` is None, as a
/// delete does, and save loadouts.toml when there was one.
pub(crate) async fn follow_profile_name(state: &SharedState, old: &str, new: Option<&str>) {
    let Some(app_data) = state.app_data.get() else {
        return;
    };
    let mut guard = state.loadout_set.lock().await;
    let Some(set) = guard.as_mut() else {
        return;
    };
    let Some(stack) = set.profiles.remove(old) else {
        return;
    };
    if let Some(new) = new {
        set.profiles.insert(new.to_string(), stack);
    }
    if let Err(e) = save_loadout_set(app_data, set) {
        warn!(error = %e, "loadouts.toml save failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use crate::loadouts::tests::tmpdir;

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

    /// A loadouts.toml from an older build carries the vars, tick and
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

    #[test]
    fn loadout_set_round_trips_through_disk() {
        let dir = tmpdir();
        let mut set = LoadoutSet::default();
        let mut warrior = Loadout::empty("warrior");
        warrior.enabled_groups = vec!["combat-melee".into(), "wartools".into()];
        set.loadouts = vec![warrior];
        set.active = vec!["warrior".into()];

        save_loadout_set(&dir, &set).unwrap();
        let loaded = load_loadout_set(&dir).unwrap();
        assert_eq!(loaded.active, vec!["warrior".to_string()]);
        assert_eq!(loaded.loadouts.len(), 1);
        assert_eq!(loaded.loadouts[0].name, "warrior");
        assert_eq!(
            loaded.loadouts[0].enabled_groups,
            vec!["combat-melee", "wartools"]
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dormant_flag_round_trips_through_disk() {
        let dir = tmpdir();
        let set = LoadoutSet {
            dormant: true,
            ..Default::default()
        };
        save_loadout_set(&dir, &set).unwrap();
        assert!(load_loadout_set(&dir).unwrap().dormant);
        fs::remove_dir_all(&dir).ok();
    }
}

//! The presets. Each one adds triggers or macros, and its install runs
//! the same way in both modes. In loadout mode what the presets add lives
//! in the catalog every character shares, so the catalog owns the list of
//! presets that are on too, and takes it from the profile files the first
//! time. A preset macro can sit on a key one of yours uses, so the bodies
//! of the commands that change your macros live here too, beside the rule
//! that keeps that key yours.

use std::collections::{BTreeMap, BTreeSet};

use vosh_automation::trigger::Trigger;

use super::catalog::GlobalCatalog;
use crate::profile::live::{Macro, Profile};
use crate::profile::set::ProfileSet;

/// What `ui.enabled_presets` holds when you turned every preset off. An
/// empty list means the defaults. Mirrors `PRESETS_OFF_MARKER` in
/// src/automation/automationRecords.ts, and a test here reads that line.
pub(crate) const PRESETS_OFF: &str = "none";

/// The presets an empty `ui.enabled_presets` list turns on, which are the
/// eleven the library held when new presets began to ship off. The list
/// is frozen. A preset added later starts off, and an empty list, the
/// value every profile holds until you change a preset, keeps the meaning
/// it had when it was saved. Mirrors `PRESETS_ON_BY_DEFAULT` in
/// src/automation/presets.ts, and a test here reads that list.
pub(crate) const PRESETS_ON_BY_DEFAULT: &[&str] = &[
    "healing_basics",
    "defensive_combat",
    "disarm_buff_fade",
    "terror_events",
    "combat_outgoing",
    "combat_incoming",
    "loot_progression",
    "potion_labels",
    "herb_labels",
    "sent_tells",
    "room_and_time",
];

/// The enabled preset lists of the profile files, for a catalog that
/// takes the list for the first time. See [`profile_preset_lists`].
#[derive(Debug, Default)]
pub(crate) struct ProfilePresetLists {
    /// The list of every profile file that reads, in index order. A
    /// profile that never saved a file holds no list and is left out.
    pub(crate) lists: Vec<Vec<String>>,
    /// The profiles whose file does not read, in index order. What their
    /// lists hold is unknown.
    pub(crate) unread: Vec<String>,
}

impl ProfilePresetLists {
    /// The lists the catalog takes, or None while it waits. A file that
    /// does not read is left out, so every preset a character whose file
    /// reads had on stays on. With no file that reads but one that does
    /// not, there is nothing to take, and the catalog waits rather than
    /// fall back to the live profile, which holds the defaults when its
    /// own file is the one that does not read.
    fn usable(&self) -> Option<&[Vec<String>]> {
        if self.lists.is_empty() && !self.unread.is_empty() {
            None
        } else {
            Some(&self.lists)
        }
    }

    /// What launch tells you about each profile file that did not read.
    /// `adopted` is true when the catalog took its list from the files
    /// that did.
    pub(crate) fn unread_notices(&self, adopted: bool) -> Vec<String> {
        self.unread
            .iter()
            .map(|name| {
                let name = crate::profile::set::display_name(name);
                if adopted {
                    format!(
                        "Vosh could not read the {name} profile file and left its presets out of \
                         the shared list. Turn on any you miss under Presets in Automation \
                         settings."
                    )
                } else {
                    format!(
                        "Vosh could not read the {name} profile file and will build the shared \
                         preset list once the file reads."
                    )
                }
            })
            .collect()
    }
}

/// The enabled preset list of every profile file in `set`, for a catalog
/// that takes the list for the first time. A file that does not read is
/// named in `unread` and never written.
pub(crate) fn profile_preset_lists(set: &ProfileSet) -> ProfilePresetLists {
    let mut found = ProfilePresetLists::default();
    for stored in set.read_all() {
        match stored.file {
            None => {}
            Some(Ok(file)) => found.lists.push(file.config.ui.enabled_presets),
            Some(Err(e)) => {
                tracing::warn!(
                    error = %e,
                    path = %stored.path.display(),
                    "profile file unreadable; its enabled presets are unknown",
                );
                found.unread.push(stored.name.to_string());
            }
        }
    }
    found
}

/// Every preset that is on in any of `lists`, in the `enabled_presets`
/// shape. An empty list means the defaults, [`PRESETS_ON_BY_DEFAULT`].
/// When one list holds the defaults and the others name no preset past
/// them, the union is the defaults and stays the empty list. A preset
/// past them, one added after the list froze, joins the defaults in a
/// list that names them all. A list that turned every preset off adds
/// none.
fn presets_on_in_any(lists: &[Vec<String>]) -> Vec<String> {
    let defaults = lists.iter().any(Vec::is_empty);
    let mut on: BTreeSet<&str> = lists
        .iter()
        .flatten()
        .map(String::as_str)
        .filter(|id| *id != PRESETS_OFF)
        .collect();
    if defaults {
        on.retain(|id| !PRESETS_ON_BY_DEFAULT.contains(id));
        if on.is_empty() {
            return Vec::new();
        }
        on.extend(PRESETS_ON_BY_DEFAULT);
    }
    if on.is_empty() {
        return vec![PRESETS_OFF.to_string()];
    }
    on.into_iter().map(str::to_string).collect()
}

/// The enabled preset list a catalog takes the first time, from `lists`,
/// the lists of the profile files that hold one. Every preset that any of
/// them had on stays on. With no list at all it takes `live`, the live
/// profile's list. Launch and the shared catalog wizard both use it, so a
/// catalog starts from the same list either way.
pub(crate) fn first_catalog_presets(lists: &[Vec<String>], live: &[String]) -> Vec<String> {
    if lists.is_empty() {
        live.to_vec()
    } else {
        presets_on_in_any(lists)
    }
}

/// Make the catalog own the list of trigger presets that are on. The
/// preset triggers live in the catalog, which every profile shares, so
/// a list kept per profile let a launch as another character put back a
/// preset you had turned off. A catalog written before the list moved
/// here has none, so it takes every preset that any profile file had on,
/// once, from `lists` (see [`profile_preset_lists`]). The launch that
/// follows removes every preset that is off, so a list from one profile
/// alone would take away presets another character used. With no profile
/// file at all it takes the live profile's list. A profile file that does
/// not read is left out (see [`ProfilePresetLists::usable`]). When there
/// is nothing to take (`lists` is None, or no file reads) the catalog
/// waits for a later launch, and the live profile keeps its own list
/// meanwhile. Otherwise the live profile then holds the catalog's list.
/// The profile files keep their own lists as they are. Returns true when
/// the catalog took a list and needs saving.
pub(crate) fn adopt_catalog_presets(
    catalog: &mut GlobalCatalog,
    profile: &mut Profile,
    lists: Option<&ProfilePresetLists>,
) -> bool {
    if let Some(list) = &catalog.enabled_presets {
        profile.ui.enabled_presets.clone_from(list);
        return false;
    }
    let Some(lists) = lists.and_then(ProfilePresetLists::usable) else {
        return false;
    };
    let adopted = first_catalog_presets(lists, &profile.ui.enabled_presets);
    profile.ui.enabled_presets.clone_from(&adopted);
    catalog.enabled_presets = Some(adopted);
    true
}

/// Hold off each preset macro on a key one of your macros uses, and turn
/// every other preset macro on, so a key you already use stays yours.
/// Yours keeps the key while it is on, off or in a group
/// that is off, except a group in `off`, which [`hold_profile_keys`]
/// fills in loadout mode. A held macro saves with `enabled` false, so
/// the command line and 0.8.1 both pass it over. Every step that changes
/// the macros runs this before it saves.
pub(crate) fn hold_taken_keys(macros: &mut [Macro], off: &BTreeSet<String>) {
    let yours: BTreeSet<String> = macros
        .iter()
        .filter(|m| m.preset.is_none())
        .filter(|m| !m.group.as_ref().is_some_and(|g| off.contains(g)))
        .map(|m| m.key.clone())
        .collect();
    for m in macros.iter_mut().filter(|m| m.preset.is_some()) {
        m.enabled = !yours.contains(&m.key);
    }
}

/// [`hold_taken_keys`] over the live profile `p`. In loadout mode every
/// character shares the catalog, and the groups each one keeps off are
/// what sets them apart, so a macro of yours in a group `p` keeps off
/// keeps no key there. Otherwise a character whose only Numpad3 was the
/// preset's d would lose d to a macro another character brought to the
/// catalog. catalog.toml stores the hold with no group left out, see
/// [`GlobalCatalog::from_profile`]. Each step that changes the macros or
/// turns a macro group on or off runs this.
pub(crate) fn hold_profile_keys(p: &mut Profile) {
    let none = BTreeSet::new();
    let off = if p.on_catalog {
        &p.disabled_macro_groups
    } else {
        &none
    };
    hold_taken_keys(&mut p.macros, off);
}

/// The triggers half of [`presets_install`] over the live profile `p`,
/// so a test can run the preset install launch runs. Presets install the
/// same way in both modes. Per profile mode saves the triggers to the
/// profile file, and in loadout mode the live profile holds the catalog's
/// triggers, so the save writes them to catalog.toml.
///
/// `triggers` holds each preset it names whole, so a stored trigger of
/// one of those presets whose name it does not carry comes out. A trigger
/// a preset fix renamed or removed then leaves the store. Returns the
/// names that came out, so the page can name one
/// you edited.
///
/// [`presets_install`]: crate::ipc::automation::presets_install
pub(crate) fn install_preset_triggers(
    p: &mut Profile,
    triggers: Vec<Trigger>,
) -> Result<Vec<String>, String> {
    let presets: BTreeSet<&str> = triggers
        .iter()
        .filter_map(|t| t.preset.as_deref())
        .collect();
    let names: BTreeSet<&str> = triggers.iter().map(|t| t.name.as_str()).collect();
    let removed: Vec<String> = p
        .triggers
        .list()
        .into_iter()
        .filter(|t| t.preset.as_deref().is_some_and(|id| presets.contains(id)))
        .filter(|t| !names.contains(t.name.as_str()))
        .map(|t| t.name)
        .collect();
    for name in &removed {
        p.triggers.remove(name);
    }
    for mut t in triggers {
        // Each install overwrites same-named presets so pattern and
        // template fixes land, but the group is your organization. A
        // built trigger with no group keeps the one its stored copy has,
        // so putting a preset into a group survives relaunch.
        if t.group.is_none() {
            if let Some(existing) = p.triggers.get(&t.name) {
                t.group.clone_from(&existing.group);
            }
        }
        p.triggers.set(t).map_err(|e| e.to_string())?;
    }
    Ok(removed)
}

/// One preset you turned on or off, as `presets_enabled_set` takes it.
#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct PresetSwitch {
    pub(crate) id: String,
    pub(crate) on: bool,
}

/// The `ui.enabled_presets` list once `switches` land on `list`, the
/// list as stored. An empty list means the defaults, so it starts from
/// them. Every id a switch does not name stays as it is, a preset of a
/// newer build among them, and a list left empty stores [`PRESETS_OFF`].
pub(crate) fn switch_presets(list: &[String], switches: &[PresetSwitch]) -> Vec<String> {
    let mut on: Vec<String> = if list.is_empty() {
        PRESETS_ON_BY_DEFAULT
            .iter()
            .map(|id| (*id).to_string())
            .collect()
    } else {
        list.iter()
            .filter(|id| *id != PRESETS_OFF)
            .cloned()
            .collect()
    };
    for s in switches {
        if !s.on {
            on.retain(|id| *id != s.id);
        } else if !on.contains(&s.id) {
            on.push(s.id.clone());
        }
    }
    if on.is_empty() {
        on.push(PRESETS_OFF.to_string());
    }
    on
}

/// The macros half of [`presets_install`] over the live profile `p`.
/// Each macro carries the id of its preset. It takes out every macro of
/// those presets and adds `macros` in the order given, which for Numpad
/// movement is the game's n e s w u d. A macro keeps
/// the group you put it in, as a preset trigger does, and one on a key of
/// yours is held off, see [`hold_taken_keys`]. Returns the number
/// installed.
///
/// [`presets_install`]: crate::ipc::automation::presets_install
pub(crate) fn install_preset_macros(p: &mut Profile, macros: Vec<Macro>) -> Result<usize, String> {
    let mut presets = BTreeSet::new();
    for m in &macros {
        let id = m
            .preset
            .as_deref()
            .ok_or("Each preset macro needs its preset id.")?;
        presets.insert(id.to_string());
    }
    // The group is your organization, so it outlives the reinstall each
    // launch runs, the way install_preset_triggers keeps it.
    let mut groups: BTreeMap<(Option<String>, String), String> = BTreeMap::new();
    p.macros.retain(|m| {
        let theirs = m.preset.as_ref().is_some_and(|id| presets.contains(id));
        if let Some(group) = m.group.as_ref().filter(|_| theirs) {
            groups.insert((m.preset.clone(), m.key.clone()), group.clone());
        }
        !theirs
    });
    let installed = macros.len();
    for mut m in macros {
        if m.group.is_none() {
            m.group = groups.remove(&(m.preset.clone(), m.key.clone()));
        }
        p.macros.push(m);
    }
    hold_profile_keys(p);
    Ok(installed)
}

/// Give the preset tag back to the macros of a preset that came back
/// through a build that drops it. 0.8.1 has no `preset` field, so a save
/// there writes each preset macro as one of yours, the held ones still
/// off. A preset of `macros` is taken as come back when `p` holds no
/// macro tagged with it and holds, for each of its macros, one of yours
/// on the same key with the same command. Each such copy takes the tag,
/// so [`install_preset_macros`] replaces it, with its group kept, and
/// your own macro on the key keeps it. The install appends the preset's
/// macros after yours and holds one off while yours keeps its key, so of
/// two copies on one key the one off, else the later one, is taken as
/// the preset's. Every install of the presets already on runs this, and
/// turning a preset on does not, since that install tags its macros
/// itself, so a preset you turn on never takes a macro of yours that
/// happens to match. Returns the number tagged.
pub(crate) fn retag_returned_macros(p: &mut Profile, macros: &[Macro]) -> usize {
    let presets: BTreeSet<&str> = macros.iter().filter_map(|m| m.preset.as_deref()).collect();
    let mut tagged = 0;
    for id in presets {
        if p.macros.iter().any(|m| m.preset.as_deref() == Some(id)) {
            continue;
        }
        let mut copies: Vec<usize> = Vec::new();
        for want in macros.iter().filter(|m| m.preset.as_deref() == Some(id)) {
            let fits: Vec<usize> = (0..p.macros.len())
                .filter(|i| {
                    let m = &p.macros[*i];
                    m.preset.is_none()
                        && m.key == want.key
                        && m.command == want.command
                        && !copies.contains(i)
                })
                .collect();
            let held = fits.iter().rev().find(|i| !p.macros[**i].enabled);
            match held.or(fits.last()) {
                Some(i) => copies.push(*i),
                None => {
                    copies.clear();
                    break;
                }
            }
        }
        for i in &copies {
            p.macros[*i].preset = Some(id.to_string());
        }
        tagged += copies.len();
    }
    tagged
}

/// Take out every macro the preset `preset` added. Your macros stay as
/// they are. Returns the number removed.
pub(crate) fn remove_preset_macros(p: &mut Profile, preset: &str) -> usize {
    let before = p.macros.len();
    p.macros.retain(|m| m.preset.as_deref() != Some(preset));
    before - p.macros.len()
}

/// The body of [`macros_set`] over the live profile `p`. It finds and
/// adds only your macros, so your macro and a preset macro on one key
/// never overwrite each other. A preset macro takes only its group from you,
/// as a preset trigger does, so a key you use stays yours.
///
/// [`macros_set`]: crate::ipc::automation::macros_set
pub(crate) fn set_macro(
    p: &mut Profile,
    key: &str,
    command: &str,
    group: Option<String>,
    enabled: Option<bool>,
    preset: Option<&str>,
) -> Result<(), String> {
    let key = key.trim();
    let command = command.trim();
    if key.is_empty() {
        return Err("key cannot be empty".into());
    }
    if command.is_empty() {
        return Err("command cannot be empty".into());
    }
    // Normalize the group: empty / whitespace-only -> None so the
    // wire format does not persist an empty group string.
    let group = group
        .map(|g| g.trim().to_string())
        .filter(|g| !g.is_empty());
    if let Some(preset) = preset {
        let theirs = p
            .macros
            .iter_mut()
            .find(|m| m.preset.as_deref() == Some(preset) && m.key == key)
            .ok_or_else(|| format!("That preset has no macro on {key} now."))?;
        theirs.group = group;
    } else if let Some(existing) = p
        .macros
        .iter_mut()
        .find(|m| m.preset.is_none() && m.key == key)
    {
        existing.command = command.to_string();
        existing.group = group;
        if let Some(enabled) = enabled {
            existing.enabled = enabled;
        }
    } else {
        p.macros.push(Macro {
            key: key.to_string(),
            command: command.to_string(),
            group,
            enabled: enabled.unwrap_or(true),
            preset: None,
        });
    }
    hold_profile_keys(p);
    Ok(())
}

/// The body of [`macros_delete`] over the live profile `p`. A preset
/// macro on `key` stays, and takes the key once yours goes.
///
/// [`macros_delete`]: crate::ipc::automation::macros_delete
pub(crate) fn delete_macro(p: &mut Profile, key: &str) {
    p.macros.retain(|m| m.preset.is_some() || m.key != key);
    hold_profile_keys(p);
}

/// The macros half of [`import_apply`] over the live profile `p`. Each
/// macro an import brought on a key of yours gives that macro its
/// command, and the rest join yours. A preset macro on the key stays, and
/// the key is yours.
///
/// [`import_apply`]: crate::ipc::automation::import_apply
pub(crate) fn import_macros(p: &mut Profile, imported: &[Macro]) {
    for m in imported {
        if let Some(existing) = p
            .macros
            .iter_mut()
            .find(|x| x.preset.is_none() && x.key == m.key)
        {
            existing.command.clone_from(&m.command);
        } else {
            p.macros.push(m.clone());
        }
    }
    hold_profile_keys(p);
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;

    use crate::loadouts::catalog::{load_global_catalog, save_global_catalog};
    use crate::profile::file::ProfileConfig;

    fn presets(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    /// Startup in loadout mode over the profile files in `dir` the way
    /// app/launch.rs runs it: load the active profile, then let the
    /// catalog take or hand out the preset list, and save the catalog
    /// when it took one. Hands back the notices launch keeps for you too.
    fn launch_with_notices(dir: &Path, set: &ProfileSet) -> (Profile, Vec<String>) {
        let mut profile = Profile::default();
        let mut notices = crate::profile::file::load_at_launch(set, &mut profile);
        let mut catalog = load_global_catalog(dir).unwrap();
        let lists = catalog
            .enabled_presets
            .is_none()
            .then(|| profile_preset_lists(set));
        let adopted = adopt_catalog_presets(&mut catalog, &mut profile, lists.as_ref());
        if adopted {
            save_global_catalog(dir, &catalog).unwrap();
        }
        if let Some(lists) = &lists {
            notices.extend(lists.unread_notices(adopted));
        }
        (profile, notices)
    }

    fn launch(dir: &Path, set: &ProfileSet) -> Profile {
        launch_with_notices(dir, set).0
    }

    /// Save `name`'s file with `list` as its enabled presets.
    fn write_presets(set: &ProfileSet, name: &str, list: &[&str]) {
        let mut config = ProfileConfig::default();
        config.ui.enabled_presets = presets(list);
        config.save(&set.profile_path(name)).unwrap();
    }

    /// Each profile file holds its own list from before the move. Ilsabet
    /// (default) turned the potion labels off. Healer never did.
    fn two_profiles(dir: &Path) -> ProfileSet {
        let set = crate::profile::tests::james_like_set(dir);
        write_presets(
            &set,
            crate::profile::set::DEFAULT_PROFILE_NAME,
            &["healing_basics"],
        );
        write_presets(&set, "Healer", &["healing_basics", "potion_labels"]);
        // A catalog saved before the list moved into it.
        save_global_catalog(dir, &GlobalCatalog::default()).unwrap();
        set
    }

    #[test]
    fn the_first_launch_keeps_every_preset_any_character_had_on() {
        let dir = tempfile::tempdir().unwrap();
        let set = two_profiles(dir.path());

        // A launch as Ilsabet takes Healer's potion labels too, so the
        // launch plan does not take them away from Healer.
        let ilsabet = launch(dir.path(), &set);
        let both = presets(&["healing_basics", "potion_labels"]);
        assert_eq!(ilsabet.ui.enabled_presets, both);
        assert_eq!(
            load_global_catalog(dir.path()).unwrap().enabled_presets,
            Some(both)
        );
        // The profile files keep their own lists.
        let file = ProfileConfig::load(&set.active_path()).unwrap();
        assert_eq!(file.ui.enabled_presets, presets(&["healing_basics"]));
    }

    #[test]
    fn a_launch_as_another_character_keeps_the_presets_you_turned_off() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = two_profiles(dir.path());
        let _ilsabet = launch(dir.path(), &set);
        // After the move you turn the potion labels off for everyone.
        save_global_catalog(
            dir.path(),
            &GlobalCatalog {
                enabled_presets: Some(presets(&["healing_basics"])),
                ..GlobalCatalog::default()
            },
        )
        .unwrap();

        // A launch as Healer keeps the catalog's list. Before, Healer's
        // own list turned the potion labels back on for everyone.
        set.switch("Healer").unwrap();
        let healer = launch(dir.path(), &set);
        assert_eq!(healer.ui.enabled_presets, presets(&["healing_basics"]));
        assert_eq!(
            load_global_catalog(dir.path()).unwrap().enabled_presets,
            Some(presets(&["healing_basics"]))
        );
    }

    #[test]
    fn a_profile_on_the_defaults_keeps_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let set = two_profiles(dir.path());
        // Healer never changed a preset, so its list means the defaults.
        write_presets(&set, "Healer", &[]);
        // Test-Prompt turned every preset off.
        write_presets(&set, "Test-Prompt", &["none"]);
        let ilsabet = launch(dir.path(), &set);
        let leftover = &ilsabet.ui.enabled_presets;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(
            load_global_catalog(dir.path()).unwrap().enabled_presets,
            Some(Vec::new())
        );
    }

    #[test]
    fn profiles_that_turned_every_preset_off_keep_them_off() {
        assert_eq!(
            presets_on_in_any(&[presets(&["none"]), presets(&["none"])]),
            presets(&["none"])
        );
        assert_eq!(
            presets_on_in_any(&[presets(&["none"]), presets(&["herb_labels"])]),
            presets(&["herb_labels"])
        );
    }

    #[test]
    fn a_profile_file_that_does_not_read_is_left_out_of_the_shared_list() {
        let dir = tempfile::tempdir().unwrap();
        let set = two_profiles(dir.path());
        write_presets(&set, "Test-Prompt", &["healing_basics", "herb_labels"]);
        // Healer is not the profile you launch as, so no other notice
        // tells you its file does not read.
        std::fs::write(set.profile_path("Healer"), "presets = = [\n").unwrap();

        let (ilsabet, notices) = launch_with_notices(dir.path(), &set);
        // The catalog takes every preset a character whose file reads had
        // on, so Test-Prompt keeps its herb labels.
        let on = presets(&["healing_basics", "herb_labels"]);
        assert_eq!(ilsabet.ui.enabled_presets, on);
        assert_eq!(
            load_global_catalog(dir.path()).unwrap().enabled_presets,
            Some(on)
        );
        assert_eq!(
            notices,
            [
                "Vosh could not read the Healer profile file and left its presets out of the \
                 shared list. Turn on any you miss under Presets in Automation settings."
            ]
        );
        // The file that does not read stays as it is.
        assert_eq!(
            std::fs::read_to_string(set.profile_path("Healer")).unwrap(),
            "presets = = [\n"
        );
    }

    #[test]
    fn the_catalog_waits_while_no_profile_file_reads() {
        let dir = tempfile::tempdir().unwrap();
        let set = crate::profile::tests::james_like_set(dir.path());
        save_global_catalog(dir.path(), &GlobalCatalog::default()).unwrap();
        // The only saved file is the one you launch as, and it does not
        // read, so there is no list to take.
        std::fs::write(set.active_path(), "presets = = [\n").unwrap();

        let (ilsabet, notices) = launch_with_notices(dir.path(), &set);
        let leftover = &ilsabet.ui.enabled_presets;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(
            load_global_catalog(dir.path()).unwrap().enabled_presets,
            None
        );
        assert_eq!(
            notices,
            [
                crate::profile::file::unread_profile_notice(
                    crate::profile::set::DEFAULT_PROFILE_NAME
                ),
                "Vosh could not read the Default profile file and will build the shared preset \
                 list once the file reads."
                    .to_string(),
            ]
        );
        assert_eq!(
            std::fs::read_to_string(set.active_path()).unwrap(),
            "presets = = [\n"
        );
    }

    #[test]
    fn the_defaults_are_the_eleven_presets_an_empty_list_has_always_meant() {
        assert_eq!(
            PRESETS_ON_BY_DEFAULT,
            [
                "healing_basics",
                "defensive_combat",
                "disarm_buff_fade",
                "terror_events",
                "combat_outgoing",
                "combat_incoming",
                "loot_progression",
                "potion_labels",
                "herb_labels",
                "sent_tells",
                "room_and_time",
            ]
        );
    }

    #[test]
    fn the_defaults_are_the_list_the_page_holds() {
        let library = include_str!("../../../src/automation/presets.ts");
        let list = regex::Regex::new(
            r"export const PRESETS_ON_BY_DEFAULT: readonly string\[\] = \[([^\]]*)\];",
        )
        .unwrap()
        .captures(library)
        .expect("presets.ts declares PRESETS_ON_BY_DEFAULT");
        let ids: Vec<&str> = regex::Regex::new(r"'([^']*)'")
            .unwrap()
            .captures_iter(list.get(1).unwrap().as_str())
            .map(|id| id.get(1).unwrap().as_str())
            .collect();
        assert_eq!(ids, PRESETS_ON_BY_DEFAULT);
    }

    #[test]
    fn the_defaults_and_presets_past_them_join_in_one_list() {
        // The defaults with a list that names only presets among them stay
        // the defaults, as they always have.
        let leftover = &presets_on_in_any(&[presets(&[]), presets(&["herb_labels", "sent_tells"])]);
        assert!(leftover.is_empty(), "{leftover:?}");
        let leftover = &presets_on_in_any(&[presets(&["none"]), presets(&[])]);
        assert!(leftover.is_empty(), "{leftover:?}");
        // A preset added after the list froze is off by default, so a
        // character who turned it on keeps it beside the defaults.
        let mut joined: Vec<String> = PRESETS_ON_BY_DEFAULT
            .iter()
            .chain(&["later_preset"])
            .map(|id| (*id).to_string())
            .collect();
        joined.sort();
        assert_eq!(
            presets_on_in_any(&[presets(&[]), presets(&["later_preset", "herb_labels"])]),
            joined
        );
    }

    #[test]
    fn presets_off_is_the_marker_the_page_stores() {
        // Settings stores PRESETS_OFF_MARKER when you turn every preset
        // off, and launch reads it back here as PRESETS_OFF.
        let records = include_str!("../../../src/automation/automationRecords.ts");
        let marker = regex::Regex::new(r"export const PRESETS_OFF_MARKER = '([^']*)';")
            .unwrap()
            .captures(records)
            .expect("automationRecords.ts declares PRESETS_OFF_MARKER");
        assert_eq!(&marker[1], PRESETS_OFF);
        // No preset may take the marker as its id.
        let library = include_str!("../../../src/automation/presets.ts");
        assert!(!library.contains(&format!("id: '{PRESETS_OFF}'")));
    }

    #[test]
    fn a_catalog_list_wins_over_the_profile_list() {
        let mut catalog = GlobalCatalog {
            enabled_presets: Some(presets(&["none"])),
            ..GlobalCatalog::default()
        };
        let mut profile = Profile::default();
        profile.ui.enabled_presets = presets(&["healing_basics"]);
        assert!(!adopt_catalog_presets(
            &mut catalog,
            &mut profile,
            Some(&ProfilePresetLists {
                lists: vec![presets(&["potion_labels"])],
                unread: Vec::new(),
            })
        ));
        assert_eq!(profile.ui.enabled_presets, presets(&["none"]));
        assert_eq!(catalog.enabled_presets, Some(presets(&["none"])));
    }

    /// The keys Numpad movement binds and what each sends, in the game's
    /// order n e s w u d.
    const NUMPAD: [(&str, &str); 6] = [
        ("Numpad8", "n"),
        ("Numpad6", "e"),
        ("Numpad2", "s"),
        ("Numpad4", "w"),
        ("Numpad9", "u"),
        ("Numpad3", "d"),
    ];

    /// The six macros Numpad movement adds.
    fn numpad() -> Vec<Macro> {
        NUMPAD
            .into_iter()
            .map(|(key, command)| Macro {
                preset: Some("numpad_movement".into()),
                ..yours(key, command)
            })
            .collect()
    }

    fn yours(key: &str, command: &str) -> Macro {
        Macro {
            key: key.into(),
            command: command.into(),
            group: None,
            enabled: true,
            preset: None,
        }
    }

    /// The key and command of each macro that is on, then of each that
    /// is off.
    fn on_and_off(p: &Profile) -> [Vec<(&str, &str)>; 2] {
        let pick = |on: bool| {
            p.macros
                .iter()
                .filter(|m| m.enabled == on)
                .map(|m| (m.key.as_str(), m.command.as_str()))
                .collect()
        };
        [pick(true), pick(false)]
    }

    #[test]
    fn preset_macros_install_in_the_order_given_and_keep_the_group_you_chose() {
        let mut p = Profile::default();
        p.macros.push(yours("F2", "flee"));
        assert_eq!(install_preset_macros(&mut p, numpad()), Ok(6));
        let commands: Vec<&str> = p.macros.iter().map(|m| m.command.as_str()).collect();
        assert_eq!(commands, ["flee", "n", "e", "s", "w", "u", "d"]);

        // You put n in a group, and the install each launch runs keeps
        // it there and keeps the order.
        let numpad_movement = Some("numpad_movement");
        let travel = Some("travel".to_string());
        set_macro(&mut p, "Numpad8", "n", travel, None, numpad_movement).unwrap();
        assert_eq!(install_preset_macros(&mut p, numpad()), Ok(6));
        let groups: Vec<(&str, Option<&str>)> = p
            .macros
            .iter()
            .map(|m| (m.command.as_str(), m.group.as_deref()))
            .collect();
        assert_eq!(
            groups,
            [
                ("flee", None),
                ("n", Some("travel")),
                ("e", None),
                ("s", None),
                ("w", None),
                ("u", None),
                ("d", None)
            ]
        );

        // A macro with no preset id is refused before anything changes.
        let before = p.macros.clone();
        assert!(install_preset_macros(&mut p, vec![yours("Numpad5", "look")]).is_err());
        assert_eq!(p.macros, before);
    }

    #[test]
    fn preset_macros_back_from_0_8_1_take_their_preset_back() {
        // Your rec on Numpad3 holds the preset d off, and you put n in a
        // group.
        let mut p = Profile::default();
        p.macros.push(yours("Numpad3", "rec"));
        install_preset_macros(&mut p, numpad()).unwrap();
        let travel = Some("travel".to_string());
        set_macro(
            &mut p,
            "Numpad8",
            "n",
            travel,
            None,
            Some("numpad_movement"),
        )
        .unwrap();
        let before = p.macros.clone();

        // 0.8.1 drops the tag as it reads the file and saves the six as
        // macros of yours, d still off.
        for m in &mut p.macros {
            m.preset = None;
        }
        assert_eq!(retag_returned_macros(&mut p, &numpad()), 6);
        install_preset_macros(&mut p, numpad()).unwrap();
        assert_eq!(p.macros, before);
        let [_, off] = on_and_off(&p);
        assert_eq!(off, [("Numpad3", "d")]);

        // Turning the preset off leaves your rec alone.
        remove_preset_macros(&mut p, "numpad_movement");
        assert_eq!(p.macros, [yours("Numpad3", "rec")]);
    }

    #[test]
    fn the_preset_copies_take_the_tag_where_yours_match_them_too() {
        // Your own six on the numpad, then Numpad movement turned on,
        // which holds its six off. Then you bind the six again after it.
        for yours_first in [true, false] {
            let mut p = Profile::default();
            let mine = || NUMPAD.iter().map(|(k, c)| yours(k, c));
            if yours_first {
                p.macros.extend(mine());
            }
            install_preset_macros(&mut p, numpad()).unwrap();
            if !yours_first {
                p.macros.extend(mine());
                hold_profile_keys(&mut p);
            }
            let before = p.macros.clone();

            // 0.8.1 leaves twelve untagged macros, the held six off.
            for m in &mut p.macros {
                m.preset = None;
            }
            assert_eq!(retag_returned_macros(&mut p, &numpad()), 6);
            install_preset_macros(&mut p, numpad()).unwrap();
            let [on, off] = on_and_off(&p);
            assert_eq!(on, NUMPAD, "each key still sends your macro");
            assert_eq!(off, NUMPAD);
            let mut after = p.macros.clone();
            let mut before = before;
            after.sort_by(|a, b| (&a.key, &a.preset).cmp(&(&b.key, &b.preset)));
            before.sort_by(|a, b| (&a.key, &a.preset).cmp(&(&b.key, &b.preset)));
            assert_eq!(after, before);
        }
    }

    #[test]
    fn in_loadout_mode_a_macro_in_a_group_you_keep_off_keeps_no_key() {
        // Another character's rec sits on Numpad3 in a group this one
        // keeps off, so d sends here.
        let mut p = Profile {
            on_catalog: true,
            ..Profile::default()
        };
        p.macros.push(Macro {
            group: Some("(Healer)".into()),
            ..yours("Numpad3", "rec")
        });
        p.disabled_macro_groups.insert("(Healer)".into());
        install_preset_macros(&mut p, numpad()).unwrap();
        let [_, off] = on_and_off(&p);
        assert!(off.is_empty(), "{off:?}");

        // Turning the group on gives the key back to rec, and off again
        // to d.
        crate::script::set_list_group(&mut p, crate::script::GroupList::Macros, "(Healer)", true);
        let [_, off] = on_and_off(&p);
        assert_eq!(off, [("Numpad3", "d")]);
        crate::script::set_list_group(&mut p, crate::script::GroupList::Macros, "(Healer)", false);
        let [_, off] = on_and_off(&p);
        assert!(off.is_empty(), "{off:?}");

        // Per profile mode keeps your key yours too, and rec keeps the
        // key in a group that is off.
        p.on_catalog = false;
        hold_profile_keys(&mut p);
        let [_, off] = on_and_off(&p);
        assert_eq!(off, [("Numpad3", "d")]);
    }

    #[test]
    fn your_own_macros_never_join_a_preset() {
        // The preset still holds its macros, so your copies stay yours.
        let mut p = Profile::default();
        p.macros.push(yours("Numpad8", "n"));
        install_preset_macros(&mut p, numpad()).unwrap();
        assert_eq!(retag_returned_macros(&mut p, &numpad()), 0);

        // Five of the six keys are not the whole preset.
        let mut p = Profile::default();
        p.macros
            .extend(NUMPAD[..5].iter().map(|(k, c)| yours(k, c)));
        assert_eq!(retag_returned_macros(&mut p, &numpad()), 0);
        assert!(p.macros.iter().all(|m| m.preset.is_none()));

        // A command you changed is yours too.
        let mut p = Profile::default();
        p.macros.extend(NUMPAD.iter().map(|(k, c)| yours(k, c)));
        p.macros[0].command = "north".into();
        assert_eq!(retag_returned_macros(&mut p, &numpad()), 0);
    }

    #[test]
    fn a_preset_macro_on_your_key_is_held_until_your_macro_moves_or_goes() {
        let mut p = Profile::default();
        p.macros.push(yours("Numpad3", "rec"));
        install_preset_macros(&mut p, numpad()).unwrap();
        let [on, off] = on_and_off(&p);
        assert_eq!(on[..2], [("Numpad3", "rec"), ("Numpad8", "n")]);
        assert_eq!(on.len(), 6);
        assert_eq!(off, [("Numpad3", "d")]);

        // Yours keeps the key while it is off.
        set_macro(&mut p, "Numpad3", "rec", None, Some(false), None).unwrap();
        let [_, off] = on_and_off(&p);
        assert_eq!(off, [("Numpad3", "rec"), ("Numpad3", "d")]);

        // Settings moves yours to Numpad8 as an unbind, then a bind. d
        // takes Numpad3 back and n is held off.
        delete_macro(&mut p, "Numpad3");
        set_macro(&mut p, "Numpad8", "rec", None, Some(true), None).unwrap();
        let [on, off] = on_and_off(&p);
        assert_eq!(on[4..], [("Numpad3", "d"), ("Numpad8", "rec")]);
        assert_eq!(on.len(), 6);
        assert_eq!(off, [("Numpad8", "n")]);

        // Once yours goes, every key of the preset sends.
        delete_macro(&mut p, "Numpad8");
        let [on, off] = on_and_off(&p);
        assert_eq!(on, NUMPAD);
        assert!(off.is_empty(), "{off:?}");
    }

    #[test]
    fn removing_a_preset_takes_only_the_macros_it_added() {
        let mut p = Profile::default();
        p.macros.push(yours("Numpad3", "rec"));
        install_preset_macros(&mut p, numpad()).unwrap();
        let other = Macro {
            preset: Some("later_preset".into()),
            ..yours("F5", "score")
        };
        install_preset_macros(&mut p, vec![other.clone()]).unwrap();
        assert_eq!(remove_preset_macros(&mut p, "numpad_movement"), 6);
        assert_eq!(p.macros, [yours("Numpad3", "rec"), other]);
        assert_eq!(remove_preset_macros(&mut p, "numpad_movement"), 0);
    }

    /// The cases in fixtures/macros/kept-keys.json, which the page reads
    /// too.
    #[derive(serde::Deserialize)]
    struct KeptKeys {
        preset: String,
        cases: Vec<KeptCase>,
    }

    #[derive(serde::Deserialize)]
    struct KeptCase {
        about: String,
        /// The macro groups the hold leaves out, as in loadout mode.
        #[serde(default)]
        off: BTreeSet<String>,
        macros: Vec<Macro>,
        kept: Vec<String>,
    }

    #[test]
    fn the_hold_holds_off_the_keys_the_page_says_your_macros_keep() {
        // keysYourMacrosKeep in src/automation/automationRecords.ts names
        // the same keys for the Presets card, and the Macros page reads
        // the preset macros this leaves off, so the three agree here.
        let file: KeptKeys =
            serde_json::from_str(include_str!("../../../fixtures/macros/kept-keys.json")).unwrap();
        for case in file.cases {
            let mut held = case.macros.clone();
            hold_taken_keys(&mut held, &case.off);
            assert_eq!(held, case.macros, "{}", case.about);
            let off: Vec<&str> = held
                .iter()
                .filter(|m| m.preset.as_deref() == Some(file.preset.as_str()) && !m.enabled)
                .map(|m| m.key.as_str())
                .collect();
            assert_eq!(off, case.kept, "{}", case.about);
        }
    }

    /// A macro as (key, command, group, on, preset).
    type MacroRow<'a> = (&'a str, &'a str, Option<&'a str>, bool, Option<&'a str>);

    fn macro_rows(p: &Profile) -> Vec<MacroRow<'_>> {
        p.macros
            .iter()
            .map(|m| {
                let preset = m.preset.as_deref();
                (&*m.key, &*m.command, m.group.as_deref(), m.enabled, preset)
            })
            .collect()
    }

    #[test]
    fn your_macro_and_a_preset_macro_on_one_key_never_overwrite_each_other() {
        let numpad = Some("numpad_movement");
        let mut p = Profile::default();
        p.macros.push(Macro {
            key: "Numpad3".into(),
            command: "d".into(),
            group: None,
            enabled: true,
            preset: Some("numpad_movement".into()),
        });
        // Yours joins beside the preset's d, which is held off.
        set_macro(&mut p, "Numpad3", "rec", None, None, None).unwrap();
        assert_eq!(
            macro_rows(&p),
            [
                ("Numpad3", "d", None, false, numpad),
                ("Numpad3", "rec", None, true, None)
            ]
        );
        // The preset's takes only its group from you.
        let travel = Some("travel".to_string());
        set_macro(&mut p, "Numpad3", "rest", travel, Some(true), numpad).unwrap();
        assert_eq!(
            macro_rows(&p)[0],
            ("Numpad3", "d", Some("travel"), false, numpad)
        );
        assert_eq!(
            set_macro(&mut p, "Numpad5", "look", None, None, numpad),
            Err("That preset has no macro on Numpad5 now.".to_string())
        );
        // An import and your edits change yours alone, and yours keeps
        // the key while it is off.
        let imported = Macro {
            key: "Numpad3".into(),
            command: "recite".into(),
            group: None,
            enabled: true,
            preset: None,
        };
        import_macros(&mut p, &[imported]);
        set_macro(&mut p, "Numpad3", "recite", None, Some(false), None).unwrap();
        assert_eq!(
            macro_rows(&p),
            [
                ("Numpad3", "d", Some("travel"), false, numpad),
                ("Numpad3", "recite", None, false, None)
            ]
        );
        // Delete takes yours, and the preset's takes the key back.
        delete_macro(&mut p, "Numpad3");
        delete_macro(&mut p, "Numpad3");
        assert_eq!(
            macro_rows(&p),
            [("Numpad3", "d", Some("travel"), true, numpad)]
        );
    }

    /// A trigger of the preset `preset` in `group`.
    fn preset_trigger(preset: &str, name: &str, group: Option<&str>) -> Trigger {
        use vosh_automation::trigger::TriggerAction;
        Trigger {
            preset: Some(preset.into()),
            group: group.map(str::to_string),
            ..Trigger::new(
                name,
                "sends your SECONDARY weapon flying",
                TriggerAction::Gag,
            )
        }
    }

    fn names(p: &Profile) -> Vec<String> {
        p.triggers.list().into_iter().map(|t| t.name).collect()
    }

    #[test]
    fn a_trigger_a_fix_renamed_leaves_the_store_at_install() {
        let mut p = Profile::default();
        let buff = "disarm_buff_fade";
        let stored = [
            preset_trigger(buff, "disarm.secondary", None),
            preset_trigger(buff, "buff.sanctuary", None),
            preset_trigger("terror_events", "terror.flee", None),
            Trigger {
                preset: None,
                ..preset_trigger(buff, "my.disarm", None)
            },
        ];
        for t in stored {
            p.triggers.set(t).unwrap();
        }
        let built = vec![
            preset_trigger(buff, "disarm.offhand", None),
            preset_trigger(buff, "buff.sanctuary", None),
        ];
        assert_eq!(
            install_preset_triggers(&mut p, built),
            Ok(vec!["disarm.secondary".to_string()])
        );
        let mut now = names(&p);
        now.sort();
        assert_eq!(
            now,
            [
                "buff.sanctuary",
                "disarm.offhand",
                "my.disarm",
                "terror.flee"
            ],
            "another preset and yours stay"
        );
    }

    #[test]
    fn a_built_trigger_with_no_group_keeps_the_stored_group_and_one_with_a_group_takes_it() {
        let mut p = Profile::default();
        let buff = "disarm_buff_fade";
        for name in ["disarm.secondary", "buff.sanctuary"] {
            p.triggers
                .set(preset_trigger(buff, name, Some("combat")))
                .unwrap();
        }
        let built = vec![
            preset_trigger(buff, "disarm.secondary", None),
            preset_trigger(buff, "buff.sanctuary", Some("buffs")),
        ];
        assert_eq!(install_preset_triggers(&mut p, built), Ok(Vec::new()));
        let group = |name| p.triggers.get(name).and_then(|t| t.group.clone());
        assert_eq!(group("disarm.secondary").as_deref(), Some("combat"));
        assert_eq!(group("buff.sanctuary").as_deref(), Some("buffs"));
    }

    fn switch(id: &str, on: bool) -> PresetSwitch {
        PresetSwitch { id: id.into(), on }
    }

    #[test]
    fn a_switch_turns_its_preset_alone_and_keeps_ids_this_build_does_not_know() {
        let stored = presets(&["sent_tells", "later_preset", "potion_labels"]);
        let switches = [switch("potion_labels", false), switch("herb_labels", true)];
        assert_eq!(
            switch_presets(&stored, &switches),
            ["sent_tells", "later_preset", "herb_labels"]
        );
        let on_again = [switch("sent_tells", true)];
        assert_eq!(switch_presets(&stored, &on_again), stored);
    }

    #[test]
    fn a_switch_starts_from_the_defaults_and_stores_none_when_every_preset_is_off() {
        let mut list = switch_presets(&[], &[switch("herb_labels", false)]);
        let mut defaults = presets(PRESETS_ON_BY_DEFAULT);
        defaults.retain(|id| id != "herb_labels");
        assert_eq!(list, defaults);
        let off: Vec<PresetSwitch> = list.iter().map(|id| switch(id, false)).collect();
        list = switch_presets(&list, &off);
        assert_eq!(list, [PRESETS_OFF]);
        assert_eq!(
            switch_presets(&list, &[switch("sent_tells", true)]),
            ["sent_tells"]
        );
    }
}

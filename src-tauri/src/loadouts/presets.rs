//! Which trigger presets are on in loadout mode. The preset triggers
//! live in the catalog every character shares, so the catalog owns the
//! list too, and takes it from the profile files the first time.

use std::collections::BTreeSet;

use super::catalog::GlobalCatalog;
use crate::profile::file::ProfileConfig;
use crate::profile::live::Profile;
use crate::profile::set::ProfileSet;

/// What `ui.enabled_presets` holds when you turned every preset off. An
/// empty list means the defaults. Mirrors `PRESETS_OFF_MARKER` in
/// src/lib/automationRecords.ts, and a test here reads that line.
pub(crate) const PRESETS_OFF: &str = "none";

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
    for entry in set.list() {
        let path = set.profile_path(&entry.name);
        if !path.exists() {
            continue;
        }
        match ProfileConfig::load(&path) {
            Ok(config) => found.lists.push(config.ui.enabled_presets),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    path = %path.display(),
                    "profile file unreadable; its enabled presets are unknown",
                );
                found.unread.push(entry.name.clone());
            }
        }
    }
    found
}

/// Every preset that is on in any of `lists`, in the `enabled_presets`
/// shape. An empty list means the defaults, and every preset in the
/// library is on by default (a test in this module checks
/// src/lib/presets.ts), so the defaults hold every preset a list can
/// name and the union is the defaults. A list that turned every preset
/// off adds none.
fn presets_on_in_any(lists: &[Vec<String>]) -> Vec<String> {
    if lists.iter().any(Vec::is_empty) {
        return Vec::new();
    }
    let on: BTreeSet<&str> = lists
        .iter()
        .flatten()
        .map(String::as_str)
        .filter(|id| *id != PRESETS_OFF)
        .collect();
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;

    use crate::loadouts::catalog::{load_global_catalog, save_global_catalog};

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
    fn every_preset_in_the_library_is_on_by_default() {
        // `presets_on_in_any` takes the defaults as holding every preset
        // a profile list can name. A preset that is off by default needs
        // its id listed there instead.
        let library = include_str!("../../../src/lib/presets.ts");
        assert!(library.contains("defaultEnabled: true"));
        assert!(!library.contains("defaultEnabled: false"));
    }

    #[test]
    fn presets_off_is_the_marker_the_page_stores() {
        // Settings stores PRESETS_OFF_MARKER when you turn every preset
        // off, and launch reads it back here as PRESETS_OFF.
        let records = include_str!("../../../src/lib/automationRecords.ts");
        let marker = regex::Regex::new(r"export const PRESETS_OFF_MARKER = '([^']*)';")
            .unwrap()
            .captures(records)
            .expect("automationRecords.ts declares PRESETS_OFF_MARKER");
        assert_eq!(&marker[1], PRESETS_OFF);
        // No preset may take the marker as its id.
        let library = include_str!("../../../src/lib/presets.ts");
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
}

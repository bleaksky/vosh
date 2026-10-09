//! The two presets that came on once for every profile.
//!
//! An `enabled_presets` list that names presets holds the presets that
//! were on when you last saved it. Tells you send and Room, time and
//! weather colors came after many such lists were saved, so launch adds
//! each once to every such list, and in loadout mode to the list the
//! shared catalog keeps, then records the step in `profiles.toml`. A
//! preset you turn off afterwards stays off.
//!
//! [`ROLLOUTS`] takes no new entries. A preset added later ships off and
//! stays out of [`PRESETS_ON_BY_DEFAULT`], so no list turns it on until
//! you do. The doc on [`ROLLOUTS`] says why.
//!
//! - An empty list means the defaults, which hold both presets already.
//! - A list that turned every preset off keeps them all off.
//! - The shared catalog takes it too, in loadout mode, when it holds a
//!   list.
//! - Nothing is written while a profile file or the catalog does not
//!   read, and the step stays unrecorded, so it runs again at the next
//!   launch.
//!
//! Launch runs it before any profile loads, under the persist lock, so
//! the live profile reads the files as the step left them.
//!
//! [`PRESETS_ON_BY_DEFAULT`]: crate::loadouts::presets::PRESETS_ON_BY_DEFAULT

use std::path::Path;

use crate::loadouts::catalog::{load_global_catalog, loadout_mode_on, save_global_catalog};
use crate::loadouts::presets::PRESETS_OFF;
use crate::profile::set::ProfileSet;

/// Each preset that comes on once, with the id the step is recorded
/// under in `profiles.toml`.
///
/// This list takes no new entries. A preset added later ships off and is
/// not in [`PRESETS_ON_BY_DEFAULT`], so an empty list leaves it off too,
/// and players who have Vosh today learn of it through the release notes.
/// Once you turn one preset on, your list names presets, so a rollout
/// would turn on a preset you never chose. The two steps here ran before
/// that rule and stay, so a launch that has not run them yet still does.
/// Both presets are among the defaults, which is why [`add_preset`]
/// leaves an empty list alone.
///
/// [`PRESETS_ON_BY_DEFAULT`]: crate::loadouts::presets::PRESETS_ON_BY_DEFAULT
pub(crate) const ROLLOUTS: &[(&str, &str)] = &[
    ("preset-sent-tells-on", "sent_tells"),
    ("preset-room-and-time-on", "room_and_time"),
];

/// Run every step in [`ROLLOUTS`] that has not run over `set`, the
/// profile set launch read from the app data folder `app_data`.
pub(crate) fn run(set: &mut ProfileSet, app_data: &Path) {
    for &(id, preset) in ROLLOUTS {
        if set.migrated(id) {
            continue;
        }
        if let Err(e) = roll_out(set, app_data, preset) {
            tracing::error!(error = %e, preset, "preset rollout stopped, it runs again at the next launch");
            continue;
        }
        if let Err(e) = set.record_migration(id) {
            tracing::error!(error = %e, preset, "preset rollout: could not record it in profiles.toml");
        }
    }
}

/// Add `preset` to every list in the profile files of `set` and in the
/// shared catalog that names presets and lacks it. Every file reads
/// before any is written.
fn roll_out(set: &ProfileSet, app_data: &Path, preset: &str) -> Result<(), String> {
    let mut files = Vec::new();
    for stored in set.read_all() {
        match stored.file {
            Some(Ok(file)) => files.push((stored.path, file.config)),
            Some(Err(e)) => return Err(format!("{}: {e}", stored.path.display())),
            None => {}
        }
    }
    let mut catalog = if loadout_mode_on(app_data) {
        Some(load_global_catalog(app_data).map_err(|e| e.to_string())?)
    } else {
        None
    };
    for (path, config) in &mut files {
        if add_preset(&mut config.ui.enabled_presets, preset) {
            config
                .save(path)
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    if let Some(catalog) = catalog.as_mut() {
        if catalog
            .enabled_presets
            .as_mut()
            .is_some_and(|list| add_preset(list, preset))
        {
            save_global_catalog(app_data, catalog).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Turn `preset` on in `list`, an `enabled_presets` list, unless the list
/// means the defaults, turned every preset off, or has it already. True
/// when the list changed.
fn add_preset(list: &mut Vec<String>, preset: &str) -> bool {
    if list.is_empty() || list.iter().any(|id| id == PRESETS_OFF || id == preset) {
        return false;
    }
    list.push(preset.to_string());
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::loadouts::presets::PRESETS_ON_BY_DEFAULT;
    use crate::profile::file::ProfileConfig;

    const INDEX: &str = r#"active = "default"

[[profile]]
name = "default"

[[profile]]
name = "Healer"

[[profile]]
name = "Ranger"

[[profile]]
name = "Quiet"
"#;

    fn file(root: &Path, name: &str) -> PathBuf {
        root.join("profiles").join(format!("{name}.toml"))
    }

    fn write_list(root: &Path, name: &str, list: &[&str]) {
        let mut config = ProfileConfig::fresh();
        config.ui.enabled_presets = strings(list);
        config.save(&file(root, name)).unwrap();
    }

    fn list(root: &Path, name: &str) -> Vec<String> {
        ProfileConfig::load(&file(root, name))
            .unwrap()
            .ui
            .enabled_presets
    }

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    /// default saved its presets, Healer turned every one off, Ranger
    /// never saved them, and Quiet has no file.
    fn folder() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("profiles")).unwrap();
        std::fs::write(root.join("profiles.toml"), INDEX).unwrap();
        write_list(root, "default", &["healing_basics", "potion_labels"]);
        write_list(root, "Healer", &["none"]);
        write_list(root, "Ranger", &[]);
        dir
    }

    /// Read the profile set in `root` and run every rollout over it, as
    /// launch does.
    fn read_and_run(root: &Path) {
        let mut set = ProfileSet::load_or_migrate(root.to_path_buf()).unwrap();
        run(&mut set, root);
    }

    /// How many steps in [`ROLLOUTS`] are recorded.
    fn recorded(root: &Path) -> usize {
        let set = ProfileSet::load_or_migrate(root.to_path_buf()).unwrap();
        ROLLOUTS.iter().filter(|(id, _)| set.migrated(id)).count()
    }

    #[test]
    fn a_saved_list_takes_the_new_preset_once() {
        let dir = folder();
        let root = dir.path();
        read_and_run(root);
        assert_eq!(
            list(root, "default"),
            strings(&[
                "healing_basics",
                "potion_labels",
                "sent_tells",
                "room_and_time"
            ])
        );
        // Every preset off stays off, and the defaults hold it already.
        assert_eq!(list(root, "Healer"), strings(&["none"]));
        let leftover = &list(root, "Ranger");
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(!file(root, "Quiet").exists());
        assert_eq!(recorded(root), ROLLOUTS.len());
    }

    #[test]
    fn a_preset_you_turn_off_afterwards_stays_off() {
        let dir = folder();
        let root = dir.path();
        read_and_run(root);
        write_list(root, "default", &["healing_basics"]);
        read_and_run(root);
        assert_eq!(list(root, "default"), strings(&["healing_basics"]));
    }

    #[test]
    fn the_shared_catalog_takes_it_in_loadout_mode() {
        let dir = folder();
        let root = dir.path();
        let mut catalog = load_global_catalog(root).unwrap();
        catalog.enabled_presets = Some(strings(&["herb_labels"]));
        save_global_catalog(root, &catalog).unwrap();
        read_and_run(root);
        assert_eq!(
            load_global_catalog(root).unwrap().enabled_presets,
            Some(strings(&["herb_labels", "sent_tells", "room_and_time"]))
        );
    }

    #[test]
    fn a_catalog_without_a_list_keeps_none() {
        let dir = folder();
        let root = dir.path();
        save_global_catalog(root, &load_global_catalog(root).unwrap()).unwrap();
        read_and_run(root);
        assert_eq!(load_global_catalog(root).unwrap().enabled_presets, None);
    }

    #[test]
    fn a_file_that_does_not_read_holds_every_write() {
        let dir = folder();
        let root = dir.path();
        std::fs::write(file(root, "Ranger"), "[ui\nbroken").unwrap();
        read_and_run(root);
        assert_eq!(
            list(root, "default"),
            strings(&["healing_basics", "potion_labels"])
        );
        assert_eq!(recorded(root), 0);
        // Once it reads, the next launch runs the step.
        write_list(root, "Ranger", &[]);
        read_and_run(root);
        assert_eq!(
            list(root, "default"),
            strings(&[
                "healing_basics",
                "potion_labels",
                "sent_tells",
                "room_and_time"
            ])
        );
        assert_eq!(recorded(root), ROLLOUTS.len());
    }

    #[tokio::test]
    async fn a_fresh_install_keeps_every_preset_off() {
        // Every launch upgrade, as launch runs them on a new install.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut set = ProfileSet::load_or_migrate(root.to_path_buf()).unwrap();
        crate::disk::upgrades::run(&mut set, root, true).await;
        assert_eq!(list(root, "default"), strings(&[PRESETS_OFF]));
        assert_eq!(recorded(root), ROLLOUTS.len());
    }

    #[test]
    fn every_rollout_names_a_library_preset() {
        let library = include_str!("../../../../src/automation/presets.ts");
        for (_, preset) in ROLLOUTS {
            assert!(library.contains(&format!("id: '{preset}',")), "{preset}");
        }
    }

    #[test]
    fn rollouts_take_no_new_entries() {
        // A preset added later ships off with no rollout. See ROLLOUTS.
        assert_eq!(
            ROLLOUTS,
            [
                ("preset-sent-tells-on", "sent_tells"),
                ("preset-room-and-time-on", "room_and_time"),
            ]
        );
        // An empty list holds each one already, so add_preset may skip it.
        for (_, preset) in ROLLOUTS {
            assert!(PRESETS_ON_BY_DEFAULT.contains(preset), "{preset}");
        }
    }
}

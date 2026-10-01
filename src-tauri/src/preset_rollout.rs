//! Presets a build adds that come on for every profile, once.
//!
//! An `enabled_presets` list that names presets holds the presets that
//! were on when you last saved it. A preset a later build adds is in none
//! of them, so it would stay off for every profile that ever saved its
//! presets, and in loadout mode for every character, since the shared
//! catalog keeps its own list. Each preset in [`ROLLOUTS`] comes on by
//! default, so launch adds it once to each such list and records the step
//! in `profiles.toml`. A preset you turn off afterwards stays off.
//!
//! - An empty list means the defaults, which hold the preset already.
//! - A list that turned every preset off keeps them all off.
//! - The shared catalog takes it too, in loadout mode, when it holds a
//!   list.
//! - Nothing is written while a profile file or the catalog does not
//!   read, and the step stays unrecorded, so it runs again at the next
//!   launch.
//!
//! Launch runs it before any profile loads, under the persist lock, so
//! the live profile reads the files as the step left them.

use std::path::Path;

use crate::loadout_store::{self, PRESETS_OFF};
use crate::profile_config::ProfileConfig;
use crate::profile_set::ProfileSet;

/// Each preset that comes on once, with the id the step is recorded
/// under in `profiles.toml`.
pub(crate) const ROLLOUTS: &[(&str, &str)] = &[("preset-sent-tells-on", "sent_tells")];

/// Run every step in [`ROLLOUTS`] that has not run over the app data
/// folder `app_data`.
pub(crate) fn run(app_data: &Path) {
    let mut set = match ProfileSet::load_or_migrate(app_data.to_path_buf()) {
        Ok(set) => set,
        Err(e) => {
            tracing::error!(error = %e, "preset rollout: profiles.toml does not read");
            return;
        }
    };
    for &(id, preset) in ROLLOUTS {
        if set.migrated(id) {
            continue;
        }
        if let Err(e) = roll_out(&set, app_data, preset) {
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
    for entry in set.list() {
        let path = set.profile_path(&entry.name);
        if !path.exists() {
            continue;
        }
        let config = ProfileConfig::load(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        files.push((path, config));
    }
    let mut catalog = if loadout_store::path_b_mode_active(app_data) {
        Some(loadout_store::load_global_catalog(app_data).map_err(|e| e.to_string())?)
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
            loadout_store::save_global_catalog(app_data, catalog).map_err(|e| e.to_string())?;
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
    use crate::loadout_store::{load_global_catalog, save_global_catalog};
    use std::path::PathBuf;

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

    fn recorded(root: &Path) -> bool {
        ProfileSet::load_or_migrate(root.to_path_buf())
            .unwrap()
            .migrated("preset-sent-tells-on")
    }

    #[test]
    fn a_saved_list_takes_the_new_preset_once() {
        let dir = folder();
        let root = dir.path();
        run(root);
        assert_eq!(
            list(root, "default"),
            strings(&["healing_basics", "potion_labels", "sent_tells"])
        );
        // Every preset off stays off, and the defaults hold it already.
        assert_eq!(list(root, "Healer"), strings(&["none"]));
        assert!(list(root, "Ranger").is_empty());
        assert!(!file(root, "Quiet").exists());
        assert!(recorded(root));
    }

    #[test]
    fn a_preset_you_turn_off_afterwards_stays_off() {
        let dir = folder();
        let root = dir.path();
        run(root);
        write_list(root, "default", &["healing_basics"]);
        run(root);
        assert_eq!(list(root, "default"), strings(&["healing_basics"]));
    }

    #[test]
    fn the_shared_catalog_takes_it_in_loadout_mode() {
        let dir = folder();
        let root = dir.path();
        let mut catalog = load_global_catalog(root).unwrap();
        catalog.enabled_presets = Some(strings(&["herb_labels"]));
        save_global_catalog(root, &catalog).unwrap();
        run(root);
        assert_eq!(
            load_global_catalog(root).unwrap().enabled_presets,
            Some(strings(&["herb_labels", "sent_tells"]))
        );
    }

    #[test]
    fn a_catalog_without_a_list_keeps_none() {
        let dir = folder();
        let root = dir.path();
        save_global_catalog(root, &load_global_catalog(root).unwrap()).unwrap();
        run(root);
        assert_eq!(load_global_catalog(root).unwrap().enabled_presets, None);
    }

    #[test]
    fn a_file_that_does_not_read_holds_every_write() {
        let dir = folder();
        let root = dir.path();
        std::fs::write(file(root, "Ranger"), "[ui\nbroken").unwrap();
        run(root);
        assert_eq!(
            list(root, "default"),
            strings(&["healing_basics", "potion_labels"])
        );
        assert!(!recorded(root));
        // Once it reads, the next launch runs the step.
        write_list(root, "Ranger", &[]);
        run(root);
        assert_eq!(
            list(root, "default"),
            strings(&["healing_basics", "potion_labels", "sent_tells"])
        );
        assert!(recorded(root));
    }

    #[test]
    fn every_rollout_names_a_library_preset() {
        let library = include_str!("../../src/lib/presets.ts");
        for (_, preset) in ROLLOUTS {
            assert!(library.contains(&format!("id: '{preset}',")), "{preset}");
        }
    }
}

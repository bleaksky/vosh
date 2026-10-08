//! The tests of the loadouts.rs root, and the helpers that more than one
//! loadouts test module shares.

use std::fs;
use std::path::PathBuf;

use vosh_automation::alias::Alias;

use super::catalog::{
    load_global_catalog, save_global_catalog, GlobalCatalog, UNREAD_CATALOG_NOTICE,
};
use super::load_at_launch;
use super::set::{
    follow_profile_name, load_loadout_set, save_loadout_set, Loadout, LoadoutSet, Stack,
    UNREAD_LOADOUTS_NOTICE,
};
use crate::disk::paths::{catalog_path, loadouts_path};

pub(super) fn tmpdir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "vosh-loadout-store-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_loadout_mode_file_that_does_not_read_holds_both_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut catalog = GlobalCatalog::default();
    catalog.aliases.push(Alias::new("kk", "kick %1"));
    save_global_catalog(dir.path(), &catalog).unwrap();
    fs::write(loadouts_path(dir.path()), "active = = [\n").unwrap();
    let catalog_text = fs::read_to_string(catalog_path(dir.path())).unwrap();

    assert_eq!(
        load_at_launch(dir.path()).unwrap_err(),
        [UNREAD_LOADOUTS_NOTICE]
    );
    // The session runs without your shared items, so a save from it
    // would write an empty catalog over them.
    assert!(save_global_catalog(dir.path(), &GlobalCatalog::default()).is_err());
    assert!(save_loadout_set(dir.path(), &LoadoutSet::default()).is_err());
    assert_eq!(
        fs::read_to_string(catalog_path(dir.path())).unwrap(),
        catalog_text
    );
    assert_eq!(
        fs::read_to_string(loadouts_path(dir.path())).unwrap(),
        "active = = [\n"
    );
}

#[test]
fn a_catalog_that_does_not_read_is_named_in_the_notice() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(catalog_path(dir.path()), "aliases = = [\n").unwrap();
    assert_eq!(
        load_at_launch(dir.path()).unwrap_err(),
        [UNREAD_CATALOG_NOTICE]
    );
    assert!(save_global_catalog(dir.path(), &GlobalCatalog::default()).is_err());
    // Nothing wrote loadouts.toml, and nothing can while the catalog
    // is held.
    assert!(save_loadout_set(dir.path(), &LoadoutSet::default()).is_err());
    assert!(!loadouts_path(dir.path()).exists());
}

#[test]
fn load_missing_files_yields_defaults() {
    let dir = tmpdir();
    let catalog = load_global_catalog(&dir).unwrap();
    let set = load_loadout_set(&dir).unwrap();
    let leftover = &catalog.aliases;
    assert!(leftover.is_empty(), "{leftover:?}");
    let leftover = &catalog.triggers;
    assert!(leftover.is_empty(), "{leftover:?}");
    let leftover = &catalog.macros;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert!(set.loadouts.is_empty());
    let leftover = &set.active;
    assert!(leftover.is_empty(), "{leftover:?}");
    fs::remove_dir_all(&dir).ok();
}

/// A stack of `active`, dormant when it holds none.
fn stack(active: &[&str]) -> Stack {
    Stack {
        active: active.iter().map(|name| (*name).to_string()).collect(),
        dormant: active.is_empty(),
    }
}

#[test]
fn with_one_profile_open_a_stack_change_writes_the_top_level() {
    let mut set = LoadoutSet::default();
    set.set_stack(Some("Default"), &["Default".into()], stack(&["Melee"]));
    assert_eq!(set.active, ["Melee"]);
    assert!(set.profiles.is_empty());
    set.set_stack(Some("Default"), &["Default".into()], stack(&[]));
    assert!(set.dormant);
    assert!(set.profiles.is_empty());
}

#[test]
fn a_profile_keeps_a_stack_of_its_own_once_another_open_profile_reads_the_top_level() {
    let open = ["Default".to_string(), "Healer".to_string()];
    let mut set = LoadoutSet {
        active: vec!["Melee".into()],
        ..LoadoutSet::default()
    };
    set.set_stack(Some("Healer"), &open, stack(&["Heals"]));
    assert_eq!(set.active, ["Melee"]);
    assert_eq!(set.profiles["Healer"], stack(&["Heals"]));
    assert_eq!(set.for_profile(Some("Healer")).active, ["Heals"]);
    assert_eq!(set.for_profile(Some("Default")).active, ["Melee"]);
    // Default is the one open profile left on the top level, so its
    // change lands there.
    set.set_stack(Some("Default"), &open, stack(&[]));
    assert!(set.dormant);
    assert!(!set.for_profile(Some("Healer")).dormant);
    // Healer changes its own stack from then on, open alone or not.
    set.set_stack(Some("Healer"), &["Healer".into()], stack(&["Melee"]));
    assert_eq!(set.profiles["Healer"], stack(&["Melee"]));
    let leftover = &set.active;
    assert!(leftover.is_empty(), "{leftover:?}");
}

#[test]
fn the_stacks_of_the_profiles_save_under_their_names_after_the_top_level() {
    let dir = tempfile::tempdir().unwrap();
    let set = LoadoutSet {
        active: vec!["Melee".into()],
        loadouts: vec![Loadout::empty("Melee")],
        profiles: [("Healer".to_string(), stack(&[]))].into(),
        ..LoadoutSet::default()
    };
    save_loadout_set(dir.path(), &set).unwrap();
    let text = fs::read_to_string(loadouts_path(dir.path())).unwrap();
    // An older build reads the top-level stack as the one stack and
    // passes over the table it does not know, so a rollback still loads
    // the file.
    assert_eq!(
        text,
        "active = [\"Melee\"]\ndormant = false\n\n[[loadouts]]\nname = \"Melee\"\n\
         enabled_groups = []\n\n[profiles.Healer]\nactive = []\ndormant = true\n"
    );
    let loaded = load_loadout_set(dir.path()).unwrap();
    assert_eq!(loaded.profiles, set.profiles);
}

#[tokio::test]
async fn a_rename_carries_the_stack_of_a_profile_and_a_delete_drops_it() {
    let dir = tempfile::tempdir().unwrap();
    let state = std::sync::Arc::new(crate::app::state::AppState::default());
    state.app_data.set(dir.path().to_path_buf()).unwrap();
    *state.loadout_set.lock().await = Some(LoadoutSet {
        profiles: [("Healer".to_string(), stack(&["Heals"]))].into(),
        ..LoadoutSet::default()
    });
    follow_profile_name(&state, "Healer", Some("Cleric")).await;
    let saved = load_loadout_set(dir.path()).unwrap();
    assert_eq!(
        saved.profiles,
        [("Cleric".to_string(), stack(&["Heals"]))].into()
    );
    follow_profile_name(&state, "Cleric", None).await;
    assert!(load_loadout_set(dir.path()).unwrap().profiles.is_empty());
}

//! The tests of the loadouts.rs root, and the helpers that more than one
//! loadouts test module shares.

use std::fs;
use std::path::PathBuf;

use vosh_automation::alias::Alias;

use super::catalog::{
    load_global_catalog, save_global_catalog, GlobalCatalog, UNREAD_CATALOG_NOTICE,
};
use super::load_at_launch;
use super::set::{load_loadout_set, save_loadout_set, LoadoutSet, UNREAD_LOADOUTS_NOTICE};
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

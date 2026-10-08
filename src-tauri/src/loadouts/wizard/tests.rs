//! The shared catalog wizard and the loadout mode files it writes.

use std::sync::atomic::Ordering;

use super::apply::{analyze_migration, apply_migration, WIZARD_WRITES_BEFORE_A_CRASH};
use crate::app::state::{AppState, SharedState};
use crate::disk::paths::{catalog_path, loadouts_path};
use crate::disk::save::tests::{launch_state, persist, read, UNREADABLE};
use crate::disk::save::PERSIST_LOCK;
use crate::loadouts::catalog::{save_global_catalog, GlobalCatalog};
use crate::loadouts::load_at_launch;
use crate::loadouts::set::{save_loadout_set, Loadout, LoadoutSet};
use crate::profile::file::ProfileConfig;
use crate::profile::set::ProfileSet;
use crate::profile::tests::james_like_set;

/// Every preset in the library src/automation/presets.ts holds.
const LIBRARY: &[&str] = &[
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
    "numpad_movement",
];

#[test]
fn the_library_here_is_the_one_presets_ts_holds() {
    let library = include_str!("../../../../src/automation/presets.ts");
    // Each preset opens with its id, four spaces in, in the order
    // the page lists them.
    let ids: Vec<&str> = library
        .split("\n    id: '")
        .skip(1)
        .map(|rest| &rest[..rest.find('\'').expect("the id closes")])
        .collect();
    assert_eq!(ids, LIBRARY);
}

const LOADOUT_SESSION_REFUSAL: &str =
    "This session runs on a shared catalog, and Vosh saves it to catalog.toml again \
     when you quit, so Vosh will not build another one now. To build a new catalog, quit \
     Vosh first, then follow the steps for a new catalog under Set up loadouts in the \
     help.";

const HELD: &str = "Vosh could not read your shared catalog at launch, so it will not \
                    build a new one over it. Fix catalog.toml or loadouts.toml and \
                    restart Vosh.";

/// Save `name`'s file with one alias, the way a profile in per
/// profile mode holds its own items.
fn write_alias(set: &ProfileSet, name: &str, alias: &str) {
    let mut config = ProfileConfig::default();
    config
        .aliases
        .push(vosh_automation::alias::Alias::new(alias, "kick %1"));
    config.save(&set.profile_path(name)).unwrap();
}

/// A catalog with your shared alias and a loadout per character,
/// and profile files that hold no items, as loadout mode keeps
/// them.
fn loadout_mode(set: &ProfileSet, dir: &std::path::Path) {
    for entry in set.list() {
        ProfileConfig::default()
            .save(&set.profile_path(&entry.name))
            .unwrap();
    }
    let mut catalog = GlobalCatalog::default();
    catalog
        .aliases
        .push(vosh_automation::alias::Alias::new("kk", "kick %1"));
    save_global_catalog(dir, &catalog).unwrap();
    let loadouts = LoadoutSet {
        loadouts: vec![Loadout::empty("default")],
        active: vec!["default".into()],
        dormant: false,
        ..Default::default()
    };
    save_loadout_set(dir, &loadouts).unwrap();
}

async fn refused(state: &SharedState) -> String {
    let analyze = analyze_migration(state, LIBRARY).await.unwrap_err();
    let apply = apply_migration(state, &[], LIBRARY).await.unwrap_err();
    assert_eq!(analyze, apply);
    apply
}

/// Quit and open Vosh again as `name`, the way app/launch.rs launches,
/// with the shared catalog and loadouts when they are on disk. The
/// folder has had every preset rollout already, as one this build
/// opened before has, so each list stays as the test wrote it. The
/// rollouts have tests of their own in `disk/upgrades/presets.rs`.
async fn relaunch_as(dir: &std::path::Path, name: &str) -> SharedState {
    let mut set = ProfileSet::load_or_migrate(dir.to_path_buf()).unwrap();
    for (id, _) in crate::disk::upgrades::presets::ROLLOUTS {
        set.record_migration(id).unwrap();
    }
    set.switch(name).unwrap();
    let state: SharedState = std::sync::Arc::new(AppState::default());
    crate::app::launch::load(&state, dir).await;
    assert!(state.profile_set.lock().await.is_some());
    state
}

fn macro_on(key: &str, command: &str) -> crate::profile::live::Macro {
    crate::profile::live::Macro {
        key: key.into(),
        command: command.into(),
        group: None,
        enabled: true,
        preset: None,
    }
}

fn macro_keys(macros: &[crate::profile::live::Macro]) -> Vec<&str> {
    macros.iter().map(|m| m.key.as_str()).collect()
}

#[tokio::test]
async fn a_loadout_save_leaves_the_macros_to_the_catalog() {
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    loadout_mode(&set, dir.path());
    let state = relaunch_as(dir.path(), crate::profile::set::DEFAULT_PROFILE_NAME).await;
    assert!(state.global_catalog.lock().await.is_some());
    state
        .selected_profile()
        .await
        .macros
        .push(macro_on("f1", "look"));
    persist(&state).await;
    let saved = crate::loadouts::catalog::load_global_catalog(dir.path()).unwrap();
    assert_eq!(macro_keys(&saved.macros), ["f1"]);
    let leftover = &ProfileConfig::load(&set.active_path()).unwrap().macros;
    assert!(leftover.is_empty(), "{leftover:?}");

    // You delete the macro while you play Healer.
    let state = relaunch_as(dir.path(), "Healer").await;
    state.selected_profile().await.macros.clear();
    persist(&state).await;

    // Back on Default, the macro stays deleted.
    let state = relaunch_as(dir.path(), crate::profile::set::DEFAULT_PROFILE_NAME).await;
    let leftover = &state.selected_profile().await.macros;
    assert!(leftover.is_empty(), "{leftover:?}");
}

/// Your edits to the presets move to catalog.toml with the list of
/// presets that are on, leave the profile file, and reach every
/// character (Presets Q2).
#[tokio::test]
async fn a_loadout_save_keeps_the_preset_edits_in_the_catalog() {
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    loadout_mode(&set, dir.path());
    let edits = crate::loadouts::preset_edits::PresetEdits::from([(
        "disarm_buff_fade".to_string(),
        crate::loadouts::preset_edits::PresetEdit {
            colors: std::collections::BTreeMap::from([(
                "line".to_string(),
                crate::loadouts::preset_edits::EditRow {
                    value: "#c3a6ff".into(),
                    was: "fg:178".into(),
                    seen: None,
                },
            )]),
            ..Default::default()
        },
    )]);
    let state = relaunch_as(dir.path(), crate::profile::set::DEFAULT_PROFILE_NAME).await;
    state.selected_profile().await.preset_edits = edits.clone();
    persist(&state).await;
    let saved = crate::loadouts::catalog::load_global_catalog(dir.path()).unwrap();
    assert_eq!(saved.preset_edits, edits);
    let text = read(&set.active_path());
    assert!(!text.contains("preset_edits"), "{text}");

    let state = relaunch_as(dir.path(), "Healer").await;
    assert_eq!(state.selected_profile().await.preset_edits, edits);
}

#[tokio::test]
async fn a_catalog_that_does_not_read_is_held_and_never_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    loadout_mode(&set, dir.path());
    std::fs::write(catalog_path(dir.path()), UNREADABLE).unwrap();
    let loadouts = read(&loadouts_path(dir.path()));

    // Launch tells you and runs on the profile files alone.
    let state = launch_state(dir.path()).await;
    assert_eq!(
        load_at_launch(dir.path()).unwrap_err(),
        [crate::loadouts::catalog::UNREAD_CATALOG_NOTICE]
    );

    // The wizard would build the catalog from profile files that
    // hold none of your shared items.
    assert_eq!(refused(&state).await, HELD);
    persist(&state).await;
    assert!(save_global_catalog(dir.path(), &GlobalCatalog::default()).is_err());
    assert!(save_loadout_set(dir.path(), &LoadoutSet::default()).is_err());

    assert_eq!(read(&catalog_path(dir.path())), UNREADABLE);
    assert_eq!(read(&loadouts_path(dir.path())), loadouts);
    // The profile files stay where they are.
    assert!(set.active_path().exists());
    assert!(!dir.path().join("profiles").join("legacy").exists());
}

#[tokio::test]
async fn a_loadouts_file_that_does_not_read_holds_the_catalog_too() {
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    loadout_mode(&set, dir.path());
    std::fs::write(loadouts_path(dir.path()), UNREADABLE).unwrap();
    let catalog = read(&catalog_path(dir.path()));

    let state = launch_state(dir.path()).await;
    assert_eq!(
        load_at_launch(dir.path()).unwrap_err(),
        [crate::loadouts::set::UNREAD_LOADOUTS_NOTICE]
    );
    assert_eq!(refused(&state).await, HELD);
    assert!(save_global_catalog(dir.path(), &GlobalCatalog::default()).is_err());
    assert_eq!(read(&catalog_path(dir.path())), catalog);
    assert_eq!(read(&loadouts_path(dir.path())), UNREADABLE);
}

#[tokio::test]
async fn the_wizard_never_builds_over_a_catalog_you_already_use() {
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    loadout_mode(&set, dir.path());
    let catalog = read(&catalog_path(dir.path()));
    let state = launch_state(dir.path()).await;
    assert!(load_at_launch(dir.path()).is_ok());

    assert_eq!(
        refused(&state).await,
        "You already have a shared catalog, so Vosh will not build another one over it."
    );
    assert_eq!(read(&catalog_path(dir.path())), catalog);
    assert!(set.active_path().exists());
}

#[tokio::test]
async fn the_wizard_never_runs_in_a_session_that_uses_a_catalog() {
    use crate::disk::paths::legacy_dir;
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    write_alias(&set, "Healer", "hh");
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    apply_migration(&state, &[], LIBRARY).await.unwrap();
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    assert!(state.global_catalog.lock().await.is_some());

    // While Vosh runs on the catalog, you move it, the loadouts,
    // and the backups out of the folder. The profile files hold no
    // items, so a run would build a catalog with none.
    let aside = tempfile::tempdir().unwrap();
    for path in [catalog_path(dir.path()), loadouts_path(dir.path())] {
        std::fs::rename(&path, aside.path().join(path.file_name().unwrap())).unwrap();
    }
    std::fs::rename(legacy_dir(dir.path()), aside.path().join("legacy")).unwrap();
    let healer = read(&set.profile_path("Healer"));

    assert_eq!(refused(&state).await, LOADOUT_SESSION_REFUSAL);
    assert!(!catalog_path(dir.path()).exists());
    assert!(!loadouts_path(dir.path()).exists());
    assert!(!legacy_dir(dir.path()).exists());
    assert_eq!(read(&set.profile_path("Healer")), healer);

    // The refusal used to say to quit and open Vosh again before
    // the wizard. The save at quit writes the catalog and the
    // loadouts back, so the wizard refused again after that.
    persist(&state).await;
    drop(state);
    assert!(catalog_path(dir.path()).exists());
    assert!(loadouts_path(dir.path()).exists());

    // With Vosh closed, you follow the steps for a new catalog in
    // the help, and the new catalog holds the items of the backups.
    let later = tempfile::tempdir().unwrap();
    for path in [catalog_path(dir.path()), loadouts_path(dir.path())] {
        std::fs::rename(&path, later.path().join(path.file_name().unwrap())).unwrap();
    }
    for entry in std::fs::read_dir(aside.path().join("legacy")).unwrap() {
        let entry = entry.unwrap();
        let back = set.profile_path(entry.path().file_stem().unwrap().to_str().unwrap());
        std::fs::copy(entry.path(), back).unwrap();
    }
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    apply_migration(&state, &[], LIBRARY).await.unwrap();
    let state = relaunch_as(dir.path(), "Healer").await;
    assert!(state.global_catalog.lock().await.is_some());
    assert_eq!(items_on(&*state.selected_profile().await), ["alias hh"]);
}

#[tokio::test]
async fn a_session_on_the_catalog_says_so_before_it_names_the_backups() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    write_alias(&set, "Healer", "hh");
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    apply_migration(&state, &[], LIBRARY).await.unwrap();
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    assert_eq!(refused(&state).await, LOADOUT_SESSION_REFUSAL);

    // While Vosh runs on the catalog, you move the catalog out, then
    // the loadouts. The refusals over loadouts.toml and the backups
    // used to answer and said to quit and copy the backups back.
    // The save at quit writes the catalog back, and a backup copied
    // back beside it lays its old items over it.
    let aside = tempfile::tempdir().unwrap();
    for path in [catalog_path(dir.path()), loadouts_path(dir.path())] {
        std::fs::rename(&path, aside.path().join(path.file_name().unwrap())).unwrap();
        assert_eq!(refused(&state).await, LOADOUT_SESSION_REFUSAL);
    }
    persist(&state).await;
    assert!(catalog_path(dir.path()).exists());
    assert!(loadouts_path(dir.path()).exists());
}

#[tokio::test]
async fn the_wizard_never_saves_over_a_loadouts_file_it_did_not_read() {
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    write_alias(&set, "Healer", "hh");
    save_loadout_set(dir.path(), &LoadoutSet::default()).unwrap();
    let loadouts = read(&loadouts_path(dir.path()));
    let state = launch_state(dir.path()).await;

    assert_eq!(
        refused(&state).await,
        "Vosh found loadouts.toml from an earlier shared catalog and will not save over \
         it. Move the file out of the Vosh folder to build a new catalog."
    );
    assert!(!catalog_path(dir.path()).exists());
    assert_eq!(read(&loadouts_path(dir.path())), loadouts);
}

#[tokio::test]
async fn the_wizard_builds_a_catalog_once() {
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    write_alias(&set, "Healer", "hh");
    let state = launch_state(dir.path()).await;

    let plan = analyze_migration(&state, LIBRARY).await.unwrap();
    assert_eq!(plan.auto_resolved.aliases.len(), 1);
    apply_migration(&state, &[], LIBRARY).await.unwrap();
    // Every save waits for the relaunch, and `#profile` only echoes.
    assert!(state.relaunch_pending.load(Ordering::Acquire));
    assert!(state.loadout_mode.load(Ordering::Acquire));
    let (catalog, _) = load_at_launch(dir.path()).unwrap();
    assert_eq!(catalog.aliases[0].name, "hh");
    // A run that wrote every file takes its journal out.
    assert!(!crate::disk::paths::journal_path(dir.path()).exists());
    // The alias left the profile file, and the copy in legacy
    // still holds it.
    let healer = set.profile_path("Healer");
    let leftover = &ProfileConfig::load(&healer).unwrap().aliases;
    assert!(leftover.is_empty(), "{leftover:?}");
    let legacy = healer.parent().unwrap().join("legacy").join("Healer.toml");
    assert_eq!(ProfileConfig::load(&legacy).unwrap().aliases[0].name, "hh");
    // A profile that never saved a file gets one that keeps the
    // Healer alias off for it, as it had no such alias.
    let prompt = ProfileConfig::load(&set.profile_path("Test-Prompt")).unwrap();
    assert_eq!(prompt.disabled_alias_groups, ["(Healer)"]);
    assert!(!legacy.with_file_name("Test-Prompt.toml").exists());

    // A second run in the same session would read the profile
    // files, which hold no items now, and write that over the
    // catalog.
    let before = read(&catalog_path(dir.path()));
    refused(&state).await;
    assert_eq!(read(&catalog_path(dir.path())), before);
    let leftover = &ProfileConfig::load(&healer).unwrap().aliases;
    assert!(leftover.is_empty(), "{leftover:?}");
}

#[tokio::test]
async fn a_conflict_you_leave_alone_keeps_the_version_that_was_on() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    // Default keeps its kk off, and the Healer uses its own.
    let mut off = vosh_automation::alias::Alias::new("kk", "kick %1");
    off.enabled = false;
    let mut config = ProfileConfig::default();
    config.aliases.push(off);
    config
        .save(&set.profile_path(DEFAULT_PROFILE_NAME))
        .unwrap();
    let mut config = ProfileConfig::default();
    config
        .aliases
        .push(vosh_automation::alias::Alias::new("kk", "kick 1."));
    config.save(&set.profile_path("Healer")).unwrap();
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    let plan = analyze_migration(&state, LIBRARY).await.unwrap();
    assert_eq!(plan.conflicts[0].default_source, "Healer");

    // You apply without a pick, and the Healer keeps the kk it
    // used. It used to get the version Default had off.
    apply_migration(&state, &[], LIBRARY).await.unwrap();
    let state = relaunch_as(dir.path(), "Healer").await;
    let p = state.selected_profile().await;
    assert_eq!(p.aliases.get("kk").unwrap().expansion, "kick 1.");
    assert_eq!(items_on(&p), ["alias kk"]);
    drop(p);
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    let leftover = &items_on(&*state.selected_profile().await);
    assert!(leftover.is_empty(), "{leftover:?}");
}

#[tokio::test]
async fn the_wizard_never_writes_over_a_copy_an_earlier_run_left_in_legacy() {
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    write_alias(&set, "Healer", "hh");
    let state = launch_state(dir.path()).await;
    apply_migration(&state, &[], LIBRARY).await.unwrap();
    let legacy = crate::disk::paths::legacy_dir(dir.path()).join("Healer.toml");
    let copy = read(&legacy);

    // You move the catalog and the loadouts out to go back to per
    // profile mode, with the profile files as the wizard left them.
    let aside = tempfile::tempdir().unwrap();
    for path in [catalog_path(dir.path()), loadouts_path(dir.path())] {
        std::fs::rename(&path, aside.path().join(path.file_name().unwrap())).unwrap();
    }
    let state = launch_state(dir.path()).await;
    let healer = read(&set.profile_path("Healer"));

    // A second run would copy the files without their items over
    // the only copy that still holds them.
    assert_eq!(refused(&state).await, LEGACY_REFUSAL);
    assert_eq!(read(&legacy), copy);
    assert_eq!(ProfileConfig::load(&legacy).unwrap().aliases[0].name, "hh");
    assert_eq!(read(&set.profile_path("Healer")), healer);
    assert!(!catalog_path(dir.path()).exists());
}

const LEGACY_REFUSAL: &str =
    "Vosh found copies of your profile files in profiles/legacy from an earlier move to \
     loadouts and will not save over them. Each copy is a backup of its profile as it \
     was before that move. Your aliases, triggers, and macros are in the catalog.toml \
     that move wrote, with every change you made since. To keep them, quit Vosh and put \
     catalog.toml and loadouts.toml back in the Vosh folder. To build a new catalog from \
     the backups instead, quit Vosh, copy each backup over its file in the profiles \
     folder, and move the legacy folder out of the profiles folder. A backup brings back \
     every setting of its profile as it was before the move and drops every change you \
     made since. Never do both, since a backup copied back beside catalog.toml lays its \
     old items over the catalog for every character.";

#[tokio::test]
async fn putting_the_catalog_back_keeps_every_item_you_added_since() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    write_alias(&set, DEFAULT_PROFILE_NAME, "kk");
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    apply_migration(&state, &[], LIBRARY).await.unwrap();
    // In loadout mode you add an alias and set a variable, which
    // the backups in legacy never saw.
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    {
        let mut p = state.selected_profile().await;
        p.aliases
            .set(vosh_automation::alias::Alias::new("zz", "sleep"));
        p.vars.set("target", "dragon");
    }
    persist(&state).await;

    // You move the catalog and the loadouts out, and the wizard
    // refuses over the backups. It used to say that only the
    // backups held your items, and copying them back lost zz and
    // the target.
    let aside = tempfile::tempdir().unwrap();
    for path in [catalog_path(dir.path()), loadouts_path(dir.path())] {
        std::fs::rename(&path, aside.path().join(path.file_name().unwrap())).unwrap();
    }
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    assert_eq!(refused(&state).await, LEGACY_REFUSAL);

    // You put them back, as it says, and keep everything.
    for path in [catalog_path(dir.path()), loadouts_path(dir.path())] {
        std::fs::rename(aside.path().join(path.file_name().unwrap()), &path).unwrap();
    }
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    let p = state.selected_profile().await;
    assert_eq!(items_on(&p), ["alias kk", "alias zz"]);
    let kept = ProfileConfig::from_profile(&p);
    assert_eq!(kept.profile_vars.get("target").unwrap(), "dragon");
}

#[tokio::test]
async fn a_backup_copied_back_beside_the_catalog_spreads_its_old_items() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    write_alias(&set, DEFAULT_PROFILE_NAME, "kk");
    write_alias(&set, "Healer", "hh");
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    apply_migration(&state, &[], LIBRARY).await.unwrap();
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    state
        .selected_profile()
        .await
        .aliases
        .set(vosh_automation::alias::Alias::new("zz", "sleep"));
    persist(&state).await;
    let state = relaunch_as(dir.path(), "Healer").await;
    assert_eq!(
        items_on(&*state.selected_profile().await),
        ["alias hh", "alias zz"]
    );

    // The help used to say a backup copied back drops every change
    // since. With catalog.toml in place it does not. zz stays, the
    // default character gets the alias of the Healer, and the next
    // save shares the old kk with every character. The help says
    // never to do this.
    let legacy = crate::disk::paths::legacy_dir(dir.path());
    let default_file = set.profile_path(DEFAULT_PROFILE_NAME);
    std::fs::copy(
        legacy.join(default_file.file_name().unwrap()),
        &default_file,
    )
    .unwrap();
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    assert_eq!(
        items_on(&*state.selected_profile().await),
        ["alias hh", "alias kk", "alias zz"]
    );
    persist(&state).await;
    let state = relaunch_as(dir.path(), "Healer").await;
    assert_eq!(
        items_on(&*state.selected_profile().await),
        ["alias hh", "alias kk", "alias zz"]
    );
}

#[tokio::test]
async fn following_the_refusals_builds_the_catalog_again_with_every_item() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let names = [DEFAULT_PROFILE_NAME, "Healer", "Test-Prompt"];
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    for (n, name) in names.iter().enumerate() {
        character(name, n as u32 + 1, &[])
            .save(&set.profile_path(name))
            .unwrap();
    }
    let mut before = Vec::new();
    for name in names {
        before.push(items_on(
            &*relaunch_as(dir.path(), name).await.selected_profile().await,
        ));
    }
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    apply_migration(&state, &[], LIBRARY).await.unwrap();

    // Later you want to build the catalog again, and you do what
    // each refusal says, in turn, with Vosh closed.
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    let aside = tempfile::tempdir().unwrap();
    assert_eq!(refused(&state).await, LOADOUT_SESSION_REFUSAL);
    persist(&state).await;
    drop(state);
    let catalog = catalog_path(dir.path());
    std::fs::rename(&catalog, aside.path().join("catalog.toml")).unwrap();
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    assert!(refused(&state)
        .await
        .starts_with("Vosh found loadouts.toml"));
    let loadouts = loadouts_path(dir.path());
    std::fs::rename(&loadouts, aside.path().join("loadouts.toml")).unwrap();
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    assert_eq!(refused(&state).await, LEGACY_REFUSAL);
    // The refusal used to say only to move the legacy folder out.
    // The wizard then built the catalog from the files without
    // their items, and every character came back with nothing.
    let legacy = crate::disk::paths::legacy_dir(dir.path());
    for entry in std::fs::read_dir(&legacy).unwrap() {
        let entry = entry.unwrap();
        let back = set.profile_path(entry.path().file_stem().unwrap().to_str().unwrap());
        std::fs::copy(entry.path(), back).unwrap();
    }
    std::fs::rename(&legacy, aside.path().join("legacy")).unwrap();

    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    apply_migration(&state, &[], LIBRARY).await.unwrap();
    for (n, name) in names.iter().enumerate() {
        let state = relaunch_as(dir.path(), name).await;
        assert!(state.global_catalog.lock().await.is_some());
        assert_eq!(
            items_on(&*state.selected_profile().await),
            before[n],
            "{name}"
        );
    }
}

#[tokio::test]
async fn the_wizard_never_rewrites_a_profile_file_it_held_at_launch() {
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    write_alias(&set, "Healer", "hh");
    std::fs::write(set.active_path(), UNREADABLE).unwrap();
    let state = launch_state(dir.path()).await;
    // You fix the file in an editor while Vosh runs.
    let mut config = ProfileConfig::default();
    config
        .aliases
        .push(vosh_automation::alias::Alias::new("kk", "kick %1"));
    let fixed = config.to_toml().unwrap();
    std::fs::write(set.active_path(), &fixed).unwrap();

    assert_eq!(
        refused(&state).await,
        "Vosh could not read the Default profile file when it started, so it will not \
         change the file. Restart Vosh and try again."
    );
    assert_eq!(read(&set.active_path()), fixed);
    assert!(!catalog_path(dir.path()).exists());
    assert!(!loadouts_path(dir.path()).exists());
    assert!(!dir.path().join("profiles").join("legacy").exists());
}

/// Save `name`'s file with `list` as its enabled presets.
fn write_presets(set: &ProfileSet, name: &str, list: &[&str]) {
    let mut config = ProfileConfig::default();
    config.ui.enabled_presets = list.iter().map(|s| (*s).to_string()).collect();
    config.save(&set.profile_path(name)).unwrap();
}

#[tokio::test]
async fn the_wizard_keeps_off_a_preset_every_character_had_off() {
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    // Both characters turned the potion labels off.
    write_presets(
        &set,
        crate::profile::set::DEFAULT_PROFILE_NAME,
        &["healing_basics"],
    );
    write_presets(&set, "Healer", &["healing_basics", "herb_labels"]);
    let state = launch_state(dir.path()).await;
    apply_migration(&state, &[], LIBRARY).await.unwrap();

    let on = vec!["healing_basics".to_string(), "herb_labels".to_string()];
    let (catalog, _) = load_at_launch(dir.path()).unwrap();
    assert_eq!(catalog.enabled_presets, Some(on.clone()));

    // The first launch in loadout mode, as either character, keeps
    // the potion labels off.
    for name in [crate::profile::set::DEFAULT_PROFILE_NAME, "Healer"] {
        let state = relaunch_as(dir.path(), name).await;
        assert_eq!(state.selected_profile().await.ui.enabled_presets, on);
    }
}

#[tokio::test]
async fn the_preview_holds_the_shared_preset_list_and_each_characters_own() {
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    write_presets(
        &set,
        crate::profile::set::DEFAULT_PROFILE_NAME,
        &["healing_basics"],
    );
    write_presets(&set, "Healer", &["healing_basics", "herb_labels"]);
    // Test-Prompt never saved a file, so it has every preset on,
    // and the shared list leaves out the ones no saved profile has
    // on.
    let state = launch_state(dir.path()).await;
    let plan = analyze_migration(&state, LIBRARY).await.unwrap();
    let list = |ids: &[&str]| ids.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
    assert_eq!(
        plan.shared_presets,
        list(&["healing_basics", "herb_labels"])
    );
    assert_eq!(
        plan.profile_presets,
        [
            list(&["healing_basics"]),
            list(&["healing_basics", "herb_labels"]),
            Vec::new(),
        ]
    );
    // The catalog takes the list the preview showed.
    apply_migration(&state, &[], LIBRARY).await.unwrap();
    let (catalog, _) = load_at_launch(dir.path()).unwrap();
    assert_eq!(catalog.enabled_presets, Some(plan.shared_presets));
}

#[tokio::test]
async fn the_preview_counts_the_live_presets_of_a_profile_that_never_saved() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    write_presets(&set, DEFAULT_PROFILE_NAME, &["healing_basics"]);
    write_presets(&set, "Healer", &["healing_basics"]);
    // You create Test-Prompt, switch to it, and open the wizard
    // before anything saves it. Its live list is the defaults.
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    crate::profile::switch::switch_profile(&state, &state.selected_session(), "Test-Prompt")
        .await
        .unwrap();
    assert!(!set.profile_path("Test-Prompt").exists());

    // Apply saves the live profile first, so the catalog takes
    // every preset on. The preview used to leave Test-Prompt out
    // as a profile without a file and show healing_basics alone.
    let plan = analyze_migration(&state, LIBRARY).await.unwrap();
    let list = |ids: &[&str]| ids.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
    assert_eq!(plan.shared_presets, Vec::<String>::new());
    assert_eq!(
        plan.profile_presets,
        [
            list(&["healing_basics"]),
            list(&["healing_basics"]),
            Vec::new()
        ]
    );
    apply_migration(&state, &[], LIBRARY).await.unwrap();
    let (catalog, _) = load_at_launch(dir.path()).unwrap();
    assert_eq!(catalog.enabled_presets, Some(plan.shared_presets));
}

#[tokio::test]
async fn the_preview_reads_the_files_after_a_profile_reset() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    write_presets(&set, DEFAULT_PROFILE_NAME, &["healing_basics"]);
    write_presets(&set, "Healer", &["healing_basics"]);
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    // `#profile reset` blanks the live profile and holds every
    // save, so apply reads the file as it is, and so does the
    // preview.
    state.selected_profile().await.ui.enabled_presets.clear();
    state.selected_session().profile().hold(true);
    let plan = analyze_migration(&state, LIBRARY).await.unwrap();
    assert_eq!(plan.shared_presets, ["healing_basics"]);
    apply_migration(&state, &[], LIBRARY).await.unwrap();
    let (catalog, _) = load_at_launch(dir.path()).unwrap();
    assert_eq!(catalog.enabled_presets, Some(plan.shared_presets));
}

/// Everything one character keeps in per profile mode. `n` makes
/// every value differ between characters, so a profile that comes
/// back with the defaults or with another character's values
/// fails the comparison.
fn character(name: &str, n: u32, presets: &[&str]) -> ProfileConfig {
    use crate::profile::panes::PaneLayoutPersist;
    use crate::profile::ui::{CustomTheme, TrackedAffect};
    let pick = |options: &[&str]| options[n as usize % options.len()].to_string();
    let trigger =
        |what: &str, pattern: &str, group: Option<&str>| vosh_automation::trigger::Trigger {
            group: group.map(String::from),
            ..vosh_automation::trigger::Trigger::new(
                format!("{name} {what}"),
                pattern,
                vosh_automation::trigger::TriggerAction::Send {
                    template: format!("say {what} {n}"),
                },
            )
        };
    let mut config = ProfileConfig::default();
    let mut combat = vosh_automation::alias::Alias::new(format!("{name} bash"), "bash %1");
    combat.group = Some("combat".into());
    config.aliases = vec![
        vosh_automation::alias::Alias::new(format!("{name} kick"), format!("kick {n}")),
        combat,
    ];
    config.triggers = vec![
        trigger("greet", "^hi$", None),
        trigger("flee", "^You flee", Some("combat")),
    ];
    config.macros = vec![
        macro_on(&format!("f{n}"), &format!("cast {n}")),
        crate::profile::live::Macro {
            group: Some("combat".into()),
            ..macro_on(&format!("ctrl+{n}"), "flee")
        },
    ];
    config.timers = vec![crate::profile::live::Timer {
        id: n,
        name: format!("drink {n}"),
        interval_secs: 60 + n,
        command: format!("drink {name}"),
        enabled: n % 2 == 0,
        group: None,
    }];
    config
        .profile_vars
        .insert("target".into(), format!("orc {n}"));
    config.tick.enabled = n % 2 == 0;
    config.tick.interval_secs = 30 + u64::from(n);
    config.tick.auto_fire = Some(format!("stand {n}"));
    config.tick.sound = n % 2 == 0;
    config.tick.reset_pattern = Some(format!("^The day {n} has begun"));
    config.tick.warn_at_secs = Some(u64::from(n));
    config.tick.warn_message = Some(format!("tick {n} soon"));
    config.tick.warn_color = Some("#ff0000".into());
    config.plugins.enabled = vec![format!("plugin {n}")];
    let ui = &mut config.ui;
    ui.enabled_presets = presets.iter().map(|s| (*s).to_string()).collect();
    ui.vitals_values = pick(&["current-max", "current", "percent"]);
    ui.vitals_meter = pick(&["line", "bar", "none"]);
    ui.vitals_density = pick(&["rows", "line"]);
    ui.vitals_warn_thirds = n % 2 == 1;
    ui.vitals_hide_when_pinned = n % 2 == 0;
    ui.affects_style = pick(&["timers", "countdown", "chips", "chips_drain"]);
    ui.affects_marker = pick(&["dot", "square", "plus_minus", "none"]);
    ui.affects_tint = n % 2 == 0;
    ui.affects_running_out_hours = 2 + n;
    ui.affects_almost_gone_hours = n;
    ui.vitals.show_delta = n % 2 == 0;
    ui.tracked_affects = vec![
        TrackedAffect {
            name: format!("Sanctuary {n}"),
            label: Some(format!("S{n}")),
        },
        TrackedAffect {
            name: format!("Haste {n}"),
            label: None,
        },
    ];
    ui.panes = Some(PaneLayoutPersist {
        panel_open: n % 2 == 1,
        panel_width: Some(300 + 20 * n),
        ..PaneLayoutPersist::default_layout()
    });
    ui.custom_themes = vec![CustomTheme {
        id: format!("night-ink-{n}"),
        label: format!("Night ink {n}"),
        ..CustomTheme::default()
    }];
    ui.theme = format!("night-ink-{n}");
    ui.chip_style = pick(&["value_only", "caption", "icon"]);
    ui.moons_position = pick(&["right-edge", "left-edge"]);
    ui.paste_line_delay_ms = 10 * n;
    config.set_prompt(vosh_prompt::PromptConfig::from_legacy(
        true,
        &format!("<%h hp {n}>"),
    ));
    config
}

/// What `config` holds besides the items and the preset list the
/// shared catalog owns, and the group checkbox lists and the
/// folder map, which name the catalog groups in loadout mode. As
/// TOML, so it compares every field.
fn settings(mut config: ProfileConfig) -> String {
    config.clear_catalog_items();
    config.ui.enabled_presets.clear();
    config.disabled_alias_groups.clear();
    config.disabled_trigger_groups.clear();
    config.disabled_macro_groups.clear();
    config.group_folders = crate::profile::file::GroupFolders::default();
    config.to_toml().unwrap()
}

/// What a save writes to the file of the live profile `p`.
fn saved_settings(p: &crate::profile::live::Profile, set: &ProfileSet) -> String {
    let mut config = ProfileConfig::from_profile(p);
    crate::profile::shared::strip_global_fields(&mut config, set.scope());
    settings(config)
}

/// The aliases, triggers, and macros that are on in `p`.
fn items_on(p: &crate::profile::live::Profile) -> Vec<String> {
    let on = |group: Option<&str>, off: &[String]| {
        group.is_none_or(|g| g.is_empty() || !off.iter().any(|o| o == g))
    };
    let alias_off = p.aliases.disabled_groups();
    let trigger_off = p.triggers.disabled_groups();
    let macro_off: Vec<String> = p.disabled_macro_groups.iter().cloned().collect();
    let mut items: Vec<String> = p
        .aliases
        .list()
        .into_iter()
        .filter(|a| a.enabled && on(a.group.as_deref(), &alias_off))
        .map(|a| format!("alias {}", a.name))
        .chain(
            p.triggers
                .list()
                .into_iter()
                .filter(|t| t.enabled && on(t.group.as_deref(), &trigger_off))
                .map(|t| format!("trigger {}", t.name)),
        )
        .chain(
            p.macros
                .iter()
                .filter(|m| m.enabled && on(m.group.as_deref(), &macro_off))
                .map(|m| format!("macro {}", m.key)),
        )
        .collect();
    items.sort();
    items
}

/// Every alias, trigger, and macro in the three lists, one JSON
/// line each, sorted. The item types do not implement `PartialEq`.
fn item_rows(
    aliases: &[vosh_automation::alias::Alias],
    triggers: &[vosh_automation::trigger::Trigger],
    macros: &[crate::profile::live::Macro],
) -> Vec<String> {
    let mut rows: Vec<String> = aliases
        .iter()
        .map(|a| serde_json::to_string(a).unwrap())
        .chain(triggers.iter().map(|t| serde_json::to_string(t).unwrap()))
        .chain(macros.iter().map(|m| serde_json::to_string(m).unwrap()))
        .collect();
    rows.sort();
    rows
}

#[tokio::test]
async fn the_wizard_keeps_every_setting_of_every_profile() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    use crate::profile::shared::{Scope, ScopeConfig};
    let dir = tempfile::tempdir().unwrap();
    let mut set = james_like_set(dir.path());
    // Your themes and panels differ per character, the way James
    // keeps them.
    set.set_scope(ScopeConfig {
        theme: Scope::Profile,
        dock_layout: Scope::Profile,
        ..ScopeConfig::default()
    })
    .unwrap();
    let names = [DEFAULT_PROFILE_NAME, "Healer", "Test-Prompt"];
    let presets: [&[&str]; 3] = [
        &["healing_basics"],
        &["healing_basics", "herb_labels"],
        &["potion_labels"],
    ];
    let mut originals = Vec::new();
    for (n, name) in names.iter().enumerate() {
        let path = set.profile_path(name);
        character(name, n as u32 + 1, presets[n])
            .save(&path)
            .unwrap();
        originals.push(read(&path));
    }

    // What each character holds in per profile mode.
    let mut before = Vec::new();
    for name in names {
        let state = relaunch_as(dir.path(), name).await;
        assert!(state.global_catalog.lock().await.is_none());
        let p = state.selected_profile().await;
        assert_eq!(items_on(&p).len(), 6, "{name}");
        before.push((
            settings(ProfileConfig::from_profile(&p)),
            items_on(&p),
            saved_settings(&p, &set),
        ));
    }

    // You build the catalog while you play Default. A save of the
    // live profile has run by then, as one runs at every launch.
    // The wizard saves it again first.
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    persist(&state).await;
    originals[0] = read(&set.profile_path(DEFAULT_PROFILE_NAME));
    apply_migration(&state, &[], LIBRARY).await.unwrap();

    let (catalog, loadouts) = load_at_launch(dir.path()).unwrap();
    let leftover = &loadouts.active;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert!(!loadouts.dormant);
    // Every item sits in the catalog once.
    let mut in_catalog: Vec<String> = catalog
        .aliases
        .iter()
        .map(|a| format!("alias {}", a.name))
        .chain(
            catalog
                .triggers
                .iter()
                .map(|t| format!("trigger {}", t.name)),
        )
        .chain(catalog.macros.iter().map(|m| format!("macro {}", m.key)))
        .collect();
    in_catalog.sort();
    let mut every_item: Vec<String> = before.iter().flat_map(|(_, on, _)| on.clone()).collect();
    every_item.sort();
    assert_eq!(in_catalog, every_item);
    let shared_presets = vec![
        "healing_basics".to_string(),
        "herb_labels".to_string(),
        "potion_labels".to_string(),
    ];
    assert_eq!(catalog.enabled_presets, Some(shared_presets.clone()));

    let legacy = dir.path().join("profiles").join("legacy");
    for (n, name) in names.iter().enumerate() {
        let path = set.profile_path(name);
        // A full copy of the file you had waits in legacy.
        assert_eq!(read(&legacy.join(format!("{name}.toml"))), originals[n]);
        // The file stays with every setting but the items.
        let kept = ProfileConfig::load(&path).unwrap();
        assert!(kept.aliases.is_empty(), "{name}");
        assert!(kept.triggers.is_empty(), "{name}");
        assert!(kept.macros.is_empty(), "{name}");
        let original = ProfileConfig::from_toml(&originals[n]).unwrap();
        assert_eq!(settings(kept), settings(original), "{name}");
        // The rewrite kept a backup of the file beside it.
        let backup = format!("{name}.toml.bak.");
        let backups = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with(&backup))
            .count();
        assert!(backups >= 1, "{name}");
    }

    // Quit, then open Vosh as each character.
    for (n, name) in names.iter().enumerate() {
        let state = relaunch_as(dir.path(), name).await;
        assert!(state.global_catalog.lock().await.is_some(), "{name}");
        {
            let p = state.selected_profile().await;
            assert_eq!(
                settings(ProfileConfig::from_profile(&p)),
                before[n].0,
                "{name}"
            );
            assert_eq!(items_on(&p), before[n].1, "{name}");
            assert_eq!(p.ui.enabled_presets, shared_presets, "{name}");
            // The live stores hold each catalog item once, as the
            // catalog has it.
            let aliases: Vec<_> = p.aliases.list().into_iter().cloned().collect();
            assert_eq!(
                item_rows(&aliases, &p.triggers.list(), &p.macros),
                item_rows(&catalog.aliases, &catalog.triggers, &catalog.macros),
                "{name}"
            );
        }

        // The first save in loadout mode.
        persist(&state).await;
        let saved = ProfileConfig::load(&set.profile_path(name)).unwrap();
        assert!(saved.aliases.is_empty(), "{name}");
        assert!(saved.triggers.is_empty(), "{name}");
        assert!(saved.macros.is_empty(), "{name}");
        assert_eq!(settings(saved), before[n].2, "{name}");
        let (saved_catalog, _) = load_at_launch(dir.path()).unwrap();
        assert_eq!(
            item_rows(
                &saved_catalog.aliases,
                &saved_catalog.triggers,
                &saved_catalog.macros
            ),
            item_rows(&catalog.aliases, &catalog.triggers, &catalog.macros),
            "{name}"
        );
    }
}

#[tokio::test]
async fn each_loadout_turns_on_the_items_its_character_shared() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    let save = |name: &str, aliases: &[(&str, &str)], combat: &[(&str, &str)]| {
        let mut config = ProfileConfig::default();
        for (alias, expansion) in aliases {
            config
                .aliases
                .push(vosh_automation::alias::Alias::new(*alias, *expansion));
        }
        for (alias, expansion) in combat {
            let mut a = vosh_automation::alias::Alias::new(*alias, *expansion);
            a.group = Some("combat".into());
            config.aliases.push(a);
        }
        config.save(&set.profile_path(name)).unwrap();
    };
    // Test-Prompt began as a copy of Default. Both have kk and
    // bash as they are, each changed cc its own way, and each
    // added one alias of its own.
    save(
        DEFAULT_PROFILE_NAME,
        &[("kk", "kick %1"), ("cc", "cast a"), ("dd", "dig")],
        &[("bash", "bash %1")],
    );
    save(
        "Test-Prompt",
        &[("kk", "kick %1"), ("cc", "cast b"), ("tp", "prompt")],
        &[("bash", "bash %1")],
    );
    save("Healer", &[("hh", "heal %1")], &[]);

    let names = [DEFAULT_PROFILE_NAME, "Healer", "Test-Prompt"];
    let mut before = Vec::new();
    for name in names {
        let state = relaunch_as(dir.path(), name).await;
        before.push(items_on(&*state.selected_profile().await));
    }

    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    apply_migration(&state, &[], LIBRARY).await.unwrap();
    let (catalog, _) = load_at_launch(dir.path()).unwrap();
    let group = |alias: &str| {
        let found = catalog.aliases.iter().find(|a| a.name == alias).unwrap();
        found.group.clone().unwrap()
    };
    // What both characters had sits in a group of its own, and
    // the combat folder they share keeps its name.
    assert_eq!(group("kk"), "(default, Test-Prompt)");
    assert_eq!(group("cc"), "(default, Test-Prompt)");
    assert_eq!(group("bash"), "combat");
    assert_eq!(group("dd"), "(default)");
    assert_eq!(group("tp"), "(Test-Prompt)");

    for (n, name) in names.iter().enumerate() {
        let mut loadouts = crate::loadouts::set::load_loadout_set(dir.path()).unwrap();
        loadouts.active = vec![(*name).to_string()];
        save_loadout_set(dir.path(), &loadouts).unwrap();
        let state = relaunch_as(dir.path(), name).await;
        assert_eq!(
            items_on(&*state.selected_profile().await),
            before[n],
            "{name}"
        );
    }
}

#[tokio::test]
async fn each_loadout_keeps_off_the_groups_its_character_had_off() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let mut set = james_like_set(dir.path());
    set.create("Bard").unwrap();
    // Default has everything on.
    character(DEFAULT_PROFILE_NAME, 1, &[])
        .save(&set.profile_path(DEFAULT_PROFILE_NAME))
        .unwrap();
    // Healer turned its combat group off in every list.
    let mut healer = character("Healer", 2, &[]);
    healer.disabled_alias_groups = vec!["combat".into()];
    healer.disabled_trigger_groups = vec!["combat".into()];
    healer.disabled_macro_groups = vec!["combat".into()];
    healer.save(&set.profile_path("Healer")).unwrap();
    // The Bard has no aliases, triggers, or macros of its own, and
    // Test-Prompt never saved a file.
    let mut bard = ProfileConfig::default();
    bard.profile_vars.insert("target".into(), "rat".into());
    bard.save(&set.profile_path("Bard")).unwrap();

    let names = [DEFAULT_PROFILE_NAME, "Healer", "Test-Prompt", "Bard"];
    let mut before = Vec::new();
    for name in names {
        let state = relaunch_as(dir.path(), name).await;
        before.push(items_on(&*state.selected_profile().await));
    }
    assert_eq!(before[0].len(), 6);
    assert_eq!(before[1].len(), 3);
    let leftover = &before[2];
    assert!(leftover.is_empty(), "{leftover:?}");
    let leftover = &before[3];
    assert!(leftover.is_empty(), "{leftover:?}");

    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    apply_migration(&state, &[], LIBRARY).await.unwrap();
    let (_, loadouts) = load_at_launch(dir.path()).unwrap();
    let groups = |name: &str| loadouts.get(name).unwrap().enabled_groups.clone();
    assert_eq!(groups("Healer"), ["(Healer)"]);
    let leftover = &groups("Test-Prompt");
    assert!(leftover.is_empty(), "{leftover:?}");
    let leftover = &groups("Bard");
    assert!(leftover.is_empty(), "{leftover:?}");
    // The Settings group checkboxes of each file say the same.
    let healer = ProfileConfig::load(&set.profile_path("Healer")).unwrap();
    assert_eq!(
        healer.disabled_alias_groups,
        ["(default)", "combat", "combat (Healer)"]
    );
    let bard = ProfileConfig::load(&set.profile_path("Bard")).unwrap();
    assert_eq!(bard.profile_vars.get("target").unwrap(), "rat");
    assert_eq!(
        bard.disabled_trigger_groups,
        ["(Healer)", "(default)", "combat", "combat (Healer)"]
    );

    // With its own loadout on, and with that loadout on beside
    // another character's, each character has on what it had.
    for (n, name) in names.iter().enumerate() {
        for other in ["", "Bard"] {
            let mut loadouts = crate::loadouts::set::load_loadout_set(dir.path()).unwrap();
            loadouts.active = vec![(*name).to_string()];
            if !other.is_empty() && other != *name {
                loadouts.active.push(other.to_string());
            }
            save_loadout_set(dir.path(), &loadouts).unwrap();
            let state = relaunch_as(dir.path(), name).await;
            assert_eq!(
                items_on(&*state.selected_profile().await),
                before[n],
                "{name}"
            );
            persist(&state).await;
            let state = relaunch_as(dir.path(), name).await;
            assert_eq!(
                items_on(&*state.selected_profile().await),
                before[n],
                "{name}"
            );
        }
    }
}

#[tokio::test]
async fn group_turns_a_folder_on_and_off_as_before_after_the_wizard() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    let in_combat = |name: &str| {
        let mut t = send_trigger(name, &format!("^{name}$"), name);
        t.group = Some("combat".into());
        t
    };
    // Both have flee in combat, and the Healer has bash there too.
    let mut config = ProfileConfig {
        triggers: vec![in_combat("flee")],
        ..ProfileConfig::default()
    };
    config
        .save(&set.profile_path(DEFAULT_PROFILE_NAME))
        .unwrap();
    config.triggers.push(in_combat("bash"));
    config.save(&set.profile_path("Healer")).unwrap();
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    apply_migration(&state, &[], LIBRARY).await.unwrap();

    // The Healer folder landed in two catalog groups. `#group
    // combat` used to find no group of that name at all.
    let state = relaunch_as(dir.path(), "Healer").await;
    let mut p = state.selected_profile().await;
    assert_eq!(items_on(&p), ["trigger bash", "trigger flee"]);
    let r = crate::input::process(&mut p, "#group combat off");
    assert_eq!(r.echo, ["group `combat` disabled for triggers"]);
    let leftover = &items_on(&p);
    assert!(leftover.is_empty(), "{leftover:?}");
    let r = crate::input::process(&mut p, "#group combat");
    assert_eq!(r.echo[1], "  triggers: off");
    crate::input::process(&mut p, "#group combat on");
    assert_eq!(items_on(&p), ["trigger bash", "trigger flee"]);
    drop(p);

    // For Default it turns off flee alone, and never turns the
    // Healer's bash on.
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    let mut p = state.selected_profile().await;
    crate::input::process(&mut p, "#group combat off");
    let leftover = &items_on(&p);
    assert!(leftover.is_empty(), "{leftover:?}");
    crate::input::process(&mut p, "#group combat on");
    assert_eq!(items_on(&p), ["trigger flee"]);
    drop(p);

    // Test-Prompt had no combat folder, and still has none.
    let state = relaunch_as(dir.path(), "Test-Prompt").await;
    let mut p = state.selected_profile().await;
    let r = crate::input::process(&mut p, "#group combat on");
    assert_eq!(
        r.echo,
        ["[group `combat` not found in triggers, aliases, macros, or timers]"]
    );
    let leftover = &items_on(&p);
    assert!(leftover.is_empty(), "{leftover:?}");
}

#[tokio::test]
async fn each_character_keeps_its_own_version_of_a_trigger() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    for (name, command) in [
        (DEFAULT_PROFILE_NAME, "wave $1"),
        ("Healer", "cast bless $1"),
    ] {
        ProfileConfig {
            triggers: vec![send_trigger("greet", r"^(\w+) arrives", command)],
            ..ProfileConfig::default()
        }
        .save(&set.profile_path(name))
        .unwrap();
    }
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    let plan = analyze_migration(&state, LIBRARY).await.unwrap();
    assert!(plan.conflicts.is_empty());
    apply_migration(&state, &[], LIBRARY).await.unwrap();

    // The wizard kept one version for both, so the Healer waved.
    for (name, sent) in [
        (DEFAULT_PROFILE_NAME, "wave Bob"),
        ("Healer", "cast bless Bob"),
    ] {
        let state = relaunch_as(dir.path(), name).await;
        let p = state.selected_profile().await;
        let line = vosh_automation::trigger::process(
            &p.triggers,
            b"Bob arrives",
            vosh_automation::StopKey::default(),
        );
        assert_eq!(line.sends, [sent], "{name}");
    }
}

#[tokio::test]
async fn triggers_on_one_line_fire_in_the_order_they_had() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    let down = "^You are knocked down";
    ProfileConfig {
        triggers: vec![
            send_trigger("zz stand", down, "stand"),
            send_trigger("aa bash", down, "bash"),
        ],
        ..ProfileConfig::default()
    }
    .save(&set.profile_path(DEFAULT_PROFILE_NAME))
    .unwrap();
    ProfileConfig {
        triggers: vec![
            send_trigger("aa bash", down, "bash"),
            send_trigger("zz stand", down, "stand"),
        ],
        ..ProfileConfig::default()
    }
    .save(&set.profile_path("Healer"))
    .unwrap();
    let fired = |state: &SharedState| {
        let state = state.clone();
        async move {
            let p = state.selected_profile().await;
            vosh_automation::trigger::process(
                &p.triggers,
                b"You are knocked down!",
                vosh_automation::StopKey::default(),
            )
            .sends
        }
    };
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    assert_eq!(fired(&state).await, ["stand", "bash"]);
    apply_migration(&state, &[], LIBRARY).await.unwrap();

    // They used to run in name order, so Default bashed while it
    // was still on the ground.
    for (name, sends) in [
        (DEFAULT_PROFILE_NAME, ["stand", "bash"]),
        ("Healer", ["bash", "stand"]),
    ] {
        let state = relaunch_as(dir.path(), name).await;
        assert_eq!(fired(&state).await, sends, "{name}");
        persist(&state).await;
        let state = relaunch_as(dir.path(), name).await;
        assert_eq!(fired(&state).await, sends, "{name}");
    }
}

/// A trigger that sends `command` on lines matching `pattern`.
fn send_trigger(name: &str, pattern: &str, command: &str) -> vosh_automation::trigger::Trigger {
    vosh_automation::trigger::Trigger::new(
        name,
        pattern,
        vosh_automation::trigger::TriggerAction::Send {
            template: command.into(),
        },
    )
}

#[tokio::test]
async fn a_trigger_group_you_had_off_stays_off_beside_its_aliases() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    // The Healer loots by hand with its loot alias and keeps the
    // auto loot trigger off. Both sit in a group named loot.
    let mut healer = ProfileConfig::default();
    let mut alias = vosh_automation::alias::Alias::new("loot", "get all corpse");
    alias.group = Some("loot".into());
    healer.aliases.push(alias);
    let mut autoloot = send_trigger("autoloot", "^You killed", "get all corpse");
    autoloot.group = Some("loot".into());
    healer.triggers.push(autoloot);
    healer.disabled_trigger_groups = vec!["loot".into()];
    healer.save(&set.profile_path("Healer")).unwrap();
    let before = items_on(
        &*relaunch_as(dir.path(), "Healer")
            .await
            .selected_profile()
            .await,
    );
    assert_eq!(before, ["alias loot"]);

    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    apply_migration(&state, &[], LIBRARY).await.unwrap();

    // The trigger used to come back on with the alias group of the
    // same name, and looted every kill.
    let state = relaunch_as(dir.path(), "Healer").await;
    assert_eq!(items_on(&*state.selected_profile().await), before);
}

#[tokio::test]
async fn a_shared_item_one_character_had_off_stays_off_for_it() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    // Test-Prompt began as a copy of Default, auto loot and all,
    // and you turned its loot group off.
    let mut autoloot = send_trigger("autoloot", "^You killed", "get all corpse");
    autoloot.group = Some("loot".into());
    let mut default = ProfileConfig::default();
    default.triggers.push(autoloot.clone());
    default
        .save(&set.profile_path(DEFAULT_PROFILE_NAME))
        .unwrap();
    let mut copy = ProfileConfig::default();
    copy.triggers.push(autoloot);
    copy.disabled_trigger_groups = vec!["loot".into()];
    copy.save(&set.profile_path("Test-Prompt")).unwrap();
    let names = [DEFAULT_PROFILE_NAME, "Test-Prompt"];
    let mut before = Vec::new();
    for name in names {
        before.push(items_on(
            &*relaunch_as(dir.path(), name).await.selected_profile().await,
        ));
    }
    assert_eq!(before[0], ["trigger autoloot"]);
    let leftover = &before[1];
    assert!(leftover.is_empty(), "{leftover:?}");

    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    apply_migration(&state, &[], LIBRARY).await.unwrap();

    // The trigger used to land in one group on for both.
    for (n, name) in names.iter().enumerate() {
        let state = relaunch_as(dir.path(), name).await;
        assert_eq!(
            items_on(&*state.selected_profile().await),
            before[n],
            "{name}"
        );
    }
}

/// The healing basics trigger as the preset library holds it.
fn heal_preset() -> vosh_automation::trigger::Trigger {
    vosh_automation::trigger::Trigger {
        preset: Some("healing_basics".into()),
        ..send_trigger("heal 1", "^You heal", "say healed")
    }
}

/// Open Vosh as `name` and install the preset triggers again, as
/// the main window does at launch for every preset that is on.
async fn launch_with_presets(dir: &std::path::Path, name: &str) -> SharedState {
    let state = relaunch_as(dir, name).await;
    let installed = {
        let mut p = state.selected_profile().await;
        let on = p.ui.enabled_presets.is_empty()
            || p.ui.enabled_presets.iter().any(|id| id == "healing_basics");
        on.then(|| crate::loadouts::presets::install_preset_triggers(&mut p, vec![heal_preset()]))
    };
    if let Some(result) = installed {
        result.unwrap();
        persist(&state).await;
    }
    state
}

/// Whether the healing basics trigger is on in `state`, and
/// whether the Presets tab shows healing basics on.
async fn heal_preset_on(state: &SharedState) -> (bool, bool) {
    let p = state.selected_profile().await;
    let tab = p.ui.enabled_presets.is_empty()
        || p.ui.enabled_presets.iter().any(|id| id == "healing_basics");
    (items_on(&p).contains(&"trigger heal 1".to_string()), tab)
}

#[tokio::test]
async fn a_preset_stays_on_for_a_character_whose_file_lacked_it() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    // Default has healing basics in its file. The Healer saved its
    // file before the preset came out, on the default presets.
    let mut default = ProfileConfig::default();
    default.triggers.push(heal_preset());
    default
        .save(&set.profile_path(DEFAULT_PROFILE_NAME))
        .unwrap();
    let mut healer = ProfileConfig::default();
    healer.profile_vars.insert("target".into(), "orc".into());
    healer.save(&set.profile_path("Healer")).unwrap();
    // Per profile, a launch as the Healer installs the preset. It
    // runs over a copy, since the launch saves the Healer file.
    {
        let copy = tempfile::tempdir().unwrap();
        let copy_set = james_like_set(copy.path());
        healer.save(&copy_set.profile_path("Healer")).unwrap();
        let state = launch_with_presets(copy.path(), "Healer").await;
        assert_eq!(heal_preset_on(&state).await, (true, true));
    }

    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    apply_migration(&state, &[], LIBRARY).await.unwrap();

    // The trigger used to sit in the default group, which the
    // Healer file kept off, while the Presets tab showed it on,
    // and each launch put it back in that group.
    for _ in 0..2 {
        let state = launch_with_presets(dir.path(), "Healer").await;
        assert_eq!(heal_preset_on(&state).await, (true, true));
    }
}

#[tokio::test]
async fn a_preset_stays_on_for_a_character_that_never_saved_a_file() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    let mut default = ProfileConfig::default();
    default.triggers.push(heal_preset());
    default
        .save(&set.profile_path(DEFAULT_PROFILE_NAME))
        .unwrap();
    // Test-Prompt never saved a file, and per profile a launch as
    // it installs every preset.
    {
        let copy = tempfile::tempdir().unwrap();
        james_like_set(copy.path());
        let state = launch_with_presets(copy.path(), "Test-Prompt").await;
        assert_eq!(heal_preset_on(&state).await, (true, true));
    }

    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    apply_migration(&state, &[], LIBRARY).await.unwrap();

    for _ in 0..2 {
        let state = launch_with_presets(dir.path(), "Test-Prompt").await;
        assert_eq!(heal_preset_on(&state).await, (true, true));
    }
}

/// The macros Numpad movement adds, in the game's order n e s w u d.
fn numpad_movement() -> Vec<crate::profile::live::Macro> {
    [
        ("Numpad8", "n"),
        ("Numpad6", "e"),
        ("Numpad2", "s"),
        ("Numpad4", "w"),
        ("Numpad9", "u"),
        ("Numpad3", "d"),
    ]
    .into_iter()
    .map(|(key, command)| crate::profile::live::Macro {
        preset: Some("numpad_movement".into()),
        ..macro_on(key, command)
    })
    .collect()
}

#[tokio::test]
async fn a_preset_macro_folds_into_one_beside_your_macro_on_its_key() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    // Both characters have Numpad movement on. Default binds its own
    // Numpad3 too, which holds the preset's d off in its file.
    for (name, yours) in [
        (DEFAULT_PROFILE_NAME, Some(macro_on("Numpad3", "rec"))),
        ("Healer", None),
    ] {
        let mut config = ProfileConfig::default();
        config.ui.enabled_presets = vec!["numpad_movement".into()];
        config.macros.extend(yours);
        config.macros.extend(numpad_movement());
        crate::loadouts::presets::hold_taken_keys(
            &mut config.macros,
            &std::collections::BTreeSet::new(),
        );
        config.save(&set.profile_path(name)).unwrap();
    }
    let state = launch_state(dir.path()).await;

    // Default's two Numpad3 rows used to fold into one with Healer's d,
    // and the wizard asked you to pick between rec and d.
    let plan = analyze_migration(&state, LIBRARY).await.unwrap();
    assert!(plan.conflicts.is_empty(), "{:?}", plan.conflicts);
    apply_migration(&state, &[], LIBRARY).await.unwrap();

    // The catalog keeps rec beside one copy of each preset macro. A
    // launch installs those for Test-Prompt too, so they need no group,
    // and d waits while rec keeps the key.
    let (catalog, _) = load_at_launch(dir.path()).unwrap();
    let rows: Vec<(&str, &str, bool)> = catalog
        .macros
        .iter()
        .map(|m| (m.key.as_str(), m.command.as_str(), m.enabled))
        .collect();
    assert_eq!(
        rows,
        [
            ("Numpad3", "rec", true),
            ("Numpad2", "s", true),
            ("Numpad3", "d", false),
            ("Numpad4", "w", true),
            ("Numpad6", "e", true),
            ("Numpad8", "n", true),
            ("Numpad9", "u", true),
        ]
    );
    let presets = catalog.macros.iter().filter(|m| m.preset.is_some());
    assert!(presets.clone().all(|m| m.group.is_none()));
    assert_eq!(presets.count(), 6);
}

/// What Numpad3 sends for the character a launch plays: each macro on it
/// that is on, in a group that is on.
async fn numpad3_sends(state: &SharedState) -> Vec<String> {
    let p = state.selected_profile().await;
    p.macros
        .iter()
        .filter(|m| m.key == "Numpad3" && m.enabled)
        .filter(|m| {
            m.group
                .as_ref()
                .is_none_or(|g| !p.disabled_macro_groups.contains(g))
        })
        .map(|m| m.command.clone())
        .collect()
}

#[tokio::test]
async fn each_character_keeps_the_preset_key_it_had_after_the_wizard() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    // Both characters have Numpad movement on. Default's only Numpad3 is
    // the preset's d, and Healer binds its own rec there, which holds d
    // off in its file.
    for (name, yours) in [
        (DEFAULT_PROFILE_NAME, None),
        ("Healer", Some(macro_on("Numpad3", "rec"))),
    ] {
        let mut config = ProfileConfig::default();
        config.ui.enabled_presets = vec!["numpad_movement".into()];
        config.macros.extend(yours);
        config.macros.extend(numpad_movement());
        crate::loadouts::presets::hold_taken_keys(
            &mut config.macros,
            &std::collections::BTreeSet::new(),
        );
        config.save(&set.profile_path(name)).unwrap();
    }
    let state = launch_state(dir.path()).await;
    apply_migration(&state, &[], LIBRARY).await.unwrap();

    // Healer's rec lands in a group Default keeps off, so Default keeps
    // going down with Numpad3 and Healer keeps its rec. Default used to
    // lose d to the rec it never had on.
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    assert_eq!(numpad3_sends(&state).await, ["d"]);
    let state = relaunch_as(dir.path(), "Healer").await;
    assert_eq!(numpad3_sends(&state).await, ["rec"]);

    // catalog.toml holds d off as rec holds it for every character, so
    // the file says the same whoever saved it last.
    let (catalog, _) = load_at_launch(dir.path()).unwrap();
    let d = catalog
        .macros
        .iter()
        .find(|m| m.preset.is_some() && m.key == "Numpad3");
    assert_eq!(d.map(|m| m.enabled), Some(false));
}

/// Default with the alias kk, a target, and a 300 pixel panel.
fn default_with_a_target(set: &ProfileSet) {
    let mut default = ProfileConfig::default();
    default
        .aliases
        .push(vosh_automation::alias::Alias::new("kk", "kick %1"));
    default.profile_vars.insert("target".into(), "orc".into());
    default.ui.panes = Some(crate::profile::panes::PaneLayoutPersist {
        panel_width: Some(300),
        ..crate::profile::panes::PaneLayoutPersist::default_layout()
    });
    default
        .save(&set.profile_path(crate::profile::set::DEFAULT_PROFILE_NAME))
        .unwrap();
}

#[tokio::test]
async fn the_wizard_keeps_what_you_changed_since_the_last_save() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    default_with_a_target(&set);
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    // Within the two seconds before the save, a script sets your
    // target, you drag the splitter, and you add an alias.
    {
        let mut p = state.selected_profile().await;
        p.vars.set("target", "dragon");
        if let Some(panes) = p.ui.panes.as_mut() {
            panes.panel_width = Some(420);
        }
        p.aliases
            .set(vosh_automation::alias::Alias::new("zz", "sleep"));
    }

    apply_migration(&state, &[], LIBRARY).await.unwrap();

    // Nothing saves between the wizard and the relaunch, so these
    // used to be lost.
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    let p = state.selected_profile().await;
    let kept = ProfileConfig::from_profile(&p);
    assert_eq!(kept.profile_vars.get("target").unwrap(), "dragon");
    assert_eq!(kept.ui.panes.unwrap().panel_width, Some(420));
    assert_eq!(items_on(&p), ["alias kk", "alias zz"]);
}

#[tokio::test]
async fn the_wizard_leaves_a_profile_you_reset_to_its_file() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    default_with_a_target(&set);
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    // `#profile reset` blanks the live profile and holds the saves
    // back, and the file stays as you saved it.
    ProfileConfig::default().apply_to(&mut *state.selected_profile().await);
    state.selected_session().profile().hold(true);

    apply_migration(&state, &[], LIBRARY).await.unwrap();

    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    let p = state.selected_profile().await;
    let kept = ProfileConfig::from_profile(&p);
    assert_eq!(kept.profile_vars.get("target").unwrap(), "orc");
    assert_eq!(kept.ui.panes.unwrap().panel_width, Some(300));
    assert_eq!(items_on(&p), ["alias kk"]);
}

#[tokio::test]
async fn a_switch_waits_for_the_relaunch_after_the_wizard() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    write_alias(&set, DEFAULT_PROFILE_NAME, "kk");
    write_alias(&set, "Healer", "hh");
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    apply_migration(&state, &[], LIBRARY).await.unwrap();

    // The Healer file holds no aliases now, and the catalog loads
    // only at launch, so a switch would leave you with none. A
    // login that picks the Healer says why on the terminal.
    let err = crate::profile::switch::switch_profile(&state, &state.selected_session(), "Healer")
        .await
        .unwrap_err();
    assert_eq!(
        crate::profile::switch::auto_switch_failed_line(&err),
        "\r\n\x1b[33mQuit Vosh and open it again to finish the move to loadouts, then \
         switch profiles.\x1b[0m\r\n"
    );
    assert_eq!(
        crate::profile::switch::tests::active(&state).await,
        DEFAULT_PROFILE_NAME
    );
    assert_eq!(items_on(&*state.selected_profile().await), ["alias kk"]);

    // Once Vosh opens again, the switch runs.
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    crate::profile::switch::switch_profile(&state, &state.selected_session(), "Healer")
        .await
        .unwrap();
    assert_eq!(items_on(&*state.selected_profile().await), ["alias hh"]);
}

/// Try to rename Healer, copy it, and make a profile from it while a
/// relaunch is pending on `state`, and check that each is refused and
/// that the profile files and the index stay as they were.
async fn renames_and_copies_are_refused(state: &SharedState, dir: &std::path::Path) {
    let set = ProfileSet::load_or_migrate(dir.to_path_buf()).unwrap();
    let files: Vec<(std::path::PathBuf, Option<String>)> = set
        .list()
        .iter()
        .map(|entry| {
            let path = set.profile_path(&entry.name);
            let text = path.exists().then(|| read(&path));
            (path, text)
        })
        .collect();
    let index = read(&dir.join("profiles.toml"));

    let rename = crate::profile::set::rename_profile(state, "Healer", "Cleric")
        .await
        .unwrap_err();
    assert_eq!(
        rename,
        "Quit Vosh and open it again to finish the move to loadouts, then rename the \
         profile."
    );
    let copy = "Quit Vosh and open it again to finish the move to loadouts, then copy \
                the profile.";
    let duplicate = crate::profile::set::duplicate_profile(state, "Healer", "Cleric")
        .await
        .unwrap_err();
    assert_eq!(duplicate, copy);
    let create = crate::profile::set::create_profile(state, "Cleric", Some("Healer"), None)
        .await
        .unwrap_err();
    assert_eq!(create, copy);

    assert_eq!(read(&dir.join("profiles.toml")), index);
    assert!(!set.profile_path("Cleric").exists());
    for (path, text) in &files {
        match text {
            Some(text) => assert_eq!(&read(path), text, "{}", path.display()),
            None => assert!(!path.exists(), "{}", path.display()),
        }
    }
}

#[tokio::test]
async fn the_live_profile_keeps_the_name_the_prompt_draws() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    james_like_set(dir.path());
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    let shown = |p: &crate::profile::live::Profile| {
        p.name.as_deref().map(crate::profile::set::display_name)
    };
    assert_eq!(
        shown(&*state.selected_profile().await).as_deref(),
        Some("Default")
    );
    crate::profile::switch::switch_profile(&state, &state.selected_session(), "Healer")
        .await
        .unwrap();
    assert_eq!(
        shown(&*state.selected_profile().await).as_deref(),
        Some("Healer")
    );
    // Renaming the live profile renames what the prompt draws.
    crate::profile::set::rename_profile(&state, "Healer", "Cleric")
        .await
        .unwrap();
    assert_eq!(
        shown(&*state.selected_profile().await).as_deref(),
        Some("Cleric")
    );
    // Renaming another profile leaves it alone.
    crate::profile::set::rename_profile(&state, "Test-Prompt", "Scratch")
        .await
        .unwrap();
    assert_eq!(
        shown(&*state.selected_profile().await).as_deref(),
        Some("Cleric")
    );
    // The events that name the active profile follow it too.
    assert_eq!(state.active_profile().as_deref(), Some("Cleric"));
    let state = relaunch_as(dir.path(), "Scratch").await;
    assert_eq!(
        shown(&*state.selected_profile().await).as_deref(),
        Some("Scratch")
    );
    assert_eq!(state.active_profile().as_deref(), Some("Scratch"));
}

#[tokio::test]
async fn renames_and_copies_wait_for_the_relaunch_after_the_wizard() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    write_alias(&set, DEFAULT_PROFILE_NAME, "kk");
    write_alias(&set, "Healer", "hh");
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    apply_migration(&state, &[], LIBRARY).await.unwrap();

    renames_and_copies_are_refused(&state, dir.path()).await;
    // A new profile that copies nothing only joins the index.
    crate::profile::set::create_profile(&state, "Bard", None, None)
        .await
        .unwrap();

    // Once Vosh opens again, the rename and the copy run.
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    crate::profile::set::duplicate_profile(&state, "Healer", "Cleric")
        .await
        .unwrap();
    crate::profile::set::rename_profile(&state, "Cleric", "Priest")
        .await
        .unwrap();
    let state = relaunch_as(dir.path(), "Priest").await;
    assert_eq!(items_on(&*state.selected_profile().await), ["alias hh"]);
}

#[tokio::test]
async fn a_wizard_run_that_stops_partway_finishes_at_the_next_launch() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let names = [DEFAULT_PROFILE_NAME, "Healer", "Test-Prompt"];
    // The run writes catalog.toml, loadouts.toml, and three
    // profile files. Stop it before each of them.
    for stop in 0..5 {
        let dir = tempfile::tempdir().unwrap();
        let set = james_like_set(dir.path());
        let mut healer = character("Healer", 2, &[]);
        healer.disabled_alias_groups = vec!["combat".into()];
        for (n, config) in [character(DEFAULT_PROFILE_NAME, 1, &[]), healer]
            .into_iter()
            .enumerate()
        {
            config.save(&set.profile_path(names[n])).unwrap();
        }
        // Test-Prompt shares Default's kick alias.
        let mut prompt = character("Test-Prompt", 3, &[]);
        prompt
            .aliases
            .push(vosh_automation::alias::Alias::new("default kick", "kick 1"));
        prompt.save(&set.profile_path("Test-Prompt")).unwrap();
        let mut before = Vec::new();
        for name in names {
            before.push(items_on(
                &*relaunch_as(dir.path(), name).await.selected_profile().await,
            ));
        }

        // A crash or a force quit stops the run where it stands.
        let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
        WIZARD_WRITES_BEFORE_A_CRASH.set(Some(stop));
        let run = tokio::spawn({
            let state = state.clone();
            async move { apply_migration(&state, &[], LIBRARY).await }
        })
        .await;
        WIZARD_WRITES_BEFORE_A_CRASH.set(None);
        assert!(run.is_err(), "stop {stop}");
        let journal = crate::disk::paths::journal_path(dir.path());
        assert!(journal.exists(), "stop {stop}");

        // The next launch writes what the run did not, before
        // anything loads, and says so. Loadout mode used to start
        // over files that still held their items under their old
        // group names, and every character got every item.
        for (n, name) in names.iter().enumerate() {
            let state = relaunch_as(dir.path(), name).await;
            let notices = state.take_launch_notices();
            let finished = [crate::loadouts::wizard::journal::WIZARD_FINISHED_NOTICE.to_string()];
            if n == 0 {
                assert_eq!(notices, finished, "stop {stop}");
            } else {
                assert!(notices.is_empty(), "stop {stop}");
            }
            assert!(state.global_catalog.lock().await.is_some(), "stop {stop}");
            let p = state.selected_profile().await;
            assert_eq!(items_on(&p), before[n], "stop {stop} {name}");
            let leftover = &ProfileConfig::load(&set.profile_path(name))
                .unwrap()
                .aliases;
            assert!(leftover.is_empty(), "{leftover:?}");
        }
        assert!(!journal.exists(), "stop {stop}");
    }
}

#[tokio::test]
async fn a_launch_that_cannot_finish_the_wizard_holds_every_save() {
    use crate::disk::paths::journal_path;
    use crate::loadouts::wizard::journal::WIZARD_UNFINISHED_NOTICE;
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let names = [DEFAULT_PROFILE_NAME, "Healer", "Test-Prompt"];
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    let mut healer = character("Healer", 2, &[]);
    healer.disabled_alias_groups = vec!["combat".into()];
    character(DEFAULT_PROFILE_NAME, 1, &[])
        .save(&set.profile_path(DEFAULT_PROFILE_NAME))
        .unwrap();
    healer.save(&set.profile_path("Healer")).unwrap();
    character("Test-Prompt", 3, &[])
        .save(&set.profile_path("Test-Prompt"))
        .unwrap();
    let mut before = Vec::new();
    for name in names {
        before.push(items_on(
            &*relaunch_as(dir.path(), name).await.selected_profile().await,
        ));
    }

    // The run stops once catalog.toml, loadouts.toml, and the
    // Default file are written.
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    WIZARD_WRITES_BEFORE_A_CRASH.set(Some(3));
    let run = tokio::spawn({
        let state = state.clone();
        async move { apply_migration(&state, &[], LIBRARY).await }
    })
    .await;
    WIZARD_WRITES_BEFORE_A_CRASH.set(None);
    assert!(run.is_err());
    let catalog = read(&catalog_path(dir.path()));

    // At the next launch the Healer file takes no writes, so the
    // run stays unfinished.
    let blocked = set.profile_path("Healer").with_extension("toml.tmp");
    std::fs::create_dir(&blocked).unwrap();
    let healer_file = read(&set.profile_path("Healer"));
    let state = relaunch_as(dir.path(), "Healer").await;
    assert!(state.relaunch_pending.load(Ordering::Acquire));
    assert!(!state.loadout_mode.load(Ordering::Acquire));
    assert_eq!(state.take_launch_notices(), [WIZARD_UNFINISHED_NOTICE]);
    // Loadout mode used to start over the Healer file, which still
    // holds its items under their old groups, so the Healer got
    // every other character's items too. The session runs on the
    // Healer file alone.
    assert!(state.global_catalog.lock().await.is_none());
    assert_eq!(items_on(&*state.selected_profile().await), before[1]);

    // Launch holds every save and every switch, since the next launch
    // writes the journal again over what this one saved.
    state.selected_profile().await.vars.set("target", "dragon");
    {
        let _persist_guard = PERSIST_LOCK.lock().await;
        crate::disk::save::persist_state(&state, &state.selected_session().profile()).await;
    }
    assert_eq!(read(&set.profile_path("Healer")), healer_file);
    assert_eq!(read(&catalog_path(dir.path())), catalog);
    assert!(crate::profile::switch::switch_profile(
        &state,
        &state.selected_session(),
        DEFAULT_PROFILE_NAME
    )
    .await
    .is_err());
    // A rename would move the Healer file away from the name the
    // journal writes it under, and a copy would take its items.
    renames_and_copies_are_refused(&state, dir.path()).await;
    assert!(journal_path(dir.path()).exists());

    // Once the file takes writes again, the next launch finishes
    // the run and each character has on what it had before.
    std::fs::remove_dir(&blocked).unwrap();
    for (n, name) in names.iter().enumerate() {
        let state = relaunch_as(dir.path(), name).await;
        assert!(state.loadout_mode.load(Ordering::Acquire), "{name}");
        assert!(!state.relaunch_pending.load(Ordering::Acquire), "{name}");
        assert_eq!(
            items_on(&*state.selected_profile().await),
            before[n],
            "{name}"
        );
    }
    assert!(!journal_path(dir.path()).exists());
}

#[tokio::test]
async fn the_wizard_waits_while_an_earlier_run_is_unfinished() {
    use crate::disk::paths::{journal_path, legacy_dir};
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    character(DEFAULT_PROFILE_NAME, 1, &[])
        .save(&set.profile_path(DEFAULT_PROFILE_NAME))
        .unwrap();
    character("Healer", 2, &[])
        .save(&set.profile_path("Healer"))
        .unwrap();
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    // The run took the items out of the Default file and then
    // stopped, the way a failed write it could not undo leaves it,
    // with catalog.toml and loadouts.toml put back to nothing.
    WIZARD_WRITES_BEFORE_A_CRASH.set(Some(3));
    let run = tokio::spawn({
        let state = state.clone();
        async move { apply_migration(&state, &[], LIBRARY).await }
    })
    .await;
    WIZARD_WRITES_BEFORE_A_CRASH.set(None);
    assert!(run.is_err());
    std::fs::remove_file(catalog_path(dir.path())).unwrap();
    std::fs::remove_file(loadouts_path(dir.path())).unwrap();
    // You move the legacy folder out, as its refusal says.
    let aside = tempfile::tempdir().unwrap();
    std::fs::rename(legacy_dir(dir.path()), aside.path().join("legacy")).unwrap();
    let journal = read(&journal_path(dir.path()));
    let default_file = read(&set.profile_path(DEFAULT_PROFILE_NAME));

    // A second run would read the Default file without its items
    // and write a journal without them over the one that holds
    // them, so Default would come back with nothing.
    assert_eq!(
        refused(&state).await,
        "Vosh has not finished an earlier move to loadouts. Quit Vosh and open it again \
         to finish it."
    );
    assert_eq!(read(&journal_path(dir.path())), journal);
    assert_eq!(read(&set.profile_path(DEFAULT_PROFILE_NAME)), default_file);
    assert!(!catalog_path(dir.path()).exists());

    // The next launch finishes the run from the journal.
    let before = character(DEFAULT_PROFILE_NAME, 1, &[]);
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    assert!(state.global_catalog.lock().await.is_some());
    let p = state.selected_profile().await;
    assert_eq!(p.aliases.list().len(), 4);
    assert!(p.aliases.get(&before.aliases[0].name).is_some());
}

#[tokio::test]
async fn a_script_that_sets_its_own_alias_again_keeps_it_to_its_character() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    write_alias(&set, DEFAULT_PROFILE_NAME, "kk");
    write_alias(&set, "Healer", "hl");
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    apply_migration(&state, &[], LIBRARY).await.unwrap();

    // A script the Healer runs at launch sets its alias again.
    let state = relaunch_as(dir.path(), "Healer").await;
    let outcome = vosh_script::ScriptOutcome {
        actions: vec![vosh_script::Action::SetAlias {
            name: "hl".into(),
            expansion: "cast heal".into(),
        }],
        ..vosh_script::ScriptOutcome::default()
    };
    let session = state.selected_session();
    crate::script::apply_actions(
        &mut *state.selected_profile().await,
        &mut session.connection.lock(),
        outcome,
    );
    assert_eq!(items_on(&*state.selected_profile().await), ["alias hl"]);
    persist(&state).await;

    // The alias lost its group, so it came on for Default too.
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    assert_eq!(items_on(&*state.selected_profile().await), ["alias kk"]);
}

#[tokio::test]
async fn a_wizard_that_cannot_finish_puts_every_file_back() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let mut set = james_like_set(dir.path());
    set.create("Bard").unwrap();
    set.create("Rich").unwrap();
    // Test-Prompt never saved a file, and every other character
    // holds its own items and settings.
    for (n, name) in [DEFAULT_PROFILE_NAME, "Healer", "Bard", "Rich"]
        .iter()
        .enumerate()
    {
        character(name, n as u32 + 1, &[])
            .save(&set.profile_path(name))
            .unwrap();
    }
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    // Each file as launch and a save of the live profile left it,
    // with its custom themes moved to global.toml. The wizard
    // saves the live profile first, as any save does.
    persist(&state).await;
    let files: Vec<(std::path::PathBuf, Option<String>)> = set
        .list()
        .iter()
        .map(|entry| {
            let path = set.profile_path(&entry.name);
            let text = path.exists().then(|| read(&path));
            (path, text)
        })
        .collect();

    // Something stops the rewrite of the Bard file, after the
    // catalog and the files before it saved, the way a full disk
    // or a lock on the file would.
    let blocked = set.profile_path("Bard").with_extension("toml.tmp");
    std::fs::create_dir(&blocked).unwrap();
    let err = apply_migration(&state, &[], LIBRARY).await.unwrap_err();
    // The raw error text of the failed write, with its colons, no
    // longer shows.
    assert_eq!(
        err,
        "Vosh could not save the Bard profile file, so it put back every file it \
         changed. Your profiles work as before, and you can try again."
    );

    // Nothing changed, so Vosh stays in per profile mode and its
    // saves go on. The copies in legacy are gone too, so they do
    // not hold back the next run.
    assert!(!state.relaunch_pending.load(Ordering::Acquire));
    assert!(!state.loadout_mode.load(Ordering::Acquire));
    assert!(!catalog_path(dir.path()).exists());
    assert!(!loadouts_path(dir.path()).exists());
    let legacy = crate::disk::paths::legacy_dir(dir.path());
    assert_eq!(std::fs::read_dir(&legacy).unwrap().count(), 0);
    assert!(!crate::disk::paths::journal_path(dir.path()).exists());
    for (path, text) in &files {
        match text {
            Some(text) => assert_eq!(&read(path), text, "{}", path.display()),
            None => assert!(!path.exists(), "{}", path.display()),
        }
    }
    let state = relaunch_as(dir.path(), "Bard").await;
    assert!(state.global_catalog.lock().await.is_none());
    {
        let p = state.selected_profile().await;
        assert_eq!(items_on(&p).len(), 6);
        let kept = ProfileConfig::from_profile(&p);
        assert_eq!(kept.profile_vars.get("target").unwrap(), "orc 3");
    }

    // Once the file saves again, the wizard runs.
    std::fs::remove_dir(&blocked).unwrap();
    apply_migration(&state, &[], LIBRARY).await.unwrap();
    assert!(state.relaunch_pending.load(Ordering::Acquire));
    assert!(state.loadout_mode.load(Ordering::Acquire));
    assert!(catalog_path(dir.path()).exists());
    let bard = ProfileConfig::load(&set.profile_path("Bard")).unwrap();
    let leftover = &bard.aliases;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert_eq!(bard.profile_vars.get("target").unwrap(), "orc 3");
}

/// The next step a wizard that could not write its first files
/// gives.
const WRITE_NEXT_STEP: &str =
    "Check that your disk has room and that Vosh can write to its folder, then try again.";

#[tokio::test]
async fn a_wizard_that_cannot_copy_or_journal_says_what_to_do() {
    use crate::disk::paths::{journal_path, legacy_dir};
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    write_alias(&set, "Healer", "hh");
    let state = launch_state(dir.path()).await;
    let healer = read(&set.profile_path("Healer"));

    // A file sits where the legacy folder goes, so no copy lands.
    let legacy = legacy_dir(dir.path());
    std::fs::write(&legacy, "").unwrap();
    let err = apply_migration(&state, &[], LIBRARY).await.unwrap_err();
    assert!(err.starts_with("Vosh could not copy "), "{err}");
    assert!(
        err.ends_with(&format!(
            " into profiles/legacy and changed nothing. {WRITE_NEXT_STEP}"
        )),
        "{err}"
    );
    assert!(!err.contains(':'), "{err}");
    std::fs::remove_file(&legacy).unwrap();

    // The journal does not save.
    let blocked = journal_path(dir.path()).with_extension("toml.tmp");
    std::fs::create_dir(&blocked).unwrap();
    let err = apply_migration(&state, &[], LIBRARY).await.unwrap_err();
    assert_eq!(
        err,
        format!(
            "Vosh could not save catalog.journal.toml and changed nothing. \
             {WRITE_NEXT_STEP}"
        )
    );
    std::fs::remove_dir(&blocked).unwrap();

    // Both changed nothing, so the wizard runs once you fix it.
    assert!(!catalog_path(dir.path()).exists());
    assert_eq!(read(&set.profile_path("Healer")), healer);
    apply_migration(&state, &[], LIBRARY).await.unwrap();
}

#[tokio::test]
async fn a_profile_file_that_stops_reading_is_named_without_the_raw_error() {
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    write_alias(&set, "Healer", "hh");
    let state = launch_state(dir.path()).await;
    // Something puts a folder in place of the Healer file after
    // launch read it.
    let healer = set.profile_path("Healer");
    std::fs::remove_file(&healer).unwrap();
    std::fs::create_dir(&healer).unwrap();
    assert_eq!(
        refused(&state).await,
        "Vosh could not read the Healer profile file, so it changed nothing. Check that \
         you can open the file, then try again."
    );
    assert!(!catalog_path(dir.path()).exists());
}

/// Build the catalog over Default, Healer, and Test-Prompt, each set
/// up as `character` sets it up, and open Vosh again as Default.
/// Returns the live state and the items each character had on,
/// in that order.
async fn converted_three(dir: &std::path::Path) -> (SharedState, Vec<Vec<String>>) {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let set = james_like_set(dir);
    let names = [DEFAULT_PROFILE_NAME, "Healer", "Test-Prompt"];
    for (n, name) in names.iter().enumerate() {
        character(name, n as u32 + 1, &[])
            .save(&set.profile_path(name))
            .unwrap();
    }
    let mut before = Vec::new();
    for name in names {
        let state = relaunch_as(dir, name).await;
        before.push(items_on(&*state.selected_profile().await));
    }
    let state = relaunch_as(dir, DEFAULT_PROFILE_NAME).await;
    apply_migration(&state, &[], LIBRARY).await.unwrap();
    (relaunch_as(dir, DEFAULT_PROFILE_NAME).await, before)
}

/// The catalog items the live stores of `state` hold.
async fn live_rows(state: &SharedState) -> Vec<String> {
    let p = state.selected_profile().await;
    let aliases: Vec<_> = p.aliases.list().into_iter().cloned().collect();
    item_rows(&aliases, &p.triggers.list(), &p.macros)
}

#[tokio::test]
async fn a_save_right_after_a_switch_keeps_the_catalog() {
    let dir = tempfile::tempdir().unwrap();
    let (state, _) = converted_three(dir.path()).await;
    let (catalog, _) = load_at_launch(dir.path()).unwrap();
    let rows = item_rows(&catalog.aliases, &catalog.triggers, &catalog.macros);
    assert_eq!(rows.len(), 18);

    // You switch to Healer while a save waits for the switch to
    // let go of the lock.
    {
        let _persist_guard = PERSIST_LOCK.lock().await;
        crate::profile::switch::switch_live_profile(&state, &state.selected_session(), "Healer")
            .await
            .unwrap();
    }
    persist(&state).await;

    let (saved, _) = load_at_launch(dir.path()).unwrap();
    assert_eq!(
        item_rows(&saved.aliases, &saved.triggers, &saved.macros),
        rows
    );
    assert_eq!(live_rows(&state).await, rows);
}

#[tokio::test]
async fn each_character_keeps_its_own_items_across_switches_after_the_wizard() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let (state, before) = converted_three(dir.path()).await;
    assert_eq!(items_on(&*state.selected_profile().await), before[0]);

    // Corvanne logs in, and Vosh switches to Healer.
    crate::profile::switch::switch_profile(&state, &state.selected_session(), "Healer")
        .await
        .unwrap();
    assert_eq!(items_on(&*state.selected_profile().await), before[1]);
    persist(&state).await;

    // The next launch opens as Healer.
    let state = relaunch_as(dir.path(), "Healer").await;
    assert_eq!(items_on(&*state.selected_profile().await), before[1]);

    // Back to Default, then on to Test-Prompt.
    crate::profile::switch::switch_profile(&state, &state.selected_session(), DEFAULT_PROFILE_NAME)
        .await
        .unwrap();
    assert_eq!(items_on(&*state.selected_profile().await), before[0]);
    crate::profile::switch::switch_profile(&state, &state.selected_session(), "Test-Prompt")
        .await
        .unwrap();
    assert_eq!(items_on(&*state.selected_profile().await), before[2]);
    persist(&state).await;

    // Every save along the way kept each character as it was.
    for (n, name) in [DEFAULT_PROFILE_NAME, "Healer", "Test-Prompt"]
        .iter()
        .enumerate()
    {
        let state = relaunch_as(dir.path(), name).await;
        assert_eq!(
            items_on(&*state.selected_profile().await),
            before[n],
            "{name}"
        );
    }
}

#[tokio::test]
async fn a_switch_keeps_the_items_a_profile_file_holds_as_launch_does() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    loadout_mode(&set, dir.path());
    // The Healer file still holds items of its own.
    let mut healer = ProfileConfig::default();
    healer
        .aliases
        .push(vosh_automation::alias::Alias::new("hh", "heal %1"));
    healer
        .aliases
        .push(vosh_automation::alias::Alias::new("kk", "kick hard %1"));
    healer
        .triggers
        .push(send_trigger("Healer greet", "^hi$", "say hi"));
    healer.macros.push(macro_on("f2", "cast heal"));
    healer.save(&set.profile_path("Healer")).unwrap();
    // The catalog holds an alias, a trigger, and a macro of the
    // same name or key, each with another body.
    let (mut catalog, _) = load_at_launch(dir.path()).unwrap();
    catalog
        .triggers
        .push(send_trigger("Healer greet", "^hello$", "say shared"));
    catalog.macros.push(macro_on("f2", "cast shared"));
    save_global_catalog(dir.path(), &catalog).unwrap();

    // A launch as Healer lays them over the catalog, and the
    // file wins each time.
    let launched = relaunch_as(dir.path(), "Healer").await;
    let rows = live_rows(&launched).await;
    assert_eq!(
        rows,
        item_rows(&healer.aliases, &healer.triggers, &healer.macros)
    );

    // A switch to Healer does the same.
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    crate::profile::switch::switch_profile(&state, &state.selected_session(), "Healer")
        .await
        .unwrap();
    assert_eq!(live_rows(&state).await, rows);

    // And the save after it keeps them for the next launch.
    persist(&state).await;
    let state = relaunch_as(dir.path(), "Healer").await;
    assert_eq!(live_rows(&state).await, rows);
}

/// Save `name`'s file with Disarms and fading buffs on and its line in
/// `color`.
fn write_line_color(set: &ProfileSet, name: &str, color: &str) {
    let mut config = ProfileConfig::default();
    config.ui.enabled_presets = vec!["disarm_buff_fade".into()];
    config.preset_edits = crate::loadouts::preset_edits::lilac_line();
    for edit in config.preset_edits.values_mut() {
        for row in edit.colors.values_mut() {
            row.value = color.into();
        }
    }
    config.save(&set.profile_path(name)).unwrap();
}

/// The color of the line of Disarms and fading buffs in `edits`.
fn line_color(edits: &crate::loadouts::preset_edits::PresetEdits) -> Option<String> {
    let row = edits.get("disarm_buff_fade")?.colors.get("line")?;
    row.value.as_str().map(str::to_string)
}

/// Edits that differ ask which to keep, the one you pick goes to
/// catalog.toml for every character, and the table leaves each profile
/// file (Presets board 5).
#[tokio::test]
async fn the_preset_edits_you_pick_reach_every_character() {
    use super::apply::ConflictResolution;
    use super::plan::ItemKind;
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    write_line_color(&set, DEFAULT_PROFILE_NAME, "#c3a6ff");
    write_line_color(&set, "Healer", "#8fa7d9");
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    let plan = analyze_migration(&state, LIBRARY).await.unwrap();
    let presets: Vec<_> = plan
        .conflicts
        .iter()
        .filter(|c| c.kind == ItemKind::Preset)
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(presets, ["disarm_buff_fade"]);

    let pick = ConflictResolution {
        kind: ItemKind::Preset,
        name: "disarm_buff_fade".into(),
        source_profile: "Healer".into(),
    };
    apply_migration(&state, &[pick], LIBRARY).await.unwrap();
    let (catalog, _) = load_at_launch(dir.path()).unwrap();
    assert_eq!(
        line_color(&catalog.preset_edits).as_deref(),
        Some("#8fa7d9")
    );
    for name in [DEFAULT_PROFILE_NAME, "Healer"] {
        let text = read(&set.profile_path(name));
        assert!(!text.contains("preset_edits"), "{name} {text}");
        let state = relaunch_as(dir.path(), name).await;
        let p = state.selected_profile().await;
        assert_eq!(line_color(&p.preset_edits).as_deref(), Some("#8fa7d9"));
    }
}

/// A run that stops after catalog.toml finishes from its journal with
/// the edits in the catalog and none in a profile file.
#[tokio::test]
async fn a_run_the_journal_finishes_keeps_the_preset_edits() {
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    let dir = tempfile::tempdir().unwrap();
    let set = james_like_set(dir.path());
    write_line_color(&set, DEFAULT_PROFILE_NAME, "#c3a6ff");
    write_line_color(&set, "Healer", "#c3a6ff");
    let state = relaunch_as(dir.path(), DEFAULT_PROFILE_NAME).await;
    // Stop once catalog.toml and loadouts.toml are written.
    WIZARD_WRITES_BEFORE_A_CRASH.set(Some(2));
    let run = tokio::spawn({
        let state = state.clone();
        async move { apply_migration(&state, &[], LIBRARY).await }
    })
    .await;
    WIZARD_WRITES_BEFORE_A_CRASH.set(None);
    assert!(run.is_err());
    assert!(read(&set.profile_path("Healer")).contains("preset_edits"));

    let state = relaunch_as(dir.path(), "Healer").await;
    let p = state.selected_profile().await;
    assert_eq!(line_color(&p.preset_edits).as_deref(), Some("#c3a6ff"));
    for name in [DEFAULT_PROFILE_NAME, "Healer"] {
        let text = read(&set.profile_path(name));
        assert!(!text.contains("preset_edits"), "{name} {text}");
    }
}

//! The import, through a mock app over profiles in a scratch folder.
//! Default, which the selected session plays, claims nothing, and Healer
//! claims a character on The Forsaken Lands. The file is the full golden
//! export, Healer profile.toml, which names Orla.

use std::path::Path;
use std::sync::Arc;

use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, Manager};

use super::*;
use crate::app::state::AppState;
use crate::disk::paths;
use crate::import::vosh::ClashKind;
use crate::profile::set::{ProfileEntry, DEFAULT_PROFILE_NAME};
use crate::profile::tests::{claim, put_claim};
use crate::profile::ui::UiConfig;

const WORLD: &str = "play.theforsakenlands.com";
const EXPORT: &str = include_str!("../../../../../fixtures/config/export.full.toml");

/// A mock app over the profiles in `dir`, with Healer claiming
/// `healer_has`.
async fn app_over(dir: &Path, healer_has: &[&str]) -> (App<MockRuntime>, SharedState) {
    let app = mock_builder().build(mock_context(noop_assets())).unwrap();
    let state: SharedState = Arc::new(AppState::default());
    app.manage::<SharedState>(state.clone());
    let mut set = ProfileSet::load_or_migrate(dir.to_path_buf()).unwrap();
    set.create("Healer").unwrap();
    put_claim(
        &mut set,
        "Healer",
        None,
        claim(WORLD, Some(1848), healer_has),
    );
    state.set_profiles(set).await;
    state.app_data.set(dir.to_path_buf()).unwrap();
    (app, state)
}

/// Import the golden export as `add_as` under `name`, turning on each
/// character in `logins`.
async fn import(
    app: &App<MockRuntime>,
    add_as: AddAs,
    name: &str,
    logins: &[&str],
) -> Result<ImportResult, String> {
    let state = app.state::<SharedState>().inner().clone();
    let logins: Vec<String> = logins.iter().map(ToString::to_string).collect();
    apply_import(
        app.handle(),
        &state,
        "Healer profile.toml",
        EXPORT,
        add_as,
        name,
        &logins,
    )
    .await
}

async fn entry(state: &SharedState, name: &str) -> ProfileEntry {
    let set = state.loaded_profile_set().await.unwrap();
    set.get(name).cloned().unwrap()
}

async fn login_on(state: &SharedState, name: &str) -> bool {
    state.loaded_profile_set().await.unwrap().login_on(name)
}

/// The characters `name` lists and whether its toggle is on.
async fn claim_of(state: &SharedState, name: &str) -> (Vec<String>, bool) {
    let am = entry(state, name).await.auto_match.unwrap();
    (am.characters, am.enabled)
}

fn saved(dir: &Path, name: &str) -> ProfileConfig {
    ProfileConfig::load(&paths::profile_path(dir, name)).unwrap()
}

fn names<T>(items: &[T], key: impl Fn(&T) -> &String) -> Vec<&str> {
    items.iter().map(|item| key(item).as_str()).collect()
}

#[tokio::test]
async fn a_new_profile_takes_the_world_and_a_character_no_profile_has_with_its_login_on() {
    let dir = tempfile::tempdir().unwrap();
    let (app, state) = app_over(dir.path(), &["Maren"]).await;
    let global = std::fs::read_to_string(paths::global_path(dir.path())).unwrap();

    let result = import(&app, AddAs::New, "Healer 2", &["orla"])
        .await
        .unwrap();
    assert_eq!(
        result,
        ImportResult {
            name: "Healer 2".into(),
            moved_from: Vec::new(),
            kept_with: Vec::new(),
            catalog_group: None,
            clashes: Vec::new(),
        }
    );
    let am = entry(&state, "Healer 2").await.auto_match.unwrap();
    assert_eq!((am.host.as_deref(), am.port), (Some(WORLD), Some(1848)));
    assert_eq!(
        claim_of(&state, "Healer 2").await,
        (vec!["Orla".into()], true)
    );
    assert!(login_on(&state, "Healer 2").await);

    // The file holds the export, with its plugins off and your shared
    // theme and font left alone.
    let file = saved(dir.path(), "Healer 2");
    assert_eq!(names(&file.aliases, |a| &a.name), ["kk", "heal"]);
    let plugins = &file.plugins.enabled;
    assert!(plugins.is_empty(), "{plugins:?}");
    let defaults = UiConfig::default();
    assert_eq!(file.ui.theme, defaults.theme);
    assert_eq!(file.ui.font_size, defaults.font_size);
    assert_eq!(
        std::fs::read_to_string(paths::global_path(dir.path())).unwrap(),
        global
    );
    // Nothing else moved.
    assert_eq!(
        claim_of(&state, "Healer").await,
        (vec!["Maren".into()], true)
    );
    assert_eq!(state.selected_profile().await.ui.theme, defaults.theme);
}

#[tokio::test]
async fn a_character_no_profile_has_that_you_turned_off_stays_off_the_list() {
    let dir = tempfile::tempdir().unwrap();
    let (app, state) = app_over(dir.path(), &["Maren"]).await;

    let result = import(&app, AddAs::New, "Healer 2", &[]).await.unwrap();
    assert_eq!(
        (result.moved_from, result.kept_with),
        (Vec::new(), Vec::new())
    );
    // The new profile keeps the world with no character and its login
    // off, so it never takes every login there.
    let am = entry(&state, "Healer 2").await.auto_match.unwrap();
    assert_eq!((am.host.as_deref(), am.port), (Some(WORLD), Some(1848)));
    assert_eq!(claim_of(&state, "Healer 2").await, (Vec::new(), false));
    assert!(!login_on(&state, "Healer 2").await);
}

#[tokio::test]
async fn a_character_another_profile_has_stays_there_until_you_turn_it_on() {
    let dir = tempfile::tempdir().unwrap();
    let (app, state) = app_over(dir.path(), &["Orla"]).await;

    let result = import(&app, AddAs::New, "Healer 2", &[]).await.unwrap();
    assert_eq!(
        result.kept_with,
        [KeptCharacter {
            character: "Orla".into(),
            profile: "Healer".into(),
        }]
    );
    assert_eq!(result.moved_from, []);
    // The new profile keeps the world with no character and its login
    // off, so it never takes every login there.
    assert_eq!(claim_of(&state, "Healer 2").await, (Vec::new(), false));
    assert!(!login_on(&state, "Healer 2").await);
    assert_eq!(
        claim_of(&state, "Healer").await,
        (vec!["Orla".into()], true)
    );
    assert!(login_on(&state, "Healer").await);

    // Turned on, Orla moves, and Healer has no character left.
    let dir = tempfile::tempdir().unwrap();
    let (app, state) = app_over(dir.path(), &["Orla"]).await;
    let result = import(&app, AddAs::New, "Healer 2", &["orla"])
        .await
        .unwrap();
    assert_eq!(
        result.moved_from,
        [MovedCharacter {
            character: "Orla".into(),
            profile: "Healer".into(),
            login_off: true,
        }]
    );
    assert_eq!(result.kept_with, []);
    assert_eq!(
        claim_of(&state, "Healer 2").await,
        (vec!["Orla".into()], true)
    );
    assert!(login_on(&state, "Healer 2").await);
    assert_eq!(claim_of(&state, "Healer").await, (Vec::new(), false));
    assert!(!login_on(&state, "Healer").await);
}

#[tokio::test]
async fn a_taken_name_is_refused_and_nothing_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let (app, state) = app_over(dir.path(), &["Orla"]).await;
    assert_eq!(
        import(&app, AddAs::New, "healer", &["Orla"]).await,
        Err("You already have a profile named Healer.".into())
    );
    assert_eq!(state.loaded_profile_set().await.unwrap().list().len(), 2);
    assert!(!paths::profile_path(dir.path(), "Healer").exists());
    assert_eq!(
        claim_of(&state, "Healer").await,
        (vec!["Orla".into()], true)
    );
}

#[tokio::test]
async fn a_replace_keeps_the_world_the_characters_and_the_plugins_of_your_profile() {
    let dir = tempfile::tempdir().unwrap();
    let (app, state) = app_over(dir.path(), &["Maren"]).await;
    let mut before = ProfileConfig::default();
    before.plugins.enabled = vec!["vitals_alert".into()];
    before
        .aliases
        .push(vosh_automation::alias::Alias::new("rr", "rest"));
    before
        .save(&paths::profile_path(dir.path(), "Healer"))
        .unwrap();
    let claim_before = entry(&state, "Healer").await.auto_match;

    let result = import(&app, AddAs::Replace, "Healer", &["Orla"])
        .await
        .unwrap();
    assert_eq!(result.name, "Healer");
    assert_eq!((result.moved_from, result.kept_with), (vec![], vec![]));

    let file = saved(dir.path(), "Healer");
    assert_eq!(names(&file.aliases, |a| &a.name), ["kk", "heal"]);
    assert_eq!(file.plugins.enabled, ["vitals_alert"]);
    assert_eq!(file.ui.theme, UiConfig::default().theme);
    let claim_after = entry(&state, "Healer").await.auto_match;
    assert_eq!(
        claim_after.map(|am| (am.host, am.port, am.characters, am.enabled)),
        claim_before.map(|am| (am.host, am.port, am.characters, am.enabled))
    );
    // Orla goes nowhere on a replace.
    assert_eq!(state.loaded_profile_set().await.unwrap().list().len(), 2);
}

/// Put the app over `dir` in loadout mode, with a catalog that holds
/// your `kk` in the group an import of `Healer profile.toml` joins, so
/// the file's kk clashes with it. An alias of that name in another
/// group would join beside it.
async fn loadout_mode(dir: &Path, state: &SharedState) {
    use crate::loadouts::catalog::save_global_catalog;
    use crate::loadouts::set::save_loadout_set;
    use vosh_automation::alias::Alias;
    let catalog = GlobalCatalog {
        aliases: vec![Alias {
            group: Some("Healer profile".into()),
            ..Alias::new("kk", "kick %1 twice")
        }],
        enabled_presets: Some(Vec::new()),
        ..GlobalCatalog::default()
    };
    save_global_catalog(dir, &catalog).unwrap();
    save_loadout_set(dir, &LoadoutSet::default()).unwrap();
    assert!(crate::app::launch::load_loadout_mode(state, dir).await);
}

#[tokio::test]
async fn in_loadout_mode_the_items_join_the_catalog_and_a_clash_keeps_yours() {
    use crate::loadouts::catalog::load_global_catalog;
    let dir = tempfile::tempdir().unwrap();
    let (app, state) = app_over(dir.path(), &["Maren"]).await;
    loadout_mode(dir.path(), &state).await;

    let result = import(&app, AddAs::New, "Healer 2", &[]).await.unwrap();
    assert_eq!(result.catalog_group.as_deref(), Some("Healer profile"));
    assert_eq!(
        result.clashes,
        [Clash {
            kind: ClashKind::Alias,
            name: "kk".into(),
        }]
    );

    // catalog.toml gains the group, and keeps your kk.
    let saved_catalog = load_global_catalog(dir.path()).unwrap();
    let group = |g: &Option<String>| g.as_deref() == Some("Healer profile");
    let kk = saved_catalog
        .aliases
        .iter()
        .find(|a| a.name == "kk")
        .unwrap();
    assert_eq!(
        (kk.expansion.as_str(), kk.group.as_deref()),
        ("kick %1 twice", Some("Healer profile"))
    );
    let heal = saved_catalog
        .aliases
        .iter()
        .find(|a| a.name == "heal")
        .unwrap();
    assert!(group(&heal.group));
    let triggers: Vec<&str> = saved_catalog
        .triggers
        .iter()
        .filter(|t| group(&t.group))
        .map(|t| t.name.as_str())
        .collect();
    // The preset trigger tells stays out with the list and the edits of
    // the file, so the presets stay as the catalog has them.
    assert_eq!(triggers, ["spam", "room-items"]);
    assert!(saved_catalog.triggers.iter().all(|t| t.preset.is_none()));
    assert_eq!(saved_catalog.enabled_presets, Some(Vec::new()));
    let edits = &saved_catalog.preset_edits;
    assert!(edits.is_empty(), "{edits:?}");
    // Your macros join in the file's group. Its preset macros stay out,
    // since a launch installs the catalog's own.
    assert_eq!(
        names(&saved_catalog.macros, |m| &m.key),
        ["F1", "F2", "Numpad3"]
    );
    assert!(saved_catalog.macros.iter().all(|m| group(&m.group)));

    // The new profile file holds none of them, and the live profile does.
    let file = saved(dir.path(), "Healer 2");
    assert!(
        file.aliases.is_empty() && file.triggers.is_empty() && file.macros.is_empty(),
        "{file:?}"
    );
    assert_eq!(
        file.ui.enabled_presets,
        [crate::loadouts::presets::PRESETS_OFF]
    );
    assert!(file.preset_edits.is_empty(), "{:?}", file.preset_edits);
    assert_eq!(file.timers.len(), 1);
    assert!(state.selected_profile().await.aliases.get("heal").is_some());
}

#[tokio::test]
async fn in_loadout_mode_your_macro_joins_beside_the_preset_macro_on_its_key() {
    use crate::loadouts::catalog::load_global_catalog;
    use crate::loadouts::presets::install_preset_macros;
    let dir = tempfile::tempdir().unwrap();
    let (app, state) = app_over(dir.path(), &["Maren"]).await;
    loadout_mode(dir.path(), &state).await;
    // You turned Numpad movement on, two of its macros for short.
    let numpad = |key: &str, command: &str| Macro {
        key: key.into(),
        command: command.into(),
        group: None,
        enabled: true,
        preset: Some("numpad_movement".into()),
    };
    install_preset_macros(
        &mut *state.selected_profile().await,
        vec![numpad("Numpad8", "n"), numpad("Numpad3", "d")],
    )
    .unwrap();

    let result = import(&app, AddAs::New, "Healer 2", &[]).await.unwrap();
    assert_eq!(
        result.clashes,
        [Clash {
            kind: ClashKind::Alias,
            name: "kk".into(),
        }]
    );
    // Your Numpad3 joins and keeps the key, so the preset's d waits. The
    // file's own preset macros stay out.
    let saved_catalog = load_global_catalog(dir.path()).unwrap();
    let rows: Vec<(&str, &str, bool, bool)> = saved_catalog
        .macros
        .iter()
        .map(|m| {
            (
                m.key.as_str(),
                m.command.as_str(),
                m.enabled,
                m.preset.is_some(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("Numpad8", "n", true, true),
            ("Numpad3", "d", false, true),
            ("F1", "score", true, false),
            ("F2", "flee", false, false),
            ("Numpad3", "rec", true, false),
        ]
    );
}

#[tokio::test]
async fn in_loadout_mode_a_replace_of_the_profile_you_play_keeps_the_catalog() {
    use crate::loadouts::catalog::load_global_catalog;
    let dir = tempfile::tempdir().unwrap();
    let (app, state) = app_over(dir.path(), &["Maren"]).await;
    loadout_mode(dir.path(), &state).await;

    let result = import(&app, AddAs::Replace, DEFAULT_PROFILE_NAME, &[])
        .await
        .unwrap();
    assert_eq!(result.catalog_group.as_deref(), Some("Healer profile"));

    // The profile takes the file's settings over the catalog it held, and
    // the file's items join it.
    {
        let p = state.selected_profile().await;
        let kk = p.aliases.get("kk").unwrap();
        assert_eq!(kk.expansion, "kick %1 twice");
        assert!(p.aliases.get("heal").is_some());
        assert_eq!(p.timers.len(), 1);
        assert_eq!(p.vars.get("target"), Some("orc"));
    }
    let saved_catalog = load_global_catalog(dir.path()).unwrap();
    assert_eq!(names(&saved_catalog.aliases, |a| &a.name), ["heal", "kk"]);
    let file = saved(dir.path(), DEFAULT_PROFILE_NAME);
    assert!(
        file.aliases.is_empty() && file.triggers.is_empty() && file.macros.is_empty(),
        "{file:?}"
    );
    assert_eq!(file.profile_vars.len(), 2);
}

//! The order launch runs its upgrades in (R2 of the refactor plan).
//!
//! The setup steps in app/launch.rs run `load`. It finishes a shared
//! catalog wizard run that stopped and reads the profile set. Then it
//! moves the prompt capture triggers into the profiles, turns on the
//! presets a build adds, and moves the custom themes older profile files
//! hold into global.toml, and only then loads the active profile. Each
//! step reads what the steps before it wrote, so the order is part of what
//! a refactor keeps. `disk::upgrades::run` holds the steps after the read
//! as one ordered list.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use vosh_prompt::config::CaptureSource;
use vosh_prompt::CaptureConfig;

use crate::app::state::{AppState, SharedState};
use crate::disk::paths::journal_path;
use crate::disk::save::{persist_state, PERSIST_LOCK};
use crate::loadouts::catalog::load_global_catalog;
use crate::loadouts::wizard::journal::{
    save_wizard_journal, JournalFile, WizardJournal, WIZARD_FINISHED_NOTICE,
    WIZARD_UNFINISHED_NOTICE,
};
use crate::profile::file::{before_prompt_editor_path, ProfileConfig};
use crate::profile::set::ProfileSet;
use crate::profile::shared::GlobalConfig;
use crate::profile::ui::CustomTheme;

/// The profile file the stopped wizard run still had to write: a capture
/// trigger that is on, and one preset turned on.
const WIZARD_PROFILE: &str = r#"aliases = []
macros = []

[[triggers]]
name = "prompt-capture"
pattern = '\[(?<hp>\d+)/(?<maxhp>\d+)hp (?<mana>\d+)/(?<maxmana>\d+)mn (?<move>\d+)/(?<maxmove>\d+)mv\]'
priority = 100
enabled = true

[[triggers.patterns]]
pattern = '\[(?<hp>\d+)/(?<maxhp>\d+)hp (?<mana>\d+)/(?<maxmana>\d+)mn (?<move>\d+)/(?<maxmove>\d+)mv\]'
enabled = true

[[triggers.actions]]
kind = "gag"

[[triggers.actions]]
kind = "script"
body = """
mud.set_prompt_var(\"hp\", captures[2])
mud.set_prompt_var(\"maxhp\", captures[3])
mud.set_prompt_var(\"mana\", captures[4])
mud.set_prompt_var(\"maxmana\", captures[5])
mud.set_prompt_var(\"move\", captures[6])
mud.set_prompt_var(\"maxmove\", captures[7])"""

[ui]
enabled_presets = ["healing_basics"]
"#;

/// The catalog the stopped run still had to write, with the same preset
/// on.
const WIZARD_CATALOG: &str = r#"aliases = []
triggers = []
macros = []
enabled_presets = ["healing_basics"]
"#;

/// A second profile's file as an older build saved it, with a custom
/// theme of its own.
const ALT_PROFILE: &str = r##"aliases = []
macros = []
triggers = []

[[ui.custom_themes]]
id = "custom-dusk"
label = "Dusk"
description = "A warm dark theme"

[ui.custom_themes.xterm]
background = "#1a1b26"
foreground = "#c0caf5"
"##;

const WIZARD_LOADOUTS: &str = r#"active = ["default"]
dormant = false

[[loadouts]]
name = "default"
enabled_groups = []
"#;

/// What the prompt upgrade tells you when it moved the capture into the
/// default profile.
const MOVED_INTO_DEFAULT: &str = "Vosh moved your prompt capture into the Default profile. \
     Profiles that do not draw a prompt now show the game's prompt.";

fn journal() -> WizardJournal {
    WizardJournal {
        catalog: WIZARD_CATALOG.to_string(),
        loadouts: WIZARD_LOADOUTS.to_string(),
        profiles: vec![JournalFile {
            path: "profiles/default.toml".to_string(),
            text: WIZARD_PROFILE.to_string(),
        }],
    }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The one-time steps profiles.toml records, in the order they ran.
fn migrations(app_data: &Path) -> Vec<String> {
    let index: toml::Table = read(&app_data.join("profiles.toml")).parse().unwrap();
    index
        .get("migrations")
        .and_then(toml::Value::as_array)
        .map(|list| {
            list.iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn theme_ids(themes: &[CustomTheme]) -> Vec<&str> {
    themes.iter().map(|t| t.id.as_str()).collect()
}

/// The ids of the custom themes global.toml shares.
fn shared_theme_ids(set: &ProfileSet) -> Vec<String> {
    let global = GlobalConfig::load(&set.global_path()).unwrap();
    let themes = global
        .custom_themes
        .expect("global.toml lists custom themes");
    themes.into_iter().map(|t| t.id).collect()
}

/// Run `app::launch::load` over `app_data` on a fresh state, as the
/// setup steps in app/launch.rs do.
async fn launch(app_data: &Path) -> SharedState {
    let state: SharedState = Arc::new(AppState::default());
    crate::app::launch::load(&state, app_data).await;
    state
}

#[tokio::test]
async fn launch_runs_the_upgrades_in_order() {
    let dir = tempfile::tempdir().unwrap();
    // A wizard run that stopped before it wrote any file, and a second
    // profile whose file holds a custom theme. The theme scope stays at
    // its global default.
    let app_data = dir.path().join("com.example.vosh");
    save_wizard_journal(&app_data, &journal()).unwrap();
    let mut set = ProfileSet::load_or_migrate(app_data.clone()).unwrap();
    set.create("alt").unwrap();
    std::fs::write(set.profile_path("alt"), ALT_PROFILE).unwrap();
    let profile_path = set.profile_path("default");

    let state = launch(&app_data).await;
    assert!(state.loadout_mode.load(Ordering::Acquire));
    assert!(!state.relaunch_pending.load(Ordering::Acquire));

    // 1. The wizard finish ran first and wrote every file the journal
    //    names.
    assert!(!journal_path(&app_data).exists(), "the run finished");

    // 2. The wizard finish ran before the prompt upgrade. The upgrade kept
    //    the file as it found it before its first [prompt], which is the
    //    file the wizard wrote, byte for byte, and moved its capture.
    assert_eq!(
        read(&before_prompt_editor_path(&profile_path)),
        WIZARD_PROFILE
    );
    let file = ProfileConfig::load(&profile_path).unwrap();
    match &file.prompt_config().capture {
        CaptureConfig::Regex(capture) => {
            assert_eq!(capture.source, Some(CaptureSource::Migrated));
        }
        other => panic!("the moved capture, got {other:?}"),
    }
    assert!(
        file.triggers.iter().all(|t| !t.enabled),
        "the trigger is off"
    );
    // The launch notices come in the same order.
    assert_eq!(
        state.take_launch_messages(),
        [WIZARD_FINISHED_NOTICE, MOVED_INTO_DEFAULT]
    );

    // 3. The prompt upgrade ran before the preset rollout, and each
    //    recorded itself as it finished.
    assert_eq!(
        migrations(&app_data),
        [
            "prompt-capture-to-profile",
            "preset-sent-tells-on",
            "preset-room-and-time-on"
        ]
    );

    // 4. The wizard finish ran before the rollout, so the catalog and the
    //    profile file it wrote both took the new preset.
    let with_rollout = ["healing_basics", "sent_tells", "room_and_time"];
    assert_eq!(file.ui.enabled_presets, with_rollout);
    assert_eq!(
        load_global_catalog(&app_data)
            .unwrap()
            .enabled_presets
            .unwrap(),
        with_rollout
    );

    // 5. The custom theme move took the theme out of the alt file and
    //    into global.toml.
    let alt = ProfileConfig::load(&set.profile_path("alt")).unwrap();
    assert!(
        alt.ui.custom_themes.is_empty(),
        "the theme left the alt file"
    );
    assert_eq!(shared_theme_ids(&set), ["custom-dusk"]);

    // 6. The active profile loaded last, so the live profile holds what
    //    every step wrote, the moved theme too.
    {
        let p = state.selected_profile().await;
        assert_eq!(p.ui.enabled_presets, with_rollout);
        assert!(p.prompt.capture.is_migrated());
        let trigger = p.triggers.get("prompt-capture").expect("the moved trigger");
        assert!(!trigger.enabled);
        assert_eq!(theme_ids(&p.ui.custom_themes), ["custom-dusk"]);
    }

    // 7. The next save writes global.toml from the live list, so the theme
    //    stays.
    {
        let _persist = PERSIST_LOCK.lock().await;
        persist_state(&state, &state.selected_session().profile()).await;
    }
    assert_eq!(shared_theme_ids(&set), ["custom-dusk"]);
}

#[tokio::test]
async fn an_unfinished_wizard_run_holds_the_upgrades_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let app_data = dir.path().join("com.example.vosh");
    save_wizard_journal(&app_data, &journal()).unwrap();
    // The profile file takes no writes, so the run stays unfinished.
    let blocked = app_data.join("profiles").join("default.toml.tmp");
    std::fs::create_dir_all(&blocked).unwrap();

    let state = launch(&app_data).await;
    assert!(state.relaunch_pending.load(Ordering::Acquire));
    assert!(!state.loadout_mode.load(Ordering::Acquire));
    assert_eq!(state.take_launch_messages(), [WIZARD_UNFINISHED_NOTICE]);
    // The prompt upgrade and the rollout wait for the run, and the
    // profiles still load.
    let leftover = &migrations(&app_data);
    assert!(leftover.is_empty(), "{leftover:?}");
    assert!(state.profile_set.lock().await.is_some());

    // Once the file takes writes, the next launch finishes the run, then
    // runs the upgrades after it in order.
    std::fs::remove_dir(&blocked).unwrap();
    let state = launch(&app_data).await;
    assert!(state.loadout_mode.load(Ordering::Acquire));
    assert!(!state.relaunch_pending.load(Ordering::Acquire));
    assert_eq!(
        state.take_launch_messages(),
        [WIZARD_FINISHED_NOTICE, MOVED_INTO_DEFAULT]
    );
    assert_eq!(
        migrations(&app_data),
        [
            "prompt-capture-to-profile",
            "preset-sent-tells-on",
            "preset-room-and-time-on"
        ]
    );
}

/// A launch never reads the app data an older `com.aabahran.mudclient`
/// install left behind, so a wizard run that folder holds never reaches
/// Vosh.
#[tokio::test]
async fn a_launch_leaves_an_older_mudclient_folder_alone() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("com.example.mudclient");
    let app_data = dir.path().join("com.example.vosh");
    save_wizard_journal(&old, &journal()).unwrap();

    let state = launch(&app_data).await;
    assert!(journal_path(&old).exists(), "the older folder stays");
    assert!(!journal_path(&app_data).exists(), "nothing was copied");
    assert!(!app_data.join("catalog.toml").exists(), "no wizard run");
    assert!(!state.loadout_mode.load(Ordering::Acquire));
    assert!(!state.relaunch_pending.load(Ordering::Acquire));
    let leftover = &state.take_launch_messages();
    assert!(leftover.is_empty(), "{leftover:?}");
}

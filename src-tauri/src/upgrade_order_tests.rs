//! The order launch runs its upgrades in (R2 of the refactor plan).
//!
//! The setup hook in lib.rs copies the app data of an older
//! `com.aabahran.mudclient` install first. Then `launch::load` finishes a
//! shared catalog wizard run that stopped, moves the prompt capture
//! triggers into the profiles, turns on the presets a build adds, and
//! only then loads the profiles. Each step reads what the steps before it
//! wrote, so the order is part of what a refactor keeps. R13 gathers the
//! steps into one list, and this order holds there too.

use std::path::Path;
use std::sync::Arc;

use vosh_prompt::config::CaptureSource;
use vosh_prompt::CaptureConfig;

use crate::commands::{AppState, SharedState};
use crate::loadout_store::{
    journal_path, load_global_catalog, save_wizard_journal, JournalFile, WizardJournal,
    WIZARD_FINISHED_NOTICE, WIZARD_UNFINISHED_NOTICE,
};
use crate::profile_config::{before_prompt_editor_path, ProfileConfig};

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

/// Launch the way the setup hook in lib.rs does, over `app_data`.
async fn launch(app_data: &Path) -> (SharedState, crate::launch::Launch) {
    crate::migrate_from_mudclient_dir(app_data);
    let state: SharedState = Arc::new(AppState::default());
    let launched = crate::launch::load(&state, app_data).await;
    (state, launched)
}

#[tokio::test]
async fn launch_runs_the_upgrades_in_order() {
    let dir = tempfile::tempdir().unwrap();
    // An older install whose wizard run stopped before it wrote any file,
    // and no Vosh folder yet.
    let old = dir.path().join("com.example.mudclient");
    let app_data = dir.path().join("com.example.vosh");
    save_wizard_journal(&old, &journal()).unwrap();
    let profile_path = app_data.join("profiles").join("default.toml");

    let (state, launched) = launch(&app_data).await;
    assert!(launched.loadout_mode);
    assert!(!launched.wizard_unfinished);

    // 1. The mudclient copy ran first, so the wizard finish found the
    //    journal the older folder held and wrote every file it names.
    assert!(app_data.join(".migrated-from-mudclient").exists());
    assert!(
        journal_path(&old).exists(),
        "the copy leaves the older folder"
    );
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
        state.take_launch_notices(),
        [WIZARD_FINISHED_NOTICE, MOVED_INTO_DEFAULT]
    );

    // 3. The prompt upgrade ran before the preset rollout, and each
    //    recorded itself as it finished.
    assert_eq!(
        migrations(&app_data),
        ["prompt-capture-to-profile", "preset-sent-tells-on"]
    );

    // 4. The wizard finish ran before the rollout, so the catalog and the
    //    profile file it wrote both took the new preset.
    let with_rollout = ["healing_basics", "sent_tells"];
    assert_eq!(file.ui.enabled_presets, with_rollout);
    assert_eq!(
        load_global_catalog(&app_data)
            .unwrap()
            .enabled_presets
            .unwrap(),
        with_rollout
    );

    // 5. The profiles loaded last, so the live profile holds what every
    //    step wrote.
    let p = state.profile.lock().await;
    assert_eq!(p.ui.enabled_presets, with_rollout);
    assert!(p.prompt.config().capture.is_migrated());
    let trigger = p.triggers.get("prompt-capture").expect("the moved trigger");
    assert!(!trigger.enabled);
}

#[tokio::test]
async fn an_unfinished_wizard_run_holds_the_upgrades_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let app_data = dir.path().join("com.example.vosh");
    save_wizard_journal(&app_data, &journal()).unwrap();
    // The profile file takes no writes, so the run stays unfinished.
    let blocked = app_data.join("profiles").join("default.toml.tmp");
    std::fs::create_dir_all(&blocked).unwrap();

    let (state, launched) = launch(&app_data).await;
    assert!(launched.wizard_unfinished);
    assert!(!launched.loadout_mode);
    assert_eq!(state.take_launch_notices(), [WIZARD_UNFINISHED_NOTICE]);
    // The prompt upgrade and the rollout wait for the run, and the
    // profiles still load.
    assert!(migrations(&app_data).is_empty());
    assert!(state.profile_set.lock().await.is_some());

    // Once the file takes writes, the next launch finishes the run, then
    // runs the upgrades after it in order.
    std::fs::remove_dir(&blocked).unwrap();
    let (state, launched) = launch(&app_data).await;
    assert!(launched.loadout_mode);
    assert!(!launched.wizard_unfinished);
    assert_eq!(
        state.take_launch_notices(),
        [WIZARD_FINISHED_NOTICE, MOVED_INTO_DEFAULT]
    );
    assert_eq!(
        migrations(&app_data),
        ["prompt-capture-to-profile", "preset-sent-tells-on"]
    );
}

/// The setup hook cannot run without a Tauri app, so this reads lib.rs to
/// pin that it copies the mudclient folder before `launch::load` runs.
/// D3 keeps or retires the copy and its guard in one commit, and this test
/// changes in that commit too.
#[test]
fn the_setup_hook_copies_the_mudclient_folder_before_launch_loads() {
    let src = include_str!("lib.rs");
    let hook = &src[src.find(".setup(move |app|").expect("the setup hook")..];
    let copy = hook
        .find("migrate_from_mudclient_dir(&path);")
        .expect("the setup hook copies the mudclient folder");
    let load = hook
        .find("launch::load(&state, &path)")
        .expect("the setup hook runs launch::load");
    assert!(
        copy < load,
        "the mudclient folder copy runs before launch::load"
    );
}

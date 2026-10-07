//! The tests of live.rs, and the helpers that more than one profile
//! test module shares.

use std::collections::BTreeMap;

use tempfile::tempdir;

use super::live::{Macro, Profile};
use super::login_match::AutoMatch;
use crate::profile::file::ProfileConfig;
use crate::profile::set::{ProfileSet, DEFAULT_PROFILE_NAME};
use crate::profile::shared::{strip_global_fields, GlobalConfig};
use crate::profile::ui::CustomTheme;

pub(crate) fn theme(id: &str, background: &str) -> CustomTheme {
    CustomTheme {
        id: id.into(),
        label: id.into(),
        description: String::new(),
        xterm: [("background".to_string(), background.to_string())]
            .into_iter()
            .collect(),
        chrome: BTreeMap::new(),
        fitted: BTreeMap::new(),
    }
}

/// A profile with every theme and font scope field off its default.
pub(crate) fn styled_profile() -> Profile {
    let mut profile = Profile::default();
    profile.ui.theme = "night-ink".into();
    profile.ui.follow_system_appearance = true;
    profile.ui.light_theme = "classic-vivid".into();
    profile.ui.dark_theme = "night-ink".into();
    profile.ui.theme_follow = "system".into();
    profile.ui.day_theme = "classic-vivid".into();
    profile.ui.night_theme = "night-ink".into();
    profile.ui.custom_themes = vec![theme("night-ink", "#000000")];
    profile.ui.font_size = 16;
    profile.ui.terminal_line_height = "loose".into();
    profile
}

/// Every shared setting off its default, custom themes included.
pub(crate) fn shared_profile() -> Profile {
    let mut profile = styled_profile();
    profile.ui.font_family = "Iosevka".into();
    profile.ui.keep_last_command = true;
    profile.ui.auto_update = true;
    profile
}

pub(crate) fn theme_ids(themes: &[CustomTheme]) -> Vec<&str> {
    themes.iter().map(|t| t.id.as_str()).collect()
}

/// Mirror `persist_profile` for the active profile.
pub(crate) fn persist_live(set: &ProfileSet, profile: &Profile) {
    let mut snapshot = ProfileConfig::from_profile(profile);
    strip_global_fields(&mut snapshot, set.scope());
    snapshot.save(&set.active_path()).unwrap();
    GlobalConfig::from_profile(profile, set.scope())
        .save(&set.global_path())
        .unwrap();
}

pub(crate) fn claim(host: &str, port: Option<u16>, characters: &[&str]) -> AutoMatch {
    AutoMatch {
        host: Some(host.into()),
        port,
        characters: characters.iter().map(ToString::to_string).collect(),
        enabled: true,
    }
}

/// Write `name`'s description and claim as they stand, the way an
/// index saved before the login rules holds them, so a test can set
/// up a double claim or a host wide fallback.
pub(crate) fn put_claim(
    set: &mut ProfileSet,
    name: &str,
    description: Option<&str>,
    auto_match: AutoMatch,
) {
    let entry = set
        .index
        .profiles
        .iter_mut()
        .find(|p| p.name == name)
        .unwrap();
    entry.description = description.map(ToString::to_string);
    entry.auto_match = Some(auto_match);
    set.save_index().unwrap();
}

/// James's index: default and Test-Prompt both claim Ilsabet on the
/// same world, and Healer claims Corvanne.
pub(crate) fn james_like_set(dir: &std::path::Path) -> ProfileSet {
    let mut set = ProfileSet::load_or_migrate(dir.to_path_buf()).unwrap();
    let world = "play.theforsakenlands.com";
    put_claim(
        &mut set,
        DEFAULT_PROFILE_NAME,
        Some("Immortal"),
        claim(world, Some(1848), &["Ilsabet"]),
    );
    set.create("Healer").unwrap();
    put_claim(
        &mut set,
        "Healer",
        None,
        claim(world, Some(1848), &["Corvanne"]),
    );
    set.create("Test-Prompt").unwrap();
    put_claim(
        &mut set,
        "Test-Prompt",
        None,
        claim(world, Some(1848), &["Ilsabet"]),
    );
    set
}

pub(crate) fn set_with_profiles(profiles: Vec<(&str, AutoMatch)>) -> ProfileSet {
    let dir = tempdir().unwrap();
    let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
    for (name, am) in profiles {
        if name != DEFAULT_PROFILE_NAME {
            set.create(name).unwrap();
        }
        put_claim(&mut set, name, None, am);
    }
    set
}

#[test]
fn macro_without_enabled_reads_as_on() {
    let m: Macro = serde_json::from_str(r#"{"key":"F1","command":"kick"}"#).unwrap();
    assert!(m.enabled);
    assert_eq!(m.group, None);
}

#[test]
fn macro_omits_enabled_while_on_and_keeps_it_off() {
    let on = Macro {
        key: "F1".into(),
        command: "kick".into(),
        group: None,
        enabled: true,
        preset: None,
    };
    let json = serde_json::to_string(&on).unwrap();
    assert!(!json.contains("enabled"), "{json}");

    let off = Macro {
        enabled: false,
        ..on
    };
    let json = serde_json::to_string(&off).unwrap();
    assert!(json.contains(r#""enabled":false"#), "{json}");
    let back: Macro = serde_json::from_str(&json).unwrap();
    assert_eq!(back, off);
}

#[test]
fn macro_enabled_round_trips_through_toml() {
    #[derive(serde::Serialize, serde::Deserialize)]
    struct Holder {
        macros: Vec<Macro>,
    }
    let holder = Holder {
        macros: vec![
            Macro {
                key: "F1".into(),
                command: "kick".into(),
                group: Some("combat".into()),
                enabled: false,
                preset: None,
            },
            Macro {
                key: "F2".into(),
                command: "bash".into(),
                group: None,
                enabled: true,
                preset: None,
            },
        ],
    };
    let text = toml::to_string(&holder).unwrap();
    let back: Holder = toml::from_str(&text).unwrap();
    assert_eq!(back.macros, holder.macros);
}

#[test]
fn timer_groups_round_trip_through_the_profile_file() {
    let mut profile = Profile::default();
    profile.timers.push(super::live::Timer {
        id: 1,
        name: "drink".into(),
        interval_secs: 60,
        command: "drink water".into(),
        enabled: true,
        group: Some("upkeep".into()),
    });
    profile.disabled_timer_groups.insert("upkeep".into());
    let text = ProfileConfig::from_profile(&profile).to_toml().unwrap();
    assert!(
        text.contains("disabled_timer_groups = [\"upkeep\"]"),
        "{text}"
    );
    assert!(text.contains("group = \"upkeep\""), "{text}");
    let mut back = Profile::default();
    let warnings = ProfileConfig::from_toml(&text).unwrap().apply_to(&mut back);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(back.timers, profile.timers);
    assert_eq!(back.disabled_timer_groups, profile.disabled_timer_groups);
    assert!(!back.timer_fires(&back.timers[0]));
    // A file with neither reads every timer as on and in no group.
    let bare =
        "[[timers]]\nid = 1\ninterval_secs = 60\ncommand = \"drink water\"\nenabled = true\n";
    let mut old = Profile::default();
    ProfileConfig::from_toml(bare).unwrap().apply_to(&mut old);
    assert_eq!(old.timers[0].group, None);
    assert!(old.timer_fires(&old.timers[0]));
}

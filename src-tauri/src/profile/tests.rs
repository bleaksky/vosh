//! The tests of live.rs, and the helpers that the tests of more than
//! one profile file share.

use std::collections::BTreeMap;

use tempfile::tempdir;

use super::login_match::AutoMatch;
use super::{Macro, Profile};
use crate::profile_config::{strip_global_fields, CustomTheme, GlobalConfig, ProfileConfig};
use crate::profile_set::{ProfileSet, DEFAULT_PROFILE_NAME};

pub(crate) fn theme(id: &str, background: &str) -> CustomTheme {
    CustomTheme {
        id: id.into(),
        label: id.into(),
        description: String::new(),
        xterm: [("background".to_string(), background.to_string())]
            .into_iter()
            .collect(),
        chrome: BTreeMap::new(),
    }
}

/// A profile with every theme and font scope field off its default.
pub(crate) fn styled_profile() -> Profile {
    let mut profile = Profile::default();
    profile.ui.theme = "night-ink".into();
    profile.ui.follow_system_appearance = true;
    profile.ui.light_theme = "classic-vivid".into();
    profile.ui.dark_theme = "night-ink".into();
    profile.ui.custom_themes = vec![theme("night-ink", "#000000")];
    profile.ui.font_size = 16;
    profile.ui.terminal_line_height = "loose".into();
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
            },
            Macro {
                key: "F2".into(),
                command: "bash".into(),
                group: None,
                enabled: true,
            },
        ],
    };
    let text = toml::to_string(&holder).unwrap();
    let back: Holder = toml::from_str(&text).unwrap();
    assert_eq!(back.macros, holder.macros);
}

//! The tests of profile.rs, and the helpers that the tests of more than
//! one profile file share.

use std::collections::BTreeMap;

use super::{Macro, Profile};
use crate::profile_config::{strip_global_fields, CustomTheme, GlobalConfig, ProfileConfig};
use crate::profile_set::ProfileSet;

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

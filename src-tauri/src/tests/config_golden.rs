//! Config golden files.
//!
//! Each golden in `fixtures/config` holds the exact bytes Vosh writes for
//! one config file: a profile file, global.toml, loadouts.toml,
//! catalog.toml and profiles.toml, once at their defaults and once with
//! every field set. A change that renames, reorders, drops or retypes a
//! field fails here before it reaches a file on your disk. Each golden is
//! written through the save function Vosh uses, and reading it back and
//! saving it again gives the same bytes.
//!
//! `first-save` holds the three files a fresh install writes on its first
//! save, through the launch and the save Vosh runs. It keeps
//! `profiles/default.toml` as `default-profile.toml`, since .gitignore
//! leaves out every `profiles/` folder.
//!
//! `old` holds files older builds wrote. They never change, and each
//! still loads with what it says.
//!
//! The golden rule: a golden changes only in a commit tied to a numbered
//! bug or a lettered decision, and that commit shows the byte diff. Run
//! with `VOSH_WRITE_CONFIG=1` to write the goldens again, then read the
//! diff. The old inputs are never written.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use vosh_automation::alert::{AlertParts, Attention};
use vosh_automation::alias::Alias;
use vosh_automation::trigger::{
    HighlightStyle, MatchMode, NamedColor, Trigger, TriggerAction, TriggerPattern, TriggerTarget,
};
use vosh_automation::StopKey;
use vosh_prompt::config::{AabahranCapture, CaptureSource, RegexCapture};
use vosh_prompt::{CaptureConfig, PromptConfig, PromptShow};

use crate::app::state::{AppState, SharedState};
use crate::disk::paths::{catalog_path, loadouts_path};
use crate::disk::save::PERSIST_LOCK;
use crate::loadouts::catalog::{load_global_catalog, save_global_catalog, GlobalCatalog};
use crate::loadouts::preset_edits::{EditRow, PresetEdit, PresetEdits};
use crate::loadouts::set::{load_loadout_set, save_loadout_set, Loadout, LoadoutSet};
use crate::profile::export::{self, VoshExport};
use crate::profile::file::{GroupFolders, OnSwitch, PluginsPersist, ProfileConfig};
use crate::profile::live::{Macro, Timer};
use crate::profile::login_match::AutoMatch;
use crate::profile::panes::{DockEntryPersist, PaneLayoutPersist, PaneNode};
use crate::profile::set::{GetStarted, ProfileEntry, ProfileSet, ProfilesIndex, SessionEntry};
use crate::profile::shared::{GlobalConfig, Scope, ScopeConfig};
use crate::profile::tests::claim;
use crate::profile::ui::{CustomTheme, TrackedAffect, UiConfig, VitalsConfig};
use crate::sessions::SessionId;
use crate::tick::TickConfig;

/// The folder that holds the goldens and the old inputs.
const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/config");

/// The switch that writes the goldens again.
const WRITE: &str = "VOSH_WRITE_CONFIG";

/// Every golden, by its path under `fixtures/config`.
const GOLDENS: [&str; 17] = [
    "profile.default.toml",
    "profile.fresh.toml",
    "profile.full.toml",
    "profile.full-regex.toml",
    "export.full.toml",
    "global.default.toml",
    "global.full.toml",
    "loadouts.default.toml",
    "loadouts.full.toml",
    "catalog.default.toml",
    "catalog.full.toml",
    "profiles.default.toml",
    "profiles.full.toml",
    "profiles.sessions.toml",
    "first-save/global.toml",
    "first-save/profiles.toml",
    "first-save/default-profile.toml",
];

/// Every old input, by its path under `fixtures/config`, with the FNV-1a
/// digest of its bytes. An old input never changes, so its digest never
/// does either.
const OLD_INPUTS: [(&str, u64); 11] = [
    (
        "old/profile-bare-tracked-affects.toml",
        0x69a9_7976_173d_2eb4,
    ),
    ("old/profiles-character.toml", 0x538c_aadd_e3ce_f277),
    ("old/profile-one-with-erelei.toml", 0x7545_3b90_6ab8_88e0),
    ("old/profile-connection.toml", 0xaf40_ce71_f6ff_f648),
    ("old/profile-no-prompt.toml", 0xf0ec_4748_02e9_8e84),
    ("old/profile-dock-no-panes.toml", 0xfbb5_fe86_4607_e7bd),
    ("old/catalog-no-presets.toml", 0x14b5_7fe9_05dc_d105),
    ("old/profile-grouped-preset.toml", 0x331a_f4ec_0d11_5763),
    ("old/profiles-0.8.1.toml", 0x1d95_4204_e236_f3be),
    ("old/profile-numpad-0.8.1.toml", 0x6b65_2022_e085_43d7),
    ("old/catalog-numpad-0.8.1.toml", 0xe4b2_17b0_8396_9e7e),
];

fn writing() -> bool {
    std::env::var_os(WRITE).is_some()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Hold `text` to the golden `name`, or write it there under
/// `VOSH_WRITE_CONFIG`. A mismatch names the first line that differs.
fn check(name: &str, text: &str) {
    let path = Path::new(DIR).join(name);
    if writing() {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the folder");
        std::fs::write(&path, text).expect("the golden writes");
        return;
    }
    let saved = read(&path);
    if saved == text {
        return;
    }
    let line = saved
        .lines()
        .zip(text.lines())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| saved.lines().count().min(text.lines().count()));
    panic!(
        "fixtures/config/{name} is not what Vosh writes. Line {} reads {:?} in the golden and \
         {:?} now. A golden changes only with a numbered bug or a lettered decision. Run with \
         {WRITE}=1 and read the diff.",
        line + 1,
        saved.lines().nth(line).unwrap_or("<end of file>"),
        text.lines().nth(line).unwrap_or("<end of file>"),
    );
}

/// The bytes the save Vosh runs writes for `config`, the profile file.
/// Settings export hands out the same text.
fn profile_bytes(config: &ProfileConfig) -> String {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profiles").join("default.toml");
    config.save(&path).unwrap();
    let text = read(&path);
    assert_eq!(
        config.to_toml().unwrap(),
        text,
        "export writes what save does"
    );
    text
}

/// A profile file that loads and saves again keeps its bytes.
fn profile_round_trip(text: &str) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("default.toml");
    std::fs::write(&path, text).unwrap();
    let loaded = ProfileConfig::load(&path).unwrap();
    assert_eq!(
        profile_bytes(&loaded),
        text,
        "a load and a save keep the bytes"
    );
}

fn global_bytes(config: &GlobalConfig) -> String {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("global.toml");
    config.save(&path).unwrap();
    read(&path)
}

fn global_round_trip(text: &str) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("global.toml");
    std::fs::write(&path, text).unwrap();
    let loaded = GlobalConfig::load(&path).unwrap();
    assert_eq!(
        global_bytes(&loaded),
        text,
        "a load and a save keep the bytes"
    );
}

fn loadouts_bytes(set: &LoadoutSet) -> String {
    let dir = tempfile::tempdir().unwrap();
    save_loadout_set(dir.path(), set).unwrap();
    read(&loadouts_path(dir.path()))
}

fn loadouts_round_trip(text: &str) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(loadouts_path(dir.path()), text).unwrap();
    let loaded = load_loadout_set(dir.path()).unwrap();
    assert_eq!(
        loadouts_bytes(&loaded),
        text,
        "a load and a save keep the bytes"
    );
}

fn catalog_bytes(catalog: &GlobalCatalog) -> String {
    let dir = tempfile::tempdir().unwrap();
    save_global_catalog(dir.path(), catalog).unwrap();
    read(&catalog_path(dir.path()))
}

fn catalog_round_trip(text: &str) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(catalog_path(dir.path()), text).unwrap();
    let loaded = load_global_catalog(dir.path()).unwrap();
    assert_eq!(
        catalog_bytes(&loaded),
        text,
        "a load and a save keep the bytes"
    );
}

/// profiles.toml as the profile set reads it and saves it back.
fn index_round_trip(text: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("profiles.toml"), text).unwrap();
    let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
    set.save_index().unwrap();
    read(&dir.path().join("profiles.toml"))
}

// ---- Full values. Each struct is spelled out field by field, so a new
// field fails to compile here until it joins the full golden.

fn full_vitals() -> VitalsConfig {
    VitalsConfig {
        show_bar: false,
        show_percent: false,
        show_numeric: false,
        show_delta: false,
        bar_filled: "#".into(),
        bar_empty: ".".into(),
        bar_width: 12,
        bar_style: "track".into(),
        bar_layout: "with_history".into(),
        layout: "gauges".into(),
        inline_style: "badge".into(),
        percent_color_mode: "accent".into(),
        pct_chip_style: "glow".into(),
        percent_color: "gradient".into(),
        template_enabled: true,
        template: "%hp/%maxhp %mn/%maxmn %mv/%maxmv".into(),
        hp_color: "#e06c75".into(),
        mn_color: "#61afef".into(),
        mv_color: "#98c379".into(),
        use_color_ramp: false,
        bar_font: "JetBrains Mono".into(),
        low_hp_vignette: true,
    }
}

fn full_panes() -> PaneLayoutPersist {
    let leaf = |pane: &str, weight: f64, props: &[(&str, &str)]| PaneNode {
        id: pane.into(),
        pane: Some(pane.into()),
        split: None,
        weight,
        children: Vec::new(),
        props: props
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect(),
    };
    PaneLayoutPersist {
        version: 1,
        panel_open: false,
        panel_width: Some(340),
        root: PaneNode {
            id: "root".into(),
            pane: None,
            split: Some("column".into()),
            weight: 1.0,
            children: vec![
                leaf("map", 0.5, &[]),
                PaneNode {
                    id: "split".into(),
                    pane: None,
                    split: Some("row".into()),
                    weight: 0.5,
                    children: vec![
                        leaf("chat", 0.6, &[("channels", "ooc,tell")]),
                        leaf("group", 0.4, &[]),
                    ],
                    props: BTreeMap::new(),
                },
            ],
            props: BTreeMap::new(),
        },
    }
}

fn full_theme() -> CustomTheme {
    CustomTheme {
        id: "custom-dusk".into(),
        label: "Dusk".into(),
        description: "A warm dark theme".into(),
        xterm: BTreeMap::from([
            ("background".into(), "#1a1b26".into()),
            ("foreground".into(), "#c0caf5".into()),
        ]),
        chrome: BTreeMap::from([
            ("accent".into(), "#ff9e64".into()),
            ("surface".into(), "#16161e".into()),
        ]),
        // An imported theme keeps its game color fit with it. Two slots
        // of this theme's real fit.
        fitted: BTreeMap::from([
            ("brightBlack".into(), "#94989f".into()),
            ("red".into(), "#cb7b74".into()),
        ]),
    }
}

fn full_ui() -> UiConfig {
    UiConfig {
        theme: "custom-dusk".into(),
        follow_system_appearance: true,
        light_theme: "kanso-pearl".into(),
        dark_theme: "tokyo-night".into(),
        theme_follow: "system".into(),
        day_theme: "kanso-pearl".into(),
        night_theme: "custom-dusk".into(),
        auto_update: true,
        font_family: "JetBrains Mono, monospace".into(),
        font_size: 16,
        terminal_line_height: "loose".into(),
        panel_font: "\"Iosevka\", Menlo, monospace".into(),
        panel_font_size: 13,
        tracked_affects: vec![
            TrackedAffect {
                name: "sanctuary".into(),
                label: None,
            },
            TrackedAffect {
                name: "Field of Discord".into(),
                label: Some("Shroud".into()),
            },
        ],
        enabled_presets: vec!["healing_basics".into(), "sent_tells".into()],
        dock_layout: vec![
            DockEntryPersist {
                id: "map".into(),
                zone: "right".into(),
                align: Some("top".into()),
            },
            DockEntryPersist {
                id: "chat".into(),
                zone: "hidden".into(),
                align: None,
            },
        ],
        panes: Some(full_panes()),
        keep_last_command: true,
        theme_terminal_colors: Some(false),
        bright_bold: true,
        blink_text: Some(false),
        // Written only while off, so on keeps the golden's bytes. The
        // fit_game_colors tests in profile/ui.rs and ipc/ui_config.rs
        // cover off.
        fit_game_colors: true,
        // Written only off Typical, the default.
        color_vision: "deuteranopia".into(),
        // Written only while off, so on keeps the golden's bytes. The
        // readable_highlights tests in ipc/ui_config.rs cover off.
        readable_highlights: true,
        screen_reader: true,
        screen_reader_background: true,
        screen_reader_prompt: true,
        screen_reader_burst: 16,
        // Written only while on, so off keeps the golden's bytes. The
        // collapse_repeats tests in ipc/ui_config.rs cover on.
        collapse_repeats: false,
        // Written only off their defaults, so the defaults keep the
        // golden's bytes. The tests in profile/ui.rs cover the others.
        collapse_fight_lines: true,
        collapse_attack_lines: false,
        terminal_base_ansi: Some(
            [
                "#000000", "#cd3131", "#0dbc79", "#e5e510", "#2472c8", "#bc3fbc", "#11a8cd",
                "#e5e5e5", "#666666", "#f14c4c", "#23d18b", "#f5f543", "#3b8eea", "#d670d6",
                "#29b8db", "#ffffff",
            ]
            .map(String::from)
            .to_vec(),
        ),
        custom_themes: vec![full_theme()],
        split_divider_color: Some("#ff00ff".into()),
        input_echo_color: Some("#88aaff".into()),
        echo_macros: false,
        input_echo_caret: false,
        input_echo_mark: "off".into(),
        input_echo_mark_text: "T>".into(),
        input_echo_mark_color: Some("#c6a46a".into()),
        input_echo_dim: true,
        input_line_mark: false,
        side_panels_fill_height: true,
        paste_line_delay_ms: 250,
        spellcheck_prompt: true,
        writing_offer: false,
        writing_ask_post: false,
        input_cursor_style: "underline_thick".into(),
        input_caret_blink: false,
        input_caret_color: Some("#c6a46a".into()),
        input_line_color: Some("#d8dee9".into()),
        input_line_background: "own".into(),
        input_line_background_color: Some("#1d1f21".into()),
        input_line_size: 16,
        input_type_colors: true,
        input_type_alias_color: Some("#8abeb7".into()),
        input_type_hash_color: Some("#b294bb".into()),
        input_type_chat_color: Some("#f0c674".into()),
        input_type_unknown_color: Some("#cc6666".into()),
        // set_prompt fills both from the [prompt] table.
        prompt_template_enabled: false,
        prompt_template: String::new(),
        vitals: full_vitals(),
        vitals_density: "line".into(),
        vitals_values: "percent".into(),
        vitals_meter: "bar".into(),
        vitals_warn_thirds: true,
        vitals_hide_when_pinned: false,
        // The vitals styles keys hold their defaults, which write nothing,
        // so this golden keeps the bytes 0.8.1 reads.
        vitals_style: None,
        vitals_place: "panel".into(),
        vitals_order: vec!["hp".into(), "mana".into(), "move".into()],
        vitals_off: Vec::new(),
        vitals_opponent: "top".into(),
        vitals_colors: BTreeMap::new(),
        vitals_text: String::new(),
        vitals_text_previous: Vec::new(),
        vitals_hit: false,
        moons_position: "before-time".into(),
        chip_style: "icon_value".into(),
        tick_count: "down_past_zero".into(),
        game_time: "12h".into(),
        affects_style: "chips".into(),
        affects_marker: "plus_minus".into(),
        affects_tint: true,
        affects_running_out_hours: 5,
        affects_almost_gone_hours: 2,
        // Written only off their defaults, so the defaults keep the
        // golden's bytes. The snoop split tests in profile/ui.rs cover
        // the others.
        snoop_share: 0.4,
        snoop_folded: false,
        // Written only once you choose, so None keeps the golden's
        // bytes. profile/ui.rs tests the choice.
        log_sessions: None,
        // The default keeps the golden's bytes. profile/ui.rs tests a size.
        scrollback_lines: 10_000,
        // Written only once you move, size or pin the writing card, so
        // the defaults keep the golden's bytes. profile/ui.rs tests them.
        writing_card_left: None,
        writing_card_top: None,
        writing_card_rows: None,
        writing_card_cols: None,
        writing_card_pinned: false,
        chat_colors: BTreeMap::from([
            ("ooc".into(), "brightBlue".into()),
            ("tell".into(), "magenta".into()),
        ]),
    }
}

fn full_tick() -> TickConfig {
    TickConfig {
        enabled: false,
        interval_secs: 45,
        auto_fire: Some("score".into()),
        sound: false,
        reset_pattern: Some(r"^The day has begun\.$".into()),
        warn_at_secs: Some(5),
        warn_message: Some("tick soon".into()),
        warn_color: Some("#ffcc00".into()),
    }
}

fn full_aliases() -> Vec<Alias> {
    vec![
        Alias {
            name: "kk".into(),
            expansion: "kick %1".into(),
            enabled: true,
            group: Some("combat".into()),
            script: None,
        },
        Alias {
            name: "heal".into(),
            expansion: String::new(),
            enabled: false,
            group: None,
            script: Some("mud.send(\"cast heal \" .. args[1])".into()),
        },
    ]
}

fn full_triggers() -> Vec<Trigger> {
    vec![
        Trigger {
            name: "tells".into(),
            patterns: vec![
                TriggerPattern::regex(r"^(\w+) tells you '(.*)'$"),
                TriggerPattern {
                    enabled: false,
                    ..TriggerPattern::regex(r"^(\w+) whispers to you")
                },
                TriggerPattern {
                    mode: MatchMode::StartsWith,
                    ..TriggerPattern::regex("Tolliver tells you")
                },
            ],
            priority: 5,
            enabled: true,
            actions: vec![
                TriggerAction::Highlight {
                    style: HighlightStyle {
                        fg: Some(NamedColor::BrightCyan),
                        bg: Some(NamedColor::Black),
                        bold: true,
                        underline: true,
                        inverse: true,
                        wash: true,
                        base: false,
                    },
                },
                TriggerAction::Replace {
                    template: "$1: $2".into(),
                },
                TriggerAction::Send {
                    template: "reply ok".into(),
                },
                TriggerAction::Route {
                    pane: "chat".into(),
                },
                TriggerAction::Script {
                    body: "mud.echo(captures[1])".into(),
                },
            ],
            preset: Some("sent_tells".into()),
            group: Some("comms".into()),
            target: TriggerTarget::Prompt,
            // The alert table, which 0.8.1 skips.
            alert: Some(AlertParts {
                banner: true,
                sound: Some("chime".into()),
                attention: Some(Attention::Until),
                background: false,
                words: true,
            }),
        },
        Trigger {
            name: "spam".into(),
            patterns: vec![
                TriggerPattern::regex("^You are hungry\\.$"),
                TriggerPattern {
                    mode: MatchMode::Text,
                    ..TriggerPattern::regex("You are thirsty.")
                },
            ],
            priority: 0,
            enabled: false,
            actions: vec![TriggerAction::Gag],
            preset: None,
            group: None,
            target: TriggerTarget::Line,
            alert: None,
        },
        // A Room trigger, which goes under `room_triggers`, so a
        // rollback still reads the file.
        Trigger {
            name: "room-items".into(),
            patterns: vec![TriggerPattern::regex("^.+$")],
            priority: 4,
            enabled: true,
            actions: vec![TriggerAction::Highlight {
                style: HighlightStyle {
                    fg: Some(NamedColor::Yellow),
                    base: true,
                    ..HighlightStyle::default()
                },
            }],
            preset: None,
            group: None,
            target: TriggerTarget::Room,
            alert: None,
        },
    ]
}

fn full_macros() -> Vec<Macro> {
    vec![
        Macro {
            key: "F1".into(),
            command: "score".into(),
            group: Some("info".into()),
            enabled: true,
            preset: None,
        },
        Macro {
            key: "F2".into(),
            command: "flee".into(),
            group: None,
            enabled: false,
            preset: None,
        },
        Macro {
            key: "Numpad3".into(),
            command: "rec".into(),
            group: None,
            enabled: true,
            preset: None,
        },
        // A preset macro on a key no macro of yours uses is on, and one
        // on a key yours uses is held off.
        Macro {
            key: "Numpad8".into(),
            command: "n".into(),
            group: None,
            enabled: true,
            preset: Some("numpad_movement".into()),
        },
        Macro {
            key: "Numpad3".into(),
            command: "d".into(),
            group: None,
            enabled: false,
            preset: Some("numpad_movement".into()),
        },
    ]
}

fn full_prompt() -> PromptConfig {
    PromptConfig {
        draw: true,
        template: "%hp/%maxhp hp %mana/%maxmana mn > ".into(),
        previous_templates: vec!["%hp hp > ".into(), "> ".into()],
        capture: CaptureConfig::Aabahran(AabahranCapture {
            prompt: "<%hhp %mm %vmv> ".into(),
            fprompt: "<%hhp %mm %vmv [%e]> ".into(),
            follow_game: false,
            seen_at: Some("2026-09-30T21:14:05-07:00".into()),
            source: Some(CaptureSource::Gmcp),
        }),
        show: PromptShow::Pinned,
        // A design of yours, which the file says with no mirror key.
        mirror: false,
    }
}

fn full_profile() -> ProfileConfig {
    let mut config = ProfileConfig {
        aliases: full_aliases(),
        profile_vars: BTreeMap::from([
            ("target".into(), "orc".into()),
            ("weapon".into(), "sword".into()),
        ]),
        triggers: full_triggers(),
        tick: full_tick(),
        ui: full_ui(),
        plugins: PluginsPersist {
            enabled: vec!["vitals_alert".into()],
        },
        macros: full_macros(),
        timers: vec![Timer {
            id: 1,
            name: "save".into(),
            interval_secs: 300,
            command: "save".into(),
            enabled: true,
            group: Some("upkeep".into()),
        }],
        disabled_alias_groups: vec!["social".into()],
        disabled_trigger_groups: vec!["spam".into()],
        disabled_macro_groups: vec!["travel".into()],
        disabled_timer_groups: vec!["upkeep".into()],
        group_folders: GroupFolders {
            aliases: BTreeMap::from([(
                "combat".into(),
                vec!["combat".into(), "combat (Healer)".into()],
            )]),
            triggers: BTreeMap::from([("comms".into(), vec!["comms".into()])]),
            macros: BTreeMap::from([("travel".into(), Vec::new())]),
        },
        prompt: None,
        alerts: full_alerts(),
        preset_edits: full_preset_edits(),
        // Reconnect when the link drops, turned off.
        reconnect: OnSwitch(false),
    };
    config.set_prompt(full_prompt());
    config
}

/// What two alert presets do, the `[alerts]` table.
fn full_alerts() -> BTreeMap<String, AlertParts> {
    BTreeMap::from([
        (
            "alert_tells".into(),
            AlertParts {
                banner: true,
                sound: Some("bell".into()),
                attention: Some(Attention::Once),
                background: true,
                words: false,
            },
        ),
        (
            "alert_low_health".into(),
            AlertParts {
                sound: Some("low".into()),
                background: false,
                ..AlertParts::default()
            },
        ),
    ])
}

/// Your edits to Disarms and fading buffs, the `[preset_edits]` table: a
/// color, a trigger switch, a Replace with, and a Then send a later fix
/// flagged.
fn full_preset_edits() -> PresetEdits {
    let row = |value: toml::Value, was: toml::Value| EditRow {
        value,
        was,
        seen: None,
    };
    let trigger = |name: &str, key: &str, edit: EditRow| {
        (name.to_string(), BTreeMap::from([(key.to_string(), edit)]))
    };
    BTreeMap::from([(
        "disarm_buff_fade".into(),
        PresetEdit {
            colors: BTreeMap::from([("line".into(), row("#c3a6ff".into(), "fg:178".into()))]),
            triggers: BTreeMap::from([
                trigger("buff.sanctuary", "enabled", row(false.into(), true.into())),
                trigger(
                    "disarm.secondary",
                    "send",
                    EditRow {
                        seen: Some("get 1.;dual 1.".into()),
                        ..row("".into(), "get 1.;wield 1.".into())
                    },
                ),
                trigger(
                    "buff.spell_turning",
                    "replace",
                    row(
                        "{line}Your shield of spell turning collapses.{reset}".into(),
                        "{mark}##{reset} {line}Your shield of spell turning collapses.{reset}"
                            .into(),
                    ),
                ),
            ]),
        },
    )])
}

/// The full profile with a regex capture in place of Aabahran's codes,
/// the other shape `[prompt.capture]` takes.
fn full_regex_profile() -> ProfileConfig {
    let mut config = full_profile();
    let mut prompt = full_prompt();
    prompt.capture = CaptureConfig::Regex(RegexCapture {
        lines: vec![r"^<(?<hp>\d+)hp (\d+)m (\d+)mv> $".into()],
        settle: true,
        names: BTreeMap::from([("2".into(), "mana".into()), ("3".into(), "move".into())]),
        seen_at: Some("2026-09-30T21:14:05-07:00".into()),
        source: Some(CaptureSource::Migrated),
    });
    config.set_prompt(prompt);
    config
}

fn full_global() -> GlobalConfig {
    GlobalConfig {
        theme: Some("custom-dusk".into()),
        auto_update: Some(true),
        keep_last_command: Some(true),
        font_family: Some("JetBrains Mono, monospace".into()),
        font_size: Some(16),
        follow_system_appearance: Some(true),
        light_theme: Some("kanso-pearl".into()),
        dark_theme: Some("tokyo-night".into()),
        theme_follow: Some("system".into()),
        day_theme: Some("kanso-pearl".into()),
        night_theme: Some("custom-dusk".into()),
        color_vision: Some("protanopia".into()),
        terminal_line_height: Some("compact".into()),
        panel_font: Some("system".into()),
        panel_font_size: Some(0),
        dock_layout: Some(vec![DockEntryPersist {
            id: "map".into(),
            zone: "left".into(),
            align: Some("bottom".into()),
        }]),
        custom_themes: Some(vec![full_theme()]),
    }
}

fn full_auto_match() -> AutoMatch {
    AutoMatch {
        host: Some("play.theforsakenlands.com".into()),
        port: Some(1848),
        characters: vec!["Tester".into(), "Testalt".into()],
        enabled: false,
    }
}

fn full_loadouts() -> LoadoutSet {
    LoadoutSet {
        active: vec!["warrior".into(), "shared".into()],
        dormant: true,
        loadouts: vec![
            // A loadout with only its name, which writes no empty
            // tables.
            Loadout::empty("shared"),
            Loadout {
                name: "warrior".into(),
                description: Some("Melee main".into()),
                auto_match: Some(full_auto_match()),
                enabled_groups: vec!["combat-melee".into(), "wartools".into()],
            },
        ],
        ..Default::default()
    }
}

fn full_catalog() -> GlobalCatalog {
    GlobalCatalog {
        aliases: full_aliases(),
        triggers: full_triggers(),
        macros: full_macros(),
        enabled_presets: Some(vec!["healing_basics".into(), "sent_tells".into()]),
        alerts: full_alerts(),
        preset_edits: full_preset_edits(),
    }
}

fn full_index() -> ProfilesIndex {
    ProfilesIndex {
        active: "Healer".into(),
        profiles: vec![
            ProfileEntry {
                name: "default".into(),
                description: None,
                auto_match: None,
            },
            ProfileEntry {
                name: "Healer".into(),
                description: Some("Cleric main".into()),
                auto_match: Some(AutoMatch {
                    enabled: true,
                    ..full_auto_match()
                }),
            },
            ProfileEntry {
                name: "Ranger".into(),
                description: None,
                auto_match: Some(AutoMatch {
                    host: Some("play.theforsakenlands.com".into()),
                    port: None,
                    characters: Vec::new(),
                    enabled: false,
                }),
            },
        ],
        scope: ScopeConfig {
            theme: Scope::Profile,
            font: Scope::Global,
            dock_layout: Scope::Profile,
            keep_last_command: Scope::Profile,
            auto_update: Scope::Global,
        },
        migrations: vec![
            "prompt-capture-to-profile".into(),
            "preset-sent-tells-on".into(),
            "prompt-line-triggers".into(),
        ],
        notices: vec!["Vosh moved your prompt capture into the Default profile.".into()],
        sessions: Vec::new(),
        selected: None,
        get_started: Some(GetStarted {
            at_launch: false,
            done: vec!["connect".into()],
        }),
        keep_logs_days: Some(90),
    }
}

/// [`full_index`] with three sessions open, the second selected, and no
/// Get started.
fn sessions_index() -> ProfilesIndex {
    let world = || Some("play.theforsakenlands.com".to_string());
    ProfilesIndex {
        sessions: vec![
            SessionEntry {
                id: SessionId::FIRST,
                name: None,
                host: world(),
                port: Some(1848),
                tls: false,
                profile: "Healer".into(),
            },
            SessionEntry {
                id: SessionId::numbered(3),
                name: Some("Build port".into()),
                host: world(),
                port: Some(1825),
                tls: true,
                profile: "Healer".into(),
            },
            SessionEntry {
                id: SessionId::numbered(4),
                name: None,
                host: None,
                port: None,
                tls: false,
                profile: "default".into(),
            },
        ],
        selected: Some(SessionId::numbered(3)),
        get_started: None,
        ..full_index()
    }
}

// ---- Goldens.

#[test]
fn a_profile_file_writes_these_bytes() {
    for (name, config) in [
        ("profile.default.toml", ProfileConfig::default()),
        ("profile.fresh.toml", ProfileConfig::fresh()),
        ("profile.full.toml", full_profile()),
        ("profile.full-regex.toml", full_regex_profile()),
    ] {
        let text = profile_bytes(&config);
        check(name, &text);
        profile_round_trip(&text);
    }
}

/// Export to Downloads writes the full profile's bytes, then the
/// `[vosh_export]` table with its world and the one character of its two
/// you ticked. A profile reads the export as the profile
/// alone, and the table reads back.
#[test]
fn an_export_writes_these_bytes() {
    let profile = profile_bytes(&full_profile());
    let world = claim("play.theforsakenlands.com", Some(1848), &["Maren", "Orla"]);
    let table = VoshExport::new(Some(&world), &["Orla".to_string()]);
    let text = table.write(&profile).unwrap();
    check("export.full.toml", &text);
    let back = ProfileConfig::from_toml(&text).unwrap();
    assert_eq!(back.to_toml().unwrap(), profile);
    assert_eq!(export::read(&text).unwrap(), Some(table));
}

#[test]
fn global_toml_writes_these_bytes() {
    for (name, config) in [
        ("global.default.toml", GlobalConfig::default()),
        ("global.full.toml", full_global()),
    ] {
        let text = global_bytes(&config);
        check(name, &text);
        global_round_trip(&text);
    }
}

#[test]
fn loadouts_toml_writes_these_bytes() {
    for (name, set) in [
        ("loadouts.default.toml", LoadoutSet::default()),
        ("loadouts.full.toml", full_loadouts()),
    ] {
        let text = loadouts_bytes(&set);
        check(name, &text);
        loadouts_round_trip(&text);
    }
}

#[test]
fn catalog_toml_writes_these_bytes() {
    for (name, catalog) in [
        ("catalog.default.toml", GlobalCatalog::default()),
        ("catalog.full.toml", full_catalog()),
    ] {
        let text = catalog_bytes(&catalog);
        check(name, &text);
        catalog_round_trip(&text);
    }
}

#[test]
fn profiles_toml_writes_these_bytes() {
    // The index a folder with no profiles.toml starts with.
    let dir = tempfile::tempdir().unwrap();
    ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
    let seeded = read(&dir.path().join("profiles.toml"));
    check("profiles.default.toml", &seeded);
    assert_eq!(index_round_trip(&seeded), seeded);

    let full = toml::to_string_pretty(&full_index()).unwrap();
    check("profiles.full.toml", &full);
    assert_eq!(index_round_trip(&full), full);

    let sessions = toml::to_string_pretty(&sessions_index()).unwrap();
    check("profiles.sessions.toml", &sessions);
    assert_eq!(index_round_trip(&sessions), sessions);
}

/// profiles.toml as 0.8.1 reads and saves it, which knows no session
/// list and no Get started.
#[derive(serde::Deserialize, serde::Serialize)]
struct OldIndex {
    active: String,
    #[serde(default, rename = "profile")]
    profiles: Vec<ProfileEntry>,
    #[serde(default)]
    scope: ScopeConfig,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    migrations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    notices: Vec<String>,
}

#[test]
fn an_older_build_reads_the_session_list_and_drops_it_on_its_save() {
    let sessions = toml::to_string_pretty(&sessions_index()).unwrap();
    let old: OldIndex = toml::from_str(&sessions).expect("0.8.1 reads it");
    assert_eq!(
        toml::to_string_pretty(&old).unwrap(),
        toml::to_string_pretty(&index_without_get_started()).unwrap()
    );
}

/// [`full_index`] as 0.8.1 saves it, with no Get started and no Keep
/// logs for.
fn index_without_get_started() -> ProfilesIndex {
    ProfilesIndex {
        get_started: None,
        keep_logs_days: None,
        ..full_index()
    }
}

#[test]
fn an_older_build_reads_get_started_and_drops_it_on_its_save() {
    let full = toml::to_string_pretty(&full_index()).unwrap();
    let old: OldIndex = toml::from_str(&full).expect("0.8.1 reads it");
    assert_eq!(
        toml::to_string_pretty(&old).unwrap(),
        toml::to_string_pretty(&index_without_get_started()).unwrap()
    );
}

/// Every file under `root`, by its path from `root`, leaving out the
/// backups a save rotates.
fn written_files(root: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap().filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else if !path.to_string_lossy().contains(".bak.") {
                let rel = path.strip_prefix(root).unwrap();
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

#[tokio::test]
async fn a_fresh_install_writes_these_files_on_its_first_save() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let state: SharedState = Arc::new(AppState::default());
    crate::app::launch::load(&state, root).await;
    {
        let _persist = PERSIST_LOCK.lock().await;
        crate::disk::save::persist_state(&state, &state.selected_session().profile()).await;
    }
    assert_eq!(
        written_files(root),
        ["global.toml", "profiles.toml", "profiles/default.toml"],
        "a first save writes these files and no others"
    );
    for (golden, file) in [
        ("first-save/profiles.toml", "profiles.toml"),
        ("first-save/global.toml", "global.toml"),
        ("first-save/default-profile.toml", "profiles/default.toml"),
    ] {
        check(golden, &read(&root.join(file)));
    }
}

#[test]
fn the_config_folder_holds_only_the_listed_files() {
    if writing() {
        // The goldens are still being written alongside.
        return;
    }
    let mut want: Vec<String> = GOLDENS
        .iter()
        .map(|s| (*s).to_string())
        .chain(OLD_INPUTS.iter().map(|(n, _)| (*n).to_string()))
        .collect();
    want.sort();
    assert_eq!(written_files(Path::new(DIR)), want);
}

// ---- Old inputs.

/// FNV-1a over `bytes`, a digest that never changes between builds.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn old_input(name: &str) -> String {
    assert!(
        OLD_INPUTS.iter().any(|(n, _)| *n == name),
        "{name} is not listed in OLD_INPUTS"
    );
    read(&Path::new(DIR).join(name))
}

/// Put the old input `name` at `rel` in a fresh app data folder.
fn place(name: &str, rel: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, old_input(name)).unwrap();
    (dir, path)
}

fn load_old_profile(name: &str) -> ProfileConfig {
    let (_dir, path) = place(name, "profiles/default.toml");
    ProfileConfig::load(&path).unwrap_or_else(|e| panic!("{name} still loads: {e}"))
}

#[test]
fn the_old_inputs_never_change() {
    for (name, digest) in OLD_INPUTS {
        let bytes = std::fs::read(Path::new(DIR).join(name)).unwrap();
        assert_eq!(
            fnv1a(&bytes),
            digest,
            "fixtures/config/{name} changed. Old inputs stand for files older builds wrote, and \
             they never change. Add a new one instead."
        );
    }
}

#[test]
fn bare_tracked_affects_still_load() {
    let config = load_old_profile("old/profile-bare-tracked-affects.toml");
    assert_eq!(
        config.ui.tracked_affects,
        [
            TrackedAffect {
                name: "sanctuary".into(),
                label: None,
            },
            TrackedAffect {
                name: "haste".into(),
                label: None,
            },
            TrackedAffect {
                name: "Field of Discord".into(),
                label: None,
            },
        ]
    );
    assert_eq!(config.ui.theme, "kanso-zen");
}

#[test]
fn an_index_from_0_8_1_keeps_get_started_shut() {
    let (dir, path) = place("old/profiles-0.8.1.toml", "profiles.toml");
    let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
    assert_eq!(set.get_started(), None);
    set.save_index().unwrap();
    assert_eq!(read(&path), old_input("old/profiles-0.8.1.toml"));
}

/// Launch over `dir` as Vosh does, then install Numpad movement, which
/// the file has on, as the main window does at launch, through
/// `presets_install` with the macros src/automation/presets.ts holds.
/// The six macros 0.8.1 saved as yours take their preset back, n keeps
/// its group, and your rec keeps Numpad3, so the preset d stays off.
/// `saved` reads the macros back from the file the save wrote, where
/// no seventh macro and no second rec turn up.
async fn numpad_comes_home(dir: &Path, saved: impl Fn() -> Vec<Macro>) {
    use tauri::test::{mock_builder, mock_context, noop_assets};
    use tauri::Manager;

    let state: SharedState = Arc::new(AppState::default());
    let app = mock_builder().build(mock_context(noop_assets())).unwrap();
    app.manage::<SharedState>(state.clone());
    crate::app::launch::load(&state, dir).await;
    let preset = |key: &str, command: &str| Macro {
        key: key.into(),
        command: command.into(),
        group: None,
        enabled: true,
        preset: Some("numpad_movement".into()),
    };
    let library = [
        ("Numpad8", "n"),
        ("Numpad6", "e"),
        ("Numpad2", "s"),
        ("Numpad4", "w"),
        ("Numpad9", "u"),
        ("Numpad3", "d"),
    ];
    let library = library.iter().map(|(k, c)| preset(k, c)).collect();
    crate::ipc::automation::presets_install(
        app.handle().clone(),
        app.state(),
        Vec::new(),
        library,
        None,
    )
    .await
    .unwrap();
    let rec = Macro {
        preset: None,
        ..preset("Numpad3", "rec")
    };
    let want = vec![
        rec,
        Macro {
            group: Some("travel".into()),
            ..preset("Numpad8", "n")
        },
        preset("Numpad6", "e"),
        preset("Numpad2", "s"),
        preset("Numpad4", "w"),
        preset("Numpad9", "u"),
        Macro {
            enabled: false,
            ..preset("Numpad3", "d")
        },
    ];
    assert_eq!(state.selected_profile().await.macros, want);
    assert_eq!(saved(), want);
}

/// A profile file that came back through 0.8.1, which drops the preset
/// tag, gets its six Numpad movement macros back as the preset's.
#[tokio::test]
async fn preset_macros_back_from_a_0_8_1_profile_file_take_their_preset_back() {
    let (dir, path) = place("old/profile-numpad-0.8.1.toml", "profiles/default.toml");
    numpad_comes_home(dir.path(), || ProfileConfig::load(&path).unwrap().macros).await;
}

/// The same in loadout mode, where 0.8.1 wrote the macros to
/// catalog.toml and the launch install lands there.
#[tokio::test]
async fn preset_macros_back_from_a_0_8_1_catalog_take_their_preset_back() {
    let (dir, _path) = place("old/catalog-numpad-0.8.1.toml", "catalog.toml");
    let mut file = load_old_profile("old/profile-numpad-0.8.1.toml");
    file.macros.clear();
    file.save(&dir.path().join("profiles/default.toml"))
        .unwrap();
    numpad_comes_home(dir.path(), || {
        load_global_catalog(dir.path()).unwrap().macros
    })
    .await;
}

#[test]
fn an_index_with_character_still_loads() {
    let (dir, _path) = place("old/profiles-character.toml", "profiles.toml");
    let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
    assert_eq!(set.active_name(), "default");
    let claim = |name: &str| {
        let entry = set.get(name).unwrap_or_else(|| panic!("{name} is listed"));
        entry.auto_match.clone().expect("a claim")
    };
    let default = claim("default");
    assert_eq!(default.characters, ["Tester"]);
    assert_eq!(default.host.as_deref(), Some("play.theforsakenlands.com"));
    assert_eq!(default.port, Some(1848));
    assert!(default.enabled);
    // The old name goes first, ahead of the list.
    assert_eq!(claim("Healer").characters, ["Testhealer", "Testalt"]);
    assert_eq!(
        set.resolve_match("play.theforsakenlands.com", 1848, Some("tester"))
            .as_deref(),
        Some("default")
    );
    assert_eq!(
        set.resolve_match("play.theforsakenlands.com", 1848, Some("Testalt"))
            .as_deref(),
        Some("Healer")
    );
}

#[test]
fn one_with_erelei_still_loads_as_the_low_hp_vignette() {
    let config = load_old_profile("old/profile-one-with-erelei.toml");
    assert!(config.ui.vitals.low_hp_vignette);
    assert_eq!(config.ui.vitals.layout, "stacked");
}

#[test]
fn a_profile_file_with_a_connection_table_still_loads() {
    let config = load_old_profile("old/profile-connection.toml");
    assert_eq!(config.tick.interval_secs, 45);
    assert!(config.tick.enabled);
    assert_eq!(
        config.profile_vars.get("target").map(String::as_str),
        Some("orc")
    );
    assert_eq!(config.ui.theme, "kanso-zen");
    assert_eq!(config.ui.theme_terminal_colors, Some(true));
    assert_eq!(config.aliases.len(), 1);
    assert_eq!(config.aliases[0].expansion, "say Another");
}

#[test]
fn a_profile_file_with_no_prompt_table_still_loads_its_design() {
    let config = load_old_profile("old/profile-no-prompt.toml");
    let prompt = config.prompt_config();
    assert!(prompt.draw);
    assert_eq!(prompt.template, "%hp/%maxhp hp %mana/%maxmana mn > ");
    assert!(prompt.capture.is_none());
    assert_eq!(prompt.show, PromptShow::Text);
    // The [ui] copy stays in step.
    assert!(config.ui.prompt_template_enabled);
    assert_eq!(config.ui.prompt_template, prompt.template);
}

#[test]
fn a_dock_layout_with_no_panes_still_loads_its_panel() {
    let config = load_old_profile("old/profile-dock-no-panes.toml");
    assert!(config.ui.panes.is_none());
    assert_eq!(config.ui.dock_layout.len(), 5);
    let leaf = |pane: &str, weight: f64| PaneNode {
        id: pane.into(),
        pane: Some(pane.into()),
        split: None,
        weight,
        children: Vec::new(),
        props: BTreeMap::new(),
    };
    assert_eq!(
        config.ui.pane_layout(),
        PaneLayoutPersist {
            version: 1,
            panel_open: true,
            panel_width: None,
            root: PaneNode {
                id: "root".into(),
                pane: None,
                split: Some("column".into()),
                weight: 1.0,
                children: vec![leaf("map", 0.45), leaf("group", 0.25), leaf("affects", 0.3)],
                props: BTreeMap::new(),
            },
        }
    );
}

/// A 0.8.1 profile file, from before the presets took your edits, keeps
/// the group you gave a preset trigger and reads with no edits.
#[test]
fn a_grouped_preset_trigger_with_no_edits_still_loads() {
    let config = load_old_profile("old/profile-grouped-preset.toml");
    assert_eq!(config.ui.enabled_presets, ["disarm_buff_fade"]);
    assert_eq!(config.triggers.len(), 1);
    let sanctuary = &config.triggers[0];
    assert_eq!(sanctuary.name, "buff.sanctuary");
    assert_eq!(sanctuary.preset.as_deref(), Some("disarm_buff_fade"));
    assert_eq!(sanctuary.group.as_deref(), Some("fights"));
    let edits = &config.preset_edits;
    assert!(edits.is_empty(), "{edits:?}");
}

/// Profile files from before the marks keep `input_echo_caret = true`,
/// which reads as the › mark, and a save keeps the old switch for them.
#[test]
fn an_old_mark_your_commands_switch_reads_as_the_chevron() {
    for name in [
        "old/profile-grouped-preset.toml",
        "old/profile-numpad-0.8.1.toml",
    ] {
        let config = load_old_profile(name);
        assert_eq!(config.ui.input_echo_mark, "chevron", "{name}");
        let text = config.to_toml().unwrap();
        assert!(text.contains("input_echo_caret = true"), "{name}: {text}");
        assert!(!text.contains("input_echo_mark"), "{name}: {text}");
    }
}

#[test]
fn a_catalog_without_enabled_presets_still_loads() {
    let (dir, _path) = place("old/catalog-no-presets.toml", "catalog.toml");
    let catalog = load_global_catalog(dir.path()).unwrap();
    assert_eq!(catalog.enabled_presets, None);
    assert_eq!(catalog.aliases.len(), 1);
    assert_eq!(catalog.aliases[0].name, "another");
    assert_eq!(catalog.triggers.len(), 1);
    assert_eq!(
        catalog.triggers[0].preset.as_deref(),
        Some("combat_outgoing")
    );
    let leftover = &catalog.macros;
    assert!(leftover.is_empty(), "{leftover:?}");
}

// ---- Rolling back to an older build.

/// A trigger as 0.8.0 and 0.7.2 read it. Their target knows `line` and
/// `prompt` and nothing else, and a `room` fails the whole file. They
/// skip every key they do not know.
#[derive(serde::Deserialize)]
struct OldTrigger {
    #[serde(default)]
    target: OldTarget,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum OldTarget {
    #[default]
    Line,
    Prompt,
}

/// The triggers of any config file, as those builds read them.
#[derive(serde::Deserialize)]
struct OldTriggers {
    #[serde(default)]
    triggers: Vec<OldTrigger>,
}

/// The triggers 0.8.0 reads in `text`, or why it fails the file.
fn old_build_reads(text: &str) -> Result<Vec<OldTrigger>, toml::de::Error> {
    toml::from_str::<OldTriggers>(text).map(|file| file.triggers)
}

/// The names of the Room and Your target triggers in `triggers`.
fn room_names(triggers: &[Trigger]) -> Vec<&str> {
    triggers
        .iter()
        .filter(|t| t.target.is_room())
        .map(|t| t.name.as_str())
        .collect()
}

#[test]
fn every_golden_still_reads_in_0_8_0_with_line_and_prompt_targets_only() {
    for name in GOLDENS {
        let text = read(&Path::new(DIR).join(name));
        if let Err(e) = old_build_reads(&text) {
            panic!("0.8.0 fails fixtures/config/{name}: {e}");
        }
    }
    // The full files hold a Room trigger, which 0.8.0 never sees, and
    // this build reads back.
    let full = read(&Path::new(DIR).join("profile.full.toml"));
    assert!(full.contains("[[room_triggers]]"), "{full}");
    let old = old_build_reads(&full).unwrap();
    assert_eq!(old.len(), 2);
    assert!(matches!(old[0].target, OldTarget::Prompt));
    assert!(matches!(old[1].target, OldTarget::Line));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("default.toml");
    std::fs::write(&path, &full).unwrap();
    let loaded = ProfileConfig::load(&path).unwrap();
    assert_eq!(room_names(&loaded.triggers), ["room-items"]);
    let catalog = read(&Path::new(DIR).join("catalog.full.toml"));
    assert!(catalog.contains("[[room_triggers]]"), "{catalog}");
    assert_eq!(old_build_reads(&catalog).unwrap().len(), 2);
}

/// A macro as 0.8.1 reads it. It knows no `preset` and skips the key.
#[derive(serde::Deserialize)]
struct OldMacro {
    key: String,
    command: String,
    #[serde(default)]
    group: Option<String>,
    #[serde(default = "old_macro_on")]
    enabled: bool,
}

fn old_macro_on() -> bool {
    true
}

/// The macros of any config file, as 0.8.1 reads them. catalog.toml
/// holds no groups that are off.
#[derive(serde::Deserialize)]
struct OldMacros {
    #[serde(default)]
    macros: Vec<OldMacro>,
    #[serde(default)]
    disabled_macro_groups: Vec<String>,
}

impl OldMacros {
    /// The command each key sends in 0.8.1. Its command line passes over
    /// a macro that is off or in a group that is off, and the last macro
    /// left on a key wins it.
    fn fired(&self) -> BTreeMap<&str, &str> {
        self.macros
            .iter()
            .filter(|m| m.enabled)
            .filter(|m| {
                m.group
                    .as_ref()
                    .map_or(true, |g| !self.disabled_macro_groups.contains(g))
            })
            .map(|m| (m.key.as_str(), m.command.as_str()))
            .collect()
    }
}

#[test]
fn every_golden_still_reads_in_0_8_1_and_your_macro_keeps_its_key() {
    let mut full = 0;
    for name in GOLDENS {
        let text = read(&Path::new(DIR).join(name));
        let old = toml::from_str::<OldMacros>(&text)
            .unwrap_or_else(|e| panic!("0.8.1 fails fixtures/config/{name}: {e}"));
        if old.macros.is_empty() {
            continue;
        }
        // Your Numpad3 keeps the key, so the preset's d is held off, and
        // the preset's n sends on Numpad8.
        let fired = old.fired();
        assert_eq!(
            fired,
            BTreeMap::from([("F1", "score"), ("Numpad3", "rec"), ("Numpad8", "n")]),
            "{name}"
        );
        full += 1;
    }
    // The two profile files, the export and the catalog.
    assert_eq!(full, 4);
    // The full macros hold what the hold leaves.
    let mut held = full_macros();
    crate::loadouts::presets::hold_taken_keys(&mut held, &std::collections::BTreeSet::new());
    assert_eq!(held, full_macros());
}

#[test]
fn the_room_time_and_weather_colors_preset_saves_where_0_8_0_still_reads_the_file() {
    #[derive(serde::Deserialize)]
    struct PresetFile {
        triggers: Vec<Trigger>,
    }
    let preset: PresetFile =
        serde_json::from_str(include_str!("../../../fixtures/room-colors/preset.json")).unwrap();
    let mut profile = crate::profile::live::Profile::default();
    for trigger in preset.triggers {
        profile.triggers.set(trigger).unwrap();
    }
    assert_eq!(
        room_names(&profile.triggers.list()),
        ["room.target", "room.contents"]
    );

    // The profile file of per profile mode.
    let text = profile_bytes(&ProfileConfig::from_profile(&profile));
    let old = old_build_reads(&text).unwrap_or_else(|e| panic!("0.8.0 reads it: {e}\n{text}"));
    assert_eq!(
        old.len(),
        4,
        "the exits, time of day, weather and WiZNET triggers"
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("default.toml");
    std::fs::write(&path, &text).unwrap();
    let loaded = ProfileConfig::load(&path).unwrap();
    assert_eq!(
        room_names(&loaded.triggers),
        ["room.target", "room.contents"]
    );
    assert_eq!(loaded.triggers.len(), 6);

    // catalog.toml in loadout mode.
    let text = catalog_bytes(&GlobalCatalog::from_profile(&profile));
    assert_eq!(
        old_build_reads(&text)
            .unwrap_or_else(|e| panic!("0.8.0 reads it: {e}\n{text}"))
            .len(),
        4
    );
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(catalog_path(dir.path()), &text).unwrap();
    let loaded = load_global_catalog(dir.path()).unwrap();
    assert_eq!(
        room_names(&loaded.triggers),
        ["room.target", "room.contents"]
    );
    assert_eq!(loaded.triggers.len(), 6);
}

#[test]
fn a_your_target_trigger_saves_where_0_8_0_still_reads_the_file() {
    let mut profile = crate::profile::live::Profile::default();
    for (name, target) in [
        ("hp", TriggerTarget::Line),
        ("room", TriggerTarget::Room),
        ("target", TriggerTarget::RoomTarget),
    ] {
        profile
            .triggers
            .set(Trigger {
                priority: 4,
                target,
                ..Trigger::new(name, "^.+$", TriggerAction::Gag)
            })
            .unwrap();
    }
    let text = profile_bytes(&ProfileConfig::from_profile(&profile));
    assert!(text.contains("target = \"room_target\""), "{text}");
    let old = old_build_reads(&text).unwrap_or_else(|e| panic!("0.8.0 reads it: {e}\n{text}"));
    assert_eq!(old.len(), 1, "the Line trigger");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("default.toml");
    std::fs::write(&path, &text).unwrap();
    let loaded = ProfileConfig::load(&path).unwrap();
    assert_eq!(room_names(&loaded.triggers), ["room", "target"]);
    let target = loaded.triggers.iter().find(|t| t.name == "target").unwrap();
    assert_eq!(target.target, TriggerTarget::RoomTarget);

    let text = catalog_bytes(&GlobalCatalog::from_profile(&profile));
    assert_eq!(
        old_build_reads(&text)
            .unwrap_or_else(|e| panic!("0.8.0 reads it: {e}\n{text}"))
            .len(),
        1
    );
}

/// A pattern row as 0.8.1 and older read and save it. They know no
/// `mode` or `text`, and they read `pattern` as a regex.
#[derive(serde::Deserialize, serde::Serialize)]
struct OldRow {
    pattern: String,
    enabled: bool,
}

/// A trigger as those builds read its rows, with its actions kept as
/// they are so it saves again.
#[derive(serde::Deserialize, serde::Serialize)]
struct OldRows {
    name: String,
    patterns: Vec<OldRow>,
    actions: toml::Value,
}

/// The triggers of a profile file or catalog.toml, as those builds read
/// their rows.
#[derive(serde::Deserialize, serde::Serialize)]
struct OldRowsFile {
    triggers: Vec<OldRows>,
}

#[test]
fn match_modes_round_trip_through_toml_and_0_8_1_reads_every_row() {
    let row = |pattern: &str, mode| TriggerPattern {
        mode,
        ..TriggerPattern::regex(pattern)
    };
    let needs = Trigger {
        patterns: vec![
            row("You are thirsty.", MatchMode::Text),
            row("You are hungry", MatchMode::StartsWith),
            row(r"^You are hungry\.$", MatchMode::Regex),
        ],
        ..Trigger::new("needs", "", TriggerAction::Gag)
    };
    let mut profile = crate::profile::live::Profile::default();
    profile.triggers.set(needs.clone()).unwrap();
    let texts = [
        profile_bytes(&ProfileConfig::from_profile(&profile)),
        catalog_bytes(&GlobalCatalog::from_profile(&profile)),
    ];
    for text in &texts {
        assert!(text.contains("mode = \"text\""), "{text}");
        assert!(text.contains("mode = \"starts_with\""), "{text}");
        assert_eq!(text.matches("\nmode = ").count(), 2, "{text}");
        assert!(text.contains("text = \"You are thirsty.\""), "{text}");
        assert!(text.contains("text = \"You are hungry\""), "{text}");
        assert_eq!(text.matches("\ntext = ").count(), 2, "{text}");
        // An older build reads the regex each row compiles to.
        let old: OldRowsFile = toml::from_str(text).unwrap();
        let rows: Vec<&str> = old.triggers[0]
            .patterns
            .iter()
            .map(|r| r.pattern.as_str())
            .collect();
        assert_eq!(
            rows,
            [
                r"^\s*You are thirsty\.\s*$",
                r"^\s*You are hungry.*",
                r"^You are hungry\.$"
            ]
        );
    }
    let loaded = ProfileConfig::from_toml(&texts[0]).unwrap();
    assert_eq!(loaded.triggers, std::slice::from_ref(&needs));
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(catalog_path(dir.path()), &texts[1]).unwrap();
    assert_eq!(load_global_catalog(dir.path()).unwrap().triggers, [needs]);
}

#[test]
fn a_hand_edited_text_row_with_no_pattern_reads_in_a_profile_file_and_catalog_toml() {
    // A Text or Starts with row reads `text`, so a hand edit can leave
    // out `pattern` and the whole file still reads.
    let text = r#"
[[triggers]]
name = "needs"

[[triggers.patterns]]
mode = "text"
text = "You are thirsty."

[[triggers.actions]]
kind = "gag"
"#;
    let needs = Trigger {
        patterns: vec![TriggerPattern {
            mode: MatchMode::Text,
            ..TriggerPattern::regex("You are thirsty.")
        }],
        ..Trigger::new("needs", "", TriggerAction::Gag)
    };
    let loaded = ProfileConfig::from_toml(text).unwrap();
    assert_eq!(loaded.triggers, std::slice::from_ref(&needs));
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(catalog_path(dir.path()), text).unwrap();
    assert_eq!(
        load_global_catalog(dir.path()).unwrap().triggers,
        std::slice::from_ref(&needs)
    );
    // The next save writes the regex an older build reads.
    let saved = profile_bytes(&loaded);
    assert!(
        saved.contains(r"pattern = '^\s*You are thirsty\.\s*$'"),
        "{saved}"
    );
    // A Regex row reads `pattern`, so one with none still fails the file.
    let regex = text.replace("mode = \"text\"\n", "");
    assert!(ProfileConfig::from_toml(&regex).is_err(), "{regex}");
}

/// The plain text of each line of fixtures/room-colors/looks.json that
/// holds a word, then the two lines `do_scan` in the game's `act_move.c`
/// prints for a room too dark to scan, the last one `*** Too Dark ***`.
fn lines_to_match() -> Vec<String> {
    let json: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/room-colors/looks.json")).unwrap();
    let mut out: Vec<String> = Vec::new();
    for case in json["cases"].as_array().unwrap() {
        for event in case["events"].as_array().unwrap() {
            if let Some(line) = event["line"].as_str() {
                let plain = vosh_protocol::ansi::plain_text(line.as_bytes());
                if plain.chars().any(char::is_alphanumeric) && !out.contains(&plain) {
                    out.push(plain);
                }
            }
        }
    }
    out.push("You vision cannot penetrate the blanket of darkness surrounding this room.".into());
    out.push("*** Too Dark ***".into());
    out
}

#[test]
fn a_text_row_saves_a_regex_that_0_8_1_reads_and_matches_the_same_lines() {
    use vosh_automation::trigger::{matching, MatchScope, TriggerStore};

    let lines = lines_to_match();
    assert!(lines.len() > 40, "{}", lines.len());
    // Read as a regex, the text of some of these lines fails, and the text
    // of others misses the very line it came from. That is what 0.8.1 made
    // of a Text row before Vosh saved the regex too.
    let too_dark = lines.last().unwrap();
    assert!(regex::Regex::new(too_dark).is_err(), "{too_dark}");
    for line in [
        "( 2) A pair of black-steel gauntlets rests on the ground.",
        "[AFK] Tolliver is resting here.",
    ] {
        assert!(lines.iter().any(|l| l.trim_start() == line), "{line}");
        assert!(!regex::Regex::new(line).unwrap().is_match(line), "{line}");
    }

    // Each line in Text as you copy it, and its first dozen letters in
    // Starts with, one trigger each.
    let mut profile = crate::profile::live::Profile::default();
    for (i, line) in lines.iter().enumerate() {
        let text = line.trim_start();
        let start = &text[..text.char_indices().nth(12).map_or(text.len(), |(at, _)| at)];
        for (name, copy, mode) in [
            (format!("text {i}"), line.as_str(), MatchMode::Text),
            (format!("starts {i}"), start, MatchMode::StartsWith),
        ] {
            profile
                .triggers
                .set(Trigger {
                    patterns: vec![TriggerPattern {
                        mode,
                        ..TriggerPattern::regex(copy)
                    }],
                    ..Trigger::new(name, "", TriggerAction::Gag)
                })
                .unwrap();
        }
    }
    // The names of the triggers in `store` that match each line.
    let matches = |store: &TriggerStore| -> Vec<Vec<String>> {
        lines
            .iter()
            .map(|line| {
                let mut names: Vec<String> =
                    matching(store, line, MatchScope::Line, StopKey::default())
                        .iter()
                        .map(|t| t.name.clone())
                        .collect();
                names.sort();
                names
            })
            .collect()
    };
    let wanted = matches(&profile.triggers);
    for (line, names) in lines.iter().zip(&wanted) {
        assert!(names.len() >= 2, "{line:?} {names:?}");
    }

    for (file, text) in [
        (
            "profile",
            profile_bytes(&ProfileConfig::from_profile(&profile)),
        ),
        (
            "catalog",
            catalog_bytes(&GlobalCatalog::from_profile(&profile)),
        ),
    ] {
        // 0.8.1 reads each row with no mode and no text. Every regex
        // compiles, so it keeps every trigger, and each matches the lines
        // it matches in this build.
        let old: OldRowsFile = toml::from_str(&text).unwrap();
        assert_eq!(old.triggers.len(), lines.len() * 2, "{file}");
        let mut old_matches = vec![Vec::new(); lines.len()];
        for t in &old.triggers {
            let regex = regex::Regex::new(&t.patterns[0].pattern)
                .unwrap_or_else(|e| panic!("0.8.1 drops {:?} from the {file}: {e}", t.name));
            for (names, line) in old_matches.iter_mut().zip(&lines) {
                if regex.is_match(line) {
                    names.push(t.name.clone());
                }
            }
        }
        for names in &mut old_matches {
            names.sort();
        }
        assert_eq!(old_matches, wanted, "{file}");

        // Its next save writes each row as a regex alone. This build reads
        // it back as a Regex row that matches the same lines.
        let saved = toml::to_string(&old).unwrap();
        assert!(!saved.contains("\nmode = "), "{saved}");
        assert!(!saved.contains("\ntext = "), "{saved}");
        let back = match file {
            "profile" => ProfileConfig::from_toml(&saved).unwrap().triggers,
            _ => {
                let dir = tempfile::tempdir().unwrap();
                std::fs::write(catalog_path(dir.path()), &saved).unwrap();
                load_global_catalog(dir.path()).unwrap().triggers
            }
        };
        assert_eq!(back.len(), lines.len() * 2, "{file}");
        let mut store = TriggerStore::new();
        for t in back {
            assert_eq!(t.patterns[0].mode, MatchMode::Regex, "{file}");
            store.set(t).unwrap();
        }
        assert_eq!(matches(&store), wanted, "{file}");
    }
}

#[test]
fn a_mode_that_is_not_a_name_reads_as_regex_in_a_profile_and_the_catalog() {
    for value in ["1", "true", "[]"] {
        let text = format!(
            "[[triggers]]\nname = \"needs\"\n\n\
             [[triggers.patterns]]\npattern = '^You are hungry\\.$'\nmode = {value}\n\n\
             [[triggers.patterns]]\npattern = \"You are thirsty.\"\nmode = \"text\"\n\n\
             [[triggers.actions]]\nkind = \"gag\"\n"
        );
        let modes = |triggers: &[Trigger]| -> Vec<MatchMode> {
            triggers[0].patterns.iter().map(|p| p.mode).collect()
        };
        let loaded = ProfileConfig::from_toml(&text).unwrap();
        assert_eq!(
            modes(&loaded.triggers),
            [MatchMode::Regex, MatchMode::Text],
            "{value}"
        );
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(catalog_path(dir.path()), &text).unwrap();
        let catalog = load_global_catalog(dir.path()).unwrap();
        assert_eq!(
            modes(&catalog.triggers),
            [MatchMode::Regex, MatchMode::Text],
            "{value}"
        );
    }
}

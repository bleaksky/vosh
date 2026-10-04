//! Config golden files (R2 of the refactor plan).
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

use vosh_automation::alias::Alias;
use vosh_automation::trigger::{
    HighlightStyle, MatchMode, NamedColor, Trigger, TriggerAction, TriggerPattern, TriggerTarget,
};
use vosh_prompt::config::{AabahranCapture, CaptureSource, RegexCapture};
use vosh_prompt::{CaptureConfig, PromptConfig, PromptShow};

use crate::app::state::{AppState, SharedState};
use crate::disk::paths::{catalog_path, loadouts_path};
use crate::disk::save::PERSIST_LOCK;
use crate::loadouts::catalog::{load_global_catalog, save_global_catalog, GlobalCatalog};
use crate::loadouts::set::{load_loadout_set, save_loadout_set, Loadout, LoadoutSet};
use crate::profile::file::{GroupFolders, PluginsPersist, ProfileConfig};
use crate::profile::live::{Macro, Timer};
use crate::profile::login_match::AutoMatch;
use crate::profile::panes::{DockEntryPersist, PaneLayoutPersist, PaneNode};
use crate::profile::set::{ProfileEntry, ProfileSet, ProfilesIndex};
use crate::profile::shared::{GlobalConfig, Scope, ScopeConfig};
use crate::profile::ui::{CustomTheme, TrackedAffect, UiConfig, VitalsConfig};
use crate::tick::TickConfig;

/// The folder that holds the goldens and the old inputs.
const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/config");

/// The switch that writes the goldens again.
const WRITE: &str = "VOSH_WRITE_CONFIG";

/// Every golden, by its path under `fixtures/config`.
const GOLDENS: [&str; 15] = [
    "profile.default.toml",
    "profile.fresh.toml",
    "profile.full.toml",
    "profile.full-regex.toml",
    "global.default.toml",
    "global.full.toml",
    "loadouts.default.toml",
    "loadouts.full.toml",
    "catalog.default.toml",
    "catalog.full.toml",
    "profiles.default.toml",
    "profiles.full.toml",
    "first-save/global.toml",
    "first-save/profiles.toml",
    "first-save/default-profile.toml",
];

/// Every old input, by its path under `fixtures/config`, with the FNV-1a
/// digest of its bytes. An old input never changes, so its digest never
/// does either.
const OLD_INPUTS: [(&str, u64); 7] = [
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
    }
}

fn full_ui() -> UiConfig {
    UiConfig {
        theme: "custom-dusk".into(),
        follow_system_appearance: true,
        light_theme: "kanso-pearl".into(),
        dark_theme: "tokyo-night".into(),
        auto_update: true,
        font_family: "JetBrains Mono, monospace".into(),
        font_size: 16,
        terminal_line_height: "loose".into(),
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
        // readable_highlights tests in ipc/ui_config.rs cover off.
        readable_highlights: true,
        // Written only while on, so off keeps the golden's bytes. The
        // collapse_repeats tests in ipc/ui_config.rs cover on.
        collapse_repeats: false,
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
        side_panels_fill_height: true,
        paste_line_delay_ms: 250,
        spellcheck_prompt: true,
        input_cursor_style: "underline_thick".into(),
        // set_prompt fills both from the [prompt] table.
        prompt_template_enabled: false,
        prompt_template: String::new(),
        vitals: full_vitals(),
        vitals_density: "line".into(),
        vitals_values: "percent".into(),
        vitals_meter: "bar".into(),
        vitals_warn_thirds: true,
        vitals_hide_when_pinned: false,
        moons_position: "before-time".into(),
        chip_style: "icon_value".into(),
        tick_count: "down_past_zero".into(),
        affects_style: "chips".into(),
        affects_marker: "plus_minus".into(),
        affects_tint: true,
        affects_running_out_hours: 5,
        affects_almost_gone_hours: 2,
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
        },
        // A Room trigger, which goes under `room_triggers` (D14).
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
        },
        Macro {
            key: "F2".into(),
            command: "flee".into(),
            group: None,
            enabled: false,
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
        }],
        disabled_alias_groups: vec!["social".into()],
        disabled_trigger_groups: vec!["spam".into()],
        disabled_macro_groups: vec!["travel".into()],
        group_folders: GroupFolders {
            aliases: BTreeMap::from([(
                "combat".into(),
                vec!["combat".into(), "combat (Healer)".into()],
            )]),
            triggers: BTreeMap::from([("comms".into(), vec!["comms".into()])]),
            macros: BTreeMap::from([("travel".into(), Vec::new())]),
        },
        prompt: None,
    };
    config.set_prompt(full_prompt());
    config
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
        terminal_line_height: Some("compact".into()),
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
            // The three empty tables a loadout writes today, which D12
            // with D14 takes out in its own commit.
            Loadout::empty("shared"),
            Loadout {
                name: "warrior".into(),
                description: Some("Melee main".into()),
                auto_match: Some(full_auto_match()),
                enabled_groups: vec!["combat-melee".into(), "wartools".into()],
            },
        ],
    }
}

fn full_catalog() -> GlobalCatalog {
    GlobalCatalog {
        aliases: full_aliases(),
        triggers: full_triggers(),
        macros: full_macros(),
        enabled_presets: Some(vec!["healing_basics".into(), "sent_tells".into()]),
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
        crate::disk::save::persist_state(&state).await;
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

// ---- Rolling back (D14).

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

#[test]
fn match_modes_round_trip_through_toml_and_0_8_0_reads_every_row() {
    /// A pattern row as 0.8.0 and 0.7.2 read it, which skips `mode`.
    #[derive(serde::Deserialize)]
    struct OldRow {
        pattern: String,
    }
    #[derive(serde::Deserialize)]
    struct OldRows {
        patterns: Vec<OldRow>,
    }
    #[derive(serde::Deserialize)]
    struct OldFile {
        triggers: Vec<OldRows>,
    }
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
        // An older build reads each row's text, and reads it as a regex.
        let old: OldFile = toml::from_str(text).unwrap();
        let rows: Vec<&str> = old.triggers[0]
            .patterns
            .iter()
            .map(|r| r.pattern.as_str())
            .collect();
        assert_eq!(
            rows,
            ["You are thirsty.", "You are hungry", r"^You are hungry\.$"]
        );
    }
    let loaded = ProfileConfig::from_toml(&texts[0]).unwrap();
    assert_eq!(loaded.triggers, std::slice::from_ref(&needs));
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(catalog_path(dir.path()), &texts[1]).unwrap();
    assert_eq!(load_global_catalog(dir.path()).unwrap().triggers, [needs]);
}

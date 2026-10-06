use super::*;
use std::str::FromStr;

fn theme(id: &str, custom: bool) -> MenuTheme {
    MenuTheme {
        id: id.to_string(),
        label: id.to_string(),
        custom,
    }
}

#[test]
fn every_accelerator_parses() {
    // Tauri drops an accelerator it cannot parse without a word, so a
    // typo would quietly take a shortcut away.
    let table = accelerators();
    assert!(!table.is_empty(), "the shared shortcut table did not load");
    for (id, accel) in table {
        let parsed = muda::accelerator::Accelerator::from_str(accel);
        assert!(parsed.is_ok(), "{id}: {accel} does not parse: {parsed:?}");
        let mods = parsed.unwrap().modifiers();
        assert!(
            mods.contains(muda::accelerator::Modifiers::SUPER),
            "{id}: {accel} needs Cmd"
        );
        assert!(
            !mods.contains(muda::accelerator::Modifiers::CONTROL),
            "{id}: {accel} takes Ctrl, which belongs to your macros"
        );
    }
}

#[test]
fn the_board_shortcuts_are_all_there() {
    let expect = [
        ("settings", "Cmd+,"),
        ("connect", "Cmd+R"),
        ("close-window", "Cmd+W"),
        ("copy", "Cmd+C"),
        ("find", "Cmd+F"),
        ("palette", "Cmd+K"),
        ("panel", "Cmd+Shift+L"),
        ("split", "Cmd+\\"),
        ("help", "Cmd+/"),
    ];
    for (id, accel) in expect {
        assert_eq!(accelerator(id), Some(accel), "{id}");
    }
    assert_eq!(accelerator("disconnect"), None);
}

#[test]
fn specs_map_mod_to_cmd() {
    assert_eq!(spec_to_accelerator("Mod+Shift+L"), "Cmd+Shift+L");
    assert_eq!(spec_to_accelerator("Mod+\\"), "Cmd+\\");
    assert_eq!(spec_to_accelerator("Mod++"), "Cmd++");
}

#[test]
fn quit_goes_through_the_exit_request_on_the_system_shortcut() {
    assert_eq!(route("quit"), Route::Quit);
    let parsed = muda::accelerator::Accelerator::from_str(QUIT_ACCELERATOR).unwrap();
    assert_eq!(
        parsed,
        muda::accelerator::Accelerator::new(
            Some(muda::accelerator::Modifiers::SUPER),
            muda::accelerator::Code::KeyQ
        )
    );
    // Quit is not a page command, so the shared table leaves it out.
    assert_eq!(accelerator("quit"), None);
}

#[test]
fn routes_follow_the_board() {
    assert_eq!(route("settings"), Route::OpenSettings);
    // Help opens its own window from wherever you are, so the main
    // window never has to be in front for it.
    assert_eq!(route("help"), Route::OpenHelp);
    assert_eq!(route("close-window"), Route::CloseFront);
    // Close session closes Settings or Help in front, as Close window
    // does, and never a game behind them.
    assert_eq!(route("session-close"), Route::CloseFront);
    assert_eq!(route("copy"), Route::Copy);
    assert_eq!(route("find"), Route::Find);
    assert_eq!(route("palette"), Route::Main { raise: true });
    assert_eq!(route("connect"), Route::Main { raise: true });
    assert_eq!(route("panel"), Route::Main { raise: true });
    assert_eq!(route("theme-nord"), Route::Main { raise: false });
}

#[test]
fn check_rows_are_the_toggles_and_themes() {
    for id in ["panel", "split", "pane-map", "pane-imm", "theme-nord"] {
        assert!(is_check_id(id), "{id}");
    }
    for id in ["connect", "palette", "panel-reset", "copy", "theme"] {
        assert!(!is_check_id(id), "{id}");
    }
}

#[test]
fn connect_names_the_world_when_there_is_one() {
    assert_eq!(
        connect_label(Some("The Forsaken Lands")),
        "Connect to The Forsaken Lands"
    );
    assert_eq!(connect_label(Some("  ")), "Connect");
    assert_eq!(connect_label(None), "Connect");
}

#[test]
fn custom_themes_follow_a_separator() {
    let themes = [
        theme("nord", false),
        theme("vellum", false),
        theme("mine", true),
    ];
    let rows = theme_rows(&themes);
    assert_eq!(
        rows,
        vec![
            ThemeRow::Theme(&themes[0]),
            ThemeRow::Theme(&themes[1]),
            ThemeRow::Separator,
            ThemeRow::Theme(&themes[2]),
        ]
    );
    let builtin_only = [theme("nord", false)];
    assert_eq!(
        theme_rows(&builtin_only),
        vec![ThemeRow::Theme(&builtin_only[0])]
    );
}

#[test]
fn staff_queues_waits_for_the_offer() {
    let mut state = MenuState {
        connected: true,
        world_name: None,
        panel_open: true,
        split_open: false,
        panes: vec![MenuPane {
            pane: "imm".to_string(),
            visible: false,
            offered: false,
        }],
        themes: Vec::new(),
        theme: "nord".to_string(),
    };
    assert!(!staff_listed(&state));
    state.panes[0].offered = true;
    assert!(staff_listed(&state));
}

#[test]
fn state_reads_camel_case() {
    let json = r#"{
        "connected": true,
        "worldName": "The Forsaken Lands",
        "panelOpen": true,
        "splitOpen": false,
        "panes": [{ "pane": "map", "visible": true, "offered": true }],
        "themes": [{ "id": "nord", "label": "Nord", "custom": false }],
        "theme": "nord"
    }"#;
    let state: MenuState = serde_json::from_str(json).unwrap();
    assert!(state.connected);
    assert_eq!(state.world_name.as_deref(), Some("The Forsaken Lands"));
    assert!(state.panel_open);
    assert_eq!(state.panes[0].pane, "map");
    assert_eq!(state.themes[0].label, "Nord");
}

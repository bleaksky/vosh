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
        // Ctrl belongs to your macros, save beside Cmd for the sessions
        // toggle, where a macro of yours on the key still wins.
        assert!(
            id == "sessions-sidebar" || !mods.contains(muda::accelerator::Modifiers::CONTROL),
            "{id}: {accel} takes Ctrl, which belongs to your macros"
        );
    }
}

#[test]
fn the_board_shortcuts_are_all_there() {
    let expect = [
        ("settings", "Cmd+,"),
        ("settings-automation:timers", "Cmd+Alt+1"),
        ("settings-automation:aliases", "Cmd+Alt+2"),
        ("settings-automation:triggers", "Cmd+Alt+3"),
        ("settings-automation:macros", "Cmd+Alt+4"),
        ("connect", "Cmd+R"),
        ("session-new", "Cmd+T"),
        ("session-close", "Cmd+W"),
        ("close-window", "Cmd+Shift+W"),
        ("session-next", "Cmd+Shift+]"),
        ("session-previous", "Cmd+Shift+["),
        ("copy", "Cmd+C"),
        ("find", "Cmd+F"),
        ("palette", "Cmd+K"),
        ("panel", "Cmd+Shift+L"),
        ("sessions-sidebar", "Ctrl+Cmd+S"),
        ("split", "Cmd+\\"),
        ("snoop", "Cmd+J"),
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
    assert_eq!(spec_to_accelerator("Mod+Shift+]"), "Cmd+Shift+]");
    assert_eq!(spec_to_accelerator("Mod+Shift+["), "Cmd+Shift+[");
}

#[test]
fn show_sessions_binds_ctrl_cmd_s() {
    // The key AppKit gives a standard Show Sidebar row. The shortcut
    // file names one key for macOS and one for the rest.
    use muda::accelerator::{Accelerator, Code, Modifiers};
    assert_eq!(spec_to_accelerator("Ctrl+Mod+S"), "Ctrl+Cmd+S");
    let parsed = Accelerator::from_str(accelerator("sessions-sidebar").unwrap()).unwrap();
    assert_eq!(
        parsed,
        Accelerator::new(Some(Modifiers::CONTROL | Modifiers::SUPER), Code::KeyS)
    );
}

#[test]
fn the_settings_keys_bind_cmd_option_and_the_digit_keys() {
    // Cmd Shift 3 and 4 are the macOS screenshot keys, which never
    // reach Vosh, so the Settings keys take Option. Option changes
    // what a digit types, and the menu binds the physical digit key.
    use muda::accelerator::{Accelerator, Code, Modifiers};
    for (id, code) in [
        ("settings-automation:timers", Code::Digit1),
        ("settings-automation:aliases", Code::Digit2),
        ("settings-automation:triggers", Code::Digit3),
        ("settings-automation:macros", Code::Digit4),
    ] {
        let parsed = Accelerator::from_str(accelerator(id).unwrap()).unwrap();
        assert_eq!(
            parsed,
            Accelerator::new(Some(Modifiers::SUPER | Modifiers::ALT), code),
            "{id}"
        );
    }
}

#[test]
fn every_spec_in_the_file_reaches_the_menu() {
    // A spec the table cannot read would drop every accelerator at once.
    let specs: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(SHORTCUTS_JSON).unwrap();
    assert_eq!(accelerators().len(), specs.len());
}

#[test]
fn the_step_keys_bind_the_bracket_keys() {
    // The menu shows them as the board draws them, Shift Cmd ] and Shift
    // Cmd [, on the bracket keys.
    use muda::accelerator::{Accelerator, Code, Modifiers};
    let shift_cmd = Some(Modifiers::SHIFT | Modifiers::SUPER);
    for (id, code) in [
        ("session-next", Code::BracketRight),
        ("session-previous", Code::BracketLeft),
    ] {
        let parsed = Accelerator::from_str(accelerator(id).unwrap()).unwrap();
        assert_eq!(parsed, Accelerator::new(shift_cmd, code), "{id}");
    }
}

#[test]
fn the_session_menu_follows_board_4() {
    let rows: Vec<String> = SESSION_ROWS
        .iter()
        .map(|row| match row {
            SessionRow::Item(id, text) => match accelerator(id) {
                Some(keys) => format!("{text} {keys}"),
                None => (*text).to_string(),
            },
            SessionRow::Separator => "-".to_string(),
        })
        .collect();
    assert_eq!(
        rows,
        [
            "Edit connection…",
            "-",
            "New session… Cmd+T",
            "Next session Cmd+Shift+]",
            "Previous session Cmd+Shift+[",
            "-",
            "Rename session…",
            "Close session Cmd+W",
            "Close window Cmd+Shift+W",
            "Save profile",
        ]
    );
}

#[test]
fn the_session_rows_dim_with_one_session() {
    let mut state = menu_state();
    for (open, enabled) in [(0, false), (1, false), (2, true), (9, true)] {
        state.sessions = open;
        assert_eq!(between_sessions(&state), enabled, "{open} sessions");
    }
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
fn quit_asks_only_while_two_or_more_sessions_are_connected() {
    // None or one connected quits at once, as Quit did before sessions.
    assert!(!quit_asks(0));
    assert!(!quit_asks(1));
    // Two or more raise the main window, which asks first.
    assert!(quit_asks(2));
    assert!(quit_asks(3));
}

#[test]
fn routes_follow_the_board() {
    assert_eq!(route("settings"), Route::OpenSettings);
    // Help opens its own window from wherever you are, so the main
    // window never has to be in front for it.
    assert_eq!(route("help"), Route::OpenHelp);
    // Get started opens in the main window, so it comes forward first.
    assert_eq!(route("get-started"), Route::Main { raise: true });
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
    // A Settings page opens in Settings, so a press from Settings or
    // Help leaves Settings in front.
    for id in [
        "settings-automation:timers",
        "settings-automation:aliases",
        "settings-automation:triggers",
        "settings-automation:macros",
    ] {
        assert_eq!(route(id), Route::Main { raise: false }, "{id}");
    }
    // A step or the sidebar from Settings brings the main window up.
    assert_eq!(route("session-next"), Route::Main { raise: true });
    assert_eq!(route("session-previous"), Route::Main { raise: true });
    assert_eq!(route("sessions-sidebar"), Route::Main { raise: true });
    // Go to snoop runs in the main window as Split terminal does, which
    // brings the snoop window forward when the snoops sit there.
    assert_eq!(route("snoop"), route("split"));
}

#[test]
fn check_rows_are_the_toggles_and_themes() {
    for id in [
        "sessions-sidebar",
        "panel",
        "split",
        "pane-map",
        "pane-imm",
        "theme-nord",
    ] {
        assert!(is_check_id(id), "{id}");
    }
    for id in [
        "connect",
        "palette",
        "panel-reset",
        "copy",
        "theme",
        "session-next",
        "session-previous",
        "snoop",
    ] {
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

/// A snapshot with one session, connected, and Staff queues not offered.
fn menu_state() -> MenuState {
    MenuState {
        connected: true,
        redialing: false,
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
        sessions: 1,
        sessions_shown: false,
        snoops: 0,
    }
}

#[test]
fn staff_queues_waits_for_the_offer() {
    let mut state = menu_state();
    assert!(!staff_listed(&state));
    state.panes[0].offered = true;
    assert!(staff_listed(&state));
}

#[test]
fn go_to_snoop_shows_while_a_snoop_is_open() {
    let mut state = menu_state();
    assert!(!snoop_listed(&state));
    state.snoops = 1;
    assert!(snoop_listed(&state));
}

#[test]
fn a_page_without_snoops_reads_none() {
    // A snapshot from a page before snoops still reads, with no snoop.
    let json = r#"{
        "connected": false,
        "worldName": null,
        "panelOpen": true,
        "splitOpen": false,
        "panes": [],
        "themes": [],
        "theme": "nord",
        "sessions": 1,
        "sessionsShown": false
    }"#;
    let state: MenuState = serde_json::from_str(json).unwrap();
    assert_eq!(state.snoops, 0);
}

#[test]
fn disconnect_shows_while_a_redial_runs() {
    let mut state = menu_state();
    assert!(shows_disconnect(&state));
    state.connected = false;
    assert!(!shows_disconnect(&state));
    // A try that waits has no link, and Disconnect still ends the tries.
    state.redialing = true;
    assert!(shows_disconnect(&state));
}

#[test]
fn state_reads_camel_case() {
    let json = r#"{
        "connected": true,
        "redialing": true,
        "worldName": "The Forsaken Lands",
        "panelOpen": true,
        "splitOpen": false,
        "panes": [{ "pane": "map", "visible": true, "offered": true }],
        "themes": [{ "id": "nord", "label": "Nord", "custom": false }],
        "theme": "nord",
        "sessions": 2,
        "sessionsShown": true,
        "snoops": 3
    }"#;
    let state: MenuState = serde_json::from_str(json).unwrap();
    assert!(state.connected);
    assert!(state.redialing);
    assert_eq!(state.world_name.as_deref(), Some("The Forsaken Lands"));
    assert!(state.panel_open);
    assert_eq!(state.panes[0].pane, "map");
    assert_eq!(state.themes[0].label, "Nord");
    assert_eq!(state.sessions, 2);
    assert!(state.sessions_shown);
    assert_eq!(state.snoops, 3);
}

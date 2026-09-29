//! The macOS menu bar (the approved `MenuBar` board). Rust owns the menu,
//! so it is there before the page loads and survives a page reload, and
//! Settings, Copy, and Close window work whichever window is in front.
//!
//! Vosh commands reach the main window as `vosh://app-menu` with the
//! palette entry id as the payload, and App.tsx runs them through the
//! same dispatcher as its keyboard shortcuts. The page owns the truth
//! for every check mark and label: it pushes a [`MenuState`] snapshot
//! through [`menu_set_state`] whenever one changes, and the menu only
//! mirrors it.
//!
//! Windows and Linux get no menu bar. Tauri would attach an app menu to
//! every frameless window there, so the builder only installs this one
//! on macOS, and the two commands below are no-ops elsewhere.

use serde::Deserialize;
use tauri::AppHandle;

/// Where the page tells the menu what to show. `camelCase` on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) struct MenuState {
    /// Connecting or connected. Swaps Connect for Disconnect.
    pub(crate) connected: bool,
    /// The world Connect dials, like `The Forsaken Lands`.
    pub(crate) world_name: Option<String>,
    pub(crate) panel_open: bool,
    /// Scrollback is open above the live terminal.
    pub(crate) split_open: bool,
    pub(crate) panes: Vec<MenuPane>,
    /// Every theme in gallery order.
    pub(crate) themes: Vec<MenuTheme>,
    /// The theme in use now.
    pub(crate) theme: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) struct MenuPane {
    /// A pane type: map, affects, group, chat, or imm.
    pub(crate) pane: String,
    /// Shown in the panel while the panel is open.
    pub(crate) visible: bool,
    /// Listed in the menu. Staff queues waits for Imm.Queues.
    pub(crate) offered: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) struct MenuTheme {
    pub(crate) id: String,
    pub(crate) label: String,
    /// Your own theme. Custom themes follow a separator.
    #[serde(default)]
    pub(crate) custom: bool,
}

/// Mirror the page's state in the menu. Sync, so it runs on the main
/// thread and the menu setters run inline.
#[tauri::command]
pub(crate) fn menu_set_state(app: AppHandle, state: MenuState) {
    #[cfg(target_os = "macos")]
    mac::apply_state(&app, &state);
    #[cfg(not(target_os = "macos"))]
    let _ = (app, state);
}

/// Edit, then Copy, from the main window. `terminal` is true when the
/// page holds no text selection of its own, and then a native terminal
/// selection wins. Anything else copies the way the system would, from
/// the focused field or the page selection.
#[tauri::command]
pub(crate) fn menu_copy(app: AppHandle, terminal: bool) {
    #[cfg(target_os = "macos")]
    {
        if terminal && crate::term_grid::selection_text().is_some_and(|t| !t.is_empty()) {
            crate::native_surface::request_copy();
        } else {
            let _ = app.run_on_main_thread(mac::system_copy);
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, terminal);
}

#[cfg(target_os = "macos")]
pub(crate) use mac::{build, on_event};

/// The event a menu command reaches the main window on.
#[cfg(target_os = "macos")]
const APP_MENU_EVENT: &str = "vosh://app-menu";

/// Find, chosen while Settings is in front, focuses the settings search.
#[cfg(target_os = "macos")]
const SETTINGS_FIND_EVENT: &str = "vosh://settings-find";

/// The shortcut specs the menu, the palette keycaps, and the page's
/// keydown handler share, so they cannot drift apart.
#[cfg(target_os = "macos")]
const SHORTCUTS_JSON: &str = include_str!("../../src/lib/appShortcuts.json");

/// Pane rows in View, in the panel's order.
#[cfg(target_os = "macos")]
const PANE_ROWS: [(&str, &str); 5] = [
    ("map", "Show map"),
    ("affects", "Show affects"),
    ("group", "Show group"),
    ("chat", "Show chat"),
    ("imm", "Show staff queues"),
];

/// A palette spec like `Mod+Shift+L` as a menu accelerator. Mod is Cmd,
/// because Ctrl belongs to your macros on macOS.
#[cfg(target_os = "macos")]
fn spec_to_accelerator(spec: &str) -> String {
    if spec == "Mod++" {
        return "Cmd++".to_string();
    }
    spec.split('+')
        .map(|part| {
            if part.eq_ignore_ascii_case("mod") {
                "Cmd"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// Every command id with a shortcut, and its accelerator.
#[cfg(target_os = "macos")]
fn accelerators() -> &'static std::collections::BTreeMap<String, String> {
    static TABLE: std::sync::OnceLock<std::collections::BTreeMap<String, String>> =
        std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let specs: std::collections::BTreeMap<String, String> =
            serde_json::from_str(SHORTCUTS_JSON).unwrap_or_default();
        specs
            .into_iter()
            .map(|(id, spec)| {
                let accel = spec_to_accelerator(&spec);
                (id, accel)
            })
            .collect()
    })
}

#[cfg(target_os = "macos")]
fn accelerator(id: &str) -> Option<&'static str> {
    accelerators().get(id).map(String::as_str)
}

/// Where a menu command runs.
#[cfg(target_os = "macos")]
#[derive(Debug, PartialEq, Eq)]
enum Route {
    /// Open Settings, or bring it to the front.
    OpenSettings,
    /// Close the window in front. The main window asks first while you
    /// are connected.
    CloseWindow,
    /// Copy in the window in front.
    Copy,
    /// Find in the window in front: settings search in Settings, the
    /// find bar in the main window.
    Find,
    /// Run in the main window, raising it first unless `raise` is off.
    Main { raise: bool },
    /// Quit through the exit request, so the windows send their pending
    /// writes first.
    Quit,
}

/// The shortcut for Quit Vosh, the one the system Quit row uses.
#[cfg(target_os = "macos")]
const QUIT_ACCELERATOR: &str = "Cmd+Q";

#[cfg(target_os = "macos")]
fn route(id: &str) -> Route {
    match id {
        "quit" => Route::Quit,
        "settings" => Route::OpenSettings,
        "close-window" => Route::CloseWindow,
        "copy" => Route::Copy,
        "find" => Route::Find,
        // A theme repaints every window, so picking one from Settings
        // leaves Settings in front.
        _ if id.starts_with("theme-") => Route::Main { raise: false },
        _ => Route::Main { raise: true },
    }
}

/// Ids of the rows that carry a check mark.
#[cfg(target_os = "macos")]
fn is_check_id(id: &str) -> bool {
    id == "panel" || id == "split" || id.starts_with("pane-") || id.starts_with("theme-")
}

/// The first Session row while you are not connected.
#[cfg(target_os = "macos")]
fn connect_label(world: Option<&str>) -> String {
    match world.map(str::trim) {
        Some(name) if !name.is_empty() => format!("Connect to {name}"),
        _ => "Connect".to_string(),
    }
}

/// Whether View lists Show staff queues.
#[cfg(target_os = "macos")]
fn staff_listed(state: &MenuState) -> bool {
    state.panes.iter().any(|p| p.pane == "imm" && p.offered)
}

/// A row in Choose theme.
#[cfg(target_os = "macos")]
#[derive(Debug, PartialEq, Eq)]
enum ThemeRow<'a> {
    Theme(&'a MenuTheme),
    Separator,
}

/// The built in themes in gallery order, then a line, then your own.
#[cfg(target_os = "macos")]
fn theme_rows(themes: &[MenuTheme]) -> Vec<ThemeRow<'_>> {
    let (builtin, custom): (Vec<&MenuTheme>, Vec<&MenuTheme>) =
        themes.iter().partition(|t| !t.custom);
    let mut rows: Vec<ThemeRow<'_>> = builtin.into_iter().map(ThemeRow::Theme).collect();
    if !custom.is_empty() {
        if !rows.is_empty() {
            rows.push(ThemeRow::Separator);
        }
        rows.extend(custom.into_iter().map(ThemeRow::Theme));
    }
    rows
}

#[cfg(target_os = "macos")]
mod mac {
    use std::sync::{Mutex, MutexGuard, PoisonError};

    use tauri::menu::{
        AboutMetadata, CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem,
        Submenu, HELP_SUBMENU_ID, WINDOW_SUBMENU_ID,
    };
    use tauri::{AppHandle, Emitter, Manager, Wry};
    use tracing::warn;

    use super::{
        accelerator, connect_label, is_check_id, route, staff_listed, theme_rows, MenuState,
        MenuTheme, Route, ThemeRow, APP_MENU_EVENT, PANE_ROWS, QUIT_ACCELERATOR,
        SETTINGS_FIND_EVENT,
    };

    const COPYRIGHT: &str = "Copyright © 2026 James Wright";

    /// The rows that change after launch. `Menu::get` only searches the
    /// top level, so the builder keeps these.
    pub(super) struct MenuHandles {
        session: Submenu<Wry>,
        connect: MenuItem<Wry>,
        disconnect_sep: PredefinedMenuItem<Wry>,
        disconnect: MenuItem<Wry>,
        view: Submenu<Wry>,
        panel: CheckMenuItem<Wry>,
        split: CheckMenuItem<Wry>,
        panes: Vec<(&'static str, CheckMenuItem<Wry>)>,
        themes: Submenu<Wry>,
        applied: Mutex<Applied>,
    }

    /// What the menu shows now, so a snapshot only touches what changed.
    struct Applied {
        connected: bool,
        connect_label: String,
        staff_listed: bool,
        themes: Vec<MenuTheme>,
        theme_items: Vec<CheckMenuItem<Wry>>,
    }

    impl MenuHandles {
        fn applied(&self) -> MutexGuard<'_, Applied> {
            self.applied.lock().unwrap_or_else(PoisonError::into_inner)
        }
    }

    /// Build the menu bar, exactly the board's menus and order, and keep
    /// the rows that change. Runs once, in the Builder's `.menu()`.
    pub(crate) fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
        let sep = || PredefinedMenuItem::separator(app);
        let item = |id: &str, text: &str| MenuItem::with_id(app, id, text, true, accelerator(id));
        let check = |id: &str, text: &str| {
            CheckMenuItem::with_id(app, id, text, true, false, accelerator(id))
        };

        let about = AboutMetadata {
            name: Some("Vosh".to_string()),
            version: Some(app.package_info().version.to_string()),
            copyright: Some(COPYRIGHT.to_string()),
            ..Default::default()
        };
        let vosh = Submenu::with_items(
            app,
            "Vosh",
            true,
            &[
                &PredefinedMenuItem::about(app, Some("About Vosh"), Some(about))?,
                &sep()?,
                &item("settings", "Settings…")?,
                &sep()?,
                &PredefinedMenuItem::services(app, Some("Services"))?,
                &sep()?,
                &PredefinedMenuItem::hide(app, Some("Hide Vosh"))?,
                &PredefinedMenuItem::hide_others(app, Some("Hide others"))?,
                &PredefinedMenuItem::show_all(app, Some("Show all"))?,
                &sep()?,
                // Quit is Vosh's own row. The system row ends the app
                // with Exit alone, which leaves no time to ask the
                // windows for the edits they hold back. This one exits
                // through the exit request, which asks them first and
                // then saves the profile once (exit_flush.rs).
                &MenuItem::with_id(app, "quit", "Quit Vosh", true, Some(QUIT_ACCELERATOR))?,
            ],
        )?;

        // Session starts as you launch: not connected. The page sends
        // the world name and the connection state at once.
        let connect = item("connect", &connect_label(None))?;
        let session = Submenu::with_items(
            app,
            "Session",
            true,
            &[
                &connect,
                &item("session-edit", "Edit connection…")?,
                &sep()?,
                &item("session-new", "New connection…")?,
                &sep()?,
                &item("close-window", "Close window")?,
                &item("profile-save", "Save profile")?,
            ],
        )?;
        let disconnect_sep = sep()?;
        let disconnect = item("disconnect", "Disconnect")?;

        // Copy is Vosh's own row so it can reach the terminal selection.
        // The rest are the system's, and act on the focused field.
        let edit = Submenu::with_items(
            app,
            "Edit",
            true,
            &[
                &PredefinedMenuItem::undo(app, Some("Undo"))?,
                &PredefinedMenuItem::redo(app, Some("Redo"))?,
                &sep()?,
                &PredefinedMenuItem::cut(app, Some("Cut"))?,
                &item("copy", "Copy")?,
                &PredefinedMenuItem::paste(app, Some("Paste"))?,
                &PredefinedMenuItem::select_all(app, Some("Select all"))?,
                &sep()?,
                &item("find", "Find in scrollback…")?,
            ],
        )?;

        let panel = check("panel", "Show panel")?;
        let split = check("split", "Split terminal")?;
        let mut panes = Vec::with_capacity(PANE_ROWS.len());
        for (pane, title) in PANE_ROWS {
            panes.push((pane, check(&format!("pane-{pane}"), title)?));
        }
        let themes = Submenu::with_id(app, "theme", "Choose theme", true)?;
        let mut view_items: Vec<&dyn IsMenuItem<Wry>> = Vec::new();
        let search = item("palette", "Search commands…")?;
        let sep_a = sep()?;
        let sep_b = sep()?;
        let sep_c = sep()?;
        let sep_d = sep()?;
        let reset = item("panel-reset", "Reset panel layout")?;
        // AppKit words this row itself, Enter Full Screen or Exit Full
        // Screen, and runs it.
        let fullscreen = PredefinedMenuItem::fullscreen(app, Some("Enter Full Screen"))?;
        view_items.push(&search);
        view_items.push(&sep_a);
        view_items.push(&panel);
        view_items.push(&split);
        view_items.push(&sep_b);
        // Staff queues joins once the MUD offers it.
        for (pane, row) in &panes {
            if *pane != "imm" {
                view_items.push(row);
            }
        }
        view_items.push(&reset);
        view_items.push(&sep_c);
        view_items.push(&themes);
        view_items.push(&sep_d);
        view_items.push(&fullscreen);
        let view = Submenu::with_items(app, "View", true, &view_items)?;

        // AppKit adds its own rows and the window list to this one.
        let window = Submenu::with_id_and_items(
            app,
            WINDOW_SUBMENU_ID,
            "Window",
            true,
            &[
                &PredefinedMenuItem::minimize(app, Some("Minimize"))?,
                &PredefinedMenuItem::maximize(app, Some("Zoom"))?,
                &sep()?,
                &PredefinedMenuItem::bring_all_to_front(app, Some("Bring all to front"))?,
            ],
        )?;
        // AppKit adds the help search field above this row.
        let help = Submenu::with_id_and_items(
            app,
            HELP_SUBMENU_ID,
            "Help",
            true,
            &[&item("help", "Vosh help")?],
        )?;

        let menu = Menu::with_items(app, &[&vosh, &session, &edit, &view, &window, &help])?;
        app.manage(MenuHandles {
            session,
            connect,
            disconnect_sep,
            disconnect,
            view,
            panel,
            split,
            panes,
            themes,
            applied: Mutex::new(Applied {
                connected: false,
                connect_label: connect_label(None),
                staff_listed: false,
                themes: Vec::new(),
                theme_items: Vec::new(),
            }),
        });
        Ok(menu)
    }

    /// A menu click. Predefined rows never get here, since `AppKit` runs
    /// them, except About, which muda shows itself.
    pub(crate) fn on_event(app: &AppHandle, event: MenuEvent) {
        let id = event.id().as_ref();
        // muda flips a check row before it reports the click, but the
        // page owns the truth and sends it back once it acts. Put the
        // mark back, so a click the page ignores leaves no wrong mark.
        if is_check_id(id) {
            revert_check(app, id);
        }
        match route(id) {
            Route::OpenSettings => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = crate::commands::open_settings_window(app).await {
                        warn!(error = %e, "menu: opening settings failed");
                    }
                });
            }
            Route::Quit => app.exit(0),
            Route::CloseWindow => {
                if is_front(app, "settings") {
                    if let Some(settings) = app.get_webview_window("settings") {
                        // A close request, the same as the close button.
                        // The page's close handler (useSettingsClose)
                        // sends the edits it holds back, then closes.
                        let _ = settings.close();
                    }
                } else if is_front(app, "main") {
                    emit_main(app, id);
                } else {
                    system_close();
                }
            }
            Route::Copy => {
                if is_front(app, "main") {
                    emit_main(app, id);
                } else {
                    system_copy();
                }
            }
            Route::Find => {
                if is_front(app, "settings") {
                    let _ = app.emit_to("settings", SETTINGS_FIND_EVENT, ());
                } else {
                    raise_main(app);
                    emit_main(app, id);
                }
            }
            Route::Main { raise } => {
                if raise {
                    raise_main(app);
                }
                emit_main(app, id);
            }
        }
    }

    /// Mirror the page's snapshot.
    pub(super) fn apply_state(app: &AppHandle, state: &MenuState) {
        let Some(handles) = app.try_state::<MenuHandles>() else {
            return;
        };
        let h = handles.inner();
        let mut applied = h.applied();

        let label = connect_label(state.world_name.as_deref());
        if applied.connect_label != label {
            log_err(h.connect.set_text(&label), "connect label");
            applied.connect_label = label;
        }
        if applied.connected != state.connected {
            if state.connected {
                // Disconnect takes the last row, on its own.
                log_err(h.session.remove(&h.connect), "remove connect");
                log_err(h.session.append(&h.disconnect_sep), "add separator");
                log_err(h.session.append(&h.disconnect), "add disconnect");
            } else {
                log_err(h.session.remove(&h.disconnect), "remove disconnect");
                log_err(h.session.remove(&h.disconnect_sep), "remove separator");
                log_err(h.session.prepend(&h.connect), "add connect");
            }
            applied.connected = state.connected;
        }

        log_err(h.panel.set_checked(state.panel_open), "panel check");
        log_err(h.split.set_checked(state.split_open), "split check");
        for (pane, row) in &h.panes {
            let visible = state.panes.iter().any(|p| p.pane == *pane && p.visible);
            log_err(row.set_checked(visible), "pane check");
        }

        let staff = staff_listed(state);
        if applied.staff_listed != staff {
            if let Some((_, row)) = h.panes.iter().find(|(pane, _)| *pane == "imm") {
                if staff {
                    let at = h
                        .view
                        .items()
                        .ok()
                        .and_then(|items| items.iter().position(|i| i.id() == "pane-chat"))
                        .map_or(0, |chat| chat + 1);
                    log_err(h.view.insert(row, at), "add staff queues");
                } else {
                    log_err(h.view.remove(row), "remove staff queues");
                }
            }
            applied.staff_listed = staff;
        }

        if applied.themes != state.themes {
            rebuild_themes(app, &h.themes, &state.themes, &mut applied);
        }
        let current = format!("theme-{}", state.theme);
        for row in &applied.theme_items {
            log_err(row.set_checked(row.id() == current), "theme check");
        }
    }

    /// Replace Choose theme's rows. Runs only when the list changed.
    fn rebuild_themes(
        app: &AppHandle,
        submenu: &Submenu<Wry>,
        themes: &[MenuTheme],
        applied: &mut Applied,
    ) {
        if let Ok(items) = submenu.items() {
            for old in items {
                log_err(submenu.remove(&old), "remove theme row");
            }
        }
        applied.theme_items.clear();
        for row in theme_rows(themes) {
            match row {
                ThemeRow::Separator => match PredefinedMenuItem::separator(app) {
                    Ok(line) => log_err(submenu.append(&line), "add theme separator"),
                    Err(e) => warn!(error = %e, "menu: theme separator failed"),
                },
                ThemeRow::Theme(theme) => {
                    let id = format!("theme-{}", theme.id);
                    match CheckMenuItem::with_id(app, id, &theme.label, true, false, None::<&str>) {
                        Ok(item) => {
                            log_err(submenu.append(&item), "add theme row");
                            applied.theme_items.push(item);
                        }
                        Err(e) => warn!(error = %e, "menu: theme row failed"),
                    }
                }
            }
        }
        applied.themes = themes.to_vec();
    }

    /// Undo muda's flip on the row that was clicked.
    fn revert_check(app: &AppHandle, id: &str) {
        let Some(handles) = app.try_state::<MenuHandles>() else {
            return;
        };
        let h = handles.inner();
        let flip = |row: &CheckMenuItem<Wry>| {
            if let Ok(on) = row.is_checked() {
                log_err(row.set_checked(!on), "revert check");
            }
        };
        match id {
            "panel" => flip(&h.panel),
            "split" => flip(&h.split),
            _ => {
                if let Some((_, row)) = h
                    .panes
                    .iter()
                    .find(|(pane, _)| id == format!("pane-{pane}"))
                {
                    flip(row);
                } else if let Some(row) = h.applied().theme_items.iter().find(|r| r.id() == id) {
                    flip(row);
                }
            }
        }
    }

    /// Whether the window `label` is the key window.
    fn is_front(app: &AppHandle, label: &str) -> bool {
        app.get_webview_window(label)
            .and_then(|w| w.is_focused().ok())
            .unwrap_or(false)
    }

    /// Bring the main window forward, out of the Dock if minimized.
    fn raise_main(app: &AppHandle) {
        if let Some(main) = app.get_webview_window("main") {
            let _ = main.unminimize();
            let _ = main.show();
            let _ = main.set_focus();
        }
    }

    fn emit_main(app: &AppHandle, id: &str) {
        if let Err(e) = app.emit_to("main", APP_MENU_EVENT, id) {
            warn!(error = %e, id, "menu: sending the command to the main window failed");
        }
    }

    fn log_err(result: tauri::Result<()>, what: &str) {
        if let Err(e) = result {
            warn!(error = %e, what, "menu: update failed");
        }
    }

    /// Copy the way the system Copy row would: send `copy:` down the
    /// responder chain of the key window. Main thread only.
    pub(super) fn system_copy() {
        send_action(objc2::sel!(copy:));
    }

    /// Close a window Vosh does not own, like the About panel, the way
    /// the system Close row would. Main thread only.
    fn system_close() {
        send_action(objc2::sel!(performClose:));
    }

    /// Send `action` to the first responder of the key window, as a
    /// menu row with no target does.
    #[allow(unsafe_code)]
    fn send_action(action: objc2::runtime::Sel) {
        use objc2::runtime::{AnyObject, Bool};
        use objc2::{class, msg_send};
        // SAFETY: NSApplication's shared instance and a nil target send
        // a standard action, on the main thread, as the menu would.
        unsafe {
            let nsapp: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
            if nsapp.is_null() {
                return;
            }
            let nil: *mut AnyObject = std::ptr::null_mut();
            let _: Bool = msg_send![nsapp, sendAction: action, to: nil, from: nil];
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
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
        assert_eq!(route("close-window"), Route::CloseWindow);
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
}

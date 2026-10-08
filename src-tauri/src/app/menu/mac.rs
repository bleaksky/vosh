use std::sync::{Mutex, MutexGuard, PoisonError};

use tauri::menu::{
    AboutMetadata, CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem,
    Submenu, HELP_SUBMENU_ID, WINDOW_SUBMENU_ID,
};
use tauri::{AppHandle, Emitter, Manager, Wry};
use tracing::warn;

use super::{
    accelerator, between_sessions, connect_label, is_check_id, quit_asks, route, shows_disconnect,
    snoop_listed, staff_listed, theme_rows, MenuState, MenuTheme, Route, SessionRow, ThemeRow,
    PANE_ROWS, QUIT_ACCELERATOR, SESSION_ROWS,
};
use crate::app::events::{APP_MENU, HELP_FIND, SETTINGS_FIND, SNOOP_FIND};
use crate::app::state::SharedState;
use crate::app::windows::{open_aux_window, snoop_in_front, HELP_WINDOW, SETTINGS_WINDOW};

const COPYRIGHT: &str = "Copyright © 2026 James Wright";

/// The rows that change after launch. `Menu::get` only searches the
/// top level, so the builder keeps these.
pub(super) struct MenuHandles {
    session: Submenu<Wry>,
    connect: MenuItem<Wry>,
    /// Next session and Previous session.
    steps: Vec<MenuItem<Wry>>,
    disconnect_sep: PredefinedMenuItem<Wry>,
    disconnect: MenuItem<Wry>,
    view: Submenu<Wry>,
    sessions: CheckMenuItem<Wry>,
    panel: CheckMenuItem<Wry>,
    split: CheckMenuItem<Wry>,
    /// Go to snoop, in View while the session has a snoop open.
    snoop: MenuItem<Wry>,
    panes: Vec<(&'static str, CheckMenuItem<Wry>)>,
    themes: Submenu<Wry>,
    applied: Mutex<Applied>,
}

/// What the menu shows now, so a snapshot only touches what changed.
struct Applied {
    /// The Session menu ends on Disconnect, see `shows_disconnect`.
    disconnect: bool,
    connect_label: String,
    staff_listed: bool,
    snoop_listed: bool,
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
    let check =
        |id: &str, text: &str| CheckMenuItem::with_id(app, id, text, true, false, accelerator(id));

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
            // then saves the profile once (app/exit.rs).
            &MenuItem::with_id(app, "quit", "Quit Vosh", true, Some(QUIT_ACCELERATOR))?,
        ],
    )?;

    // Session starts as you launch: not connected, with one session. The
    // page sends the world name, the connection state and the sessions
    // at once.
    let connect = item("connect", &connect_label(None))?;
    let session = Submenu::with_items(app, "Session", true, &[&connect])?;
    let mut steps = Vec::new();
    for row in &SESSION_ROWS {
        match row {
            SessionRow::Item(id, text) => {
                let row = item(id, text)?;
                session.append(&row)?;
                if *id == "session-next" || *id == "session-previous" {
                    row.set_enabled(false)?;
                    steps.push(row);
                }
            }
            SessionRow::Separator => session.append(&sep()?)?,
        }
    }
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

    let sessions = check("sessions-sidebar", "Show sessions")?;
    sessions.set_enabled(false)?;
    let panel = check("panel", "Show panel")?;
    let split = check("split", "Split terminal")?;
    // Go to snoop joins after Split terminal once a snoop opens.
    let snoop = item("snoop", "Go to snoop")?;
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
    view_items.push(&sessions);
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
        &[
            &item("help", "Vosh help")?,
            &item("get-started", "Get started")?,
        ],
    )?;

    let menu = Menu::with_items(app, &[&vosh, &session, &edit, &view, &window, &help])?;
    app.manage(MenuHandles {
        session,
        connect,
        steps,
        disconnect_sep,
        disconnect,
        view,
        sessions,
        panel,
        split,
        snoop,
        panes,
        themes,
        applied: Mutex::new(Applied {
            disconnect: false,
            connect_label: connect_label(None),
            staff_listed: false,
            snoop_listed: false,
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
            // Open on the async runtime, as the page's command does,
            // not on the main thread the click arrives on.
            tauri::async_runtime::spawn(async move {
                if let Err(e) = open_aux_window(&app, &SETTINGS_WINDOW) {
                    warn!(error = %e, "menu: opening settings failed");
                }
            });
        }
        Route::OpenHelp => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = open_aux_window(&app, &HELP_WINDOW) {
                    warn!(error = %e, "menu: opening help failed");
                }
            });
        }
        Route::Quit => {
            let app = app.clone();
            // Off the main thread, since each session's count takes its
            // connection lock. A quit from the Dock or at log out never
            // comes here and cannot ask (app/exit.rs).
            tauri::async_runtime::spawn(async move {
                let main = app.get_webview_window("main").is_some();
                if main && quit_asks(connected_sessions(&app)) {
                    raise_main(&app);
                    emit_main(&app, "quit");
                } else {
                    app.exit(0);
                }
            });
        }
        Route::CloseFront => {
            if is_front(app, "settings") {
                if let Some(settings) = app.get_webview_window("settings") {
                    // A close request, the same as the close button.
                    // The page's close handler (useSettingsClose)
                    // sends the edits it holds back, then closes.
                    let _ = settings.close();
                }
            } else if is_front(app, "help") {
                // Help holds no edits, so it closes at once.
                if let Some(help) = app.get_webview_window("help") {
                    let _ = help.close();
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
                let _ = app.emit_to("settings", SETTINGS_FIND, ());
            } else if is_front(app, "help") {
                let _ = app.emit_to("help", HELP_FIND, ());
            } else if let Some((label, session)) = snoop_in_front(app) {
                let _ = app.emit_to(label.as_str(), SNOOP_FIND, session);
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
pub(crate) fn apply_state(app: &AppHandle, state: &MenuState) {
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
    let disconnect = shows_disconnect(state);
    if applied.disconnect != disconnect {
        if disconnect {
            // Disconnect takes the last row, on its own.
            log_err(h.session.remove(&h.connect), "remove connect");
            log_err(h.session.append(&h.disconnect_sep), "add separator");
            log_err(h.session.append(&h.disconnect), "add disconnect");
        } else {
            log_err(h.session.remove(&h.disconnect), "remove disconnect");
            log_err(h.session.remove(&h.disconnect_sep), "remove separator");
            log_err(h.session.prepend(&h.connect), "add connect");
        }
        applied.disconnect = disconnect;
    }

    let between = between_sessions(state);
    for row in &h.steps {
        log_err(row.set_enabled(between), "step row");
    }
    log_err(h.sessions.set_enabled(between), "sessions row");
    log_err(
        h.sessions.set_checked(state.sessions_shown),
        "sessions check",
    );
    log_err(h.panel.set_checked(state.panel_open), "panel check");
    log_err(h.split.set_checked(state.split_open), "split check");
    for (pane, row) in &h.panes {
        let visible = state.panes.iter().any(|p| p.pane == *pane && p.visible);
        log_err(row.set_checked(visible), "pane check");
    }

    let snoop = snoop_listed(state);
    if applied.snoop_listed != snoop {
        if snoop {
            let at = h
                .view
                .items()
                .ok()
                .and_then(|items| items.iter().position(|i| i.id() == "split"))
                .map_or(0, |split| split + 1);
            log_err(h.view.insert(&h.snoop, at), "add go to snoop");
        } else {
            log_err(h.view.remove(&h.snoop), "remove go to snoop");
        }
        applied.snoop_listed = snoop;
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
        "sessions-sidebar" => flip(&h.sessions),
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

/// How many sessions are connected.
fn connected_sessions(app: &AppHandle) -> usize {
    app.try_state::<SharedState>().map_or(0, |state| {
        state
            .all_sessions()
            .iter()
            .filter(|session| session.connected())
            .count()
    })
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
    if let Err(e) = app.emit_to("main", APP_MENU, id) {
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
pub(crate) fn system_copy() {
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

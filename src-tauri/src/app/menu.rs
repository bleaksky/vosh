//! The macOS menu bar (the approved `MenuBar` board). Rust owns the menu,
//! so it is there before the page loads and survives a page reload, and
//! Settings, Help, Copy, Close session and Close window work whichever
//! window is in front.
//!
//! Vosh commands reach the main window as `vosh://app-menu` with the
//! palette entry id as the payload, and shell/useAppCommands.ts runs them
//! through the same dispatcher as the keyboard shortcuts. The page owns
//! the truth for every check mark, label and dimmed row: it pushes a
//! [`MenuState`] snapshot through the `menu_set_state` command whenever
//! one changes, and the menu only mirrors it.
//!
//! Windows and Linux get no menu bar. Tauri would attach an app menu to
//! every frameless window there, so the builder only installs this one
//! on macOS, and the page's two menu commands are no-ops elsewhere.

use serde::Deserialize;

/// Where the page tells the menu what to show. `camelCase` on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) struct MenuState {
    /// Connecting or connected. Swaps Connect for Disconnect.
    pub(crate) connected: bool,
    /// A series of redials runs, so Disconnect shows between the tries
    /// too, as the palette and the session menu show it.
    #[serde(default)]
    pub(crate) redialing: bool,
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
    /// How many sessions are open.
    pub(crate) sessions: usize,
    /// The sessions sidebar shows in the main window.
    pub(crate) sessions_shown: bool,
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

#[cfg(target_os = "macos")]
pub(crate) use mac::{apply_state, build, on_event, system_copy};

/// The shortcut specs the menu, the palette keycaps, and the page's
/// keydown handler share, so they cannot drift apart.
#[cfg(target_os = "macos")]
const SHORTCUTS_JSON: &str = include_str!("../../../src/lib/appShortcuts.json");

/// A row of the Session menu after Connect to, which comes and goes.
#[cfg(target_os = "macos")]
#[derive(Debug, PartialEq, Eq)]
enum SessionRow {
    /// A command row, by its palette id and its words.
    Item(&'static str, &'static str),
    Separator,
}

/// The Session menu after Connect to, in the order board 4 of the
/// Sessions review draws it. Disconnect follows on its own while a
/// session is connected.
#[cfg(target_os = "macos")]
const SESSION_ROWS: [SessionRow; 10] = [
    SessionRow::Item("session-edit", "Edit connection…"),
    SessionRow::Separator,
    SessionRow::Item("session-new", "New session…"),
    SessionRow::Item("session-next", "Next session"),
    SessionRow::Item("session-previous", "Previous session"),
    SessionRow::Separator,
    SessionRow::Item("session-rename", "Rename session…"),
    SessionRow::Item("session-close", "Close session"),
    SessionRow::Item("close-window", "Close window"),
    SessionRow::Item("profile-save", "Save profile"),
];

/// Whether the rows that move between sessions take a click: Next
/// session, Previous session and Show sessions. One session has nowhere
/// to step and no sidebar, so they show dimmed (Sessions Q12).
#[cfg(target_os = "macos")]
fn between_sessions(state: &MenuState) -> bool {
    state.sessions >= 2
}

/// Whether the Session menu ends on Disconnect in place of Connect: while
/// the session is connecting or connected, and while a redial waits.
#[cfg(target_os = "macos")]
fn shows_disconnect(state: &MenuState) -> bool {
    state.connected || state.redialing
}

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
    /// Open Help, or bring it to the front.
    OpenHelp,
    /// Close what is in front. Settings and Help close. In the main
    /// window the command runs, Close window or Close session, and asks
    /// first while a session it ends is connected. So Close session never
    /// closes a game from Settings (Sessions Q11).
    CloseFront,
    /// Copy in the window in front.
    Copy,
    /// Find in the window in front: settings search in Settings, help
    /// search in Help, the find bar in the main window.
    Find,
    /// Run in the main window, raising it first unless `raise` is off.
    Main { raise: bool },
    /// Quit through the exit request, so the windows send their pending
    /// writes first. With two or more sessions connected the main window
    /// asks first, see [`quit_asks`].
    Quit,
}

/// Whether Quit hands the main window the question before it quits, by
/// how many sessions are connected. Two or more ask, so one keeps the
/// Quit it had before sessions (Sessions Q13).
#[cfg(target_os = "macos")]
const fn quit_asks(connected: usize) -> bool {
    connected >= 2
}

/// The shortcut for Quit Vosh, the one the system Quit row uses.
#[cfg(target_os = "macos")]
const QUIT_ACCELERATOR: &str = "Cmd+Q";

#[cfg(target_os = "macos")]
fn route(id: &str) -> Route {
    match id {
        "quit" => Route::Quit,
        "settings" => Route::OpenSettings,
        "help" => Route::OpenHelp,
        "close-window" | "session-close" => Route::CloseFront,
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
    id == "panel"
        || id == "split"
        || id == "sessions-sidebar"
        || id.starts_with("pane-")
        || id.starts_with("theme-")
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
mod mac;

#[cfg(all(test, target_os = "macos"))]
mod tests;

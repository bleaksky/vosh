//! The Settings, Help and snoop windows, the ground and native
//! appearance a new window opens on, what the main window's close and
//! blur do, and spellcheck in the macOS webview.
//!
//! Settings, Help and each snoop window open hidden and show themselves once their page has
//! painted the theme. A frame the page has not painted yet shows the
//! window's own background, so every theme paint in any window reports
//! the theme's ground to [`set_backdrop`], and [`open_aux_window`]
//! builds the window on it. An open Settings, Help or snoop window takes
//! each new ground as it arrives. The appearance pins the light or dark native
//! appearance while the theme is your pick, and is `None` while the
//! theme follows the system, so the window follows the system too. A
//! theme whose ground is not one solid color reports no ground, and the
//! window keeps its own clear color.

use std::borrow::Cow;
use std::sync::Mutex;

use tauri::{
    window::Color, AppHandle, Manager, Runtime, Theme, WebviewUrl, WebviewWindowBuilder, Window,
};
use tracing::warn;

use crate::app::state::SharedState;
use crate::profile::open::OpenProfile;
use crate::profile::ui::{
    coerce_snoop_window_side, coerce_writing_card_edge, UiConfig, SNOOP_WINDOW_MIN_HEIGHT,
    SNOOP_WINDOW_MIN_WIDTH,
};
use crate::sessions::SessionId;

/// What a new window opens on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Backdrop {
    /// The theme's ground, sRGB, or `None` when it is not one solid
    /// color.
    rgb: Option<(u8, u8, u8)>,
    /// The native appearance to pin, or `None` to follow the system.
    appearance: Option<Theme>,
}

static BACKDROP: Mutex<Option<Backdrop>> = Mutex::new(None);

/// Whether a window's own color carries the theme's ground. On macOS it
/// is the `NSWindow` background, which shows through wherever the page
/// has not painted, as in a live resize, and which an open window takes
/// again on every theme change. Windows paints a transparent window with
/// the color it was created with for as long as the window is open, so
/// a theme change would leave that color behind, and Linux keeps the
/// window clear. The Settings page draws its own rounded frame there
/// over a transparent window, and an opaque window color would fill the
/// corners outside it. On both, the page's startup paint covers the
/// first frame, since the window stays hidden until the page shows it.
const PAINTS_WINDOW: bool = cfg!(target_os = "macos");

fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let hex = s.trim().strip_prefix('#')?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |range: std::ops::Range<usize>| u8::from_str_radix(hex.get(range)?, 16).ok();
    match hex.len() {
        6 => Some((channel(0..2)?, channel(2..4)?, channel(4..6)?)),
        3 => {
            let short = |i: usize| channel(i..i + 1).map(|n| n * 17);
            Some((short(0)?, short(1)?, short(2)?))
        }
        _ => None,
    }
}

impl Backdrop {
    /// Read a `#rrggbb` or `#rgb` ground, or none, and an appearance of
    /// `light`, `dark`, or none. Anything else is `None`.
    fn parse(background: Option<&str>, appearance: Option<&str>) -> Option<Backdrop> {
        let appearance = match appearance {
            None => None,
            Some("light") => Some(Theme::Light),
            Some("dark") => Some(Theme::Dark),
            Some(_) => return None,
        };
        let rgb = match background {
            None => None,
            Some(hex) => Some(parse_hex(hex)?),
        };
        Some(Backdrop { rgb, appearance })
    }

    /// The backdrop the last theme paint reported, or `None` before any.
    fn current() -> Option<Backdrop> {
        BACKDROP.lock().ok().and_then(|slot| *slot)
    }

    /// Keep a reported backdrop for the next window.
    fn record(background: Option<&str>, appearance: Option<&str>) -> Result<Backdrop, String> {
        let backdrop = Self::parse(background, appearance)
            .ok_or_else(|| format!("not a window backdrop: {background:?} {appearance:?}"))?;
        if let Ok(mut slot) = BACKDROP.lock() {
            *slot = Some(backdrop);
        }
        Ok(backdrop)
    }

    /// The window's own color, or `None` to keep it clear. See
    /// [`PAINTS_WINDOW`].
    fn window_color(self) -> Option<Color> {
        self.window_color_on(PAINTS_WINDOW)
    }

    fn window_color_on(self, paints_window: bool) -> Option<Color> {
        if !paints_window {
            return None;
        }
        let (r, g, b) = self.rgb?;
        Some(Color(r, g, b, 255))
    }

    /// Open a window on this backdrop.
    fn dress<R: Runtime, M: Manager<R>>(
        self,
        builder: WebviewWindowBuilder<'_, R, M>,
    ) -> WebviewWindowBuilder<'_, R, M> {
        let builder = builder.theme(self.appearance);
        match self.window_color() {
            Some(color) => builder.background_color(color),
            None => builder,
        }
    }

    /// Give an open window this backdrop's ground. Its appearance is the
    /// page's to set, since the page follows the theme itself.
    fn redress<R: Runtime>(self, window: &Window<R>) {
        if PAINTS_WINDOW {
            let _ = window.set_background_color(self.window_color());
        }
    }
}

/// Whether the window `label` opened on the reported backdrop and takes
/// each new ground while open: Settings, Help and each snoop window.
fn is_dressed(label: &str) -> bool {
    label == SETTINGS_WINDOW.label || label == HELP_WINDOW.label || snoop_session(label).is_some()
}

/// A theme paint in a window reported the ground and appearance a new
/// window should open on. Keep them for the next window, and give the
/// ground to each open Settings, Help or snoop window now, so a theme
/// change while it is open leaves no old color under it.
pub(crate) fn set_backdrop(
    app: &AppHandle,
    background: Option<&str>,
    appearance: Option<&str>,
) -> Result<(), String> {
    let backdrop = Backdrop::record(background, appearance)?;
    for (label, window) in app.webview_windows() {
        if is_dressed(&label) {
            backdrop.redress(&window.as_ref().window());
        }
    }
    Ok(())
}

/// A window beside the main one that loads the same bundle with its own
/// `?view=`, like Settings, Help and the snoop window. Each opens hidden
/// on the theme's ground and shows itself once its page has painted your
/// theme. The window state plugin keeps the size and place of Settings
/// and Help under their labels, as it keeps the main window's. A snoop
/// window keeps its own in the profile of its session, and opens at the
/// `place` read from there.
pub(crate) struct AuxWindow {
    /// The window label, which the capabilities and the menu name.
    label: Cow<'static, str>,
    /// The page the bundle renders, `index.html?view=...`.
    url: Cow<'static, str>,
    title: Cow<'static, str>,
    /// The default size, the one its layout is drawn for.
    size: (f64, f64),
    /// The smallest size whose layout still fits.
    min_size: (f64, f64),
    /// Where it opens, when it keeps its own place. None opens it at
    /// `size` where the system puts it.
    place: Option<WindowPlace>,
}

/// A window's outer left and top edges and its inner width and height,
/// in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WindowPlace {
    pub(crate) left: f64,
    pub(crate) top: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

impl WindowPlace {
    /// A snoop window place, held to the rules of the profile's `[ui]`
    /// keys. None when a side is missing, not a number, or under 480 by
    /// 240.
    fn snoop(
        left: Option<f64>,
        top: Option<f64>,
        width: Option<f64>,
        height: Option<f64>,
    ) -> Option<WindowPlace> {
        Some(WindowPlace {
            left: coerce_writing_card_edge(left)?,
            top: coerce_writing_card_edge(top)?,
            width: coerce_snoop_window_side(width, SNOOP_WINDOW_MIN_WIDTH)?,
            height: coerce_snoop_window_side(height, SNOOP_WINDOW_MIN_HEIGHT)?,
        })
    }

    /// The snoop window place a profile keeps, or None before the window
    /// first moved.
    pub(crate) fn snoop_from(ui: &UiConfig) -> Option<WindowPlace> {
        Self::snoop(
            ui.snoop_window_left,
            ui.snoop_window_top,
            ui.snoop_window_width,
            ui.snoop_window_height,
        )
    }

    /// Keep this place as the profile's snoop window place.
    fn snoop_into(self, ui: &mut UiConfig) {
        ui.snoop_window_left = Some(self.left);
        ui.snoop_window_top = Some(self.top);
        ui.snoop_window_width = Some(self.width);
        ui.snoop_window_height = Some(self.height);
    }
}

/// How much of a window's top band, in logical pixels, has to sit on a
/// monitor for you to reach it and drag it: 100 across and its title bar
/// of 28 down.
const REACHABLE_ACROSS: f64 = 100.0;
const REACHABLE_DOWN: f64 = 28.0;

/// `place` when its top band sits on one of `monitors`, each the logical
/// left, top, width and height of a screen, or None when you could not
/// reach it there, as after the screen it was on went away.
fn place_on_screen(place: WindowPlace, monitors: &[WindowPlace]) -> Option<WindowPlace> {
    let overlap = |a: f64, a_len: f64, b: f64, b_len: f64| (a + a_len).min(b + b_len) - a.max(b);
    monitors
        .iter()
        .any(|m| {
            overlap(place.left, place.width, m.left, m.width) >= REACHABLE_ACROSS
                && overlap(place.top, REACHABLE_DOWN, m.top, m.height) >= REACHABLE_DOWN
        })
        .then_some(place)
}

/// The logical bounds of every monitor on hand.
fn monitor_bounds<R: Runtime>(app: &AppHandle<R>) -> Vec<WindowPlace> {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|monitor| {
            let scale = monitor.scale_factor();
            let position = monitor.position().to_logical::<f64>(scale);
            let size = monitor.size().to_logical::<f64>(scale);
            WindowPlace {
                left: position.x,
                top: position.y,
                width: size.width,
                height: size.height,
            }
        })
        .collect()
}

/// Settings, at the 880×600 its layout is drawn for. Under 820×560 its
/// two column layouts no longer fit.
pub(crate) const SETTINGS_WINDOW: AuxWindow = AuxWindow {
    label: Cow::Borrowed("settings"),
    url: Cow::Borrowed("index.html?view=settings"),
    title: Cow::Borrowed("Settings"),
    size: (880.0, 600.0),
    min_size: (820.0, 560.0),
    place: None,
};

/// Help, at the 1040×700 its layout is drawn for. Under 860 wide the
/// article no longer keeps its measure beside the 280 px sidebar.
pub(crate) const HELP_WINDOW: AuxWindow = AuxWindow {
    label: Cow::Borrowed("help"),
    url: Cow::Borrowed("index.html?view=help"),
    title: Cow::Borrowed("Help"),
    size: (1040.0, 700.0),
    min_size: (860.0, 560.0),
    place: None,
};

/// What each snoop window's label starts with, before its session's
/// number.
const SNOOP_PREFIX: &str = "snoop-";

/// The snoop window of the session `session`, which Open in a window
/// opens with every tab of the session in it, at the 760×480 its layout
/// is drawn for. Its title reads `Snoop, ` and the
/// session's label, `label`, so the Window menu and Mission Control can
/// tell one apart from another. Under 480×240 the band no longer holds
/// a few tabs and Stop over some rows of text. It opens at `place`, the
/// one the session's profile keeps, when that is still on a screen.
pub(crate) fn snoop_window(
    session: SessionId,
    label: Option<&str>,
    place: Option<WindowPlace>,
) -> AuxWindow {
    AuxWindow {
        label: Cow::Owned(snoop_label(session)),
        url: Cow::Owned(format!("index.html?view=snoop&session={session}")),
        title: Cow::Owned(match label {
            Some(label) => format!("Snoop, {label}"),
            None => "Snoop".to_string(),
        }),
        size: (760.0, 480.0),
        min_size: (480.0, 240.0),
        place,
    }
}

/// The label of the snoop window of `session`.
pub(crate) fn snoop_label(session: SessionId) -> String {
    format!("{SNOOP_PREFIX}{session}")
}

/// The session whose snoop window has the label `label`, or None for
/// any other window.
fn snoop_session(label: &str) -> Option<SessionId> {
    let number = label.strip_prefix(SNOOP_PREFIX)?;
    if !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    number.parse().ok().map(SessionId::from_number)
}

/// The snoop window that is the key window, by its label, with its
/// session.
#[cfg(target_os = "macos")]
pub(crate) fn snoop_in_front<R: Runtime>(app: &AppHandle<R>) -> Option<(String, SessionId)> {
    app.webview_windows()
        .into_iter()
        .find_map(|(label, window)| {
            let session = snoop_session(&label)?;
            window.is_focused().ok()?.then_some((label, session))
        })
}

/// The snoop window `label` closed. The tabs of its session go back to
/// the split, which comes back with them. A session that closed first
/// has nothing to take them.
fn snoop_window_closed<R: Runtime>(app: &AppHandle<R>, state: &SharedState, label: &str) {
    let Some(id) = snoop_session(label) else {
        return;
    };
    if let Ok(session) = state.session(Some(id)) {
        crate::session::snoop::set_windowed(app, &session, false);
    }
}

/// Whether a moved or resized window's place is worth keeping. A
/// minimized, maximized or full screen window keeps the place it had
/// before.
fn keeps_place(minimized: bool, maximized: bool, fullscreen: bool) -> bool {
    !(minimized || maximized || fullscreen)
}

/// Keep `place` as the snoop window place of the profile the session
/// `id` plays, and return that profile for a save. None when the
/// session has closed or the place does not hold.
async fn record_snoop_place(
    state: &SharedState,
    id: SessionId,
    place: WindowPlace,
) -> Option<std::sync::Arc<OpenProfile>> {
    let place = WindowPlace::snoop(
        Some(place.left),
        Some(place.top),
        Some(place.width),
        Some(place.height),
    )?;
    let session = state.session(Some(id)).ok()?;
    let mut profile = session.lock_profile().await;
    place.snoop_into(&mut profile.ui);
    Some(profile.open().clone())
}

/// A snoop window moved or changed size. Keep its place in the profile
/// of its session, which the debounced save and the save at quit then
/// write, unless it is minimized, maximized or full screen.
fn snoop_window_placed(window: &Window, state: &SharedState) {
    let Some(id) = snoop_session(window.label()) else {
        return;
    };
    if !keeps_place(
        window.is_minimized().unwrap_or(true),
        window.is_maximized().unwrap_or(true),
        window.is_fullscreen().unwrap_or(true),
    ) {
        return;
    }
    let (Ok(position), Ok(size), Ok(scale)) = (
        window.outer_position(),
        window.inner_size(),
        window.scale_factor(),
    ) else {
        return;
    };
    let position = position.to_logical::<f64>(scale);
    let size = size.to_logical::<f64>(scale);
    let place = WindowPlace {
        left: position.x,
        top: position.y,
        width: size.width,
        height: size.height,
    };
    let app = window.app_handle().clone();
    let state = state.clone();
    tauri::async_runtime::spawn(async move {
        if let Some(open) = record_snoop_place(&state, id, place).await {
            // A move is no consent to save a profile that `#profile
            // reset` left apart from its file, so this schedules without
            // clearing that hold, as a pane drag does.
            crate::disk::save::schedule_profile_persist(&app, &open);
        }
    });
}

/// The logical size a window should take when the window state plugin
/// restored it at `restored`, or None when it already fits. A side under
/// the minimum, saved by an older and smaller window, goes back to the
/// default. The system does not apply the minimum to a size set from
/// code, so this has to.
fn window_fit(window: &AuxWindow, restored: (f64, f64)) -> Option<(f64, f64)> {
    let (width, height) = restored;
    let (min_width, min_height) = window.min_size;
    if width >= min_width && height >= min_height {
        return None;
    }
    Some((
        if width < min_width {
            window.size.0
        } else {
            width
        },
        if height < min_height {
            window.size.1
        } else {
            height
        },
    ))
}

/// Whether opening a window again brings the open one forward now. A
/// window neither on screen nor minimized is still loading. Its page
/// shows it once it has painted your theme, and showing it sooner would
/// put a frame without your theme on screen.
fn shows_on_reopen(visible: bool, minimized: bool) -> bool {
    visible || minimized
}

/// How long a window may stay hidden after an open before the backend
/// shows it anyway. The page shows it well before this, within its own
/// 500 ms fallback once it runs. This covers a page that never gets
/// that far, so the window always opens.
const SHOW_BACKSTOP: std::time::Duration = std::time::Duration::from_secs(2);

/// Show `window` after [`SHOW_BACKSTOP`] if its page has not shown it by
/// then.
fn show_backstop(window: tauri::WebviewWindow) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SHOW_BACKSTOP).await;
        if !window.is_visible().unwrap_or(true) && !window.is_minimized().unwrap_or(false) {
            warn!(
                window = window.label(),
                "the page never showed its window, showing it now"
            );
            let _ = window.show();
            let _ = window.set_focus();
        }
    });
}

/// Open `spec`, or bring it forward when it is already open. The window
/// is a separate webview on the same frontend bundle, and its `?view=`
/// tells the React entry which page to render. Every window shares the
/// one Rust backend state.
pub(crate) fn open_aux_window(app: &AppHandle, spec: &AuxWindow) -> Result<(), String> {
    if let Some(existing) = app.get_webview_window(&spec.label) {
        let visible = existing.is_visible().unwrap_or(true);
        let minimized = existing.is_minimized().unwrap_or(false);
        if shows_on_reopen(visible, minimized) {
            existing.show().map_err(|e| e.to_string())?;
            existing.set_focus().map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    let url = WebviewUrl::App(spec.url.as_ref().into());
    let place = spec
        .place
        .and_then(|place| place_on_screen(place, &monitor_bounds(app)));
    let (width, height) = place.map_or(spec.size, |p| (p.width, p.height));
    let builder = WebviewWindowBuilder::new(app, spec.label.as_ref(), url)
        .title(spec.title.as_ref())
        .inner_size(width, height)
        .min_inner_size(spec.min_size.0, spec.min_size.1)
        .resizable(true)
        .transparent(true)
        // Stay hidden until the page has painted your theme and shows
        // the window itself, so the first frame is never the dark
        // stylesheet defaults.
        .visible(false)
        // Disable Tauri's OS file-drop handler. When enabled it
        // intercepts HTML5 drag-and-drop inside the webview, which
        // can break overlay drag interactions.
        .disable_drag_drop_handler();
    let builder = match place {
        Some(place) => builder.position(place.left, place.top),
        None => builder,
    };
    // Open on the theme's appearance, which the last theme paint
    // reported, and on macOS on its ground as well, so even a frame the
    // page has not painted yet is in your theme. Windows and Linux keep
    // the window clear (PAINTS_WINDOW explains why). Before any paint
    // the window keeps the defaults.
    let builder = match Backdrop::current() {
        Some(backdrop) => backdrop.dress(builder),
        None => builder,
    };
    // macOS gives the window the main window's titled frame: native
    // traffic lights over the sidebar at the same centers, a hidden
    // title, and the system's corners and rim. Windows and Linux stay
    // frameless, and the page draws its own window controls.
    #[cfg(target_os = "macos")]
    let builder = builder
        .decorations(true)
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true);
    #[cfg(not(target_os = "macos"))]
    let builder = builder.decorations(false);
    let window = builder.build().map_err(|e| e.to_string())?;
    show_backstop(window.clone());
    if let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) {
        let current = size.to_logical::<f64>(scale);
        if let Some((width, height)) = window_fit(spec, (current.width, current.height)) {
            let _ = window.set_size(tauri::LogicalSize::new(width, height));
        }
    }
    Ok(())
}

/// Closing the main window should take every auxiliary window
/// (settings, etc.) down with it. Tauri only exits the process
/// when the LAST window closes, so without this the settings
/// popup hangs around alone after the user closes the main
/// client. They go once the main window is gone, not at its close
/// request, since the page holds that request while it asks whether
/// to end a connected session, and Cancel keeps them.
pub(crate) fn on_window_event(window: &Window, event: &tauri::WindowEvent) {
    // The focus rule of the alerts counts Vosh in front while any of its
    // windows has focus, Settings and Help included.
    if let Some(state) = window.app_handle().try_state::<SharedState>() {
        match event {
            tauri::WindowEvent::Focused(focused) => {
                state.focus.set(window.label(), *focused);
                // Once you come to the main window, a later start of Vosh
                // selects no session for a banner from before.
                if *focused && window.label() == "main" {
                    state.banners.forget_newest();
                }
            }
            tauri::WindowEvent::Destroyed => {
                state.focus.set(window.label(), false);
                // Settings closed, so no page holds unsaved edits.
                if window.label() == "settings" {
                    let state = state.inner().clone();
                    tauri::async_runtime::spawn(async move {
                        crate::ipc::profiles::hold_edits(&state, None).await;
                    });
                }
                snoop_window_closed(window.app_handle(), state.inner(), window.label());
            }
            tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
                snoop_window_placed(window, state.inner());
            }
            _ => {}
        }
    }
    if window.label() != "main" {
        return;
    }
    match event {
        tauri::WindowEvent::Destroyed => {
            let app = window.app_handle();
            for (label, w) in app.webview_windows() {
                if label != "main" {
                    let _ = w.close();
                }
            }
        }
        // A drag on the native grid whose release may never come
        // ends as the main window loses focus.
        #[cfg(native_surface)]
        tauri::WindowEvent::Focused(false) => crate::native::surface::pointer::window_blurred(),
        _ => {}
    }
}

/// Vosh started again while it ran, as a click on a toast of an
/// installed Windows Vosh does, since the toast has no activator. The
/// Vosh that runs takes the start in place of a second one, selects the
/// session of the newest banner since you last came to it, once, and
/// comes to the front. With no such banner it only comes to the front.
#[cfg(windows)]
pub(crate) fn second_start<R: Runtime>(app: &AppHandle<R>) {
    let newest = app
        .try_state::<SharedState>()
        .and_then(|state| state.banners.take_newest());
    match newest {
        Some(session) => crate::alert::banner::show_session(app, session),
        None => {
            if let Some(main) = app.get_webview_window("main") {
                let _ = main.unminimize();
                let _ = main.show();
                let _ = main.set_focus();
            }
        }
    }
}

// macOS-only: WKWebView ignores the HTML `spellcheck` attribute
// until continuous spell-checking is enabled at the NSView level.
// The context-menu "Check Spelling While Typing" item works, which
// means the action `toggleContinuousSpellChecking:` is dispatchable
// through the responder chain. We mirror that path: query
// isContinuousSpellCheckingEnabled first, or the state WebKit saved in
// the app's defaults when the view has no getter, then send the toggle
// action only if it is off, so we never flip it back off. All
// sends are gated with respondsToSelector: — earlier unguarded
// sends of NSTextView-only selectors crashed the app at launch.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
pub(crate) fn enable_macos_spellcheck(window: &tauri::WebviewWindow) -> Result<(), tauri::Error> {
    use objc2::runtime::{AnyObject, Bool, Sel};
    window.with_webview(|webview| {
        let raw = webview.inner().cast::<AnyObject>();
        if raw.is_null() {
            tracing::warn!("macos spellcheck: webview.inner() was null");
            return;
        }
        unsafe {
            let setter: Sel = objc2::sel!(setContinuousSpellCheckingEnabled:);
            let getter: Sel = objc2::sel!(isContinuousSpellCheckingEnabled);
            let toggler: Sel = objc2::sel!(toggleContinuousSpellChecking:);
            let r_set: Bool = objc2::msg_send![raw, respondsToSelector: setter];
            let r_get: Bool = objc2::msg_send![raw, respondsToSelector: getter];
            let r_tog: Bool = objc2::msg_send![raw, respondsToSelector: toggler];
            tracing::info!(
                set = r_set.as_bool(),
                get = r_get.as_bool(),
                toggle = r_tog.as_bool(),
                "macos spellcheck: selectors reachable on WKWebView"
            );
            if r_set.as_bool() {
                let _: () = objc2::msg_send![raw, setContinuousSpellCheckingEnabled: true];
                tracing::info!("macos spellcheck: setContinuousSpellCheckingEnabled:YES sent");
                return;
            }
            if r_tog.as_bool() {
                // WKWebView has no getter, and the toggle saves its state
                // in the app's defaults, so the saved state says whether
                // it is on. Toggling without reading it turned spell
                // check off at every other launch.
                let enabled: Bool = if r_get.as_bool() {
                    objc2::msg_send![raw, isContinuousSpellCheckingEnabled]
                } else {
                    Bool::new(saved_continuous_spellcheck())
                };
                if enabled.as_bool() {
                    tracing::info!("macos spellcheck: already enabled, no toggle needed");
                } else {
                    let _: () = objc2::msg_send![raw, toggleContinuousSpellChecking: raw];
                    tracing::info!("macos spellcheck: toggleContinuousSpellChecking: sent");
                }
            } else {
                tracing::warn!("macos spellcheck: no reachable setter or toggle on WKWebView");
            }
        }
    })
}

/// The continuous spell check state the web view saved in the app's defaults,
/// false when it never saved one.
#[cfg(target_os = "macos")]
fn saved_continuous_spellcheck() -> bool {
    use objc2_foundation::{NSString, NSUserDefaults};
    let key = NSString::from_str("WebContinuousSpellCheckingEnabled");
    NSUserDefaults::standardUserDefaults().boolForKey(&key)
}

#[cfg(test)]
mod tests {
    use super::{
        is_dressed, keeps_place, place_on_screen, record_snoop_place, shows_on_reopen, snoop_label,
        snoop_session, snoop_window, snoop_window_closed, window_fit, Backdrop, WindowPlace,
        HELP_WINDOW, PAINTS_WINDOW, SETTINGS_WINDOW,
    };
    use crate::sessions::SessionId;
    use tauri::{window::Color, Theme};

    #[test]
    fn reads_the_ground_and_the_appearance() {
        assert_eq!(
            Backdrop::parse(Some("#f4efe4"), Some("light")),
            Some(Backdrop {
                rgb: Some((0xf4, 0xef, 0xe4)),
                appearance: Some(Theme::Light),
            })
        );
        assert_eq!(
            Backdrop::parse(Some(" #1A1B26 "), Some("dark")),
            Some(Backdrop {
                rgb: Some((0x1a, 0x1b, 0x26)),
                appearance: Some(Theme::Dark),
            })
        );
        assert_eq!(
            Backdrop::parse(Some("#fff"), None),
            Some(Backdrop {
                rgb: Some((255, 255, 255)),
                appearance: None,
            })
        );
    }

    #[test]
    fn takes_the_appearance_of_a_theme_without_a_solid_ground() {
        // A custom theme's Background can be translucent, or a color
        // only the page can read. The appearance still counts.
        assert_eq!(
            Backdrop::parse(None, Some("dark")),
            Some(Backdrop {
                rgb: None,
                appearance: Some(Theme::Dark),
            })
        );
        assert_eq!(
            Backdrop::parse(None, None),
            Some(Backdrop {
                rgb: None,
                appearance: None,
            })
        );
    }

    #[test]
    fn turns_away_anything_else() {
        for bad in [
            "",
            "#",
            "f4efe4",
            "#f4efe",
            "#f4efe4ff",
            "#ggeeff",
            "#+1+2+3",
            "rgb(0,0,0)",
            "#ffé",
        ] {
            assert_eq!(Backdrop::parse(Some(bad), None), None, "{bad}");
        }
        assert_eq!(Backdrop::parse(Some("#f4efe4"), Some("dim")), None);
        assert_eq!(Backdrop::parse(Some("#f4efe4"), Some("")), None);
        assert_eq!(Backdrop::parse(None, Some("dim")), None);
    }

    #[test]
    fn paints_the_window_only_where_a_theme_change_repaints_it() {
        let backdrop = Backdrop {
            rgb: Some((0xf4, 0xef, 0xe4)),
            appearance: Some(Theme::Light),
        };
        // macOS: the NSWindow color, which an open window takes again
        // on every theme change.
        assert_eq!(
            backdrop.window_color_on(true),
            Some(Color(0xf4, 0xef, 0xe4, 255))
        );
        // Windows paints a transparent window with its creation color
        // for as long as it is open, so a theme change would leave it
        // behind. Linux keeps the window clear for the rounded Settings
        // frame. Both keep the window clear.
        assert_eq!(backdrop.window_color_on(false), None);
        assert_eq!(PAINTS_WINDOW, cfg!(target_os = "macos"));
        assert_eq!(
            backdrop.window_color(),
            backdrop.window_color_on(PAINTS_WINDOW)
        );
        // No solid ground: the window keeps its own clear color.
        let clear = Backdrop {
            rgb: None,
            appearance: Some(Theme::Dark),
        };
        assert_eq!(clear.window_color_on(true), None);
    }

    #[test]
    fn keeps_the_last_good_report() {
        Backdrop::record(Some("#102030"), Some("dark")).unwrap();
        assert!(Backdrop::record(Some("nope"), None).is_err());
        assert_eq!(
            Backdrop::current(),
            Some(Backdrop {
                rgb: Some((0x10, 0x20, 0x30)),
                appearance: Some(Theme::Dark),
            })
        );
        Backdrop::record(Some("#fdfcf8"), None).unwrap();
        assert_eq!(
            Backdrop::current(),
            Some(Backdrop {
                rgb: Some((0xfd, 0xfc, 0xf8)),
                appearance: None,
            })
        );
        // A theme without a solid ground drops the old ground and pins
        // its own appearance, so a new window opens on neither the old
        // color nor the old appearance.
        Backdrop::record(None, Some("light")).unwrap();
        assert_eq!(
            Backdrop::current(),
            Some(Backdrop {
                rgb: None,
                appearance: Some(Theme::Light),
            })
        );
    }

    #[test]
    fn settings_window_keeps_a_size_that_fits() {
        let fit = |size| window_fit(&SETTINGS_WINDOW, size);
        assert_eq!(fit((880.0, 600.0)), None);
        assert_eq!(fit((820.0, 560.0)), None);
        assert_eq!(fit((1200.0, 900.0)), None);
    }

    #[test]
    fn reopening_a_window_leaves_a_loading_one_to_its_page() {
        // On screen, or minimized: bring it forward now.
        assert!(shows_on_reopen(true, false));
        assert!(shows_on_reopen(false, true));
        assert!(shows_on_reopen(true, true));
        // Neither: the page has not painted your theme yet, and shows
        // the window itself once it has.
        assert!(!shows_on_reopen(false, false));
    }

    #[test]
    fn settings_window_grows_a_side_left_under_the_minimum() {
        let fit = |size| window_fit(&SETTINGS_WINDOW, size);
        // The old Settings window opened at 780×640.
        assert_eq!(fit((780.0, 640.0)), Some((880.0, 640.0)));
        assert_eq!(fit((900.0, 420.0)), Some((900.0, 600.0)));
        assert_eq!(fit((520.0, 420.0)), Some((880.0, 600.0)));
    }

    #[test]
    fn help_opens_at_the_board_size_on_its_own_page() {
        assert_eq!(HELP_WINDOW.label, "help");
        assert_eq!(HELP_WINDOW.url, "index.html?view=help");
        assert_eq!(HELP_WINDOW.size, (1040.0, 700.0));
        let fit = |size| window_fit(&HELP_WINDOW, size);
        assert_eq!(fit((1040.0, 700.0)), None);
        assert_eq!(fit((860.0, 560.0)), None);
        // A side under the minimum goes back to the default size.
        assert_eq!(fit((700.0, 800.0)), Some((1040.0, 800.0)));
        assert_eq!(fit((900.0, 400.0)), Some((900.0, 700.0)));
    }

    #[test]
    fn a_snoop_window_opens_per_session_at_the_board_size() {
        let id = SessionId::numbered(3);
        let window = snoop_window(id, Some("Staff"), None);
        assert_eq!(window.label, "snoop-3");
        assert_eq!(window.url, "index.html?view=snoop&session=3");
        assert_eq!(window.title, "Snoop, Staff");
        assert_eq!(window.size, (760.0, 480.0));
        assert_eq!(window_fit(&window, (760.0, 480.0)), None);
        assert_eq!(window_fit(&window, (300.0, 600.0)), Some((760.0, 600.0)));
        assert_eq!(window.place, None);
        // A session with nothing to go by yet.
        assert_eq!(snoop_window(id, None, None).title, "Snoop");
        assert_eq!(snoop_label(id), "snoop-3");
    }

    const PLACE: WindowPlace = WindowPlace {
        left: 200.0,
        top: 120.0,
        width: 900.0,
        height: 520.0,
    };

    #[test]
    fn a_snoop_window_carries_the_place_its_profile_keeps() {
        let window = snoop_window(SessionId::numbered(2), None, Some(PLACE));
        assert_eq!(window.place, Some(PLACE));
        // The default size stays for a place that no longer fits a screen.
        assert_eq!(window.size, (760.0, 480.0));
        let mut ui = crate::profile::ui::UiConfig::default();
        assert_eq!(WindowPlace::snoop_from(&ui), None);
        PLACE.snoop_into(&mut ui);
        assert_eq!(WindowPlace::snoop_from(&ui), Some(PLACE));
        // A place with a side missing is no place.
        ui.snoop_window_top = None;
        assert_eq!(WindowPlace::snoop_from(&ui), None);
    }

    #[test]
    fn a_place_holds_only_on_a_screen_you_can_reach_it_on() {
        let laptop = WindowPlace {
            left: 0.0,
            top: 0.0,
            width: 1512.0,
            height: 982.0,
        };
        let left_screen = WindowPlace {
            left: -1920.0,
            top: -200.0,
            width: 1920.0,
            height: 1080.0,
        };
        assert_eq!(place_on_screen(PLACE, &[laptop]), Some(PLACE));
        let there = WindowPlace {
            left: -1400.0,
            top: 40.0,
            ..PLACE
        };
        assert_eq!(place_on_screen(there, &[laptop, left_screen]), Some(there));
        // The screen it was on went away.
        assert_eq!(place_on_screen(there, &[laptop]), None);
        assert_eq!(place_on_screen(PLACE, &[]), None);
        // Only a sliver of the title bar shows, or none of it.
        let sliver = WindowPlace {
            left: 1450.0,
            ..PLACE
        };
        assert_eq!(place_on_screen(sliver, &[laptop]), None);
        let below = WindowPlace {
            top: 970.0,
            ..PLACE
        };
        assert_eq!(place_on_screen(below, &[laptop]), None);
    }

    #[test]
    fn a_minimized_maximized_or_full_screen_window_keeps_its_old_place() {
        assert!(keeps_place(false, false, false));
        assert!(!keeps_place(true, false, false));
        assert!(!keeps_place(false, true, false));
        assert!(!keeps_place(false, false, true));
    }

    #[test]
    fn each_snoop_window_keeps_its_place_in_its_own_profile() {
        use std::sync::Arc;

        use crate::app::state::{AppState, SharedState};
        use crate::profile::live::Profile;

        let state: SharedState = Arc::new(AppState::default());
        let orla = state.add_open_profile("Orla", Profile::default());
        let maren = state.add_open_profile("Maren", Profile::default());
        let first = state.open_session(orla.clone());
        let second = state.open_session(maren.clone());
        let third = state.open_session(orla.clone());
        let kept = |open: &Arc<crate::profile::open::OpenProfile>| {
            let open = open.clone();
            tauri::async_runtime::block_on(
                async move { WindowPlace::snoop_from(&open.lock().await.ui) },
            )
        };
        let record =
            |id, place| tauri::async_runtime::block_on(record_snoop_place(&state, id, place));

        let saved = record(third.id, PLACE).unwrap();
        assert!(Arc::ptr_eq(&saved, &orla));
        assert_eq!(kept(&orla), Some(PLACE));
        assert_eq!(kept(&maren), None);

        let other = WindowPlace {
            left: 40.0,
            ..PLACE
        };
        assert!(Arc::ptr_eq(&record(second.id, other).unwrap(), &maren));
        assert_eq!(kept(&maren), Some(other));
        assert_eq!(kept(&orla), Some(PLACE));

        // A place under the minimum, or of a session gone, is not kept.
        let small = WindowPlace {
            width: 300.0,
            ..PLACE
        };
        assert!(record(first.id, small).is_none());
        assert!(record(SessionId::numbered(99), PLACE).is_none());
        assert_eq!(kept(&orla), Some(PLACE));
    }

    #[test]
    fn only_a_snoop_label_names_a_session() {
        assert_eq!(snoop_session("snoop-12"), Some(SessionId::numbered(12)));
        for label in [
            "main", "settings", "help", "snoop-", "snoop-x", "snoop-+1", "snoop",
        ] {
            assert_eq!(snoop_session(label), None, "{label}");
        }
        assert!(is_dressed("settings"));
        assert!(is_dressed("help"));
        assert!(is_dressed("snoop-1"));
        assert!(!is_dressed("main"));
    }

    #[test]
    fn the_split_gives_its_tabs_to_the_window_and_takes_them_back() {
        use std::sync::{Arc, Mutex};

        use serde_json::json;
        use tauri::test::{mock_builder, mock_context, noop_assets};
        use tauri::{Listener, Manager};

        use crate::app::events::SNOOP;
        use crate::app::state::{AppState, SharedState};
        use crate::profile::live::Profile;

        let app = mock_builder().build(mock_context(noop_assets())).unwrap();
        app.manage::<SharedState>(Arc::new(AppState::default()));
        let state: SharedState = app.state::<SharedState>().inner().clone();
        let orla = state.add_open_profile("Orla", Profile::default());
        let session = state.open_session(orla);
        let lists = Arc::new(Mutex::new(Vec::new()));
        let heard = lists.clone();
        app.listen_any(SNOOP, move |event| {
            let list: serde_json::Value = serde_json::from_str(event.payload()).unwrap();
            heard.lock().unwrap().push(list);
        });
        let windowed = || session.connection.lock().snoops.snapshot().windowed;
        let handle = app.handle();
        let label = snoop_label(session.id);

        // Open in a window marks the tabs as windowed and sends the list.
        crate::session::snoop::set_windowed(handle, &session, true);
        assert!(windowed());
        // Another window closing leaves them there.
        snoop_window_closed(handle, &state, "snoop-99");
        snoop_window_closed(handle, &state, "help");
        assert!(windowed());
        // The window going brings them back to the split.
        snoop_window_closed(handle, &state, &label);
        assert!(!windowed());
        let want = |on| json!({ "session": session.id, "tabs": [], "windowed": on });
        assert_eq!(*lists.lock().unwrap(), [want(true), want(false)]);
    }
}

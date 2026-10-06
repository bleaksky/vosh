//! The Settings and Help windows, the ground and native appearance a
//! new window opens on, what the main window's close and blur do, and
//! spellcheck in the macOS webview.
//!
//! Settings and Help open hidden and show themselves once their page has
//! painted the theme. A frame the page has not painted yet shows the
//! window's own background, so every theme paint in any window reports
//! the theme's ground to [`set_backdrop`], and [`open_aux_window`]
//! builds the window on it. An open Settings or Help window takes each
//! new ground as it arrives. The appearance pins the light or dark native
//! appearance while the theme is your pick, and is `None` while the
//! theme follows the system, so the window follows the system too. A
//! theme whose ground is not one solid color reports no ground, and the
//! window keeps its own clear color.

use std::sync::Mutex;

use tauri::{
    window::Color, AppHandle, Manager, Runtime, Theme, WebviewUrl, WebviewWindowBuilder, Window,
};
use tracing::warn;

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

/// The windows that open on the reported backdrop and take each new
/// ground while open.
const DRESSED_WINDOWS: [&str; 2] = ["settings", "help"];

/// A theme paint in a window reported the ground and appearance a new
/// window should open on. Keep them for the next window, and give the
/// ground to an open Settings or Help window now, so a theme change
/// while it is open leaves no old color under it.
pub(crate) fn set_backdrop(
    app: &AppHandle,
    background: Option<&str>,
    appearance: Option<&str>,
) -> Result<(), String> {
    let backdrop = Backdrop::record(background, appearance)?;
    for label in DRESSED_WINDOWS {
        if let Some(window) = app.get_webview_window(label) {
            backdrop.redress(&window.as_ref().window());
        }
    }
    Ok(())
}

/// A window beside the main one that loads the same bundle with its own
/// `?view=`, like Settings and Help. Each opens hidden on the theme's
/// ground and shows itself once its page has painted your theme.
pub(crate) struct AuxWindow {
    /// The window label, which the capabilities and the menu name.
    label: &'static str,
    /// The page the bundle renders, `index.html?view=...`.
    url: &'static str,
    title: &'static str,
    /// The default size, the approved boards' window.
    size: (f64, f64),
    /// The smallest size whose layout still fits.
    min_size: (f64, f64),
}

/// Settings, at the approved boards' 880×600. Under 820×560 its two
/// column layouts no longer fit.
pub(crate) const SETTINGS_WINDOW: AuxWindow = AuxWindow {
    label: "settings",
    url: "index.html?view=settings",
    title: "Settings",
    size: (880.0, 600.0),
    min_size: (820.0, 560.0),
};

/// Help, at the approved Help boards' 1040×700. Under 860 wide the
/// article no longer keeps its measure beside the 280 px sidebar.
pub(crate) const HELP_WINDOW: AuxWindow = AuxWindow {
    label: "help",
    url: "index.html?view=help",
    title: "Help",
    size: (1040.0, 700.0),
    min_size: (860.0, 560.0),
};

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
    if let Some(existing) = app.get_webview_window(spec.label) {
        let visible = existing.is_visible().unwrap_or(true);
        let minimized = existing.is_minimized().unwrap_or(false);
        if shows_on_reopen(visible, minimized) {
            existing.show().map_err(|e| e.to_string())?;
            existing.set_focus().map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    let builder = WebviewWindowBuilder::new(app, spec.label, WebviewUrl::App(spec.url.into()))
        .title(spec.title)
        .inner_size(spec.size.0, spec.size.1)
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
/// to end a connected session (Sessions Q13), and Cancel keeps them.
pub(crate) fn on_window_event(window: &Window, event: &tauri::WindowEvent) {
    // The focus rule of the alerts counts Vosh in front while any of its
    // windows has focus, Settings and Help included.
    if let Some(state) = window
        .app_handle()
        .try_state::<crate::app::state::SharedState>()
    {
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
        .try_state::<crate::app::state::SharedState>()
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
// isContinuousSpellCheckingEnabled first, then send the toggle
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
                let enabled: Bool = if r_get.as_bool() {
                    objc2::msg_send![raw, isContinuousSpellCheckingEnabled]
                } else {
                    Bool::NO
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

#[cfg(test)]
mod tests {
    use super::{
        shows_on_reopen, window_fit, Backdrop, HELP_WINDOW, PAINTS_WINDOW, SETTINGS_WINDOW,
    };
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
        // A side under the minimum goes back to the board size.
        assert_eq!(fit((700.0, 800.0)), Some((1040.0, 800.0)));
        assert_eq!(fit((900.0, 400.0)), Some((900.0, 700.0)));
    }
}

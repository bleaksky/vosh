//! The ground and native appearance a new window opens on.
//!
//! Settings and Help open hidden and show themselves once their page has
//! painted the theme. A frame the page has not painted yet shows the
//! window's own background, so every theme paint in any window reports
//! the theme's ground here (`window_backdrop_set`), and the window opener
//! in commands.rs builds the window on it. An open Settings or Help
//! window takes each new ground as it arrives. The appearance pins the light or dark native
//! appearance while the theme is your pick, and is `None` while the
//! theme follows the system, so the window follows the system too. A
//! theme whose ground is not one solid color reports no ground, and the
//! window keeps its own clear color.

use std::sync::Mutex;

use tauri::{window::Color, AppHandle, Manager, Runtime, Theme, WebviewWindowBuilder, Window};

/// What a new window opens on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Backdrop {
    /// The theme's ground, sRGB, or `None` when it is not one solid
    /// color.
    pub(crate) rgb: Option<(u8, u8, u8)>,
    /// The native appearance to pin, or `None` to follow the system.
    pub(crate) appearance: Option<Theme>,
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

/// Read a `#rrggbb` or `#rgb` ground, or none, and an appearance of
/// `light`, `dark`, or none. Anything else is `None`.
pub(crate) fn parse(background: Option<&str>, appearance: Option<&str>) -> Option<Backdrop> {
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

/// The backdrop the last theme paint reported, or `None` before any.
pub(crate) fn current() -> Option<Backdrop> {
    BACKDROP.lock().ok().and_then(|slot| *slot)
}

fn set(backdrop: Backdrop) {
    if let Ok(mut slot) = BACKDROP.lock() {
        *slot = Some(backdrop);
    }
}

impl Backdrop {
    /// The window's own color, or `None` to keep it clear. See
    /// [`PAINTS_WINDOW`].
    pub(crate) fn window_color(self) -> Option<Color> {
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
    pub(crate) fn dress<R: Runtime, M: Manager<R>>(
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

/// Keep a reported backdrop for the next window.
fn record(background: Option<&str>, appearance: Option<&str>) -> Result<Backdrop, String> {
    let backdrop = parse(background, appearance)
        .ok_or_else(|| format!("not a window backdrop: {background:?} {appearance:?}"))?;
    set(backdrop);
    Ok(backdrop)
}

/// The windows that open on the reported backdrop and take each new
/// ground while open.
const DRESSED_WINDOWS: [&str; 2] = ["settings", "help"];

/// A theme paint in a window reports the ground and appearance a new
/// window should open on. An open Settings or Help window takes the
/// ground now, so a theme change while it is open leaves no old color
/// under it.
#[tauri::command]
pub(crate) fn window_backdrop_set(
    app: AppHandle,
    background: Option<String>,
    appearance: Option<String>,
) -> Result<(), String> {
    let backdrop = record(background.as_deref(), appearance.as_deref())?;
    for label in DRESSED_WINDOWS {
        if let Some(window) = app.get_webview_window(label) {
            backdrop.redress(&window.as_ref().window());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{current, parse, record, Backdrop, PAINTS_WINDOW};
    use tauri::{window::Color, Theme};

    #[test]
    fn reads_the_ground_and_the_appearance() {
        assert_eq!(
            parse(Some("#f4efe4"), Some("light")),
            Some(Backdrop {
                rgb: Some((0xf4, 0xef, 0xe4)),
                appearance: Some(Theme::Light),
            })
        );
        assert_eq!(
            parse(Some(" #1A1B26 "), Some("dark")),
            Some(Backdrop {
                rgb: Some((0x1a, 0x1b, 0x26)),
                appearance: Some(Theme::Dark),
            })
        );
        assert_eq!(
            parse(Some("#fff"), None),
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
            parse(None, Some("dark")),
            Some(Backdrop {
                rgb: None,
                appearance: Some(Theme::Dark),
            })
        );
        assert_eq!(
            parse(None, None),
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
            assert_eq!(parse(Some(bad), None), None, "{bad}");
        }
        assert_eq!(parse(Some("#f4efe4"), Some("dim")), None);
        assert_eq!(parse(Some("#f4efe4"), Some("")), None);
        assert_eq!(parse(None, Some("dim")), None);
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
        record(Some("#102030"), Some("dark")).unwrap();
        assert!(record(Some("nope"), None).is_err());
        assert_eq!(
            current(),
            Some(Backdrop {
                rgb: Some((0x10, 0x20, 0x30)),
                appearance: Some(Theme::Dark),
            })
        );
        record(Some("#fdfcf8"), None).unwrap();
        assert_eq!(
            current(),
            Some(Backdrop {
                rgb: Some((0xfd, 0xfc, 0xf8)),
                appearance: None,
            })
        );
        // A theme without a solid ground drops the old ground and pins
        // its own appearance, so a new window opens on neither the old
        // color nor the old appearance.
        record(None, Some("light")).unwrap();
        assert_eq!(
            current(),
            Some(Backdrop {
                rgb: None,
                appearance: Some(Theme::Light),
            })
        );
    }
}

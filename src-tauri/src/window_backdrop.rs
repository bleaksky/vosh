//! The ground and native appearance a new window opens on.
//!
//! Settings opens hidden and shows itself once its page has painted the
//! theme. A frame the page has not painted yet shows the window's own
//! background, so every theme paint in any window reports the theme's
//! ground here (`window_backdrop_set`), and `open_settings_window`
//! builds the window on it. The appearance pins the light or dark native
//! appearance while the theme is your pick, and is `None` while the
//! theme follows the system, so the window follows the system too.

use std::sync::Mutex;

use tauri::{window::Color, Manager, Runtime, Theme, WebviewWindowBuilder};

/// What a new window opens on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Backdrop {
    /// The theme's ground, sRGB.
    pub(crate) rgb: (u8, u8, u8),
    /// The native appearance to pin, or `None` to follow the system.
    pub(crate) appearance: Option<Theme>,
}

static BACKDROP: Mutex<Option<Backdrop>> = Mutex::new(None);

/// Read a `#rrggbb` or `#rgb` ground and an appearance of `light`,
/// `dark`, or none. Anything else is `None`.
pub(crate) fn parse(background: &str, appearance: Option<&str>) -> Option<Backdrop> {
    let appearance = match appearance {
        None => None,
        Some("light") => Some(Theme::Light),
        Some("dark") => Some(Theme::Dark),
        Some(_) => return None,
    };
    Some(Backdrop {
        rgb: parse_hex(background)?,
        appearance,
    })
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
    /// The window's own color. Linux keeps it clear. The Settings page
    /// draws its own rounded frame there over a transparent window, and
    /// an opaque window color would fill the corners outside it.
    pub(crate) fn window_color(self) -> Color {
        let (r, g, b) = self.rgb;
        let alpha = if cfg!(target_os = "linux") { 0 } else { 255 };
        Color(r, g, b, alpha)
    }

    /// Open a window on this backdrop.
    pub(crate) fn dress<R: Runtime, M: Manager<R>>(
        self,
        builder: WebviewWindowBuilder<'_, R, M>,
    ) -> WebviewWindowBuilder<'_, R, M> {
        builder
            .theme(self.appearance)
            .background_color(self.window_color())
    }
}

/// A theme paint in a window reports the ground and appearance a new
/// window should open on.
#[tauri::command]
pub(crate) fn window_backdrop_set(
    background: String,
    appearance: Option<String>,
) -> Result<(), String> {
    let backdrop = parse(&background, appearance.as_deref())
        .ok_or_else(|| format!("not a window backdrop: {background} {appearance:?}"))?;
    set(backdrop);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{current, parse, window_backdrop_set, Backdrop};
    use tauri::{window::Color, Theme};

    #[test]
    fn reads_the_ground_and_the_appearance() {
        assert_eq!(
            parse("#f4efe4", Some("light")),
            Some(Backdrop {
                rgb: (0xf4, 0xef, 0xe4),
                appearance: Some(Theme::Light),
            })
        );
        assert_eq!(
            parse(" #1A1B26 ", Some("dark")),
            Some(Backdrop {
                rgb: (0x1a, 0x1b, 0x26),
                appearance: Some(Theme::Dark),
            })
        );
        assert_eq!(
            parse("#fff", None),
            Some(Backdrop {
                rgb: (255, 255, 255),
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
            assert_eq!(parse(bad, None), None, "{bad}");
        }
        assert_eq!(parse("#f4efe4", Some("dim")), None);
        assert_eq!(parse("#f4efe4", Some("")), None);
    }

    #[test]
    fn paints_the_window_except_on_linux() {
        let backdrop = Backdrop {
            rgb: (0xf4, 0xef, 0xe4),
            appearance: Some(Theme::Light),
        };
        let alpha = if cfg!(target_os = "linux") { 0 } else { 255 };
        assert_eq!(backdrop.window_color(), Color(0xf4, 0xef, 0xe4, alpha));
    }

    #[test]
    fn keeps_the_last_good_report() {
        window_backdrop_set("#102030".into(), Some("dark".into())).unwrap();
        assert!(window_backdrop_set("nope".into(), None).is_err());
        assert_eq!(
            current(),
            Some(Backdrop {
                rgb: (0x10, 0x20, 0x30),
                appearance: Some(Theme::Dark),
            })
        );
        window_backdrop_set("#fdfcf8".into(), None).unwrap();
        assert_eq!(
            current(),
            Some(Backdrop {
                rgb: (0xfd, 0xfc, 0xf8),
                appearance: None,
            })
        );
    }
}

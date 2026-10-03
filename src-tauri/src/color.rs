//! Reading the colors the page sends as CSS text. The native renderer
//! takes its chrome colors and the split divider color through
//! [`parse_css_color`], as a [`Paint`] with straight alpha.

#[cfg(native_surface)]
use alacritty_terminal::vte::ansi::Rgb;

/// A color with straight alpha: sRGB bytes plus an alpha in 0..1, the way
/// CSS writes `rgba()`. The chrome tokens arrive in this form, and several
/// of them are translucent.
#[cfg(native_surface)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Paint {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32,
}

#[cfg(native_surface)]
impl Paint {
    pub(crate) fn opaque(c: Rgb) -> Self {
        Self::tint(c, 1.0)
    }

    pub(crate) fn tint(c: Rgb, a: f32) -> Self {
        Self {
            r: c.r,
            g: c.g,
            b: c.b,
            a,
        }
    }
}

/// Parse a CSS color: `#rgb`, `#rgba`, `#rrggbb`, or `#rrggbbaa` (the `#`
/// optional), or `rgb()`/`rgba()` with comma or space separated channels
/// and an optional alpha as a fraction or a percentage. None for anything
/// else, so the caller falls back to its default.
#[cfg(native_surface)]
// The channels read clearest as r, g, b and a, the names CSS gives them.
#[allow(clippy::many_single_char_names)]
pub(crate) fn parse_css_color(value: &str) -> Option<Paint> {
    let v = value.trim().to_ascii_lowercase();
    let hex = v.strip_prefix('#').unwrap_or(&v);
    if matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        let digit = |i: usize| u8::from_str_radix(&hex[i..=i], 16).unwrap_or(0);
        let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0);
        let (r, g, b, a) = if hex.len() <= 4 {
            let a = if hex.len() == 4 { digit(3) * 17 } else { 255 };
            (digit(0) * 17, digit(1) * 17, digit(2) * 17, a)
        } else {
            let a = if hex.len() == 8 { pair(6) } else { 255 };
            (pair(0), pair(2), pair(4), a)
        };
        return Some(Paint {
            r,
            g,
            b,
            a: f32::from(a) / 255.0,
        });
    }
    let inner = v
        .strip_prefix("rgba(")
        .or_else(|| v.strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let parts: Vec<&str> = inner
        .split(|c: char| c == ',' || c == '/' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() != 3 && parts.len() != 4 {
        return None;
    }
    let ch = |s: &str| {
        s.parse::<f32>()
            .ok()
            .filter(|n| n.is_finite())
            .map(|n| n.round().clamp(0.0, 255.0) as u8)
    };
    let a = match parts.get(3) {
        None => 1.0,
        Some(s) => match s.strip_suffix('%') {
            Some(pct) => pct.parse::<f32>().ok()? / 100.0,
            None => s.parse::<f32>().ok()?,
        },
    };
    if !a.is_finite() {
        return None;
    }
    Some(Paint {
        r: ch(parts[0])?,
        g: ch(parts[1])?,
        b: ch(parts[2])?,
        a: a.clamp(0.0, 1.0),
    })
}

#[cfg(all(test, native_surface))]
mod tests {
    use super::{parse_css_color, Paint};

    fn paint(r: u8, g: u8, b: u8, a: f32) -> Paint {
        Paint { r, g, b, a }
    }

    #[test]
    fn parse_css_color_accepts_hex_and_rgb_forms() {
        assert_eq!(
            parse_css_color("#3a404c"),
            Some(paint(0x3a, 0x40, 0x4c, 1.0))
        );
        assert_eq!(
            parse_css_color("3a404c"),
            Some(paint(0x3a, 0x40, 0x4c, 1.0))
        );
        assert_eq!(parse_css_color("#fff"), Some(paint(255, 255, 255, 1.0)));
        assert_eq!(parse_css_color("rgb(1, 2, 3)"), Some(paint(1, 2, 3, 1.0)));
        assert_eq!(
            parse_css_color("rgba(10,20,30,0.5)"),
            Some(paint(10, 20, 30, 0.5))
        );
        assert_eq!(parse_css_color("bright-red"), None);
        assert_eq!(parse_css_color(""), None);
    }

    #[test]
    fn parse_css_color_reads_alpha_in_every_form() {
        assert_eq!(
            parse_css_color("#ffffff80"),
            Some(paint(255, 255, 255, 128.0 / 255.0))
        );
        assert_eq!(
            parse_css_color("#0008"),
            Some(paint(0, 0, 0, 136.0 / 255.0))
        );
        assert_eq!(
            parse_css_color("rgba(136, 192, 208, 0.22)"),
            Some(paint(136, 192, 208, 0.22))
        );
        assert_eq!(
            parse_css_color("rgb(136 192 208 / 22%)"),
            Some(paint(136, 192, 208, 0.22))
        );
        // Alpha clamps into 0..1, and a bad channel or alpha rejects.
        assert_eq!(parse_css_color("rgba(1,2,3,4)"), Some(paint(1, 2, 3, 1.0)));
        assert_eq!(parse_css_color("rgba(1,2,3,x)"), None);
        assert_eq!(parse_css_color("rgb(1,2)"), None);
        assert_eq!(parse_css_color("rgb(1,2,3,4,5)"), None);
    }
}

//! Select Graphic Rendition state. Tracks foreground, background, and the
//! common attribute flags across SGR sequences.

use crate::color::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Attributes {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub inverse: bool,
}

impl Attributes {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Render an SGR parameter list against an [`Attributes`] state.
///
/// The parameter list is what `vte` hands us inside a CSI `m` dispatch:
/// each element is a slice because xterm-style colon separated subparameters
/// can carry multiple integers (for example the 38;2;r;g;b truecolor form).
pub struct Sgr;

impl Sgr {
    pub fn apply(attrs: &mut Attributes, params: &vte::Params) {
        let mut iter = params.iter();
        while let Some(param) = iter.next() {
            let primary = param.first().copied().unwrap_or(0);
            match primary {
                0 => attrs.reset(),
                1 => attrs.bold = true,
                3 => attrs.italic = true,
                4 => attrs.underline = true,
                7 => attrs.inverse = true,
                9 => attrs.strikethrough = true,
                22 => attrs.bold = false,
                23 => attrs.italic = false,
                24 => attrs.underline = false,
                27 => attrs.inverse = false,
                29 => attrs.strikethrough = false,
                30..=37 => attrs.fg = Color::Indexed16((primary - 30) as u8),
                38 => attrs.fg = parse_extended_color(param, &mut iter),
                39 => attrs.fg = Color::Default,
                40..=47 => attrs.bg = Color::Indexed16((primary - 40) as u8),
                48 => attrs.bg = parse_extended_color(param, &mut iter),
                49 => attrs.bg = Color::Default,
                90..=97 => attrs.fg = Color::Indexed16((primary - 90 + 8) as u8),
                100..=107 => attrs.bg = Color::Indexed16((primary - 100 + 8) as u8),
                _ => {}
            }
        }
    }
}

/// Parse an extended color (256 or truecolor) starting at `param`. xterm
/// supports two forms.
///
/// Subparameter form, e.g. `38:5:200` or `38:2:0:255:0`. The whole color
/// payload sits inside one parameter slice.
///
/// Legacy semicolon form, e.g. `38;5;200` or `38;2;255;0;0`. The payload is
/// split across consecutive parameters in the iterator.
fn parse_extended_color(
    param: &[u16],
    iter: &mut vte::ParamsIter<'_>,
) -> Color {
    if param.len() > 1 {
        match param.get(1).copied() {
            Some(5) => return Color::Indexed256(param.get(2).copied().unwrap_or(0) as u8),
            Some(2) => {
                // 38:2:r:g:b or 38:2:colorspace:r:g:b. Pick whichever fits.
                if param.len() >= 5 {
                    return Color::Rgb {
                        r: param.get(2).copied().unwrap_or(0) as u8,
                        g: param.get(3).copied().unwrap_or(0) as u8,
                        b: param.get(4).copied().unwrap_or(0) as u8,
                    };
                }
                if param.len() >= 6 {
                    return Color::Rgb {
                        r: param.get(3).copied().unwrap_or(0) as u8,
                        g: param.get(4).copied().unwrap_or(0) as u8,
                        b: param.get(5).copied().unwrap_or(0) as u8,
                    };
                }
            }
            _ => return Color::Default,
        }
    }

    // Legacy semicolon form. Pull subsequent parameters from the iterator.
    let kind = iter
        .next()
        .and_then(|p| p.first().copied())
        .unwrap_or(0);
    match kind {
        5 => {
            let idx = iter
                .next()
                .and_then(|p| p.first().copied())
                .unwrap_or(0) as u8;
            Color::Indexed256(idx)
        }
        2 => {
            let r = iter.next().and_then(|p| p.first().copied()).unwrap_or(0) as u8;
            let g = iter.next().and_then(|p| p.first().copied()).unwrap_or(0) as u8;
            let b = iter.next().and_then(|p| p.first().copied()).unwrap_or(0) as u8;
            Color::Rgb { r, g, b }
        }
        _ => Color::Default,
    }
}

//! The SGR state the renderer tracks as it writes, and the fewest codes
//! that turn one state into another. The stage reads game lines with it
//! too.

use super::SpanColor;
use crate::design::{Layer, UnderlineStyle};

/// A color in the SGR state, kept in the form it was written so a restore
/// writes the same form back.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Color {
    #[default]
    Default,
    /// SGR 30 to 37 and 90 to 97 (40 to 47 and 100 to 107), 0 to 15.
    Ansi(u8),
    /// A 256 palette index.
    Index(u8),
    Rgb(u8, u8, u8),
}

impl Color {
    pub(super) fn params(self, background: bool) -> String {
        let base = if background { 40 } else { 30 };
        match self {
            Color::Default => (base + 9).to_string(),
            Color::Ansi(n) if n < 8 => (base + u32::from(n)).to_string(),
            Color::Ansi(n) => (base + 60 + u32::from(n - 8)).to_string(),
            Color::Index(n) => format!("{};5;{n}", base + 8),
            Color::Rgb(r, g, b) => format!("{};2;{r};{g};{b}", base + 8),
        }
    }

    /// The SGR parameters for the underline's color. SGR 58 has no short
    /// form for a theme color, so one goes as its palette index, and a
    /// true color names its empty color space as terminals expect.
    pub(super) fn underline_params(self) -> String {
        match self {
            Color::Default => "59".to_string(),
            Color::Ansi(n) | Color::Index(n) => format!("58:5:{n}"),
            Color::Rgb(r, g, b) => format!("58:2::{r}:{g}:{b}"),
        }
    }

    pub(super) fn layer_params(self, layer: Layer) -> String {
        match layer {
            Layer::Fg => self.params(false),
            Layer::Bg => self.params(true),
            Layer::Underline => self.underline_params(),
        }
    }

    pub(super) fn span(self) -> SpanColor {
        match self {
            Color::Default => SpanColor::Default,
            Color::Ansi(index) | Color::Index(index) => SpanColor::Index { index },
            Color::Rgb(r, g, b) => SpanColor::Rgb { r, g, b },
        }
    }
}

/// An extended color, `5;n` or `2;r;g;b`, from the numbers after a 38,
/// 48 or 58. A colon form may name a color space first, `2::r:g:b`, and
/// `colon` says the numbers came that way. Returns the color and how many
/// numbers it took.
fn extended_color(nums: &[u32], colon: bool) -> (Option<Color>, usize) {
    let byte = |n: Option<&u32>| n.copied().unwrap_or(0).min(255) as u8;
    match nums.first() {
        Some(5) => (nums.get(1).map(|n| Color::Index(byte(Some(n)))), 2),
        Some(2) => {
            let at = if colon && nums.len() > 4 { 2 } else { 1 };
            let rgb = Color::Rgb(
                byte(nums.get(at)),
                byte(nums.get(at + 1)),
                byte(nums.get(at + 2)),
            );
            (Some(rgb), 4)
        }
        _ => (None, 1),
    }
}

/// The SGR state the renderer tracks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SgrState {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    /// The kind of underline, None with no underline.
    pub underline: Option<UnderlineStyle>,
    /// The underline's color, Default for the text's own.
    pub underline_color: Color,
    pub inverse: bool,
    pub strike: bool,
    /// SGR 5. The rapid 6 draws steady in every renderer, as xterm
    /// draws it, so it is no blink here either.
    pub blink: bool,
}

impl SgrState {
    /// Apply an SGR parameter list such as `0;1;31`, `38;5;240` or
    /// `4:3;58:2::191:97:106`. A parameter with colons is one code with
    /// its sub parameters, so `4:3` is a curly underline and never an
    /// underline and an italic.
    pub fn apply(&mut self, params: &str) {
        let num = |p: &str| {
            if p.is_empty() {
                0
            } else {
                p.parse::<u32>().unwrap_or(u32::MAX)
            }
        };
        let groups: Vec<Vec<u32>> = params
            .split(';')
            .map(|group| group.split(':').map(num).collect())
            .collect();
        let mut i = 0;
        while i < groups.len() {
            let n = groups[i][0];
            let subs = &groups[i][1..];
            match n {
                0 => *self = SgrState::default(),
                1 => self.bold = true,
                2 => self.dim = true,
                3 => self.italic = true,
                4 => {
                    self.underline = match subs.first() {
                        Some(kind) => UnderlineStyle::from_sgr(*kind),
                        None => Some(UnderlineStyle::Single),
                    };
                }
                5 => self.blink = true,
                7 => self.inverse = true,
                9 => self.strike = true,
                21 => self.underline = Some(UnderlineStyle::Double),
                22 => {
                    self.bold = false;
                    self.dim = false;
                }
                23 => self.italic = false,
                24 => self.underline = None,
                25 => self.blink = false,
                27 => self.inverse = false,
                29 => self.strike = false,
                30..=37 => self.fg = Color::Ansi((n - 30) as u8),
                39 => self.fg = Color::Default,
                40..=47 => self.bg = Color::Ansi((n - 40) as u8),
                49 => self.bg = Color::Default,
                59 => self.underline_color = Color::Default,
                90..=97 => self.fg = Color::Ansi((n - 90 + 8) as u8),
                100..=107 => self.bg = Color::Ansi((n - 100 + 8) as u8),
                38 | 48 | 58 => {
                    let color = if subs.is_empty() {
                        // The semicolon form: the color takes the
                        // parameters after this one.
                        let rest: Vec<u32> = groups[i + 1..].iter().map(|g| g[0]).collect();
                        let (color, used) = extended_color(&rest, false);
                        i += used.min(rest.len());
                        color
                    } else {
                        extended_color(subs, true).0
                    };
                    if let Some(color) = color {
                        match n {
                            38 => self.fg = color,
                            48 => self.bg = color,
                            _ => self.underline_color = color,
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    /// The fewest SGR parameters that turn this state into `to`. Empty
    /// when they match.
    pub fn transition(&self, to: &SgrState) -> String {
        let mut p: Vec<String> = Vec::new();
        if (self.bold && !to.bold) || (self.dim && !to.dim) {
            p.push("22".into());
            if to.bold {
                p.push("1".into());
            }
            if to.dim {
                p.push("2".into());
            }
        } else {
            if to.bold && !self.bold {
                p.push("1".into());
            }
            if to.dim && !self.dim {
                p.push("2".into());
            }
        }
        if self.italic != to.italic {
            p.push(if to.italic { "3" } else { "23" }.into());
        }
        if self.underline != to.underline {
            p.push(to.underline.map_or("24", UnderlineStyle::sgr).into());
        }
        for (from, want, on, off) in [
            (self.blink, to.blink, "5", "25"),
            (self.inverse, to.inverse, "7", "27"),
            (self.strike, to.strike, "9", "29"),
        ] {
            if from != want {
                p.push(if want { on } else { off }.into());
            }
        }
        if self.fg != to.fg {
            p.push(to.fg.params(false));
        }
        if self.bg != to.bg {
            p.push(to.bg.params(true));
        }
        if self.underline_color != to.underline_color {
            p.push(to.underline_color.underline_params());
        }
        p.join(";")
    }
}

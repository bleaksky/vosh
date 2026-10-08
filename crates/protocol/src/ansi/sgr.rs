//! The SGR model. [`AnsiParser`] splits bytes into [`Span`]s of text that
//! share one set of [`Attributes`], and [`Sgr`] tracks the foreground, the
//! background and the common flags across SGR sequences. Save a scene
//! writes each span of a line as HTML, and the readable highlight tests in
//! vosh-automation read the color of each span, with a parser apart from
//! the scan they check.

use vte::{Params, Parser, Perform};

use crate::ansi::color::Color;
use crate::ansi::parser::keeps_control;

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
fn parse_extended_color(param: &[u16], iter: &mut vte::ParamsIter<'_>) -> Color {
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
    let kind = iter.next().and_then(|p| p.first().copied()).unwrap_or(0);
    match kind {
        5 => {
            let idx = iter.next().and_then(|p| p.first().copied()).unwrap_or(0) as u8;
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

/// A run of text sharing the same SGR attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub attrs: Attributes,
}

#[derive(Default)]
struct Collector {
    attrs: Attributes,
    pending: String,
    spans: Vec<Span>,
}

impl Collector {
    fn flush(&mut self) {
        if !self.pending.is_empty() {
            self.spans.push(Span {
                text: std::mem::take(&mut self.pending),
                attrs: self.attrs,
            });
        }
    }
}

impl Perform for Collector {
    fn print(&mut self, c: char) {
        self.pending.push(c);
    }

    fn execute(&mut self, byte: u8) {
        // The same text plain_text keeps. The renderers read the other
        // control bytes from the raw line.
        if keeps_control(byte) {
            self.pending.push(char::from(byte));
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        if action == 'm' {
            // SGR boundary. Flush the run of text under the old attributes
            // before applying the new ones.
            self.flush();
            Sgr::apply(&mut self.attrs, params);
        }
        // Cursor movement, screen control and the rest belong to the
        // renderers, which read the raw line.
    }
}

/// Splits bytes into [`Span`]s by their SGR attributes. It keeps its state
/// between calls to [`AnsiParser::feed`], so an escape cut across two
/// calls still parses.
pub struct AnsiParser {
    machine: Parser,
    collector: Collector,
}

impl Default for AnsiParser {
    fn default() -> Self {
        Self::new()
    }
}

impl AnsiParser {
    pub fn new() -> Self {
        Self {
            machine: Parser::new(),
            collector: Collector::default(),
        }
    }

    /// Feed bytes that have already had telnet IAC sequences stripped. Drains
    /// all currently complete spans and returns them.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Span> {
        for &b in bytes {
            self.machine.advance(&mut self.collector, b);
        }
        self.collector.flush();
        std::mem::take(&mut self.collector.spans)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_data_makes_one_span() {
        let mut p = AnsiParser::new();
        let spans = p.feed(b"hello world");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "hello world");
        assert_eq!(spans[0].attrs, Attributes::default());
    }

    #[test]
    fn red_then_default_makes_two_spans() {
        let mut p = AnsiParser::new();
        let spans = p.feed(b"\x1b[31mred\x1b[0m off");
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].text, "red");
        assert_eq!(spans[0].attrs.fg, Color::Indexed16(1));
        assert_eq!(spans[1].text, " off");
        assert_eq!(spans[1].attrs, Attributes::default());
    }

    #[test]
    fn parses_256_color_legacy_form() {
        let mut p = AnsiParser::new();
        let spans = p.feed(b"\x1b[38;5;200mfoo");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "foo");
        assert_eq!(spans[0].attrs.fg, Color::Indexed256(200));
    }

    #[test]
    fn parses_truecolor_legacy_form() {
        let mut p = AnsiParser::new();
        let spans = p.feed(b"\x1b[38;2;255;128;0mhot");
        assert_eq!(spans.len(), 1);
        assert_eq!(
            spans[0].attrs.fg,
            Color::Rgb {
                r: 255,
                g: 128,
                b: 0
            }
        );
    }

    #[test]
    fn parses_truecolor_background() {
        let mut p = AnsiParser::new();
        let spans = p.feed(b"\x1b[48;2;0;255;0mlawn");
        assert_eq!(spans[0].attrs.bg, Color::Rgb { r: 0, g: 255, b: 0 });
    }

    #[test]
    fn bright_foreground_maps_to_high_palette() {
        let mut p = AnsiParser::new();
        let spans = p.feed(b"\x1b[91mbright");
        assert_eq!(spans[0].attrs.fg, Color::Indexed16(9));
    }

    #[test]
    fn bold_then_unbold() {
        let mut p = AnsiParser::new();
        let spans = p.feed(b"\x1b[1mB\x1b[22mn");
        assert!(spans[0].attrs.bold);
        assert!(!spans[1].attrs.bold);
    }

    #[test]
    fn handles_escape_split_across_chunks() {
        let mut p = AnsiParser::new();
        let mut spans = p.feed(b"a\x1b[3");
        spans.extend(p.feed(b"1mred"));
        // First span emitted on flush before color was known.
        assert_eq!(spans[0].text, "a");
        assert_eq!(spans[0].attrs, Attributes::default());
        // The color landed before "red" arrived.
        let colored = spans.iter().find(|s| s.text == "red").unwrap();
        assert_eq!(colored.attrs.fg, Color::Indexed16(1));
    }

    #[test]
    fn utf8_passes_through() {
        let mut p = AnsiParser::new();
        let bytes = "\u{1f409} \u{4e2d}\u{6587}".as_bytes();
        let spans = p.feed(bytes);
        let combined: String = spans.into_iter().map(|s| s.text).collect();
        assert_eq!(combined, "\u{1f409} \u{4e2d}\u{6587}");
    }
}

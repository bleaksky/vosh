//! Streaming ANSI parser. Wraps the `vte` state machine and produces
//! attributed text spans suitable for trigger matching.

use vte::{Params, Parser, Perform};

use crate::sgr::{Attributes, Sgr};

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
        // Treat printable control bytes (CR, LF, BS, TAB) as text. Drop the
        // rest. xterm.js handles cursor motion on its side.
        match byte {
            b'\n' | b'\r' | b'\t' | 0x08 => self.pending.push(byte as char),
            _ => {}
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
        // Cursor movement, screen control, and the rest are renderer
        // concerns. xterm.js handles them.
    }
}

/// Streaming ANSI parser. Hold an instance per session so escape sequences
/// that straddle byte chunks parse correctly.
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

    /// Current SGR attributes after all bytes consumed so far.
    pub fn current_attributes(&self) -> Attributes {
        self.collector.attrs
    }

    /// Reset attributes and pending state. Call when the connection drops.
    pub fn reset(&mut self) {
        self.machine = Parser::new();
        self.collector = Collector::default();
    }
}

/// Strip every escape sequence and control byte from a buffer, returning the
/// printable text. Useful for trigger matching.
pub fn plain_text(bytes: &[u8]) -> String {
    let mut p = AnsiParser::new();
    p.feed(bytes)
        .into_iter()
        .map(|s| s.text)
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

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
        assert_eq!(
            spans[0].attrs.bg,
            Color::Rgb {
                r: 0,
                g: 255,
                b: 0
            }
        );
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
        assert_eq!(spans[0].attrs.bold, true);
        assert_eq!(spans[1].attrs.bold, false);
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
    fn plain_text_strips_escapes() {
        let stripped = plain_text(b"\x1b[31mYou are hit.\x1b[0m");
        assert_eq!(stripped, "You are hit.");
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

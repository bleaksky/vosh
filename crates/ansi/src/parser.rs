//! The two readers the app runs on a line the game sends. Each call
//! starts a fresh `vte` parser on the bytes it gets, so nothing carries
//! from one call to the next.

use vte::{Params, Parser, Perform};

/// The control bytes [`plain_text`] keeps as text. Newline, carriage
/// return, tab and backspace.
pub(crate) fn keeps_control(byte: u8) -> bool {
    matches!(byte, b'\n' | b'\r' | b'\t' | 0x08)
}

/// The text of a line without its escape sequences. The session reads it
/// for every line the game sends, to match triggers and to log. On a line
/// that holds an escape, every control byte but newline, carriage return,
/// tab and backspace drops too.
///
/// A line with no ESC byte holds no escape sequence, so it skips the
/// parser and costs one `String::from_utf8_lossy`. Most prompts and many
/// says and tells arrive that way.
pub fn plain_text(bytes: &[u8]) -> String {
    struct Text(String);
    impl Perform for Text {
        fn print(&mut self, c: char) {
            self.0.push(c);
        }

        fn execute(&mut self, byte: u8) {
            if keeps_control(byte) {
                self.0.push(char::from(byte));
            }
        }
    }

    if !bytes.contains(&0x1B) {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let mut machine = Parser::new();
    let mut text = Text(String::with_capacity(bytes.len()));
    for &b in bytes {
        machine.advance(&mut text, b);
    }
    text.0
}

/// What a stretch of a line's bytes is, as [`plain_text`] reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PieceKind {
    /// A character [`plain_text`] keeps.
    Text(char),
    /// An SGR sequence. Its parameters as `;` separated numbers, with the
    /// parts of a colon group joined by `:`. A missing number reads `0`,
    /// so `ESC[m` gives `0`.
    Sgr(String),
    /// Bytes [`plain_text`] drops that set no text attribute, such as a
    /// cursor sequence, an OSC string or a bell.
    Other,
}

/// One stretch of a line's bytes. See [`pieces`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub kind: PieceKind,
    /// The bytes it covers.
    pub raw: std::ops::Range<usize>,
}

/// The [`Piece`]s of `bytes`, in order. Each byte belongs to the piece
/// whose last byte follows it, so the ranges run end to end, and bytes
/// after the last piece, such as an escape cut off at the end, belong to
/// none. The `Text` pieces spell exactly what [`plain_text`] returns,
/// since both run the same parser, so a span of the plain text maps back
/// to the bytes the game sent.
pub fn pieces(bytes: &[u8]) -> Vec<Piece> {
    struct Pieces {
        /// The byte the parser is on.
        at: usize,
        /// Where the next piece starts.
        start: usize,
        out: Vec<Piece>,
    }
    impl Pieces {
        fn push(&mut self, kind: PieceKind) {
            let end = self.at + 1;
            self.out.push(Piece {
                kind,
                raw: self.start..end,
            });
            self.start = end;
        }
    }
    impl Perform for Pieces {
        fn print(&mut self, c: char) {
            self.push(PieceKind::Text(c));
        }

        fn execute(&mut self, byte: u8) {
            if keeps_control(byte) {
                self.push(PieceKind::Text(char::from(byte)));
            } else {
                self.push(PieceKind::Other);
            }
        }

        fn csi_dispatch(
            &mut self,
            params: &Params,
            intermediates: &[u8],
            _ignore: bool,
            action: char,
        ) {
            if action != 'm' || !intermediates.is_empty() {
                self.push(PieceKind::Other);
                return;
            }
            let codes: Vec<String> = params
                .iter()
                .map(|group| {
                    group
                        .iter()
                        .map(u16::to_string)
                        .collect::<Vec<_>>()
                        .join(":")
                })
                .collect();
            self.push(PieceKind::Sgr(codes.join(";")));
        }

        fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {
            self.push(PieceKind::Other);
        }

        fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {
            self.push(PieceKind::Other);
        }

        fn unhook(&mut self) {
            self.push(PieceKind::Other);
        }
    }

    let mut machine = Parser::new();
    let mut walk = Pieces {
        at: 0,
        start: 0,
        out: Vec::new(),
    };
    for (at, &b) in bytes.iter().enumerate() {
        walk.at = at;
        machine.advance(&mut walk, b);
    }
    walk.out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The text the `Text` pieces of `bytes` spell.
    fn spelled(bytes: &[u8]) -> String {
        pieces(bytes)
            .into_iter()
            .filter_map(|p| match p.kind {
                PieceKind::Text(c) => Some(c),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn pieces_cover_the_bytes_end_to_end_and_spell_the_plain_text() {
        // The WiZNET line act_wiz.c sends with the TICK! message from
        // update.c, with a cursor move, a bell, an OSC title, a 256 color,
        // a colon group, an empty SGR and a tab in among it.
        let line = "\x1b[0;1;37mW\x1b[0;1;30mi\x1b[0;1;37mZNET\x1b[0;0m \x1b[2K\x07\
                    \x1b]0;\x07\x1b[38;5;208m08:20:01\x1b[4:3m:\x1b[m\tTICK!";
        let bytes = line.as_bytes();
        let all = pieces(bytes);
        let mut at = 0;
        for piece in &all {
            assert_eq!(piece.raw.start, at, "{all:?}");
            at = piece.raw.end;
        }
        assert_eq!(at, bytes.len());
        assert_eq!(spelled(bytes), plain_text(bytes));
        let sgr: Vec<&str> = all
            .iter()
            .filter_map(|p| match &p.kind {
                PieceKind::Sgr(params) => Some(params.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            sgr,
            ["0;1;37", "0;1;30", "0;1;37", "0;0", "38;5;208", "4:3", "0"]
        );
        let other = all.iter().filter(|p| p.kind == PieceKind::Other).count();
        assert_eq!(other, 3, "{all:?}");
        assert_eq!(all.last().unwrap().kind, PieceKind::Text('!'));
    }

    #[test]
    fn an_escape_cut_off_at_the_end_belongs_to_no_piece() {
        let all = pieces(b"OK\x1b[0;3");
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].raw, 1..2);
    }

    #[test]
    fn plain_text_strips_escapes() {
        let stripped = plain_text(b"\x1b[31mYou are hit.\x1b[0m");
        assert_eq!(stripped, "You are hit.");
    }
}

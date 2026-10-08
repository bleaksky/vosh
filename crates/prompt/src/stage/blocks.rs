//! A prompt the stage read, line by line as the game sent it, and the
//! open row that draws it with what your prompt shows.

use std::collections::BTreeMap;

use crate::render::Span;

/// How a line of a recognized prompt ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    /// A line end the prompt waits for, as one ending in `%c` does.
    Line,
    /// A GA or EOR ended the partial.
    Marker,
    /// The partial settled at the end of a read, with nothing after it,
    /// or a GA or EOR came after a prompt that settles.
    Settled,
    /// The prompt settles, so it was whole before the line end that came
    /// after it. The line end starts the game's next row, so it follows
    /// the prompt as it would had the prompt settled at the end of a
    /// read.
    SettledLine,
}

impl End {
    /// What ends the line when it shows as sent. A GA ends the row, as it
    /// always did, unless the prompt settles and so keeps the cursor after
    /// it.
    pub(super) fn terminator(self) -> &'static [u8] {
        match self {
            End::Line | End::Marker | End::SettledLine => b"\r\n",
            End::Settled => b"",
        }
    }
}

/// One line of a recognized prompt, as the game sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockLine {
    pub raw: Vec<u8>,
    pub plain: String,
    pub end: End,
}

/// A prompt the stage read, as the game sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub lines: Vec<BlockLine>,
    /// The lines the drawn prompt replaces, by index. The drawn prompt
    /// always replaces the final line. A line above it shows as the game
    /// sent it unless your design reads a value it carries.
    pub replaced: Vec<usize>,
    /// What each group read, by variable.
    pub values: BTreeMap<String, String>,
    /// The game's away prompt. It shows as sent, even while Vosh draws.
    pub afk: bool,
    /// The groups each line reads, top line first, so the lines the drawn
    /// prompt replaces follow an edit that changes what your design
    /// reads. Empty when not known, and then `replaced` stays as it is.
    pub groups: Vec<Vec<String>>,
}

impl Block {
    /// The line Prompts triggers run on.
    pub fn final_line(&self) -> &BlockLine {
        self.lines
            .last()
            .expect("a recognized block has at least one line")
    }

    /// The block as sent, colors included, for `%{raw}`.
    pub fn raw_text(&self) -> String {
        let lines: Vec<String> = self
            .lines
            .iter()
            .map(|l| String::from_utf8_lossy(&l.raw).into_owned())
            .collect();
        lines.join("\r\n")
    }

    /// The lines above the final one that show as the game sent them
    /// while Vosh draws, each with its line end.
    pub(super) fn heads_shown(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let last = self.lines.len().saturating_sub(1);
        for (index, line) in self.lines.iter().enumerate().take(last) {
            if !self.replaced.contains(&index) {
                out.extend_from_slice(&line.raw);
                out.extend_from_slice(b"\r\n");
            }
        }
        out
    }

    /// How many lines [`Block::heads_shown`] shows.
    pub(super) fn heads_shown_rows(&self) -> usize {
        let last = self.lines.len().saturating_sub(1);
        (0..last)
            .filter(|index| !self.replaced.contains(index))
            .count()
    }

    /// The lines [`Block::heads_shown`] shows, with their plain text, or
    /// None when it shows none.
    pub(super) fn heads_shown_with_text(&self) -> Option<(Vec<u8>, String)> {
        let bytes = self.heads_shown();
        if bytes.is_empty() {
            return None;
        }
        let last = self.lines.len().saturating_sub(1);
        let plain: Vec<&str> = self
            .lines
            .iter()
            .enumerate()
            .take(last)
            .filter(|(index, _)| !self.replaced.contains(index))
            .map(|(_, line)| line.plain.as_str())
            .collect();
        Some((bytes, plain.join("\n")))
    }

    /// Every line above the final one, as the game sent it, each with its
    /// line end, for a block that shows as sent.
    pub(super) fn heads(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let last = self.lines.len().saturating_sub(1);
        for line in self.lines.iter().take(last) {
            out.extend_from_slice(&line.raw);
            out.extend_from_slice(b"\r\n");
        }
        out
    }

    /// The lines the drawn prompt replaced, as the game showed them, for
    /// drawing off.
    pub fn shown(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for index in &self.replaced {
            if let Some(line) = self.lines.get(*index) {
                out.extend_from_slice(&line.raw);
                out.extend_from_slice(line.end.terminator());
            }
        }
        out
    }
}

/// The lift the open row carries while your prompt shows lifted, or the
/// open card borrows Lifted's band in the text, and whether its start
/// mark sits inside the row's region, as it does when you chose Lifted
/// with the row open, so a repaint writes it again. Its end keeps your
/// echo a cell away, the card's borrowed lift too (the 2026-09-30
/// addendum, item 2), so a row the card lifted keeps that space when you
/// later choose Lifted, in the open row and in your scrollback alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OpenLift {
    pub(super) id: u64,
    pub(super) start_inside: bool,
}

/// The drawn prompt while it is the last thing on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenRow {
    pub gen: u64,
    /// What the row shows: the drawn prompt, or with drawing off the lines
    /// it replaced.
    pub body: Vec<u8>,
    /// The drawn prompt with the live values, while the row shows a
    /// preview in its place. Renderers hold it as the row's restore.
    pub live: Option<Vec<u8>>,
    /// Where each piece of the design landed in the row, from the render
    /// that drew it. Empty while the row shows the game's own lines.
    pub spans: Vec<Span>,
    /// The rows of the design the row shows, as plain text joined by
    /// `\n`, which the webview wraps at its own width to put each span
    /// on screen. Empty while the row shows the game's own lines.
    pub plain: String,
}

/// What your prompt shows, handed to [`Stage::draw_view`],
/// [`Stage::pin_view`] and [`Stage::repaint_view`].
///
/// [`Stage::draw_view`]: super::Stage::draw_view
/// [`Stage::pin_view`]: super::Stage::pin_view
/// [`Stage::repaint_view`]: super::Stage::repaint_view
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct View<'a> {
    /// Your design drawn as your prompt shows it, or None for the lines
    /// the game sent: drawing off, or the card reading your codes.
    pub shown: Option<&'a str>,
    /// Your design drawn with the live values, while `shown` is a preview
    /// of the open card's in its place: other values, the labels of the
    /// values with nothing to show, or the game's own line. The region
    /// carries it as its restore, so only live renders reach history.
    /// None when your prompt shows the live render. The band above the
    /// command line never needs it, since nothing on it reaches history.
    pub live: Option<&'a str>,
    /// Where each piece of the design landed in `shown`, from the render
    /// that drew it. Empty while `shown` is None.
    pub spans: &'a [Span],
    /// The rows `shown` draws, as plain text joined by `\n`. Empty while
    /// `shown` is None.
    pub plain: &'a str,
}

#[cfg(any(test, feature = "testkit"))]
impl<'a> View<'a> {
    /// The live render, or the game's lines with drawing off. Test only.
    /// The app's tests reach it through `Stage::draw` and
    /// `Stage::repaint`, so it sits behind the `testkit` feature with
    /// them.
    pub(crate) fn live(rendered: Option<&'a str>) -> Self {
        Self {
            shown: rendered,
            ..Self::default()
        }
    }
}

impl View<'_> {
    /// The open row `gen` showing `body`, with the live render behind it,
    /// and where the pieces of what it shows landed.
    pub(super) fn open_row(self, gen: u64, body: Vec<u8>, live: Option<Vec<u8>>) -> OpenRow {
        let drawn = self.shown.is_some();
        OpenRow {
            gen,
            body,
            live,
            spans: if drawn {
                self.spans.to_vec()
            } else {
                Vec::new()
            },
            plain: if drawn {
                self.plain.to_string()
            } else {
                String::new()
            },
        }
    }
}

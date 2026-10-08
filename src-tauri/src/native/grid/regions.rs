//! The regions Vosh may replace on the grid, such as your drawn prompt,
//! with the line ends held back while it shows pinned, the lifts that tag
//! a lifted prompt and the rewrites that collapse repeated lines. Its twin
//! is `RegionWriter` in the page's terminalRegion.ts, and the pinned,
//! preview and pointer prompt fixtures and the collapse splits prove the
//! two agree.

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags, Hyperlink};
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::Processor;
use vosh_prompt::stage::{close_pin_row, Output, MARK_OSC};

use super::{GridSize, NoopListener, TermGrid};

/// A lift the grid saw the start mark of, while your prompt shows lifted:
/// where the mark came, as a region's does, and the bytes fed since, so
/// its end mark can count the rows back to it at the grid's width, however
/// far the screen scrolled between the two.
#[derive(Debug, Clone)]
pub(super) struct LiftTrack {
    id: u64,
    col: usize,
    wrap_pending: bool,
    bytes: Vec<u8>,
}

/// The hyperlink uri that tags a cell of a lifted prompt. Nothing in Vosh
/// reads hyperlinks, and alacritty keeps them through reflow, scrolling
/// and history as it keeps OSC 8 links.
pub(crate) const LIFT_URI: &str = "vosh:lift";

/// One row of a lift as the grid holds it: the lift, the grid line, the
/// first tagged column, one past the last tagged cell that shows a glyph,
/// and whether anything outside the lift shows after it on the row, such
/// as your echo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LiftSpan {
    pub id: u64,
    pub line: i32,
    pub first: usize,
    pub end: usize,
    pub after: bool,
}

/// Where the grid's cursor sits and where the open region starts, for
/// `terminal_cursor`. Lines count from the top of the live
/// screen, negative in history, as alacritty counts them, so while you are
/// at the bottom a line is the screen row the renderer draws it on. The
/// webview maps a pointer to a piece of your prompt from it: the cell under
/// the pointer, then the span there once the design is laid out from the
/// region's start at `cols` wide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(crate) struct CursorReport {
    pub line: i32,
    pub col: usize,
    /// The screen shows the live tail, not history you scrolled back to.
    pub at_bottom: bool,
    pub cols: usize,
    /// The open region, None once anything was written after it.
    pub region: Option<RegionStart>,
}

/// The live screen as text for `terminal_screen_rows`: each row with
/// trailing blanks gone and a wide character read once, the grid's width,
/// and whether the screen shows the live tail.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct ScreenRows {
    pub rows: Vec<String>,
    pub cols: usize,
    pub at_bottom: bool,
}

/// Where open region `gen` starts on the grid: the cell its first
/// character lands in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(crate) struct RegionStart {
    pub gen: u64,
    pub line: i32,
    pub col: usize,
}

/// A region Vosh may replace later, such as the drawn prompt, as this
/// grid holds it. It starts at a mark `ESC ] 7717 ; o ; G BEL` and
/// stays open while nothing else is written after it. The grid keeps
/// the bytes it wrote after the mark, so it can count the rows they take
/// at the width it has when a replace comes, a resize included.
#[derive(Debug, Clone)]
pub(super) struct Region {
    gen: u64,
    /// The column the mark came at.
    col: usize,
    /// The mark came with the cursor held past the last column, so the
    /// region starts at the next row.
    wrap_pending: bool,
    /// What the grid wrote after the mark, as it fed it.
    bytes: Vec<u8>,
    /// The live render to write back before anything else lands, when
    /// the region shows a preview.
    restore: Option<Vec<u8>>,
}

impl TermGrid {
    /// Write one session output, as `session://output` carries it to
    /// xterm: its replace, then its bytes, then its restore.
    ///
    /// A replace for the open region moves to the region's start,
    /// erases to the end of the screen and writes its bytes there. For a
    /// region something was written after, a fresh replace writes its
    /// bytes on a new row, and any other is dropped. The bytes then
    /// follow as they are, which closes the region. A restore rides the
    /// region the output leaves open, and goes back over it before
    /// anything else lands. Text is word wrapped at the grid width, as
    /// xterm's is by the webview.
    ///
    /// Line ends the last output held back go out before anything this
    /// one writes at the cursor, and after a replace of the open region,
    /// which lies before them. This output's own hold then waits. An
    /// output that writes nothing at the cursor keeps the longer of the
    /// two holds, so a pulse that pinned a prompt and wrote nothing never
    /// stacks empty rows. While the row a pinned prompt left is open, the
    /// line end that would end it writes nothing, and the output says
    /// whether the row is open after it.
    ///
    /// A replace whose bytes leave out the end of their region, which the
    /// grid should still hold back (`Replace::tail`), holds that end back
    /// in their place when it writes them on a new row, or finds the
    /// region open with nothing held, as when the region came from the
    /// scrollback the grid loaded.
    ///
    /// Bytes that start a row of their own (`Output::fresh`) get a line
    /// end first when, the held line ends written, the cursor sits past
    /// the start of a row.
    pub(crate) fn session_output(&mut self, out: &Output) {
        // A lift's two marks ride in one output.
        self.lift_tracks.clear();
        let mut wrote = false;
        if let Some(replace) = &out.replace {
            // Half a character the last write held back belongs to the
            // region the replace rewrites whole.
            self.pending_utf8.clear();
            let text = self.wrap(&String::from_utf8_lossy(&replace.bytes));
            if let Some(to_start) = self.locate(replace.gen) {
                // The lines above the region go with it when they are
                // there, such as the tank line on a change of where your
                // prompt shows.
                let above = replace.above.as_ref().and_then(|above| {
                    let to_first = self.locate_above(replace.gen, &above.plain)?;
                    Some((to_first, self.wrap(&String::from_utf8_lossy(&above.bytes))))
                });
                self.region = None;
                match above {
                    Some((to_first, above)) => {
                        self.feed(&to_first);
                        self.feed_marked(above.as_bytes());
                    }
                    None => {
                        self.feed(&to_start);
                        self.feed_marked(text.as_bytes());
                    }
                }
                if self.pending_hold.is_empty() {
                    self.pending_hold.clone_from(&replace.tail);
                }
            } else if replace.fresh && !replace.bytes.is_empty() {
                self.restore_first();
                self.write_hold();
                self.region = None;
                if !self.at_row_start() {
                    self.feed(b"\r\n");
                }
                let text = self.land(text);
                self.feed_marked(text.as_bytes());
                self.pending_hold.clone_from(&replace.tail);
            }
        }
        if !out.bytes.is_empty() {
            self.restore_first();
            self.write_hold();
            self.region = None;
            let mut text = self.decode(&out.bytes);
            // A line Vosh prints about itself starts a row of its own.
            // The row a pinned prompt left drops the line end, as it
            // drops any.
            if out.fresh && !self.at_row_start() {
                text.insert_str(0, "\r\n");
            }
            let text = self.land(text);
            if !text.is_empty() {
                let text = self.wrap(&text);
                // A quick key's echo, which the session sends.
                let text = self.without_mark(text.as_bytes());
                self.feed_marked(text);
            }
            wrote = true;
        }
        if wrote {
            self.pending_hold.clone_from(&out.hold);
        } else if self.echo_held {
            // The game's line ends come after your echo's own.
            if !out.hold.is_empty() {
                self.pending_hold.extend_from_slice(&out.hold);
                self.echo_held = false;
            }
        } else if out.hold.len() > self.pending_hold.len() {
            self.pending_hold.clone_from(&out.hold);
        }
        if let Some(open) = out.pin_row {
            self.pin_row = open;
        }
        if let (Some(restore), Some(region)) = (&out.restore, self.region.as_mut()) {
            region.restore = Some(restore.clone());
        }
    }

    /// Write text the webview wrote itself, such as your typed echo. It
    /// lands after the open region, so it closes it, and a preview the
    /// region shows goes back to the live render first. Held line ends go
    /// out before it. Your echo's mark drops where its row already ends
    /// in `>`.
    ///
    /// While your prompt shows pinned, the line ends the text ends on wait
    /// as held line ends do, so a reply that brings only a prompt leaves
    /// the text on your echo's row, as the last line of any other reply
    /// does. The grid knows it shows pinned by the line ends it holds
    /// back or the row the pinned prompt left open, which only a pinned
    /// prompt brings.
    pub(crate) fn local_write(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let pinned = !self.pending_hold.is_empty() || self.pin_row;
        self.lift_tracks.clear();
        self.restore_first();
        self.write_hold();
        self.region = None;
        let bytes = if self.pin_row {
            let (rest, closed) = close_pin_row(bytes);
            self.pin_row = !closed;
            rest
        } else {
            std::borrow::Cow::Borrowed(bytes)
        };
        let bytes = self.without_mark(&bytes);
        let at = if pinned {
            line_ends_start(bytes)
        } else {
            bytes.len()
        };
        self.feed(&bytes[..at]);
        if at < bytes.len() {
            self.pending_hold = bytes[at..].to_vec();
            self.echo_held = true;
        }
    }

    /// Your echo `bytes` without the grey mark Mark your commands draws
    /// first, when the row it lands on already ends in `>`. Anything else
    /// comes back as it is. The page's twin is `withoutMark` in
    /// terminalRegion.ts.
    fn without_mark<'a>(&self, bytes: &'a [u8]) -> &'a [u8] {
        match bytes.strip_prefix(crate::input::ECHO_CARET.as_bytes()) {
            Some(rest) if self.ends_in_prompt() => rest,
            _ => bytes,
        }
    }

    /// The row the cursor sits on already asks for your input, as a
    /// game's prompt such as `Account name> ` does: what it shows before
    /// the cursor ends in `>` once trailing blanks go. A cursor held past
    /// the last column writes on the next row, which holds nothing yet.
    /// The same rule as `endsInPrompt` in terminalRegion.ts.
    fn ends_in_prompt(&self) -> bool {
        let cursor = &self.term.grid().cursor;
        if cursor.input_needs_wrap {
            return false;
        }
        let row = &self.term.grid()[cursor.point.line];
        let before: String = (0..cursor.point.column.0)
            .map(|col| &row[Column(col)])
            .filter(|cell| !cell.flags.contains(Flags::WIDE_CHAR_SPACER))
            .map(|cell| if cell.c == '\0' { ' ' } else { cell.c })
            .collect();
        before.trim_end().ends_with('>')
    }

    /// `text` as it lands at the cursor: without the line end that would
    /// end the row a pinned prompt left, while that row is open.
    fn land(&mut self, text: String) -> String {
        if !self.pin_row {
            return text;
        }
        let (rest, closed) = close_pin_row(text.as_bytes());
        self.pin_row = !closed;
        match rest {
            std::borrow::Cow::Borrowed(_) => text,
            // Only a line end and a carriage return went, so it is text.
            std::borrow::Cow::Owned(rest) => String::from_utf8(rest).unwrap_or_default(),
        }
    }

    /// Write the line ends the session held back, which closes the open
    /// region, since they come after it.
    fn write_hold(&mut self) {
        if self.pending_hold.is_empty() {
            return;
        }
        self.echo_held = false;
        let hold = std::mem::take(&mut self.pending_hold);
        self.region = None;
        self.feed(&hold);
    }

    /// The line ends held back now, for a test.
    #[cfg(test)]
    pub(crate) fn pending_hold(&self) -> &[u8] {
        &self.pending_hold
    }

    /// The cursor's row and column, for a test.
    #[cfg(test)]
    pub(crate) fn cursor(&self) -> (i32, usize) {
        let point = self.term.grid().cursor.point;
        (point.line.0, point.column.0)
    }

    /// Put the live render back over the open region when it holds one.
    /// The write that follows closes the region.
    fn restore_first(&mut self) {
        let Some(region) = self.region.as_mut() else {
            return;
        };
        let Some(restore) = region.restore.take() else {
            return;
        };
        let gen = region.gen;
        if let Some(to_start) = self.locate(gen) {
            let text = self.wrap(&String::from_utf8_lossy(&restore));
            self.region = None;
            self.feed(&to_start);
            self.feed_marked(text.as_bytes());
        }
    }

    /// The bytes that move the cursor to the start of open region `gen`
    /// and erase from there to the end of the screen. Empty when the
    /// region wrote nothing yet. None when `gen` is not open, or its
    /// start has scrolled above the screen.
    fn locate(&self, gen: u64) -> Option<Vec<u8>> {
        let region = self.region.as_ref().filter(|r| r.gen == gen)?;
        match region_extent(self.columns(), region)? {
            Extent::Nothing => Some(Vec::new()),
            Extent::Rows { above, col } => {
                let cursor = self.term.grid().cursor.point.line.0;
                if cursor < 0 || above > cursor as usize {
                    return None;
                }
                Some(erase_back(above, col))
            }
        }
    }

    /// The bytes that move the cursor to the first row of the lines
    /// `plain` holds and erase from there to the end of the screen, when
    /// those lines sit on the screen right above open region `gen`. None
    /// when they do not.
    fn locate_above(&self, gen: u64, plain: &str) -> Option<Vec<u8>> {
        let region = self.region.as_ref().filter(|r| r.gen == gen)?;
        let cursor = self.term.grid().cursor.point.line.0;
        let start = match region_extent(self.columns(), region)? {
            Extent::Nothing if !region.wrap_pending && region.col == 0 => cursor,
            Extent::Rows { above, col: 0 } => cursor - i32::try_from(above).ok()?,
            _ => return None,
        };
        let want: usize = plain.chars().filter(|c| !c.is_whitespace()).count();
        let mut rows: Vec<String> = Vec::new();
        let mut line = start - 1;
        while line >= 0 && start - line <= ABOVE_ROWS {
            let row = self.row_string(usize::try_from(line).ok()?);
            rows.insert(0, row.replace('\0', " "));
            if vosh_prompt::stage::shows_lines(&rows, plain) {
                let up = usize::try_from(cursor - line).ok()?;
                return Some(erase_back(up, 0));
            }
            let got: usize = rows
                .iter()
                .flat_map(|r| r.chars())
                .filter(|c| !c.is_whitespace())
                .count();
            if got > want {
                return None;
            }
            line -= 1;
        }
        None
    }

    /// Where the cursor sits and where the open region starts, the start
    /// counted back from the cursor through the region's bytes at the
    /// grid's width now, as a replace counts it.
    pub(crate) fn cursor_report(&self) -> CursorReport {
        let grid = self.term.grid();
        let cursor = &grid.cursor;
        let (line, col) = (cursor.point.line.0, cursor.point.column.0);
        let region = self.region.as_ref().and_then(|region| {
            let (start, at) = match region_extent(self.columns(), region)? {
                // Nothing moved the cursor on from the mark, so the region
                // starts where the next character lands.
                Extent::Nothing if region.wrap_pending => (line + 1, 0),
                Extent::Nothing => (line, region.col),
                Extent::Rows { above, col } => (line - i32::try_from(above).ok()?, col),
            };
            Some(RegionStart {
                gen: region.gen,
                line: start,
                col: at,
            })
        });
        CursorReport {
            line,
            col,
            at_bottom: grid.display_offset() == 0,
            cols: self.columns(),
            region,
        }
    }

    /// The live screen as text, as [`ScreenRows`] says.
    pub(crate) fn screen_rows(&self) -> ScreenRows {
        let grid = self.term.grid();
        let rows = (0..self.size.screen_lines)
            .map(|line| {
                let row = &grid[Line(line as i32)];
                let text: String = (0..self.size.columns)
                    .map(|c| &row[Column(c)])
                    .filter(|cell| !cell.flags.contains(Flags::WIDE_CHAR_SPACER))
                    .map(|cell| if cell.c == '\0' { ' ' } else { cell.c })
                    .collect();
                text.trim_end().to_string()
            })
            .collect();
        ScreenRows {
            rows,
            cols: self.columns(),
            at_bottom: grid.display_offset() == 0,
        }
    }

    /// The cursor sits at the start of a row with nothing held.
    fn at_row_start(&self) -> bool {
        let cursor = &self.term.grid().cursor;
        cursor.point.column.0 == 0 && !cursor.input_needs_wrap
    }

    /// `bytes` as text, holding back a character split at its end.
    fn decode(&mut self, bytes: &[u8]) -> String {
        self.pending_utf8.extend_from_slice(bytes);
        match std::str::from_utf8(&self.pending_utf8) {
            Ok(s) => {
                let s = s.to_string();
                self.pending_utf8.clear();
                s
            }
            Err(e) => {
                let valid = e.valid_up_to();
                let s = String::from_utf8_lossy(&self.pending_utf8[..valid]).into_owned();
                // Keep at most one code point of tail. Longer garbage is
                // not a split character, so let it through lossily.
                if self.pending_utf8.len() - valid <= 3 {
                    self.pending_utf8.drain(..valid);
                } else {
                    self.pending_utf8.clear();
                }
                s
            }
        }
    }

    /// Word wrap `text` at the grid width. xterm receives the same
    /// stream word wrapped by the webview's `WordWrapper`
    /// (src/terminal/wordWrap.ts). Without it the grid would break mid word
    /// at its edge and the two renderers would disagree. Both run
    /// `vosh_prompt::wrap` against one fixture.
    fn wrap(&self, text: &str) -> String {
        vosh_prompt::wrap::wrap_stream(text, self.columns())
    }

    /// Feed `bytes`, taking each mark out. A region mark notes where its
    /// region starts, so the bytes after it are that region's. A lift
    /// start notes where its lift starts, and a lift end tags the lift's
    /// cells.
    fn feed_marked(&mut self, bytes: &[u8]) {
        let mut rest = bytes;
        while let Some((start, end, mark)) = find_mark(rest) {
            self.feed_region(&rest[..start]);
            let cursor = &self.term.grid().cursor;
            let (col, wrap_pending) = (cursor.point.column.0, cursor.input_needs_wrap);
            match mark {
                Mark::Region(gen) => {
                    self.region = Some(Region {
                        gen,
                        col,
                        wrap_pending,
                        bytes: Vec::new(),
                        restore: None,
                    });
                }
                Mark::LiftStart(id) => {
                    self.lift_tracks.retain(|t| t.id != id);
                    self.lift_tracks.push(LiftTrack {
                        id,
                        col,
                        wrap_pending,
                        bytes: Vec::new(),
                    });
                }
                Mark::LiftEnd(id) => self.end_lift(id),
            }
            rest = &rest[end..];
        }
        self.feed_region(rest);
    }

    /// Feed `bytes`, which belong to the open region when there is one,
    /// and to every lift whose start this output fed.
    fn feed_region(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.feed(bytes);
        if let Some(region) = self.region.as_mut() {
            region.bytes.extend_from_slice(bytes);
        }
        for track in &mut self.lift_tracks {
            track.bytes.extend_from_slice(bytes);
        }
    }

    /// Lift `id` ends at the cursor. Tag every cell from its start to the
    /// cursor. The start is counted back from the cursor through the bytes
    /// fed since the start mark, laid out again at the grid's width, the
    /// way a region's is, so a screen that scrolled between the two marks
    /// never moves it. A repaint's end mark comes with no start in its
    /// output, and the lines above the region were tagged by the first
    /// end mark, so it tags from the open region's start.
    fn end_lift(&mut self, id: u64) {
        let (col, wrap_pending, bytes) = match self.lift_tracks.iter().position(|t| t.id == id) {
            Some(i) => {
                let track = self.lift_tracks.remove(i);
                (track.col, track.wrap_pending, track.bytes)
            }
            None => match &self.region {
                Some(region) => (region.col, region.wrap_pending, region.bytes.clone()),
                None => return,
            },
        };
        let laid = Region {
            gen: 0,
            col,
            wrap_pending,
            bytes,
            restore: None,
        };
        let Some(Extent::Rows { above, col: first }) = region_extent(self.columns(), &laid) else {
            return;
        };
        let cursor = self.term.grid().cursor.clone();
        // A cursor held past the last column sits on a filled cell.
        let end = cursor.point.column.0 + usize::from(cursor.input_needs_wrap);
        let last = cursor.point.line.0;
        let top = last - i32::try_from(above).unwrap_or(i32::MAX);
        self.tag_lift(id, top, first, last, end);
    }

    /// Tag the cells of lift `id` from line `top` at column `first` to
    /// line `last` before column `end`, with one shared hyperlink extra.
    fn tag_lift(&mut self, id: u64, top: i32, first: usize, last: i32, end: usize) {
        let link = Hyperlink::new(Some(format!("vosh-lift-{id}")), LIFT_URI.to_string());
        let mut template = Cell::default();
        template.set_hyperlink(Some(link.clone()));
        let shared = template.extra;
        let cols = self.columns();
        let grid = self.term.grid_mut();
        let topmost = grid.topmost_line().0;
        let bottommost = grid.bottommost_line().0;
        for line in top.max(topmost)..=last.min(bottommost) {
            let from = if line == top { first } else { 0 };
            let to = if line == last { end.min(cols) } else { cols };
            let row = &mut grid[Line(line)];
            for col in from..to {
                let cell = &mut row[Column(col)];
                if cell.extra.is_none() {
                    cell.extra.clone_from(&shared);
                } else {
                    cell.set_hyperlink(Some(link.clone()));
                }
            }
        }
    }

    /// The rows of every lift between grid lines `from` and `to`, each
    /// with its first tagged column and one past its last tagged glyph.
    /// Empty rows of a lift are left out.
    pub(crate) fn lift_spans(&self, from: i32, to: i32) -> Vec<LiftSpan> {
        let grid = self.term.grid();
        let cols = grid.columns();
        let mut spans: Vec<LiftSpan> = Vec::new();
        for line in from.max(grid.topmost_line().0)..=to.min(grid.bottommost_line().0) {
            let row = &grid[Line(line)];
            let start = spans.len();
            for col in 0..cols {
                let cell = &row[Column(col)];
                let Some(id) = lift_of(cell) else {
                    continue;
                };
                let shows = shows_glyph(cell);
                let width = if cell.flags.contains(Flags::WIDE_CHAR) {
                    2
                } else {
                    1
                };
                match spans[start..].iter_mut().find(|s| s.id == id) {
                    Some(span) => {
                        if shows {
                            span.end = span.end.max(col + width);
                        }
                    }
                    None => spans.push(LiftSpan {
                        id,
                        line,
                        first: col,
                        end: if shows { col + width } else { col },
                        after: false,
                    }),
                }
            }
            // Anything outside a lift that shows after it on the row.
            for span in &mut spans[start..] {
                span.after = (span.end..cols).any(|col| {
                    let cell = &row[Column(col)];
                    shows_glyph(cell) && lift_of(cell) != Some(span.id)
                });
            }
        }
        spans.retain(|s| s.end > s.first);
        spans
    }

    /// The rows from the top of the screen to the cursor, when every row
    /// below the cursor is empty. None when there are none below, when a
    /// row below holds anything, when you scrolled back, or on the
    /// alternate screen, which does not reflow.
    pub(super) fn rows_to_cursor(&self) -> Option<usize> {
        if self.term.mode().contains(TermMode::ALT_SCREEN) {
            return None;
        }
        let grid = self.term.grid();
        if grid.display_offset() != 0 {
            return None;
        }
        let used = usize::try_from(grid.cursor.point.line.0).ok()? + 1;
        let lines = self.size.screen_lines;
        let empty_below = (used..lines).all(|line| grid[Line(line as i32)].is_clear());
        (used < lines && empty_below).then_some(used)
    }
}

/// The lift a cell of a lifted prompt belongs to, from its tag.
fn lift_of(cell: &Cell) -> Option<u64> {
    cell.extra.as_ref()?;
    let link = cell.hyperlink()?;
    if link.uri() != LIFT_URI {
        return None;
    }
    link.id().strip_prefix("vosh-lift-")?.parse().ok()
}

/// The cell shows a glyph: not a blank and not the spacer after a wide
/// character.
fn shows_glyph(cell: &Cell) -> bool {
    cell.c != ' ' && cell.c != '\0' && !cell.flags.contains(Flags::WIDE_CHAR_SPACER)
}

/// Where a region starts, counted back from the cursor at its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Extent {
    /// The region wrote nothing that moved the cursor on from its mark,
    /// so a replace writes where the cursor is.
    Nothing,
    /// The region starts `above` rows over the cursor, at `col`.
    Rows { above: usize, col: usize },
}

/// The most rows a region's layout is worked out for. A region taller
/// than any screen has its start above it anyway.
const EXTENT_ROWS: usize = 1024;

/// The most rows the lines above a region are looked for in.
const ABOVE_ROWS: i32 = 64;

/// Lay `region` out again at `columns` wide, the way this grid laid it
/// out, and say where it starts. The count comes from the region's own
/// bytes, so it holds at the grid's width now, after a resize too. The
/// grid reflows rows it wrapped itself, and the word wrap's line ends
/// are hard, so the region takes the rows a fresh layout gives it. None
/// when the region is taller than [`EXTENT_ROWS`].
fn region_extent(columns: usize, region: &Region) -> Option<Extent> {
    if region.bytes.is_empty() {
        return Some(Extent::Nothing);
    }
    let columns = columns.max(1);
    // Each hard row, plus a row for every full width of bytes, which
    // counts escape codes too, so it never falls short.
    let hard_rows = region.bytes.split(|&b| b == b'\n').count();
    let rows = hard_rows + (region.col + region.bytes.len()) / columns + 2;
    if rows > EXTENT_ROWS {
        return None;
    }
    let size = GridSize {
        columns,
        screen_lines: rows,
    };
    let config = Config {
        scrolling_history: 0,
        ..Config::default()
    };
    let mut term = Term::new(config, &size, NoopListener);
    let mut parser: Processor = Processor::new();
    let mut feed = |bytes: &[u8]| {
        for &byte in bytes {
            parser.advance(&mut term, byte);
        }
    };
    // Put the cursor where the mark came, held past the last column when
    // it was there, by writing the last cell.
    if region.wrap_pending {
        feed(format!("\x1b[{columns}Gx").as_bytes());
    } else {
        feed(format!("\x1b[{}G", region.col + 1).as_bytes());
    }
    feed(&region.bytes);
    let end = term.grid().cursor.point.line.0.max(0) as usize;
    let (start, col) = if region.wrap_pending {
        (1, 0)
    } else {
        (0, region.col)
    };
    Some(if end < start {
        Extent::Nothing
    } else {
        Extent::Rows {
            above: end - start,
            col,
        }
    })
}

/// Move the cursor from the end of a region to its start, `above` rows up
/// at `col`, then erase to the end of the screen. Nothing is written after
/// an open region, so the erase clears its old render and nothing else,
/// however many rows it took. The erase fills with the default background.
/// The cells it clears take the background in force, and a line the region
/// ends on can leave its own on while the line ends after it wait, so
/// without it the rows below would take that color too. What the replace
/// writes sets its own.
pub(super) fn erase_back(above: usize, col: usize) -> Vec<u8> {
    let mut out = b"\r".to_vec();
    if above > 0 {
        out.extend(format!("\x1b[{above}A").into_bytes());
    }
    if col > 0 {
        out.extend(format!("\x1b[{col}C").into_bytes());
    }
    out.extend_from_slice(b"\x1b[49m\x1b[0J");
    out
}

/// A private mark the session writes, where a region it may replace starts
/// or where a lifted prompt starts or ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mark {
    /// `o;G`, where region G starts.
    Region(u64),
    /// `l;L`, where lift L starts.
    LiftStart(u64),
    /// `e;L`, where lift L ends.
    LiftEnd(u64),
}

/// The first mark in `bytes`: where it starts, where it ends and which
/// it is.
pub(super) fn find_mark(bytes: &[u8]) -> Option<(usize, usize, Mark)> {
    let prefix = format!("\x1b]{MARK_OSC};");
    let prefix = prefix.as_bytes();
    let mut from = 0;
    while let Some(at) = bytes[from..]
        .windows(prefix.len())
        .position(|w| w == prefix)
        .map(|i| from + i)
    {
        let body = &bytes[at + prefix.len()..];
        let kind = body.first().copied();
        if matches!(kind, Some(b'o' | b'l' | b'e')) && body.get(1) == Some(&b';') {
            let digits = &body[2..];
            let len = digits.iter().take_while(|b| b.is_ascii_digit()).count();
            if len > 0 && digits.get(len) == Some(&0x07) {
                let number = std::str::from_utf8(&digits[..len])
                    .ok()
                    .and_then(|s| s.parse().ok());
                if let Some(n) = number {
                    let mark = match kind {
                        Some(b'o') => Mark::Region(n),
                        Some(b'l') => Mark::LiftStart(n),
                        _ => Mark::LiftEnd(n),
                    };
                    return Some((at, at + prefix.len() + 2 + len + 1, mark));
                }
            }
        }
        from = at + 1;
    }
    None
}

/// Where the run of `\r` and `\n` that `bytes` ends on starts, when it
/// holds a line end. `bytes.len()` when there is none.
fn line_ends_start(bytes: &[u8]) -> usize {
    let at = bytes
        .iter()
        .rposition(|&b| b != b'\r' && b != b'\n')
        .map_or(0, |i| i + 1);
    if bytes[at..].contains(&b'\n') {
        at
    } else {
        bytes.len()
    }
}

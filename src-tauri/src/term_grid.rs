//! Tier 3 native terminal renderer, M2b (see docs/native-renderer.md).
//!
//! Wraps `alacritty_terminal`'s `Term` so the post-telnet byte stream
//! (the same bytes Vosh hands xterm) builds a real cell grid: characters,
//! colors, styles, cursor, and scrollback, with all the VT escape-code
//! semantics handled by Alacritty's parser. M2c's wgpu renderer walks
//! this grid and draws each cell.
//!
//! macOS only for now (the renderer that consumes it is). The grid model
//! itself is platform independent and ungates when other platforms land.

#![cfg(native_surface)]
// The pointer-driven grid helpers (selection, URL lookup, wheel scroll)
// are only called from the mouse-capable surfaces; the Linux surface is
// display-only for now, so they sit unused there.
#![cfg_attr(target_os = "linux", allow(dead_code))]

use std::sync::{Mutex, OnceLock};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::{Cell, Flags, Hyperlink};
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor};
use regex::RegexBuilder;
use vosh_prompt::stage::{close_pin_row, Output, MARK_OSC};

/// Render-relevant cell attributes, decoupled from alacritty's `Flags`.
#[derive(Clone, Copy, Default)]
pub(crate) struct CellFlags {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub inverse: bool,
    pub underline: bool,
    pub strikeout: bool,
}

/// `Term` requires an event listener for bell, title, clipboard, and
/// similar callbacks. The renderer only reads the grid, so every event
/// is dropped.
struct NoopListener;
impl EventListener for NoopListener {
    fn send_event(&self, _event: Event) {}
}

/// Screen geometry handed to `Term::new`. Alacritty grows its own
/// scrollback as rows scroll off the top, so no preset history here.
#[derive(Clone, Copy)]
struct GridSize {
    columns: usize,
    screen_lines: usize,
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.screen_lines
    }
    fn screen_lines(&self) -> usize {
        self.screen_lines
    }
    fn columns(&self) -> usize {
        self.columns
    }
}

pub(crate) struct TermGrid {
    term: Term<NoopListener>,
    parser: Processor,
    // Read by the deferred cell accessors (see the impl note below).
    #[allow(dead_code)]
    size: GridSize,
    /// The region the last session write left open, if any (D22).
    region: Option<Region>,
    /// The start of a character the last session write split, held
    /// until the rest arrives so it decodes whole before wrapping.
    pending_utf8: Vec<u8>,
    /// Line ends the session asked the grid to keep back, written before
    /// the next write lands, while your prompt shows pinned.
    pending_hold: Vec<u8>,
    /// Lifts whose start mark this output fed, with what it fed since.
    lift_tracks: Vec<LiftTrack>,
    /// The row a pinned prompt left is where the next write lands, so the
    /// line end that would end it writes nothing.
    pin_row: bool,
    /// The newest output of the prompt stage the grid took, by its id
    /// (`Output::id`), 0 before the first. Text the webview writes lands
    /// after it, which the session reads to tell what the text follows.
    taken: u64,
}

/// A lift the grid saw the start mark of, while your prompt shows lifted:
/// where the mark came, as a region's does, and the bytes fed since, so
/// its end mark can count the rows back to it at the grid's width, however
/// far the screen scrolled between the two.
#[derive(Debug, Clone)]
struct LiftTrack {
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
/// `terminal_cursor` (section 6). Lines count from the top of the live
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
/// grid holds it (D22). It starts at a mark `ESC ] 7717 ; o ; G BEL` and
/// stays open while nothing else is written after it. The grid keeps
/// the bytes it wrote after the mark, so it can count the rows they take
/// at the width it has when a replace comes, a resize included.
#[derive(Debug, Clone)]
struct Region {
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

// The read side (size + cell accessors) is the grid API the M2c wgpu
// renderer will consume; for now it is exercised only by the unit tests,
// so allow it to sit unused in the lib build until the renderer lands.
#[allow(dead_code)]
impl TermGrid {
    pub(crate) fn new(columns: usize, screen_lines: usize) -> Self {
        let size = GridSize {
            columns: columns.max(1),
            screen_lines: screen_lines.max(1),
        };
        let term = Term::new(Config::default(), &size, NoopListener);
        Self {
            term,
            parser: Processor::new(),
            size,
            region: None,
            pending_utf8: Vec::new(),
            pending_hold: Vec::new(),
            lift_tracks: Vec::new(),
            pin_row: false,
            taken: 0,
        }
    }

    /// Note that the grid took output `id` of the prompt stage.
    pub(crate) fn took(&mut self, id: u64) {
        self.taken = self.taken.max(id);
    }

    /// The newest output of the prompt stage the grid took, 0 before the
    /// first.
    pub(crate) fn taken(&self) -> u64 {
        self.taken
    }

    /// Write one session output, as `session://output` carries it to
    /// xterm: its replace, then its bytes, then its restore (D22).
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
            } else if replace.fresh && !replace.bytes.is_empty() {
                self.restore_first();
                self.write_hold();
                self.region = None;
                if !self.at_row_start() {
                    self.feed(b"\r\n");
                }
                let text = self.land(text);
                self.feed_marked(text.as_bytes());
                wrote = true;
            }
        }
        if !out.bytes.is_empty() {
            self.restore_first();
            self.write_hold();
            self.region = None;
            let text = self.decode(&out.bytes);
            let text = self.land(text);
            if !text.is_empty() {
                let text = self.wrap(&text);
                self.feed_marked(text.as_bytes());
            }
            wrote = true;
        }
        if wrote || out.hold.len() > self.pending_hold.len() {
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
    /// out before it.
    pub(crate) fn local_write(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
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
        self.feed(&bytes);
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
    /// (src/lib/wordWrap.ts). Without it the grid would break mid word
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

    /// Advance the VT parser over a chunk of post-telnet bytes. vte
    /// 0.13's `advance` is byte-at-a-time.
    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.parser.advance(&mut self.term, byte);
        }
    }

    pub(crate) fn columns(&self) -> usize {
        self.size.columns
    }

    pub(crate) fn screen_lines(&self) -> usize {
        self.size.screen_lines
    }

    /// The character at a visible-screen cell (line 0 = top row).
    pub(crate) fn char_at(&self, line: usize, col: usize) -> char {
        self.term.grid()[Line(line as i32)][Column(col)].c
    }

    /// The visible row as a string (trailing blanks included).
    pub(crate) fn row_string(&self, line: usize) -> String {
        let grid = self.term.grid();
        (0..self.size.columns)
            .map(|c| grid[Line(line as i32)][Column(c)].c)
            .collect()
    }

    /// A cell's character and fg/bg colors at a visible row, accounting for
    /// the scrollback display offset (scrollback lives at negative lines).
    /// Out-of-range rows (scrolled past the top) read as blank.
    pub(crate) fn cell(&self, line: usize, col: usize) -> (char, Color, Color) {
        let grid = self.term.grid();
        let target = Line(line as i32 - grid.display_offset() as i32);
        if target < grid.topmost_line() || target > grid.bottommost_line() {
            return (
                ' ',
                Color::Named(NamedColor::Foreground),
                Color::Named(NamedColor::Background),
            );
        }
        let cell = &grid[target][Column(col)];
        (cell.c, cell.fg, cell.bg)
    }

    /// A cell at an explicit grid line (0 = top of the live screen,
    /// negatives are scrollback). Out-of-range lines read as blank. Lets
    /// the split renderer read the top and bottom regions at different
    /// offsets.
    pub(crate) fn cell_at_line(
        &self,
        grid_line: i32,
        col: usize,
    ) -> (char, Color, Color, CellFlags) {
        let grid = self.term.grid();
        let target = Line(grid_line);
        if target < grid.topmost_line() || target > grid.bottommost_line() {
            return (
                ' ',
                Color::Named(NamedColor::Foreground),
                Color::Named(NamedColor::Background),
                CellFlags::default(),
            );
        }
        let cell = &grid[target][Column(col)];
        let flags = cell.flags;
        let cell_flags = CellFlags {
            bold: flags.contains(Flags::BOLD),
            dim: flags.contains(Flags::DIM),
            italic: flags.contains(Flags::ITALIC),
            inverse: flags.contains(Flags::INVERSE),
            underline: flags.intersects(Flags::ALL_UNDERLINES),
            strikeout: flags.contains(Flags::STRIKEOUT),
        };
        (cell.c, cell.fg, cell.bg, cell_flags)
    }

    /// Current scrollback display offset (0 = live tail).
    pub(crate) fn display_offset(&self) -> usize {
        self.term.grid().display_offset()
    }

    /// Total scrollback length (lines above the live screen). The max the
    /// display offset can reach; drives the scroll-depth indicator.
    pub(crate) fn scrollback_len(&self) -> usize {
        let grid = self.term.grid();
        grid.total_lines().saturating_sub(grid.screen_lines())
    }

    /// The active selection as start and end line/column in grid
    /// coordinates (line-major, inclusive), for highlighting.
    pub(crate) fn selection_bounds(&self) -> Option<(i32, usize, i32, usize)> {
        let range = self.term.selection.as_ref()?.to_range(&self.term)?;
        Some((
            range.start.line.0,
            range.start.column.0,
            range.end.line.0,
            range.end.column.0,
        ))
    }

    /// Select every line the grid holds, scrollback included, from the
    /// first cell of the oldest line to the last cell of the live screen.
    pub(crate) fn select_all(&mut self) {
        let grid = self.term.grid();
        let start = Point::new(grid.topmost_line(), Column(0));
        let end = Point::new(grid.bottommost_line(), grid.last_column());
        let mut selection = Selection::new(SelectionType::Simple, start, Side::Left);
        selection.update(end, Side::Right);
        self.term.selection = Some(selection);
    }

    /// Scroll the display by `delta` lines (positive scrolls up into
    /// scrollback, clamped to history).
    pub(crate) fn scroll(&mut self, delta: i32) {
        self.term.scroll_display(Scroll::Delta(delta));
    }

    /// Resize the grid to fit the surface; reflows existing content.
    ///
    /// Narrower, alacritty keeps the cursor on its row and pushes the rows
    /// the reflow adds over it into history, even with empty rows below
    /// the cursor. So the grid first takes the empty rows off, narrows,
    /// and then grows back, which pulls those rows out of history again.
    /// A nearly empty screen keeps what it shows, as xterm does, and a
    /// region near its top stays within reach of a replace (D22).
    pub(crate) fn resize(&mut self, columns: usize, screen_lines: usize) {
        let columns = columns.max(1);
        let screen_lines = screen_lines.max(1);
        if columns == self.size.columns && screen_lines == self.size.screen_lines {
            return;
        }
        if columns < self.size.columns {
            if let Some(used) = self.rows_to_cursor() {
                self.term.resize(GridSize {
                    columns: self.size.columns,
                    screen_lines: used,
                });
                self.term.resize(GridSize {
                    columns,
                    screen_lines: used,
                });
            }
        }
        self.size = GridSize {
            columns,
            screen_lines,
        };
        self.term.resize(self.size);
    }

    /// The rows from the top of the screen to the cursor, when every row
    /// below the cursor is empty. None when there are none below, when a
    /// row below holds anything, when you scrolled back, or on the
    /// alternate screen, which does not reflow.
    fn rows_to_cursor(&self) -> Option<usize> {
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

/// Move the cursor from the end of a region to its start, `above` rows
/// up at `col`, then erase to the end of the screen (D22 rule c).
fn erase_back(above: usize, col: usize) -> Vec<u8> {
    let mut out = b"\r".to_vec();
    if above > 0 {
        out.extend(format!("\x1b[{above}A").into_bytes());
    }
    if col > 0 {
        out.extend(format!("\x1b[{col}C").into_bytes());
    }
    out.extend_from_slice(b"\x1b[0J");
    out
}

/// A private mark the session writes (D22, and Where your prompt shows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mark {
    /// `o;G`, where region G starts.
    Region(u64),
    /// `l;L`, where lift L starts.
    LiftStart(u64),
    /// `e;L`, where lift L ends.
    LiftEnd(u64),
}

/// The first mark in `bytes`: where it starts, where it ends and which
/// it is.
fn find_mark(bytes: &[u8]) -> Option<(usize, usize, Mark)> {
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

static GRID: OnceLock<Mutex<Option<TermGrid>>> = OnceLock::new();

fn grid_slot() -> &'static Mutex<Option<TermGrid>> {
    GRID.get_or_init(|| Mutex::new(None))
}

/// Resize the shared grid to fit the native surface (creating it if it does
/// not exist yet). Called by the renderer before each frame.
pub(crate) fn resize_grid(columns: usize, screen_lines: usize) {
    if let Ok(mut slot) = grid_slot().lock() {
        match slot.as_mut() {
            Some(grid) => grid.resize(columns, screen_lines),
            None => *slot = Some(TermGrid::new(columns, screen_lines)),
        }
    }
}

static SEEDED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Claim the one seeding of the shared grid from the persisted
/// scrollback. The grid lives as long as the process, so a webview
/// reload or a remounted terminal asking again would write the history
/// a second time over a grid that already holds it (and glue the last
/// prompt to the first restored line). True on the first call only.
pub(crate) fn claim_seed() -> bool {
    !SEEDED.swap(true, std::sync::atomic::Ordering::AcqRel)
}

/// Write text the webview wrote itself into the shared grid, creating it
/// on first use: your typed echo, a notice, the restored scrollback. See
/// [`TermGrid::local_write`]. Lock guarded, and the renderer reads the
/// same grid. Returns the newest output of the prompt stage the grid took
/// before the text, which the text follows.
pub(crate) fn feed_local(bytes: &[u8]) -> u64 {
    let Ok(mut slot) = grid_slot().lock() else {
        return 0;
    };
    let grid = slot.get_or_insert_with(|| TermGrid::new(80, 24));
    grid.local_write(bytes);
    grid.taken()
}

/// Write one session output into the shared grid under its lock, word
/// wrapped at the grid width, with its replace and restore. See
/// [`TermGrid::session_output`]. Every `session://output` goes through
/// here as well, so the grid holds what xterm holds. `id` names the
/// output when the prompt stage made it.
pub(crate) fn feed_session_output(out: &Output, id: Option<u64>) {
    let Ok(mut slot) = grid_slot().lock() else {
        return;
    };
    let grid = slot.get_or_insert_with(|| TermGrid::new(80, 24));
    grid.session_output(out);
    if let Some(id) = id {
        grid.took(id);
    }
}

/// Where the shared grid's cursor sits and where its open region starts,
/// under its lock. None before the grid exists. See
/// [`TermGrid::cursor_report`].
pub(crate) fn cursor_report() -> Option<CursorReport> {
    grid_slot()
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().map(TermGrid::cursor_report))
}

/// The shared grid's live screen as text, under its lock. None before
/// the grid exists. See [`TermGrid::screen_rows`].
pub(crate) fn screen_rows() -> Option<ScreenRows> {
    grid_slot()
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().map(TermGrid::screen_rows))
}

/// Current (display offset, scrollback length) of the shared grid, for the
/// scrollbar thumb geometry and drag mapping.
pub(crate) fn scroll_metrics() -> (usize, usize) {
    grid_slot().lock().map_or((0, 0), |slot| {
        slot.as_ref()
            .map_or((0, 0), |g| (g.display_offset(), g.scrollback_len()))
    })
}

/// Scroll the shared grid to an absolute display offset (0 = live tail),
/// clamped to history. Drives the scrollbar thumb drag.
pub(crate) fn scroll_to_offset(target: usize) {
    if let Ok(mut slot) = grid_slot().lock() {
        if let Some(grid) = slot.as_mut() {
            let target = target.min(grid.scrollback_len()) as i32;
            let current = grid.display_offset() as i32;
            grid.scroll(target - current);
        }
    }
}

/// Scroll the shared grid by `delta` lines (positive = up into scrollback).
pub(crate) fn scroll(delta: i32) {
    if let Ok(mut slot) = grid_slot().lock() {
        if let Some(grid) = slot.as_mut() {
            grid.scroll(delta);
        }
    }
}

/// Page the shared grid up or down (PageUp/PageDown).
pub(crate) fn scroll_page(up: bool) {
    if let Ok(mut slot) = grid_slot().lock() {
        if let Some(grid) = slot.as_mut() {
            grid.term
                .scroll_display(if up { Scroll::PageUp } else { Scroll::PageDown });
        }
    }
}

/// Snap the shared grid to the live tail (collapses the split).
pub(crate) fn scroll_to_bottom() {
    if let Ok(mut slot) = grid_slot().lock() {
        if let Some(grid) = slot.as_mut() {
            grid.term.scroll_display(Scroll::Bottom);
        }
    }
}

/// Compiled URL matcher, built once.
fn url_regex() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"https?://[^\s<>()\[\]]+").expect("valid url regex"))
}

/// The URL spanning the cell at (`grid_line`, `col`) as (url, start column,
/// end column), if any. Trailing sentence punctuation is trimmed. Used by
/// Cmd+click and the hover underline on the surface.
pub(crate) fn url_at(grid_line: i32, col: usize) -> Option<(String, usize, usize)> {
    let slot = grid_slot().lock().ok()?;
    let g = slot.as_ref()?;
    let grid = g.term.grid();
    if Line(grid_line) < grid.topmost_line() || Line(grid_line) > grid.bottommost_line() {
        return None;
    }
    let cols = grid.columns();
    let text: String = (0..cols)
        .map(|c| grid[Line(grid_line)][Column(c)].c)
        .collect();
    url_in_line(&text, col)
}

/// The URL spanning char index `col` in a grid line's text. `text` holds one
/// char per grid column (wide-char spacer cells read as a space), so char
/// offsets are grid columns even with double-width glyphs on the line.
fn url_in_line(text: &str, col: usize) -> Option<(String, usize, usize)> {
    for m in url_regex().find_iter(text) {
        let start_col = text[..m.start()].chars().count();
        let end_col = text[..m.end()].chars().count();
        if col >= start_col && col < end_col {
            let url = m
                .as_str()
                .trim_end_matches(['.', ',', ')', ']', '!', '?'])
                .to_string();
            let trimmed_end = start_col + url.chars().count();
            return Some((url, start_col, trimmed_end));
        }
    }
    None
}

/// Current scrollback offset of the shared grid (0 = live tail, no split).
pub(crate) fn current_display_offset() -> usize {
    grid_slot()
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().map(TermGrid::display_offset))
        .unwrap_or(0)
}

/// Begin a text selection anchored at a grid cell.
pub(crate) fn start_selection(line: i32, col: usize) {
    if let Ok(mut slot) = grid_slot().lock() {
        if let Some(grid) = slot.as_mut() {
            let point = Point::new(Line(line), Column(col));
            grid.term.selection = Some(Selection::new(SelectionType::Simple, point, Side::Left));
        }
    }
}

/// Extend the active selection to a grid cell.
pub(crate) fn update_selection(line: i32, col: usize) {
    if let Ok(mut slot) = grid_slot().lock() {
        if let Some(grid) = slot.as_mut() {
            if let Some(selection) = grid.term.selection.as_mut() {
                selection.update(Point::new(Line(line), Column(col)), Side::Left);
            }
        }
    }
}

/// Drop the active selection.
pub(crate) fn clear_selection() {
    if let Ok(mut slot) = grid_slot().lock() {
        if let Some(grid) = slot.as_mut() {
            grid.term.selection = None;
        }
    }
}

/// Select everything in the shared grid, scrollback included. Backs the
/// terminal menu's Select all and Cmd+A on an empty command line.
pub(crate) fn select_all() {
    if let Ok(mut slot) = grid_slot().lock() {
        if let Some(grid) = slot.as_mut() {
            grid.select_all();
        }
    }
}

/// The selected text, or None when there is no selection.
pub(crate) fn selection_text() -> Option<String> {
    grid_slot().lock().ok().and_then(|slot| {
        slot.as_ref()
            .and_then(|grid| grid.term.selection_to_string())
    })
}

// Find/search state. Matches are (grid_line, col_start, col_end) in reading
// order (top of scrollback to bottom); active is an index into them. The
// query is remembered so repeated calls with the same query advance the
// active match instead of resetting it.
static FIND_MATCHES: Mutex<Vec<(i32, usize, usize)>> = Mutex::new(Vec::new());
static FIND_ACTIVE: Mutex<usize> = Mutex::new(0);
static FIND_QUERY: Mutex<String> = Mutex::new(String::new());

fn build_find_regex(
    query: &str,
    is_regex: bool,
    case_sensitive: bool,
    whole_word: bool,
) -> Option<regex::Regex> {
    if query.is_empty() {
        return None;
    }
    let mut pattern = if is_regex {
        query.to_string()
    } else {
        regex::escape(query)
    };
    if whole_word {
        pattern = format!(r"\b{pattern}\b");
    }
    RegexBuilder::new(&pattern)
        .case_insensitive(!case_sensitive)
        .build()
        .ok()
}

/// A match location: grid line, start column, end column (character cells).
pub(crate) type FindMatch = (i32, usize, usize);

/// Collect every match of `query` in the grid as line/start/end in reading
/// order. Column indices are character cells.
fn collect_matches(
    grid: &TermGrid,
    query: &str,
    is_regex: bool,
    case_sensitive: bool,
    whole_word: bool,
) -> Vec<(i32, usize, usize)> {
    let Some(re) = build_find_regex(query, is_regex, case_sensitive, whole_word) else {
        return Vec::new();
    };
    let g = grid.term.grid();
    let cols = g.columns();
    let mut matches = Vec::new();
    for line in g.topmost_line().0..=g.bottommost_line().0 {
        let text: String = (0..cols).map(|c| g[Line(line)][Column(c)].c).collect();
        for m in re.find_iter(&text) {
            let start_col = text[..m.start()].chars().count();
            let end_col = text[..m.end()].chars().count();
            if end_col > start_col {
                matches.push((line, start_col, end_col));
            }
        }
    }
    matches
}

/// All matches plus the active match, for the renderer's highlight pass.
pub(crate) fn find_snapshot() -> (Vec<FindMatch>, Option<FindMatch>) {
    let matches = match FIND_MATCHES.lock() {
        Ok(m) => m.clone(),
        Err(_) => Vec::new(),
    };
    let active = FIND_ACTIVE
        .lock()
        .ok()
        .and_then(|i| matches.get(*i).copied());
    (matches, active)
}

/// Run a search and step to the next (or previous) match, scrolling it into
/// view. Returns (current, total) for the toolbar, 1-based; (0, 0) when
/// there is no match.
pub(crate) fn find_run(
    query: &str,
    is_regex: bool,
    case_sensitive: bool,
    whole_word: bool,
    forward: bool,
) -> (usize, usize) {
    let matches = match grid_slot().lock() {
        Ok(slot) => match slot.as_ref() {
            Some(grid) => collect_matches(grid, query, is_regex, case_sensitive, whole_word),
            None => Vec::new(),
        },
        Err(_) => Vec::new(),
    };
    if matches.is_empty() {
        find_clear();
        return (0, 0);
    }
    let total = matches.len();
    let query_changed = FIND_QUERY.lock().map_or(true, |q| *q != query);
    let active = if query_changed {
        if forward {
            0
        } else {
            total - 1
        }
    } else {
        let prev = FIND_ACTIVE.lock().map_or(0, |i| *i).min(total - 1);
        if forward {
            (prev + 1) % total
        } else {
            (prev + total - 1) % total
        }
    };
    let target_line = matches[active].0;
    if let Ok(mut q) = FIND_QUERY.lock() {
        *q = query.to_string();
    }
    if let Ok(mut m) = FIND_MATCHES.lock() {
        *m = matches;
    }
    if let Ok(mut a) = FIND_ACTIVE.lock() {
        *a = active;
    }
    scroll_to_grid_line(target_line);
    (active + 1, total)
}

/// Scroll the display so `line` sits near the middle of the screen.
fn scroll_to_grid_line(line: i32) {
    if let Ok(mut slot) = grid_slot().lock() {
        if let Some(grid) = slot.as_mut() {
            let g = grid.term.grid();
            let screen = g.screen_lines();
            let history = g.total_lines().saturating_sub(screen);
            let target = (screen as i32 / 2 - line).max(0) as usize;
            let target = target.min(history);
            let delta = target as i32 - g.display_offset() as i32;
            if delta != 0 {
                grid.term.scroll_display(Scroll::Delta(delta));
            }
        }
    }
}

/// Clear the find state (matches, active, query).
pub(crate) fn find_clear() {
    if let Ok(mut m) = FIND_MATCHES.lock() {
        m.clear();
    }
    if let Ok(mut a) = FIND_ACTIVE.lock() {
        *a = 0;
    }
    if let Ok(mut q) = FIND_QUERY.lock() {
        q.clear();
    }
}

/// Read the shared grid (None until the first feed). The renderer calls
/// this on the main thread to build a frame.
pub(crate) fn with_grid<R>(f: impl FnOnce(Option<&TermGrid>) -> R) -> R {
    match grid_slot().lock() {
        Ok(slot) => f(slot.as_ref()),
        Err(_) => f(None),
    }
}

/// Held by every test that feeds or reads the shared grid. The grid
/// lives for the whole process, so two such tests on different threads
/// would otherwise see each other's rows.
#[cfg(test)]
pub(crate) fn lock_shared_grid_for_test() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Swap a blank `columns` by `screen_lines` grid in for the shared one,
/// with no half character carried over. Call with
/// [`lock_shared_grid_for_test`] held.
#[cfg(test)]
pub(crate) fn blank_shared_grid_for_test(columns: usize, screen_lines: usize) {
    *grid_slot().lock().unwrap() = Some(TermGrid::new(columns, screen_lines));
}

/// The rows on the shared grid's screen, trailing blanks trimmed. Empty
/// before the first feed.
#[cfg(test)]
pub(crate) fn shared_screen_rows_for_test() -> Vec<String> {
    with_grid(|grid| {
        grid.map(|g| {
            (0..g.screen_lines())
                .map(|line| g.row_string(line).trim_end().to_string())
                .collect()
        })
        .unwrap_or_default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::vte::ansi::{Color, NamedColor};

    fn cell_fg(g: &TermGrid, line: usize, col: usize) -> Color {
        g.term.grid()[Line(line as i32)][Column(col)].fg
    }

    #[test]
    fn plain_text_lands_in_the_grid() {
        let mut g = TermGrid::new(80, 24);
        g.feed(b"hello");
        assert_eq!(&g.row_string(0)[..5], "hello");
        assert_eq!(g.char_at(0, 0), 'h');
    }

    #[test]
    fn crlf_moves_to_the_next_row() {
        let mut g = TermGrid::new(80, 24);
        g.feed(b"ab\r\ncd");
        assert_eq!(&g.row_string(0)[..2], "ab");
        assert_eq!(&g.row_string(1)[..2], "cd");
    }

    #[test]
    fn sgr_sets_the_foreground_color() {
        let mut g = TermGrid::new(80, 24);
        g.feed(b"\x1b[31mR");
        assert_eq!(cell_fg(&g, 0, 0), Color::Named(NamedColor::Red));
    }

    #[test]
    fn semicolon_truecolor_sgr_sets_spec_fg() {
        use alacritty_terminal::vte::ansi::Rgb;
        let mut g = TermGrid::new(80, 24);
        g.feed(b"\x1b[38;2;100;100;100mX");
        assert_eq!(
            cell_fg(&g, 0, 0),
            Color::Spec(Rgb {
                r: 100,
                g: 100,
                b: 100
            })
        );
    }

    #[test]
    fn bracket_right_after_truecolor_renders() {
        let mut g = TermGrid::new(80, 24);
        g.feed(b"\x1b[38;2;100;100;100m[\x1b[0mABC");
        assert_eq!(g.char_at(0, 0), '[');
        assert_eq!(g.char_at(0, 1), 'A');
        assert_eq!(g.char_at(0, 2), 'B');
        assert_eq!(g.char_at(0, 3), 'C');
    }

    #[test]
    fn long_line_wraps_to_the_next_row() {
        let mut g = TermGrid::new(4, 24);
        g.feed(b"abcdef");
        assert_eq!(&g.row_string(0)[..4], "abcd");
        assert_eq!(&g.row_string(1)[..2], "ef");
    }

    #[test]
    fn the_grid_seeds_from_scrollback_once() {
        assert!(claim_seed());
        assert!(!claim_seed());
        assert!(!claim_seed());
    }

    #[test]
    fn a_local_write_creates_and_fills_the_shared_grid() {
        let _shared = lock_shared_grid_for_test();
        *grid_slot().lock().unwrap() = None;
        feed_local(b"shared");
        let slot = grid_slot().lock().unwrap();
        let g = slot.as_ref().expect("grid created on first feed");
        assert!(g.row_string(0).starts_with("shared"));
    }

    #[test]
    fn a_local_write_names_the_newest_output_of_the_stage_the_grid_took() {
        let _shared = lock_shared_grid_for_test();
        blank_shared_grid_for_test(40, 10);
        assert_eq!(feed_local(b"restored\r\n"), 0, "none yet");
        let mut first = Output::new(false);
        first.text(&marked(1, b"<1020hp> "));
        feed_session_output(&first, Some(first.id()));
        // Output from elsewhere, such as a slash command's echo, is none
        // of the stage's.
        let mut other = Output::new(false);
        other.text(b"[not connected]\r\n");
        feed_session_output(&other, None);
        assert_eq!(feed_local(b"look\r\n"), first.id());
        let mut next = Output::new(false);
        next.text(&marked(2, b"<1000hp> "));
        feed_session_output(&next, Some(next.id()));
        assert_eq!(feed_local(b"x"), next.id());
    }

    #[test]
    fn terminal_cursor_reports_the_shared_grid() {
        let _shared = lock_shared_grid_for_test();
        *grid_slot().lock().unwrap() = None;
        assert_eq!(crate::commands::terminal_cursor(), None, "no grid yet");
        blank_shared_grid_for_test(40, 10);
        let mut out = Output::new(false);
        out.text(b"You are hungry.\r\n");
        out.text(&marked(3, b"<1020hp> "));
        feed_session_output(&out, Some(out.id()));
        let report = serde_json::to_value(crate::commands::terminal_cursor()).expect("json");
        assert_eq!(
            report,
            serde_json::json!({
                "line": 1,
                "col": 9,
                "at_bottom": true,
                "cols": 40,
                "region": {"gen": 3, "line": 1, "col": 0},
            })
        );
        feed_local(b"look\r\n");
        assert_eq!(cursor_report().and_then(|r| r.region), None);
    }

    #[test]
    fn terminal_screen_rows_reads_the_shared_screen_as_text() {
        let _shared = lock_shared_grid_for_test();
        *grid_slot().lock().unwrap() = None;
        assert_eq!(crate::commands::terminal_screen_rows(), None, "no grid yet");
        blank_shared_grid_for_test(20, 4);
        let mut out = Output::new(false);
        out.text("You rest.\r\n<1020hp> 中文 ".as_bytes());
        feed_session_output(&out, Some(out.id()));
        let report = serde_json::to_value(crate::commands::terminal_screen_rows()).expect("json");
        // A wide character takes two cells and reads once.
        assert_eq!(
            report,
            serde_json::json!({
                "rows": ["You rest.", "<1020hp> 中文", "", ""],
                "cols": 20,
                "at_bottom": true,
            })
        );
    }

    #[test]
    fn select_all_spans_scrollback_and_the_live_screen() {
        let mut g = TermGrid::new(10, 2);
        g.feed(b"one\r\ntwo\r\nthree");
        assert!(g.scrollback_len() > 0);
        g.select_all();
        let text = g.term.selection_to_string().expect("a selection");
        assert_eq!(text.trim_end(), "one\ntwo\nthree");
        let (start_line, start_col, end_line, _) = g.selection_bounds().expect("bounds");
        assert_eq!((start_line, start_col), (-1, 0));
        assert_eq!(end_line, 1);
    }

    #[test]
    fn collect_matches_finds_plain_substrings_with_columns() {
        let mut g = TermGrid::new(80, 24);
        g.feed(b"the cat sat\r\nthe cat ran");
        let m = collect_matches(&g, "cat", false, false, false);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0], (0, 4, 7));
        assert_eq!(m[1], (1, 4, 7));
    }

    #[test]
    fn sim_prompt_then_echo_lands_after_prompt() {
        let mut g = TermGrid::new(80, 24);
        // Server blank line then the response block (as the line pipeline
        // emits them), then the gagged prompt replaced by the rendered
        // template WITHOUT trailing newline.
        g.feed(b"\r\nPlayers matched: 9\r\n\r\n");
        g.feed(
            vosh_prompt::wrap::wrap_stream(
                "\x1b[3m\x1b[38;5;240m[\x1b[0m329(\x1b[38;5;42m100%\x1b[0m)h\x1b[0m",
                80,
            )
            .as_bytes(),
        );
        // Local echo of a typed command, written at the cursor.
        g.feed(b"\x1b[38;2;200;200;100mwho\x1b[0m\r\n");
        let row3 = g.row_string(3);
        eprintln!("row3: {:?}", row3.trim_end());
        assert!(row3.starts_with("[329(100%)hwho"), "got: {row3:?}");
    }

    /// Values for a drawn prompt test. Health reads 1020 of 1020 and mana
    /// is hidden.
    struct PromptValues;

    impl vosh_prompt::Values for PromptValues {
        fn resolve(&self, field: &vosh_prompt::FieldRef) -> vosh_prompt::Resolved {
            use vosh_prompt::{Resolved, Value};
            match field.name.as_str() {
                "hp" => Resolved::Value(Value::Gauge {
                    cur: 1020,
                    max: Some(1020),
                    pct: None,
                }),
                "maxhp" => Resolved::Value(Value::Num(1020)),
                "mana" => Resolved::Hidden,
                _ => Resolved::Unknown,
            }
        }
    }

    #[test]
    fn a_drawn_prompt_keeps_italic_across_c_default() {
        let out = vosh_prompt::render_str(
            "%s_italic%c_red%hp%c_default/%c_hp%{maxhp} %c_blue%mana!",
            &PromptValues,
            vosh_prompt::RenderOptions::default(),
        );
        let mut g = TermGrid::new(80, 24);
        g.feed(out.ansi.as_bytes());
        let cell = |col| g.cell_at_line(0, col);
        // 1020 in red, then the slash back in the text color, still italic.
        let (c, fg, _, flags) = cell(0);
        assert_eq!((c, fg), ('1', Color::Named(NamedColor::Red)));
        assert!(flags.italic);
        let (c, fg, _, flags) = cell(4);
        assert_eq!((c, fg), ('/', Color::Named(NamedColor::Foreground)));
        assert!(flags.italic);
        // Color by how full is the theme green at full health.
        let (c, fg, _, flags) = cell(5);
        assert_eq!((c, fg), ('1', Color::Named(NamedColor::Green)));
        assert!(flags.italic);
        // The hidden mark is bright black, and the blue before it comes back.
        let (c, fg, _, flags) = cell(10);
        assert_eq!((c, fg), ('?', Color::Named(NamedColor::BrightBlack)));
        assert!(flags.italic);
        let (c, fg, _, _) = cell(11);
        assert_eq!((c, fg), ('!', Color::Named(NamedColor::Blue)));
        // The render ends in a reset, so what follows is plain.
        g.feed(b"x");
        let (c, fg, _, flags) = g.cell_at_line(0, 12);
        assert_eq!((c, fg), ('x', Color::Named(NamedColor::Foreground)));
        assert!(!flags.italic);
    }

    /// A session output that writes `bytes`.
    fn text(bytes: &[u8]) -> Output {
        let mut out = Output::new(false);
        out.text(bytes);
        out
    }

    /// A session output that replaces region `gen` with `bytes`.
    fn replace(gen: u64, bytes: &[u8], fresh: bool) -> Output {
        let mut out = Output::new(false);
        out.replace(gen, bytes.to_vec(), fresh);
        out
    }

    fn marked(gen: u64, bytes: &[u8]) -> Vec<u8> {
        [vosh_prompt::stage::mark(gen).as_slice(), bytes].concat()
    }

    /// The screen's rows, trailing blanks trimmed, up to the last row
    /// that shows anything.
    fn screen(g: &TermGrid) -> Vec<String> {
        let mut rows: Vec<String> = (0..g.screen_lines())
            .map(|line| g.row_string(line).trim_end().to_string())
            .collect();
        while rows.last().is_some_and(String::is_empty) {
            rows.pop();
        }
        rows
    }

    /// An output that writes `bytes` and holds `hold` back.
    fn held(bytes: &[u8], hold: &[u8]) -> Output {
        let mut out = Output::new(false);
        out.bytes = bytes.to_vec();
        out.hold = hold.to_vec();
        out
    }

    fn cursor(g: &TermGrid) -> (i32, usize) {
        g.cursor()
    }

    fn lift(id: u64, inner: &[u8]) -> Vec<u8> {
        [
            vosh_prompt::stage::lift_start(id).as_slice(),
            inner,
            &vosh_prompt::stage::lift_end(id),
        ]
        .concat()
    }

    /// Every lift row on the grid, as (id, line, first, end).
    fn spans(g: &TermGrid) -> Vec<(u64, i32, usize, usize)> {
        let top = g.term.grid().topmost_line().0;
        g.lift_spans(top, g.screen_lines() as i32)
            .into_iter()
            .map(|s| (s.id, s.line, s.first, s.end))
            .collect()
    }

    #[test]
    fn a_lift_tags_its_cells_and_leaves_your_echo_plain() {
        let mut g = TermGrid::new(40, 10);
        let mut prompt = b"room\r\n\r\n".to_vec();
        prompt.extend(lift(
            3,
            &[b"Tester: [===]\r\n".as_slice(), &marked(4, b"<1020hp>")].concat(),
        ));
        prompt.push(b' ');
        g.session_output(&text(&prompt));
        g.local_write(b"look\r\n");
        assert_eq!(screen(&g), ["room", "", "Tester: [===]", "<1020hp> look"]);
        assert_eq!(spans(&g), [(3, 2, 0, 13), (3, 3, 0, 8)]);
        // The space after the end mark and your echo carry no tag.
        let grid = g.term.grid();
        assert!(grid[Line(3)][Column(8)].hyperlink().is_none());
        assert!(grid[Line(3)][Column(9)].hyperlink().is_none());
    }

    #[test]
    fn a_lift_row_says_when_your_echo_shows_after_it() {
        let mut g = TermGrid::new(40, 10);
        let mut prompt = lift(
            3,
            &[b"Tester: [===]\r\n".as_slice(), &marked(4, b"<1020hp>")].concat(),
        );
        prompt.push(b' ');
        g.session_output(&text(&prompt));
        let after = |g: &TermGrid| -> Vec<bool> {
            g.lift_spans(0, 10).into_iter().map(|s| s.after).collect()
        };
        assert_eq!(after(&g), [false, false], "nothing after it yet");
        g.local_write(b"look\r\n");
        assert_eq!(after(&g), [false, true]);
    }

    #[test]
    fn a_lift_that_scrolls_the_screen_between_its_marks_still_starts_right() {
        let mut g = TermGrid::new(20, 3);
        let body = lift(1, b"one\r\ntwo\r\nthree");
        g.session_output(&text(&[b"a\r\nb\r\nc\r\n".as_slice(), &body].concat()));
        assert_eq!(screen(&g), ["one", "two", "three"]);
        assert_eq!(spans(&g), [(1, 0, 0, 3), (1, 1, 0, 3), (1, 2, 0, 5)]);
        // Word wrapped by the grid, a long lift covers every row it takes.
        let mut g = TermGrid::new(10, 4);
        g.session_output(&text(&lift(2, b"1020/1020hp 800/800mn")));
        assert_eq!(screen(&g), ["1020/1020h", "p", "800/800mn"]);
        assert_eq!(spans(&g), [(2, 0, 0, 10), (2, 1, 0, 1), (2, 2, 0, 9)]);
    }

    #[test]
    fn a_repaint_tags_the_new_prompt_from_its_region() {
        let mut g = TermGrid::new(40, 10);
        g.session_output(&text(&lift(
            1,
            &[b"Tank\r\n".as_slice(), &marked(2, b"<1020hp>")].concat(),
        )));
        g.session_output(&replace(
            2,
            &[
                marked(3, b"<999hp 800m>").as_slice(),
                &vosh_prompt::stage::lift_end(1),
            ]
            .concat(),
            false,
        ));
        assert_eq!(screen(&g), ["Tank", "<999hp 800m>"]);
        assert_eq!(spans(&g), [(1, 0, 0, 4), (1, 1, 0, 12)]);
    }

    #[test]
    fn a_repaint_that_wraps_tags_every_row_it_takes() {
        // At 8 columns the new prompt wraps under the head line.
        let mut g = TermGrid::new(8, 10);
        g.session_output(&text(&lift(
            1,
            &[b"Tank\r\n".as_slice(), &marked(2, b"<1020hp>")].concat(),
        )));
        assert_eq!(spans(&g), [(1, 0, 0, 4), (1, 1, 0, 8)]);
        g.session_output(&replace(
            2,
            &[
                marked(3, b"<999hp 800m>").as_slice(),
                &vosh_prompt::stage::lift_end(1),
            ]
            .concat(),
            false,
        ));
        assert_eq!(screen(&g), ["Tank", "<999hp", "800m>"]);
        assert_eq!(spans(&g), [(1, 0, 0, 4), (1, 1, 0, 6), (1, 2, 0, 5)]);
        // A repaint back to one row leaves no tag on the row below.
        g.session_output(&replace(
            3,
            &[
                marked(4, b"<1020hp>").as_slice(),
                &vosh_prompt::stage::lift_end(1),
            ]
            .concat(),
            false,
        ));
        assert_eq!(screen(&g), ["Tank", "<1020hp>"]);
        assert_eq!(spans(&g), [(1, 0, 0, 4), (1, 1, 0, 8)]);
    }

    #[test]
    fn tags_stay_on_their_text_through_history_and_a_resize() {
        let mut g = TermGrid::new(30, 4);
        g.session_output(&text(&lift(5, b"1020/1020hp 800/800mn")));
        g.session_output(&text(b"\r\nx\r\ny\r\nz\r\nw\r\n"));
        // Scrolled into history, the tags are still there.
        assert_eq!(spans(&g), [(5, -2, 0, 21)]);
        // Reflowed at 12 the space ends the first row, and a span ends at
        // its last glyph.
        g.resize(12, 4);
        let rows: Vec<(u64, usize)> = spans(&g).into_iter().map(|s| (s.0, s.3 - s.2)).collect();
        assert_eq!(rows, [(5, 11), (5, 9)]);
        g.resize(40, 4);
        assert_eq!(spans(&g).len(), 1);
    }

    #[test]
    fn a_lift_that_starts_past_a_full_row_starts_on_the_next() {
        let mut g = TermGrid::new(10, 4);
        g.local_write(b"0123456789");
        g.session_output(&text(&lift(1, b"prompt")));
        assert_eq!(screen(&g), ["0123456789", "prompt"]);
        assert_eq!(spans(&g), [(1, 1, 0, 6)]);
    }

    #[test]
    fn held_line_ends_wait_until_the_next_write_lands() {
        let mut g = TermGrid::new(40, 10);
        g.session_output(&held(b"room\r\n[Exits: south]", b"\r\n\r\n"));
        assert_eq!(screen(&g), ["room", "[Exits: south]"]);
        assert_eq!(cursor(&g), (1, 14), "the text ends on its last line");
        assert_eq!(g.pending_hold(), b"\r\n\r\n");
        // The next output writes them first, then holds its own.
        g.session_output(&held(b"tell", b"\r\n\r\n"));
        assert_eq!(screen(&g), ["room", "[Exits: south]", "", "tell"]);
        // Your echo takes them first too, once.
        g.local_write(b"look\r\n");
        g.local_write(b"x");
        assert_eq!(
            screen(&g),
            ["room", "[Exits: south]", "", "tell", "", "look", "x"]
        );
        assert!(g.pending_hold().is_empty());
    }

    /// The rows the grid shows at its display offset, trailing blanks
    /// trimmed on each.
    fn shown(g: &TermGrid) -> Vec<String> {
        (0..g.screen_lines())
            .map(|line| {
                let row: String = (0..g.columns()).map(|col| g.cell(line, col).0).collect();
                row.trim_end().to_string()
            })
            .collect()
    }

    #[test]
    fn the_newest_line_stays_above_the_pinned_band_as_it_borrows_a_row() {
        // Thirty lines and a pinned prompt, whose line end the grid holds,
        // on a screen of ten rows. A fight grows the band by a row and the
        // grid gives up a row for it, then takes it back, three times.
        let lines: Vec<String> = (1..=30).map(|n| format!("line {n}")).collect();
        let mut g = TermGrid::new(40, 10);
        g.session_output(&held(lines.join("\r\n").as_bytes(), b"\r\n"));
        let calm = screen(&g);
        assert_eq!(calm.first().map(String::as_str), Some("line 21"));
        for _ in 0..3 {
            g.resize(40, 9);
            // The top line leaves for the scrollback and the newest stays
            // on the last row, right above the band.
            let fight = screen(&g);
            assert_eq!(fight.len(), 9);
            assert_eq!(fight.first().map(String::as_str), Some("line 22"));
            assert_eq!(fight.last().map(String::as_str), Some("line 30"));
            assert_eq!(cursor(&g), (8, 7), "the cursor stays after the newest line");
            g.resize(40, 10);
            // The line comes back at the top, and nothing else moved.
            assert_eq!(screen(&g), calm);
            assert_eq!(cursor(&g), (9, 7));
        }
        // The held line end still lands first when the next text comes.
        g.session_output(&held(b"tell", b"\r\n"));
        assert_eq!(screen(&g).last().map(String::as_str), Some("tell"));
        assert_eq!(
            screen(&g).iter().rev().nth(1).map(String::as_str),
            Some("line 30")
        );
    }

    #[test]
    fn a_reader_scrolled_back_keeps_the_lines_in_view_as_the_band_borrows_a_row() {
        let lines: Vec<String> = (1..=60).map(|n| format!("line {n}")).collect();
        let mut g = TermGrid::new(40, 10);
        g.session_output(&held(lines.join("\r\n").as_bytes(), b"\r\n"));
        g.scroll(20);
        let reading = shown(&g);
        assert_eq!(reading.first().map(String::as_str), Some("line 31"));
        g.resize(40, 9);
        // The view keeps its top line, and the row the band took goes
        // from its bottom.
        assert_eq!(shown(&g), reading[..9]);
        g.resize(40, 10);
        assert_eq!(shown(&g), reading);
        assert_eq!(g.display_offset(), 20);
    }

    #[test]
    fn an_output_that_writes_nothing_keeps_the_longer_hold() {
        let mut g = TermGrid::new(40, 10);
        g.session_output(&held(b"room", b"\r\n\r\n"));
        // A pulse whose lines were all swallowed or hidden.
        let mut band = held(b"", b"");
        band.pin = Some(b"<1020>".to_vec());
        g.session_output(&band);
        assert_eq!(g.pending_hold(), b"\r\n\r\n");
        // A shorter hold from such an output never shortens it, and a
        // longer one never stacks with it.
        g.session_output(&held(b"", b"\r\n"));
        assert_eq!(g.pending_hold(), b"\r\n\r\n");
        g.session_output(&held(b"", b"\r\n\r\n\r\n"));
        assert_eq!(g.pending_hold(), b"\r\n\r\n\r\n");
        assert_eq!(screen(&g), ["room"]);
        // Text in a later output replaces it with its own.
        g.session_output(&held(b"tell", b"\r\n"));
        assert_eq!(screen(&g), ["room", "", "", "tell"]);
        assert_eq!(g.pending_hold(), b"\r\n");
    }

    #[test]
    fn a_replace_of_the_open_region_goes_before_the_hold_and_a_fresh_one_after() {
        let mut g = TermGrid::new(40, 10);
        g.session_output(&text(&marked(1, b"partial")));
        // Held line ends after an open region, as a later pulse with
        // nothing but line ends would leave them.
        g.session_output(&held(b"", b"\r\n"));
        g.session_output(&replace(1, &marked(2, b"WHOLE"), false));
        assert_eq!(screen(&g), ["WHOLE"]);
        assert_eq!(g.pending_hold(), b"\r\n");
        // A fresh replace of a closed region writes at the cursor, after
        // the held line ends.
        // Text lands after the held line end.
        g.session_output(&text(b"and more"));
        assert_eq!(screen(&g), ["WHOLE", "and more"]);
        g.session_output(&held(b"", b"\r\n\r\n"));
        g.session_output(&replace(9, b"fresh\r\n", true));
        assert_eq!(screen(&g), ["WHOLE", "and more", "", "fresh"]);
    }

    #[test]
    fn a_replace_rewrites_the_open_region_where_it_starts() {
        let mut g = TermGrid::new(40, 10);
        g.session_output(&text(
            &[b"hungry\r\n".as_slice(), &marked(1, b"PROMPT")].concat(),
        ));
        g.session_output(&replace(1, &marked(2, b"NEW"), false));
        assert_eq!(screen(&g), ["hungry", "NEW"]);
        // The replace opened region 2, so the next one lands too.
        g.session_output(&replace(2, &marked(3, b"LAST"), false));
        assert_eq!(screen(&g), ["hungry", "LAST"]);
        // Region 2 is gone, so a replace for it is dropped.
        g.session_output(&replace(2, b"STALE", false));
        assert_eq!(screen(&g), ["hungry", "LAST"]);
    }

    #[test]
    fn a_replace_after_a_local_write_is_dropped() {
        let mut g = TermGrid::new(40, 10);
        g.session_output(&text(
            &[b"hungry\r\n".as_slice(), &marked(1, b"PROMPT")].concat(),
        ));
        g.local_write(b"look\r\n");
        g.session_output(&replace(1, &marked(2, b"NEW"), false));
        // The echo stays and the prompt shows once.
        assert_eq!(screen(&g), ["hungry", "PROMPTlook"]);
    }

    #[test]
    fn a_replace_after_other_output_is_dropped_unless_fresh() {
        let mut g = TermGrid::new(40, 10);
        g.session_output(&text(&marked(1, b"abc")));
        g.session_output(&text(b"xyz"));
        g.session_output(&replace(1, b"dropped", false));
        assert_eq!(screen(&g), ["abcxyz"]);
        // A fresh one goes on a new row, since the cursor is mid row.
        g.session_output(&replace(1, b"abcdef\r\n", true));
        assert_eq!(screen(&g), ["abcxyz", "abcdef"]);
        // At the start of a row it writes there.
        g.session_output(&text(&marked(2, b"You are hun")));
        g.local_write(b"look\r\n");
        g.session_output(&replace(2, b"You are hungry.\r\n", true));
        assert_eq!(
            screen(&g),
            ["abcxyz", "abcdef", "You are hunlook", "You are hungry."]
        );
    }

    #[test]
    fn a_line_completing_a_painted_partial_replaces_it() {
        let mut g = TermGrid::new(40, 10);
        g.session_output(&text(&marked(1, b"You are hun")));
        let mut next = replace(1, b"You are hungry.\r\n", true);
        next.text(b"You feel better.\r\n");
        g.session_output(&next);
        assert_eq!(screen(&g), ["You are hungry.", "You feel better."]);
        // The completed line carries no mark, so nothing stays open.
        g.session_output(&replace(1, b"again", false));
        assert_eq!(screen(&g), ["You are hungry.", "You feel better."]);
    }

    #[test]
    fn a_region_the_grid_wrapped_is_erased_whole() {
        let mut g = TermGrid::new(12, 10);
        g.session_output(&text(b"before\r\n"));
        g.session_output(&text(&marked(1, b"[1020/1020hp 800/800mn 930/930mv]")));
        assert_eq!(screen(&g).len(), 4, "{:?}", screen(&g));
        g.session_output(&replace(1, &marked(2, b"NEW"), false));
        assert_eq!(screen(&g), ["before", "NEW"]);
    }

    #[test]
    fn a_replace_after_a_resize_counts_the_rows_at_the_new_width() {
        // A full screen, as in the app, so the prompt sits on the last row.
        let mut g = TermGrid::new(40, 6);
        g.session_output(&text(b"one\r\ntwo\r\nthree\r\nfour\r\nbefore\r\n"));
        g.session_output(&text(&marked(1, b"[1020/1020hp 800/800mn 930/930mv]")));
        assert_eq!(
            screen(&g),
            [
                "one",
                "two",
                "three",
                "four",
                "before",
                "[1020/1020hp 800/800mn 930/930mv]"
            ]
        );
        // Narrower, the prompt takes three rows.
        g.resize(12, 6);
        assert_eq!(
            screen(&g),
            [
                "three",
                "four",
                "before",
                "[1020/1020hp",
                " 800/800mn 9",
                "30/930mv]"
            ]
        );
        g.session_output(&replace(1, &marked(2, b"NEW"), false));
        assert_eq!(screen(&g), ["three", "four", "before", "NEW"]);
        // A prompt the word wrap broke keeps its break when the grid
        // widens again.
        g.session_output(&replace(2, &marked(3, b"[1020/1020hp 800/800mn]"), false));
        assert_eq!(
            screen(&g),
            ["three", "four", "before", "[1020/1020hp", "800/800mn]"]
        );
        g.resize(40, 6);
        g.session_output(&replace(3, &marked(4, b"WIDE"), false));
        let rows = screen(&g);
        assert_eq!(rows[rows.len() - 2..], ["before", "WIDE"], "{rows:?}");
    }

    #[test]
    fn a_nearly_empty_screen_keeps_its_rows_when_the_grid_narrows() {
        // A partial painted near the top of the screen, a narrower grid,
        // then the line that completes it. The rows over the partial stay
        // on screen, so the grid erases the partial whole, as xterm does.
        let line = b"Some long line of text that wraps a few times at twelve.\r\n";
        for before in [&b""[..], b"one\r\ntwo\r\n"] {
            for (wide, narrow) in [(40, 12), (40, 20), (20, 7)] {
                let mut g = TermGrid::new(wide, 10);
                g.session_output(&text(before));
                g.session_output(&text(&marked(1, b"Some long line of te")));
                g.resize(narrow, 10);
                g.session_output(&replace(1, line, true));
                let mut expect = TermGrid::new(narrow, 10);
                expect.session_output(&text(&[before, line].concat()));
                let case = format!("{wide} to {narrow} wide after {before:?}");
                assert_eq!(screen(&g), screen(&expect), "{case}");
                assert_eq!(g.scrollback_len(), expect.scrollback_len(), "{case}");
            }
        }
        // The drawn prompt alone on the screen stays within reach too.
        let mut g = TermGrid::new(40, 10);
        g.session_output(&text(&marked(1, b"[1020/1020hp 800/800mn 930/930mv]")));
        g.resize(12, 10);
        g.session_output(&replace(1, &marked(2, b"NEW"), false));
        assert_eq!(screen(&g), ["NEW"]);
    }

    #[test]
    fn a_region_a_resize_pushes_above_the_screen_counts_as_closed() {
        // A full screen of three rows, where the narrower prompt takes
        // five, so its start is in history, out of reach. A replace for
        // it is dropped, and a fresh one writes on a new row.
        let mut g = TermGrid::new(40, 3);
        g.session_output(&text(b"one\r\ntwo\r\n"));
        g.session_output(&text(&marked(1, b"[1020/1020hp 800/800mn 930/930mv]")));
        g.resize(8, 3);
        let before = screen(&g);
        g.session_output(&replace(1, b"dropped", false));
        assert_eq!(screen(&g), before);
        g.session_output(&replace(1, b"fresh", true));
        assert_eq!(screen(&g).last().map(String::as_str), Some("fresh"));
    }

    #[test]
    fn a_mark_after_a_full_row_starts_its_region_on_the_next_row() {
        let mut g = TermGrid::new(10, 10);
        g.local_write(b"0123456789");
        g.session_output(&text(&marked(1, b"PROMPT")));
        assert_eq!(screen(&g), ["0123456789", "PROMPT"]);
        g.session_output(&replace(1, &marked(2, b"NEW"), false));
        assert_eq!(screen(&g), ["0123456789", "NEW"]);
        // A region that wrote nothing yet takes the replace where it is.
        g.local_write(b"\r\n0123456789");
        g.session_output(&text(&marked(3, b"")));
        g.session_output(&replace(3, &marked(4, b"HERE"), false));
        assert_eq!(screen(&g), ["0123456789", "NEW", "0123456789", "HERE"]);
    }

    /// The open region's start in a cursor report, as (gen, line, col).
    fn region_at(g: &TermGrid) -> Option<(u64, i32, usize)> {
        g.cursor_report().region.map(|r| (r.gen, r.line, r.col))
    }

    #[test]
    fn the_cursor_report_names_where_the_open_region_starts() {
        // One row, under a line of text.
        let mut g = TermGrid::new(40, 10);
        g.session_output(&text(b"You are hungry.\r\n"));
        g.session_output(&text(&marked(7, b"[1020/1020hp]")));
        assert_eq!(
            g.cursor_report(),
            CursorReport {
                line: 1,
                col: 13,
                at_bottom: true,
                cols: 40,
                region: Some(RegionStart {
                    gen: 7,
                    line: 1,
                    col: 0
                }),
            }
        );

        // Two rows, as a line break in the design draws them.
        let mut g = TermGrid::new(40, 10);
        g.session_output(&text(b"You are hungry.\r\n"));
        g.session_output(&text(&marked(2, b"Tank 100%\r\n[1020/1020hp]")));
        assert_eq!(region_at(&g), Some((2, 1, 0)));
        assert_eq!(cursor(&g), (2, 13));

        // Word wrapped at a narrow width, the region still starts on its
        // first row.
        let mut g = TermGrid::new(12, 10);
        g.session_output(&text(b"You are hungry.\r\n"));
        g.session_output(&text(&marked(3, b"[1020/1020hp 800/800mn 930/930mv]")));
        assert_eq!(
            screen(&g),
            [
                "You are",
                "hungry.",
                "[1020/1020hp",
                "800/800mn",
                "930/930mv]"
            ]
        );
        assert_eq!(region_at(&g), Some((3, 2, 0)));

        // A region that starts mid row, and one that wrote nothing yet.
        let mut g = TermGrid::new(40, 10);
        g.session_output(&text(b"<10hp> "));
        g.session_output(&text(&marked(4, b"more")));
        assert_eq!(region_at(&g), Some((4, 0, 7)));
        g.session_output(&text(&marked(5, b"")));
        assert_eq!(region_at(&g), Some((5, 0, 11)));
        // A mark after a full row starts its region on the next.
        let mut g = TermGrid::new(10, 10);
        g.local_write(b"0123456789");
        g.session_output(&text(&marked(6, b"")));
        assert_eq!(region_at(&g), Some((6, 1, 0)));

        // Lift marks inside a region take no room.
        let mut g = TermGrid::new(12, 10);
        g.session_output(&text(b"room\r\n"));
        g.session_output(&text(&marked(8, &lift(1, b"[1020/1020hp 800/800mn]"))));
        assert_eq!(screen(&g), ["room", "[1020/1020hp", "800/800mn]"]);
        assert_eq!(region_at(&g), Some((8, 1, 0)));
    }

    #[test]
    fn the_cursor_report_has_no_region_once_anything_lands_after_it() {
        let mut g = TermGrid::new(40, 10);
        g.session_output(&text(&marked(1, b"<1020hp> ")));
        assert_eq!(region_at(&g), Some((1, 0, 0)));
        // Your echo.
        g.local_write(b"look\r\n");
        assert_eq!(region_at(&g), None);
        // Game text after the next prompt.
        g.session_output(&text(&marked(2, b"<1020hp> ")));
        g.session_output(&text(b"\r\nYou are hungry.\r\n"));
        assert_eq!(region_at(&g), None);
        // A replace keeps it open at its new start.
        g.session_output(&text(&marked(3, b"<1020hp> ")));
        g.session_output(&replace(3, &marked(4, b"Tank\r\n<1020hp> "), false));
        assert_eq!(region_at(&g), Some((4, 3, 0)));
    }

    #[test]
    fn the_cursor_report_follows_a_resize_and_says_when_you_scrolled_back() {
        // The card keeps the row open through a resize, and the start is
        // counted again at the new width.
        let mut g = TermGrid::new(40, 6);
        g.session_output(&text(b"one\r\ntwo\r\n"));
        g.session_output(&text(&marked(1, b"[1020/1020hp 800/800mn 930/930mv]")));
        assert_eq!(region_at(&g), Some((1, 2, 0)));
        g.resize(12, 6);
        let report = g.cursor_report();
        assert_eq!(report.cols, 12);
        let start = report.region.expect("still open");
        assert_eq!(report.line - start.line, 2, "three rows at 12 wide");

        // Scrolled back into history, the report says so.
        let mut g = TermGrid::new(20, 3);
        for n in 0..10 {
            g.session_output(&text(format!("line {n}\r\n").as_bytes()));
        }
        g.session_output(&text(&marked(9, b"<1020hp> ")));
        assert!(g.cursor_report().at_bottom);
        g.scroll(4);
        let report = g.cursor_report();
        assert!(!report.at_bottom);
        assert_eq!(report.region.map(|r| r.line), Some(2));
    }

    #[test]
    fn a_region_that_starts_mid_row_keeps_what_came_before_it() {
        let mut g = TermGrid::new(40, 10);
        g.session_output(&text(b"<10hp> "));
        g.session_output(&text(&marked(1, b"You are hun")));
        g.session_output(&replace(1, b"You are hungry.\r\n", true));
        assert_eq!(screen(&g), ["<10hp> You are hungry."]);
    }

    #[test]
    fn a_restore_goes_back_before_anything_else_lands() {
        // Before a local write.
        let mut g = TermGrid::new(40, 10);
        let mut preview = text(&marked(1, b"PREVIEW"));
        preview.restore = Some(b"LIVE".to_vec());
        g.session_output(&preview);
        assert_eq!(screen(&g), ["PREVIEW"]);
        g.local_write(b"look\r\n");
        assert_eq!(screen(&g), ["LIVElook"]);

        // Before session output.
        let mut g = TermGrid::new(40, 10);
        g.session_output(&preview);
        g.session_output(&text(b"\r\nYou flee!\r\n"));
        assert_eq!(screen(&g), ["LIVE", "You flee!"]);

        // A replace of the region itself takes its place, and its own
        // restore rides the region it opens.
        let mut g = TermGrid::new(40, 10);
        g.session_output(&preview);
        let mut again = replace(1, &marked(2, b"OTHER"), false);
        again.restore = Some(b"LIVE2".to_vec());
        g.session_output(&again);
        assert_eq!(screen(&g), ["OTHER"]);
        g.session_output(&replace(2, &marked(3, b"PLAIN"), false));
        g.local_write(b"look\r\n");
        assert_eq!(screen(&g), ["PLAINlook"]);

        // A replace of the region and text after it, in one output: the
        // replace goes first, and the text lands after it.
        let mut g = TermGrid::new(40, 10);
        g.session_output(&preview);
        let mut next = replace(1, &marked(2, b"NEW> "), false);
        next.text(b"\r\nYou flee!\r\n");
        g.session_output(&next);
        assert_eq!(screen(&g), ["NEW>", "You flee!"]);
    }

    #[test]
    fn a_split_character_decodes_whole_across_outputs() {
        let mut g = TermGrid::new(40, 10);
        let word = "caf\u{e9}".as_bytes();
        g.session_output(&text(&word[..4]));
        g.session_output(&text(&word[4..]));
        assert_eq!(screen(&g), ["caf\u{e9}"]);
    }

    #[test]
    fn marks_are_found_whole_and_only_whole() {
        let mark = vosh_prompt::stage::mark(42);
        let bytes = [b"ab".as_slice(), &mark, b"cd"].concat();
        assert_eq!(
            find_mark(&bytes),
            Some((2, 2 + mark.len(), Mark::Region(42)))
        );
        assert_eq!(find_mark(b"\x1b]7717;o;\x07"), None);
        assert_eq!(find_mark(b"\x1b]7717;o;12"), None);
        assert_eq!(find_mark(b"plain"), None);
        let start = vosh_prompt::stage::lift_start(7);
        let end = vosh_prompt::stage::lift_end(7);
        assert_eq!(
            find_mark(&start),
            Some((0, start.len(), Mark::LiftStart(7)))
        );
        assert_eq!(find_mark(&end), Some((0, end.len(), Mark::LiftEnd(7))));
        assert_eq!(find_mark(b"\x1b]7717;x;7\x07"), None);
        assert_eq!(erase_back(0, 0), b"\r\x1b[0J");
        assert_eq!(erase_back(2, 7), b"\r\x1b[2A\x1b[7C\x1b[0J");
    }

    // The wrap itself runs fixtures/wrap/cases.json in crates/prompt and in
    // src/lib/wordWrap.test.ts.
    #[test]
    fn session_feed_word_wraps_at_the_grid_width() {
        let _shared = lock_shared_grid_for_test();
        let Ok(mut slot) = grid_slot().lock() else {
            panic!("grid lock");
        };
        *slot = Some(TermGrid::new(10, 24));
        drop(slot);
        feed_session_output(&text(b"the quick brown fox\r\n"), None);
        let slot = grid_slot().lock().unwrap();
        let g = slot.as_ref().unwrap();
        assert!(g.row_string(0).starts_with("the quick"));
        assert!(g.row_string(1).starts_with("brown fox"));
    }

    #[test]
    fn collect_matches_columns_stay_aligned_after_wide_chars() {
        let mut g = TermGrid::new(80, 24);
        // 日 and 本 are double-width: the glyph occupies its cell and the
        // next holds a spacer that reads as a space. Line text collects one
        // char per column, so char offsets stay 1:1 with grid columns.
        g.feed("ab\u{65e5}\u{672c} cat".as_bytes());
        let m = collect_matches(&g, "cat", false, false, false);
        assert_eq!(m.len(), 1);
        // Columns: a=0 b=1 日=2 (spacer 3) 本=4 (spacer 5) space=6 c=7.
        assert_eq!(m[0], (0, 7, 10));
    }

    #[test]
    fn url_columns_stay_aligned_after_wide_chars() {
        // Same one-char-per-column construction url_at feeds url_in_line:
        // 日=0 spacer=1 本=2 spacer=3, url starts at column 4.
        let text = "\u{65e5} \u{672c} http://x.dev";
        let hit = url_in_line(text, 6).expect("url under cursor");
        assert_eq!(hit, ("http://x.dev".to_string(), 4, 16));
        assert!(url_in_line(text, 3).is_none());
    }

    #[test]
    fn collect_matches_honors_regex_and_case() {
        let mut g = TermGrid::new(80, 24);
        g.feed(b"HP: 100  hp: 50");
        // Regex, case-insensitive: both HP and hp match.
        assert_eq!(collect_matches(&g, r"hp: \d+", true, false, false).len(), 2);
        // Case-sensitive: only the lowercase one.
        assert_eq!(collect_matches(&g, r"hp: \d+", true, true, false).len(), 1);
    }

    #[test]
    fn collect_matches_whole_word_excludes_substrings() {
        let mut g = TermGrid::new(80, 24);
        g.feed(b"cat category");
        // Without whole-word, "cat" matches inside "category" too.
        assert_eq!(collect_matches(&g, "cat", false, false, false).len(), 2);
        // With whole-word, only the standalone "cat".
        assert_eq!(collect_matches(&g, "cat", false, false, true).len(), 1);
    }

    /// The stage's output driven into the grid, as the session and the
    /// native renderer pass it along (section 9, stage into `TermGrid`).
    mod stage_into_grid {
        use super::*;
        use vosh_prompt::config::RegexCapture;
        use vosh_prompt::stage::{End, Stage};
        use vosh_prompt::CaptureConfig;

        const CAPTURE: &str = r"\[(?<hp>\d+)/(?<maxhp>\d+)hp\]";
        const GAME: &str = "[1020/1020hp]";
        /// A design of one row at 40 wide and three rows at 12 wide.
        const ONE_ROW: &str = "[1020/1020hp 800/800mn 930/930mv]";
        /// A design of two rows, as `%nl` draws it.
        const TWO_ROWS: &str = "Tank 100%\r\n[1020/1020hp]";

        fn stage() -> Stage {
            let mut stage = Stage::default();
            stage.set_capture(&CaptureConfig::Regex(RegexCapture {
                lines: vec![CAPTURE.to_string()],
                ..RegexCapture::default()
            }));
            stage
        }

        /// A read that brings `before`, then the game's prompt, which the
        /// stage draws as `drawn`.
        fn prompt_read(stage: &mut Stage, before: &[u8], drawn: &str) -> Output {
            let mut out = Output::new(false);
            out.text(before);
            let block = stage
                .recognize(GAME.as_bytes(), GAME, End::Line)
                .expect("the capture reads the game's prompt");
            stage.draw(&mut out, block, None, b"", drawn);
            stage.finish(&mut out);
            out
        }

        /// A repaint of the open row as `drawn`, or as the game sent it.
        fn repaint(stage: &mut Stage, drawn: Option<&str>) -> Output {
            let mut out = Output::new(false);
            stage.repaint(&mut out, drawn);
            out
        }

        /// How many rows of `rows` hold `text`.
        fn count(rows: &[String], text: &str) -> usize {
            rows.iter().filter(|row| row.contains(text)).count()
        }

        #[test]
        fn a_repaint_after_your_echo_is_dropped_at_either_width() {
            for (columns, design, expect) in [
                (
                    40,
                    ONE_ROW,
                    vec!["You are hungry.", "[1020/1020hp 800/800mn 930/930mv]look"],
                ),
                (
                    12,
                    ONE_ROW,
                    vec![
                        "You are",
                        "hungry.",
                        "[1020/1020hp",
                        "800/800mn",
                        "930/930mv]lo",
                        "ok",
                    ],
                ),
                (
                    40,
                    TWO_ROWS,
                    vec!["You are hungry.", "Tank 100%", "[1020/1020hp]look"],
                ),
                (
                    12,
                    TWO_ROWS,
                    vec!["You are", "hungry.", "Tank 100%", "[1020/1020hp", "]look"],
                ),
            ] {
                let mut stage = stage();
                let mut g = TermGrid::new(columns, 12);
                g.session_output(&prompt_read(&mut stage, b"You are hungry.\r\n", design));
                // Your echo lands before the session hears of it, so the
                // stage still holds the row open and repaints it.
                g.local_write(b"look\r\n");
                let out = repaint(&mut stage, Some("NEW"));
                assert!(out.replace.is_some());
                g.session_output(&out);
                let rows = screen(&g);
                assert_eq!(rows, expect, "{columns} wide");
                // The echo shows once, even where it wraps, and the
                // prompt shows once.
                assert_eq!(rows.concat().matches("look").count(), 1);
                assert_eq!(rows.concat().matches("1020hp").count(), 1);
                assert_eq!(count(&rows, "NEW"), 0);
            }
        }

        #[test]
        fn your_echo_after_a_repaint_follows_the_new_prompt_at_either_width() {
            for (columns, design) in [(40, ONE_ROW), (12, ONE_ROW), (40, TWO_ROWS), (12, TWO_ROWS)]
            {
                let mut stage = stage();
                let mut g = TermGrid::new(columns, 12);
                g.session_output(&prompt_read(&mut stage, b"You are hungry.\r\n", design));
                g.session_output(&repaint(&mut stage, Some("NEW> ")));
                g.local_write(b"look\r\n");
                let rows = screen(&g);
                let hungry = if columns == 40 {
                    vec!["You are hungry."]
                } else {
                    vec!["You are", "hungry."]
                };
                assert_eq!(
                    rows,
                    [hungry, vec!["NEW> look"]].concat(),
                    "{columns} wide, {design:?}"
                );
                assert_eq!(count(&rows, "1020"), 0, "the old design is gone whole");
            }
        }

        #[test]
        fn drawing_off_shows_the_game_prompt_where_the_design_was() {
            let mut stage = stage();
            let mut g = TermGrid::new(40, 12);
            g.session_output(&prompt_read(&mut stage, b"You are hungry.\r\n", TWO_ROWS));
            g.session_output(&repaint(&mut stage, None));
            g.local_write(b"look\r\n");
            assert_eq!(screen(&g), ["You are hungry.", GAME, "look"]);
        }

        #[test]
        fn a_prompt_whole_before_its_line_end_repaints_with_its_line_end() {
            // A capture that settles, on a prompt the game followed with
            // a line end in the same read.
            let mut stage = Stage::default();
            stage.set_capture(&CaptureConfig::Regex(RegexCapture {
                lines: vec![r"^\[(?<hp>\d+)/(?<maxhp>\d+)hp\]$".to_string()],
                settle: true,
                ..RegexCapture::default()
            }));
            for columns in [40, 12] {
                let mut g = TermGrid::new(columns, 12);
                g.session_output(&prompt_read(&mut stage, b"You are hungry.\r\n", ONE_ROW));
                // Drawing off shows the game's prompt where the design
                // was, and your echo lands on the row after it.
                g.session_output(&repaint(&mut stage, None));
                g.local_write(b"look\r\n");
                let mut expect = TermGrid::new(columns, 12);
                expect.session_output(&text(b"You are hungry.\r\n[1020/1020hp]\r\nlook\r\n"));
                assert_eq!(screen(&g), screen(&expect), "{columns} wide");
            }
        }

        #[test]
        fn a_prompt_split_across_reads_replaces_its_painted_start() {
            let mut stage = stage();
            let mut g = TermGrid::new(12, 12);
            let mut first = Output::new(false);
            first.text(b"You are hungry.\r\n");
            let painted = stage.paint_partial(&mut first, b"[1020/10", None);
            stage.finish(&mut first);
            g.session_output(&first);
            assert_eq!(screen(&g), ["You are", "hungry.", "[1020/10"]);
            let mut second = Output::new(false);
            let block = stage
                .recognize(GAME.as_bytes(), GAME, End::Line)
                .expect("the prompt");
            stage.draw(
                &mut second,
                block,
                painted.map(|(gen, _)| gen),
                b"",
                ONE_ROW,
            );
            g.session_output(&second);
            assert_eq!(
                screen(&g),
                [
                    "You are",
                    "hungry.",
                    "[1020/1020hp",
                    "800/800mn",
                    "930/930mv]"
                ]
            );
            // The drawn prompt is the open row now.
            g.session_output(&repaint(&mut stage, Some("NEW")));
            assert_eq!(screen(&g), ["You are", "hungry.", "NEW"]);
        }

        #[test]
        fn a_repaint_after_a_resize_while_the_row_stays_open() {
            // With the card open the session keeps the row open through a
            // resize, and the grid finds the region at its new width.
            let mut stage = stage();
            let mut g = TermGrid::new(40, 4);
            g.session_output(&prompt_read(
                &mut stage,
                b"one\r\ntwo\r\nYou are hungry.\r\n",
                ONE_ROW,
            ));
            g.resize(12, 4);
            g.session_output(&repaint(&mut stage, Some("NEW")));
            // The rows over the prompt reflow at 12 wide, and the narrower
            // grid keeps the cursor row, so the first ones move into
            // history. The row just over the prompt stays whole.
            assert_eq!(screen(&g), ["ry.", "NEW"]);
        }

        #[test]
        fn a_preview_goes_back_to_the_live_render_before_your_echo() {
            let mut stage = stage();
            let mut g = TermGrid::new(40, 12);
            g.session_output(&prompt_read(&mut stage, b"You are hungry.\r\n", "LIVE> "));
            let mut preview = repaint(&mut stage, Some("PREVIEW> "));
            preview.restore = Some(b"LIVE> ".to_vec());
            g.session_output(&preview);
            assert_eq!(screen(&g), ["You are hungry.", "PREVIEW>"]);
            g.local_write(b"look\r\n");
            assert_eq!(screen(&g), ["You are hungry.", "LIVE> look"]);
        }

        #[test]
        fn a_repaint_that_crosses_later_output_is_dropped() {
            let mut stage = stage();
            let mut g = TermGrid::new(40, 12);
            g.session_output(&prompt_read(&mut stage, b"", "DRAWN> "));
            // A repaint the session sent before the next read reached the
            // renderer, which then finds output after the region.
            let stale = repaint(&mut stage.clone(), Some("NEW> "));
            let mut next = Output::new(false);
            next.text(b"\r\nYou flee!\r\n");
            g.session_output(&next);
            g.session_output(&stale);
            assert_eq!(screen(&g), ["DRAWN>", "You flee!"]);
        }
    }
}

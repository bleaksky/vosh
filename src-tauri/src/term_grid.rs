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
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor};
use regex::RegexBuilder;
use vosh_prompt::stage::{Output, MARK_OSC};

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
        }
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
    pub(crate) fn session_output(&mut self, out: &Output) {
        if let Some(replace) = &out.replace {
            // Half a character the last write held back belongs to the
            // region the replace rewrites whole.
            self.pending_utf8.clear();
            let text = self.wrap(&String::from_utf8_lossy(&replace.bytes));
            if let Some(to_start) = self.locate(replace.gen) {
                self.region = None;
                self.feed(&to_start);
                self.feed_marked(text.as_bytes());
            } else if replace.fresh && !replace.bytes.is_empty() {
                self.restore_first();
                self.region = None;
                if !self.at_row_start() {
                    self.feed(b"\r\n");
                }
                self.feed_marked(text.as_bytes());
            }
        }
        if !out.bytes.is_empty() {
            self.restore_first();
            self.region = None;
            let text = self.decode(&out.bytes);
            if !text.is_empty() {
                let text = self.wrap(&text);
                self.feed_marked(text.as_bytes());
            }
        }
        if let (Some(restore), Some(region)) = (&out.restore, self.region.as_mut()) {
            region.restore = Some(restore.clone());
        }
    }

    /// Write text the webview wrote itself, such as your typed echo. It
    /// lands after the open region, so it closes it, and a preview the
    /// region shows goes back to the live render first.
    pub(crate) fn local_write(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.restore_first();
        self.region = None;
        self.feed(bytes);
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

    /// Feed `bytes`, taking each region mark out and noting where it
    /// came, so the bytes after it are that region's.
    fn feed_marked(&mut self, bytes: &[u8]) {
        let mut rest = bytes;
        while let Some((start, end, gen)) = find_mark(rest) {
            self.feed_region(&rest[..start]);
            let cursor = &self.term.grid().cursor;
            self.region = Some(Region {
                gen,
                col: cursor.point.column.0,
                wrap_pending: cursor.input_needs_wrap,
                bytes: Vec::new(),
                restore: None,
            });
            rest = &rest[end..];
        }
        self.feed_region(rest);
    }

    /// Feed `bytes`, which belong to the open region when there is one.
    fn feed_region(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.feed(bytes);
        if let Some(region) = self.region.as_mut() {
            region.bytes.extend_from_slice(bytes);
        }
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
    pub(crate) fn resize(&mut self, columns: usize, screen_lines: usize) {
        let columns = columns.max(1);
        let screen_lines = screen_lines.max(1);
        if columns == self.size.columns && screen_lines == self.size.screen_lines {
            return;
        }
        self.size = GridSize {
            columns,
            screen_lines,
        };
        self.term.resize(self.size);
    }
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

/// The first region mark in `bytes`: where it starts, where it ends and
/// its generation.
fn find_mark(bytes: &[u8]) -> Option<(usize, usize, u64)> {
    let prefix = format!("\x1b]{MARK_OSC};o;");
    let prefix = prefix.as_bytes();
    let mut from = 0;
    while let Some(at) = bytes[from..]
        .windows(prefix.len())
        .position(|w| w == prefix)
        .map(|i| from + i)
    {
        let digits = &bytes[at + prefix.len()..];
        let len = digits.iter().take_while(|b| b.is_ascii_digit()).count();
        if len > 0 && digits.get(len) == Some(&0x07) {
            let gen = std::str::from_utf8(&digits[..len])
                .ok()
                .and_then(|s| s.parse().ok());
            if let Some(gen) = gen {
                return Some((at, at + prefix.len() + len + 1, gen));
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
/// same grid.
pub(crate) fn feed_local(bytes: &[u8]) {
    let Ok(mut slot) = grid_slot().lock() else {
        return;
    };
    let grid = slot.get_or_insert_with(|| TermGrid::new(80, 24));
    grid.local_write(bytes);
}

/// Write one session output into the shared grid under its lock, word
/// wrapped at the grid width, with its replace and restore. See
/// [`TermGrid::session_output`]. Every `session://output` goes through
/// here as well, so the grid holds what xterm holds.
pub(crate) fn feed_session_output(out: &Output) {
    let Ok(mut slot) = grid_slot().lock() else {
        return;
    };
    let grid = slot.get_or_insert_with(|| TermGrid::new(80, 24));
    grid.session_output(out);
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
    fn a_region_a_resize_pushes_above_the_screen_counts_as_closed() {
        // A nearly empty screen: the narrower grid pushes the rows over
        // the cursor into history, where the region's start is out of
        // reach, so a replace for it is dropped and a fresh one writes on
        // a new row.
        let mut g = TermGrid::new(40, 10);
        g.session_output(&text(&marked(1, b"[1020/1020hp 800/800mn 930/930mv]")));
        g.resize(12, 10);
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
        assert_eq!(find_mark(&bytes), Some((2, 2 + mark.len(), 42)));
        assert_eq!(find_mark(b"\x1b]7717;o;\x07"), None);
        assert_eq!(find_mark(b"\x1b]7717;o;12"), None);
        assert_eq!(find_mark(b"plain"), None);
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
        feed_session_output(&text(b"the quick brown fox\r\n"));
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
            stage.finish(&out);
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
        fn a_prompt_split_across_reads_replaces_its_painted_start() {
            let mut stage = stage();
            let mut g = TermGrid::new(12, 12);
            let mut first = Output::new(false);
            first.text(b"You are hungry.\r\n");
            let painted = stage.paint_partial(&mut first, b"[1020/10", None);
            stage.finish(&first);
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

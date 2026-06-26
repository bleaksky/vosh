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

#![cfg(target_os = "macos")]

use std::sync::{Mutex, OnceLock};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor};
use regex::RegexBuilder;

/// Render-relevant cell attributes, decoupled from alacritty's `Flags`.
#[derive(Clone, Copy, Default)]
pub(crate) struct CellFlags {
    pub bold: bool,
    pub dim: bool,
    pub inverse: bool,
    pub underline: bool,
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
            inverse: flags.contains(Flags::INVERSE),
            underline: flags.intersects(Flags::ALL_UNDERLINES),
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

/// Feed the live session's display bytes into the shared grid, creating
/// it on first use. Called from the session loop. Cheap and lock-guarded;
/// the renderer reads the same grid.
pub(crate) fn feed_bytes(bytes: &[u8]) {
    let Ok(mut slot) = grid_slot().lock() else {
        return;
    };
    let grid = slot.get_or_insert_with(|| TermGrid::new(80, 24));
    grid.feed(bytes);
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
    fn long_line_wraps_to_the_next_row() {
        let mut g = TermGrid::new(4, 24);
        g.feed(b"abcdef");
        assert_eq!(&g.row_string(0)[..4], "abcd");
        assert_eq!(&g.row_string(1)[..2], "ef");
    }

    #[test]
    fn feed_bytes_creates_and_fills_the_shared_grid() {
        feed_bytes(b"shared");
        let slot = grid_slot().lock().unwrap();
        let g = slot.as_ref().expect("grid created on first feed");
        assert!(g.row_string(0).starts_with("shared"));
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
}

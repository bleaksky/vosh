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
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor};

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
    pub(crate) fn cell_at_line(&self, grid_line: i32, col: usize) -> (char, Color, Color) {
        let grid = self.term.grid();
        let target = Line(grid_line);
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

    /// Current scrollback display offset (0 = live tail).
    pub(crate) fn display_offset(&self) -> usize {
        self.term.grid().display_offset()
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
}

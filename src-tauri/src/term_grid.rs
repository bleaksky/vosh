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
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{Color, Processor};

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

    /// A cell's character and fg/bg colors, for the renderer.
    pub(crate) fn cell(&self, line: usize, col: usize) -> (char, Color, Color) {
        let cell = &self.term.grid()[Line(line as i32)][Column(col)];
        (cell.c, cell.fg, cell.bg)
    }
}

static GRID: OnceLock<Mutex<Option<TermGrid>>> = OnceLock::new();

fn grid_slot() -> &'static Mutex<Option<TermGrid>> {
    GRID.get_or_init(|| Mutex::new(None))
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

/// Read the shared grid (None until the first feed). The renderer calls
/// this on the main thread to build a frame. Consumed by the pipeline in
/// M2c part 3b's wiring step.
#[allow(dead_code)]
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

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
//!
//! This file holds the grid, its cells, selection, scroll and resize, and
//! the shared grid the session feeds and the renderer reads. `regions`
//! writes the session's output with the prompt regions it may replace,
//! `find` searches the grid, `links` finds the web links in it, and
//! `blink` keeps the blink alacritty drops.

use std::sync::{Mutex, OnceLock};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor};
use vosh_prompt::stage::Output;

mod blink;
pub(crate) mod find;
pub(crate) mod links;
pub(crate) mod regions;

use blink::{Blinking, BLINK};
use regions::{CursorReport, LiftTrack, Region, ScreenRows};

/// How a cell is underlined: SGR 4 and its `4:x` sub parameter, where
/// 1 is single, 2 double, 3 curly, 4 dotted, and 5 dashed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Underline {
    #[default]
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

impl Underline {
    /// The kind alacritty set on a cell. It keeps one kind at a time, so
    /// a new SGR 4 replaces the last.
    fn of(flags: Flags) -> Self {
        if flags.contains(Flags::DOUBLE_UNDERLINE) {
            Self::Double
        } else if flags.contains(Flags::UNDERCURL) {
            Self::Curly
        } else if flags.contains(Flags::DOTTED_UNDERLINE) {
            Self::Dotted
        } else if flags.contains(Flags::DASHED_UNDERLINE) {
            Self::Dashed
        } else if flags.contains(Flags::UNDERLINE) {
            Self::Single
        } else {
            Self::None
        }
    }
}

/// Render-relevant cell attributes, decoupled from alacritty's `Flags`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CellFlags {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub inverse: bool,
    pub underline: Underline,
    /// The SGR 58 underline color. `None` draws the line in the text
    /// color.
    pub underline_color: Option<Color>,
    pub strikeout: bool,
    /// SGR 8: the cell keeps its place and background but shows no glyph.
    pub hidden: bool,
    /// SGR 5: the glyph blinks (see [`BLINK`]).
    pub blink: bool,
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
    size: GridSize,
    /// The region the last session write left open, if any.
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

    /// Advance the VT parser over a chunk of post-telnet bytes. vte
    /// 0.13's `advance` is byte-at-a-time. The terminal takes them
    /// through [`Blinking`], which keeps the blink it drops.
    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        let mut term = Blinking(&mut self.term);
        for &byte in bytes {
            self.parser.advance(&mut term, byte);
        }
    }

    pub(crate) fn columns(&self) -> usize {
        self.size.columns
    }

    pub(crate) fn screen_lines(&self) -> usize {
        self.size.screen_lines
    }

    /// The character at a visible-screen cell (line 0 = top row).
    #[cfg(test)]
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
    #[cfg(test)]
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
            underline: Underline::of(flags),
            underline_color: cell.underline_color(),
            strikeout: flags.contains(Flags::STRIKEOUT),
            hidden: flags.contains(Flags::HIDDEN),
            blink: flags.contains(BLINK),
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

    /// Begin a text selection anchored at the left edge of a grid cell.
    pub(crate) fn start_selection(&mut self, line: i32, col: usize) {
        let point = Point::new(Line(line), Column(col));
        self.term.selection = Some(Selection::new(SelectionType::Simple, point, Side::Left));
    }

    /// Extend the active selection to the left edge of a grid cell, or
    /// with `through_end` through the last cell of the line, so the whole
    /// line and its line break come along.
    pub(crate) fn extend_selection(&mut self, line: i32, col: usize, through_end: bool) {
        let last = self.term.grid().last_column();
        if let Some(selection) = self.term.selection.as_mut() {
            if through_end {
                selection.update(Point::new(Line(line), last), Side::Right);
            } else {
                selection.update(Point::new(Line(line), Column(col)), Side::Left);
            }
        }
    }

    /// The selected text, or None when there is no selection.
    pub(crate) fn selection_text(&self) -> Option<String> {
        self.term.selection_to_string()
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
    /// region near its top stays within reach of a replace.
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

/// Current scrollback offset of the shared grid (0 = live tail, no split).
pub(crate) fn current_display_offset() -> usize {
    grid_slot()
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().map(TermGrid::display_offset))
        .unwrap_or(0)
}

/// You are selecting text in the shared grid or reading back in it, so a
/// clock repaint of your prompt waits and the row you select or read never
/// moves. False with no grid.
pub(crate) fn reader_busy() -> bool {
    grid_slot().lock().is_ok_and(|slot| {
        slot.as_ref().is_some_and(|grid| {
            grid.display_offset() != 0
                || grid
                    .term
                    .selection
                    .as_ref()
                    .is_some_and(|selection| !selection.is_empty())
        })
    })
}

/// Begin a text selection anchored at a grid cell.
pub(crate) fn start_selection(line: i32, col: usize) {
    with_grid_mut(|grid| grid.start_selection(line, col));
}

/// Extend the active selection to a grid cell.
pub(crate) fn update_selection(line: i32, col: usize) {
    with_grid_mut(|grid| grid.extend_selection(line, col, false));
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
    grid_slot()
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().and_then(TermGrid::selection_text))
}

/// Read the shared grid (None until the first feed). The renderer calls
/// this on the main thread to build a frame.
pub(crate) fn with_grid<R>(f: impl FnOnce(Option<&TermGrid>) -> R) -> R {
    match grid_slot().lock() {
        Ok(slot) => f(slot.as_ref()),
        Err(_) => f(None),
    }
}

/// Change the shared grid under its lock. None until the first feed.
pub(crate) fn with_grid_mut<R>(f: impl FnOnce(&mut TermGrid) -> R) -> Option<R> {
    grid_slot()
        .lock()
        .ok()
        .and_then(|mut slot| slot.as_mut().map(f))
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
mod tests;

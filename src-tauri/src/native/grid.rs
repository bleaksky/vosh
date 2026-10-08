//! The native terminal renderer (see docs/renderer.md).
//!
//! Wraps `alacritty_terminal`'s `Term` so the post-telnet byte stream
//! (the same bytes Vosh hands xterm) builds a real cell grid: characters,
//! colors, styles, cursor, and scrollback, with all the VT escape-code
//! semantics handled by Alacritty's parser. The renderer in `gpu` walks
//! this grid and draws each cell.
//!
//! This file holds the grid, its cells, selection, scroll and resize, and
//! a grid for each session, which the session feeds and the renderer
//! reads while it shows. `regions`
//! writes the session's output with the prompt regions it may replace,
//! `find` searches the grid, `links` finds the web links in it, and
//! `blink` keeps the blink alacritty drops.

use std::collections::BTreeMap;
use std::sync::Mutex;

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor};
use vosh_prompt::stage::Output;

use crate::sessions::SessionId;

mod blink;
pub(crate) mod find;
pub(crate) mod links;
pub(crate) mod regions;

use blink::{Blinking, BLINK};
use find::Find;
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
    /// `pending_hold` is the line end your echo ended on, and no session
    /// output came since, so the line ends the next one holds back come
    /// after it.
    echo_held: bool,
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
            echo_held: false,
            lift_tracks: Vec::new(),
            pin_row: false,
            taken: 0,
        }
    }

    /// Keep `lines` of history above the screen, dropping the oldest
    /// past it, as Scrollback size sets it.
    pub(crate) fn set_history(&mut self, lines: usize) {
        self.term.set_options(Config {
            scrolling_history: lines,
            ..Config::default()
        });
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

    /// Drop every line above the screen, for Clear scrollback in the
    /// terminal menu. The screen keeps what it shows, the view goes back
    /// to the live tail, and a selection goes with the lines it held.
    pub(crate) fn clear_history(&mut self) {
        self.term.scroll_display(Scroll::Bottom);
        self.term.grid_mut().clear_history();
        self.term.selection = None;
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

/// One session's grid with what goes with it: whether its saved
/// scrollback went in, its find, and whether its prompt draws on bands.
#[derive(Default)]
pub(crate) struct SessionGrid {
    /// None until the session's first write, or the first frame that
    /// shows it.
    term: Option<TermGrid>,
    /// The saved scrollback went in, see [`claim_seed`].
    seeded: bool,
    find: Find,
    /// Your prompt shows lifted in this session, so each lift draws on a
    /// band.
    prompt_bands: bool,
    /// The history Scrollback size keeps, None for the grid's own 10,000
    /// until the session says.
    history: Option<usize>,
}

impl SessionGrid {
    /// The grid, None before the session's first write or frame.
    pub(crate) fn term(&self) -> Option<&TermGrid> {
        self.term.as_ref()
    }

    /// The find matches and the one you stepped to.
    pub(crate) fn find(&self) -> &Find {
        &self.find
    }

    /// Whether a band goes under each lifted prompt.
    pub(crate) fn prompt_bands(&self) -> bool {
        self.prompt_bands
    }

    /// Size the grid to `columns` by `screen_lines`, making it if it does
    /// not exist yet.
    fn size(&mut self, columns: usize, screen_lines: usize) {
        match self.term.as_mut() {
            Some(grid) => grid.resize(columns, screen_lines),
            None => self.term = Some(made(self.history, columns, screen_lines)),
        }
    }

    /// The grid a write lands in, made at 80 by 24 when no size came yet.
    fn written(&mut self) -> &mut TermGrid {
        let history = self.history;
        self.term.get_or_insert_with(|| made(history, 80, 24))
    }
}

/// A new grid that keeps `history` lines when Scrollback size set them.
fn made(history: Option<usize>, columns: usize, screen_lines: usize) -> TermGrid {
    let mut grid = TermGrid::new(columns, screen_lines);
    if let Some(lines) = history {
        grid.set_history(lines);
    }
    grid
}

/// Every session's grid, and the session whose grid shows. One Metal
/// layer draws one pane, so a frame draws the shown grid alone, and only
/// that grid takes the pointer and the size the frame gives.
struct Grids {
    shown: SessionId,
    by_session: BTreeMap<SessionId, SessionGrid>,
}

impl Grids {
    /// No grid yet, with the first session's showing.
    const fn new() -> Self {
        Self {
            shown: SessionId::FIRST,
            by_session: BTreeMap::new(),
        }
    }

    /// What `session` holds, made empty on first use.
    fn of(&mut self, session: SessionId) -> &mut SessionGrid {
        self.by_session.entry(session).or_default()
    }
}

static GRIDS: Mutex<Grids> = Mutex::new(Grids::new());

/// Run `f` on what `session` holds, under the map's lock. None only when
/// a holder of the lock panicked.
fn with_session<R>(session: SessionId, f: impl FnOnce(&mut SessionGrid) -> R) -> Option<R> {
    GRIDS.lock().ok().map(|mut grids| f(grids.of(session)))
}

/// Run `f` on the grid of `session`, under the map's lock. None before
/// the session's first write or frame.
pub(crate) fn with_grid_mut<R>(
    session: SessionId,
    f: impl FnOnce(&mut TermGrid) -> R,
) -> Option<R> {
    let mut grids = GRIDS.lock().ok()?;
    grids.by_session.get_mut(&session)?.term.as_mut().map(f)
}

/// Show the grid of `session`. From the next frame on, the frames draw
/// and size it, and the pointer acts on it.
pub(crate) fn show(session: SessionId) {
    if let Ok(mut grids) = GRIDS.lock() {
        grids.shown = session;
    }
}

/// Keep `lines` of history in the grid of `session`, now and in a grid
/// it makes later.
pub(crate) fn set_history(session: SessionId, lines: usize) {
    with_session(session, |held| {
        held.history = Some(lines);
        if let Some(grid) = held.term.as_mut() {
            grid.set_history(lines);
        }
    });
}

/// Drop the grid of `session`, which closed, with its find and its bands.
pub(crate) fn forget(session: SessionId) {
    if let Ok(mut grids) = GRIDS.lock() {
        grids.by_session.remove(&session);
    }
}

/// The session whose grid shows.
pub(crate) fn shown() -> SessionId {
    GRIDS.lock().map_or(SessionId::FIRST, |grids| grids.shown)
}

/// Resize the shown grid to fit the native surface (creating it if it
/// does not exist yet). Called by the renderer before each frame.
pub(crate) fn resize_grid(columns: usize, screen_lines: usize) {
    if let Ok(mut grids) = GRIDS.lock() {
        let shown = grids.shown;
        grids.of(shown).size(columns, screen_lines);
    }
}

/// Size the grid of `session` to the window size the page gives it,
/// while another session's grid shows. The shown grid keeps the size
/// each frame gives it.
pub(crate) fn size_hidden(session: SessionId, columns: usize, screen_lines: usize) {
    if let Ok(mut grids) = GRIDS.lock() {
        if grids.shown != session {
            grids.of(session).size(columns, screen_lines);
        }
    }
}

/// Claim the one seeding of the grid of `session` from its persisted
/// scrollback. The grid lives as long as the process, so a webview
/// reload or a remounted terminal asking again would write the history
/// a second time over a grid that already holds it (and glue the last
/// prompt to the first restored line). True on each session's first
/// call only.
pub(crate) fn claim_seed(session: SessionId) -> bool {
    with_session(session, |held| !std::mem::replace(&mut held.seeded, true)).unwrap_or(false)
}

/// Write text the webview wrote itself into the grid of `session`,
/// creating it on first use: your typed echo, a notice, the restored
/// scrollback. See [`TermGrid::local_write`]. Lock guarded, and the
/// renderer reads the same grid while it shows. Returns the newest output
/// of the prompt stage the grid took before the text, which the text
/// follows.
pub(crate) fn feed_local(session: SessionId, bytes: &[u8]) -> u64 {
    with_session(session, |held| {
        let grid = held.written();
        grid.local_write(bytes);
        grid.taken()
    })
    .unwrap_or(0)
}

/// Write one output of `session` into its grid under the lock, word
/// wrapped at the grid width, with its replace and restore. See
/// [`TermGrid::session_output`]. Every `session://output` goes through
/// here as well, so the grid holds what xterm holds. `id` names the
/// output when the prompt stage made it.
pub(crate) fn feed_session_output(session: SessionId, out: &Output, id: Option<u64>) {
    with_session(session, |held| {
        let grid = held.written();
        grid.session_output(out);
        if let Some(id) = id {
            grid.took(id);
        }
    });
}

/// Where the cursor of the grid of `session` sits and where its open
/// region starts, under the lock. None before the grid exists. See
/// [`TermGrid::cursor_report`].
pub(crate) fn cursor_report(session: SessionId) -> Option<CursorReport> {
    with_grid_mut(session, |grid| grid.cursor_report())
}

/// The live screen of the grid of `session` as text, under the lock.
/// None before the grid exists. See [`TermGrid::screen_rows`].
pub(crate) fn screen_rows(session: SessionId) -> Option<ScreenRows> {
    with_grid_mut(session, |grid| grid.screen_rows())
}

/// Current (display offset, scrollback length) of the grid of `session`,
/// for the scrollbar thumb geometry and drag mapping.
pub(crate) fn scroll_metrics(session: SessionId) -> (usize, usize) {
    with_grid_mut(session, |grid| {
        (grid.display_offset(), grid.scrollback_len())
    })
    .unwrap_or((0, 0))
}

/// Scroll the grid of `session` to an absolute display offset (0 = live
/// tail), clamped to history. Drives the scrollbar thumb drag.
pub(crate) fn scroll_to_offset(session: SessionId, target: usize) {
    with_grid_mut(session, |grid| {
        let target = target.min(grid.scrollback_len()) as i32;
        let current = grid.display_offset() as i32;
        grid.scroll(target - current);
    });
}

/// Scroll the grid of `session` by `delta` lines (positive = up into
/// scrollback).
pub(crate) fn scroll(session: SessionId, delta: i32) {
    with_grid_mut(session, |grid| grid.scroll(delta));
}

/// The fewest screen rows that open the scrollback split. A shorter
/// grid scrolls back as one full view.
pub(crate) const SPLIT_MIN_ROWS: usize = 6;

/// How many lines one page up or down moves on a grid of `rows`
/// screen rows with the divider at `split_ratio` of the height. That is
/// the whole history rows the split shows above the divider less one, so
/// the row you read last stays in view. A grid too short to split pages
/// by its rows less one. Never 0.
// Row counts are far inside f32 range, and the product is never negative.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
pub(crate) fn page_lines(rows: usize, split_ratio: f32) -> usize {
    let shown = if rows >= SPLIT_MIN_ROWS {
        ((split_ratio * rows as f32).floor() as usize).clamp(1, rows - 1)
    } else {
        rows
    };
    shown.saturating_sub(1).max(1)
}

/// Page the grid of `session` up or down by [`page_lines`], with the
/// divider at `split_ratio`. A page up from the live tail opens the split.
pub(crate) fn scroll_page(session: SessionId, up: bool, split_ratio: f32) {
    with_grid_mut(session, |grid| {
        let lines = i32::try_from(page_lines(grid.screen_lines(), split_ratio)).unwrap_or(i32::MAX);
        grid.scroll(if up { lines } else { -lines });
    });
}

/// Snap the grid of `session` to the live tail (collapses the split).
pub(crate) fn scroll_to_bottom(session: SessionId) {
    with_grid_mut(session, |grid| grid.term.scroll_display(Scroll::Bottom));
}

/// Current scrollback offset of the grid of `session` (0 = live tail, no
/// split).
pub(crate) fn current_display_offset(session: SessionId) -> usize {
    with_grid_mut(session, |grid| grid.display_offset()).unwrap_or(0)
}

/// You are selecting text in the grid of `session` or reading back in
/// it, so a clock repaint of your prompt waits and the row you select or
/// read never moves. False with no grid.
pub(crate) fn reader_busy(session: SessionId) -> bool {
    with_grid_mut(session, |grid| {
        grid.display_offset() != 0
            || grid
                .term
                .selection
                .as_ref()
                .is_some_and(|selection| !selection.is_empty())
    })
    .unwrap_or(false)
}

/// Begin a text selection anchored at a cell of the grid of `session`.
pub(crate) fn start_selection(session: SessionId, line: i32, col: usize) {
    with_grid_mut(session, |grid| grid.start_selection(line, col));
}

/// Extend the active selection to a cell of the grid of `session`.
pub(crate) fn update_selection(session: SessionId, line: i32, col: usize) {
    with_grid_mut(session, |grid| grid.extend_selection(line, col, false));
}

/// Drop the history of the grid of `session`, for Clear scrollback.
pub(crate) fn clear_history(session: SessionId) {
    with_grid_mut(session, TermGrid::clear_history);
}

/// Drop the active selection in the grid of `session`.
pub(crate) fn clear_selection(session: SessionId) {
    with_grid_mut(session, |grid| grid.term.selection = None);
}

/// Select everything in the grid of `session`, scrollback included.
/// Backs the terminal menu's Select all and Cmd+A on an empty command
/// line.
pub(crate) fn select_all(session: SessionId) {
    with_grid_mut(session, TermGrid::select_all);
}

/// The selected text in the grid of `session`, or None when there is no
/// selection.
pub(crate) fn selection_text(session: SessionId) -> Option<String> {
    with_grid_mut(session, |grid| grid.selection_text()).flatten()
}

/// Draw a band under each lifted prompt of `session`, reported by the
/// page from where your prompt shows.
pub(crate) fn set_prompt_bands(session: SessionId, on: bool) {
    with_session(session, |held| held.prompt_bands = on);
}

/// Read the shown grid with its find and its bands. The renderer calls
/// this on the main thread to build a frame.
pub(crate) fn with_shown<R>(f: impl FnOnce(Option<&SessionGrid>) -> R) -> R {
    match GRIDS.lock() {
        Ok(grids) => f(grids.by_session.get(&grids.shown)),
        Err(_) => f(None),
    }
}

/// Read the shown grid (None until the first feed).
#[cfg(test)]
pub(crate) fn with_grid<R>(f: impl FnOnce(Option<&TermGrid>) -> R) -> R {
    with_shown(|shown| f(shown.and_then(SessionGrid::term)))
}

/// Held by every test that feeds or reads the grids. They live for the
/// whole process, so two such tests on different threads would
/// otherwise see each other's rows. Each holder starts with no grid and
/// the first session's showing.
#[cfg(test)]
pub(crate) fn lock_shared_grid_for_test() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    let held = LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *GRIDS.lock().unwrap() = Grids::new();
    held
}

/// Swap a blank `columns` by `screen_lines` grid in for the shown one,
/// with no half character carried over. Call with
/// [`lock_shared_grid_for_test`] held.
#[cfg(test)]
pub(crate) fn blank_shared_grid_for_test(columns: usize, screen_lines: usize) {
    let mut grids = GRIDS.lock().unwrap();
    let shown = grids.shown;
    grids.of(shown).term = Some(TermGrid::new(columns, screen_lines));
}

/// The rows on the shown grid's screen, trailing blanks trimmed. Empty
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

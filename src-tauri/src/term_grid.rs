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
    // Keep the absolute row counter in step with every feed path so
    // wash line marks stay anchored (see the line-marks block below).
    add_abs_rows(count_newlines(&String::from_utf8_lossy(bytes)));
}

// ── Line marks: full-line wash accent bars ──────────────────────────
// Wash-highlighted lines arrive prefixed with a private OSC sequence
// `ESC ] 7770 ; R ; G ; B BEL` (session.rs append_line_result). The
// session feed intercepts the marker, records which absolute rows the
// line occupies, and the renderer converts those rows to grid lines
// each frame. Absolute rows are simply newlines fed to the grid: the
// row the cursor is writing right now is always `ABS_ROWS`, so a mark
// at absolute row A sits `ABS_ROWS - A` lines above the cursor. Marks
// do not persist across restarts (scrollback stores the line without
// its marker); the wash background itself rides the ANSI stream and
// survives everywhere.
/// One wash mark: the absolute row it lives on plus its accent color.
type LineMark = (u64, (u8, u8, u8));
static LINE_MARKS: Mutex<Vec<LineMark>> = Mutex::new(Vec::new());
static ABS_ROWS: Mutex<u64> = Mutex::new(0);
const MARK_OSC_PREFIX: &str = "\x1b]7770;";
const MARK_KEEP_ROWS: u64 = 20_000;

fn add_abs_rows(n: u64) {
    if let Ok(mut a) = ABS_ROWS.lock() {
        *a += n;
    }
}

fn abs_rows_now() -> u64 {
    ABS_ROWS.lock().map_or(0, |a| *a)
}

fn count_newlines(s: &str) -> u64 {
    s.bytes().filter(|&b| b == b'\n').count() as u64
}

fn parse_mark_rgb(params: &str) -> Option<(u8, u8, u8)> {
    let mut it = params.split(';').map(|p| p.trim().parse::<u8>().ok());
    match (it.next(), it.next(), it.next(), it.next()) {
        (Some(Some(r)), Some(Some(g)), Some(Some(b)), None) => Some((r, g, b)),
        _ => None,
    }
}

fn push_line_mark(start: u64, rows: u64, rgb: (u8, u8, u8)) {
    if let Ok(mut m) = LINE_MARKS.lock() {
        for i in 0..rows {
            m.push((start + i, rgb));
        }
        let floor = abs_rows_now().saturating_sub(MARK_KEEP_ROWS);
        m.retain(|(abs, _)| *abs >= floor);
    }
}

/// Line marks currently addressable in the grid, as (`grid_line`, rgb).
/// Converts stored absolute rows against the cursor's current row; rows
/// scrolled off the top of history drop out.
pub(crate) fn line_marks_snapshot() -> Vec<(i32, (u8, u8, u8))> {
    let Ok(slot) = grid_slot().lock() else {
        return Vec::new();
    };
    let Some(grid) = slot.as_ref() else {
        return Vec::new();
    };
    let g = grid.term.grid();
    let cursor_line = i64::from(g.cursor.point.line.0);
    let top = i64::from(g.topmost_line().0);
    let abs_now = abs_rows_now();
    let Ok(marks) = LINE_MARKS.lock() else {
        return Vec::new();
    };
    marks
        .iter()
        .filter_map(|&(abs, rgb)| {
            let delta = abs_now.checked_sub(abs)? as i64;
            let line = cursor_line - delta;
            if line >= top {
                Some((line as i32, rgb))
            } else {
                None
            }
        })
        .collect()
}

// UTF-8 tail carried between session chunks so a multi-byte character
// split across two network reads decodes whole before wrapping.
static WRAP_PENDING: Mutex<Vec<u8>> = Mutex::new(Vec::new());

/// Feed the live session's display bytes word-wrapped at the grid width.
/// xterm receives this stream word-wrapped by the frontend `WordWrapper`
/// (src/lib/wordWrap.ts); without the same treatment the surface hard-wraps
/// mid-word at the grid edge and the two renderers disagree. This is a
/// faithful port: complete lines wrap at the last whitespace before the
/// width, the chunk's partial tail flushes immediately (parity with the
/// frontend, which flushes per chunk so prompts appear), and ANSI escape
/// sequences count zero width.
pub(crate) fn feed_session_bytes(bytes: &[u8]) {
    let Ok(mut slot) = grid_slot().lock() else {
        return;
    };
    let grid = slot.get_or_insert_with(|| TermGrid::new(80, 24));
    let cols = grid.columns();
    let text = {
        let Ok(mut pending) = WRAP_PENDING.lock() else {
            return;
        };
        pending.extend_from_slice(bytes);
        match std::str::from_utf8(&pending) {
            Ok(s) => {
                let s = s.to_string();
                pending.clear();
                s
            }
            Err(e) => {
                let valid = e.valid_up_to();
                let s = String::from_utf8_lossy(&pending[..valid]).into_owned();
                // Keep at most one code point of tail; longer garbage is
                // not a split character, so let it through lossily.
                if pending.len() - valid <= 3 {
                    pending.drain(..valid);
                } else {
                    pending.clear();
                }
                s
            }
        }
    };
    if text.is_empty() {
        return;
    }
    // Intercept wash line-mark markers. Each marker precedes exactly one
    // logical line; the mark covers every row that line wraps into. The
    // pieces around markers feed separately, which is behavior-identical
    // because markers only ever sit at line starts (never mid-line).
    let mut remaining = text.as_str();
    while let Some(pos) = remaining.find(MARK_OSC_PREFIX) {
        let before = &remaining[..pos];
        if !before.is_empty() {
            let wrapped = wrap_stream(before, cols);
            grid.feed(wrapped.as_bytes());
            add_abs_rows(count_newlines(&wrapped));
        }
        let after = &remaining[pos + MARK_OSC_PREFIX.len()..];
        let Some(term_i) = after.find('\u{07}') else {
            // Marker split across chunks. The VTE parser is stateful, so
            // feed the tail as-is (the mark is lost, rendering is not).
            remaining = &remaining[pos..];
            break;
        };
        let rgb = parse_mark_rgb(&after[..term_i]);
        remaining = &after[term_i + 1..];
        let (segment, rest) = match remaining.find('\n') {
            Some(nl) => remaining.split_at(nl + 1),
            None => (remaining, ""),
        };
        let wrapped = wrap_stream(segment, cols);
        let fed_newlines = count_newlines(&wrapped);
        let rows = if segment.ends_with('\n') {
            fed_newlines.max(1)
        } else {
            fed_newlines + 1
        };
        if let Some(rgb) = rgb {
            push_line_mark(abs_rows_now(), rows, rgb);
        }
        grid.feed(wrapped.as_bytes());
        add_abs_rows(fed_newlines);
        remaining = rest;
    }
    if !remaining.is_empty() {
        let wrapped = wrap_stream(remaining, cols);
        grid.feed(wrapped.as_bytes());
        add_abs_rows(count_newlines(&wrapped));
    }
}

/// Word-wrap a chunk of terminal text at `cols`: complete lines (any \r or
/// \n terminator) wrap in place, and the trailing partial line wraps and
/// emits immediately.
fn wrap_stream(text: &str, cols: usize) -> String {
    let mut out = String::with_capacity(text.len());
    let mut line = String::new();
    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            out.push_str(&wrap_line(&line, cols));
            out.push(ch);
            line.clear();
        } else {
            line.push(ch);
        }
    }
    out.push_str(&wrap_line(&line, cols));
    out
}

/// Walk `line` once, tracking the visible column with ANSI escapes at zero
/// width. When the column exceeds `cols`, insert a CRLF at the last
/// whitespace so the wrap lands between words; a single token wider than
/// the width hard-wraps at the boundary so the line still terminates.
fn wrap_line(line: &str, cols: usize) -> String {
    enum AnsiState {
        Normal,
        Esc,
        Csi,
        Osc,
    }
    let cols = cols.max(1);
    let mut out = String::with_capacity(line.len());
    let mut visible_col = 0usize;
    // Byte position in `out` of the most recent whitespace on this line;
    // None when the line (or current wrapped segment) starts with a word.
    let mut last_ws: Option<(usize, usize)> = None; // (byte pos, visible col)
    let mut state = AnsiState::Normal;

    for ch in line.chars() {
        match state {
            AnsiState::Esc => {
                out.push(ch);
                state = match ch {
                    '[' => AnsiState::Csi,
                    ']' => AnsiState::Osc,
                    _ => AnsiState::Normal,
                };
                continue;
            }
            AnsiState::Csi => {
                out.push(ch);
                if ('\u{40}'..='\u{7e}').contains(&ch) {
                    state = AnsiState::Normal;
                }
                continue;
            }
            AnsiState::Osc => {
                out.push(ch);
                if ch == '\u{07}' || ch == '\u{9c}' {
                    state = AnsiState::Normal;
                } else if ch == '\u{1b}' {
                    state = AnsiState::Esc;
                }
                continue;
            }
            AnsiState::Normal => {}
        }
        if ch == '\u{1b}' {
            out.push(ch);
            state = AnsiState::Esc;
            continue;
        }

        out.push(ch);
        visible_col += 1;

        if ch == ' ' || ch == '\t' {
            last_ws = Some((out.len() - 1, visible_col));
        }

        if visible_col > cols {
            if let Some((ws_pos, ws_col)) = last_ws {
                // Replace the whitespace with CRLF so the wrap lands
                // between words; everything after it starts the next line.
                out.replace_range(ws_pos..=ws_pos, "\r\n");
                visible_col -= ws_col;
                last_ws = None;
            } else {
                // Single token wider than the terminal: hard-wrap at the
                // boundary, keeping the current char on the new line.
                let pos = out.len() - ch.len_utf8();
                out.insert_str(pos, "\r\n");
                visible_col = 1;
            }
        }
    }
    out
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
    fn sim_prompt_then_echo_lands_after_prompt() {
        let mut g = TermGrid::new(80, 24);
        // Server blank line then the response block (as the line pipeline
        // emits them), then the gagged prompt replaced by the rendered
        // template WITHOUT trailing newline.
        g.feed(b"\r\nPlayers matched: 9\r\n\r\n");
        g.feed(
            wrap_stream(
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

    #[test]
    fn wrap_line_breaks_at_the_last_whitespace() {
        assert_eq!(
            wrap_line("the quick brown fox", 10),
            "the quick\r\nbrown fox"
        );
    }

    #[test]
    fn wrap_line_counts_ansi_escapes_as_zero_width() {
        let line = "\x1b[33mthe quick\x1b[0m brown fox";
        assert_eq!(wrap_line(line, 10), "\x1b[33mthe quick\x1b[0m\r\nbrown fox");
    }

    #[test]
    fn wrap_line_hard_wraps_a_token_wider_than_the_terminal() {
        assert_eq!(wrap_line("abcdefgh", 5), "abcde\r\nfgh");
    }

    #[test]
    fn wrap_stream_wraps_complete_lines_and_the_partial_tail() {
        // Line-terminated content wraps in place; the unterminated tail
        // (a prompt) flushes immediately, matching the frontend wrapper.
        let out = wrap_stream("a long enough line here\r\nprompt> ", 12);
        assert_eq!(out, "a long\r\nenough line\r\nhere\r\nprompt> ");
    }

    #[test]
    fn session_feed_word_wraps_at_the_grid_width() {
        let Ok(mut slot) = grid_slot().lock() else {
            panic!("grid lock");
        };
        *slot = Some(TermGrid::new(10, 24));
        drop(slot);
        feed_session_bytes(b"the quick brown fox\r\n");
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
}

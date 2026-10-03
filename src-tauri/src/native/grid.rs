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

use std::sync::{Mutex, OnceLock};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::{Cell, Flags, Hyperlink};
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::cursor_icon::CursorIcon;
use alacritty_terminal::vte::ansi::{
    Attr, CharsetIndex, ClearMode, Color, CursorShape, CursorStyle, Handler, Hyperlink as LinkSpec,
    KeyboardModes, KeyboardModesApplyBehavior, LineClearMode, Mode, ModifyOtherKeys, NamedColor,
    PrivateMode, Processor, Rgb, StandardCharset, TabulationClearMode,
};
use regex::RegexBuilder;
use vosh_prompt::stage::{close_pin_row, Output, MARK_OSC};

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

/// The blink of SGR 5 as a mark on the cell. `alacritty_terminal` reads
/// SGR 5 and 25 and drops them, and leaves the top bit of its `Flags`
/// free, so the grid keeps blink there. A cell written while the
/// cursor's template holds the bit carries it through scrolling, reflow
/// and a saved cursor, and a reset clears it with every other style.
/// The rapid blink of SGR 6 draws steady, as xterm draws it.
pub(crate) const BLINK: Flags = Flags::from_bits_retain(1 << 15);

/// The terminal, with the blink it drops kept on the cursor's template.
/// alacritty's parser hands SGR 5 and 25 to its handler as `BlinkSlow`
/// and `CancelBlink`, in the order the parameters come, so a 5 inside a
/// color stays part of the color and a reset in the same sequence clears
/// what came before it. Every other call goes to the terminal as it
/// comes. On an update of `alacritty_terminal`, check its `Handler` for
/// new methods, since one missing here falls to the trait's empty
/// default instead of the terminal.
struct Blinking<'a>(&'a mut Term<NoopListener>);

/// Hand each listed `Handler` method to the terminal.
macro_rules! to_term {
    ($($name:ident($($arg:ident: $ty:ty),*);)*) => {
        $(
            #[inline]
            fn $name(&mut self, $($arg: $ty),*) {
                Handler::$name(&mut *self.0, $($arg),*);
            }
        )*
    };
}

impl Handler for Blinking<'_> {
    #[inline]
    fn terminal_attribute(&mut self, attr: Attr) {
        match attr {
            Attr::BlinkSlow => self.0.grid_mut().cursor.template.flags.insert(BLINK),
            Attr::CancelBlink => self.0.grid_mut().cursor.template.flags.remove(BLINK),
            attr => Handler::terminal_attribute(&mut *self.0, attr),
        }
    }

    to_term! {
        set_title(title: Option<String>);
        set_cursor_style(style: Option<CursorStyle>);
        set_cursor_shape(shape: CursorShape);
        input(c: char);
        goto(line: i32, col: usize);
        goto_line(line: i32);
        goto_col(col: usize);
        insert_blank(count: usize);
        move_up(rows: usize);
        move_down(rows: usize);
        identify_terminal(intermediate: Option<char>);
        device_status(arg: usize);
        move_forward(col: usize);
        move_backward(col: usize);
        move_down_and_cr(row: usize);
        move_up_and_cr(row: usize);
        put_tab(count: u16);
        backspace();
        carriage_return();
        linefeed();
        bell();
        substitute();
        newline();
        set_horizontal_tabstop();
        scroll_up(rows: usize);
        scroll_down(rows: usize);
        insert_blank_lines(rows: usize);
        delete_lines(rows: usize);
        erase_chars(count: usize);
        delete_chars(count: usize);
        move_backward_tabs(count: u16);
        move_forward_tabs(count: u16);
        save_cursor_position();
        restore_cursor_position();
        clear_line(mode: LineClearMode);
        clear_screen(mode: ClearMode);
        clear_tabs(mode: TabulationClearMode);
        reset_state();
        reverse_index();
        set_mode(mode: Mode);
        unset_mode(mode: Mode);
        report_mode(mode: Mode);
        set_private_mode(mode: PrivateMode);
        unset_private_mode(mode: PrivateMode);
        report_private_mode(mode: PrivateMode);
        set_scrolling_region(top: usize, bottom: Option<usize>);
        set_keypad_application_mode();
        unset_keypad_application_mode();
        set_active_charset(index: CharsetIndex);
        configure_charset(index: CharsetIndex, charset: StandardCharset);
        set_color(index: usize, color: Rgb);
        dynamic_color_sequence(prefix: String, index: usize, terminator: &str);
        reset_color(index: usize);
        clipboard_store(clipboard: u8, base64: &[u8]);
        clipboard_load(clipboard: u8, terminator: &str);
        decaln();
        push_title();
        pop_title();
        text_area_size_pixels();
        text_area_size_chars();
        set_hyperlink(link: Option<LinkSpec>);
        set_mouse_cursor_icon(icon: CursorIcon);
        report_keyboard_mode();
        push_keyboard_mode(mode: KeyboardModes);
        pop_keyboard_modes(to_pop: u16);
        set_keyboard_mode(mode: KeyboardModes, behavior: KeyboardModesApplyBehavior);
        set_modify_other_keys(mode: ModifyOtherKeys);
        report_modify_other_keys();
    }
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

/// Move the cursor from the end of a region to its start, `above` rows up
/// at `col`, then erase to the end of the screen. Nothing is written after
/// an open region, so the erase clears its old render and nothing else,
/// however many rows it took. The erase fills with the default background.
/// The cells it clears take the background in force, and a line the region
/// ends on can leave its own on while the line ends after it wait, so
/// without it the rows below would take that color too. What the replace
/// writes sets its own.
fn erase_back(above: usize, col: usize) -> Vec<u8> {
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

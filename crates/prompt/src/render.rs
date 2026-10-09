//! Draw a parsed template as ANSI text, with a span per piece.
//!
//! The renderer writes each code as the SGR sequence the first renderer
//! wrote, so a template that uses no new form draws byte for byte as
//! before, apart from the colors that now follow the theme. It tracks the
//! SGR state as it writes, and each span carries the effective look at the
//! piece's first cell. Codes take no cells.
//!
//! Named colors, color by how full, a bar's empty cells and hidden marks
//! use SGR 30 to 37 and 90 to 97, which both renderers draw from the
//! theme's palette. A hidden mark, a placeholder, a bar and the game's
//! tank bar each restore the look that was in effect before them, so they
//! never change the pieces after them.
//!
//! `%{right}` pushes what follows it on its row to the right edge: once
//! the row ends, the renderer puts spaces where the push sat, in the look
//! in effect there, so the row ends on the last of [`RenderOptions::cols`].
//! A row that does not fit, or a render with no width, gets one space.

// Bar widths are capped at 80, so the float math is exact.
#![allow(clippy::cast_precision_loss)]

mod sgr;

pub use sgr::{Color, SgrState};

use serde::Serialize;

use crate::design::{
    BarColor, Code, ColorSpec, FieldRef, Format, Layer, PieceKind, Scale, Style, Template,
    TokenKind, ValueRef,
};
use crate::values::format::{
    h_band, how_full, p_band, step_color, tank_bar_cells, Band, Resolved, Value,
};
use crate::values::Values;

/// Ends every non-empty render, so an unclosed color never bleeds into the
/// game output that follows.
pub(crate) const RESET: &str = "\x1b[0m";

/// The mark for a value the game hides.
pub(crate) const HIDDEN_MARK: &str = "?";

/// A hidden bar cell.
const HIDDEN_CELL: &str = "·";

/// The dim color of hidden marks, placeholders and a bar's empty cells,
/// SGR 90, the theme's bright black.
const DIM_FG: &str = "90";

/// How to draw.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderOptions {
    /// Draw each Missing or Absent value as its label in SGR 90, so the
    /// open editor can point at it.
    pub placeholders: bool,
    /// The columns of the terminal your prompt shows in, which `%{right}`
    /// pushes the rest of its row against. None for a render no terminal
    /// shows, such as a sample, where the push is one space.
    pub cols: Option<usize>,
}

/// A color in a span. The webview resolves palette indexes through the
/// active theme.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpanColor {
    #[default]
    Default,
    Index {
        index: u8,
    },
    Rgb {
        r: u8,
        g: u8,
        b: u8,
    },
}

/// Where a piece landed. `row` is the line from `%nl`, `col` the cell in
/// that line before any wrap, and `width` the cells it takes, a wide
/// character two and a combining mark none ([`crate::wrap::cell_width`]).
/// A piece that spans rows has one span per row. The look is the
/// effective SGR state at the piece's first cell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Span {
    pub piece: usize,
    pub row: usize,
    pub col: usize,
    pub width: usize,
    pub fg: SpanColor,
    pub bg: SpanColor,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// The span is the spaces a `%{right}` put in, so a band that cannot
    /// hold the row at the width the push reached to knows which gap to
    /// close first. Only the first `%{right}` on a row pushes.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub push: bool,
    /// The whole look at the piece's first cell, dim, the underline's
    /// kind and color, inverse, strike and blink included, so a test can
    /// check an edit kept it. The webview reads the fields above alone.
    #[serde(skip)]
    pub look: SgrState,
}

/// A drawn template.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Rendered {
    /// The bytes to write, reset terminated when not empty.
    pub ansi: String,
    /// The visible text, rows joined by `\n`.
    pub plain: String,
    /// How many rows the prompt takes before any wrap. 0 when it draws
    /// nothing.
    pub rows: usize,
    pub spans: Vec<Span>,
}

/// Where a `%{right}` sat on the row being written, so the spaces that
/// push the rest of the row right go there once it ends.
#[derive(Debug, Clone, Copy)]
struct Push {
    /// The byte in the output and in the row's text.
    out: usize,
    row: usize,
    /// The column.
    col: usize,
    /// The push's own span, which takes the spaces as its cells.
    span: usize,
}

/// Writes the output and keeps the row, column, state and spans.
struct Writer {
    out: String,
    rows: Vec<String>,
    state: SgrState,
    col: usize,
    spans: Vec<Span>,
    open: Option<(usize, SgrState)>,
    open_col: usize,
    /// The columns a push reaches to.
    cols: Option<usize>,
    /// The first push on the row being written.
    push: Option<Push>,
    /// For each row, whether a piece on it reads a field the caller
    /// asked about, see [`render_reading`].
    marks: Vec<bool>,
    /// What the piece being written sets with its own codes.
    own: Own,
}

/// The look a piece sets with its own codes, the ones right before its
/// content, as opposed to the look it takes from the pieces before it.
#[derive(Debug, Clone, Copy, Default)]
struct Own {
    /// A text color other than the default.
    fg: bool,
    dim: bool,
}

impl Writer {
    fn new(cols: Option<usize>) -> Self {
        Self {
            out: String::new(),
            rows: vec![String::new()],
            state: SgrState::default(),
            col: 0,
            spans: Vec::new(),
            open: None,
            open_col: 0,
            cols,
            push: None,
            marks: vec![false],
            own: Own::default(),
        }
    }

    /// A `%{right}` drawn at the cursor, whose span `span` is. Only the
    /// first on a row pushes. A later one takes no cells.
    fn push_right(&mut self, span: usize) {
        if self.push.is_some() {
            return;
        }
        self.push = Some(Push {
            out: self.out.len(),
            row: self.rows.last().map_or(0, String::len),
            col: self.col,
            span,
        });
    }

    /// The row ends: put the spaces of its push where the push sat, so
    /// what came after it ends on the last column, or one space when it
    /// does not fit. The spans after the push move right with it.
    fn settle_push(&mut self) {
        let Some(push) = self.push.take() else {
            return;
        };
        let after = self.col.saturating_sub(push.col);
        let pad = match self.cols {
            Some(cols) if push.col + after < cols => cols - push.col - after,
            _ => 1,
        };
        let spaces = " ".repeat(pad);
        self.out.insert_str(push.out, &spaces);
        if let Some(row) = self.rows.last_mut() {
            row.insert_str(push.row, &spaces);
        }
        if let Some(span) = self.spans.get_mut(push.span) {
            span.width = pad;
            span.push = true;
        }
        for span in self.spans.iter_mut().skip(push.span + 1) {
            span.col += pad;
        }
        if self.open.is_some() && self.open_col >= push.col {
            self.open_col += pad;
        }
        self.col += pad;
    }

    fn row(&self) -> usize {
        self.rows.len() - 1
    }

    /// A piece on the row being written reads a field the caller asked
    /// about.
    fn mark(&mut self) {
        if let Some(row) = self.marks.last_mut() {
            *row = true;
        }
    }

    fn sgr(&mut self, params: &str) {
        if params.is_empty() {
            return;
        }
        self.out.push_str("\x1b[");
        self.out.push_str(params);
        self.out.push('m');
        self.state.apply(params);
    }

    fn restore(&mut self, to: SgrState) {
        let params = self.state.transition(&to);
        self.sgr(&params);
    }

    /// Reset, then set `to` again from nothing. Used after styled game text
    /// whose codes the state may not fully track.
    fn reset_to(&mut self, to: SgrState) {
        self.sgr("0");
        let params = SgrState::default().transition(&to);
        self.sgr(&params);
    }

    fn visible(&mut self, c: char) {
        self.out.push(c);
        if let Some(row) = self.rows.last_mut() {
            row.push(c);
        }
        self.col += crate::wrap::cell_width(c);
    }

    fn line_break(&mut self, bytes: &str) {
        self.settle_push();
        self.close_span();
        self.out.push_str(bytes);
        self.rows.push(String::new());
        self.marks.push(false);
        self.col = 0;
        self.reopen_span();
    }

    /// Write text, with escape sequences at zero width. SGR sequences in it
    /// update the state. In `styled` text every line break becomes `\r\n`.
    fn text(&mut self, s: &str, styled: bool) {
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\x1b' => {
                    let mut seq = String::from(c);
                    match chars.next() {
                        Some('[') => {
                            seq.push('[');
                            let mut params = String::new();
                            let mut end = None;
                            for n in chars.by_ref() {
                                seq.push(n);
                                if ('\u{40}'..='\u{7e}').contains(&n) {
                                    end = Some(n);
                                    break;
                                }
                                params.push(n);
                            }
                            self.out.push_str(&seq);
                            if end == Some('m') {
                                self.state.apply(&params);
                            }
                        }
                        Some(']') => {
                            seq.push(']');
                            while let Some(n) = chars.next() {
                                seq.push(n);
                                if n == '\u{07}' || n == '\u{9c}' {
                                    break;
                                }
                                if n == '\x1b' && chars.peek() == Some(&'\\') {
                                    seq.push('\\');
                                    chars.next();
                                    break;
                                }
                            }
                            self.out.push_str(&seq);
                        }
                        Some(n) => {
                            seq.push(n);
                            self.out.push_str(&seq);
                        }
                        None => self.out.push_str(&seq),
                    }
                }
                '\r' | '\n' if styled => {
                    let other = if c == '\r' { '\n' } else { '\r' };
                    if chars.peek() == Some(&other) {
                        chars.next();
                    }
                    self.line_break("\r\n");
                }
                '\n' => self.line_break("\n"),
                '\r' => {
                    self.out.push('\r');
                    self.col = 0;
                }
                _ => self.visible(c),
            }
        }
    }

    /// Write `s` in SGR 90, then restore the look before it.
    fn dim(&mut self, s: &str) {
        let before = self.state;
        self.sgr(DIM_FG);
        self.text(s, false);
        self.restore(before);
    }

    fn open_span(&mut self, piece: usize) {
        self.open = Some((piece, self.state));
        self.open_col = self.col;
    }

    fn close_span(&mut self) {
        if let Some((piece, look)) = self.open {
            self.spans.push(Span {
                piece,
                row: self.row(),
                col: self.open_col,
                width: self.col.saturating_sub(self.open_col),
                fg: look.fg.span(),
                bg: look.bg.span(),
                bold: look.bold,
                italic: look.italic,
                underline: look.underline.is_some(),
                push: false,
                look,
            });
        }
    }

    fn reopen_span(&mut self) {
        self.open_col = self.col;
    }

    fn end_span(&mut self) {
        self.close_span();
        self.open = None;
    }
}

/// Draw a template.
pub fn render(template: &Template, values: &dyn Values, options: RenderOptions) -> Rendered {
    draw(template, values, options, None).0
}

/// Draw a template, and say for each row it draws whether a piece on it
/// reads a field `asks` holds true for. A value, a color that follows a
/// value and a condition that holds count on the row where they sit, so
/// a condition that fails, and what it leaves out, mark nothing. The
/// marks follow the rows the render drew, one for each.
pub fn render_reading(
    template: &Template,
    values: &dyn Values,
    options: RenderOptions,
    asks: &dyn Fn(&FieldRef) -> bool,
) -> (Rendered, Vec<bool>) {
    draw(template, values, options, Some(asks))
}

fn draw(
    template: &Template,
    values: &dyn Values,
    options: RenderOptions,
    asks: Option<&dyn Fn(&FieldRef) -> bool>,
) -> (Rendered, Vec<bool>) {
    let mut w = Writer::new(options.cols);
    let mut conditions: Vec<bool> = Vec::new();
    let tokens = template.tokens();
    let reads = |range: std::ops::Range<usize>| {
        asks.is_some_and(|asks| {
            range.into_iter().any(|token| {
                let mut hit = false;
                tokens[token]
                    .kind
                    .each_read(&mut |field| hit |= asks(field));
                hit
            })
        })
    };

    for (index, piece) in template.pieces().iter().enumerate() {
        let active = conditions.iter().all(|c| *c);
        match piece.kind {
            PieceKind::If | PieceKind::IfNot => {
                let holds = match &tokens[piece.content.start].kind {
                    TokenKind::If(field) | TokenKind::IfNot(field) => {
                        active && {
                            // A change of 0 is no change.
                            let has = match values.resolve(field) {
                                Resolved::Value(Value::Change(0)) => false,
                                Resolved::Value(_) | Resolved::Hidden => true,
                                _ => false,
                            };
                            has == (piece.kind == PieceKind::If)
                        }
                    }
                    _ => false,
                };
                if holds && reads(piece.content.clone()) {
                    w.mark();
                }
                conditions.push(holds);
                continue;
            }
            PieceKind::End => {
                conditions.pop();
                continue;
            }
            _ => {}
        }
        if !active {
            continue;
        }
        if reads(piece.codes.start..piece.content.end) {
            w.mark();
        }
        w.own = Own::default();
        for code in piece.codes.clone() {
            write_code(&mut w, template, code, values);
        }
        if piece.kind == PieceKind::Codes {
            continue;
        }
        w.open_span(index);
        let first = piece.content.start;
        match piece.kind {
            PieceKind::Text => {
                for token in piece.content.clone() {
                    match &tokens[token].kind {
                        TokenKind::Text(text) => w.text(text, false),
                        _ => w.text("%", false),
                    }
                }
            }
            PieceKind::Value => {
                if let TokenKind::Value(value) = &tokens[first].kind {
                    write_value(&mut w, template, first, value, values, options);
                }
            }
            PieceKind::CurMax => write_cur_max(&mut w, template, first, values, options),
            PieceKind::Percent => write_percent(&mut w, template, first, values, options),
            PieceKind::Nl => {
                w.end_span();
                w.line_break("\r\n");
                continue;
            }
            PieceKind::Right => {
                w.end_span();
                w.push_right(w.spans.len() - 1);
                continue;
            }
            PieceKind::Raw => {
                let field = FieldRef::new("raw");
                match values.resolve(&field) {
                    Resolved::Value(Value::Styled(raw) | Value::Text(raw)) => {
                        let before = w.state;
                        w.text(&raw, true);
                        w.reset_to(before);
                    }
                    Resolved::Hidden => w.dim(HIDDEN_MARK),
                    _ => placeholder(&mut w, values, &field, options),
                }
            }
            _ => w.text(template.token_text(first), false),
        }
        w.end_span();
    }
    w.settle_push();

    let mut ansi = w.out;
    if !ansi.is_empty() {
        ansi.push_str(RESET);
    }
    let rows = if ansi.is_empty() { 0 } else { w.rows.len() };
    let mut marks = w.marks;
    marks.truncate(rows);
    let rendered = Rendered {
        ansi,
        plain: w.rows.join("\n"),
        rows,
        spans: w.spans,
    };
    (rendered, marks)
}

/// Parse and draw a template in one step.
pub fn render_str(template: &str, values: &dyn Values, options: RenderOptions) -> Rendered {
    render(&Template::parse(template), values, options)
}

fn write_code(w: &mut Writer, template: &Template, token: usize, values: &dyn Values) {
    let TokenKind::Code(code) = &template.tokens()[token].kind else {
        return;
    };
    let (spec, layer) = match code {
        Code::Reset => {
            w.own = Own::default();
            return w.sgr("0");
        }
        Code::Style(style) => {
            match style {
                Style::Dim => w.own.dim = true,
                Style::Off => w.own.dim = false,
                _ => {}
            }
            return w.sgr(style.sgr());
        }
        Code::Fg(spec) => (spec, Layer::Fg),
        Code::Bg(spec) => (spec, Layer::Bg),
        Code::UnderlineColor(spec) => (spec, Layer::Underline),
    };
    match color_params(spec, layer, values) {
        Some(params) => {
            if layer == Layer::Fg {
                w.own.fg = *spec != ColorSpec::Default;
            }
            w.sgr(&params);
        }
        None => w.text(template.token_text(token), false),
    }
}

/// The SGR parameters for a color spec. None when a color by value names
/// a field nothing knows, or a value with no share, so the code prints as
/// written as it always did.
fn color_params(spec: &ColorSpec, layer: Layer, values: &dyn Values) -> Option<String> {
    let color = |c: Color| Some(c.layer_params(layer));
    match spec {
        ColorSpec::Named(n) => color(Color::Ansi(*n)),
        ColorSpec::Index(n) => color(Color::Index(*n)),
        ColorSpec::Rgb(r, g, b) => color(Color::Rgb(*r, *g, *b)),
        ColorSpec::Default => color(Color::Default),
        ColorSpec::ByValue { field, scale } => {
            let band = |band: Band| {
                Some(match layer {
                    Layer::Fg => band.fg().to_string(),
                    Layer::Bg => band.bg().to_string(),
                    // The underline has no bold, so a band is its color.
                    Layer::Underline => match band {
                        Band::Plain => Color::Default,
                        Band::Yellow | Band::BoldYellow => Color::Ansi(3),
                        Band::Red | Band::BoldRed => Color::Ansi(1),
                    }
                    .underline_params(),
                })
            };

            match values.resolve(field) {
                Resolved::Unknown => None,
                Resolved::Value(value) => match scale {
                    Scale::Game => value.game_percent().map(h_band).and_then(band),
                    Scale::Thirds => value
                        .fraction()
                        .and_then(|f| color(Color::Ansi(how_full(f)))),
                    Scale::Steps => value
                        .game_percent()
                        .and_then(|pct| color(Color::Index(step_color(pct)))),
                },
                _ => color(Color::Default),
            }
        }
    }
}

/// Draw a Missing or Absent value, the label in SGR 90 while the editor
/// is open and nothing otherwise.
fn placeholder(w: &mut Writer, values: &dyn Values, field: &FieldRef, options: RenderOptions) {
    if options.placeholders {
        w.dim(&values.label(field));
    }
}

/// The hidden mark for a format, `?` or a dotted bar.
fn hidden_mark(field: &FieldRef, format: &Format) -> String {
    match format {
        Format::Bar { width, .. } => HIDDEN_CELL.repeat(usize::from(*width)),
        Format::Game if field.name == "tank_hp" => {
            let third = HIDDEN_CELL.repeat(3);
            format!("[{third}|{third}|{third}|{third}]")
        }
        _ => HIDDEN_MARK.to_string(),
    }
}

fn write_value(
    w: &mut Writer,
    template: &Template,
    token: usize,
    value: &ValueRef,
    values: &dyn Values,
    options: RenderOptions,
) {
    let field = &value.field;
    match values.resolve(field) {
        Resolved::Unknown => w.text(template.token_text(token), false),
        Resolved::Hidden => w.dim(&hidden_mark(field, &value.format)),
        Resolved::Absent if value.format == Format::Off => {
            w.text(&values.label(field), false);
        }
        Resolved::Absent | Resolved::Missing => placeholder(w, values, field, options),
        Resolved::Value(v) => {
            if !write_formatted(w, &v, &value.format, values, &values.label(field)) {
                w.text(template.token_text(token), false);
            }
        }
    }
}

/// Draw a value in a format. False when the format does not apply.
fn write_formatted(
    w: &mut Writer,
    value: &Value,
    format: &Format,
    values: &dyn Values,
    label: &str,
) -> bool {
    match (format, value) {
        (Format::Bar { width, color }, _) => {
            if !value.has_bar() {
                return false;
            }
            write_bar(w, value, usize::from(*width), color, values);
        }
        (Format::Game, Value::TankHp(pct)) => write_tank_bar(w, *pct),
        (Format::Game, Value::Level { word, level }) => {
            let before = w.state;
            w.sgr("38;5;240");
            w.text(&format!("({word} {level})"), false);
            w.restore(before);
        }
        (Format::Value | Format::Zero | Format::PlusMinus, Value::Change(n)) => {
            let Some(text) = value.text(format, label) else {
                return false;
            };
            write_change(w, *n, &text);
        }
        (Format::Value | Format::Game, Value::Styled(raw)) => {
            let before = w.state;
            w.text(raw, true);
            w.reset_to(before);
        }
        _ => match value.text(format, label) {
            Some(text) => w.text(&text, false),
            None => return false,
        },
    }
    true
}

/// A change of a vital, in the theme's green for a gain and red for a
/// loss at normal intensity. A text color the value's own piece sets
/// wins, and so does dim when the piece sets it. A color or dim the
/// pieces before it leave does not count, and the look comes back after
/// it. A zero takes the look around it.
fn write_change(w: &mut Writer, n: i64, text: &str) {
    let before = w.state;
    let sign = match n.signum() {
        1 => Some(Color::Ansi(2)),
        -1 => Some(Color::Ansi(1)),
        _ => None,
    };
    match sign.filter(|_| !w.own.fg && !text.is_empty()) {
        Some(color) => {
            let mut look = before;
            look.fg = color;
            look.dim = w.own.dim;
            w.restore(look);
            w.text(text, false);
            w.restore(before);
        }
        None => w.text(text, false),
    }
}

fn write_bar(w: &mut Writer, value: &Value, width: usize, color: &BarColor, values: &dyn Values) {
    let before = w.state;
    let fraction = value.fraction();
    let filled = fraction.map_or(0, |f| (f * width as f64).round() as usize);
    let empty = width.saturating_sub(filled);
    if filled > 0 {
        let fill = match color {
            BarColor::Auto => Color::Ansi(how_full(fraction.unwrap_or(0.0))).params(false),
            BarColor::Game => value
                .game_percent()
                .map_or(Band::Plain, p_band)
                .fg()
                .to_string(),
            BarColor::Color(spec) => color_params(spec, Layer::Fg, values)
                .unwrap_or_else(|| Color::Ansi(2).params(false)),
        };
        w.sgr(&fill);
        w.text(&"█".repeat(filled), false);
    }
    if empty > 0 {
        w.sgr(DIM_FG);
        w.text(&"░".repeat(empty), false);
    }
    w.restore(before);
}

/// The game's `%P` bar, drawn from the integer percent as `health_prompt`
/// draws it. Its colors restore the look before them at each divider.
fn write_tank_bar(w: &mut Writer, pct: i64) {
    let before = w.state;
    let band = p_band(pct);
    let colored = band != Band::Plain;
    w.text("[", false);
    if colored {
        w.sgr(band.fg());
    }
    for (i, full) in tank_bar_cells(pct).into_iter().enumerate() {
        if i != 0 && i % 3 == 0 {
            if colored {
                w.restore(before);
            }
            w.text("|", false);
            if colored {
                w.sgr(band.fg());
            }
        }
        w.text(if full { "=" } else { "-" }, false);
    }
    if colored {
        w.restore(before);
    }
    w.text("]", false);
}

/// `%X/%{maxX}`. Hidden draws `?/?`. With no value the whole piece draws
/// nothing, slash included.
fn write_cur_max(
    w: &mut Writer,
    template: &Template,
    first: usize,
    values: &dyn Values,
    options: RenderOptions,
) {
    let tokens = template.tokens();
    let (TokenKind::Value(cur), TokenKind::Value(max)) =
        (&tokens[first].kind, &tokens[first + 2].kind)
    else {
        return;
    };
    match values.resolve(&cur.field) {
        Resolved::Unknown => {
            w.text(template.token_text(first), false);
            w.text("/", false);
            write_value(w, template, first + 2, max, values, options);
        }
        Resolved::Hidden => w.dim("?/?"),
        Resolved::Absent | Resolved::Missing => placeholder(w, values, &cur.field, options),
        Resolved::Value(_) => {
            write_value(w, template, first, cur, values, options);
            w.text("/", false);
            write_value(w, template, first + 2, max, values, options);
        }
    }
}

/// `%pct_X%%`. Hidden draws `?%`. With no value the whole piece draws
/// nothing, sign included.
fn write_percent(
    w: &mut Writer,
    template: &Template,
    first: usize,
    values: &dyn Values,
    options: RenderOptions,
) {
    let TokenKind::Value(pct) = &template.tokens()[first].kind else {
        return;
    };
    match values.resolve(&pct.field) {
        Resolved::Hidden => w.dim("?%"),
        Resolved::Absent | Resolved::Missing => placeholder(w, values, &pct.field, options),
        Resolved::Unknown | Resolved::Value(_) => {
            write_value(w, template, first, pct, values, options);
            w.text("%", false);
        }
    }
}

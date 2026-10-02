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

// Bar widths are capped at 80, so the float math is exact.
#![allow(clippy::cast_precision_loss)]

use std::collections::BTreeMap;

use chrono::NaiveDateTime;
use serde::Serialize;

use crate::format::{h_band, how_full, p_band, step_color, tank_bar_cells, Band, Resolved, Value};
use crate::template::{
    BarColor, Code, ColorSpec, FieldRef, Format, PieceKind, Scale, Template, TokenKind,
    UnderlineStyle, ValueRef,
};

/// Ends every non-empty render, so an unclosed color never bleeds into the
/// game output that follows.
pub const RESET: &str = "\x1b[0m";

/// The mark for a value the game hides.
pub const HIDDEN_MARK: &str = "?";

/// A hidden bar cell.
const HIDDEN_CELL: &str = "·";

/// The dim color of hidden marks, placeholders and a bar's empty cells,
/// SGR 90, the theme's bright black.
const DIM_FG: &str = "90";

/// What the renderer asks about each field.
pub trait Values {
    fn resolve(&self, field: &FieldRef) -> Resolved;

    /// The field's label, drawn as a placeholder and by the `on` and `off`
    /// formats.
    fn label(&self, field: &FieldRef) -> String {
        field.to_string()
    }
}

/// How to draw.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderOptions {
    /// Draw each Missing or Absent value as its label in SGR 90, so the
    /// open editor can point at it.
    pub placeholders: bool,
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

/// A color in the SGR state, kept in the form it was written so a restore
/// writes the same form back.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Color {
    #[default]
    Default,
    /// SGR 30 to 37 and 90 to 97 (40 to 47 and 100 to 107), 0 to 15.
    Ansi(u8),
    /// A 256 palette index.
    Index(u8),
    Rgb(u8, u8, u8),
}

impl Color {
    fn params(self, background: bool) -> String {
        let base = if background { 40 } else { 30 };
        match self {
            Color::Default => (base + 9).to_string(),
            Color::Ansi(n) if n < 8 => (base + u32::from(n)).to_string(),
            Color::Ansi(n) => (base + 60 + u32::from(n - 8)).to_string(),
            Color::Index(n) => format!("{};5;{n}", base + 8),
            Color::Rgb(r, g, b) => format!("{};2;{r};{g};{b}", base + 8),
        }
    }

    /// The SGR parameters for the underline's color. SGR 58 has no short
    /// form for a theme color, so one goes as its palette index, and a
    /// true color names its empty color space as terminals expect.
    fn underline_params(self) -> String {
        match self {
            Color::Default => "59".to_string(),
            Color::Ansi(n) | Color::Index(n) => format!("58:5:{n}"),
            Color::Rgb(r, g, b) => format!("58:2::{r}:{g}:{b}"),
        }
    }

    fn layer_params(self, layer: Layer) -> String {
        match layer {
            Layer::Fg => self.params(false),
            Layer::Bg => self.params(true),
            Layer::Underline => self.underline_params(),
        }
    }

    fn span(self) -> SpanColor {
        match self {
            Color::Default => SpanColor::Default,
            Color::Ansi(index) | Color::Index(index) => SpanColor::Index { index },
            Color::Rgb(r, g, b) => SpanColor::Rgb { r, g, b },
        }
    }
}

/// What a color code paints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layer {
    Fg,
    Bg,
    Underline,
}

/// An extended color, `5;n` or `2;r;g;b`, from the numbers after a 38,
/// 48 or 58. A colon form may name a color space first, `2::r:g:b`, and
/// `colon` says the numbers came that way. Returns the color and how many
/// numbers it took.
fn extended_color(nums: &[u32], colon: bool) -> (Option<Color>, usize) {
    let byte = |n: Option<&u32>| n.copied().unwrap_or(0).min(255) as u8;
    match nums.first() {
        Some(5) => (nums.get(1).map(|n| Color::Index(byte(Some(n)))), 2),
        Some(2) => {
            let at = if colon && nums.len() > 4 { 2 } else { 1 };
            let rgb = Color::Rgb(
                byte(nums.get(at)),
                byte(nums.get(at + 1)),
                byte(nums.get(at + 2)),
            );
            (Some(rgb), 4)
        }
        _ => (None, 1),
    }
}

/// The SGR state the renderer tracks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SgrState {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    /// The kind of underline, None with no underline.
    pub underline: Option<UnderlineStyle>,
    /// The underline's color, Default for the text's own.
    pub underline_color: Color,
    pub inverse: bool,
    pub strike: bool,
    /// SGR 5. The rapid 6 draws steady in every renderer, as xterm
    /// draws it, so it is no blink here either.
    pub blink: bool,
}

impl SgrState {
    /// Apply an SGR parameter list such as `0;1;31`, `38;5;240` or
    /// `4:3;58:2::191:97:106`. A parameter with colons is one code with
    /// its sub parameters, so `4:3` is a curly underline and never an
    /// underline and an italic.
    pub fn apply(&mut self, params: &str) {
        let num = |p: &str| {
            if p.is_empty() {
                0
            } else {
                p.parse::<u32>().unwrap_or(u32::MAX)
            }
        };
        let groups: Vec<Vec<u32>> = params
            .split(';')
            .map(|group| group.split(':').map(num).collect())
            .collect();
        let mut i = 0;
        while i < groups.len() {
            let n = groups[i][0];
            let subs = &groups[i][1..];
            match n {
                0 => *self = SgrState::default(),
                1 => self.bold = true,
                2 => self.dim = true,
                3 => self.italic = true,
                4 => {
                    self.underline = match subs.first() {
                        Some(kind) => UnderlineStyle::from_sgr(*kind),
                        None => Some(UnderlineStyle::Single),
                    };
                }
                5 => self.blink = true,
                7 => self.inverse = true,
                9 => self.strike = true,
                21 => self.underline = Some(UnderlineStyle::Double),
                22 => {
                    self.bold = false;
                    self.dim = false;
                }
                23 => self.italic = false,
                24 => self.underline = None,
                25 => self.blink = false,
                27 => self.inverse = false,
                29 => self.strike = false,
                30..=37 => self.fg = Color::Ansi((n - 30) as u8),
                39 => self.fg = Color::Default,
                40..=47 => self.bg = Color::Ansi((n - 40) as u8),
                49 => self.bg = Color::Default,
                59 => self.underline_color = Color::Default,
                90..=97 => self.fg = Color::Ansi((n - 90 + 8) as u8),
                100..=107 => self.bg = Color::Ansi((n - 100 + 8) as u8),
                38 | 48 | 58 => {
                    let color = if subs.is_empty() {
                        // The semicolon form: the color takes the
                        // parameters after this one.
                        let rest: Vec<u32> = groups[i + 1..].iter().map(|g| g[0]).collect();
                        let (color, used) = extended_color(&rest, false);
                        i += used.min(rest.len());
                        color
                    } else {
                        extended_color(subs, true).0
                    };
                    if let Some(color) = color {
                        match n {
                            38 => self.fg = color,
                            48 => self.bg = color,
                            _ => self.underline_color = color,
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    /// The fewest SGR parameters that turn this state into `to`. Empty
    /// when they match.
    pub fn transition(&self, to: &SgrState) -> String {
        let mut p: Vec<String> = Vec::new();
        if (self.bold && !to.bold) || (self.dim && !to.dim) {
            p.push("22".into());
            if to.bold {
                p.push("1".into());
            }
            if to.dim {
                p.push("2".into());
            }
        } else {
            if to.bold && !self.bold {
                p.push("1".into());
            }
            if to.dim && !self.dim {
                p.push("2".into());
            }
        }
        if self.italic != to.italic {
            p.push(if to.italic { "3" } else { "23" }.into());
        }
        if self.underline != to.underline {
            p.push(to.underline.map_or("24", UnderlineStyle::sgr).into());
        }
        for (from, want, on, off) in [
            (self.blink, to.blink, "5", "25"),
            (self.inverse, to.inverse, "7", "27"),
            (self.strike, to.strike, "9", "29"),
        ] {
            if from != want {
                p.push(if want { on } else { off }.into());
            }
        }
        if self.fg != to.fg {
            p.push(to.fg.params(false));
        }
        if self.bg != to.bg {
            p.push(to.bg.params(true));
        }
        if self.underline_color != to.underline_color {
            p.push(to.underline_color.underline_params());
        }
        p.join(";")
    }
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
}

impl Writer {
    fn new() -> Self {
        Self {
            out: String::new(),
            rows: vec![String::new()],
            state: SgrState::default(),
            col: 0,
            spans: Vec::new(),
            open: None,
            open_col: 0,
        }
    }

    fn row(&self) -> usize {
        self.rows.len() - 1
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
        self.close_span();
        self.out.push_str(bytes);
        self.rows.push(String::new());
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
    let mut w = Writer::new();
    let mut conditions: Vec<bool> = Vec::new();
    let tokens = template.tokens();

    for (index, piece) in template.pieces().iter().enumerate() {
        let active = conditions.iter().all(|c| *c);
        match piece.kind {
            PieceKind::If | PieceKind::IfNot => {
                let holds = match &tokens[piece.content.start].kind {
                    TokenKind::If(field) | TokenKind::IfNot(field) => {
                        active && {
                            let has = matches!(
                                values.resolve(field),
                                Resolved::Value(_) | Resolved::Hidden
                            );
                            has == (piece.kind == PieceKind::If)
                        }
                    }
                    _ => false,
                };
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

    let mut ansi = w.out;
    if !ansi.is_empty() {
        ansi.push_str(RESET);
    }
    let rows = if ansi.is_empty() { 0 } else { w.rows.len() };
    Rendered {
        ansi,
        plain: w.rows.join("\n"),
        rows,
        spans: w.spans,
    }
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
        Code::Reset => return w.sgr("0"),
        Code::Style(style) => return w.sgr(style.sgr()),
        Code::Fg(spec) => (spec, Layer::Fg),
        Code::Bg(spec) => (spec, Layer::Bg),
        Code::UnderlineColor(spec) => (spec, Layer::Underline),
    };
    match color_params(spec, layer, values) {
        Some(params) => w.sgr(&params),
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

/// Values from a plain map of prompt vars, the way the first renderer read
/// them. A number with a max under any of its spellings (`mhp`, `hp_max`,
/// `max_hp`, `maxhp`) is a gauge. A name the map lacks is unknown, so its
/// token prints as written. `time` and `date` read the clock given.
pub struct MapValues<'a> {
    vars: &'a BTreeMap<String, String>,
    now: NaiveDateTime,
}

impl<'a> MapValues<'a> {
    pub fn new(vars: &'a BTreeMap<String, String>, now: NaiveDateTime) -> Self {
        Self { vars, now }
    }

    /// Read the local clock now.
    pub fn now(vars: &'a BTreeMap<String, String>) -> Self {
        Self::new(vars, chrono::Local::now().naive_local())
    }

    fn number(&self, key: &str) -> Option<Value> {
        Value::parse_number(self.vars.get(key)?)
    }

    fn max_of(&self, name: &str) -> Option<Value> {
        max_spellings(name).iter().find_map(|key| self.number(key))
    }
}

/// The names a prompt var's max goes by, in the order the first renderer
/// tried them: `mhp`, `hp_max`, `max_hp`, `maxhp`.
pub(crate) fn max_spellings(name: &str) -> [String; 4] {
    [
        format!("m{name}"),
        format!("{name}_max"),
        format!("max_{name}"),
        format!("max{name}"),
    ]
}

impl Values for MapValues<'_> {
    fn resolve(&self, field: &FieldRef) -> Resolved {
        if field.param.is_some() {
            return Resolved::Unknown;
        }
        let name = field.name.as_str();
        match name {
            "time" | "date" => {
                return Resolved::Value(Value::Clock {
                    at: self.now,
                    date: name == "date",
                })
            }
            _ => {}
        }
        let Some(raw) = self.vars.get(name) else {
            return Resolved::Unknown;
        };
        Resolved::Value(match Value::parse_number(raw) {
            Some(cur) => match self.max_of(name) {
                Some(max) => cur.over(&max, None).unwrap_or(cur),
                None => cur,
            },
            None => Value::Text(raw.clone()),
        })
    }
}

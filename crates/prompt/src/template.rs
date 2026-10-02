//! The prompt template grammar, version 2.
//!
//! A template is text with codes in it. The tokenizer keeps every form the
//! first grammar accepted (moved from `src-tauri/src/prompt_template.rs`)
//! and adds the forms of the prompt editor.
//!
//! ```text
//! %%                       a literal percent sign
//! %name  %{name}           a value, `name` is [A-Za-z0-9_], lowercased
//! %pct_name                the value as a percent of its max
//! %name_bar[:W[:C]]        a bar W cells wide (default 10, 1 to 80) in color C
//! %bar_name[:W[:C]]        the same bar
//! %maxname                 the max, a field of its own
//! %c_<spec> %{c:<spec>}    foreground: a theme color name, a 256 index,
//!                          #rrggbb, r,g,b, `default`, `reset`, or a field
//!                          name to color by how full it is
//! %{c:hp:game}             foreground by the game's own %h bands
//! %{c:hp:steps}            foreground in eleven steps from red to green,
//!                          one for each tenth
//! %bg_<spec> %{bg:<spec>}  background, same specs
//! %{ul:<spec>}             the underline's color, same specs. Braced only,
//!                          so a value named `ul_...` stays a value
//! %s_<style> %{s:<style>}  bold dim italic underline inverse strike blink
//!                          off reset, and the underline kinds double curly
//!                          dotted dashed
//! %{field:format:args}     a value in a format (see [`Format`])
//! %{field:param:format}    for the fields that take a parameter, `aff`,
//!                          `member_*`, `queue` and `gmcp`
//! %{if:x} %{ifnot:x}       draw what follows up to %{end} only when x has a
//! %{end}                   value (or is hidden), or only when it has none
//! %nl %{nl}                a line break
//! %{raw}                   the game's own prompt, colors kept
//! ```
//!
//! A `%` followed by anything else stays literal, so `%)h` prints `%)h`.
//! A code the grammar does not know becomes an [`TokenKind::Unknown`] token
//! and prints as written, so a typo stays visible.
//!
//! [`Template::parse`] also groups tokens into pieces, the parts the editor
//! shows. A piece is a run of color and style codes followed by one value
//! or one run of text. Two runs fold into one piece. `%X/%{maxX}` is a
//! current and max piece, and `%pct_X%%` a percent piece.

use std::collections::BTreeSet;
use std::fmt;
use std::ops::Range;

/// Bar width when the template does not give one.
pub const BAR_DEFAULT_WIDTH: u8 = 10;
/// The widest bar a template can ask for.
pub const BAR_MAX_WIDTH: u8 = 80;

/// A field the template reads, with its parameter for the fields that take
/// one (`aff:sanctuary`, `member_hp:quenby`, `queue:bugs`,
/// `gmcp:Char.Vitals.ep`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldRef {
    /// The field name, lowercased.
    pub name: String,
    /// The parameter, as written for `aff`, `member_*` and `gmcp` (which
    /// match it in any case), lowercased for `queue`.
    pub param: Option<String>,
}

impl FieldRef {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            param: None,
        }
    }

    pub fn with_param(name: impl Into<String>, param: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            param: Some(param.into()),
        }
    }
}

impl fmt::Display for FieldRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.param {
            Some(param) => write!(f, "{}:{param}", self.name),
            None => f.write_str(&self.name),
        }
    }
}

/// True for the fields written with a parameter, `%{aff:sanctuary}`.
pub fn takes_param(name: &str) -> bool {
    matches!(name, "aff" | "queue" | "gmcp") || name.starts_with("member_")
}

/// True when the parameter keeps the case it was written in.
fn param_keeps_case(name: &str) -> bool {
    matches!(name, "aff" | "gmcp") || name.starts_with("member_")
}

/// A color the template names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColorSpec {
    /// A theme color, ANSI 0 to 15 (`%c_green` is 2, `%c_gray` is 8). These
    /// follow the theme's palette.
    Named(u8),
    /// A 256 palette index (`%c_42`).
    Index(u8),
    /// A true color (`%{c:#80c8ff}`, `%{c:128,200,255}`).
    Rgb(u8, u8, u8),
    /// The terminal's own color, SGR 39 or 49 (`%c_default`).
    Default,
    /// Color by how full a field is, on the scale `scale` names.
    ByValue { field: FieldRef, scale: Scale },
}

/// How a color by value turns how full a field is into a color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scale {
    /// The theme's green from two thirds, yellow from one third and red
    /// below (`%c_hp`).
    Thirds,
    /// The game's own `%h` bands (`%{c:hp:game}`).
    Game,
    /// Eleven 256 colors from red to green, one for each tenth of the
    /// percent as integer division takes it, as the old tt++ prompt
    /// colored its percents (`%{c:hp:steps}`).
    Steps,
}

/// The line an underline draws. One kind holds at a time, so turning one
/// on replaces another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnderlineStyle {
    /// One straight line, SGR 4.
    Single,
    /// Two straight lines, SGR 4:2.
    Double,
    /// A wavy line, SGR 4:3.
    Curly,
    /// A dotted line, SGR 4:4.
    Dotted,
    /// A dashed line, SGR 4:5.
    Dashed,
}

impl UnderlineStyle {
    /// The SGR parameter the kind writes. Single stays the plain `4`
    /// every terminal reads.
    pub fn sgr(self) -> &'static str {
        match self {
            UnderlineStyle::Single => "4",
            UnderlineStyle::Double => "4:2",
            UnderlineStyle::Curly => "4:3",
            UnderlineStyle::Dotted => "4:4",
            UnderlineStyle::Dashed => "4:5",
        }
    }

    /// The kind an SGR `4:n` names, None for `4:0`, which turns it off.
    /// A kind no terminal names draws the single line, as terminals do.
    pub fn from_sgr(n: u32) -> Option<UnderlineStyle> {
        Some(match n {
            0 => return None,
            2 => UnderlineStyle::Double,
            3 => UnderlineStyle::Curly,
            4 => UnderlineStyle::Dotted,
            5 => UnderlineStyle::Dashed,
            _ => UnderlineStyle::Single,
        })
    }
}

/// A text style code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Bold,
    Dim,
    Italic,
    /// An underline of one kind, `%s_underline` the single line.
    Underline(UnderlineStyle),
    Inverse,
    Strike,
    /// Blinking text, SGR 5.
    Blink,
    /// Every style off, colors kept (SGR 22;23;24;25;27;29).
    Off,
}

impl Style {
    /// The SGR parameters the style writes.
    pub fn sgr(self) -> &'static str {
        match self {
            Style::Bold => "1",
            Style::Dim => "2",
            Style::Italic => "3",
            Style::Underline(line) => line.sgr(),
            Style::Inverse => "7",
            Style::Strike => "9",
            Style::Blink => "5",
            Style::Off => "22;23;24;25;27;29",
        }
    }
}

/// A color or style code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Code {
    Fg(ColorSpec),
    Bg(ColorSpec),
    /// The color of the underline (SGR 58, or 59 for the text's own).
    /// It shows only while an underline is on, and `%s_off` keeps it.
    UnderlineColor(ColorSpec),
    Style(Style),
    /// Every color and style off (SGR 0), from `%c_reset`, `%bg_reset` or
    /// `%s_reset`.
    Reset,
}

/// The fill color of a bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BarColor {
    /// By how full the bar is.
    Auto,
    /// By the game's tank bar bands (yellow under 75, red under 25, bold
    /// red under 5 percent).
    Game,
    Color(ColorSpec),
}

/// How a value is drawn (section 1.4 of the build spec).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Format {
    /// The value itself.
    Value,
    /// The max of a gauge.
    Max,
    /// A rounded percent with no sign.
    Pct,
    /// A bar of `█` and `░` cells.
    Bar { width: u8, color: BarColor },
    /// Exactly as the game prints it, colors included.
    Game,
    /// An enum as a word, the hour as `2 pm`.
    Word,
    /// The hour on a 12 hour clock with no space, `3PM`, `12AM`.
    Ampm,
    /// A moon's phase name, a member's name.
    Name,
    /// A number with thousands separators, `1,250`.
    Grouped,
    /// A number in short form, `1.2k`.
    Short,
    /// A number in thousands with one decimal and a capital K, `12.3K`.
    Thousands,
    /// With its unit, `14s`, `61°F`.
    Unit,
    /// The seconds since the tick, counting up, `16s`.
    Since,
    /// The first N characters.
    Trunc(usize),
    /// A clock as `08:42`.
    Hm,
    /// A clock as `08:42:10`.
    Hms,
    /// A date as `Sep 29`.
    Md,
    /// How many names a list holds.
    Count,
    /// The names of a list, comma separated.
    Names,
    /// The label when the flag holds, nothing otherwise.
    On,
    /// The label when the flag does not hold, nothing otherwise.
    Off,
}

/// A value in a format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueRef {
    pub field: FieldRef,
    pub format: Format,
}

/// One lexical unit of a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// Literal text. A `%` that starts no token is part of it.
    Text(String),
    /// `%%`, a literal percent sign.
    Percent,
    Code(Code),
    Value(ValueRef),
    If(FieldRef),
    IfNot(FieldRef),
    End,
    /// `%nl`, a line break.
    Nl,
    /// `%{raw}`, the game's prompt as it arrived.
    Raw,
    /// A code the grammar does not know. It prints as written.
    Unknown,
}

/// A token with its byte range in the template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    pub kind: TokenKind,
}

/// What a piece holds after its leading codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieceKind {
    /// Codes with nothing after them, at the end of the template or before
    /// a line break or a condition.
    Codes,
    /// A run of text and `%%`.
    Text,
    /// One value.
    Value,
    /// `%X/%{maxX}`, a value, a slash and its own max.
    CurMax,
    /// `%pct_X%%`, a percent and its sign.
    Percent,
    Nl,
    Raw,
    If,
    IfNot,
    End,
    Unknown,
}

/// One part of a template as the editor shows it. `codes` and `content`
/// index [`Template::tokens`] and sit next to each other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    /// Byte range in the template, codes included.
    pub start: usize,
    pub end: usize,
    pub codes: Range<usize>,
    pub content: Range<usize>,
    pub kind: PieceKind,
}

/// A parsed template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    source: String,
    tokens: Vec<Token>,
    pieces: Vec<Piece>,
}

impl Template {
    /// Parse a template. Parsing never fails. Anything malformed stays
    /// literal or becomes an unknown token that prints as written.
    pub fn parse(source: &str) -> Self {
        let tokens = tokenize(source);
        let pieces = group(&tokens);
        Self {
            source: source.to_string(),
            tokens,
            pieces,
        }
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn is_empty(&self) -> bool {
        self.source.is_empty()
    }

    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    pub fn pieces(&self) -> &[Piece] {
        &self.pieces
    }

    /// The template text of one token, exactly as written.
    pub fn token_text(&self, index: usize) -> &str {
        let token = &self.tokens[index];
        &self.source[token.start..token.end]
    }

    /// The template text of one piece, codes included.
    pub fn piece_text(&self, index: usize) -> &str {
        let piece = &self.pieces[index];
        &self.source[piece.start..piece.end]
    }

    /// Every field the template reads, in values, conditions and colors
    /// by value. `%{raw}` reads the field `raw`.
    pub fn reads(&self) -> BTreeSet<FieldRef> {
        let mut out = BTreeSet::new();
        for token in &self.tokens {
            match &token.kind {
                TokenKind::Value(value) => {
                    out.insert(value.field.clone());
                    if let Format::Bar {
                        color: BarColor::Color(ColorSpec::ByValue { field, .. }),
                        ..
                    } = &value.format
                    {
                        out.insert(field.clone());
                    }
                }
                TokenKind::Code(
                    Code::Fg(ColorSpec::ByValue { field, .. })
                    | Code::Bg(ColorSpec::ByValue { field, .. })
                    | Code::UnderlineColor(ColorSpec::ByValue { field, .. }),
                )
                | TokenKind::If(field)
                | TokenKind::IfNot(field) => {
                    out.insert(field.clone());
                }
                TokenKind::Raw => {
                    out.insert(FieldRef::new("raw"));
                }
                _ => {}
            }
        }
        out
    }
}

/// The characters a braced body may hold.
fn brace_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | ',' | '#' | '.' | '=' | '[' | ']')
}

fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// True for a field name the grammar can hold, `[a-z0-9_]+`.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Split a template into tokens.
pub fn tokenize(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut text = String::new();
    let mut text_start = 0;
    let mut i = 0;

    let flush = |out: &mut Vec<Token>, text: &mut String, text_start: usize, end: usize| {
        if !text.is_empty() {
            out.push(Token {
                start: text_start,
                end,
                kind: TokenKind::Text(std::mem::take(text)),
            });
        }
    };

    while i < source.len() {
        let ch = source[i..].chars().next().unwrap_or('\0');
        if ch != '%' {
            if text.is_empty() {
                text_start = i;
            }
            text.push(ch);
            i += ch.len_utf8();
            continue;
        }
        let next = bytes.get(i + 1).copied();
        if next == Some(b'%') {
            flush(&mut out, &mut text, text_start, i);
            out.push(Token {
                start: i,
                end: i + 2,
                kind: TokenKind::Percent,
            });
            i += 2;
            continue;
        }
        if next == Some(b'{') {
            if let Some(close_rel) = source[i + 2..].find('}') {
                let body = &source[i + 2..i + 2 + close_rel];
                if !body.is_empty() && body.chars().all(brace_char) {
                    flush(&mut out, &mut text, text_start, i);
                    let mut end = i + 2 + close_rel + 1;
                    let kind = parse_braced(body, source, &mut end);
                    out.push(Token {
                        start: i,
                        end,
                        kind,
                    });
                    i = end;
                    continue;
                }
            }
            if text.is_empty() {
                text_start = i;
            }
            text.push('%');
            i += 1;
            continue;
        }
        let run = source[i + 1..]
            .find(|c: char| !name_char(c))
            .unwrap_or(source.len() - i - 1);
        if run == 0 {
            if text.is_empty() {
                text_start = i;
            }
            text.push('%');
            i += 1;
            continue;
        }
        flush(&mut out, &mut text, text_start, i);
        let name = source[i + 1..i + 1 + run].to_ascii_lowercase();
        let mut end = i + 1 + run;
        let kind = parse_name(&name, source, &mut end);
        out.push(Token {
            start: i,
            end,
            kind,
        });
        i = end;
    }
    flush(&mut out, &mut text, text_start, source.len());
    out
}

/// The field a legacy bar name reads, `hp` for `hp_bar` and `bar_hp`.
fn bar_base(name: &str) -> Option<&str> {
    if let Some(base) = name.strip_suffix("_bar") {
        return (!base.is_empty()).then_some(base);
    }
    if let Some(base) = name.strip_prefix("bar_") {
        return (!base.is_empty()).then_some(base);
    }
    None
}

/// Consume the legacy `:W:C` bar parameters that follow a bar token, as
/// the first grammar did. A first `:` is taken even with no digits after
/// it, and the color is a run of letters.
fn consume_bar_params(source: &str, end: &mut usize) -> (u8, BarColor) {
    let mut width = BAR_DEFAULT_WIDTH;
    let mut color = BarColor::Auto;
    let tail = &source[*end..];
    let Some(after_colon) = tail.strip_prefix(':') else {
        return (width, color);
    };
    let digits_len = after_colon
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(after_colon.len());
    let digits = &after_colon[..digits_len];
    let mut consumed = 1 + digits_len;
    if let Ok(w) = digits.parse::<usize>() {
        if w > 0 {
            width = w.min(usize::from(BAR_MAX_WIDTH)) as u8;
        }
    }
    if let Some(after_second) = tail[consumed..].strip_prefix(':') {
        let letters_len = after_second
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(after_second.len());
        if letters_len > 0 {
            color = bar_color(&after_second[..letters_len].to_ascii_lowercase());
            consumed += 1 + letters_len;
        }
    }
    *end += consumed;
    (width, color)
}

fn bar_color(spec: &str) -> BarColor {
    match spec {
        "auto" => BarColor::Auto,
        "game" => BarColor::Game,
        other => match parse_color_spec(other) {
            Some(Code::Fg(spec)) => BarColor::Color(spec),
            _ => BarColor::Auto,
        },
    }
}

/// A theme color name as its ANSI index.
pub fn named_color(name: &str) -> Option<u8> {
    Some(match name {
        "black" => 0,
        "red" => 1,
        "green" => 2,
        "yellow" => 3,
        "blue" => 4,
        "magenta" => 5,
        "cyan" => 6,
        "white" => 7,
        "gray" => 8,
        "bright_red" => 9,
        "bright_green" => 10,
        "bright_yellow" => 11,
        "bright_blue" => 12,
        "bright_magenta" => 13,
        "bright_cyan" => 14,
        "bright_white" => 15,
        _ => return None,
    })
}

/// The name a theme color goes by, the reverse of [`named_color`].
pub fn color_name(index: u8) -> Option<&'static str> {
    Some(match index {
        0 => "black",
        1 => "red",
        2 => "green",
        3 => "yellow",
        4 => "blue",
        5 => "magenta",
        6 => "cyan",
        7 => "white",
        8 => "gray",
        9 => "bright_red",
        10 => "bright_green",
        11 => "bright_yellow",
        12 => "bright_blue",
        13 => "bright_magenta",
        14 => "bright_cyan",
        15 => "bright_white",
        _ => return None,
    })
}

/// A color spec as a braced body writes it, `green`, `42`, `#80c8ff`,
/// `default`, `hp` or `hp:game`.
fn color_body(spec: &ColorSpec) -> String {
    match spec {
        ColorSpec::Named(n) => color_name(*n).map_or_else(|| n.to_string(), str::to_string),
        ColorSpec::Index(n) => n.to_string(),
        ColorSpec::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        ColorSpec::Default => "default".to_string(),
        ColorSpec::ByValue {
            field,
            scale: Scale::Thirds,
        } => field.to_string(),
        ColorSpec::ByValue {
            field,
            scale: Scale::Game,
        } => format!("{field}:game"),
        ColorSpec::ByValue {
            field,
            scale: Scale::Steps,
        } => format!("{field}:steps"),
    }
}

/// A color spec as the short form writes it after `%c_` or `%bg_`, or
/// None when only the braced form reads it back.
fn color_short(spec: &ColorSpec) -> Option<String> {
    match spec {
        ColorSpec::Named(n) => color_name(*n).map(str::to_string),
        ColorSpec::Index(n) => Some(n.to_string()),
        ColorSpec::Default => Some("default".to_string()),
        ColorSpec::ByValue {
            field,
            scale: Scale::Thirds,
        } if field.param.is_none() && !field.name.chars().all(|c| c.is_ascii_digit()) => {
            Some(field.name.clone())
        }
        ColorSpec::Rgb(..) | ColorSpec::ByValue { .. } => None,
    }
}

/// The name a style goes by in `%s_<style>`.
pub fn style_name(style: Style) -> &'static str {
    match style {
        Style::Bold => "bold",
        Style::Dim => "dim",
        Style::Italic => "italic",
        Style::Underline(UnderlineStyle::Single) => "underline",
        Style::Underline(UnderlineStyle::Double) => "double",
        Style::Underline(UnderlineStyle::Curly) => "curly",
        Style::Underline(UnderlineStyle::Dotted) => "dotted",
        Style::Underline(UnderlineStyle::Dashed) => "dashed",
        Style::Inverse => "inverse",
        Style::Strike => "strike",
        Style::Blink => "blink",
        Style::Off => "off",
    }
}

/// The name and arguments a format goes by in a braced value, such as
/// `pct` or `trunc:20`. None for the value format, which has none.
fn format_body(format: &Format) -> Option<String> {
    Some(match format {
        Format::Value => return None,
        Format::Max => "max".into(),
        Format::Pct => "pct".into(),
        Format::Bar { width, color } => match color {
            BarColor::Auto => format!("bar:{width}"),
            BarColor::Game => format!("bar:{width}:game"),
            BarColor::Color(spec) => format!("bar:{width}:{}", color_body(spec)),
        },
        Format::Game => "game".into(),
        Format::Word => "word".into(),
        Format::Ampm => "ampm".into(),
        Format::Name => "name".into(),
        Format::Grouped => "grouped".into(),
        Format::Short => "short".into(),
        Format::Thousands => "thousands".into(),
        Format::Unit => "unit".into(),
        Format::Since => "since".into(),
        Format::Trunc(n) => format!("trunc:{n}"),
        Format::Hm => "hm".into(),
        Format::Hms => "hms".into(),
        Format::Md => "md".into(),
        Format::Count => "count".into(),
        Format::Names => "names".into(),
        Format::On => "on".into(),
        Format::Off => "off".into(),
    })
}

/// Text as a template writes it, each `%` doubled so it stays literal
/// whatever follows it.
pub fn escape_text(text: &str) -> String {
    text.replace('%', "%%")
}

/// A token as the piece writer writes it: the short form where the
/// grammar has one (`%hp`, `%pct_hp`, `%c_green`, `%s_italic`, `%nl`)
/// unless `braced` asks for braces, and the braced form everywhere else
/// (`%{hp:bar:6}`, `%{c:#80c8ff}`, `%{if:fight}`). An unknown token has no
/// form of its own and writes nothing.
pub fn write_token(kind: &TokenKind, braced: bool) -> String {
    match kind {
        TokenKind::Text(text) => escape_text(text),
        TokenKind::Percent => "%%".to_string(),
        TokenKind::Code(Code::Reset) if braced => "%{c:reset}".to_string(),
        TokenKind::Code(Code::Reset) => "%c_reset".to_string(),
        TokenKind::Code(Code::Style(style)) if braced => format!("%{{s:{}}}", style_name(*style)),
        TokenKind::Code(Code::Style(style)) => format!("%s_{}", style_name(*style)),
        TokenKind::Code(Code::Fg(spec)) => write_color("c", spec, braced),
        TokenKind::Code(Code::Bg(spec)) => write_color("bg", spec, braced),
        // The underline color has no short form, so it always goes braced.
        TokenKind::Code(Code::UnderlineColor(spec)) => write_color("ul", spec, true),
        TokenKind::Value(value) => write_value(value, braced),
        TokenKind::If(field) => format!("%{{if:{field}}}"),
        TokenKind::IfNot(field) => format!("%{{ifnot:{field}}}"),
        TokenKind::End => "%{end}".to_string(),
        TokenKind::Nl if braced => "%{nl}".to_string(),
        TokenKind::Nl => "%nl".to_string(),
        TokenKind::Raw => "%{raw}".to_string(),
        TokenKind::Unknown => String::new(),
    }
}

fn write_color(prefix: &str, spec: &ColorSpec, braced: bool) -> String {
    match color_short(spec) {
        Some(short) if !braced => format!("%{prefix}_{short}"),
        _ => format!("%{{{prefix}:{}}}", color_body(spec)),
    }
}

fn write_value(value: &ValueRef, braced: bool) -> String {
    let field = &value.field;
    let plain = field.param.is_none();
    match (&value.format, braced) {
        (Format::Value, false) if plain => format!("%{}", field.name),
        (Format::Pct, false) if plain => format!("%pct_{}", field.name),
        (
            Format::Bar {
                width,
                color: BarColor::Auto,
            },
            false,
        ) if plain => format!("%{}_bar:{width}", field.name),
        (format, _) => match format_body(format) {
            Some(body) => format!("%{{{field}:{body}}}"),
            None => format!("%{{{field}}}"),
        },
    }
}

/// True when a token written as `text` would read on into `next`, the
/// first character written after it, so it needs its braced form: a
/// short name before a name character, or a bar before a colon or a
/// digit, which the first grammar took as the bar's width and color.
pub fn runs_on(kind: &TokenKind, text: &str, next: Option<char>) -> bool {
    let Some(next) = next else {
        return false;
    };
    let short = text.starts_with('%') && text[1..].starts_with(name_char);
    let bar_body = text.starts_with("%{") && !text[2..].contains(':');
    let bar = matches!(
        kind,
        TokenKind::Value(ValueRef {
            format: Format::Bar { .. },
            ..
        })
    );
    (short && name_char(next)) || (bar && (short || bar_body) && (next == ':' || name_char(next)))
}

/// Tokens as template text, each in its short form unless the next one
/// would extend it, as [`write_token`] and [`runs_on`] decide.
pub fn write_tokens(kinds: &[TokenKind]) -> String {
    let texts: Vec<String> = kinds.iter().map(|k| write_token(k, false)).collect();
    let mut out = String::new();
    for (index, kind) in kinds.iter().enumerate() {
        let next = texts.get(index + 1).and_then(|t| t.chars().next());
        if runs_on(kind, &texts[index], next) {
            out.push_str(&write_token(kind, true));
        } else {
            out.push_str(&texts[index]);
        }
    }
    out
}

/// Parse a color spec as a foreground code. `reset` gives
/// [`Code::Reset`]. None when the spec names nothing.
fn parse_color_spec(spec: &str) -> Option<Code> {
    if spec == "reset" {
        return Some(Code::Reset);
    }
    if spec == "default" {
        return Some(Code::Fg(ColorSpec::Default));
    }
    if let Some(index) = named_color(spec) {
        return Some(Code::Fg(ColorSpec::Named(index)));
    }
    if (1..=3).contains(&spec.len()) && spec.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(index) = spec.parse::<u8>() {
            return Some(Code::Fg(ColorSpec::Index(index)));
        }
    }
    let hex = spec.strip_prefix('#').unwrap_or(spec);
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        let channel = |s: &str| u8::from_str_radix(s, 16).unwrap_or(0);
        return Some(Code::Fg(ColorSpec::Rgb(
            channel(&hex[0..2]),
            channel(&hex[2..4]),
            channel(&hex[4..6]),
        )));
    }
    let parts: Vec<&str> = spec.split(',').collect();
    if parts.len() == 3
        && parts
            .iter()
            .all(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
    {
        let clamp = |s: &str| s.parse::<u32>().unwrap_or(0).min(255) as u8;
        return Some(Code::Fg(ColorSpec::Rgb(
            clamp(parts[0]),
            clamp(parts[1]),
            clamp(parts[2]),
        )));
    }
    if let Some((name, scale)) = spec.split_once(':') {
        let scale = match scale {
            "game" => Scale::Game,
            "steps" => Scale::Steps,
            _ => return None,
        };
        return valid_name(name).then(|| {
            Code::Fg(ColorSpec::ByValue {
                field: FieldRef::new(name),
                scale,
            })
        });
    }
    valid_name(spec).then(|| {
        Code::Fg(ColorSpec::ByValue {
            field: FieldRef::new(spec),
            scale: Scale::Thirds,
        })
    })
}

/// What a color code paints.
#[derive(Clone, Copy)]
enum Layer {
    Fg,
    Bg,
    Underline,
}

fn color_code(spec: &str, layer: Layer) -> TokenKind {
    match (parse_color_spec(spec), layer) {
        (Some(Code::Fg(spec)), Layer::Bg) => TokenKind::Code(Code::Bg(spec)),
        (Some(Code::Fg(spec)), Layer::Underline) => TokenKind::Code(Code::UnderlineColor(spec)),
        (Some(code), _) => TokenKind::Code(code),
        (None, _) => TokenKind::Unknown,
    }
}

fn style_code(name: &str) -> TokenKind {
    let style = match name {
        "bold" => Style::Bold,
        "dim" => Style::Dim,
        "italic" => Style::Italic,
        "underline" | "under" => Style::Underline(UnderlineStyle::Single),
        "double" => Style::Underline(UnderlineStyle::Double),
        "curly" => Style::Underline(UnderlineStyle::Curly),
        "dotted" => Style::Underline(UnderlineStyle::Dotted),
        "dashed" => Style::Underline(UnderlineStyle::Dashed),
        "inverse" | "inv" => Style::Inverse,
        "strike" => Style::Strike,
        "blink" => Style::Blink,
        "off" => Style::Off,
        "reset" => return TokenKind::Code(Code::Reset),
        _ => return TokenKind::Unknown,
    };
    TokenKind::Code(Code::Style(style))
}

fn value(field: FieldRef, format: Format) -> TokenKind {
    TokenKind::Value(ValueRef { field, format })
}

/// Parse an unbraced name, or a braced body with no `:`. `end` moves past
/// any legacy bar parameters.
fn parse_name(name: &str, source: &str, end: &mut usize) -> TokenKind {
    if let Some(base) = bar_base(name) {
        let (width, color) = consume_bar_params(source, end);
        if takes_param(base) {
            return TokenKind::Unknown;
        }
        return value(FieldRef::new(base), Format::Bar { width, color });
    }
    if let Some(spec) = name.strip_prefix("c_") {
        return color_code(spec, Layer::Fg);
    }
    if let Some(spec) = name.strip_prefix("bg_") {
        return color_code(spec, Layer::Bg);
    }
    if let Some(style) = name.strip_prefix("s_") {
        return style_code(style);
    }
    match name {
        "nl" => return TokenKind::Nl,
        "raw" => return TokenKind::Raw,
        "end" => return TokenKind::End,
        _ => {}
    }
    if takes_param(name) {
        return TokenKind::Unknown;
    }
    if let Some(base) = name.strip_prefix("pct_") {
        if !base.is_empty() && !takes_param(base) {
            return value(FieldRef::new(base), Format::Pct);
        }
    }
    if valid_name(name) {
        value(FieldRef::new(name), Format::Value)
    } else {
        TokenKind::Unknown
    }
}

/// Parse the field at the start of `segs`, taking its parameter when it
/// has one. Returns the field and the segments after it.
fn parse_field<'a>(segs: &'a [&'a str]) -> Option<(FieldRef, &'a [&'a str])> {
    let name = segs.first()?.to_ascii_lowercase();
    if !valid_name(&name) {
        return None;
    }
    if takes_param(&name) {
        let raw = segs.get(1)?;
        if raw.is_empty() {
            return None;
        }
        let param = if param_keeps_case(&name) {
            (*raw).to_string()
        } else {
            raw.to_ascii_lowercase()
        };
        return Some((FieldRef::with_param(name, param), &segs[2..]));
    }
    Some((FieldRef::new(name), &segs[1..]))
}

/// Parse a format name and its arguments. None for a format the grammar
/// does not know or arguments it cannot read.
fn parse_format(segs: &[&str]) -> Option<Format> {
    let Some(first) = segs.first() else {
        return Some(Format::Value);
    };
    let name = first.to_ascii_lowercase();
    let args: Vec<String> = segs[1..].iter().map(|s| s.to_ascii_lowercase()).collect();
    let no_args = |format: Format| args.is_empty().then_some(format);
    match name.as_str() {
        "value" => no_args(Format::Value),
        "max" => no_args(Format::Max),
        "pct" => no_args(Format::Pct),
        "game" => no_args(Format::Game),
        "word" => no_args(Format::Word),
        "ampm" => no_args(Format::Ampm),
        "name" => no_args(Format::Name),
        "grouped" => no_args(Format::Grouped),
        "short" => no_args(Format::Short),
        "thousands" => no_args(Format::Thousands),
        "unit" => no_args(Format::Unit),
        "since" => no_args(Format::Since),
        "hm" => no_args(Format::Hm),
        "hms" => no_args(Format::Hms),
        "md" => no_args(Format::Md),
        "count" => no_args(Format::Count),
        "names" => no_args(Format::Names),
        "on" => no_args(Format::On),
        "off" => no_args(Format::Off),
        "trunc" => match args.as_slice() {
            [n] => n.parse::<usize>().ok().map(Format::Trunc),
            _ => None,
        },
        "bar" => {
            let mut rest = args.as_slice();
            let mut width = BAR_DEFAULT_WIDTH;
            if let Some(first) = rest.first() {
                if !first.is_empty() && first.chars().all(|c| c.is_ascii_digit()) {
                    if let Ok(w) = first.parse::<usize>() {
                        if w > 0 {
                            width = w.min(usize::from(BAR_MAX_WIDTH)) as u8;
                        }
                    }
                    rest = &rest[1..];
                }
            }
            let color = match rest {
                [] => BarColor::Auto,
                [spec] => match spec.as_str() {
                    "auto" => BarColor::Auto,
                    "game" => BarColor::Game,
                    other => match parse_color_spec(other) {
                        Some(Code::Fg(spec)) => BarColor::Color(spec),
                        _ => return None,
                    },
                },
                _ => return None,
            };
            Some(Format::Bar { width, color })
        }
        _ => None,
    }
}

/// Parse a braced body. `end` moves past legacy bar parameters that
/// follow `%{hp_bar}`.
fn parse_braced(body: &str, source: &str, end: &mut usize) -> TokenKind {
    let segs: Vec<&str> = body.split(':').collect();
    let head = segs[0].to_ascii_lowercase();
    if segs.len() == 1 {
        return parse_name(&head, source, end);
    }
    let rest = segs[1..].join(":").to_ascii_lowercase();
    match head.as_str() {
        "c" => return color_code(&rest, Layer::Fg),
        "bg" => return color_code(&rest, Layer::Bg),
        "ul" => return color_code(&rest, Layer::Underline),
        "s" => return style_code(&rest),
        "if" | "ifnot" => {
            return match parse_field(&segs[1..]) {
                Some((field, [])) if head == "if" => TokenKind::If(field),
                Some((field, [])) => TokenKind::IfNot(field),
                _ => TokenKind::Unknown,
            };
        }
        _ => {}
    }
    if let Some(spec) = head.strip_prefix("c_") {
        return color_code(&format!("{spec}:{rest}"), Layer::Fg);
    }
    if let Some(spec) = head.strip_prefix("bg_") {
        return color_code(&format!("{spec}:{rest}"), Layer::Bg);
    }

    let Some((field, format_segs)) = parse_field(&segs) else {
        return TokenKind::Unknown;
    };
    if matches!(
        field.name.as_str(),
        "nl" | "raw" | "end" | "if" | "ifnot" | "s"
    ) {
        return TokenKind::Unknown;
    }
    match parse_format(format_segs) {
        Some(format) => value(field, format),
        None => TokenKind::Unknown,
    }
}

/// True when `max` reads the max of `base`, in any spelling the first
/// grammar looked a max up by, or as `%{base:max}`.
fn is_max_of(base: &ValueRef, max: &ValueRef) -> bool {
    if base.format != Format::Value || base.field.param.is_some() || max.field.param.is_some() {
        return false;
    }
    let name = &base.field.name;
    match max.format {
        Format::Max => max.field.name == *name,
        Format::Value => {
            let m = &max.field.name;
            *m == format!("max{name}")
                || *m == format!("m{name}")
                || *m == format!("{name}_max")
                || *m == format!("max_{name}")
        }
        _ => false,
    }
}

/// Group tokens into pieces.
fn group(tokens: &[Token]) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut i = 0;
    let piece = |codes: Range<usize>, content: Range<usize>, kind: PieceKind| {
        let first = if codes.is_empty() {
            content.start
        } else {
            codes.start
        };
        let last = if content.is_empty() {
            codes.end
        } else {
            content.end
        };
        Piece {
            start: tokens[first].start,
            end: tokens[last - 1].end,
            codes,
            content,
            kind,
        }
    };
    while i < tokens.len() {
        let codes_start = i;
        while i < tokens.len() && matches!(tokens[i].kind, TokenKind::Code(_)) {
            i += 1;
        }
        let codes = codes_start..i;
        if i == tokens.len() {
            if !codes.is_empty() {
                pieces.push(piece(codes, i..i, PieceKind::Codes));
            }
            break;
        }
        let marker = match tokens[i].kind {
            TokenKind::If(_) => Some(PieceKind::If),
            TokenKind::IfNot(_) => Some(PieceKind::IfNot),
            TokenKind::End => Some(PieceKind::End),
            TokenKind::Nl => Some(PieceKind::Nl),
            _ => None,
        };
        if let Some(kind) = marker {
            if !codes.is_empty() {
                pieces.push(piece(codes.clone(), codes.end..codes.end, PieceKind::Codes));
            }
            pieces.push(piece(i..i, i..i + 1, kind));
            i += 1;
            continue;
        }
        let (len, kind) = match &tokens[i].kind {
            TokenKind::Text(_) | TokenKind::Percent => {
                let run = tokens[i..]
                    .iter()
                    .take_while(|t| matches!(t.kind, TokenKind::Text(_) | TokenKind::Percent))
                    .count();
                (run, PieceKind::Text)
            }
            TokenKind::Value(base) => {
                let slash_max = match (tokens.get(i + 1), tokens.get(i + 2)) {
                    (
                        Some(Token {
                            kind: TokenKind::Text(slash),
                            ..
                        }),
                        Some(Token {
                            kind: TokenKind::Value(max),
                            ..
                        }),
                    ) => slash == "/" && is_max_of(base, max),
                    _ => false,
                };
                let percent = base.format == Format::Pct
                    && matches!(
                        tokens.get(i + 1),
                        Some(Token {
                            kind: TokenKind::Percent,
                            ..
                        })
                    );
                if slash_max {
                    (3, PieceKind::CurMax)
                } else if percent {
                    (2, PieceKind::Percent)
                } else {
                    (1, PieceKind::Value)
                }
            }
            TokenKind::Raw => (1, PieceKind::Raw),
            _ => (1, PieceKind::Unknown),
        };
        pieces.push(piece(codes, i..i + len, kind));
        i += len;
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    const JAMES: &str = "%{c:100,100,100}[%c_reset%s_italic%hp(%c_hp%pct_hp%c_reset%s_italic%)h %mana(%{c:128,200,255}%pct_mana%c_reset%s_italic%)m %move(%{c:200,255,23}%pct_move%c_reset%s_italic%)v%c_reset%{c:100,100,100}] %c_reset";

    fn kinds(source: &str) -> Vec<TokenKind> {
        tokenize(source).into_iter().map(|t| t.kind).collect()
    }

    fn val(name: &str) -> TokenKind {
        value(FieldRef::new(name), Format::Value)
    }

    fn fmt(name: &str, format: Format) -> TokenKind {
        value(FieldRef::new(name), format)
    }

    fn text(s: &str) -> TokenKind {
        TokenKind::Text(s.to_string())
    }

    fn fg(spec: ColorSpec) -> TokenKind {
        TokenKind::Code(Code::Fg(spec))
    }

    fn bg(spec: ColorSpec) -> TokenKind {
        TokenKind::Code(Code::Bg(spec))
    }

    fn by_value(name: &str) -> ColorSpec {
        ColorSpec::ByValue {
            field: FieldRef::new(name),
            scale: Scale::Thirds,
        }
    }

    fn by_scale(name: &str, scale: Scale) -> ColorSpec {
        ColorSpec::ByValue {
            field: FieldRef::new(name),
            scale,
        }
    }

    fn bar(width: u8, color: BarColor) -> Format {
        Format::Bar { width, color }
    }

    #[test]
    fn legacy_values_parse_as_before() {
        assert_eq!(kinds("%hp"), vec![val("hp")]);
        assert_eq!(kinds("%HP"), vec![val("hp")]);
        assert_eq!(kinds("%{hp}"), vec![val("hp")]);
        assert_eq!(kinds("%{MaxHp}"), vec![val("maxhp")]);
        assert_eq!(kinds("%pct_hp"), vec![fmt("hp", Format::Pct)]);
        assert_eq!(kinds("%{pct_hp}"), vec![fmt("hp", Format::Pct)]);
        assert_eq!(
            kinds("%time %date"),
            vec![val("time"), text(" "), val("date")]
        );
        assert_eq!(kinds("%nope"), vec![val("nope")]);
        assert_eq!(kinds("%hp/%mhp"), vec![val("hp"), text("/"), val("mhp")]);
    }

    #[test]
    fn legacy_bars_take_their_parameters_from_the_text_after_them() {
        let auto = BarColor::Auto;
        let named = |i| BarColor::Color(ColorSpec::Named(i));
        assert_eq!(kinds("%hp_bar"), vec![fmt("hp", bar(10, auto.clone()))]);
        assert_eq!(
            kinds("%hp_bar:4:green after"),
            vec![fmt("hp", bar(4, named(2))), text(" after")]
        );
        assert_eq!(
            kinds("%bar_mana:6"),
            vec![fmt("mana", bar(6, auto.clone()))]
        );
        // Width 0 keeps the default, and anything past 80 is 80.
        assert_eq!(
            kinds("%move_bar:0:yellow"),
            vec![fmt("move", bar(10, named(3)))]
        );
        assert_eq!(kinds("%hp_bar:200"), vec![fmt("hp", bar(80, auto.clone()))]);
        // A color with no width still reads, and a lone colon is taken.
        assert_eq!(kinds("%hp_bar::red"), vec![fmt("hp", bar(10, named(1)))]);
        assert_eq!(
            kinds("%hp_bar:x"),
            vec![fmt("hp", bar(10, auto.clone())), text("x")]
        );
        assert_eq!(
            kinds("%hp_bar:10:"),
            vec![fmt("hp", bar(10, auto.clone())), text(":")]
        );
        assert_eq!(kinds("%{hp_bar}:3"), vec![fmt("hp", bar(3, auto.clone()))]);
        assert_eq!(
            kinds("%hp_bar%c_red"),
            vec![fmt("hp", bar(10, auto)), fg(ColorSpec::Named(1))]
        );
    }

    #[test]
    fn legacy_colors_and_styles_parse_as_before() {
        assert_eq!(kinds("%c_red"), vec![fg(ColorSpec::Named(1))]);
        assert_eq!(kinds("%{c:196}"), vec![fg(ColorSpec::Index(196))]);
        assert_eq!(kinds("%c_042"), vec![fg(ColorSpec::Index(42))]);
        assert_eq!(kinds("%{c:#FF8800}"), vec![fg(ColorSpec::Rgb(255, 136, 0))]);
        assert_eq!(kinds("%c_ff8800"), vec![fg(ColorSpec::Rgb(255, 136, 0))]);
        assert_eq!(
            kinds("%{c:255,128,0}"),
            vec![fg(ColorSpec::Rgb(255, 128, 0))]
        );
        assert_eq!(
            kinds("%{c:300,1,99999999999}"),
            vec![fg(ColorSpec::Rgb(255, 1, 0))]
        );
        assert_eq!(kinds("%c_hp"), vec![fg(by_value("hp"))]);
        assert_eq!(kinds("%{c:hp}"), vec![fg(by_value("hp"))]);
        assert_eq!(kinds("%c_300"), vec![fg(by_value("300"))]);
        assert_eq!(kinds("%c_reset"), vec![TokenKind::Code(Code::Reset)]);
        assert_eq!(kinds("%{c:reset}"), vec![TokenKind::Code(Code::Reset)]);
        assert_eq!(kinds("%bg_reset"), vec![TokenKind::Code(Code::Reset)]);
        assert_eq!(kinds("%s_reset"), vec![TokenKind::Code(Code::Reset)]);
        assert_eq!(kinds("%bg_green"), vec![bg(ColorSpec::Named(2))]);
        assert_eq!(kinds("%{bg:#330033}"), vec![bg(ColorSpec::Rgb(51, 0, 51))]);
        assert_eq!(kinds("%bg_hp"), vec![bg(by_value("hp"))]);
        assert_eq!(kinds("%{c_red}"), vec![fg(ColorSpec::Named(1))]);
        assert_eq!(kinds("%{bg_blue}"), vec![bg(ColorSpec::Named(4))]);
        for (name, style) in [
            ("bold", Style::Bold),
            ("dim", Style::Dim),
            ("italic", Style::Italic),
            ("underline", Style::Underline(UnderlineStyle::Single)),
            ("under", Style::Underline(UnderlineStyle::Single)),
            ("inverse", Style::Inverse),
            ("inv", Style::Inverse),
            ("strike", Style::Strike),
            ("blink", Style::Blink),
        ] {
            assert_eq!(
                kinds(&format!("%s_{name}")),
                vec![TokenKind::Code(Code::Style(style))]
            );
            assert_eq!(
                kinds(&format!("%{{s:{name}}}")),
                vec![TokenKind::Code(Code::Style(style))]
            );
        }
    }

    fn style(style: Style) -> TokenKind {
        TokenKind::Code(Code::Style(style))
    }

    fn ul(spec: ColorSpec) -> TokenKind {
        TokenKind::Code(Code::UnderlineColor(spec))
    }

    #[test]
    fn underline_kinds_parse_as_styles() {
        for (name, line) in [
            ("double", UnderlineStyle::Double),
            ("curly", UnderlineStyle::Curly),
            ("dotted", UnderlineStyle::Dotted),
            ("dashed", UnderlineStyle::Dashed),
        ] {
            assert_eq!(
                kinds(&format!("%s_{name}")),
                vec![style(Style::Underline(line))]
            );
            assert_eq!(
                kinds(&format!("%{{S:{name}}}")),
                vec![style(Style::Underline(line))]
            );
        }
        assert_eq!(
            kinds("%s_underline"),
            vec![style(Style::Underline(UnderlineStyle::Single))]
        );
    }

    #[test]
    fn an_underline_color_takes_the_forms_of_a_text_color() {
        assert_eq!(
            kinds("%{ul:#BF616A}"),
            vec![ul(ColorSpec::Rgb(191, 97, 106))]
        );
        assert_eq!(
            kinds("%{ul:191,97,106}"),
            vec![ul(ColorSpec::Rgb(191, 97, 106))]
        );
        assert_eq!(
            kinds("%{ul:bf616a}"),
            vec![ul(ColorSpec::Rgb(191, 97, 106))]
        );
        assert_eq!(kinds("%{ul:red}"), vec![ul(ColorSpec::Named(1))]);
        assert_eq!(kinds("%{ul:208}"), vec![ul(ColorSpec::Index(208))]);
        assert_eq!(kinds("%{ul:default}"), vec![ul(ColorSpec::Default)]);
        assert_eq!(kinds("%{ul:hp}"), vec![ul(by_value("hp"))]);
        assert_eq!(
            kinds("%{ul:hp:game}"),
            vec![ul(by_scale("hp", Scale::Game))]
        );
        assert_eq!(kinds("%{ul:reset}"), vec![TokenKind::Code(Code::Reset)]);
        assert_eq!(kinds("%{ul:}"), vec![TokenKind::Unknown]);
        assert_eq!(kinds("%{ul:1,2}"), vec![TokenKind::Unknown]);
    }

    #[test]
    fn a_name_that_starts_with_ul_stays_a_value() {
        // A script names its values freely, so the underline color has no
        // short form to take `ul_` from them.
        assert_eq!(kinds("%ul_kills"), vec![val("ul_kills")]);
        assert_eq!(kinds("%ul_red"), vec![val("ul_red")]);
        assert_eq!(kinds("%{ul_red}"), vec![val("ul_red")]);
        assert_eq!(
            kinds("%{ul_red:pct}"),
            vec![TokenKind::Value(ValueRef {
                field: FieldRef::new("ul_red"),
                format: Format::Pct,
            })]
        );
        assert_eq!(kinds("%ul"), vec![val("ul")]);
    }

    #[test]
    fn each_style_writes_its_own_sgr() {
        let sgr = |s: Style| s.sgr();
        assert_eq!(sgr(Style::Bold), "1");
        assert_eq!(sgr(Style::Dim), "2");
        assert_eq!(sgr(Style::Italic), "3");
        assert_eq!(sgr(Style::Underline(UnderlineStyle::Single)), "4");
        assert_eq!(sgr(Style::Underline(UnderlineStyle::Double)), "4:2");
        assert_eq!(sgr(Style::Underline(UnderlineStyle::Curly)), "4:3");
        assert_eq!(sgr(Style::Underline(UnderlineStyle::Dotted)), "4:4");
        assert_eq!(sgr(Style::Underline(UnderlineStyle::Dashed)), "4:5");
        assert_eq!(sgr(Style::Inverse), "7");
        assert_eq!(sgr(Style::Strike), "9");
        assert_eq!(sgr(Style::Blink), "5");
        assert_eq!(sgr(Style::Off), "22;23;24;25;27;29");
    }

    #[test]
    fn new_style_codes_write_back_as_they_read() {
        for source in [
            "%s_double",
            "%s_curly",
            "%s_dotted",
            "%s_dashed",
            "%s_strike",
            "%s_dim",
            "%s_inverse",
            "%s_blink",
            "%{ul:red}",
            "%{ul:default}",
            "%{ul:#bf616a}",
            "%{ul:hp:game}",
            "%{c:hp:steps}",
            "%{bg:mana:steps}",
            "%{ul:move:steps}",
        ] {
            let tokens = kinds(source);
            assert_eq!(write_tokens(&tokens), source, "{source}");
        }
        assert_eq!(
            write_token(&ul(ColorSpec::Rgb(191, 97, 106)), false),
            "%{ul:#bf616a}"
        );
        assert_eq!(write_token(&ul(ColorSpec::Named(1)), false), "%{ul:red}");
        assert_eq!(write_token(&ul(ColorSpec::Default), false), "%{ul:default}");
        assert_eq!(write_token(&ul(by_value("hp")), false), "%{ul:hp}");
        assert_eq!(write_token(&ul(ColorSpec::Named(1)), true), "%{ul:red}");
        assert_eq!(
            write_token(&style(Style::Underline(UnderlineStyle::Curly)), true),
            "%{s:curly}"
        );
    }

    #[test]
    fn every_code_the_help_lists_reads_as_a_code() {
        let help = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../HELP.md"))
            .expect("HELP.md at the repo root");
        let topic = help
            .find(" Prompt design codes\n")
            .expect("the prompt design codes topic");
        let rows: Vec<&str> = help[topic..]
            .lines()
            .skip_while(|l| !l.starts_with("| Code"))
            .skip(2)
            .take_while(|l| l.starts_with('|'))
            .collect();
        assert!(rows.len() >= 18, "{rows:?}");
        for row in rows {
            let cell = row.split('|').nth(1).expect("a code cell");
            for code in cell.split('`').skip(1).step_by(2) {
                let template = Template::parse(code);
                let tokens = template.tokens();
                assert!(
                    tokens
                        .iter()
                        .all(|t| t.kind != TokenKind::Unknown
                            && !matches!(t.kind, TokenKind::Text(_))),
                    "{code} reads as {tokens:?}"
                );
                // A color or style code reads as one, never as a value
                // by a name nothing has.
                let painted = [
                    "%c_", "%bg_", "%ul_", "%s_", "%{c:", "%{bg:", "%{ul:", "%{s:",
                ];
                if painted.iter().any(|p| code.starts_with(p)) {
                    assert!(
                        matches!(
                            tokens,
                            [Token {
                                kind: TokenKind::Code(_),
                                ..
                            }]
                        ),
                        "{code} reads as {tokens:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn reads_the_field_an_underline_color_follows() {
        let template = Template::parse("%{ul:hp}%s_curly%{ul:mana:game}x");
        let names: Vec<String> = template.reads().iter().map(ToString::to_string).collect();
        assert_eq!(names, vec!["hp", "mana"]);
    }

    #[test]
    fn anything_else_after_a_percent_stays_literal() {
        assert_eq!(kinds("%%"), vec![TokenKind::Percent]);
        assert_eq!(kinds("%)h"), vec![text("%)h")]);
        assert_eq!(kinds("% "), vec![text("% ")]);
        assert_eq!(kinds("end %"), vec![text("end %")]);
        assert_eq!(kinds("%{"), vec![text("%{")]);
        assert_eq!(kinds("%{}"), vec![text("%{}")]);
        assert_eq!(kinds("%{a b}"), vec![text("%{a b}")]);
        // A colon ends an unbraced name, as it always did.
        assert_eq!(kinds("%c:red"), vec![val("c"), text(":red")]);
    }

    #[test]
    fn codes_the_grammar_does_not_know_print_as_written() {
        for source in [
            "%s_nope",
            "%c_",
            "%{c:}",
            "%{c:1,2}",
            "%{a.b}",
            "%{hp:bogus}",
        ] {
            let tokens = tokenize(source);
            assert_eq!(tokens.len(), 1, "{source}");
            assert_eq!(tokens[0].kind, TokenKind::Unknown, "{source}");
            assert_eq!(&source[tokens[0].start..tokens[0].end], source);
        }
    }

    #[test]
    fn tokens_keep_their_byte_ranges() {
        let source = "é%hp_bar:4 %{c:hp:game}%%x";
        let tokens = tokenize(source);
        let spans: Vec<&str> = tokens.iter().map(|t| &source[t.start..t.end]).collect();
        assert_eq!(
            spans,
            vec!["é", "%hp_bar:4", " ", "%{c:hp:game}", "%%", "x"]
        );
    }

    #[test]
    fn james_template_parses_to_the_same_tokens_as_before() {
        let tokens = tokenize(JAMES);
        let written: Vec<&str> = tokens.iter().map(|t| &JAMES[t.start..t.end]).collect();
        assert_eq!(
            written,
            vec![
                "%{c:100,100,100}",
                "[",
                "%c_reset",
                "%s_italic",
                "%hp",
                "(",
                "%c_hp",
                "%pct_hp",
                "%c_reset",
                "%s_italic",
                "%)h ",
                "%mana",
                "(",
                "%{c:128,200,255}",
                "%pct_mana",
                "%c_reset",
                "%s_italic",
                "%)m ",
                "%move",
                "(",
                "%{c:200,255,23}",
                "%pct_move",
                "%c_reset",
                "%s_italic",
                "%)v",
                "%c_reset",
                "%{c:100,100,100}",
                "] ",
                "%c_reset",
            ]
        );
        assert!(tokens.iter().all(|t| t.kind != TokenKind::Unknown));
    }

    #[test]
    fn new_forms_parse() {
        assert_eq!(kinds("%c_default"), vec![fg(ColorSpec::Default)]);
        assert_eq!(kinds("%bg_default"), vec![bg(ColorSpec::Default)]);
        assert_eq!(
            kinds("%s_off"),
            vec![TokenKind::Code(Code::Style(Style::Off))]
        );
        assert_eq!(kinds("%c_gray"), vec![fg(ColorSpec::Named(8))]);
        assert_eq!(kinds("%c_black"), vec![fg(ColorSpec::Named(0))]);
        assert_eq!(kinds("%c_bright_white"), vec![fg(ColorSpec::Named(15))]);
        assert_eq!(kinds("%{c:hp:game}"), vec![fg(by_scale("hp", Scale::Game))]);
        assert_eq!(
            kinds("%{c:hp:steps}"),
            vec![fg(by_scale("hp", Scale::Steps))]
        );
        assert_eq!(
            kinds("%{bg:Mana:STEPS}"),
            vec![bg(by_scale("mana", Scale::Steps))]
        );
        assert_eq!(
            kinds("%{ul:move:steps}"),
            vec![ul(by_scale("move", Scale::Steps))]
        );
        assert_eq!(kinds("%{c:hp:tenths}"), vec![TokenKind::Unknown]);
        assert_eq!(kinds("%nl%{nl}"), vec![TokenKind::Nl, TokenKind::Nl]);
        assert_eq!(kinds("%{raw}"), vec![TokenKind::Raw]);
        assert_eq!(kinds("%{end}"), vec![TokenKind::End]);
        assert_eq!(
            kinds("%{if:fight}%{ifnot:Fight}"),
            vec![
                TokenKind::If(FieldRef::new("fight")),
                TokenKind::IfNot(FieldRef::new("fight"))
            ]
        );
        assert_eq!(kinds("%{if:}"), vec![TokenKind::Unknown]);
        assert_eq!(kinds("%maxhp"), vec![val("maxhp")]);
        assert_eq!(kinds("%{hp:max}"), vec![fmt("hp", Format::Max)]);
        assert_eq!(kinds("%{hp:pct}"), vec![fmt("hp", Format::Pct)]);
        assert_eq!(
            kinds("%{hp:bar:10:auto}"),
            vec![fmt("hp", bar(10, BarColor::Auto))]
        );
        assert_eq!(
            kinds("%{opponent_hp:bar}"),
            vec![fmt("opponent_hp", bar(10, BarColor::Auto))]
        );
        assert_eq!(
            kinds("%{hp:bar:6:game}"),
            vec![fmt("hp", bar(6, BarColor::Game))]
        );
        assert_eq!(
            kinds("%{hp:bar:6:#FF0000}"),
            vec![fmt(
                "hp",
                bar(6, BarColor::Color(ColorSpec::Rgb(255, 0, 0)))
            )]
        );
        assert_eq!(
            kinds("%{hp:bar:red}"),
            vec![fmt("hp", bar(10, BarColor::Color(ColorSpec::Named(1))))]
        );
        assert_eq!(kinds("%{tank_hp:game}"), vec![fmt("tank_hp", Format::Game)]);
        assert_eq!(kinds("%{pos:word}"), vec![fmt("pos", Format::Word)]);
        assert_eq!(kinds("%{hour:ampm}"), vec![fmt("hour", Format::Ampm)]);
        assert_eq!(kinds("%{Hour:AMPM}"), vec![fmt("hour", Format::Ampm)]);
        assert_eq!(kinds("%{moon1:name}"), vec![fmt("moon1", Format::Name)]);
        assert_eq!(kinds("%{gold:grouped}"), vec![fmt("gold", Format::Grouped)]);
        assert_eq!(kinds("%{gold:short}"), vec![fmt("gold", Format::Short)]);
        assert_eq!(
            kinds("%{gold:thousands}"),
            vec![fmt("gold", Format::Thousands)]
        );
        assert_eq!(kinds("%{gold:thousands:1}"), vec![TokenKind::Unknown]);
        assert_eq!(kinds("%{temp:unit}"), vec![fmt("temp", Format::Unit)]);
        assert_eq!(kinds("%{tick:since}"), vec![fmt("tick", Format::Since)]);
        assert_eq!(
            kinds("%{room:trunc:20}"),
            vec![fmt("room", Format::Trunc(20))]
        );
        assert_eq!(kinds("%{room:trunc}"), vec![TokenKind::Unknown]);
        assert_eq!(kinds("%{time:hm}"), vec![fmt("time", Format::Hm)]);
        assert_eq!(kinds("%{time:hms}"), vec![fmt("time", Format::Hms)]);
        assert_eq!(kinds("%{date:md}"), vec![fmt("date", Format::Md)]);
        assert_eq!(
            kinds("%{missing:names}"),
            vec![fmt("missing", Format::Names)]
        );
        assert_eq!(
            kinds("%{missing:count}"),
            vec![fmt("missing", Format::Count)]
        );
        assert_eq!(kinds("%{hp:max:1}"), vec![TokenKind::Unknown]);
    }

    #[test]
    fn the_forms_of_the_old_prompt_write_back_as_they_read() {
        for source in [
            "%{gold:thousands}",
            "%{exp:thousands}",
            "%{hour:ampm}",
            "%{tick:since}",
        ] {
            let tokens = kinds(source);
            assert!(tokens.iter().all(|t| *t != TokenKind::Unknown), "{source}");
            assert_eq!(write_tokens(&tokens), source, "{source}");
        }
    }

    #[test]
    fn param_fields_keep_the_case_of_their_parameter() {
        let param =
            |name: &str, p: &str, format: Format| value(FieldRef::with_param(name, p), format);
        assert_eq!(
            kinds("%{aff:Giant_Strength}"),
            vec![param("aff", "Giant_Strength", Format::Value)]
        );
        assert_eq!(
            kinds("%{aff:sanctuary:on}"),
            vec![param("aff", "sanctuary", Format::On)]
        );
        assert_eq!(
            kinds("%{member_hp:Quenby:bar:6}"),
            vec![param("member_hp", "Quenby", bar(6, BarColor::Auto))]
        );
        assert_eq!(
            kinds("%{member_hp:id=3}"),
            vec![param("member_hp", "id=3", Format::Value)]
        );
        assert_eq!(
            kinds("%{QUEUE:Bugs}"),
            vec![param("queue", "bugs", Format::Value)]
        );
        assert_eq!(
            kinds("%{gmcp:Char.Vitals.ep}"),
            vec![param("gmcp", "Char.Vitals.ep", Format::Value)]
        );
        assert_eq!(
            kinds("%{gmcp:Char.Affects.affects[name=sanctuary].level}"),
            vec![param(
                "gmcp",
                "Char.Affects.affects[name=sanctuary].level",
                Format::Value
            )]
        );
        assert_eq!(
            kinds("%{if:aff:Sanctuary}"),
            vec![TokenKind::If(FieldRef::with_param("aff", "Sanctuary"))]
        );
        // A parameter field with no parameter reads nothing.
        assert_eq!(kinds("%{aff}"), vec![TokenKind::Unknown]);
        assert_eq!(kinds("%aff"), vec![TokenKind::Unknown]);
    }

    fn piece_kinds(source: &str) -> Vec<(PieceKind, String)> {
        let template = Template::parse(source);
        (0..template.pieces().len())
            .map(|i| {
                (
                    template.pieces()[i].kind,
                    template.piece_text(i).to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn pieces_are_codes_then_one_value_or_one_run_of_text() {
        use PieceKind::{Codes, Percent, Text, Value};
        let got = piece_kinds(JAMES);
        let expect: Vec<(PieceKind, &str)> = vec![
            (Text, "%{c:100,100,100}["),
            (Value, "%c_reset%s_italic%hp"),
            (Text, "("),
            (Value, "%c_hp%pct_hp"),
            (Text, "%c_reset%s_italic%)h "),
            (Value, "%mana"),
            (Text, "("),
            (Value, "%{c:128,200,255}%pct_mana"),
            (Text, "%c_reset%s_italic%)m "),
            (Value, "%move"),
            (Text, "("),
            (Value, "%{c:200,255,23}%pct_move"),
            (Text, "%c_reset%s_italic%)v"),
            (Text, "%c_reset%{c:100,100,100}] "),
            (Codes, "%c_reset"),
        ];
        let expect: Vec<(PieceKind, String)> = expect
            .into_iter()
            .map(|(k, s)| (k, s.to_string()))
            .collect();
        assert_eq!(got, expect);
        assert!(!got.iter().any(|(k, _)| *k == Percent));
    }

    #[test]
    fn current_and_max_fold_into_one_piece() {
        for source in [
            "%hp/%{maxhp}",
            "%hp/%maxhp",
            "%hp/%mhp",
            "%hp/%hp_max",
            "%hp/%max_hp",
            "%{hp}/%{hp:max}",
        ] {
            let got = piece_kinds(source);
            assert_eq!(
                got,
                vec![(PieceKind::CurMax, source.to_string())],
                "{source}"
            );
        }
        // Anything between the two, or another field's max, does not fold.
        assert_eq!(piece_kinds("%hp/%maxmana").len(), 3);
        assert_eq!(piece_kinds("%hp /%maxhp").len(), 3);
        assert_eq!(piece_kinds("%hp%c_red/%maxhp").len(), 3);
        assert_eq!(piece_kinds("%pct_hp/%maxhp").len(), 3);
    }

    #[test]
    fn a_percent_and_its_sign_fold_into_one_piece() {
        assert_eq!(
            piece_kinds("hp %c_hp%pct_hp%%%c_default mn"),
            vec![
                (PieceKind::Text, "hp ".to_string()),
                (PieceKind::Percent, "%c_hp%pct_hp%%".to_string()),
                (PieceKind::Text, "%c_default mn".to_string()),
            ]
        );
        assert_eq!(
            piece_kinds("%{hp:pct}%%)"),
            vec![
                (PieceKind::Percent, "%{hp:pct}%%".to_string()),
                (PieceKind::Text, ")".to_string()),
            ]
        );
        // Text right after the sign stays its own piece.
        assert_eq!(
            piece_kinds("[%c_hp%pct_hp%%hp%c_default/"),
            vec![
                (PieceKind::Text, "[".to_string()),
                (PieceKind::Percent, "%c_hp%pct_hp%%".to_string()),
                (PieceKind::Text, "hp".to_string()),
                (PieceKind::Text, "%c_default/".to_string()),
            ]
        );
        // A percent sign after anything but a percent value is text.
        assert_eq!(
            piece_kinds("%hp%%"),
            vec![
                (PieceKind::Value, "%hp".to_string()),
                (PieceKind::Text, "%%".to_string()),
            ]
        );
    }

    #[test]
    fn codes_before_a_line_break_or_a_condition_stand_alone() {
        use PieceKind::{Codes, End, If, Nl, Text, Value};
        let got = piece_kinds("%c_red%{if:fight}%opponent%c_reset%nl%{end}x");
        let expect = vec![
            (Codes, "%c_red"),
            (If, "%{if:fight}"),
            (Value, "%opponent"),
            (Codes, "%c_reset"),
            (Nl, "%nl"),
            (End, "%{end}"),
            (Text, "x"),
        ];
        let expect: Vec<(PieceKind, String)> = expect
            .into_iter()
            .map(|(k, s)| (k, s.to_string()))
            .collect();
        assert_eq!(got, expect);
    }

    #[test]
    fn reads_lists_every_field_the_template_uses() {
        let template = Template::parse(
            "%{if:fight}%{c:hp:game}%hp%{end}%{mana:bar:6:move}%bg_tick%{raw}%{aff:Haste}",
        );
        let names: Vec<String> = template.reads().iter().map(ToString::to_string).collect();
        assert_eq!(
            names,
            vec!["aff:Haste", "fight", "hp", "mana", "move", "raw", "tick"]
        );
    }
}

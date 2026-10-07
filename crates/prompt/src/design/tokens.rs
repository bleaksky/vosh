//! The tokens of the grammar and the tokenizer that reads a template into
//! them.

use std::fmt;

/// Bar width when the template does not give one.
pub(crate) const BAR_DEFAULT_WIDTH: u8 = 10;
/// The widest bar a template can ask for.
pub(crate) const BAR_MAX_WIDTH: u8 = 80;

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
pub(crate) fn takes_param(name: &str) -> bool {
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
    pub(crate) fn sgr(self) -> &'static str {
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
    pub(crate) fn from_sgr(n: u32) -> Option<UnderlineStyle> {
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
    pub(crate) fn sgr(self) -> &'static str {
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
    /// The percent as the game works it out, cut to a whole number by
    /// integer division, with no sign. 300 of 800 is 37, where `Pct`
    /// rounds it to 38.
    PctGame,
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
    /// `%{right}`, which pushes what follows it on its row to the right
    /// edge of the terminal.
    Right,
    /// `%{raw}`, the game's prompt as it arrived.
    Raw,
    /// A code the grammar does not know. It prints as written.
    Unknown,
}

impl TokenKind {
    /// Hand `read` each field the token reads, its value, its condition
    /// or the field a color follows. `%{raw}` reads the field `raw`.
    pub(crate) fn each_read(&self, read: &mut dyn FnMut(&FieldRef)) {
        match self {
            TokenKind::Value(value) => {
                read(&value.field);
                if let Format::Bar {
                    color: BarColor::Color(ColorSpec::ByValue { field, .. }),
                    ..
                } = &value.format
                {
                    read(field);
                }
            }
            TokenKind::Code(
                Code::Fg(ColorSpec::ByValue { field, .. })
                | Code::Bg(ColorSpec::ByValue { field, .. })
                | Code::UnderlineColor(ColorSpec::ByValue { field, .. }),
            )
            | TokenKind::If(field)
            | TokenKind::IfNot(field) => read(field),
            TokenKind::Raw => read(&FieldRef::new("raw")),
            _ => {}
        }
    }
}

/// A token with its byte range in the template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    pub kind: TokenKind,
}

/// The characters a braced body may hold.
pub(crate) fn brace_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | ',' | '#' | '.' | '=' | '[' | ']')
}

pub(super) fn name_char(c: char) -> bool {
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
pub(crate) fn tokenize(source: &str) -> Vec<Token> {
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
pub(crate) fn named_color(name: &str) -> Option<u8> {
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
pub(crate) fn color_name(index: u8) -> Option<&'static str> {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Layer {
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

pub(super) fn value(field: FieldRef, format: Format) -> TokenKind {
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
pub(crate) fn parse_field<'a>(segs: &'a [&'a str]) -> Option<(FieldRef, &'a [&'a str])> {
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
        "pct" => match args.as_slice() {
            [] => Some(Format::Pct),
            [game] if game == "game" => Some(Format::PctGame),
            _ => None,
        },
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
        if head == "right" {
            return TokenKind::Right;
        }
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
        "nl" | "raw" | "end" | "if" | "ifnot" | "s" | "right"
    ) {
        return TokenKind::Unknown;
    }
    match parse_format(format_segs) {
        Some(format) => value(field, format),
        None => TokenKind::Unknown,
    }
}

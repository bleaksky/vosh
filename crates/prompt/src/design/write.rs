//! Tokens written back as template text.

use super::tokens::{color_name, name_char, ColorSpec, Scale, Style, UnderlineStyle};
use super::tokens::{BarColor, Code, Format, TokenKind, ValueRef};

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
pub(crate) fn style_name(style: Style) -> &'static str {
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
        Format::PctGame => "pct:game".into(),
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
        Format::Zero => "zero".into(),
        Format::PlusMinus => "plusminus".into(),
    })
}

/// Text as a template writes it, each `%` doubled so it stays literal
/// whatever follows it.
pub(crate) fn escape_text(text: &str) -> String {
    text.replace('%', "%%")
}

/// A token as the piece writer writes it: the short form where the
/// grammar has one (`%hp`, `%pct_hp`, `%c_green`, `%s_italic`, `%nl`)
/// unless `braced` asks for braces, and the braced form everywhere else
/// (`%{hp:bar:6}`, `%{c:#80c8ff}`, `%{if:fight}`). An unknown token has no
/// form of its own and writes nothing.
pub(crate) fn write_token(kind: &TokenKind, braced: bool) -> String {
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
        TokenKind::Right => "%{right}".to_string(),
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
pub(crate) fn runs_on(kind: &TokenKind, text: &str, next: Option<char>) -> bool {
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
pub(crate) fn write_tokens(kinds: &[TokenKind]) -> String {
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

//! `prompt_edit`, the only place Vosh writes template text.
//!
//! An edit names a piece by its index in [`Template::pieces`], the index
//! every span of a render carries, so the card can point at the part you
//! clicked. Each op keeps the look of every other piece. A piece's look is
//! the SGR state at its first cell, and SGR state carries across pieces,
//! so an edit that changes what one piece's codes leave behind writes,
//! right after that piece, the codes that give the next piece the look it
//! had before. Nothing else in the template changes.
//!
//! The look is worked out with every condition holding. A condition that
//! the card writes with When (`%{if:fight}` … `%{end}`) ends where it
//! started, so a piece after it has one look in a fight and out of one.
//!
//! The writer keeps the text of every token it does not touch, writes new
//! tokens in their short form where the grammar has one, and uses braces
//! whenever the next character would extend a name.

mod doc;

pub(crate) use doc::Doc;

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::design::{
    self, BarColor, ColorSpec, FieldRef, Format, Item, PieceKind, Scale, Style, Template,
    TokenKind, UnderlineStyle, ValueRef, BAR_MAX_WIDTH,
};
use crate::values::{self, Kind};

/// One change to a design.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum EditOp {
    /// Show a value another way (Show as), or set a bar's width.
    SetFormat { piece: usize, format: FormatChoice },
    /// Color a piece: its text, its ground with `background`, or its
    /// underline with `underline`.
    SetColor {
        piece: usize,
        color: ColorChoice,
        #[serde(default)]
        background: bool,
        #[serde(default)]
        underline: bool,
    },
    /// Turn a style on or off for a piece. An underline kind replaces the
    /// kind the piece had, and any underline turned off is off.
    SetStyle {
        piece: usize,
        style: StyleChoice,
        on: bool,
    },
    /// Show a piece always, only in a fight, or only out of one.
    SetWhen { piece: usize, when: When },
    /// Replace the text of a text piece. Empty text removes it.
    SetText { piece: usize, text: String },
    /// Take a piece out. A condition goes with its end, and an end with
    /// its condition.
    Remove { piece: usize },
    /// Add a value before piece `at`, or at the end when `at` is the
    /// number of pieces.
    InsertField {
        at: usize,
        field: String,
        #[serde(default)]
        format: Option<FormatChoice>,
    },
    /// Add text before piece `at`.
    InsertText { at: usize, text: String },
    /// Add a line break before piece `at`.
    InsertNl { at: usize },
    /// Add a push to the right edge before piece `at`, so what follows it
    /// on its row ends on the terminal's last column.
    InsertRight { at: usize },
    /// Move a piece so it lands before piece `to`, counted before the
    /// move, or at the end when `to` is the number of pieces.
    Move { piece: usize, to: usize },
}

/// How a value shows.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FormatChoice {
    pub format: FormatName,
    /// A bar's width in cells, 1 to 80. A bar keeps its width without it.
    #[serde(default)]
    pub width: Option<u8>,
    /// A bar's color. By value with no field is how full it is.
    #[serde(default)]
    pub color: Option<ColorChoice>,
    /// How many characters `trunc` keeps.
    #[serde(default)]
    pub chars: Option<usize>,
}

impl FormatChoice {
    pub fn of(format: FormatName) -> Self {
        Self {
            format,
            width: None,
            color: None,
            chars: None,
        }
    }
}

/// The formats a value takes, and the two pieces the parser folds:
/// Current and max (`%hp/%{maxhp}`) and Percent (`%pct_hp%%`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatName {
    Value,
    CurMax,
    Max,
    Pct,
    PctGame,
    Percent,
    Bar,
    Game,
    Word,
    Ampm,
    Name,
    Grouped,
    Short,
    Thousands,
    Unit,
    Since,
    Trunc,
    Hm,
    Hms,
    Md,
    Count,
    Names,
    On,
    Off,
    Zero,
    PlusMinus,
}

/// A color the card offers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ColorChoice {
    /// The terminal's own color, `%c_default`.
    Default,
    /// A theme color, 0 to 15.
    Named {
        index: u8,
    },
    /// A 256 palette index.
    Index {
        index: u8,
    },
    Rgb {
        r: u8,
        g: u8,
        b: u8,
    },
    /// By how full a value is, the piece's own value when `field` is
    /// None, by the game's `%h` bands with `game`, or in eleven steps
    /// from red to green with `steps`.
    ByValue {
        #[serde(default)]
        field: Option<String>,
        #[serde(default)]
        game: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        steps: bool,
    },
}

fn is_false(on: &bool) -> bool {
    !*on
}

/// A style the card turns on or off. `Underline` is the single line, and
/// `Double` to `Dashed` the other kinds of underline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StyleChoice {
    Bold,
    Dim,
    Italic,
    Underline,
    Double,
    Curly,
    Dotted,
    Dashed,
    Inverse,
    Strike,
    Blink,
}

/// When a piece shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum When {
    Always,
    /// `%{if:fight}`.
    Fight,
    /// `%{ifnot:fight}`.
    NotFight,
}

/// Why an edit changed nothing, as a sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditError(pub String);

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for EditError {}

fn error<T>(text: &str) -> Result<T, EditError> {
    Err(EditError(text.to_string()))
}

/// Apply `op` to `template` and return the new template. `known` says
/// whether a field has a value Vosh can draw, so the writer never writes
/// an unknown name.
pub fn apply(
    template: &str,
    op: &EditOp,
    known: &dyn Fn(&FieldRef) -> bool,
) -> Result<String, EditError> {
    apply_at(template, op, known).map(|(text, _)| text)
}

/// [`apply`], with where the piece the edit acted on sits in the new
/// template: the piece it changed or moved, or the one it added. A When
/// that adds a condition before the piece moves it on by one, and text
/// added next to text joins it, so the card follows the piece by this.
/// None after a removal, or when nothing was added.
pub fn apply_at(
    template: &str,
    op: &EditOp,
    known: &dyn Fn(&FieldRef) -> bool,
) -> Result<(String, Option<usize>), EditError> {
    let parsed = Template::parse(template);
    let mut doc = Doc::of(&parsed);
    let before = doc.looks();
    let target = match op {
        EditOp::SetFormat { piece, .. }
        | EditOp::SetColor { piece, .. }
        | EditOp::SetStyle { piece, .. }
        | EditOp::SetWhen { piece, .. }
        | EditOp::SetText { piece, .. }
        | EditOp::Move { piece, .. } => doc.pieces.get(*piece).map(|p| p.uid),
        EditOp::Remove { .. } => None,
        EditOp::InsertField { .. }
        | EditOp::InsertText { .. }
        | EditOp::InsertNl { .. }
        | EditOp::InsertRight { .. } => Some(doc.next_uid),
    };
    let edited = doc.run(op, known)?;
    doc.repair(&before, edited);
    let (text, starts) = doc.write()?;
    let at = target
        .and_then(|uid| doc.pieces.iter().position(|p| p.uid == uid))
        .and_then(|index| starts.get(index).copied())
        .and_then(|offset| {
            Template::parse(&text)
                .pieces()
                .iter()
                .position(|p| p.start <= offset && offset < p.end)
        });
    Ok((text, at))
}

/// A field as the card names it, `hp`, `aff:sanctuary` or
/// `gmcp:Char.Vitals.ep`, read by the grammar as the body of a braced
/// field with no format after it.
fn parse_field(text: &str) -> Result<FieldRef, EditError> {
    let text = text.trim();
    if text.chars().all(design::brace_char) {
        let segs: Vec<&str> = text.split(':').collect();
        if let Some((field, [])) = design::parse_field(&segs) {
            return Ok(field);
        }
    }
    error("Vosh does not know that value.")
}

/// The catalog kind of a field, None for a name only scripts set.
pub(crate) fn kind_of(field: &FieldRef) -> Option<Kind> {
    values::entry_for(field).map(|e| e.kind)
}

/// The field a value shows and whether it is a gauge's max: `%{maxhp}`
/// alone is Health in the form Max, so Show as turns it into any form of
/// Health, as the card describes it.
pub(crate) fn gauge_of(value: &ValueRef) -> (FieldRef, bool) {
    let field = &value.field;
    if value.format == Format::Value && field.param.is_none() {
        if let Some(pair) = values::Pair::of(&field.name) {
            if field.name != pair.cur() && field.name != pair.pct() {
                return (FieldRef::new(pair.cur()), true);
            }
        }
    }
    (field.clone(), false)
}

/// The name of a gauge's max as Current and max writes it.
fn max_name(field: &FieldRef) -> String {
    match values::Pair::of(&field.name) {
        Some(pair) => pair.max().to_string(),
        None => format!("max{}", field.name),
    }
}

/// A gauge's max as Current and max writes it, `%{maxhp}`.
fn max_item(field: &FieldRef) -> Item {
    let name = max_name(field);
    Item {
        text: Some(format!("%{{{name}}}")),
        kind: TokenKind::Value(ValueRef {
            field: FieldRef::new(name),
            format: Format::Value,
        }),
    }
}

/// The piece kind and tokens a value shows as. `was` is the format the
/// piece had, whose bar width a new bar keeps.
fn content_for(
    field: &FieldRef,
    choice: &FormatChoice,
    was: Option<&Format>,
) -> Result<(PieceKind, Vec<Item>), EditError> {
    use values::FormatId as F;
    let kind = kind_of(field);
    let offered = |name: FormatName| -> bool {
        let Some(kind) = kind else {
            // A script value reads as text or a number, so any form a
            // number takes is fine.
            return true;
        };
        let id = match name {
            FormatName::CurMax | FormatName::Max => {
                return kind == Kind::Gauge || (name == FormatName::Max && kind == Kind::Seconds);
            }
            FormatName::Percent => {
                return matches!(kind, Kind::Gauge | Kind::Pct | Kind::TankPct | Kind::Member);
            }
            FormatName::Value => F::Value,
            FormatName::Pct => F::Pct,
            FormatName::PctGame => F::PctGame,
            FormatName::Bar => F::Bar,
            FormatName::Game => F::Game,
            FormatName::Word => F::Word,
            FormatName::Ampm => F::Ampm,
            FormatName::Name => F::Name,
            FormatName::Grouped => F::Grouped,
            FormatName::Short => F::Short,
            FormatName::Thousands => F::Thousands,
            FormatName::Unit => F::Unit,
            FormatName::Since => F::Since,
            FormatName::Trunc => F::Trunc,
            FormatName::Hm => F::Hm,
            FormatName::Hms => F::Hms,
            FormatName::Md => F::Md,
            FormatName::Count => F::Count,
            FormatName::Names => F::Names,
            FormatName::On => F::On,
            FormatName::Off => F::Off,
            FormatName::Zero => F::Zero,
            FormatName::PlusMinus => F::PlusMinus,
        };
        id == F::Value || kind.formats().contains(&id)
    };
    if !offered(choice.format) {
        return error("Vosh cannot show that value that way.");
    }
    let value = |format: Format| {
        Item::new(TokenKind::Value(ValueRef {
            field: field.clone(),
            format,
        }))
    };
    let one = |format: Format| Ok((PieceKind::Value, vec![value(format)]));
    // The game's prompt is a token of its own, `%{raw}`.
    if kind == Some(Kind::Raw) {
        return match choice.format {
            FormatName::Value => Ok((PieceKind::Raw, vec![Item::new(TokenKind::Raw)])),
            _ => error("Vosh cannot show that value that way."),
        };
    }
    match choice.format {
        FormatName::Value => one(Format::Value),
        FormatName::Max if values::Pair::of(&field.name).is_some() && field.param.is_none() => {
            Ok((PieceKind::Value, vec![max_item(field)]))
        }
        FormatName::Max => one(Format::Max),
        FormatName::CurMax => {
            if field.param.is_some() {
                return error("Vosh cannot show that value that way.");
            }
            Ok((
                PieceKind::CurMax,
                vec![value(Format::Value), Item::text("/"), max_item(field)],
            ))
        }
        FormatName::Pct => one(Format::Pct),
        FormatName::PctGame => one(Format::PctGame),
        FormatName::Percent => Ok((
            PieceKind::Percent,
            vec![value(Format::Pct), Item::new(TokenKind::Percent)],
        )),
        FormatName::Bar => {
            let (old_width, old_color) = match was {
                Some(Format::Bar { width, color }) => (*width, color.clone()),
                _ => (crate::design::BAR_DEFAULT_WIDTH, BarColor::Auto),
            };
            let width = choice.width.unwrap_or(old_width);
            if !(1..=BAR_MAX_WIDTH).contains(&width) {
                return error("A bar is 1 to 80 cells wide.");
            }
            let color = match &choice.color {
                None => old_color,
                Some(ColorChoice::ByValue {
                    field: None,
                    game: false,
                    steps: false,
                }) => BarColor::Auto,
                Some(ColorChoice::ByValue { game: true, .. }) => {
                    return error("A bar cannot take the game's colors for health.");
                }
                Some(ColorChoice::ByValue { steps: true, .. }) => return error(BAR_STEPS),
                Some(other) => BarColor::Color(color_spec(other, Some(field))?),
            };
            one(Format::Bar { width, color })
        }
        FormatName::Game => one(Format::Game),
        FormatName::Word => one(Format::Word),
        FormatName::Ampm => one(Format::Ampm),
        FormatName::Name => one(Format::Name),
        FormatName::Grouped => one(Format::Grouped),
        FormatName::Short => one(Format::Short),
        FormatName::Thousands => one(Format::Thousands),
        FormatName::Unit => one(Format::Unit),
        FormatName::Since => one(Format::Since),
        FormatName::Trunc => one(Format::Trunc(choice.chars.unwrap_or(20).max(1))),
        FormatName::Hm => one(Format::Hm),
        FormatName::Hms => one(Format::Hms),
        FormatName::Md => one(Format::Md),
        FormatName::Count => one(Format::Count),
        FormatName::Names => one(Format::Names),
        FormatName::On => one(Format::On),
        FormatName::Off => one(Format::Off),
        FormatName::Zero => one(Format::Zero),
        FormatName::PlusMinus => one(Format::PlusMinus),
    }
}

/// Why a bar takes no steps: its cells take one color, by thirds or as
/// you choose.
const BAR_STEPS: &str = "A bar cannot take the steps from red to green.";

/// The color spec a choice names. By value with no field takes `own`,
/// the piece's own value.
fn color_spec(choice: &ColorChoice, own: Option<&FieldRef>) -> Result<ColorSpec, EditError> {
    Ok(match choice {
        ColorChoice::Default => ColorSpec::Default,
        ColorChoice::Named { index } if *index < 16 => ColorSpec::Named(*index),
        ColorChoice::Named { .. } => return error("A theme color is one of the sixteen."),
        ColorChoice::Index { index } => ColorSpec::Index(*index),
        ColorChoice::Rgb { r, g, b } => ColorSpec::Rgb(*r, *g, *b),
        ColorChoice::ByValue { field, game, steps } => {
            let scale = match (game, steps) {
                (false, false) => Scale::Thirds,
                (true, false) => Scale::Game,
                (false, true) => Scale::Steps,
                (true, true) => {
                    return error("A color follows the game's bands or the steps, not both.")
                }
            };
            let field = match field {
                Some(name) => parse_field(name)?,
                None => match own {
                    Some(own) => own.clone(),
                    None => return error("Only a value can take its color from how full it is."),
                },
            };
            if field.param.is_some() {
                return error("Only a value can take its color from how full it is.");
            }
            ColorSpec::ByValue { field, scale }
        }
    })
}

/// The color a choice names for a look's text color.
pub(crate) fn choice(spec: Option<&ColorSpec>, own: Option<&FieldRef>) -> ColorChoice {
    match spec {
        None | Some(ColorSpec::Default) => ColorChoice::Default,
        Some(ColorSpec::Named(index)) => ColorChoice::Named { index: *index },
        Some(ColorSpec::Index(index)) => ColorChoice::Index { index: *index },
        Some(ColorSpec::Rgb(r, g, b)) => ColorChoice::Rgb {
            r: *r,
            g: *g,
            b: *b,
        },
        Some(ColorSpec::ByValue { field, scale }) => ColorChoice::ByValue {
            field: (Some(field) != own).then(|| field.to_string()),
            game: *scale == Scale::Game,
            steps: *scale == Scale::Steps,
        },
    }
}

/// The color a choice names for a bar's cells.
pub(crate) fn bar_choice(color: &BarColor, own: Option<&FieldRef>) -> ColorChoice {
    match color {
        BarColor::Auto => ColorChoice::ByValue {
            field: None,
            game: false,
            steps: false,
        },
        BarColor::Game => ColorChoice::ByValue {
            field: None,
            game: true,
            steps: false,
        },
        BarColor::Color(spec) => choice(Some(spec), own),
    }
}

fn style_of(style: StyleChoice) -> Style {
    match style {
        StyleChoice::Bold => Style::Bold,
        StyleChoice::Dim => Style::Dim,
        StyleChoice::Italic => Style::Italic,
        StyleChoice::Underline => Style::Underline(UnderlineStyle::Single),
        StyleChoice::Double => Style::Underline(UnderlineStyle::Double),
        StyleChoice::Curly => Style::Underline(UnderlineStyle::Curly),
        StyleChoice::Dotted => Style::Underline(UnderlineStyle::Dotted),
        StyleChoice::Dashed => Style::Underline(UnderlineStyle::Dashed),
        StyleChoice::Inverse => Style::Inverse,
        StyleChoice::Strike => Style::Strike,
        StyleChoice::Blink => Style::Blink,
    }
}

/// The choice that names an underline kind, the reverse of [`style_of`].
pub(crate) fn underline_choice(line: UnderlineStyle) -> StyleChoice {
    match line {
        UnderlineStyle::Single => StyleChoice::Underline,
        UnderlineStyle::Double => StyleChoice::Double,
        UnderlineStyle::Curly => StyleChoice::Curly,
        UnderlineStyle::Dotted => StyleChoice::Dotted,
        UnderlineStyle::Dashed => StyleChoice::Dashed,
    }
}

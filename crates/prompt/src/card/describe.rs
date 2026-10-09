//! What the prompt card shows about a design.
//!
//! The card never parses a template itself, so a design never reads
//! two ways. For each piece of the
//! design it reads here what the piece is, the field it shows and how,
//! when it shows, the color and style its rows check, and what it reads
//! now. For Edit as text it reads each token with the piece it belongs
//! to, so the token under the caret and the part it draws carry the same
//! mark. For Show as and the picker it reads the forms a value takes, each
//! with a sample drawn with the values the card shows.
//!
//! A piece's look is the SGR state at its first cell with every condition
//! holding, as [`crate::card::edit`] reads it, so the rows show what an
//! edit keeps. A bar draws its cells in its own color, so its Color row
//! reads the bar's color.

use serde::Serialize;

use crate::card::edit::{
    self, bar_choice, choice, ColorChoice, Doc, EditOp, FormatChoice, FormatName, StyleChoice, When,
};
use crate::design::{FieldRef, Format, PieceKind, Template, TokenKind};
use crate::render::{render, RenderOptions, Rendered};
use crate::values::format::{Resolved, Value};
use crate::values::{self, Group, Kind, Values};

/// One form a value takes, for Show as and the picker's formats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FormView {
    pub format: FormatName,
    /// Its name in the picker, `Current and max`.
    pub label: &'static str,
    /// What its Show as segment reads: the sample when it is short, as
    /// `1020/1020`, else the name, as `Bar`.
    pub segment: String,
    /// The value drawn in this form alone, in its default color.
    pub sample: Rendered,
    /// Show as offers it. The picker offers every form.
    pub show_as: bool,
}

/// One piece of a design as the card shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PieceView {
    /// Its index, as every span of a render names it.
    pub piece: usize,
    pub kind: PieceKind,
    /// The template text of the piece, its own codes included, exactly as
    /// written. The name line shows it.
    pub text: String,
    /// The field it shows, `hp` or `aff:sanctuary`, for a value. A max
    /// shows as its gauge's field in the form Max.
    pub field: Option<String>,
    /// Its name in the card: the field's label, `Text`, `Line break`.
    pub label: String,
    /// How the value shows now.
    pub format: Option<FormatName>,
    /// A bar's width in cells.
    pub width: Option<u8>,
    /// When it shows: inside `%{if:fight}`, `%{ifnot:fight}`, or always.
    pub when: When,
    /// A fight condition outside another condition holds it, so When
    /// cannot change it.
    pub when_fixed: bool,
    /// The color the Color row checks: its text color at its first cell,
    /// or a bar's own color. By value with no field is its own value.
    pub color: ColorChoice,
    /// The ground at its first cell, Default for the terminal's own.
    pub background: ColorChoice,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    /// An underline of any kind is on.
    pub underline: bool,
    /// The kind of underline, `underline` for the single line, None with
    /// no underline.
    pub underline_style: Option<StyleChoice>,
    /// The underline's color, Default for the text's own.
    pub underline_color: ColorChoice,
    pub inverse: bool,
    pub strike: bool,
    pub blink: bool,
    /// What a text piece prints.
    pub literal: Option<String>,
    /// What the value reads now, `1020 of 1020` or `60 percent`, with
    /// `in this preview` while a preview draws it.
    pub meta: Option<String>,
    /// The forms Show as offers, with the one it shows now among them.
    pub forms: Vec<FormView>,
    /// Its value has a fullness, so By value colors it.
    pub by_value: bool,
    /// It takes cells, so the card can pick it.
    pub shows: bool,
}

/// What a token is, for the colors of Edit as text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenKindName {
    Text,
    Code,
    Value,
    Condition,
    Line,
    Raw,
    Unknown,
}

/// One token of a design, with its place in the text in UTF-16 units, as
/// the webview counts them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TokenView {
    pub start: usize,
    pub end: usize,
    pub piece: usize,
    pub kind: TokenKindName,
    /// The name a value token reads.
    pub name: Option<String>,
    /// Vosh has a value by that name. An unknown name prints as written.
    pub known: bool,
}

/// A design as the card shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Described {
    pub pieces: Vec<PieceView>,
    pub tokens: Vec<TokenView>,
}

/// Describe `template` with `values`. `preview` says the values are a
/// preview's, which the meta says.
pub fn describe(template: &Template, values: &dyn Values, preview: bool) -> Described {
    let doc = Doc::of(template);
    let (_, looks) = doc.walk();
    let pieces = template
        .pieces()
        .iter()
        .enumerate()
        .map(|(index, piece)| {
            let edited = &doc.pieces[index];
            let look = &looks[index];
            let shown = edited.value().map(shown_as);
            let (field, format, width) = match &shown {
                Some((field, format, width)) => (Some(field.clone()), Some(*format), *width),
                None => (None, None, None),
            };
            // A piece the parser folds shows in the form it folds into:
            // `%hp/%{maxhp}` as Current and max, `%pct_hp%%` as Percent.
            let format = match edited.kind {
                PieceKind::CurMax => format.map(|_| FormatName::CurMax),
                PieceKind::Percent => format.map(|_| FormatName::Percent),
                _ => format,
            };
            let (when, when_fixed) = when_of(&doc, index);
            let color = match edited.value().map(|v| &v.format) {
                Some(Format::Bar { color, .. }) => bar_choice(color, field.as_ref()),
                _ => choice(look.fg.as_ref(), field.as_ref()),
            };
            let kind = edited.kind;
            let literal = (kind == PieceKind::Text).then(|| literal(template, piece));
            let label = match (&field, kind) {
                (Some(field), _) => values.label(field),
                (None, PieceKind::Text) => "Text".to_string(),
                (None, PieceKind::Nl) => "Line break".to_string(),
                (None, PieceKind::Right) => "Right edge".to_string(),
                (None, PieceKind::Raw) => "The game's prompt".to_string(),
                (None, PieceKind::If | PieceKind::IfNot | PieceKind::End) => {
                    "Condition".to_string()
                }
                _ => template.piece_text(index).to_string(),
            };
            let forms = match (&field, format) {
                (Some(field), Some(format)) => show_as(field, format, values),
                _ => Vec::new(),
            };
            PieceView {
                piece: index,
                kind,
                text: template.piece_text(index).to_string(),
                field: field.as_ref().map(ToString::to_string),
                label,
                format,
                width,
                when,
                when_fixed,
                color,
                background: choice(look.bg.as_ref(), field.as_ref()),
                bold: look.bold,
                dim: look.dim,
                italic: look.italic,
                underline: look.underline.is_some(),
                underline_style: look.underline.map(edit::underline_choice),
                underline_color: choice(look.underline_color.as_ref(), field.as_ref()),
                inverse: look.inverse,
                strike: look.strike,
                blink: look.blink,

                literal,
                meta: field.as_ref().and_then(|f| meta(f, values, preview)),
                forms,
                by_value: field.as_ref().is_some_and(full),
                shows: edited.shows(),
            }
        })
        .collect();
    Described {
        pieces,
        tokens: tokens(template, values),
    }
}

/// The forms `field` takes, each drawn with `values`, for the picker.
pub fn forms(field: &FieldRef, values: &dyn Values) -> Vec<FormView> {
    form_list(edit::kind_of(field))
        .iter()
        .map(|&(format, label, show_as)| form(field, format, label, show_as, values))
        .collect()
}

/// The forms Show as offers for a value shown in `now`, which is among
/// them even when only the picker offers it.
fn show_as(field: &FieldRef, now: FormatName, values: &dyn Values) -> Vec<FormView> {
    let list = form_list(edit::kind_of(field));
    let mut out: Vec<FormView> = list
        .iter()
        .filter(|(format, _, show)| *show || *format == now)
        .map(|&(format, label, show)| form(field, format, label, show, values))
        .collect();
    if !list.iter().any(|(format, _, _)| *format == now) {
        out.push(form(field, now, format_label(now), true, values));
    }
    out
}

fn form(
    field: &FieldRef,
    format: FormatName,
    label: &'static str,
    show_as: bool,
    values: &dyn Values,
) -> FormView {
    let op = EditOp::InsertField {
        at: 0,
        field: field.to_string(),
        format: Some(FormatChoice::of(format)),
    };
    let sample = edit::apply("", &op, &|_: &FieldRef| true)
        .map(|text| render(&Template::parse(&text), values, RenderOptions::default()))
        .unwrap_or_default();
    let segment = match format {
        // The game's percent reads as the rounded one most of the time,
        // so it goes by its name, as a bar does. The name says it has no
        // sign, since picking it on a Percent drops the sign.
        FormatName::Bar | FormatName::PctGame => label.to_string(),
        FormatName::Game if edit::kind_of(field) == Some(Kind::TankPct) => label.to_string(),
        _ if sample.plain.is_empty() || sample.plain.chars().count() > 16 => label.to_string(),
        _ => sample.plain.clone(),
    };
    FormView {
        format,
        label,
        segment,
        sample,
        show_as,
    }
}

/// Each form a kind of field takes, its name, and whether Show as offers
/// it. A gauge's max changes what the piece reads, so only the picker
/// offers it. A gauge takes five forms and the percent the game works
/// out. Grouped, short and thousands are a number's forms.
fn form_list(kind: Option<Kind>) -> &'static [(FormatName, &'static str, bool)] {
    use FormatName as F;
    let Some(kind) = kind else {
        return &[(F::Value, "Text", true)];
    };
    match kind {
        Kind::Gauge => &[
            (F::Value, "Current", true),
            (F::CurMax, "Current and max", true),
            (F::Max, "Max", false),
            (F::Percent, "Percent", true),
            (F::PctGame, "Game percent, no sign", true),
            (F::Bar, "Bar", true),
        ],
        Kind::Num => &[
            (F::Value, "Number", true),
            (F::Grouped, "Grouped", true),
            (F::Short, "Short", true),
            (F::Thousands, "Thousands", true),
        ],
        Kind::Pct => &[(F::Percent, "Percent", true), (F::Bar, "Bar", true)],
        Kind::TankPct => &[
            (F::Percent, "Percent", true),
            (F::Game, "Game bar", true),
            (F::Bar, "Bar", true),
        ],
        Kind::Text => &[(F::Value, "Text", true), (F::Trunc, "First 20", true)],
        Kind::Raw => &[(F::Value, "As sent", true)],
        Kind::Flag => &[
            (F::On, "Mark when on", true),
            (F::Off, "Mark when off", true),
        ],
        Kind::Count => &[(F::Count, "Count", true), (F::Names, "Names", true)],
        Kind::Position => &[
            (F::Value, "Short", true),
            (F::Word, "Word", true),
            (F::Game, "Game style", true),
        ],
        Kind::Lang => &[(F::Value, "Word", true), (F::Game, "Game style", true)],
        Kind::Exits => &[(F::Value, "Letters", true), (F::Game, "Game style", true)],
        Kind::Level => &[(F::Value, "Number", true), (F::Game, "Game style", true)],
        Kind::Slot => &[(F::Value, "Time left", true), (F::Game, "Game style", true)],
        Kind::Moon => &[
            (F::Game, "Game style", true),
            (F::Word, "Word", true),
            (F::Name, "Phase name", true),
        ],
        Kind::Hour => &[
            (F::Value, "Number", true),
            (F::Word, "Clock", true),
            (F::Ampm, "Compact clock", true),
        ],
        Kind::Temp => &[(F::Value, "Number", true), (F::Unit, "With unit", true)],
        Kind::Seconds => &[
            (F::Value, "Seconds", true),
            (F::Unit, "With unit", true),
            (F::Since, "Since the tick", true),
            (F::Bar, "Bar", true),
        ],
        Kind::Clock => &[
            (F::Hm, "Hours and minutes", true),
            (F::Hms, "With seconds", true),
        ],
        Kind::Date => &[
            (F::Md, "Month and day", true),
            (F::Value, "Full date", true),
        ],
        Kind::Ticks => &[
            (F::Value, "Time left", true),
            (F::On, "Mark when on", true),
            (F::Off, "Mark when off", true),
        ],
        Kind::Change => &[
            (F::Value, "Nothing at zero", true),
            (F::Zero, "0 at zero", true),
            (F::PlusMinus, "±0 at zero", true),
        ],
        Kind::Member => &[
            (F::Percent, "Percent", true),
            (F::Bar, "Bar", true),
            (F::Name, "Name", true),
            (F::Value, "Name and percent", true),
        ],
    }
}

/// The name of a form no list offers for a kind, such as the percent with
/// no sign a design wrote by hand.
fn format_label(format: FormatName) -> &'static str {
    match format {
        FormatName::Value => "Value",
        FormatName::CurMax => "Current and max",
        FormatName::Max => "Max",
        FormatName::Pct => "Percent, no sign",
        FormatName::PctGame => "Game percent, no sign",
        FormatName::Percent => "Percent",
        FormatName::Bar => "Bar",
        FormatName::Game => "Game style",
        FormatName::Word => "Word",
        FormatName::Ampm => "Compact clock",
        FormatName::Name => "Name",
        FormatName::Grouped => "Grouped",
        FormatName::Short => "Short",
        FormatName::Thousands => "Thousands",
        FormatName::Unit => "With unit",
        FormatName::Since => "Since the tick",
        FormatName::Trunc => "Shortened",
        FormatName::Hm => "Hours and minutes",
        FormatName::Hms => "With seconds",
        FormatName::Md => "Month and day",
        FormatName::Count => "Count",
        FormatName::Names => "Names",
        FormatName::On => "Mark when on",
        FormatName::Off => "Mark when off",
        FormatName::Zero => "0 at zero",
        FormatName::PlusMinus => "±0 at zero",
    }
}

/// The field a value piece shows, how and how wide. `%{maxhp}` alone is
/// Health in the form Max.
fn shown_as(value: &crate::design::ValueRef) -> (FieldRef, FormatName, Option<u8>) {
    let (gauge, max) = edit::gauge_of(value);
    if max {
        return (gauge, FormatName::Max, None);
    }
    let field = &value.field;
    let (format, width) = match &value.format {
        Format::Value => (FormatName::Value, None),
        Format::Max => (FormatName::Max, None),
        Format::Pct => (FormatName::Pct, None),
        Format::PctGame => (FormatName::PctGame, None),
        Format::Bar { width, .. } => (FormatName::Bar, Some(*width)),
        Format::Game => (FormatName::Game, None),
        Format::Word => (FormatName::Word, None),
        Format::Ampm => (FormatName::Ampm, None),
        Format::Name => (FormatName::Name, None),
        Format::Grouped => (FormatName::Grouped, None),
        Format::Short => (FormatName::Short, None),
        Format::Thousands => (FormatName::Thousands, None),
        Format::Unit => (FormatName::Unit, None),
        Format::Since => (FormatName::Since, None),
        Format::Trunc(_) => (FormatName::Trunc, None),
        Format::Hm => (FormatName::Hm, None),
        Format::Hms => (FormatName::Hms, None),
        Format::Md => (FormatName::Md, None),
        Format::Count => (FormatName::Count, None),
        Format::Names => (FormatName::Names, None),
        Format::On => (FormatName::On, None),
        Format::Off => (FormatName::Off, None),
        Format::Zero => (FormatName::Zero, None),
        Format::PlusMinus => (FormatName::PlusMinus, None),
    };
    (field.clone(), format, width)
}

/// When piece `index` shows, and whether When can change it, as
/// [`crate::card::edit`] decides.
fn when_of(doc: &Doc, index: usize) -> (When, bool) {
    let around = doc.around(index);
    let fights: Vec<PieceKind> = around
        .iter()
        .filter_map(|(open, _)| doc.pieces[*open].fight())
        .collect();
    let inner = around
        .first()
        .and_then(|(open, _)| doc.pieces[*open].fight());
    let when = match fights.first() {
        Some(PieceKind::If) => When::Fight,
        Some(PieceKind::IfNot) => When::NotFight,
        _ => When::Always,
    };
    (when, fights.len() > usize::from(inner.is_some()))
}

/// The value has a fullness, so By value and a bar's colors follow it.
fn full(field: &FieldRef) -> bool {
    matches!(
        edit::kind_of(field),
        Some(Kind::Gauge | Kind::Pct | Kind::TankPct | Kind::Member | Kind::Seconds)
    )
}

/// What a text piece prints, `%%` as one `%`.
fn literal(template: &Template, piece: &crate::design::Piece) -> String {
    let mut out = String::new();
    for token in &template.tokens()[piece.content.clone()] {
        match &token.kind {
            TokenKind::Text(text) => out.push_str(text),
            TokenKind::Percent => out.push('%'),
            _ => {}
        }
    }
    out
}

/// What a field reads now, as the name line says it.
fn meta(field: &FieldRef, values: &dyn Values, preview: bool) -> Option<String> {
    let said = match values.resolve(field) {
        Resolved::Value(value) => match &value {
            Value::Gauge {
                cur,
                max: Some(max),
                ..
            } => format!("{cur} of {max}"),
            Value::Decimal {
                text,
                max: Some(max),
                ..
            } => format!("{text} of {max}"),
            Value::Pct(pct) => format!("{pct} percent"),
            _ => value.text(&Format::Value, &values.label(field))?,
        },
        Resolved::Hidden => "The game hides it".to_string(),
        Resolved::Absent if values::entry_for(field).is_some_and(|e| e.group == Group::Fight) => {
            return Some("Only in a fight".to_string());
        }
        _ => return None,
    };
    if said.is_empty() {
        return None;
    }
    Some(if preview {
        format!("{said} in this preview")
    } else {
        said
    })
}

/// Each token with the piece it belongs to, in UTF-16 units.
fn tokens(template: &Template, values: &dyn Values) -> Vec<TokenView> {
    let source = template.source();
    let utf16 = |byte: usize| source[..byte].encode_utf16().count();
    let mut owner = vec![0; template.tokens().len()];
    for (index, piece) in template.pieces().iter().enumerate() {
        for token in piece.codes.clone().chain(piece.content.clone()) {
            owner[token] = index;
        }
    }
    template
        .tokens()
        .iter()
        .enumerate()
        .map(|(i, token)| {
            let (kind, name, known) = match &token.kind {
                TokenKind::Text(_) | TokenKind::Percent => (TokenKindName::Text, None, true),
                TokenKind::Code(_) => (TokenKindName::Code, None, true),
                TokenKind::Value(value) => (
                    TokenKindName::Value,
                    Some(value.field.to_string()),
                    !matches!(values.resolve(&value.field), Resolved::Unknown),
                ),
                TokenKind::If(field) | TokenKind::IfNot(field) => (
                    TokenKindName::Condition,
                    Some(field.to_string()),
                    !matches!(values.resolve(field), Resolved::Unknown),
                ),
                TokenKind::End => (TokenKindName::Condition, None, true),
                TokenKind::Nl | TokenKind::Right => (TokenKindName::Line, None, true),
                TokenKind::Raw => (TokenKindName::Raw, None, true),
                TokenKind::Unknown => (TokenKindName::Unknown, None, false),
            };
            TokenView {
                start: utf16(token.start),
                end: utf16(token.end),
                piece: owner[i],
                kind,
                name,
                known,
            }
        })
        .collect()
}

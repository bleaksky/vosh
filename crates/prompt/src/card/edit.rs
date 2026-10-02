//! `prompt_edit`, the only place Vosh writes template text (sections 1.4
//! and 6 of the build spec).
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
//! whenever the next character would extend a name (D15).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::design::{
    self, bg, code, color, fg, restore, runs_on, transition, underline_color, write_token,
    BarColor, Code, ColorSpec, FieldRef, Format, Item, Layer, Look, PieceKind, Scale, Style,
    Template, TokenKind, UnderlineStyle, ValueRef, BAR_MAX_WIDTH,
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

/// The formats of section 1.4, and the two pieces the parser folds:
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

// ---------------------------------------------------------------------
// The design as pieces
// ---------------------------------------------------------------------

/// A piece of the design being edited.
#[derive(Debug, Clone)]
pub(crate) struct Piece {
    /// Which piece this is, however the pieces move.
    uid: usize,
    /// The piece of the template before the edit, None for a new one.
    origin: Option<usize>,
    pub(crate) kind: PieceKind,
    codes: Vec<Item>,
    content: Vec<Item>,
}

impl Piece {
    fn marker(&self) -> bool {
        matches!(
            self.kind,
            PieceKind::If | PieceKind::IfNot | PieceKind::End | PieceKind::Nl | PieceKind::Right
        )
    }

    /// The piece takes cells, or would with a value: not a marker and
    /// not codes alone.
    pub(crate) fn shows(&self) -> bool {
        !self.marker() && self.kind != PieceKind::Codes
    }

    /// The first value the piece reads.
    pub(crate) fn value(&self) -> Option<&ValueRef> {
        self.content.iter().find_map(|item| match &item.kind {
            TokenKind::Value(value) => Some(value),
            _ => None,
        })
    }

    /// The field of a fight condition, and whether it is `%{if:fight}`.
    pub(crate) fn fight(&self) -> Option<PieceKind> {
        match self.content.first().map(|item| &item.kind) {
            Some(TokenKind::If(f) | TokenKind::IfNot(f))
                if f.name == "fight" && f.param.is_none() =>
            {
                Some(self.kind)
            }
            _ => None,
        }
    }
}

/// The looks of the pieces before the edit, by origin.
struct Looks {
    /// The look before each piece's codes.
    before: Vec<Look>,
    /// The look at each piece's first cell.
    at: Vec<Look>,
}

pub(crate) struct Doc {
    pub(crate) pieces: Vec<Piece>,
    next_uid: usize,
}

impl Doc {
    pub(crate) fn of(template: &Template) -> Self {
        let item = |index: usize| Item {
            kind: template.tokens()[index].kind.clone(),
            text: Some(template.token_text(index).to_string()),
        };
        let pieces: Vec<Piece> = template
            .pieces()
            .iter()
            .enumerate()
            .map(|(index, piece)| Piece {
                uid: index,
                origin: Some(index),
                kind: piece.kind,
                codes: piece.codes.clone().map(item).collect(),
                content: piece.content.clone().map(item).collect(),
            })
            .collect();
        let next_uid = pieces.len();
        Self { pieces, next_uid }
    }

    fn new_piece(&mut self, kind: PieceKind, codes: Vec<Item>, content: Vec<Item>) -> Piece {
        let uid = self.next_uid;
        self.next_uid += 1;
        Piece {
            uid,
            origin: None,
            kind,
            codes,
            content,
        }
    }

    fn marker_piece(&mut self, kind: TokenKind) -> Piece {
        let piece_kind = match kind {
            TokenKind::If(_) => PieceKind::If,
            TokenKind::IfNot(_) => PieceKind::IfNot,
            TokenKind::End => PieceKind::End,
            TokenKind::Right => PieceKind::Right,
            _ => PieceKind::Nl,
        };
        self.new_piece(piece_kind, Vec::new(), vec![Item::new(kind)])
    }

    /// The look before each piece and at its first cell, every condition
    /// holding.
    pub(crate) fn walk(&self) -> (Vec<Look>, Vec<Look>) {
        let mut state = Look::default();
        let mut before = Vec::with_capacity(self.pieces.len());
        let mut at = Vec::with_capacity(self.pieces.len());
        for piece in &self.pieces {
            before.push(state.clone());
            state = state.after(&piece.codes);
            at.push(state.clone());
        }
        (before, at)
    }

    fn looks(&self) -> Looks {
        let (before, at) = self.walk();
        Looks { before, at }
    }

    fn piece(&self, index: usize) -> Result<&Piece, EditError> {
        match self.pieces.get(index) {
            Some(piece) => Ok(piece),
            None => error("That part is no longer in your design."),
        }
    }

    fn check_at(&self, at: usize) -> Result<(), EditError> {
        if at > self.pieces.len() {
            return error("That place is no longer in your design.");
        }
        Ok(())
    }

    /// Run `op`. Returns the piece whose look changes on purpose.
    fn run(
        &mut self,
        op: &EditOp,
        known: &dyn Fn(&FieldRef) -> bool,
    ) -> Result<Option<usize>, EditError> {
        match op {
            EditOp::SetFormat { piece, format } => {
                self.set_format(*piece, format)?;
                Ok(None)
            }
            EditOp::SetColor {
                piece,
                color,
                background,
                underline,
            } => {
                let layer = match (background, underline) {
                    (false, false) => Layer::Fg,
                    (true, false) => Layer::Bg,
                    (false, true) => Layer::Underline,
                    (true, true) => {
                        return error("A color goes on the text, its ground, or its underline.")
                    }
                };
                self.set_color(*piece, color, layer).map(Some)
            }
            EditOp::SetStyle { piece, style, on } => {
                self.set_style(*piece, style_of(*style), *on).map(Some)
            }
            EditOp::SetWhen { piece, when } => {
                self.set_when(*piece, *when)?;
                Ok(None)
            }
            EditOp::SetText { piece, text } => {
                let found = self.piece(*piece)?;
                if found.kind != PieceKind::Text {
                    return error("Only text can change its words.");
                }
                if text.is_empty() {
                    self.pieces.remove(*piece);
                } else {
                    self.pieces[*piece].content = vec![Item::text(text)];
                }
                Ok(None)
            }
            EditOp::Remove { piece } => {
                self.remove(*piece)?;
                Ok(None)
            }
            EditOp::InsertField { at, field, format } => {
                self.check_at(*at)?;
                let field = parse_field(field)?;
                if !known(&field) {
                    return error("Vosh does not know that value.");
                }
                let choice = format
                    .clone()
                    .unwrap_or_else(|| FormatChoice::of(FormatName::Value));
                let (kind, content) = content_for(&field, &choice, None)?;
                let piece = self.new_piece(kind, Vec::new(), content);
                self.pieces.insert(*at, piece);
                Ok(None)
            }
            EditOp::InsertText { at, text } => {
                self.check_at(*at)?;
                if !text.is_empty() {
                    let piece = self.new_piece(PieceKind::Text, Vec::new(), vec![Item::text(text)]);
                    self.pieces.insert(*at, piece);
                }
                Ok(None)
            }
            EditOp::InsertNl { at } => {
                self.check_at(*at)?;
                let piece = self.marker_piece(TokenKind::Nl);
                self.pieces.insert(*at, piece);
                Ok(None)
            }
            EditOp::InsertRight { at } => {
                self.check_at(*at)?;
                let piece = self.marker_piece(TokenKind::Right);
                self.pieces.insert(*at, piece);
                Ok(None)
            }
            EditOp::Move { piece, to } => {
                self.piece(*piece)?;
                self.check_at(*to)?;
                if *to == *piece || *to == *piece + 1 {
                    return Ok(None);
                }
                let moved = self.pieces.remove(*piece);
                let at = if *to > *piece { *to - 1 } else { *to };
                self.pieces.insert(at, moved);
                Ok(None)
            }
        }
    }

    fn set_format(&mut self, index: usize, choice: &FormatChoice) -> Result<(), EditError> {
        let piece = self.piece(index)?;
        let Some(value) = piece.value().cloned() else {
            return error("Only a value can change how it shows.");
        };
        if !matches!(
            piece.kind,
            PieceKind::Value | PieceKind::CurMax | PieceKind::Percent
        ) {
            return error("Only a value can change how it shows.");
        }
        let (field, _) = gauge_of(&value);
        let (kind, content) = content_for(&field, choice, Some(&value.format))?;
        let piece = &mut self.pieces[index];
        piece.kind = kind;
        piece.content = content;
        Ok(())
    }

    fn set_color(
        &mut self,
        index: usize,
        choice: &ColorChoice,
        layer: Layer,
    ) -> Result<usize, EditError> {
        let piece = self.piece(index)?;
        if !piece.shows() {
            return error("Only a part that shows can take a color.");
        }
        let own = piece.value().map(|v| v.field.clone());
        let spec = color_spec(choice, own.as_ref())?;
        let uid = piece.uid;
        // A bar draws its cells in its own color.
        if layer == Layer::Fg {
            if let [Item {
                kind:
                    TokenKind::Value(ValueRef {
                        field,
                        format: Format::Bar { width, .. },
                    }),
                ..
            }] = piece.content.as_slice()
            {
                let color = match &spec {
                    ColorSpec::ByValue {
                        field: f,
                        scale: Scale::Thirds,
                    } if Some(f) == own.as_ref() => BarColor::Auto,
                    ColorSpec::ByValue {
                        scale: Scale::Game, ..
                    } => {
                        return error("A bar cannot take the game's colors for health.");
                    }
                    ColorSpec::ByValue {
                        scale: Scale::Steps,
                        ..
                    } => return error(BAR_STEPS),
                    other => BarColor::Color(other.clone()),
                };
                let value = ValueRef {
                    field: field.clone(),
                    format: Format::Bar {
                        width: *width,
                        color,
                    },
                };
                self.pieces[index].content = vec![Item::new(TokenKind::Value(value))];
                return Ok(uid);
            }
        }
        let (before, _) = self.walk();
        let state = &before[index];
        let piece = &mut self.pieces[index];
        piece.codes.retain(|item| {
            !matches!(
                (&item.kind, layer),
                (TokenKind::Code(Code::Fg(_)), Layer::Fg)
                    | (TokenKind::Code(Code::Bg(_)), Layer::Bg)
                    | (TokenKind::Code(Code::UnderlineColor(_)), Layer::Underline)
            )
        });
        let now = state.after(&piece.codes);
        let want = color(&spec);
        let current = match layer {
            Layer::Fg => now.fg,
            Layer::Bg => now.bg,
            Layer::Underline => now.underline_color,
        };
        if current != want {
            piece.codes.push(match layer {
                Layer::Fg => fg(want.as_ref()),
                Layer::Bg => bg(want.as_ref()),
                Layer::Underline => underline_color(want.as_ref()),
            });
        }
        Ok(uid)
    }

    fn set_style(&mut self, index: usize, style: Style, on: bool) -> Result<usize, EditError> {
        let piece = self.piece(index)?;
        if !piece.shows() {
            return error("Only a part that shows can take a style.");
        }
        let uid = piece.uid;
        let (before, at) = self.walk();
        let state = before[index].clone();
        let mut target = at[index].clone();
        let underline = matches!(style, Style::Underline(_));
        // Off is off for an underline of any kind.
        let has = if underline && !on {
            target.underline.is_some()
        } else {
            target.style(style)
        };
        if has == on {
            return Ok(uid);
        }
        target.set_style(style, on);
        let piece = &mut self.pieces[index];
        if on && !underline {
            piece.codes.push(code(Code::Style(style)));
            return Ok(uid);
        }
        // The piece's own underline codes go, so one kind never stacks on
        // another, and what the piece inherits is put right below.
        piece.codes.retain(|item| match &item.kind {
            TokenKind::Code(Code::Style(Style::Underline(_))) => !underline,
            kind => *kind != TokenKind::Code(Code::Style(style)),
        });
        let now = state.after(&piece.codes);
        if now != target {
            piece.codes.extend(transition(&now, &target));
        }
        Ok(uid)
    }

    /// Take out piece `index`, with the other half of a condition.
    fn remove(&mut self, index: usize) -> Result<(), EditError> {
        let piece = self.piece(index)?;
        match piece.kind {
            PieceKind::If | PieceKind::IfNot => {
                let close = self.section_at(index).map(|(_, close)| close);
                if let Some(close) = close.filter(|c| *c < self.pieces.len()) {
                    self.pieces.remove(close);
                }
                self.pieces.remove(index);
            }
            PieceKind::End => {
                let open = self
                    .sections()
                    .into_iter()
                    .find(|(_, close)| *close == index)
                    .map(|(open, _)| open);
                self.pieces.remove(index);
                if let Some(open) = open {
                    self.pieces.remove(open);
                }
            }
            _ => {
                self.pieces.remove(index);
            }
        }
        Ok(())
    }

    /// Every condition and its end, as piece indexes. A condition with no
    /// end runs to the end of the design, as the renderer draws it, and
    /// its end is the number of pieces.
    fn sections(&self) -> Vec<(usize, usize)> {
        let mut open = Vec::new();
        let mut out = Vec::new();
        for (index, piece) in self.pieces.iter().enumerate() {
            match piece.kind {
                PieceKind::If | PieceKind::IfNot => open.push(index),
                PieceKind::End => {
                    if let Some(start) = open.pop() {
                        out.push((start, index));
                    }
                }
                _ => {}
            }
        }
        for start in open {
            out.push((start, self.pieces.len()));
        }
        out.sort_unstable();
        out
    }

    /// The section a condition at `open` starts.
    fn section_at(&self, open: usize) -> Option<(usize, usize)> {
        self.sections().into_iter().find(|(o, _)| *o == open)
    }

    /// The sections around piece `index`, innermost first.
    pub(crate) fn around(&self, index: usize) -> Vec<(usize, usize)> {
        let mut found: Vec<(usize, usize)> = self
            .sections()
            .into_iter()
            .filter(|(open, close)| *open < index && index < *close)
            .collect();
        found.sort_unstable_by_key(|s| std::cmp::Reverse(s.0));
        found
    }

    fn set_when(&mut self, index: usize, when: When) -> Result<(), EditError> {
        // A line break shows nothing, but it starts a row only when it
        // draws, so it takes When as a part that shows does (P10). So does
        // a push to the right edge.
        let piece = self.piece(index)?;
        if !piece.shows() && !matches!(piece.kind, PieceKind::Nl | PieceKind::Right) {
            return error("Only a part that shows can change when it shows.");
        }
        let want = match when {
            When::Always => None,
            When::Fight => Some(PieceKind::If),
            When::NotFight => Some(PieceKind::IfNot),
        };
        let around = self.around(index);
        let fight_sections: Vec<(usize, usize)> = around
            .iter()
            .copied()
            .filter(|(open, _)| self.pieces[*open].fight().is_some())
            .collect();
        let inner_fight = around
            .first()
            .filter(|(open, _)| self.pieces[*open].fight().is_some())
            .copied();
        // A fight condition that is not the innermost one would split the
        // conditions inside it.
        if fight_sections.len() > usize::from(inner_fight.is_some()) {
            return error("Vosh can change when this part shows only outside other conditions.");
        }
        let mut touched = Vec::new();
        let mut index = index;
        if let Some((open, close)) = inner_fight {
            if self.pieces[open].fight() == want {
                return Ok(());
            }
            index = self.take_out(open, close, index, &mut touched);
        }
        if let Some(kind) = want {
            self.wrap(index, kind, &mut touched);
        }
        self.neutralize(&touched);
        Ok(())
    }

    /// Take piece `index` out of the section `open`..`close` it sits in.
    /// Returns where the piece is after.
    fn take_out(
        &mut self,
        open: usize,
        close: usize,
        index: usize,
        touched: &mut Vec<usize>,
    ) -> usize {
        let opener = self.pieces[open].uid;
        let closer = self.pieces.get(close).map(|p| p.uid);
        let end = close.min(self.pieces.len());
        let alone = (open + 1..end).all(|i| i == index || self.pieces[i].kind == PieceKind::Codes);
        if alone {
            // Nothing else is in the condition but codes that kept it
            // ending as it started, so it goes, and they go with it.
            let mut at = index;
            for i in (open..=end.min(self.pieces.len() - 1)).rev() {
                if i != index && (i == open || i == close || i < end) {
                    self.pieces.remove(i);
                    if i < at {
                        at -= 1;
                    }
                }
            }
            return at;
        }
        let first = index == open + 1;
        let last = index + 1 == close;
        match (first, last) {
            (true, true) => {
                if close < self.pieces.len() {
                    self.pieces.remove(close);
                }
                self.pieces.remove(open);
                index - 1
            }
            (true, false) => {
                let marker = self.pieces.remove(open);
                self.pieces.insert(open + 1, marker);
                touched.push(opener);
                open
            }
            (false, true) => {
                if close < self.pieces.len() {
                    let marker = self.pieces.remove(close);
                    self.pieces.insert(index, marker);
                } else {
                    let end = self.marker_piece(TokenKind::End);
                    touched.push(end.uid);
                    self.pieces.insert(index, end);
                }
                touched.push(opener);
                touched.extend(closer);
                index + 1
            }
            (false, false) => {
                let end = self.marker_piece(TokenKind::End);
                let again = self.pieces[open].content[0].kind.clone();
                let again = self.marker_piece(again);
                touched.extend([opener, end.uid, again.uid]);
                self.pieces.insert(index + 1, again);
                self.pieces.insert(index, end);
                index + 1
            }
        }
    }

    /// Put piece `index` in a fight condition of `kind`, sharing one with
    /// the pieces next to it when they have one.
    fn wrap(&mut self, index: usize, kind: PieceKind, touched: &mut Vec<usize>) {
        let sections = self.sections();
        let prev = index
            .checked_sub(1)
            .and_then(|p| sections.iter().find(|(_, close)| *close == p))
            .filter(|(open, _)| self.pieces[*open].fight() == Some(kind))
            .copied();
        let next = sections
            .iter()
            .find(|(open, _)| *open == index + 1)
            .filter(|(open, _)| self.pieces[*open].fight() == Some(kind))
            .copied();
        match (prev, next) {
            (Some((open, close)), Some(_)) => {
                touched.push(self.pieces[open].uid);
                // The end before the piece and the condition after it go,
                // so both sections become one.
                self.pieces.remove(index + 1);
                self.pieces.remove(close);
            }
            (Some((open, close)), None) => {
                touched.push(self.pieces[open].uid);
                let end = self.pieces.remove(close);
                self.pieces.insert(index, end);
            }
            (None, Some((open, _))) => {
                touched.push(self.pieces[open].uid);
                let start = self.pieces.remove(open);
                self.pieces.insert(index, start);
            }
            (None, None) => {
                let field = FieldRef::new("fight");
                let start = self.marker_piece(if kind == PieceKind::If {
                    TokenKind::If(field)
                } else {
                    TokenKind::IfNot(field)
                });
                let end = self.marker_piece(TokenKind::End);
                touched.push(start.uid);
                self.pieces.insert(index + 1, end);
                self.pieces.insert(index, start);
            }
        }
    }

    /// Make each touched section end with the look it started with, so a
    /// piece after it has the same look whether it drew or not.
    fn neutralize(&mut self, touched: &[usize]) {
        let mut sections: Vec<(usize, usize)> = self
            .sections()
            .into_iter()
            .filter(|(open, close)| {
                touched.contains(&self.pieces[*open].uid)
                    || self
                        .pieces
                        .get(*close)
                        .is_some_and(|p| touched.contains(&p.uid))
            })
            .collect();
        // From the last, so an insert never moves a section still to come.
        sections.sort_unstable_by_key(|s| std::cmp::Reverse(s.1));
        for (open, close) in sections {
            let (before, _) = self.walk();
            let start = before[open].clone();
            let end = if close < self.pieces.len() {
                before[close].clone()
            } else {
                let last = self.pieces.len() - 1;
                before[last].after(&self.pieces[last].codes)
            };
            let fix = transition(&end, &start);
            if !fix.is_empty() {
                let piece = self.new_piece(PieceKind::Codes, fix, Vec::new());
                self.pieces.insert(close, piece);
            }
        }
    }

    /// Give every piece from before the edit the look it had, but the one
    /// the edit changed on purpose, by writing codes right before it.
    /// A line break keeps the look it had too, so a color never runs on
    /// into the next row where it did not before. So does a condition and
    /// its end: codes that put a look back after a piece inside a
    /// condition go before its end, inside it, and after a piece before a
    /// condition they go before the condition, so they apply exactly when
    /// the piece's own codes do and a piece after the condition looks as
    /// it did whether the condition holds or not.
    fn repair(&mut self, before: &Looks, edited: Option<usize>) {
        let mut state = Look::default();
        let pieces = std::mem::take(&mut self.pieces);
        let mut out = Vec::with_capacity(pieces.len());
        for mut piece in pieces {
            match piece.origin {
                Some(origin) if piece.shows() && Some(piece.uid) != edited => {
                    let want_in = &before.before[origin];
                    let want_at = &before.at[origin];
                    if state.after(&piece.codes) != *want_at {
                        let fix = restore(&state, want_in, want_at, &piece.codes);
                        piece.codes.splice(0..0, fix);
                    }
                }
                Some(origin) if piece.marker() && state != before.before[origin] => {
                    let fix = transition(&state, &before.before[origin]);
                    state = state.after(&fix);
                    out.push(self.new_piece(PieceKind::Codes, fix, Vec::new()));
                }
                _ => {}
            }
            state = state.after(&piece.codes);
            out.push(piece);
        }
        self.pieces = out;
    }

    /// The template text, each token as it was written unless it now
    /// needs braces or a doubled `%` to read the same, with where each
    /// piece's content starts in it, or its codes for a piece with no
    /// content.
    fn write(&self) -> Result<(String, Vec<usize>), EditError> {
        let mut first_content = Vec::with_capacity(self.pieces.len());
        let mut count = 0;
        for piece in &self.pieces {
            first_content.push(if piece.content.is_empty() {
                count
            } else {
                count + piece.codes.len()
            });
            count += piece.codes.len() + piece.content.len();
        }
        let items: Vec<&Item> = self
            .pieces
            .iter()
            .flat_map(|p| p.codes.iter().chain(p.content.iter()))
            .collect();
        let texts: Vec<String> = items
            .iter()
            .map(|item| {
                item.text
                    .clone()
                    .unwrap_or_else(|| write_token(&item.kind, false))
            })
            .collect();
        let mut out = String::new();
        let mut offsets = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            offsets.push(out.len());
            let next = texts.get(index + 1).and_then(|t| t.chars().next());
            let mut text = texts[index].clone();
            if runs_on(&item.kind, &text, next) {
                text = write_token(&item.kind, true);
            }
            if let TokenKind::Text(_) = item.kind {
                // A lone `%` at the end of text would start a code with
                // what follows.
                if item.text.is_some()
                    && text.ends_with('%')
                    && next.is_some_and(|c| {
                        c == '%' || c == '{' || c.is_ascii_alphanumeric() || c == '_'
                    })
                {
                    text.push('%');
                }
            }
            out.push_str(&text);
        }
        let wanted = shown(items.iter().map(|item| &item.kind));
        let got = shown(Template::parse(&out).tokens().iter().map(|t| &t.kind));
        if wanted != got {
            return error("Vosh could not write that change to your design.");
        }
        let starts = first_content
            .into_iter()
            .map(|item| offsets.get(item).copied().unwrap_or(out.len()))
            .collect();
        Ok((out, starts))
    }
}

/// Tokens with every run of text and `%%` as one text, which is how they
/// draw.
fn shown<'a>(kinds: impl Iterator<Item = &'a TokenKind>) -> Vec<TokenKind> {
    let mut out: Vec<TokenKind> = Vec::new();
    for kind in kinds {
        let text = match kind {
            TokenKind::Text(text) => Some(text.as_str()),
            TokenKind::Percent => Some("%"),
            _ => None,
        };
        match (text, out.last_mut()) {
            (Some(text), Some(TokenKind::Text(last))) => last.push_str(text),
            (Some(text), _) => out.push(TokenKind::Text(text.to_string())),
            (None, _) => out.push(kind.clone()),
        }
    }
    out
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

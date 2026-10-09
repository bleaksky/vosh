//! The design as pieces while an edit runs. [`Doc`] runs each op on
//! them, gives every other piece the look it had, and writes them back
//! as template text.

use super::{
    color_spec, content_for, error, gauge_of, kind_of, parse_field, style_of, ColorChoice,
    EditError, EditOp, FormatChoice, FormatName, When, BAR_STEPS,
};
use crate::design::{
    bg, code, color, fg, restore, runs_on, transition, underline_color, write_token, BarColor,
    Code, ColorSpec, FieldRef, Format, Item, Layer, Look, Own, PieceKind, Scale, Style, Template,
    TokenKind, ValueRef,
};
use crate::values::Kind;

/// A piece of the design being edited.
#[derive(Debug, Clone)]
pub(crate) struct Piece {
    /// Which piece this is, however the pieces move.
    pub(super) uid: usize,
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

    /// The piece is a change of a vital, such as `%hp_change`.
    fn change(&self) -> bool {
        self.kind == PieceKind::Value
            && self.value().is_some_and(|value| {
                matches!(
                    value.format,
                    Format::Value | Format::Zero | Format::PlusMinus
                ) && kind_of(&value.field) == Some(Kind::Change)
            })
    }

    /// The piece is a change of a vital that draws in its sign color,
    /// green for a gain and red for a loss, since its own codes set no
    /// text color. A color or dim it takes from the pieces before it does
    /// not apply then, and a restore written onto it would read as its
    /// own.
    fn sign_colored(&self) -> bool {
        self.change() && !Own::of(&self.codes).fg
    }

    /// The look the piece draws in, from `at`, its look at its first
    /// cell. A change in its sign color has no text color and is dim
    /// only when its own codes dim it, as the renderer draws it.
    pub(crate) fn shown(&self, at: &Look) -> Look {
        let mut look = at.clone();
        if self.sign_colored() {
            look.fg = None;
            look.dim = Own::of(&self.codes).dim;
        }
        look
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
pub(super) struct Looks {
    /// The look before each piece's codes.
    before: Vec<Look>,
    /// The look at each piece's first cell.
    at: Vec<Look>,
}

pub(crate) struct Doc {
    pub(crate) pieces: Vec<Piece>,
    pub(super) next_uid: usize,
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

    pub(super) fn looks(&self) -> Looks {
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
    pub(super) fn run(
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
            // With no text color of its own left, a change draws its
            // sign color, whatever color comes before it.
            Layer::Fg if piece.change() => None,
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
        let shown = piece.shown(&target);
        let underline = matches!(style, Style::Underline(_));
        // Off is off for an underline of any kind.
        let has = if underline && !on {
            shown.underline.is_some()
        } else {
            shown.style(style)
        };
        if has == on {
            return Ok(uid);
        }
        // A change in its sign color is dim only by its own codes.
        if style == Style::Dim && piece.sign_colored() {
            let piece = &mut self.pieces[index];
            if on {
                piece.codes.push(code(Code::Style(style)));
            } else {
                piece
                    .codes
                    .retain(|item| item.kind != TokenKind::Code(Code::Style(style)));
            }
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
        // draws, so it takes When as a part that shows does. So does
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
    /// A change of a vital in its sign color would read those codes as a
    /// color or dim of its own, so it keeps the look before it, and the
    /// codes go on the piece after it, or at the end of the design when
    /// none follows.
    /// A line break keeps the look it had too, so a color never runs on
    /// into the next row where it did not before. So does a condition and
    /// its end: codes that put a look back after a piece inside a
    /// condition go before its end, inside it, and after a piece before a
    /// condition they go before the condition, so they apply exactly when
    /// the piece's own codes do and a piece after the condition looks as
    /// it did whether the condition holds or not.
    pub(super) fn repair(&mut self, before: &Looks, edited: Option<usize>) {
        let mut state = Look::default();
        // The look a change in its sign color left owed, for the end of
        // the design when no piece follows it.
        let mut owed: Option<Look> = None;
        let pieces = std::mem::take(&mut self.pieces);
        let mut out = Vec::with_capacity(pieces.len());
        for mut piece in pieces {
            match piece.origin {
                Some(origin) if piece.shows() && Some(piece.uid) != edited => {
                    let want_in = &before.before[origin];
                    let want_at = &before.at[origin];
                    if state.after(&piece.codes) != *want_at {
                        if piece.sign_colored() {
                            state = state.after(&piece.codes);
                            owed = Some(want_at.clone());
                            out.push(piece);
                            continue;
                        }
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
            owed = match owed {
                Some(look) if !piece.shows() && !piece.marker() => Some(look.after(&piece.codes)),
                _ => None,
            };
            state = state.after(&piece.codes);
            out.push(piece);
        }
        if let Some(look) = owed.filter(|look| *look != state) {
            let fix = transition(&state, &look);
            out.push(self.new_piece(PieceKind::Codes, fix, Vec::new()));
        }
        self.pieces = out;
    }

    /// The template text, each token as it was written unless it now
    /// needs braces or a doubled `%` to read the same, with where each
    /// piece's content starts in it, or its codes for a piece with no
    /// content.
    pub(super) fn write(&self) -> Result<(String, Vec<usize>), EditError> {
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

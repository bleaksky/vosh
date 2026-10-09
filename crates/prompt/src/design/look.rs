//! The look a design's codes leave at each cell, and the fewest codes
//! that turn one look into another. The editor uses them to keep the
//! look of every piece an edit does not touch.

use super::tokens::{Code, ColorSpec, Style, TokenKind, UnderlineStyle};

/// The SGR state the codes leave, with each color as the template names
/// it. A color by value stands for itself, whatever it draws.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Look {
    pub(crate) fg: Option<ColorSpec>,
    pub(crate) bg: Option<ColorSpec>,
    /// The underline's color, None for the text's own.
    pub(crate) underline_color: Option<ColorSpec>,
    pub(crate) bold: bool,
    pub(crate) dim: bool,
    pub(crate) italic: bool,
    /// The kind of underline, None with no underline.
    pub(crate) underline: Option<UnderlineStyle>,
    pub(crate) inverse: bool,
    pub(crate) strike: bool,
    pub(crate) blink: bool,
}

/// The styles that are on or off, in the order the writer writes them,
/// which is the order of their SGR numbers. The underline sits between
/// italic and blink, as one slot that holds one kind at a time.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    Bold,
    Dim,
    Italic,
    Underline,
    Blink,
    Inverse,
    Strike,
}

const SLOTS: [Slot; 7] = [
    Slot::Bold,
    Slot::Dim,
    Slot::Italic,
    Slot::Underline,
    Slot::Blink,
    Slot::Inverse,
    Slot::Strike,
];

impl Look {
    pub(crate) fn style(&self, style: Style) -> bool {
        match style {
            Style::Bold => self.bold,
            Style::Dim => self.dim,
            Style::Italic => self.italic,
            Style::Underline(line) => self.underline == Some(line),
            Style::Inverse => self.inverse,
            Style::Strike => self.strike,
            Style::Blink => self.blink,
            Style::Off => false,
        }
    }

    /// Turn a style on or off. An underline kind turned on replaces the
    /// kind there was, and turned off leaves no underline of any kind.
    pub(crate) fn set_style(&mut self, style: Style, on: bool) {
        match style {
            Style::Bold => self.bold = on,
            Style::Dim => self.dim = on,
            Style::Italic => self.italic = on,
            Style::Underline(line) => self.underline = on.then_some(line),
            Style::Inverse => self.inverse = on,
            Style::Strike => self.strike = on,
            Style::Blink => self.blink = on,
            Style::Off => {
                for slot in SLOTS {
                    self.set_slot(slot, &Look::default());
                }
            }
        }
    }

    /// The style a slot holds, None while it is off.
    fn slot(&self, slot: Slot) -> Option<Style> {
        let on = |on: bool, style: Style| on.then_some(style);
        match slot {
            Slot::Bold => on(self.bold, Style::Bold),
            Slot::Dim => on(self.dim, Style::Dim),
            Slot::Italic => on(self.italic, Style::Italic),
            Slot::Underline => self.underline.map(Style::Underline),
            Slot::Inverse => on(self.inverse, Style::Inverse),
            Slot::Strike => on(self.strike, Style::Strike),
            Slot::Blink => on(self.blink, Style::Blink),
        }
    }

    /// Give a slot what it holds in `from`.
    fn set_slot(&mut self, slot: Slot, from: &Look) {
        match slot {
            Slot::Bold => self.bold = from.bold,
            Slot::Dim => self.dim = from.dim,
            Slot::Italic => self.italic = from.italic,
            Slot::Underline => self.underline = from.underline,
            Slot::Inverse => self.inverse = from.inverse,
            Slot::Strike => self.strike = from.strike,
            Slot::Blink => self.blink = from.blink,
        }
    }

    /// The styles that are on, in the order the writer writes them.
    fn styles(&self) -> impl Iterator<Item = Style> + '_ {
        SLOTS.into_iter().filter_map(|slot| self.slot(slot))
    }

    fn code(&mut self, code: &Code) {
        match code {
            Code::Reset => *self = Look::default(),
            Code::Fg(spec) => self.fg = color(spec),
            Code::Bg(spec) => self.bg = color(spec),
            Code::UnderlineColor(spec) => self.underline_color = color(spec),
            Code::Style(style) => self.set_style(*style, *style != Style::Off),
        }
    }

    /// The look after `items`, codes among them, starting from this one.
    pub(crate) fn after(&self, items: &[Item]) -> Look {
        let mut look = self.clone();
        for item in items {
            if let TokenKind::Code(code) = &item.kind {
                look.code(code);
            }
        }
        look
    }
}

/// What a piece sets with its own codes, the ones right before its
/// content, as the renderer reads them for a change of a vital. A color
/// or dim the pieces before it leave does not count. The default color
/// and a reset are no color of its own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Own {
    /// A text color other than the default.
    pub(crate) fg: bool,
    pub(crate) dim: bool,
}

impl Own {
    /// What the piece sets after one more of its codes.
    pub(crate) fn take(&mut self, code: &Code) {
        match code {
            Code::Reset => *self = Own::default(),
            Code::Style(Style::Dim) => self.dim = true,
            Code::Style(Style::Off) => self.dim = false,
            Code::Fg(spec) => self.fg = *spec != ColorSpec::Default,
            _ => {}
        }
    }

    /// What `items`, a piece's codes, set.
    pub(crate) fn of(items: &[Item]) -> Own {
        let mut own = Own::default();
        for item in items {
            if let TokenKind::Code(code) = &item.kind {
                own.take(code);
            }
        }
        own
    }
}

/// A color as the look keeps it, None for the terminal's own.
pub(crate) fn color(spec: &ColorSpec) -> Option<ColorSpec> {
    (*spec != ColorSpec::Default).then(|| spec.clone())
}

pub(crate) fn code(code: Code) -> Item {
    Item::new(TokenKind::Code(code))
}

pub(crate) fn fg(color: Option<&ColorSpec>) -> Item {
    code(Code::Fg(color.cloned().unwrap_or(ColorSpec::Default)))
}

pub(crate) fn bg(color: Option<&ColorSpec>) -> Item {
    code(Code::Bg(color.cloned().unwrap_or(ColorSpec::Default)))
}

pub(crate) fn underline_color(color: Option<&ColorSpec>) -> Item {
    code(Code::UnderlineColor(
        color.cloned().unwrap_or(ColorSpec::Default),
    ))
}

/// The fewest codes that turn `from` into `to`: the colors and styles
/// that differ, `%s_off` first when a style must go, or `%c_reset` and
/// what `to` holds when that is shorter. One underline kind turns into
/// another with its own code alone, and `%s_off` keeps the underline's
/// color.
pub(crate) fn transition(from: &Look, to: &Look) -> Vec<Item> {
    if from == to {
        return Vec::new();
    }
    let mut delta = Vec::new();
    if from.fg != to.fg {
        delta.push(fg(to.fg.as_ref()));
    }
    if from.bg != to.bg {
        delta.push(bg(to.bg.as_ref()));
    }
    if from.underline_color != to.underline_color {
        delta.push(underline_color(to.underline_color.as_ref()));
    }
    let off = SLOTS
        .iter()
        .any(|slot| from.slot(*slot).is_some() && to.slot(*slot).is_none());
    if off {
        delta.push(code(Code::Style(Style::Off)));
    }
    for style in to.styles() {
        if off || !from.style(style) {
            delta.push(code(Code::Style(style)));
        }
    }
    let mut reset = vec![code(Code::Reset)];
    if to.fg.is_some() {
        reset.push(fg(to.fg.as_ref()));
    }
    if to.bg.is_some() {
        reset.push(bg(to.bg.as_ref()));
    }
    if to.underline_color.is_some() {
        reset.push(underline_color(to.underline_color.as_ref()));
    }
    for style in to.styles() {
        reset.push(code(Code::Style(style)));
    }
    if reset.len() < delta.len() {
        reset
    } else {
        delta
    }
}

/// The codes to put before a piece whose own codes `codes` should give
/// `want_at`, now that `state` comes before it where `want_in` used to.
/// Only what the piece's codes leave alone needs to come back.
pub(crate) fn restore(state: &Look, want_in: &Look, want_at: &Look, codes: &[Item]) -> Vec<Item> {
    let from_state = state.after(codes);
    let from_want = want_in.after(codes);
    let mut target = want_in.clone();
    if from_state.fg == from_want.fg {
        target.fg.clone_from(&state.fg);
    }
    if from_state.bg == from_want.bg {
        target.bg.clone_from(&state.bg);
    }
    if from_state.underline_color == from_want.underline_color {
        target.underline_color.clone_from(&state.underline_color);
    }
    for slot in SLOTS {
        if from_state.slot(slot) == from_want.slot(slot) {
            target.set_slot(slot, state);
        }
    }
    let fix = transition(state, &target);
    let mut check = fix.clone();
    check.extend_from_slice(codes);
    if state.after(&check) == *want_at {
        fix
    } else {
        transition(state, want_in)
    }
}

/// A token, with the text it was written as when it came from the
/// template, so the writer keeps what it does not touch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    pub(crate) kind: TokenKind,
    pub(crate) text: Option<String>,
}

impl Item {
    pub(crate) fn new(kind: TokenKind) -> Self {
        Self { kind, text: None }
    }

    pub(crate) fn text(text: &str) -> Self {
        Self::new(TokenKind::Text(text.to_string()))
    }
}

//! What Vosh knows about Aabahran alone.
//!
//! The PROMPT line compiler (section 3) lives here. It is pure. The
//! session feeds it your settings and matches lines with what it returns.
//!
//! - [`lex`] stores a setting you typed as `do_prompt` does, and reads
//!   it in the two passes the game prints it in.
//! - [`codes`] holds every value code, the field it fills and the
//!   pattern Vosh reads it with.
//! - [`colors`] holds the backtick colors the game sends and rebuilds
//!   your codes from them, for your PROMPT setting and for a text the
//!   writing card reads back.
//! - [`shapes`] compiles your settings into the shapes Vosh recognizes
//!   your prompt by, with the settle flag of each.
//! - [`observer`] reads the lines the game answers `prompt` and
//!   `fprompt` with, and your own `prompt off`.
//! - [`damage`] holds the damage ladder of `dam_message` and tells the
//!   attack lines it prints, which Collapse repeated lines can leave whole.
//!
//! Every warning carries the span of the setting it is about, as the
//! game stores it, and a sentence the card and `#prompt` show as they
//! are.

pub mod codes;
pub mod colors;
pub mod damage;
pub mod lex;
pub mod observer;
pub mod shapes;
mod who;

pub use shapes::{compile, Compiled, Origin, Shape, ShapeKind};
pub use who::Who;

pub(crate) use shapes::ShapeLine;

use std::fmt;
use std::ops::Range;

use serde::Serialize;

/// Which of your two settings a warning or a shape comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub enum Which {
    /// `prompt`, the setting the game draws out of a fight.
    #[serde(rename = "prompt")]
    Prompt,
    /// `fprompt`, the setting it draws in a fight when one is set.
    #[serde(rename = "fprompt")]
    Fight,
}

/// Something about your setting that keeps Vosh from reading all of it,
/// or that the game changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningKind {
    /// Two codes with nothing between them that tells where one ends.
    RunTogether,
    /// A shape too short to tell from other lines.
    Short,
    /// `%u` for a mortal.
    PacifyMortal,
    /// `%s` while you control a mobile.
    LangMobile,
    /// The game kept only the characters that fit on the line you typed.
    Cut,
    /// A `%` at the end eats the space the game adds.
    LonePercent,
    /// A code shows twice, and Vosh reads the first.
    Twice,
}

/// A warning with the span of the setting it is about and its sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Warning {
    pub kind: WarningKind,
    pub which: Which,
    /// Byte range in the setting as the game stores it. The cut is an
    /// empty range where the game cut.
    pub span: Range<usize>,
    /// The sentence to show, final copy.
    pub text: String,
}

impl Warning {
    pub(crate) fn new(kind: WarningKind, which: Which, span: Range<usize>, text: String) -> Self {
        Self {
            kind,
            which,
            span,
            text,
        }
    }
}

/// A setting Vosh cannot read at all. A color code that ends right
/// before a code takes the code's first character as its own, so the
/// game prints something no pattern can follow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompileError {
    pub which: Which,
    /// The code the color runs into, as written, such as `%h`.
    pub code: String,
    /// Byte range from the backtick to the end of the code.
    pub span: Range<usize>,
    /// The sentence to show, final copy.
    pub text: String,
}

impl CompileError {
    pub(crate) fn runs_into(which: Which, code: &str, span: Range<usize>) -> Self {
        Self {
            which,
            code: code.to_string(),
            span,
            text: format!("A color code runs into {code}. Put a space between them in the game."),
        }
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl std::error::Error for CompileError {}

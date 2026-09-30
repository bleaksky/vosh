//! What Vosh knows about Aabahran alone.
//!
//! The lamented tears rule (H7 in section 1.2 of the build spec) lives
//! here. The older server build sends true values under the song, and
//! only Char.Affects naming it tells Vosh the game means to hide them.
//!
//! The PROMPT line compiler (section 3) grows here too.
//!
//! - [`lex`] stores a setting you typed as `do_prompt` does, and reads
//!   it in the two passes the game prints it in.
//! - [`codes`] holds every value code, the field it fills and the
//!   pattern Vosh reads it with.
//! - [`colors`] holds the backtick colors the game sends and rebuilds
//!   your codes from them.
//!
//! Every warning carries the span of the setting it is about, as the
//! game stores it, and a sentence the card and `#prompt` show as they
//! are.

pub mod codes;
pub mod colors;
pub mod lex;

use std::fmt;
use std::ops::Range;

use serde::Serialize;

use crate::gmcp::Affects;

/// Who the prompt is for, which decides what `%u` and `%s` print.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Who {
    /// An immortal, for whom `%u` prints `pacified` or `not pacified`.
    pub immortal: bool,
    /// You control a mobile, which leaves `%s` repeating the text of the
    /// code before it.
    pub mobile: bool,
}

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
    /// The game kept the first 255 characters.
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

/// The song that hides your vitals, affects, group and your opponent's
/// condition while it is on you.
pub const LAMENT: &str = "lamented tears";

/// True when Char.Affects names lamented tears, in any case.
pub fn names_lament(affects: &Affects) -> bool {
    affects
        .list
        .iter()
        .any(|a| a.name.trim().eq_ignore_ascii_case(LAMENT))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gmcp::Affect;

    fn affects(names: &[&str]) -> Affects {
        Affects {
            list: names
                .iter()
                .map(|n| Affect {
                    name: (*n).to_string(),
                    ..Affect::default()
                })
                .collect(),
            hidden: false,
        }
    }

    #[test]
    fn the_song_is_found_by_name_in_any_case() {
        assert!(names_lament(&affects(&["bless", "lamented tears"])));
        assert!(names_lament(&affects(&["Lamented Tears"])));
        assert!(!names_lament(&affects(&["bless", "tears"])));
        assert!(!names_lament(&affects(&[])));
    }
}

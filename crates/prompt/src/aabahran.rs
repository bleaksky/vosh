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
//!   your codes from them.
//! - [`shapes`] compiles your settings into the shapes Vosh recognizes
//!   your prompt by, with the settle flag of each.
//! - [`observer`] reads the lines the game answers `prompt` and
//!   `fprompt` with, and your own `prompt off`.
//!
//! Every warning carries the span of the setting it is about, as the
//! game stores it, and a sentence the card and `#prompt` show as they
//! are.

pub mod codes;
pub mod colors;
pub mod lex;
pub mod observer;
pub mod shapes;
mod who;

pub use shapes::{compile, Compiled, Origin, Shape, ShapeKind, ShapeLine};
pub use who::{Who, LEVEL_IMMORTAL, TRUST_BACKTICKS};

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

/// The vitals a setting reads as a phrase, `Health, Mana, and Moves
/// with their maxes`, and the labels of every other value it reads, each
/// once. A max with no value of its own counts as another value.
fn vitals_and_others(names: &[&str]) -> (Option<String>, Vec<String>, usize) {
    let pairs = [
        ("hp", "maxhp", "Health"),
        ("mana", "maxmana", "Mana"),
        ("move", "maxmove", "Moves"),
    ];
    let mut vitals: Vec<(&str, bool)> = Vec::new();
    let mut others: Vec<String> = Vec::new();
    for name in names {
        if let Some((cur, max, label)) = pairs.iter().find(|(c, m, _)| name == c || name == m) {
            if vitals.iter().any(|(l, _)| l == label) {
                continue;
            }
            if *name == *cur {
                vitals.push((label, names.contains(max)));
                continue;
            }
            if names.contains(cur) {
                continue;
            }
        }
        let label = value_label(name);
        if !others.contains(&label) {
            others.push(label);
        }
    }
    if vitals.is_empty() {
        return (None, others, 0);
    }
    let every_max = vitals.iter().all(|(_, max)| *max);
    let items: Vec<String> = vitals
        .iter()
        .map(|(label, max)| {
            if *max && !every_max {
                format!("{label} with its max")
            } else {
                (*label).to_string()
            }
        })
        .collect();
    let tail = match (every_max, items.len()) {
        (true, 1) => " with its max",
        (true, _) => " with their maxes",
        (false, _) => "",
    };
    (
        Some(format!("{}{tail}", and_list(&items))),
        others,
        items.len(),
    )
}

/// What `#prompt game` and `#prompt fight` say Vosh reads from a
/// setting, as a sentence or two: the vitals first, each with its max
/// when the setting shows it, then any other value.
pub fn reads_sentence(names: &[&str], fight: bool) -> String {
    let what = if fight {
        "this fight prompt"
    } else {
        "this prompt"
    };
    let (vitals, others, _) = vitals_and_others(names);
    let mut out = Vec::new();
    if let Some(vitals) = vitals {
        out.push(format!("Vosh reads {vitals} from {what}."));
        if !others.is_empty() {
            out.push(format!("It also reads {}.", and_list(&others)));
        }
    } else if !others.is_empty() {
        out.push(format!("Vosh reads {} from {what}.", and_list(&others)));
    } else {
        out.push(format!(
            "Vosh knows {what} by its codes and reads no values from it."
        ));
    }
    out.join(" ")
}

/// What the card says a setting shows, the vitals first, then any other
/// value, then your tank, as one phrase: `Health, Mana, and Moves with
/// their maxes, and your tank and its health in a fight`. None when it
/// reads no value. The immortal prefix's levels are left out, since the
/// card names them in a note of their own.
fn shows_phrase(names: &[&str]) -> Option<String> {
    let tank = names.contains(&"tank");
    let tank_hp = names.iter().any(|n| *n == "tank_pct" || *n == "tank_bar");
    let rest: Vec<&str> = names
        .iter()
        .copied()
        .filter(|n| !matches!(*n, "tank" | "tank_pct" | "tank_bar" | "wizi" | "incog"))
        .collect();
    let (vitals, others, listed) = vitals_and_others(&rest);
    let mut parts: Vec<String> = vitals.into_iter().collect();
    parts.extend(others);
    match (tank, tank_hp) {
        (true, true) => parts.push("your tank and its health in a fight".into()),
        (true, false) => parts.push("your tank in a fight".into()),
        (false, true) => parts.push("your tank's health in a fight".into()),
        (false, false) => {}
    }
    match parts.as_slice() {
        [] => None,
        // A list of vitals before one more part takes a comma, so the
        // last `and` reads as the list's own.
        [first, second] if listed > 1 => Some(format!("{first}, and {second}")),
        _ => Some(and_list(&parts)),
    }
}

/// What the card says a setting shows, `It shows Health, Mana, and
/// Moves.`, or None when it reads no value.
pub fn shows_sentence(names: &[&str]) -> Option<String> {
    shows_phrase(names).map(|phrase| format!("It shows {phrase}."))
}

/// What the card says while codes run together: what Vosh still reads,
/// then which values the game supplies until you fix the prompt.
pub fn fix_sentence(names: &[&str], unread: &[String]) -> String {
    let reads = match shows_phrase(names) {
        Some(phrase) => format!("Vosh reads {phrase}."),
        None => "Vosh reads no value from this prompt.".to_string(),
    };
    if unread.is_empty() {
        return reads;
    }
    let verb = if unread.len() == 1 { "comes" } else { "come" };
    format!(
        "{reads} {} {verb} from the game until you fix the prompt.",
        and_list(unread)
    )
}

/// The label a value a setting reads goes by.
pub(crate) fn value_label(name: &str) -> String {
    match name {
        "hp_pct" => "Health percent".into(),
        "mana_pct" => "Mana percent".into(),
        "move_pct" => "Moves percent".into(),
        "tank_pct" | "tank_bar" => "Tank health".into(),
        _ => crate::values::entry(name).map_or_else(|| name.to_string(), |e| e.label.to_string()),
    }
}

/// `A`, `A and B`, or `A, B, and C`.
pub(crate) fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => format!("{first} and {second}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sentence_names_what_a_setting_reads() {
        let james = ["hp", "maxhp", "mana", "maxmana", "move", "maxmove"];
        assert_eq!(
            reads_sentence(&james, false),
            "Vosh reads Health, Mana, and Moves with their maxes from this prompt."
        );
        let mut tank = james.to_vec();
        tank.extend(["tank", "tank_bar"]);
        assert_eq!(
            reads_sentence(&tank, false),
            "Vosh reads Health, Mana, and Moves with their maxes from this prompt. It also reads Tank and Tank health."
        );
        assert_eq!(
            reads_sentence(&["hp", "maxhp", "mana", "move"], false),
            "Vosh reads Health with its max, Mana, and Moves from this prompt."
        );
        assert_eq!(
            reads_sentence(&["hp", "maxhp"], true),
            "Vosh reads Health with its max from this fight prompt."
        );
        assert_eq!(
            reads_sentence(&["hp", "tank_pct"], true),
            "Vosh reads Health from this fight prompt. It also reads Tank health."
        );
        assert_eq!(
            reads_sentence(&["maxhp", "gold"], false),
            "Vosh reads Max health and Gold from this prompt."
        );
        assert_eq!(
            reads_sentence(&[], false),
            "Vosh knows this prompt by its codes and reads no values from it."
        );
    }
}

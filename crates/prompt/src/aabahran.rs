//! What Vosh knows about Aabahran alone.
//!
//! The lamented tears rule (H7 in section 1.2 of the build spec) lives
//! here. The older server build sends true values under the song, and
//! only Char.Affects naming it tells Vosh the game means to hide them.
//!
//! The PROMPT line compiler (section 3) lives here too. It is pure. The
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

pub use shapes::{compile, Compiled, Origin, Shape, ShapeKind, ShapeLine};

use std::fmt;
use std::ops::Range;

use serde::Serialize;

use crate::gmcp::Affects;

/// Who the prompt is for, which decides what `%u` and `%s` print and
/// what the game keeps of a setting you type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Who {
    /// An immortal, for whom `%u` prints `pacified` or `not pacified`.
    pub immortal: bool,
    /// You control a mobile, which leaves `%s` repeating the text of the
    /// code before it.
    pub mobile: bool,
    /// The game keeps the backticks you type. For anyone with trust under
    /// 55 it drops each one and the character after it as it reads the
    /// line (`read_from_buffer`, `comm.c:1496-1497`).
    pub keeps_backticks: bool,
}

/// The level above which the game counts you as an immortal
/// (`LEVEL_IMMORTAL`, `merc.h`), for `%u`.
pub const LEVEL_IMMORTAL: i64 = 51;

/// The trust from which the game keeps the backticks you type. Vosh
/// reads it from the level in Char.Status, which is your trust unless an
/// immortal set another.
pub const TRUST_BACKTICKS: i64 = 55;

impl Who {
    /// Who the prompt is for, from the packets the game sent: the level
    /// in Char.Status, and Char.State, whose language is empty while you
    /// control a mobile (correction 27). Without a packet Vosh takes you
    /// for a mortal in your own body.
    pub fn from_packets(level: Option<i64>, language: Option<&str>) -> Self {
        Self {
            immortal: level.is_some_and(|l| l > LEVEL_IMMORTAL),
            mobile: language.is_some_and(str::is_empty),
            keeps_backticks: level.is_some_and(|l| l >= TRUST_BACKTICKS),
        }
    }
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

/// What `#prompt game` and `#prompt fight` say Vosh reads from a
/// setting, as a sentence or two: the vitals first, each with its max
/// when the setting shows it, then any other value.
pub fn reads_sentence(names: &[&str], fight: bool) -> String {
    let what = if fight {
        "this fight prompt"
    } else {
        "this prompt"
    };
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
    let mut out = Vec::new();
    if !vitals.is_empty() {
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
        out.push(format!(
            "Vosh reads {}{tail} from {what}.",
            and_list(&items)
        ));
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

/// The label a value a setting reads goes by.
fn value_label(name: &str) -> String {
    match name {
        "hp_pct" => "Health percent".into(),
        "mana_pct" => "Mana percent".into(),
        "move_pct" => "Moves percent".into(),
        "tank_pct" | "tank_bar" => "Tank health".into(),
        _ => crate::vars::entry(name).map_or_else(|| name.to_string(), |e| e.label.to_string()),
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

    #[test]
    fn who_follows_the_level_and_the_language() {
        assert_eq!(Who::from_packets(None, None), Who::default());
        assert!(!Who::from_packets(Some(51), None).immortal);
        assert!(Who::from_packets(Some(52), None).immortal);
        assert!(!Who::from_packets(None, Some("common")).mobile);
        assert!(Who::from_packets(Some(60), Some("")).mobile);
        // The game keeps the backticks you type from trust 55.
        assert!(!Who::from_packets(None, None).keeps_backticks);
        assert!(!Who::from_packets(Some(54), None).keeps_backticks);
        assert!(Who::from_packets(Some(55), None).keeps_backticks);
    }

    #[test]
    fn the_song_is_found_by_name_in_any_case() {
        assert!(names_lament(&affects(&["bless", "lamented tears"])));
        assert!(names_lament(&affects(&["Lamented Tears"])));
        assert!(!names_lament(&affects(&["bless", "tears"])));
        assert!(!names_lament(&affects(&[])));
    }
}

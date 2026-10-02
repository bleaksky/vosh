//! The sentences the card and `#prompt` show about your settings. They
//! say what Vosh reads from a setting, what it shows, what to fix and
//! why it kept a moved pattern, with the label each code and value goes
//! by.

use crate::aabahran::codes::Code;
use crate::aabahran::Which;
use crate::engine::{Kept, PromptEngine};

// A code's label comes from the values catalog, so it lives here with the
// other labels, and the game module reads nothing from values.
impl Code {
    /// What the card and a warning call it, the catalog's label where the
    /// field has one.
    pub fn label(self) -> String {
        match self {
            Self::HpPct => "Health percent".into(),
            Self::ManaPct => "Mana percent".into(),
            Self::MovePct => "Moves percent".into(),
            Self::TankPct | Self::TankBar => "Tank health".into(),
            Self::Moon(_) => self
                .name()
                .and_then(crate::values::entry)
                .map_or("Moon", |e| e.label)
                .into(),
            _ => self
                .name()
                .and_then(crate::values::entry)
                .map_or_else(|| self.written(), |e| e.label.to_string()),
        }
    }
}

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
pub(crate) fn shows_sentence(names: &[&str]) -> Option<String> {
    shows_phrase(names).map(|phrase| format!("It shows {phrase}."))
}

/// What the card says while codes run together: what Vosh still reads,
/// then which values the game supplies until you fix the prompt.
pub(crate) fn fix_sentence(names: &[&str], unread: &[String]) -> String {
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

// The sentence on a kept pattern joins names as the other sentences do,
// so it lives here, and the engine reads nothing from the card.
impl PromptEngine {
    /// Why the migrated capture kept its pattern when the game last
    /// showed your PROMPT this session, as one sentence for `#prompt`.
    pub fn kept_pattern(&self) -> Option<String> {
        let because = match self.kept_pattern.as_ref()? {
            Kept::Compile(error) => {
                let setting = match error.which {
                    Which::Prompt => "prompt",
                    Which::Fight => "fight prompt",
                };
                format!(
                    "a color code runs into {} in the {setting} the game sent",
                    error.code
                )
            }
            Kept::Unknown(names) => match names.as_slice() {
                [name] => {
                    format!("it fills a value named {name}, and no prompt code fills that name")
                }
                _ => format!(
                    "it fills values named {}, and no prompt code fills those names",
                    and_list(names)
                ),
            },
        };
        Some(format!(
            "Vosh kept the pattern from your old capture trigger because {because}."
        ))
    }
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
pub fn and_list(items: &[String]) -> String {
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

    #[test]
    fn each_code_goes_by_its_label() {
        assert_eq!(Code::Hp.label(), "Health");
        assert_eq!(Code::Mana.label(), "Mana");
        assert_eq!(Code::MaxHp.label(), "Max health");
        assert_eq!(Code::Room.label(), "Room");
        assert_eq!(Code::Area.label(), "Area");
        assert_eq!(Code::Slot(0).label(), "Affect slot 10");
        assert_eq!(Code::Moon(2).label(), "Nercuros");
        assert_eq!(Code::Moon(7).label(), "Moon");
        assert_eq!(Code::HpPct.label(), "Health percent");
        assert_eq!(Code::TankBar.label(), "Tank health");
    }
}

//! The attack lines Aabahran prints: a hit or a miss, yours, one on you or
//! one you watch, as `dam_message` in `fight.c` writes them. Collapse
//! repeated lines leaves them whole unless you collapse attack lines too.
//!
//! `dam_message` picks a verb from its damage ladder by the damage dealt
//! and builds the line from a few formats: who or what attacks, the verb
//! in a color of its own, who is hit, and a mark, `.` up to `maul` and `!`
//! from `decimate` on. Without its colors the line reads as the attacker,
//! a space, the verb, a space, the one hit and the mark, as in `Your slash
//! hits a Blackwatch guard.` The game capitalizes its first letter, and a
//! watch room sees it after the room's name in brackets.

/// One step of the damage ladder, as `dam_message` in `fight.c` writes it
/// without the color codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    /// The verb after `You`, and after an attack noun that reads as a
    /// plural, such as `thorns` (`vs`).
    pub vs: &'static str,
    /// The verb after anyone else, and after an attack noun (`vp`).
    pub vp: &'static str,
    /// The mark that ends the line.
    pub mark: char,
}

const fn step(vs: &'static str, vp: &'static str, mark: char) -> Step {
    Step { vs, vp, mark }
}

/// The damage ladder `dam_message` climbs, from a miss to the most
/// damage. The damage presets color the same verbs from `scratch` on
/// (`DAMAGE_VERBS` in `src/automation/presets.ts`), and the verb of the
/// assassinate skill besides, and a test holds the two lists together.
pub const LADDER: [Step; 21] = [
    step("miss", "misses", '.'),
    step("scratch", "scratches", '.'),
    step("graze", "grazes", '.'),
    step("hit", "hits", '.'),
    step("injure", "injures", '.'),
    step("wound", "wounds", '.'),
    step("maul", "mauls", '.'),
    step("decimate", "decimates", '!'),
    step("devastate", "devastates", '!'),
    step("maim", "maims", '!'),
    step("MUTILATE", "MUTILATES", '!'),
    step("LACERATE", "LACERATES", '!'),
    step("DISMEMBER", "DISMEMBERS", '!'),
    step("MASSACRE", "MASSACRES", '!'),
    step("MANGLE", "MANGLES", '!'),
    step("*** DEMOLISH ***", "*** DEMOLISHES ***", '!'),
    step("*** OBLITERATE ***", "*** OBLITERATES ***", '!'),
    step("=== DISINTEGRATE ===", "=== DISINTEGRATES ===", '!'),
    step(">>> ANNIHILATE <<<", ">>> ANNIHILATES <<<", '!'),
    step("<<< ERADICATE >>>", "<<< ERADICATES >>>", '!'),
    step(
        "do UNSPEAKABLE things to",
        "does UNSPEAKABLE things to",
        '!',
    ),
];

/// The health lines that read like a hit, a ladder verb with words on
/// either side and a `.` after: the line the game prints about your
/// opponent before your prompt (`comm.c`), and the one about someone you
/// look at (`act_info.c`).
const HEALTH: [&str; 2] = [
    "has some small wounds and bruises.",
    "has some big nasty wounds and scratches.",
];

/// True when `plain`, a line the game sent without its colors, is an
/// attack line: a hit or a miss that `dam_message` printed, whoever
/// attacks and whoever is hit. The line starts with a capital, has a
/// verb of the ladder with words on either side, and ends with that
/// verb's mark. A watch room's name in brackets may come first.
pub fn attack_line(plain: &str) -> bool {
    let line = match plain
        .strip_prefix('[')
        .and_then(|rest| rest.split_once("] "))
    {
        Some((_, rest)) => rest,
        None => plain,
    };
    let Some(mark) = line.chars().next_back() else {
        return false;
    };
    if (mark != '.' && mark != '!') || !line.starts_with(|c: char| c.is_ascii_uppercase()) {
        return false;
    }
    if HEALTH.iter().any(|health| line.ends_with(health)) {
        return false;
    }
    let body = &line[..line.len() - mark.len_utf8()];
    LADDER
        .iter()
        .filter(|step| step.mark == mark)
        .any(|step| stands_in(body, step.vs) || stands_in(body, step.vp))
}

/// True when `plain` is an attack line of your own fight: you or yours
/// hit, or the one hit is you, as the formats of `dam_message` that go to
/// the attacker and to the one hit print it (`fight.c:12124` to
/// `12214`). A hit you watch between others is none, and neither is a
/// line a watch room sees, which starts with the room's name.
pub fn your_attack_line(plain: &str) -> bool {
    attack_line(plain)
        && !plain.starts_with('[')
        && (plain.starts_with("You ")
            || plain.starts_with("Your ")
            || plain.ends_with(" you.")
            || plain.ends_with(" you!"))
}

/// The words that say whose something is. Lines about wounds put one
/// right before `wounds`, as in `Some of your wounds disappear.`, and no
/// format of `dam_message` puts one before its verb.
const POSSESSIVES: [&str; 6] = ["your", "her", "his", "its", "their", "our"];

/// True when `verb` stands in `body` with a space and at least one more
/// character on each side, and the word right before it says whose
/// something is in none of the ways [`owns`] reads.
fn stands_in(body: &str, verb: &str) -> bool {
    let bytes = body.as_bytes();
    body.match_indices(verb).any(|(at, _)| {
        let end = at + verb.len();
        at >= 2
            && bytes[at - 1] == b' '
            && bytes.get(end) == Some(&b' ')
            && end + 1 < bytes.len()
            && !owns(word_before(body, at - 1))
    })
}

/// The word in `body` that ends at the space at `space`, from the space
/// or the start before it. Empty when two spaces stand together, as
/// after the empty attack noun of a skill in `Maren's  hits Orla.`
fn word_before(body: &str, space: usize) -> &str {
    let head = &body[..space];
    head.rfind(' ').map_or(head, |at| &head[at + 1..])
}

/// True when `word` says whose the next word is: one of the
/// [`POSSESSIVES`] in any case, or a word that ends in `'s`, as in `Some
/// of Maren's wounds disappear.` In `dam_message` the word before the
/// verb is an attack noun or a name, and the empty attack noun leaves
/// it empty, so neither shape comes there.
fn owns(word: &str) -> bool {
    POSSESSIVES.iter().any(|p| word.eq_ignore_ascii_case(p))
        || word.len() > 2 && word.ends_with("'s")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What fills a `%s` in a format of `dam_message`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Arg {
        /// The attack noun, `attack`.
        Noun,
        /// Where a virtual attack comes from, `origin`.
        Origin,
        /// `a `, `Your ` or nothing, before the origin.
        Lead(&'static str),
        Vs,
        /// `vp`, which is `vs` after an attack noun that reads as a plural.
        Vp,
    }

    use Arg::{Lead, Noun, Origin, Vp, Vs};

    /// The formats `dam_message` in `fight.c` builds a hit or a miss from,
    /// as the game writes them, with what fills each `%s` in order. `%c` is
    /// the mark, `$n` the attacker, `$N` the one hit, `$m` the attacker as
    /// him, her or it, and `$c` the color before the verb. The verb ends
    /// with the color reset, which the printed line leaves out with the
    /// other colors.
    const FORMATS: &[(&str, &[Arg])] = &[
        // A plain hit, `dt == TYPE_HIT`.
        ("$n's %s $c%s $m%c", &[Origin, Vp]),
        ("Your %s $c%s you%c", &[Origin, Vp]),
        ("$n $c%s $mself%c", &[Vp]),
        ("You $c%s yourself%c", &[Vs]),
        ("$n's %s $c%s $N%c", &[Origin, Vp]),
        ("Your %s $c%s $N%c", &[Origin, Vs]),
        ("$n's %s $c%s you%c", &[Origin, Vp]),
        ("$n $c%s $N%c", &[Vp]),
        ("You $c%s $N%c", &[Vs]),
        ("$n $c%s you%c", &[Vp]),
        // A hit with an attack noun.
        ("The sun's %s $c%s $n%c", &[Noun, Vp]),
        ("The sun's %s $c%s you%c", &[Noun, Vp]),
        ("%s%s's %s $c%s $m%c", &[Lead("a "), Origin, Noun, Vp]),
        ("%s%s's %s $c%s you%c", &[Lead("a "), Origin, Noun, Vp]),
        ("$n's %s $c%s $m%c", &[Noun, Vp]),
        ("Your %s $c%s you%c", &[Noun, Vp]),
        ("%s%s's %s $c%s $N%c", &[Lead("a "), Origin, Noun, Vp]),
        ("%s%s's %s $c%s $N%c", &[Lead("Your "), Origin, Noun, Vp]),
        ("%s%s's %s $c%s you%c", &[Lead("a "), Origin, Noun, Vp]),
        ("$n's %s $c%s $N%c", &[Noun, Vp]),
        ("Your %s $c%s $N%c", &[Noun, Vp]),
        ("$n's %s $c%s you%c", &[Noun, Vp]),
    ];

    /// Attack nouns from `attack_table` in `tables.c`: two plain ones, one
    /// that is a ladder verb too, one that holds `'s` and one that reads as
    /// a plural. Then the empty `noun_damage` many skills in `const.c`
    /// have, which leaves two spaces before the verb.
    const NOUNS: &[&str] = &[
        "slash",
        "pierce",
        "scratch",
        "phantom dragon's claw",
        "thorns",
        "",
    ];

    /// Where a virtual attack comes from, a noun of `attack_table`, as
    /// `get_vir_attack` in `handler.c` returns it.
    const ORIGIN: &str = "phantom dragon";

    /// Who attacks and who is hit, by the names the repo's fixtures use.
    const NAMES: &[&str] = &["a Blackwatch guard", "Tolliver", "Maren", "Orla"];

    /// What `parse_dam_message` prints for `format`, filled with `args`,
    /// without colors: the codes filled in and the first letter a capital.
    fn print(format: &str, args: &[&str], mark: char, n: &str, big_n: &str) -> String {
        let mut filled = String::new();
        let mut args = args.iter();
        let mut rest = format;
        while let Some(at) = rest.find('%') {
            filled.push_str(&rest[..at]);
            match &rest[at + 1..at + 2] {
                "s" => filled.push_str(args.next().copied().unwrap_or_default()),
                _ => filled.push(mark),
            }
            rest = &rest[at + 2..];
        }
        filled.push_str(rest);
        let line = filled
            .replace("$c", "")
            .replace("$mself", "himself")
            .replace("$m", "him")
            .replace("$N", big_n)
            .replace("$n", n);
        let mut chars = line.chars();
        let first = chars.next().map(|c| c.to_ascii_uppercase());
        first.into_iter().chain(chars).collect()
    }

    /// Every line the formats print for one step of the ladder and one
    /// attack noun, between every two names.
    fn lines(step: &Step, noun: &'static str) -> Vec<String> {
        // `dam_message` takes `vs` for `vp` after a plural attack noun.
        let plural = noun.ends_with('s') && !noun.ends_with("ss");
        let mut out = Vec::new();
        for (format, args) in FORMATS {
            let vp = if plural && args.contains(&Noun) {
                step.vs
            } else {
                step.vp
            };
            let args: Vec<&str> = args
                .iter()
                .map(|arg| match arg {
                    Noun => noun,
                    Origin => ORIGIN,
                    Lead(lead) => lead,
                    Vs => step.vs,
                    Vp => vp,
                })
                .collect();
            for n in NAMES {
                for big_n in NAMES.iter().rev() {
                    out.push(print(format, &args, step.mark, n, big_n));
                }
            }
        }
        out
    }

    #[test]
    fn every_hit_and_miss_dam_message_prints_is_an_attack_line() {
        let mut checked = 0;
        for step in &LADDER {
            for noun in NOUNS {
                for line in lines(step, noun) {
                    assert!(attack_line(&line), "{line}");
                    // A watch room sees it after the room's name.
                    let watched = format!("[The Bank of Aabahran] {line}");
                    assert!(attack_line(&watched), "{watched}");
                    checked += 1;
                }
            }
        }
        assert!(checked > 1000, "{checked}");
    }

    #[test]
    fn attack_lines_read_as_the_game_prints_them() {
        for line in [
            "Your slash hits a Blackwatch guard.",
            "A Blackwatch guard's pierce misses you.",
            "You miss a Blackwatch guard.",
            "Tolliver's slash DISMEMBERS a Blackwatch guard!",
            "A Blackwatch guard's scratch scratches Maren.",
            "Orla's thorns scratch a Blackwatch guard.",
            "Your pierce *** DEMOLISHES *** a Blackwatch guard!",
            "You do UNSPEAKABLE things to a Blackwatch guard!",
            "A Blackwatch guard does UNSPEAKABLE things to you!",
            "Maren mauls herself.",
            // A skill whose noun_damage is empty.
            "Maren's  hits Orla.",
        ] {
            assert!(attack_line(line), "{line}");
        }
    }

    #[test]
    fn other_lines_are_not_attack_lines() {
        for line in [
            "",
            "You are hungry.",
            "You dodge Quenby's attack.",
            "You parry Quenby's attack.",
            "A Blackwatch guard attacks you!",
            "A Blackwatch guard flees south.",
            // The line before your prompt in a fight, with the space the
            // game leaves before the line end, for every health it names.
            "A Blackwatch guard is in excellent condition. ",
            "A Blackwatch guard has a few scratches. ",
            "A Blackwatch guard has some small wounds and bruises. ",
            "A Blackwatch guard has quite a few wounds. ",
            "A Blackwatch guard has some big nasty wounds and scratches. ",
            "A Blackwatch guard looks pretty hurt. ",
            "A Blackwatch guard is in awful condition. ",
            // The same health as `look` prints it, with no space after.
            "Maren has some small wounds and bruises.",
            "Maren has some big nasty wounds and scratches.",
            // What dam_message prints against an immunity names no verb.
            "Luckily, you are immune to that.",
            "A Blackwatch guard is unaffected by your slash!",
            // The wrong mark for the verb, or nothing after it.
            "Your slash hits a Blackwatch guard!",
            "Your slash DISMEMBERS a Blackwatch guard.",
            "Your slash hits.",
            // No capital at the start.
            "your slash hits a Blackwatch guard.",
            "[Exits: north south]",
            // Wounds as a noun, after a word that says whose they are
            // (magic3.c:1508 and 1509, skills5.c:1570, fight.c:2428,
            // act_obj.c:704).
            "Some of your wounds disappear.",
            "You feel your wounds heal rapidly.",
            "Maren suddenly clutches her wounds and slumps to the ground.",
            "Some of Maren's wounds disappear.",
            "A brilliant light flashes around Maren, and their wounds begin to close.",
        ] {
            assert!(!attack_line(line), "{line}");
        }
    }

    /// The core word of a ladder verb, the one the presets color.
    fn word(verb: &'static str) -> &'static str {
        verb.split(' ')
            .find(|w| {
                w.chars().all(|c| c.is_ascii_alphabetic())
                    && !["do", "does", "things", "to"].contains(w)
            })
            .unwrap_or(verb)
    }

    #[test]
    fn the_ladder_holds_the_verbs_the_damage_presets_color() {
        let presets = include_str!("../../../../src/automation/presets.ts");
        let start = presets
            .find("const DAMAGE_VERBS = [")
            .expect("presets.ts lists DAMAGE_VERBS");
        let list = &presets[start..];
        let list = &list[list.find('[').unwrap_or(0) + 1..list.find("];").unwrap_or(0)];
        let theirs: Vec<&str> = list
            .split(',')
            .map(|w| w.trim().trim_matches('\''))
            .filter(|w| !w.is_empty())
            .collect();
        // The presets color damage, so a miss is not among them. They
        // list each step's verb after anyone else first. They also color
        // the line the assassinate skill prints in `skills.c`, which is
        // not a line of `dam_message`.
        assert_eq!(LADDER[0].vs, "miss");
        let assassinate = ["EVISCERATES", "EVISCERATE"];
        let theirs: Vec<&str> = theirs
            .into_iter()
            .filter(|w| !assassinate.contains(w))
            .collect();
        let mut ours: Vec<&str> = LADDER[1..]
            .iter()
            .flat_map(|step| [word(step.vp), word(step.vs)])
            .collect();
        ours.dedup();
        assert_eq!(ours, theirs);
    }

    #[test]
    fn the_mark_turns_at_decimate() {
        let turn = LADDER
            .iter()
            .position(|step| step.mark == '!')
            .expect("a step ends with !");
        assert_eq!(LADDER[turn].vs, "decimate");
        let leftover: Vec<&Step> = LADDER[turn..].iter().filter(|s| s.mark != '!').collect();
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn knows_an_attack_line_of_your_own_fight() {
        for yours in [
            "Your slash hits a Blackwatch guard.",
            "You hit a Blackwatch guard.",
            "A Blackwatch guard's slash hits you.",
            "A Blackwatch guard decimates you!",
            "You hit yourself.",
            "The sun's rays hit you.",
        ] {
            assert!(your_attack_line(yours), "{yours}");
        }
        for not_yours in [
            "Maren's slash hits a Blackwatch guard.",
            "Tolliver hits Orla.",
            "[Market Square] A Blackwatch guard's slash hits you.",
            "You dodge a Blackwatch guard's attack.",
        ] {
            assert!(!your_attack_line(not_yours), "{not_yours}");
        }
    }
}

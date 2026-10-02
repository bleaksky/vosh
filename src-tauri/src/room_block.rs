//! Which lines of a room look list what the room holds.
//!
//! A look on Aabahran prints the room's name, its description and its
//! exits line, then what the room holds in three runs (`act_info.c`
//! `do_look`).
//!
//! - The armies (`armies.c` `show_room_armies`), one line for each group
//!   that shares a long text. No packet names them. The server prints each
//!   one as `%s``\n\r`, so the reset code of two backticks, `ESC[0;0m`,
//!   ends every army line, and nothing comes before its text unless you
//!   belong to an area cabal, which puts the cabal and a health bar there.
//! - The things on the floor (`act_info.c` `list_to_char`), one line for
//!   each long text, five spaces before it, or a count like `( 2) ` for
//!   the objects that share it. An object with no long text prints no
//!   line. Room.Items names each object you can see, one entry each.
//! - The people (`act_info.c` `show_char_to_char`), one line each, with
//!   nothing before the text. Room.Chars names each one you can see. The
//!   units of a city raid (`raid.c`) are mobs and print here too.
//!
//! An inventory has the shape of the things, and an army or a person a
//! line with no shape at all, so the session counts. Both packets reach
//! Vosh before the look's text, since `gmcp_send` writes the socket at
//! once while text waits for the end of the pulse. So the session
//! follows each look, line by line.
//!
//! - Room.Chars and Room.Items each hold a count for the next exits line.
//! - The exits line opens the block and takes both counts.
//! - Lines shaped like an army come first, each one an army. An army
//!   line ends in the reset and holds no other code, or opens with the
//!   cabal in brackets an area cabal sees. The lines the game colors
//!   whole, such as `You have explored a quarter of The Eastern Road.`
//!   after a move into an empty room, carry a code of their own, so they
//!   close the block.
//! - Then lines shaped like a thing, each one spending its count of the
//!   objects Room.Items named, until none are left.
//! - Then as many lines as Room.Chars named, each one a person.
//! - A blank line, a line past the counts, a new exits line, your prompt,
//!   a GA or EOR and a disconnect each close the block. So a say or an
//!   arrival after the people in the same pulse stays a plain line.
//!
//! A ranger's `You spot some fresh spur.` line, which follows the exits
//! line when the game draws its minimap, leaves the block open and stays
//! plain. A look with no Room.Items before it takes every line shaped
//! like a thing until the first person, and a look with no Room.Chars
//! before it lists its armies and things only, since nothing says how
//! many people follow. Lines in the block run [`MatchScope::Room`], so
//! Line and Room triggers both see them. A person's line that names the
//! one you target with `tar` (see [`names_target`]) runs
//! [`MatchScope::RoomTarget`], so Your target triggers see it too.
//!
//! Rare looks the counts get wrong by one: a mob with no long text or a
//! character in catalepsy (one line after the people turns into a room
//! line), people that height sense or sense evil shows past Room.Chars
//! (the last of them stays a plain line), a long text of two lines, a
//! mob whose long text ends in the reset with no other code and no thing
//! before it (it reads as an army, and one line after the people turns
//! into a room line), armies with your color off (they read as people),
//! and two looks in one pulse.
//!
//! [`MatchScope::Room`]: vosh_trigger::MatchScope::Room
//! [`MatchScope::RoomTarget`]: vosh_trigger::MatchScope::RoomTarget

use std::sync::OnceLock;

use regex::Regex;

/// The exits line a look prints with autoexit on (`act_info.c` `do_exits`
/// with "auto"). Each exit shows by its full name, in parentheses while
/// closed, with a `+` where you see a trap, or the line reads `none`. The
/// prompt's `%e` code prints single letters instead, so a prompt never
/// opens a block.
pub(crate) const EXITS_PATTERN: &str =
    r"^\[Exits:(?: none|(?: \(?\+?(?:north|east|south|west|up|down)\)?)+)\]$";

/// A thing on the floor (`act_info.c` `list_to_char`), five spaces, or a
/// count like `( 2) ` or `(12) ` for things with the same text, which the
/// group captures.
const THING_PATTERN: &str = r"^(?:     |\(\s?(\d+)\) )\S";

/// A ranger's tracks (skills4.c `show_tracks`).
const SPUR_PATTERN: &str = r"^You spot some (?:new|fresh|recent|old) spur\.$";

/// The code two backticks send (`comm.c` `process_color`), which ends
/// every army line (`armies.c` `show_room_armies`).
const RESET: &[u8] = b"\x1b[0;0m";

/// Whether a line with no shape of a thing has the shape of an army line
/// (`armies.c` `show_room_armies`). To a player outside an area cabal the
/// server prints the long text and the reset, and no army in the area
/// files holds a color code, so the reset is the only code in the line.
/// To a member of an area cabal it prints the cabal in brackets first.
/// The lines the game colors whole carry a code of their own before the
/// reset, as `explore_room` does with `You have explored a quarter of
/// The Eastern Road.` after a move into an empty room.
fn army_shaped(plain: &str, bytes: &[u8]) -> bool {
    let Some(text) = bytes.strip_suffix(RESET) else {
        return false;
    };
    !text.contains(&0x1b) || plain.starts_with('[')
}

/// `pattern`, compiled once into `cell`.
fn compiled(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("a room pattern compiles"))
}

fn exits() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    compiled(&CELL, EXITS_PATTERN)
}

fn thing() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    compiled(&CELL, THING_PATTERN)
}

fn spur() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    compiled(&CELL, SPUR_PATTERN)
}

/// How many objects a line shaped like a thing stands for, its count
/// or 1, or None for a line of any other shape.
fn thing_count(plain: &str) -> Option<usize> {
    let caps = thing().captures(plain)?;
    Some(
        caps.get(1)
            .and_then(|n| n.as_str().parse().ok())
            .unwrap_or(1),
    )
}

/// What a line is to the look the session follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoomLine {
    /// No line the room lists, outside a look or past its end.
    Other,
    /// An army or a group of armies with the same long text.
    Army,
    /// A thing on the floor, or the things that share a long text.
    Thing,
    /// A person, a player or a mob.
    Person,
}

/// The look the session is following, if any. Session state that lives
/// in the profile and resets on a disconnect.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct RoomBlock {
    /// How many people the latest Room.Chars named, held for the next
    /// exits line.
    people: Option<usize>,
    /// How many objects the latest Room.Items named, held for the next
    /// exits line.
    things: Option<usize>,
    /// The open block, from its exits line to its last line.
    open: Option<Open>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Open {
    /// Objects still to come, or None when no Room.Items came before
    /// the look.
    things_left: Option<usize>,
    /// People lines still to come, or None when no Room.Chars came
    /// before the look.
    people_left: Option<usize>,
    /// The run the block is in.
    run: Run,
}

/// The runs of a look, in the order the server prints them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Run {
    Armies,
    Things,
    People,
}

impl RoomBlock {
    /// Room.Chars named `count` people.
    pub(crate) fn room_chars(&mut self, count: usize) {
        self.people = Some(count);
    }

    /// Room.Items named `count` objects.
    pub(crate) fn room_items(&mut self, count: usize) {
        self.things = Some(count);
    }

    /// The next complete line of text, `plain` without ANSI and `bytes`
    /// as the game sent it. Returns what the line is to the open look.
    pub(crate) fn line(&mut self, plain: &str, bytes: &[u8]) -> RoomLine {
        if exits().is_match(plain) {
            self.open = Some(Open {
                things_left: self.things.take(),
                people_left: self.people.take(),
                run: Run::Armies,
            });
            return RoomLine::Other;
        }
        let Some(open) = self.open.as_mut() else {
            return RoomLine::Other;
        };
        if plain.trim().is_empty() {
            self.open = None;
            return RoomLine::Other;
        }
        let count = thing_count(plain);
        if open.run == Run::Armies {
            if spur().is_match(plain) {
                return RoomLine::Other;
            }
            if count.is_none() && army_shaped(plain, bytes) {
                return RoomLine::Army;
            }
        }
        if let (Some(count), Run::Armies | Run::Things) = (count, open.run) {
            match open.things_left {
                None => {
                    open.run = Run::Things;
                    return RoomLine::Thing;
                }
                Some(left) if left > 0 => {
                    open.things_left = Some(left.saturating_sub(count));
                    open.run = Run::Things;
                    return RoomLine::Thing;
                }
                // Every object Room.Items named has its line.
                Some(_) => {}
            }
        }
        match open.people_left {
            Some(left) if left > 0 => {
                open.people_left = Some(left - 1);
                open.run = Run::People;
                RoomLine::Person
            }
            _ => {
                self.open = None;
                RoomLine::Other
            }
        }
    }

    /// Your prompt, a GA or an EOR. The look is over. The counts the
    /// packets left stay, since a prompt the session reads at its line
    /// end can land after the next look's packets.
    pub(crate) fn end(&mut self) {
        self.open = None;
    }
}

/// What `char_to_char` prints after a person in a fight, before the one
/// they fight.
const FIGHTING: &str = " is here, fighting ";

/// The articles a mob's short text starts with.
const ARTICLES: [&str; 4] = ["a ", "an ", "the ", "some "];

/// `text` without a leading article, in any case.
fn without_article(text: &str) -> &str {
    let lower = text.to_ascii_lowercase();
    ARTICLES
        .iter()
        .find(|article| lower.starts_with(*article))
        .map_or(text, |article| &text[article.len()..])
}

/// Whether `words` stands in `text` as whole words, in any case.
fn has_words(text: &str, words: &str) -> bool {
    let words = words.trim().to_lowercase();
    if words.is_empty() {
        return false;
    }
    let text = text.to_lowercase();
    text.match_indices(&words).any(|(at, found)| {
        let before = text[..at].chars().next_back();
        let after = text[at + found.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// Whether `plain`, the line of a person a room look lists, is the line
/// of your target. `target` is what you gave `tar`, and `name` the name
/// of the Room.Chars entry it points at, if any.
///
/// The name counts first, since `tar gris` points at The Baron Grisvald
/// while gris is no word of his line. The target counts too, since a
/// mob's long text often words its name another way, as `A large murder
/// of crows` for `a murder of crows`. Each counts as whole words in any
/// case, with or without the article a short text starts with. Who a
/// fighting person fights never counts, so `Maren is here, fighting a
/// villager.` is no line of the villager.
pub(crate) fn names_target(plain: &str, target: &str, name: Option<&str>) -> bool {
    let who = plain.split(FIGHTING).next().unwrap_or(plain);
    name.into_iter()
        .chain(std::iter::once(target))
        .flat_map(|text| [text, without_article(text)])
        .any(|text| has_words(who, text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use RoomLine::{Army, Other, Person, Thing};

    const EXITS_LINE: &str = "[Exits: north east south west]";
    const HELM: &str = "     A black-steel helm is here, gleaming darkly.";
    const GAUNTLETS: &str = "( 2) A pair of black-steel gauntlets rests on the ground.";
    const VILLAGER: &str = "A Blackwatch villager scurries about, taking care of business.";
    const RESTING: &str = "Tolliver is resting here.";
    const WALKS_IN: &str = "Tolliver walks in.";
    /// Knight Fortress, army 12 in area/limbo.are, as `show_room_armies`
    /// prints it to a player outside an area cabal.
    const FORTRESS: &str = "A mighty Fortress looms over the area.\x1b[0;0m";
    /// A Demon Horde, army 18 in area/limbo.are.
    const HORDE: &str = "A horde of Demons hovers overhead, their talons dripping blood.\x1b[0;0m";

    /// What each line is, in order. Each line is its own bytes.
    fn kinds(block: &mut RoomBlock, lines: &[&str]) -> Vec<RoomLine> {
        lines
            .iter()
            .map(|line| block.line(&vosh_ansi::plain_text(line.as_bytes()), line.as_bytes()))
            .collect()
    }

    #[test]
    fn the_exits_pattern_reads_every_exits_line_a_look_prints() {
        for line in [
            "[Exits: north east south west]",
            "[Exits: (north) south]",
            "[Exits: (+north) south]",
            "[Exits: +east]",
            "[Exits: none]",
            "[Exits: north south east west up down]",
        ] {
            assert!(exits().is_match(line), "{line}");
        }
        for line in [
            "[Exits: N E (S) W]",
            "[Exits: --- ]",
            "[Exits: ??? ]",
            "Obvious exits:",
            "Tolliver says '[Exits: north]'",
        ] {
            assert!(!exits().is_match(line), "{line}");
        }
    }

    #[test]
    fn a_thing_counts_the_objects_its_line_stands_for() {
        assert_eq!(thing_count(HELM), Some(1));
        assert_eq!(thing_count(GAUNTLETS), Some(2));
        assert_eq!(
            thing_count("(12) A pair of black-steel gauntlets rests on the ground."),
            Some(12)
        );
        assert_eq!(thing_count(VILLAGER), None);
        assert_eq!(thing_count("      "), None);
    }

    #[test]
    fn room_chars_counts_the_people_after_the_things() {
        let mut block = RoomBlock::default();
        block.room_chars(2);
        assert_eq!(
            kinds(
                &mut block,
                &[EXITS_LINE, HELM, GAUNTLETS, VILLAGER, RESTING, WALKS_IN, HELM],
            ),
            [Other, Thing, Thing, Person, Person, Other, Other]
        );
    }

    #[test]
    fn an_army_comes_before_the_things_and_costs_no_person() {
        // Room 6909, The Crossroads, in area/eastroad.are, with a herb
        // stand from skills3.c and mob 6904, which resets there.
        let mut block = RoomBlock::default();
        block.room_chars(1);
        block.room_items(1);
        assert_eq!(
            kinds(
                &mut block,
                &[
                    EXITS_LINE,
                    FORTRESS,
                    "     A stand of blue dried leaves grows in the wild here.",
                    "A young werebeast stands here, leaning on his spear.",
                    "A werebeast looks into the sky.",
                ],
            ),
            [Other, Army, Thing, Person, Other]
        );
    }

    #[test]
    fn an_army_with_no_things_after_it_still_leaves_the_people_their_count() {
        // Room 6904, Thickening Woods, with mob 6936, which resets there.
        let mut block = RoomBlock::default();
        block.room_chars(1);
        block.room_items(0);
        assert_eq!(
            kinds(
                &mut block,
                &[
                    "[Exits: east west]",
                    HORDE,
                    "A large murder of crows nearly turns the trees black here.",
                    WALKS_IN,
                ],
            ),
            [Other, Army, Person, Other]
        );
    }

    #[test]
    fn two_armies_each_take_a_line() {
        let mut block = RoomBlock::default();
        block.room_chars(0);
        block.room_items(0);
        assert_eq!(
            kinds(&mut block, &[EXITS_LINE, FORTRESS, HORDE, WALKS_IN]),
            [Other, Army, Army, Other]
        );
    }

    #[test]
    fn a_line_ending_in_the_reset_after_the_things_is_no_army() {
        let mut block = RoomBlock::default();
        block.room_chars(0);
        block.room_items(1);
        assert_eq!(
            kinds(&mut block, &[EXITS_LINE, HELM, FORTRESS]),
            [Other, Thing, Other]
        );
        let mut block = RoomBlock::default();
        block.room_chars(1);
        block.room_items(0);
        assert_eq!(
            kinds(&mut block, &[EXITS_LINE, VILLAGER, FORTRESS]),
            [Other, Person, Other]
        );
    }

    #[test]
    fn room_items_counts_the_things_and_a_count_spends_its_number() {
        // Three objects, one helm and two gauntlets on one line, and no
        // one else in the room. The inventory line after them is no
        // thing of the room.
        let mut block = RoomBlock::default();
        block.room_chars(0);
        block.room_items(3);
        assert_eq!(
            kinds(
                &mut block,
                &[EXITS_LINE, GAUNTLETS, HELM, "     a black-steel helm"]
            ),
            [Other, Thing, Thing, Other]
        );
    }

    #[test]
    fn a_line_shaped_like_a_thing_past_the_items_is_a_person_or_ends_the_look() {
        let mut block = RoomBlock::default();
        block.room_chars(1);
        block.room_items(1);
        assert_eq!(
            kinds(&mut block, &[EXITS_LINE, HELM, HELM, HELM]),
            [Other, Thing, Person, Other]
        );
    }

    #[test]
    fn with_no_one_in_the_room_the_things_end_the_block() {
        let mut block = RoomBlock::default();
        block.room_chars(0);
        assert_eq!(
            kinds(&mut block, &[EXITS_LINE, HELM, WALKS_IN, VILLAGER]),
            [Other, Thing, Other, Other]
        );
    }

    #[test]
    fn with_no_room_chars_only_the_armies_and_things_count() {
        let mut block = RoomBlock::default();
        assert_eq!(
            kinds(&mut block, &[EXITS_LINE, FORTRESS, HELM, VILLAGER, RESTING]),
            [Other, Army, Thing, Other, Other]
        );
    }

    #[test]
    fn a_blank_line_closes_the_block() {
        let mut block = RoomBlock::default();
        block.room_chars(1);
        assert_eq!(
            kinds(&mut block, &[EXITS_LINE, "", VILLAGER]),
            [Other, Other, Other]
        );
    }

    #[test]
    fn a_line_shaped_like_a_thing_after_a_person_is_a_person() {
        let mut block = RoomBlock::default();
        block.room_chars(2);
        assert_eq!(
            kinds(&mut block, &[EXITS_LINE, VILLAGER, HELM, HELM]),
            [Other, Person, Person, Other]
        );
    }

    #[test]
    fn the_spur_line_stays_plain_and_keeps_the_block_open() {
        let mut block = RoomBlock::default();
        block.room_chars(1);
        assert_eq!(
            kinds(
                &mut block,
                &[
                    EXITS_LINE,
                    "You spot some fresh spur.",
                    FORTRESS,
                    HELM,
                    RESTING
                ]
            ),
            [Other, Other, Army, Thing, Person]
        );
    }

    #[test]
    fn the_prompt_ends_the_block_and_an_inventory_after_it_stays_plain() {
        let mut block = RoomBlock::default();
        block.room_chars(1);
        assert_eq!(kinds(&mut block, &[EXITS_LINE, HELM]), [Other, Thing]);
        block.end();
        assert_eq!(
            kinds(&mut block, &["You are carrying:", HELM, VILLAGER]),
            [Other, Other, Other]
        );
    }

    #[test]
    fn the_counts_wait_for_their_exits_line_past_a_prompt() {
        // The prompt before this look reads at its line end, after the
        // look's packets came.
        let mut block = RoomBlock::default();
        block.room_chars(1);
        block.room_items(1);
        block.end();
        assert_eq!(
            kinds(&mut block, &[EXITS_LINE, HELM, VILLAGER, WALKS_IN]),
            [Other, Thing, Person, Other]
        );
    }

    #[test]
    fn each_exits_line_takes_its_own_counts() {
        let mut block = RoomBlock::default();
        block.room_chars(1);
        block.room_items(0);
        assert_eq!(kinds(&mut block, &[EXITS_LINE, VILLAGER]), [Other, Person]);
        // A second look with no packets of its own lists armies and
        // things only.
        assert_eq!(
            kinds(&mut block, &[EXITS_LINE, HELM, HELM, VILLAGER]),
            [Other, Thing, Thing, Other]
        );
    }

    #[test]
    fn a_dark_room_with_no_exits_line_opens_no_block() {
        let mut block = RoomBlock::default();
        assert_eq!(
            kinds(
                &mut block,
                &["It is pitch black ... ", FORTRESS, HELM, VILLAGER]
            ),
            [Other, Other, Other, Other]
        );
    }

    #[test]
    fn a_line_the_game_colors_whole_after_an_empty_room_is_no_army() {
        // explore.c `explore_room`, which `move_char` runs right after the
        // look, in `8 bold black. Room 6910 in area/eastroad.are holds no
        // one and nothing.
        let mut block = RoomBlock::default();
        block.room_chars(0);
        block.room_items(0);
        assert_eq!(
            kinds(
                &mut block,
                &[
                    "[Exits: east west]",
                    "\x1b[0;1;30mYou have explored a quarter of The Eastern Road.\x1b[0;0m",
                ],
            ),
            [Other, Other]
        );
    }

    #[test]
    fn an_area_cabal_sees_an_army_with_its_cabal_and_health_first() {
        // `show_room_armies` to a member of an area cabal. Knight Fortress
        // at full health, its cabal `6KNIGHT`` padded to 15 and the bar
        // from `short_bar` in misc.c in `2 green.
        let fortress = "[\x1b[0;36mKNIGHT\x1b[0;0m]   \x1b[0;0m \
                        [\x1b[0;32m||||||\x1b[0;0m]    A mighty Fortress looms over the area.\x1b[0;0m";
        let mut block = RoomBlock::default();
        block.room_chars(1);
        block.room_items(0);
        assert_eq!(
            kinds(&mut block, &[EXITS_LINE, fortress, VILLAGER]),
            [Other, Army, Person]
        );
    }

    const WEREBEAST: &str = "A young werebeast stands here, leaning on his spear.";
    const CROWS: &str = "A large murder of crows nearly turns the trees black here.";

    #[test]
    fn the_target_names_a_line_as_whole_words_in_any_case() {
        assert!(names_target(WEREBEAST, "werebeast", Some("a werebeast")));
        assert!(names_target(WEREBEAST, "WEREBEAST", None));
        assert!(names_target(RESTING, "tolliver", Some("Tolliver")));
        assert!(names_target(
            "[\u{1b}[0;31mAFK\u{1b}[0;0m] Tolliver is resting here.",
            "Tolliver",
            None
        ));
        // A part of a word is no whole word.
        assert!(!names_target(WEREBEAST, "were", None));
        assert!(!names_target(VILLAGER, "village", None));
        assert!(!names_target(WEREBEAST, "", None));
        assert!(!names_target(VILLAGER, "werebeast", Some("a werebeast")));
    }

    #[test]
    fn the_room_chars_name_the_target_points_at_counts_first() {
        // tar toll points at Tolliver, and toll is no word of the line.
        assert!(names_target(RESTING, "toll", Some("Tolliver")));
        assert!(!names_target(RESTING, "toll", None));
        assert!(names_target(
            "The Baron Grisvald is resting here.",
            "gris",
            Some("The Baron Grisvald")
        ));
        // tar crow points at a murder of crows, whose line words the name
        // another way after its article.
        assert!(names_target(CROWS, "crow", Some("a murder of crows")));
        assert!(!names_target(CROWS, "crow", None));
    }

    #[test]
    fn a_target_set_by_its_number_counts_without_its_article() {
        // tar 1 and tarn set the target to the full Room.Chars name.
        let name = "a murder of crows";
        assert!(names_target(CROWS, name, Some(name)));
        assert!(names_target(VILLAGER, "a villager", Some("a villager")));
    }

    #[test]
    fn who_a_person_fights_is_no_line_of_theirs() {
        let villager = "A villager is here, fighting Maren.";
        let maren = "Maren is here, fighting a villager.";
        assert!(names_target(villager, "villager", Some("a villager")));
        assert!(!names_target(maren, "villager", Some("a villager")));
        assert!(names_target(maren, "maren", Some("Maren")));
        assert!(!names_target(villager, "maren", Some("Maren")));
    }
}

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
//! `do_look` sends Room.Info, Room.Chars and Room.Items right after the
//! people, and a move (`act_move.c` `move_char`) and an immortal's goto
//! (`act_wiz.c` `do_goto`) show the room through the same `do_look`. Where
//! the packets land beside the text depends on the server. Before
//! d50e4a24 `gmcp_send` wrote each packet to the socket at once while
//! the text waited for the end of the pulse, so the packets came before
//! the look. Since d50e4a24 the packets wait in the output buffer with
//! the text, so they come right after the people and before anything else
//! the pulse prints, such as `You have explored a quarter of The Eastern
//! Road.` after a move or a mob that greets you. The session reads both.
//!
//! - An exits line that no prompt follows waits for its packets. A
//!   Room.Chars or Room.Items packet that comes while it waits belongs to
//!   that look, which is over, and says the game sends its packets after
//!   the text. Any other one comes before the look it goes with, says the
//!   game sends its packets first, and holds its count for the next exits
//!   line. Until a packet says which, the session takes the packets to
//!   follow the text, as the game sends them now.
//! - The exits line opens the block. Where the packets come first it
//!   takes both counts. Where they follow, no count is known yet, so the
//!   block runs until the packets come.
//! - Lines shaped like an army come first, each one an army. An army
//!   line ends in the reset and holds no other code, or opens with the
//!   cabal in brackets an area cabal sees. The lines the game colors
//!   whole, such as `You have explored a quarter of The Eastern Road.`
//!   after a move into an empty room where the packets come first, carry
//!   a code of their own, so they close the block.
//! - Then lines shaped like a thing, each one spending its count of the
//!   objects Room.Items named, until none are left, or every one of them
//!   where no count is known.
//! - Then as many lines as Room.Chars named, each one a person, in the
//!   order of Room.Chars, since the look and the packet both walk the
//!   people of the room in one order. Where the packets follow, every line
//!   until they come is a person.
//! - A blank line, a line past the counts, a new exits line, the packets
//!   that follow a look, your prompt, a GA or EOR and a disconnect each
//!   close the block. So a say or an arrival after the people in the same
//!   pulse stays a plain line.
//!
//! A ranger's `You spot some fresh spur.` line, which follows the exits
//! line when the game draws its minimap, leaves the block open and stays
//! plain. Where the packets come first, a look with no Room.Items before
//! it takes every line shaped like a thing until the first person, and a
//! look with no Room.Chars before it lists its armies and things only,
//! since nothing says how many people follow. Lines in the block run
//! [`MatchScope::Room`], so Line and Room triggers both see them. The
//! person at the place in Room.Chars of the one you target with `tar`,
//! the place `tar` marks with `>`, runs [`MatchScope::RoomTarget`], so
//! Your target triggers see that line too, however its long text words
//! the name. Where the packets follow the look, the Room.Chars the session
//! holds is the one the last look sent, so a person has a place only
//! when a line before the exits line named the room that Room.Info last
//! named, as a look in the same room does. After a move or a goto into
//! another room no line of the look is your target's.
//!
//! Rare looks the counts get wrong by one: a mob with no long text or a
//! character in catalepsy (one line after the people turns into a room
//! line), people that height sense or sense evil shows past Room.Chars
//! (the last of them stays a plain line, and the color of your target
//! can land on the line before theirs), a long text of two lines, a mob
//! whose long text ends in the reset with no other code and no thing
//! before it (it reads as an army, and one line after the people turns
//! into a room line), armies with your color off (they read as people),
//! two looks in one pulse, and a look that sends no packets where they
//! follow the text, as `do_look` sends none while burrowed, which runs to
//! the blank line before the prompt.
//!
//! [`MatchScope::Room`]: vosh_automation::trigger::MatchScope::Room
//! [`MatchScope::RoomTarget`]: vosh_automation::trigger::MatchScope::RoomTarget

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
    /// A person, a player or a mob, with their place in the Room.Chars
    /// the session holds from 1, the place `tar` gives your target, or
    /// None when that Room.Chars is another room's.
    Person(Option<usize>),
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
    /// Whether the game sends a look's packets before its text, as it did
    /// before d50e4a24. False until a packet shows it, since the game
    /// sends them after the text now.
    packets_lead: bool,
    /// An exits line came and no prompt since, so the packets of its look
    /// can still follow.
    due: bool,
    /// The packets of the look that is due came, so the next line is past
    /// them.
    claimed: bool,
    /// The room the latest Room.Info named.
    room: Option<String>,
    /// A line since the last look named that room.
    named: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Open {
    /// Objects still to come, or None when no Room.Items came before
    /// the look.
    things_left: Option<usize>,
    /// People lines still to come, or None when no Room.Chars came
    /// before the look.
    people_left: Option<usize>,
    /// Every line until the packets is a person, since they follow the
    /// look.
    until_packets: bool,
    /// Whether the Room.Chars the session holds is this room's, so a
    /// person has a place in it.
    placed: bool,
    /// People lines so far.
    people_seen: usize,
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
        if !self.trails() {
            self.people = Some(count);
        }
    }

    /// Room.Items named `count` objects.
    pub(crate) fn room_items(&mut self, count: usize) {
        if !self.trails() {
            self.things = Some(count);
        }
    }

    /// Room.Info named the room `name`.
    pub(crate) fn room_info(&mut self, name: &str) {
        let name = name.trim();
        self.room = (!name.is_empty()).then(|| name.to_string());
    }

    /// Whether a Room.Chars or Room.Items packet follows the look it goes
    /// with. The look is over then. Otherwise the packet comes before its
    /// look, and the game sends its packets first.
    fn trails(&mut self) -> bool {
        if self.due {
            self.claimed = true;
            self.packets_lead = false;
            self.open = None;
            self.named = false;
            true
        } else {
            self.packets_lead = true;
            false
        }
    }

    /// The next complete line of text, `plain` without ANSI and `bytes`
    /// as the game sent it. Returns what the line is to the open look.
    pub(crate) fn line(&mut self, plain: &str, bytes: &[u8]) -> RoomLine {
        if self.claimed {
            self.due = false;
            self.claimed = false;
        }
        if exits().is_match(plain) {
            let (things_left, people_left) = (self.things.take(), self.people.take());
            self.open = Some(if self.packets_lead {
                Open {
                    things_left,
                    people_left,
                    until_packets: false,
                    placed: true,
                    people_seen: 0,
                    run: Run::Armies,
                }
            } else {
                Open {
                    things_left: None,
                    people_left: None,
                    until_packets: true,
                    placed: self.named,
                    people_seen: 0,
                    run: Run::Armies,
                }
            });
            self.due = true;
            self.named = false;
            return RoomLine::Other;
        }
        let Some(open) = self.open.as_mut() else {
            if let Some(room) = &self.room {
                self.named |= plain.contains(room.as_str());
            }
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
        let person = match open.people_left {
            _ if open.until_packets => true,
            Some(left) if left > 0 => {
                open.people_left = Some(left - 1);
                true
            }
            _ => false,
        };
        if !person {
            self.open = None;
            return RoomLine::Other;
        }
        open.people_seen += 1;
        open.run = Run::People;
        RoomLine::Person(open.placed.then_some(open.people_seen))
    }

    /// Your prompt, a GA or an EOR. The look is over, and packets after
    /// it come before the next one. The counts the packets left stay,
    /// since a prompt the session reads at its line end can land after
    /// the next look's packets.
    pub(crate) fn end(&mut self) {
        self.open = None;
        self.due = false;
        self.claimed = false;
        self.named = false;
    }
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
            .map(|line| {
                block.line(
                    &vosh_protocol::ansi::plain_text(line.as_bytes()),
                    line.as_bytes(),
                )
            })
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
            [
                Other,
                Thing,
                Thing,
                Person(Some(1)),
                Person(Some(2)),
                Other,
                Other
            ]
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
            [Other, Army, Thing, Person(Some(1)), Other]
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
            [Other, Army, Person(Some(1)), Other]
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
            [Other, Person(Some(1)), Other]
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
            [Other, Thing, Person(Some(1)), Other]
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
        // Room.Items alone came first, so the game sends its packets
        // before the look and nothing says how many people follow.
        let mut block = RoomBlock::default();
        block.room_items(1);
        assert_eq!(
            kinds(&mut block, &[EXITS_LINE, FORTRESS, HELM, VILLAGER, RESTING]),
            [Other, Army, Thing, Other, Other]
        );
    }

    #[test]
    fn packets_after_the_look_end_it_and_the_lines_before_them_are_the_room() {
        // Since d50e4a24 the game queues the packets after the people.
        let mut block = RoomBlock::default();
        assert_eq!(
            kinds(
                &mut block,
                &[EXITS_LINE, FORTRESS, HELM, GAUNTLETS, VILLAGER, RESTING]
            ),
            [Other, Army, Thing, Thing, Person(None), Person(None)]
        );
        block.room_info("The Bank of Aabahran");
        block.room_chars(2);
        block.room_items(3);
        assert_eq!(kinds(&mut block, &[WALKS_IN, HELM]), [Other, Other]);
        // Neither count waits for the next look, which runs until its own
        // packets.
        block.end();
        assert_eq!(
            kinds(&mut block, &["[Exits: east west]", HELM, VILLAGER, RESTING]),
            [Other, Thing, Person(None), Person(None)]
        );
        block.room_chars(2);
        assert_eq!(kinds(&mut block, &[WALKS_IN]), [Other]);
    }

    #[test]
    fn a_move_after_packets_that_follow_takes_no_count_from_the_room_left() {
        // Room 6910 holds no one and nothing. Its packets follow its look
        // and the explore line follows them. The move west into room 6909
        // lists an army, a thing and a mob all the same.
        let mut block = RoomBlock::default();
        let explored = "\x1b[0;1;30mYou have explored a quarter of The Eastern Road.\x1b[0;0m";
        assert_eq!(kinds(&mut block, &["[Exits: east west]"]), [Other]);
        block.room_info("Nearing the Crossroads");
        block.room_chars(0);
        block.room_items(0);
        assert_eq!(kinds(&mut block, &[explored, ""]), [Other, Other]);
        block.end();
        assert_eq!(
            kinds(
                &mut block,
                &[
                    "\x1b[38;5;82m\x1b[0;33mThe Crossroads\x1b[0;0m\x1b[0;0m",
                    "[Exits: north east south west]",
                    FORTRESS,
                    "     A stand of blue dried leaves grows in the wild here.",
                    "A young werebeast stands here, leaning on his spear.",
                ],
            ),
            [Other, Other, Army, Thing, Person(None)]
        );
        block.room_info("The Crossroads");
        block.room_chars(1);
        block.room_items(1);
        assert_eq!(
            kinds(&mut block, &["A werebeast looks into the sky."]),
            [Other]
        );
    }

    #[test]
    fn a_look_in_the_room_room_info_named_gives_each_person_a_place() {
        let mut block = RoomBlock::default();
        block.room_info("The Bank of Aabahran");
        assert_eq!(
            kinds(
                &mut block,
                &[
                    "\x1b[0;1;30mThe Bank of Aabahran\x1b[0;0m [Room 5279]",
                    EXITS_LINE,
                    VILLAGER,
                    RESTING
                ]
            ),
            [Other, Other, Person(Some(1)), Person(Some(2))]
        );
    }

    #[test]
    fn packets_before_a_look_after_ones_that_followed_count_again() {
        let mut block = RoomBlock::default();
        assert_eq!(
            kinds(&mut block, &[EXITS_LINE, VILLAGER]),
            [Other, Person(None)]
        );
        block.room_chars(1);
        block.end();
        block.room_chars(1);
        block.room_items(0);
        assert_eq!(
            kinds(&mut block, &[EXITS_LINE, VILLAGER, WALKS_IN]),
            [Other, Person(Some(1)), Other]
        );
    }

    #[test]
    fn a_look_with_no_packets_where_they_follow_runs_to_the_blank_line() {
        let mut block = RoomBlock::default();
        assert_eq!(
            kinds(
                &mut block,
                &[EXITS_LINE, HELM, VILLAGER, WALKS_IN, "", RESTING]
            ),
            [Other, Thing, Person(None), Person(None), Other, Other]
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
            [Other, Person(Some(1)), Person(Some(2)), Other]
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
            [Other, Other, Army, Thing, Person(Some(1))]
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
            [Other, Thing, Person(Some(1)), Other]
        );
    }

    #[test]
    fn each_exits_line_takes_its_own_counts() {
        let mut block = RoomBlock::default();
        block.room_chars(1);
        block.room_items(0);
        assert_eq!(
            kinds(&mut block, &[EXITS_LINE, VILLAGER]),
            [Other, Person(Some(1))]
        );
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
            [Other, Army, Person(Some(1))]
        );
    }

    #[test]
    fn each_person_takes_the_next_place_in_room_chars() {
        // Room 4811 in area/gastride.are, where mobs 4801, 4808 and 4812
        // reset in that order, so the last stands first in the room.
        let mut block = RoomBlock::default();
        block.room_chars(3);
        block.room_items(0);
        assert_eq!(
            kinds(
                &mut block,
                &[
                    "[Exits: north south west]",
                    "A warrior medic stands here ready to tend to the wounded warriors.",
                    "A master warrior stands here watching over others.",
                    "A warrior stands here ready to train.",
                    WALKS_IN,
                ],
            ),
            [
                Other,
                Person(Some(1)),
                Person(Some(2)),
                Person(Some(3)),
                Other
            ]
        );
    }
}

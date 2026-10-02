//! Which lines of a room look list what the room holds.
//!
//! A look on Aabahran prints the room's name, its description and its
//! exits line, then the things on the floor, then the people (`act_info.c`
//! `do_look`, `list_to_char` and `show_char_to_char`). A thing has a shape, five
//! spaces or a count like `( 2) ` before its text, but an inventory has
//! the same shape, and a person's line has none at all. The game says how
//! many people a look shows, though. Its Room.Chars packet names each one
//! you can see, and it reaches Vosh before the look's text, since
//! `gmcp_send` writes the socket at once while text waits for the end of
//! the pulse. So the session follows each look, line by line.
//!
//! - Room.Chars holds the count of people for the next exits line.
//! - The exits line opens the block and takes that count.
//! - Lines shaped like a thing come first, each one a thing.
//! - Then as many lines as Room.Chars named, each one a person.
//! - A blank line, a line that is neither, a new exits line, your prompt,
//!   a GA or EOR and a disconnect each close the block.
//!
//! A ranger's `You spot some fresh spur.` line, which follows the exits
//! line when the game draws its minimap, leaves the block open and stays
//! plain. A look with no Room.Chars before it lists its things only,
//! since nothing says how many people follow. Lines in the block run
//! [`MatchScope::Room`], so Line and Room triggers both see them.
//!
//! Rare looks the count gets wrong by one: a mob with no long text or a
//! character in catalepsy (one line after the people turns into a room
//! line), people that height sense or sense evil shows past Room.Chars
//! (the last of them stays a plain line), a long text of two lines, an
//! army, and two looks in one pulse.

use std::sync::OnceLock;

use regex::Regex;
use vosh_trigger::MatchScope;

/// The exits line a look prints with autoexit on (`act_info.c` `do_exits`
/// with "auto"). Each exit shows by its full name, in parentheses while
/// closed, with a `+` where you see a trap, or the line reads `none`. The
/// prompt's `%e` code prints single letters instead, so a prompt never
/// opens a block.
pub(crate) const EXITS_PATTERN: &str =
    r"^\[Exits:(?: none|(?: \(?\+?(?:north|east|south|west|up|down)\)?)+)\]$";

/// A thing on the floor (`act_info.c` `list_to_char`), five spaces, or a
/// count like `( 2) ` or `(12) ` for things with the same text.
const THING_PATTERN: &str = r"^(?:     |\(\s?\d+\) )\S";

/// A ranger's tracks (skills4.c `show_tracks`).
const SPUR_PATTERN: &str = r"^You spot some (?:new|fresh|recent|old) spur\.$";

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

/// The look the session is following, if any. Session state that lives
/// in the profile and resets on a disconnect.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct RoomBlock {
    /// How many people the latest Room.Chars named, held for the next
    /// exits line.
    pending: Option<usize>,
    /// The open block, from its exits line to its last line.
    open: Option<Open>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Open {
    /// People lines still to come, or None when no Room.Chars came
    /// before the look.
    people_left: Option<usize>,
    /// A person's line came, so a line shaped like a thing is no longer
    /// one.
    people_started: bool,
}

impl RoomBlock {
    /// Room.Chars named `count` people.
    pub(crate) fn room_chars(&mut self, count: usize) {
        self.pending = Some(count);
    }

    /// `plain`, the next complete line of text, without ANSI. Returns the
    /// scope its triggers run in, [`MatchScope::Room`] for a thing or a
    /// person in the open block and [`MatchScope::Line`] for any other
    /// line.
    pub(crate) fn line(&mut self, plain: &str) -> MatchScope {
        if exits().is_match(plain) {
            self.open = Some(Open {
                people_left: self.pending.take(),
                people_started: false,
            });
            return MatchScope::Line;
        }
        let Some(open) = self.open.as_mut() else {
            return MatchScope::Line;
        };
        if plain.trim().is_empty() {
            self.open = None;
            return MatchScope::Line;
        }
        if spur().is_match(plain) {
            return MatchScope::Line;
        }
        if !open.people_started && thing().is_match(plain) {
            return MatchScope::Room;
        }
        match open.people_left {
            Some(left) if left > 0 => {
                open.people_started = true;
                open.people_left = Some(left - 1);
                MatchScope::Room
            }
            _ => {
                self.open = None;
                MatchScope::Line
            }
        }
    }

    /// Your prompt, a GA or an EOR. The look is over. The count Room.Chars
    /// left stays, since a prompt the session reads at its line end can
    /// land after the next look's packet.
    pub(crate) fn end(&mut self) {
        self.open = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use MatchScope::{Line, Room};

    const EXITS_LINE: &str = "[Exits: north east south west]";
    const HELM: &str = "     A black-steel helm is here, gleaming darkly.";
    const GAUNTLETS: &str = "( 2) A pair of black-steel gauntlets rests on the ground.";
    const VILLAGER: &str = "A Blackwatch villager scurries about, taking care of business.";
    const RESTING: &str = "Tolliver is resting here.";
    const WALKS_IN: &str = "Tolliver walks in.";

    /// The scope each line takes, in order.
    fn scopes(block: &mut RoomBlock, lines: &[&str]) -> Vec<MatchScope> {
        lines.iter().map(|line| block.line(line)).collect()
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
    fn room_chars_counts_the_people_after_the_things() {
        let mut block = RoomBlock::default();
        block.room_chars(2);
        assert_eq!(
            scopes(
                &mut block,
                &[EXITS_LINE, HELM, GAUNTLETS, VILLAGER, RESTING, WALKS_IN, HELM],
            ),
            [Line, Room, Room, Room, Room, Line, Line]
        );
    }

    #[test]
    fn with_no_one_in_the_room_the_things_end_the_block() {
        let mut block = RoomBlock::default();
        block.room_chars(0);
        assert_eq!(
            scopes(&mut block, &[EXITS_LINE, HELM, WALKS_IN, VILLAGER]),
            [Line, Room, Line, Line]
        );
    }

    #[test]
    fn with_no_room_chars_only_the_things_count() {
        let mut block = RoomBlock::default();
        assert_eq!(
            scopes(&mut block, &[EXITS_LINE, HELM, VILLAGER, RESTING]),
            [Line, Room, Line, Line]
        );
    }

    #[test]
    fn a_blank_line_closes_the_block() {
        let mut block = RoomBlock::default();
        block.room_chars(1);
        assert_eq!(
            scopes(&mut block, &[EXITS_LINE, "", VILLAGER]),
            [Line, Line, Line]
        );
    }

    #[test]
    fn a_line_shaped_like_a_thing_after_a_person_is_a_person() {
        let mut block = RoomBlock::default();
        block.room_chars(2);
        assert_eq!(
            scopes(&mut block, &[EXITS_LINE, VILLAGER, HELM, HELM]),
            [Line, Room, Room, Line]
        );
    }

    #[test]
    fn the_spur_line_stays_plain_and_keeps_the_block_open() {
        let mut block = RoomBlock::default();
        block.room_chars(1);
        assert_eq!(
            scopes(
                &mut block,
                &[EXITS_LINE, "You spot some fresh spur.", HELM, RESTING]
            ),
            [Line, Line, Room, Room]
        );
    }

    #[test]
    fn the_prompt_ends_the_block_and_an_inventory_after_it_stays_plain() {
        let mut block = RoomBlock::default();
        block.room_chars(1);
        assert_eq!(scopes(&mut block, &[EXITS_LINE, HELM]), [Line, Room]);
        block.end();
        assert_eq!(
            scopes(&mut block, &["You are carrying:", HELM, VILLAGER]),
            [Line, Line, Line]
        );
    }

    #[test]
    fn the_count_waits_for_its_exits_line_past_a_prompt() {
        // The prompt before this look reads at its line end, after the
        // look's packet came.
        let mut block = RoomBlock::default();
        block.room_chars(1);
        block.end();
        assert_eq!(
            scopes(&mut block, &[EXITS_LINE, VILLAGER, WALKS_IN]),
            [Line, Room, Line]
        );
    }

    #[test]
    fn each_exits_line_takes_its_own_count() {
        let mut block = RoomBlock::default();
        block.room_chars(1);
        assert_eq!(scopes(&mut block, &[EXITS_LINE, VILLAGER]), [Line, Room]);
        // A second look with no packet of its own lists things only.
        assert_eq!(
            scopes(&mut block, &[EXITS_LINE, HELM, VILLAGER]),
            [Line, Room, Line]
        );
    }

    #[test]
    fn a_dark_room_with_no_exits_line_opens_no_block() {
        let mut block = RoomBlock::default();
        assert_eq!(
            scopes(&mut block, &["It is pitch black ... ", HELM, VILLAGER]),
            [Line, Line, Line]
        );
    }
}

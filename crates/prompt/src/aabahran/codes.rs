//! Every value code `bust_a_prompt` knows (`comm.c:1815-1940`), the field
//! it fills and the pattern Vosh reads it with.
//!
//! The patterns follow E section 6.4 with the fixes in section 3 of the
//! build spec. `%x` may be negative, `%s` is one word of letters and
//! apostrophes, `%p` and `%P` carry their brackets, `%K %k %E` fill the
//! percents, and `%G` is one of the eight region names. The group names
//! are the catalog's, so a capture feeds the resolver as it is.
//!
//! The breaks `%c` and `%C`, the color codes `%l` and `%L`, and `%%` are
//! not values, and the lexer reads them itself.

use super::Who;

/// A value code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Code {
    /// `%h`
    Hp,
    /// `%H`
    MaxHp,
    /// `%m`
    Mana,
    /// `%M`
    MaxMana,
    /// `%v`
    Move,
    /// `%V`
    MaxMove,
    /// `%K`
    HpPct,
    /// `%k`
    ManaPct,
    /// `%E`
    MovePct,
    /// `%a`
    Cp,
    /// `%A`
    Rp,
    /// `%g`
    Gold,
    /// `%x`
    Exp,
    /// `%X`
    Tnl,
    /// `%t`
    Hour,
    /// `%w`
    Temp,
    /// `%W`
    Weather,
    /// `%G`
    Region,
    /// `%s`
    Lang,
    /// `%S`
    Pos,
    /// `%i`
    Stallion,
    /// `%e`
    Exits,
    /// `%f` and a digit, slots 1 to 9 and `0` for slot 10.
    Slot(u8),
    /// `%j` and a digit. 1 to 3 name a moon, and any other digit prints
    /// `-`.
    Moon(u8),
    /// `%n`
    Tank,
    /// `%p`
    TankPct,
    /// `%P`
    TankBar,
    /// `%r`
    Room,
    /// `%R`
    RoomNum,
    /// `%z`
    Area,
    /// `%b`
    AreaNum,
    /// `%o`
    Olc,
    /// `%O`
    OlcVnum,
    /// `%u`
    Pacify,
}

/// `prompt_sky`'s words for `%W` (`comm.c:1692-1707`).
pub const SKY: [&str; 10] = [
    "indoors",
    "cloudless",
    "cloudy",
    "snowing",
    "sleeting",
    "rainy",
    "blizzard",
    "hailing",
    "lightning",
    "error!",
];

/// `region_table`'s names for `%G` (`tables.c:4335-4352`).
pub const REGIONS: [&str; 8] = [
    "Temperate",
    "Coastal North",
    "Coastal South",
    "Desert",
    "Tundra",
    "Mountain North",
    "Mountain South",
    "Mountain East",
];

/// `pos_abbrev` for `%S` (`comm.c:1904-1907`), with meditate printing
/// nothing.
pub const POSITIONS: [&str; 9] = [
    "dea", "mor", "inc", "stn", "slp", "rst", "sit", "fgt", "std",
];

/// The three letter phases `%j` prints, and `-` for a moon that is not
/// up (`comm.c:1851-1868`).
pub const PHASES: [&str; 9] = ["new", "wax", "Hwx", "Gwx", "FUL", "Gwn", "Hwn", "wan", "-"];

impl Code {
    /// The code a letter after `%` names, for every letter but `f` and
    /// `j`, which take a digit.
    pub fn from_letter(letter: char) -> Option<Self> {
        Some(match letter {
            'h' => Self::Hp,
            'H' => Self::MaxHp,
            'm' => Self::Mana,
            'M' => Self::MaxMana,
            'v' => Self::Move,
            'V' => Self::MaxMove,
            'K' => Self::HpPct,
            'k' => Self::ManaPct,
            'E' => Self::MovePct,
            'a' => Self::Cp,
            'A' => Self::Rp,
            'g' => Self::Gold,
            'x' => Self::Exp,
            'X' => Self::Tnl,
            't' => Self::Hour,
            'w' => Self::Temp,
            'W' => Self::Weather,
            'G' => Self::Region,
            's' => Self::Lang,
            'S' => Self::Pos,
            'i' => Self::Stallion,
            'e' => Self::Exits,
            'n' => Self::Tank,
            'p' => Self::TankPct,
            'P' => Self::TankBar,
            'r' => Self::Room,
            'R' => Self::RoomNum,
            'z' => Self::Area,
            'b' => Self::AreaNum,
            'o' => Self::Olc,
            'O' => Self::OlcVnum,
            'u' => Self::Pacify,
            _ => return None,
        })
    }

    /// `%f` or `%j` with its digit.
    pub fn with_digit(letter: char, digit: char) -> Option<Self> {
        let d = digit.to_digit(10)? as u8;
        match letter {
            'f' => Some(Self::Slot(d)),
            'j' => Some(Self::Moon(d)),
            _ => None,
        }
    }

    /// The code as you write it, such as `%h` or `%f1`.
    pub fn written(self) -> String {
        let letter = match self {
            Self::Hp => 'h',
            Self::MaxHp => 'H',
            Self::Mana => 'm',
            Self::MaxMana => 'M',
            Self::Move => 'v',
            Self::MaxMove => 'V',
            Self::HpPct => 'K',
            Self::ManaPct => 'k',
            Self::MovePct => 'E',
            Self::Cp => 'a',
            Self::Rp => 'A',
            Self::Gold => 'g',
            Self::Exp => 'x',
            Self::Tnl => 'X',
            Self::Hour => 't',
            Self::Temp => 'w',
            Self::Weather => 'W',
            Self::Region => 'G',
            Self::Lang => 's',
            Self::Pos => 'S',
            Self::Stallion => 'i',
            Self::Exits => 'e',
            Self::Slot(d) => return format!("%f{d}"),
            Self::Moon(d) => return format!("%j{d}"),
            Self::Tank => 'n',
            Self::TankPct => 'p',
            Self::TankBar => 'P',
            Self::Room => 'r',
            Self::RoomNum => 'R',
            Self::Area => 'z',
            Self::AreaNum => 'b',
            Self::Olc => 'o',
            Self::OlcVnum => 'O',
            Self::Pacify => 'u',
        };
        format!("%{letter}")
    }

    /// The field the code fills, which names its group in the pattern.
    /// None for a `%j` digit that names no moon, which always prints `-`.
    pub fn name(self) -> Option<&'static str> {
        const SLOTS: [&str; 10] = [
            "slot10", "slot1", "slot2", "slot3", "slot4", "slot5", "slot6", "slot7", "slot8",
            "slot9",
        ];
        Some(match self {
            Self::Hp => "hp",
            Self::MaxHp => "maxhp",
            Self::Mana => "mana",
            Self::MaxMana => "maxmana",
            Self::Move => "move",
            Self::MaxMove => "maxmove",
            Self::HpPct => "hp_pct",
            Self::ManaPct => "mana_pct",
            Self::MovePct => "move_pct",
            Self::Cp => "cp",
            Self::Rp => "rp",
            Self::Gold => "gold",
            Self::Exp => "exp",
            Self::Tnl => "tnl",
            Self::Hour => "hour",
            Self::Temp => "temp",
            Self::Weather => "weather",
            Self::Region => "region",
            Self::Lang => "lang",
            Self::Pos => "pos",
            Self::Stallion => "stallion",
            Self::Exits => "exits",
            Self::Slot(d) => SLOTS.get(usize::from(d))?,
            Self::Moon(1) => "moon1",
            Self::Moon(2) => "moon2",
            Self::Moon(3) => "moon3",
            Self::Moon(_) => return None,
            Self::Tank => "tank",
            Self::TankPct => "tank_pct",
            Self::TankBar => "tank_bar",
            Self::Room => "room",
            Self::RoomNum => "room_num",
            Self::Area => "area",
            Self::AreaNum => "area_num",
            Self::Olc => "olc",
            Self::OlcVnum => "olc_vnum",
            Self::Pacify => "pacify",
        })
    }

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

    /// `%n`, `%p` and `%P`, which print only while your opponent fights
    /// someone in your group.
    pub fn is_tank(self) -> bool {
        matches!(self, Self::Tank | Self::TankPct | Self::TankBar)
    }

    /// Whether Vosh can read what the code prints for you. `%u` for a
    /// mortal and `%s` while you control a mobile leave the game's
    /// buffer as it was, so they repeat the text of the code before them.
    pub fn readable(self, who: Who) -> bool {
        match self {
            Self::Pacify => who.immortal,
            Self::Lang => !who.mobile,
            _ => true,
        }
    }

    /// The pattern for what the code prints.
    pub fn pattern(self, who: Who) -> Pattern {
        if !self.readable(who) {
            return Pattern::inner(".*?");
        }
        match self {
            Self::Hp
            | Self::Mana
            | Self::Move
            | Self::HpPct
            | Self::ManaPct
            | Self::MovePct
            | Self::Cp
            | Self::Tnl
            | Self::Exp
            | Self::Temp => Pattern::inner(r"-?\d+"),
            Self::MaxHp | Self::MaxMana | Self::MaxMove | Self::Rp | Self::Gold => {
                Pattern::inner(r"\d+")
            }
            Self::Hour => Pattern::inner(r"\d{1,2}"),
            Self::RoomNum | Self::AreaNum | Self::OlcVnum => Pattern::inner(r"\d*"),
            Self::Weather => Pattern::words(&SKY),
            Self::Region => Pattern::words(&REGIONS),
            Self::Lang => Pattern::inner("[A-Za-z']+"),
            Self::Pos => Pattern::inner(format!("(?:{})?", POSITIONS.join("|"))),
            Self::Stallion => Pattern::inner("[MD]"),
            Self::Exits => Pattern {
                before: r"\[Exits:",
                inner: r"[^\]]*".into(),
                after: r"\]",
            },
            Self::Slot(_) => Pattern::inner(r"\d+|~|-"),
            Self::Moon(_) => Pattern::words(&PHASES),
            Self::Tank => Pattern {
                before: "",
                inner: ".+?".into(),
                after: ": ",
            },
            Self::TankPct => Pattern {
                before: r"\[",
                inner: r"-?\d+".into(),
                after: r"\]",
            },
            Self::TankBar => Pattern {
                before: r"\[",
                inner: r"[=-]{3}(?:\|[=-]{3}){3}".into(),
                after: r"\]",
            },
            Self::Room | Self::Area => Pattern::inner(".*?"),
            Self::Olc => Pattern::inner("[A-Za-z]*"),
            Self::Pacify => Pattern::inner("(?:pacified|not pacified)?"),
        }
    }

    /// The characters the code can start and end with, and whether its
    /// end is plain from what it printed, which decides when two codes
    /// with nothing between them run together.
    pub fn edges(self, who: Who) -> Edges {
        use chars::{ANY, APOS, BRACKET, DIGIT, LETTER, MINUS, OTHER, SPACE};
        let number = Edges::new(MINUS | DIGIT, DIGIT);
        let digits = Edges::new(DIGIT, DIGIT);
        if !self.readable(who) {
            return Edges::new(ANY, ANY).nullable();
        }
        match self {
            Self::Hp
            | Self::Mana
            | Self::Move
            | Self::HpPct
            | Self::ManaPct
            | Self::MovePct
            | Self::Cp
            | Self::Tnl
            | Self::Exp
            | Self::Temp => number,
            Self::MaxHp | Self::MaxMana | Self::MaxMove | Self::Rp | Self::Gold | Self::Hour => {
                digits
            }
            Self::RoomNum | Self::AreaNum | Self::OlcVnum => digits.nullable(),
            Self::Weather => Edges::new(LETTER, LETTER | OTHER).words(),
            Self::Region | Self::Stallion => Edges::new(LETTER, LETTER).words(),
            Self::Lang => Edges::new(LETTER | APOS, LETTER | APOS),
            Self::Pos | Self::Olc => Edges::new(LETTER, LETTER).nullable(),
            Self::Exits | Self::TankPct | Self::TankBar => Edges::new(BRACKET, BRACKET).bracketed(),
            Self::Slot(_) => Edges::new(DIGIT | MINUS | OTHER, DIGIT | MINUS | OTHER),
            Self::Moon(_) => Edges::new(LETTER | MINUS, LETTER | MINUS).words(),
            // The name ends at the first colon and space.
            Self::Tank => Edges::new(ANY, SPACE).words(),
            Self::Room | Self::Area => Edges::new(ANY, ANY).nullable(),
            Self::Pacify => Edges::new(LETTER, LETTER).words().nullable(),
        }
    }
}

/// The pattern for one code: fixed text before and after, and the part
/// the field's group holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    pub before: &'static str,
    pub inner: String,
    pub after: &'static str,
}

impl Pattern {
    fn inner(inner: impl Into<String>) -> Self {
        Self {
            before: "",
            inner: inner.into(),
            after: "",
        }
    }

    fn words(words: &[&str]) -> Self {
        let words: Vec<String> = words.iter().map(|w| regex::escape(w)).collect();
        Self::inner(words.join("|"))
    }

    /// The pattern with the inner part in the named group, or in a group
    /// that reads nothing when `name` is None.
    pub fn regex(&self, name: Option<&str>) -> String {
        let Self {
            before,
            inner,
            after,
        } = self;
        match name {
            Some(name) => format!("{before}(?<{name}>{inner}){after}"),
            None => format!("{before}(?:{inner}){after}"),
        }
    }
}

/// Sets of characters, as bits, for [`Edges`].
pub mod chars {
    pub const DIGIT: u8 = 1;
    pub const MINUS: u8 = 1 << 1;
    pub const LETTER: u8 = 1 << 2;
    pub const APOS: u8 = 1 << 3;
    pub const SPACE: u8 = 1 << 4;
    pub const BRACKET: u8 = 1 << 5;
    pub const OTHER: u8 = 1 << 6;
    pub const ANY: u8 = u8::MAX;
}

/// What the edges of a code's text can be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edges {
    /// The characters it can start with.
    pub first: u8,
    /// The characters it can end with.
    pub last: u8,
    /// No text it prints starts another, so where it ends is plain from
    /// the left.
    pub words: bool,
    /// It starts with a bracket and ends at the matching one.
    pub bracketed: bool,
    /// It can print nothing.
    pub nullable: bool,
}

impl Edges {
    const fn new(first: u8, last: u8) -> Self {
        Self {
            first,
            last,
            words: false,
            bracketed: false,
            nullable: false,
        }
    }

    const fn words(self) -> Self {
        Self {
            words: true,
            ..self
        }
    }

    const fn bracketed(self) -> Self {
        Self {
            words: true,
            bracketed: true,
            ..self
        }
    }

    /// The same edges for a code that can print nothing.
    #[must_use]
    pub const fn nullable(self) -> Self {
        Self {
            nullable: true,
            ..self
        }
    }

    /// True when Vosh cannot tell where `self` ends and `next` begins
    /// with nothing between them.
    pub fn runs_into(self, next: Self) -> bool {
        !self.words && !next.bracketed && self.last & next.first != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    /// Every letter `bust_a_prompt`'s switch reads as a value, `f` and
    /// `j` with a digit.
    const LETTERS: &str = "hHmMvVKkEaAgxXtwWGsSienpPrRzboOu";

    fn every_code() -> Vec<Code> {
        let mut codes: Vec<Code> = LETTERS.chars().filter_map(Code::from_letter).collect();
        for d in '0'..='9' {
            codes.extend(Code::with_digit('f', d));
            codes.extend(Code::with_digit('j', d));
        }
        codes
    }

    #[test]
    fn every_value_letter_is_a_code_that_writes_back_the_same() {
        for letter in LETTERS.chars() {
            let code = Code::from_letter(letter).unwrap_or_else(|| panic!("%{letter}"));
            assert_eq!(code.written(), format!("%{letter}"));
        }
        for letter in ['f', 'j'] {
            assert_eq!(Code::from_letter(letter), None, "%{letter} takes a digit");
            for d in '0'..='9' {
                let code = Code::with_digit(letter, d).unwrap();
                assert_eq!(code.written(), format!("%{letter}{d}"));
            }
            assert_eq!(Code::with_digit(letter, 'x'), None);
        }
        // Breaks, colors and anything else are not values.
        for letter in ['c', 'C', 'l', 'L', '%', 'q', 'y', 'Z', ' '] {
            assert_eq!(Code::from_letter(letter), None, "%{letter}");
        }
        assert_eq!(every_code().len(), LETTERS.len() + 20);
    }

    #[test]
    fn names_are_the_catalogs_and_each_is_its_own() {
        let mut seen = std::collections::BTreeSet::new();
        for code in every_code() {
            let Some(name) = code.name() else {
                assert!(matches!(code, Code::Moon(d) if !(1..=3).contains(&d)));
                continue;
            };
            assert!(seen.insert(name), "{name} twice");
            let percent = ["hp_pct", "mana_pct", "move_pct", "tank_pct", "tank_bar"];
            assert!(
                crate::values::entry(name).is_some() || percent.contains(&name),
                "{name} is no catalog field"
            );
            // A moved pattern that fills a name Vosh does not know keeps
            // its pattern, since no code ever fills that name.
            assert!(crate::values::known(name), "{name} is known");
        }
        assert_eq!(Code::Slot(0).name(), Some("slot10"));
        assert_eq!(Code::Slot(1).name(), Some("slot1"));
    }

    #[test]
    fn labels_are_the_catalogs() {
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
        for code in every_code() {
            if let Some(entry) = code.name().and_then(crate::values::entry) {
                assert_eq!(code.label(), entry.label, "{}", code.written());
            }
        }
    }

    fn reads(code: Code, who: Who, text: &str) -> Option<String> {
        let re = Regex::new(&format!("^{}$", code.pattern(who).regex(Some("v")))).unwrap();
        re.captures(text).map(|c| c["v"].to_string())
    }

    #[test]
    fn each_pattern_reads_what_its_code_prints() {
        let who = Who::default();
        let cases: &[(Code, &str, &str)] = &[
            (Code::Hp, "-12", "-12"),
            (Code::MaxHp, "1020", "1020"),
            (Code::HpPct, "-5", "-5"),
            (Code::Exp, "-40", "-40"),
            (Code::Hour, "23", "23"),
            (Code::Temp, "-3", "-3"),
            (Code::Weather, "error!", "error!"),
            (Code::Region, "Mountain East", "Mountain East"),
            (Code::Lang, "thsu'ul", "thsu'ul"),
            (Code::Pos, "", ""),
            (Code::Pos, "fgt", "fgt"),
            (Code::Stallion, "M", "M"),
            (Code::Exits, "[Exits: N (E) S]", " N (E) S"),
            (Code::Exits, "[Exits: --- ]", " --- "),
            (Code::Slot(1), "~", "~"),
            (Code::Slot(1), "-", "-"),
            (Code::Slot(1), "14", "14"),
            (Code::Moon(1), "Hwx", "Hwx"),
            (Code::Moon(9), "-", "-"),
            (Code::Tank, "Tester: ", "Tester"),
            (Code::TankPct, "[24]", "24"),
            (Code::TankBar, "[===|---|---|---]", "===|---|---|---"),
            (Code::Room, "", ""),
            (Code::RoomNum, "3001", "3001"),
            (Code::Olc, "MPEdit", "MPEdit"),
            (Code::OlcVnum, "", ""),
        ];
        for (code, printed, value) in cases {
            assert_eq!(
                reads(*code, who, printed).as_deref(),
                Some(*value),
                "{} on {printed:?}",
                code.written()
            );
        }
        for (code, printed) in [
            (Code::MaxHp, "-1"),
            (Code::Hour, "123"),
            (Code::Weather, "sunny"),
            (Code::Region, "Mountain West"),
            (Code::Lang, "two words"),
            (Code::Pos, "med"),
            (Code::Stallion, "X"),
            (Code::Slot(1), "+"),
            (Code::TankBar, "[====|---|---]"),
        ] {
            assert_eq!(reads(code, who, printed), None, "{}", code.written());
        }
    }

    #[test]
    fn pacify_reads_for_an_immortal_and_nothing_for_anyone_else() {
        let immortal = Who {
            immortal: true,
            ..Who::default()
        };
        assert!(Code::Pacify.readable(immortal));
        assert_eq!(
            reads(Code::Pacify, immortal, "not pacified").as_deref(),
            Some("not pacified")
        );
        assert!(!Code::Pacify.readable(Who::default()));
        assert_eq!(Code::Pacify.pattern(Who::default()).inner, ".*?");
        let mobile = Who {
            mobile: true,
            ..Who::default()
        };
        assert!(!Code::Lang.readable(mobile));
        assert!(Code::Lang.readable(Who::default()));
    }

    #[test]
    fn a_pattern_with_no_name_reads_nothing() {
        let bar = Code::TankBar.pattern(Who::default());
        assert_eq!(
            bar.regex(Some("tank_bar")),
            r"\[(?<tank_bar>[=-]{3}(?:\|[=-]{3}){3})\]"
        );
        assert_eq!(bar.regex(None), r"\[(?:[=-]{3}(?:\|[=-]{3}){3})\]");
        assert_eq!(
            Code::Tank.pattern(Who::default()).regex(Some("tank")),
            "(?<tank>.+?): "
        );
    }

    #[test]
    fn codes_run_together_only_where_their_edges_meet() {
        let who = Who::default();
        let runs = |a: Code, b: Code| a.edges(who).runs_into(b.edges(who));
        assert!(runs(Code::Hp, Code::Mana));
        assert!(runs(Code::Room, Code::Area));
        assert!(runs(Code::Pos, Code::Lang));
        assert!(runs(Code::Slot(1), Code::Slot(2)));
        assert!(!runs(Code::Hp, Code::Pos));
        assert!(!runs(Code::Stallion, Code::Lang));
        assert!(!runs(Code::Tank, Code::TankBar));
        assert!(!runs(Code::Tank, Code::Hp));
        assert!(!runs(Code::Moon(1), Code::Moon(2)));
        assert!(!runs(Code::Hour, Code::Moon(1)));
        assert!(!runs(Code::Room, Code::Exits));
    }
}

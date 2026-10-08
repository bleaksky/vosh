//! The catalog of every field Vosh knows, with its kind, group, sources
//! and sample, and the reading of a field reference against it.

use chrono::NaiveDateTime;
use serde::Serialize;

use super::samples::value_of;
use crate::design::FieldRef;
use crate::values::format::Resolved;
use crate::values::gmcp::{
    self, CHAR_COMBAT, CHAR_STATE, CHAR_STATUS, CHAR_VITALS, CHAR_WORTH, GROUP_INFO, IMM_QUEUES,
    ROOM_CHARS, ROOM_INFO, ROOM_ITEMS, ROOM_WEATHER, WORLD_MOONS, WORLD_TIME,
};

// ---------------------------------------------------------------------
// The catalog
// ---------------------------------------------------------------------

/// What a field holds, which decides the formats it offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A number with a max, such as health.
    Gauge,
    /// A whole number.
    Num,
    /// A percent, 0 to 100.
    Pct,
    /// The tank's health, a percent with the game's `%P` bar.
    TankPct,
    Text,
    /// On or off. Off is Absent.
    Flag,
    /// A count of names. Zero is Absent.
    Count,
    Position,
    Lang,
    Moon,
    Exits,
    /// An immortal level, `(Wizi 60)`.
    Level,
    /// A `%f` affect slot.
    Slot,
    Hour,
    Temp,
    Seconds,
    Clock,
    Date,
    /// Ticks left on an affect.
    Ticks,
    /// A group member and their health.
    Member,
    /// The game's prompt with its colors.
    Raw,
}

/// A format a field offers in the picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatId {
    Value,
    Max,
    Pct,
    PctGame,
    Bar,
    Game,
    Word,
    Ampm,
    Name,
    Grouped,
    Short,
    Thousands,
    Unit,
    Since,
    Trunc,
    Hm,
    Hms,
    Md,
    Count,
    Names,
    On,
    Off,
}

impl Kind {
    /// The formats a field of this kind offers, the value first.
    pub fn formats(self) -> &'static [FormatId] {
        use FormatId as F;
        match self {
            Kind::Gauge => &[
                F::Value,
                F::Max,
                F::Pct,
                F::PctGame,
                F::Bar,
                F::Grouped,
                F::Short,
                F::Thousands,
            ],
            Kind::Num => &[F::Value, F::Grouped, F::Short, F::Thousands],
            Kind::Pct => &[F::Value, F::Pct, F::Bar],
            Kind::TankPct => &[F::Value, F::Pct, F::Bar, F::Game],
            Kind::Text | Kind::Raw => &[F::Value, F::Trunc],
            Kind::Flag => &[F::On, F::Off],
            Kind::Count => &[F::Count, F::Names],
            Kind::Position => &[F::Value, F::Word, F::Game],
            Kind::Lang | Kind::Exits | Kind::Level | Kind::Slot => &[F::Value, F::Game],
            Kind::Moon => &[F::Value, F::Game, F::Word, F::Name],
            Kind::Hour => &[F::Value, F::Word, F::Ampm],
            Kind::Temp => &[F::Value, F::Unit],
            Kind::Seconds => &[F::Value, F::Unit, F::Since, F::Bar],
            Kind::Clock => &[F::Hm, F::Hms],
            Kind::Date => &[F::Md, F::Value],
            Kind::Ticks => &[F::Value, F::On, F::Off],
            Kind::Member => &[F::Value, F::Name, F::Pct, F::Bar],
        }
    }
}

/// The picker's groups, in the order it lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Vitals,
    Fight,
    Group,
    Character,
    Worth,
    Affects,
    Room,
    TimeAndSky,
    Vosh,
    /// Names only your scripts set.
    Scripts,
    Building,
    More,
}

/// One field Vosh knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Entry {
    /// The token, `[a-z0-9_]`.
    pub name: &'static str,
    /// Other names that read the same field, such as `mhp` or `tar`.
    pub aliases: &'static [&'static str],
    pub label: &'static str,
    pub kind: Kind,
    pub group: Group,
    /// The package that feeds it, for "not sent yet".
    pub package: Option<&'static str>,
    /// Only the new server build sends that package.
    pub new_build: bool,
    /// The PROMPT codes that feed it.
    pub codes: &'static [&'static str],
    /// A sample for previews with no live value, in the form the
    /// catalog's kind reads. Empty is Absent.
    pub sample: &'static str,
    /// More words the picker searches, next to the label and the codes.
    pub search: &'static [&'static str],
    /// Written with a parameter, `%{aff:sanctuary}`.
    pub param: bool,
    /// Shown as its own row in the picker. The max spellings read as
    /// part of their gauge.
    pub listed: bool,
}

const BASE: Entry = Entry {
    name: "",
    aliases: &[],
    label: "",
    kind: Kind::Text,
    group: Group::More,
    package: None,
    new_build: false,
    codes: &[],
    sample: "",
    search: &[],
    param: false,
    listed: true,
};

/// Every field Vosh knows, in the picker's order. The comment in an
/// entry names the GMCP package and fields its value comes from.
pub static CATALOG: &[Entry] = &[
    // Vitals
    Entry {
        name: "hp",
        label: "Health",
        kind: Kind::Gauge,
        group: Group::Vitals,
        // Char.Vitals hp maxhp
        package: Some(CHAR_VITALS),
        codes: &["%h", "%H", "%K"],
        sample: "1020/1020",
        search: &["hp", "hit points"],
        ..BASE
    },
    Entry {
        name: "maxhp",
        aliases: &["mhp", "hp_max", "max_hp"],
        label: "Max health",
        kind: Kind::Num,
        group: Group::Vitals,
        // Char.Vitals maxhp
        package: Some(CHAR_VITALS),
        codes: &["%H"],
        sample: "1020",
        listed: false,
        ..BASE
    },
    Entry {
        name: "mana",
        label: "Mana",
        kind: Kind::Gauge,
        group: Group::Vitals,
        // Char.Vitals mana maxmana
        package: Some(CHAR_VITALS),
        codes: &["%m", "%M", "%k"],
        sample: "800/800",
        search: &["sp", "mp"],
        ..BASE
    },
    Entry {
        name: "maxmana",
        aliases: &["mmana", "mana_max", "max_mana"],
        label: "Max mana",
        kind: Kind::Num,
        group: Group::Vitals,
        // Char.Vitals maxmana
        package: Some(CHAR_VITALS),
        codes: &["%M"],
        sample: "800",
        listed: false,
        ..BASE
    },
    Entry {
        name: "move",
        label: "Moves",
        kind: Kind::Gauge,
        group: Group::Vitals,
        // Char.Vitals move maxmove
        package: Some(CHAR_VITALS),
        codes: &["%v", "%V", "%E"],
        sample: "930/930",
        search: &["mv", "ep", "stamina"],
        ..BASE
    },
    Entry {
        name: "maxmove",
        aliases: &["mmove", "move_max", "max_move"],
        label: "Max moves",
        kind: Kind::Num,
        group: Group::Vitals,
        // Char.Vitals maxmove
        package: Some(CHAR_VITALS),
        codes: &["%V"],
        sample: "930",
        listed: false,
        ..BASE
    },
    // Fight
    Entry {
        name: "fight",
        label: "In a fight",
        kind: Kind::Flag,
        group: Group::Fight,
        // Char.Combat target
        package: Some(CHAR_COMBAT),
        sample: "1",
        search: &["combat", "fighting"],
        ..BASE
    },
    Entry {
        name: "opponent",
        label: "Opponent",
        kind: Kind::Text,
        group: Group::Fight,
        // Char.Combat target
        package: Some(CHAR_COMBAT),
        sample: "Blackwatch Guard",
        search: &["enemy", "foe", "victim"],
        ..BASE
    },
    Entry {
        name: "opponent_hp",
        label: "Opponent health",
        kind: Kind::Pct,
        group: Group::Fight,
        // Char.Combat hp_pct
        package: Some(CHAR_COMBAT),
        sample: "60",
        search: &["enemy", "foe"],
        ..BASE
    },
    Entry {
        name: "opponent_cond",
        label: "Opponent condition",
        kind: Kind::Text,
        group: Group::Fight,
        // Char.Combat condition
        package: Some(CHAR_COMBAT),
        sample: "quite a few wounds",
        search: &["enemy", "foe", "wounds"],
        ..BASE
    },
    Entry {
        name: "tank",
        label: "Tank",
        kind: Kind::Text,
        group: Group::Fight,
        // Char.Combat tank.name
        package: Some(CHAR_COMBAT),
        new_build: true,
        codes: &["%n"],
        sample: "Brask",
        ..BASE
    },
    Entry {
        name: "tank_hp",
        label: "Tank health",
        kind: Kind::TankPct,
        group: Group::Fight,
        // Char.Combat tank.hp_pct
        package: Some(CHAR_COMBAT),
        new_build: true,
        codes: &["%p", "%P"],
        sample: "78",
        ..BASE
    },
    Entry {
        name: "pos",
        label: "Position",
        kind: Kind::Position,
        group: Group::Fight,
        // Char.State position
        package: Some(CHAR_STATE),
        new_build: true,
        codes: &["%S"],
        sample: "standing",
        search: &["position", "standing", "sitting", "resting", "sleeping"],
        ..BASE
    },
    // Group
    Entry {
        name: "leader",
        label: "Leader",
        group: Group::Group,
        // Group.Info leader
        package: Some(GROUP_INFO),
        sample: "Ketterly",
        ..BASE
    },
    Entry {
        name: "group_size",
        label: "Group size",
        kind: Kind::Count,
        group: Group::Group,
        // Group.Info members
        package: Some(GROUP_INFO),
        sample: "Ketterly,a loyal wolf",
        search: &["members"],
        ..BASE
    },
    Entry {
        name: "group_low",
        label: "Lowest health",
        kind: Kind::Member,
        group: Group::Group,
        // Group.Info members
        package: Some(GROUP_INFO),
        sample: "Iskra 45",
        search: &["group", "member", "weakest"],
        ..BASE
    },
    Entry {
        name: "member_hp",
        label: "A member's health",
        kind: Kind::Pct,
        group: Group::Group,
        // Group.Info members[].hp_pct
        package: Some(GROUP_INFO),
        sample: "78",
        param: true,
        ..BASE
    },
    Entry {
        name: "member_mana",
        label: "A member's mana",
        kind: Kind::Pct,
        group: Group::Group,
        // Group.Info members[].mana_pct
        package: Some(GROUP_INFO),
        sample: "55",
        param: true,
        ..BASE
    },
    Entry {
        name: "member_move",
        label: "A member's moves",
        kind: Kind::Pct,
        group: Group::Group,
        // Group.Info members[].move_pct
        package: Some(GROUP_INFO),
        sample: "93",
        param: true,
        ..BASE
    },
    Entry {
        name: "member_level",
        label: "A member's level",
        kind: Kind::Num,
        group: Group::Group,
        // Group.Info members[].level
        package: Some(GROUP_INFO),
        sample: "50",
        param: true,
        ..BASE
    },
    Entry {
        name: "member_class",
        label: "A member's class",
        group: Group::Group,
        // Group.Info members[].class
        package: Some(GROUP_INFO),
        sample: "Dkn",
        param: true,
        ..BASE
    },
    Entry {
        name: "member_tnl",
        label: "A member's experience to level",
        kind: Kind::Num,
        group: Group::Group,
        // Group.Info members[].tnl
        package: Some(GROUP_INFO),
        sample: "1250",
        param: true,
        search: &["tnl"],
        ..BASE
    },
    // Character
    Entry {
        name: "name",
        label: "Name",
        group: Group::Character,
        // Char.Status name
        package: Some(CHAR_STATUS),
        sample: "Wystan",
        ..BASE
    },
    Entry {
        name: "level",
        label: "Level",
        kind: Kind::Num,
        group: Group::Character,
        // Char.Status level
        package: Some(CHAR_STATUS),
        sample: "50",
        ..BASE
    },
    Entry {
        name: "race",
        label: "Race",
        group: Group::Character,
        // Char.Status race
        package: Some(CHAR_STATUS),
        sample: "human",
        ..BASE
    },
    Entry {
        name: "class",
        label: "Class",
        group: Group::Character,
        // Char.Status class
        package: Some(CHAR_STATUS),
        sample: "warrior",
        ..BASE
    },
    Entry {
        name: "lang",
        label: "Language",
        kind: Kind::Lang,
        group: Group::Character,
        // Char.State language
        package: Some(CHAR_STATE),
        new_build: true,
        codes: &["%s"],
        sample: "common",
        search: &["language", "speaking"],
        ..BASE
    },
    Entry {
        name: "stallion",
        label: "Stallion",
        group: Group::Character,
        codes: &["%i"],
        sample: "M",
        search: &["mount", "horse"],
        ..BASE
    },
    Entry {
        name: "wizi",
        label: "Wizi",
        kind: Kind::Level,
        group: Group::Character,
        sample: "60",
        search: &["invisible", "immortal"],
        ..BASE
    },
    Entry {
        name: "incog",
        label: "Incog",
        kind: Kind::Level,
        group: Group::Character,
        sample: "60",
        search: &["incognito", "immortal"],
        ..BASE
    },
    Entry {
        name: "afk",
        label: "Away",
        kind: Kind::Flag,
        group: Group::Character,
        sample: "1",
        search: &["afk"],
        ..BASE
    },
    // Worth
    Entry {
        name: "gold",
        label: "Gold",
        kind: Kind::Num,
        group: Group::Worth,
        // Char.Worth gold
        package: Some(CHAR_WORTH),
        codes: &["%g"],
        sample: "1250",
        search: &["gp", "coins", "money"],
        ..BASE
    },
    Entry {
        name: "bank",
        label: "Bank",
        kind: Kind::Num,
        group: Group::Worth,
        // Char.Worth bank
        package: Some(CHAR_WORTH),
        sample: "5000",
        search: &["gold", "coins"],
        ..BASE
    },
    Entry {
        name: "exp",
        label: "Experience",
        kind: Kind::Num,
        group: Group::Worth,
        // Char.Worth exp
        package: Some(CHAR_WORTH),
        codes: &["%x"],
        sample: "125000",
        search: &["xp"],
        ..BASE
    },
    Entry {
        name: "tnl",
        label: "To next level",
        kind: Kind::Num,
        group: Group::Worth,
        // Char.Worth tnl
        package: Some(CHAR_WORTH),
        codes: &["%X"],
        sample: "1250",
        search: &["tnl", "xp", "experience"],
        ..BASE
    },
    Entry {
        name: "trains",
        label: "Trains",
        kind: Kind::Num,
        group: Group::Worth,
        // Char.Worth trains
        package: Some(CHAR_WORTH),
        sample: "3",
        ..BASE
    },
    Entry {
        name: "pracs",
        label: "Practices",
        kind: Kind::Num,
        group: Group::Worth,
        // Char.Worth practices
        package: Some(CHAR_WORTH),
        sample: "12",
        ..BASE
    },
    Entry {
        name: "cp",
        label: "Cabal points",
        kind: Kind::Num,
        group: Group::Worth,
        // Char.Worth cps
        package: Some(CHAR_WORTH),
        codes: &["%a"],
        sample: "40",
        search: &["cps"],
        ..BASE
    },
    Entry {
        name: "rp",
        label: "RP points",
        kind: Kind::Num,
        group: Group::Worth,
        // Char.Worth rps
        package: Some(CHAR_WORTH),
        codes: &["%A"],
        sample: "7",
        search: &["rps", "roleplay"],
        ..BASE
    },
    Entry {
        name: "cabal",
        label: "Cabal",
        group: Group::Worth,
        // Char.Worth cabal
        package: Some(CHAR_WORTH),
        sample: "Nexus",
        ..BASE
    },
    // Affects
    Entry {
        name: "missing",
        label: "Tracked affects missing",
        kind: Kind::Count,
        group: Group::Affects,
        // Char.Affects
        package: Some(gmcp::CHAR_AFFECTS),
        sample: "sanctuary,haste",
        search: &["tracked", "spells", "buffs"],
        ..BASE
    },
    Entry {
        name: "aff",
        label: "An affect",
        kind: Kind::Ticks,
        group: Group::Affects,
        // Char.Affects
        package: Some(gmcp::CHAR_AFFECTS),
        sample: "12",
        search: &["spell", "buff", "duration"],
        param: true,
        ..BASE
    },
    slot("slot1", "Affect slot 1", &["%f1"]),
    slot("slot2", "Affect slot 2", &["%f2"]),
    slot("slot3", "Affect slot 3", &["%f3"]),
    slot("slot4", "Affect slot 4", &["%f4"]),
    slot("slot5", "Affect slot 5", &["%f5"]),
    slot("slot6", "Affect slot 6", &["%f6"]),
    slot("slot7", "Affect slot 7", &["%f7"]),
    slot("slot8", "Affect slot 8", &["%f8"]),
    slot("slot9", "Affect slot 9", &["%f9"]),
    slot("slot10", "Affect slot 10", &["%f0"]),
    // Room
    Entry {
        name: "room",
        label: "Room",
        group: Group::Room,
        // Room.Info name
        package: Some(ROOM_INFO),
        codes: &["%r"],
        sample: "The Bank of Aabahran",
        ..BASE
    },
    Entry {
        name: "room_num",
        label: "Room number",
        kind: Kind::Num,
        group: Group::Room,
        // Room.Info num
        package: Some(ROOM_INFO),
        codes: &["%R"],
        sample: "5279",
        search: &["vnum"],
        ..BASE
    },
    Entry {
        name: "area",
        label: "Area",
        group: Group::Room,
        // Room.Info area
        package: Some(ROOM_INFO),
        codes: &["%z"],
        sample: "Fort Blackwatch",
        search: &["zone"],
        ..BASE
    },
    Entry {
        name: "area_num",
        label: "Area number",
        kind: Kind::Num,
        group: Group::Room,
        codes: &["%b"],
        sample: "12",
        search: &["vnum", "zone"],
        ..BASE
    },
    Entry {
        name: "exits",
        label: "Exits",
        kind: Kind::Exits,
        group: Group::Room,
        // Room.Info exits
        package: Some(ROOM_INFO),
        new_build: true,
        codes: &["%e"],
        sample: "S",
        search: &["directions", "doors"],
        ..BASE
    },
    Entry {
        name: "terrain",
        label: "Terrain",
        group: Group::Room,
        // Room.Info terrain
        package: Some(ROOM_INFO),
        sample: "inside",
        search: &["sector"],
        ..BASE
    },
    Entry {
        name: "region",
        aliases: &["climate"],
        label: "Region",
        group: Group::Room,
        // Room.Weather region, then Room.Info climate
        package: Some(ROOM_WEATHER),
        new_build: true,
        codes: &["%G"],
        sample: "Temperate",
        search: &["climate"],
        ..BASE
    },
    Entry {
        name: "sector",
        label: "Sector",
        kind: Kind::Num,
        group: Group::Room,
        // Room.Info sector
        package: Some(ROOM_INFO),
        sample: "0",
        search: &["terrain"],
        ..BASE
    },
    Entry {
        name: "region_num",
        label: "Region number",
        kind: Kind::Num,
        group: Group::Room,
        // Room.Info region
        package: Some(ROOM_INFO),
        sample: "0",
        search: &["climate"],
        ..BASE
    },
    Entry {
        name: "temp",
        label: "Temperature",
        kind: Kind::Temp,
        group: Group::Room,
        // Room.Weather temp unit
        package: Some(ROOM_WEATHER),
        new_build: true,
        codes: &["%w"],
        sample: "61F",
        search: &["weather", "degrees"],
        ..BASE
    },
    Entry {
        name: "weather",
        label: "Weather",
        group: Group::Room,
        // Room.Weather sky
        package: Some(ROOM_WEATHER),
        new_build: true,
        codes: &["%W"],
        sample: "cloudless",
        search: &["sky", "rain"],
        ..BASE
    },
    Entry {
        name: "people",
        label: "People here",
        kind: Kind::Count,
        group: Group::Room,
        // Room.Chars
        package: Some(ROOM_CHARS),
        sample: "a Blackwatch guard,a loyal wolf",
        search: &["mobs", "characters"],
        ..BASE
    },
    Entry {
        name: "things",
        label: "Things here",
        kind: Kind::Count,
        group: Group::Room,
        // Room.Items
        package: Some(ROOM_ITEMS),
        sample: "a wooden torch",
        search: &["items", "objects"],
        ..BASE
    },
    // Time and sky
    Entry {
        name: "hour",
        label: "Game hour",
        kind: Kind::Hour,
        group: Group::TimeAndSky,
        // World.Time hour
        package: Some(WORLD_TIME),
        codes: &["%t"],
        sample: "14",
        search: &["time", "clock"],
        ..BASE
    },
    Entry {
        name: "day",
        label: "Day",
        kind: Kind::Num,
        group: Group::TimeAndSky,
        // World.Time day
        package: Some(WORLD_TIME),
        sample: "12",
        search: &["date"],
        ..BASE
    },
    Entry {
        name: "month",
        label: "Month",
        kind: Kind::Num,
        group: Group::TimeAndSky,
        // World.Time month
        package: Some(WORLD_TIME),
        sample: "3",
        search: &["date"],
        ..BASE
    },
    Entry {
        name: "year",
        label: "Year",
        kind: Kind::Num,
        group: Group::TimeAndSky,
        // World.Time year
        package: Some(WORLD_TIME),
        sample: "812",
        search: &["date"],
        ..BASE
    },
    Entry {
        name: "sun",
        label: "Sunlight",
        group: Group::TimeAndSky,
        // World.Time sunlight
        package: Some(WORLD_TIME),
        sample: "light",
        search: &["day", "night", "dark"],
        ..BASE
    },
    Entry {
        name: "sky",
        label: "Sky",
        group: Group::TimeAndSky,
        // World.Time sky
        package: Some(WORLD_TIME),
        sample: "cloudless",
        search: &["weather"],
        ..BASE
    },
    moon("moon1", "Lysenties", &["%j1"], "4"),
    moon("moon2", "Nercuros", &["%j2"], "2"),
    moon("moon3", "Dyphrities", &["%j3"], "6"),
    Entry {
        name: "eclipse",
        label: "Eclipse",
        kind: Kind::Flag,
        group: Group::TimeAndSky,
        // World.Moons eclipse
        package: Some(WORLD_MOONS),
        sample: "1",
        search: &["moons"],
        ..BASE
    },
    Entry {
        name: "triad",
        label: "Triad",
        kind: Kind::Flag,
        group: Group::TimeAndSky,
        // World.Moons triad
        package: Some(WORLD_MOONS),
        sample: "1",
        search: &["moons"],
        ..BASE
    },
    Entry {
        name: "near",
        label: "Moons near alignment",
        kind: Kind::Flag,
        group: Group::TimeAndSky,
        // World.Moons near_alignment
        package: Some(WORLD_MOONS),
        sample: "1",
        search: &["moons", "alignment"],
        ..BASE
    },
    // Vosh
    Entry {
        name: "tick",
        label: "Tick",
        kind: Kind::Seconds,
        group: Group::Vosh,
        sample: "14/60",
        search: &["timer", "seconds"],
        ..BASE
    },
    Entry {
        name: "time",
        label: "Clock",
        kind: Kind::Clock,
        group: Group::Vosh,
        search: &["time"],
        ..BASE
    },
    Entry {
        name: "date",
        label: "Date",
        kind: Kind::Date,
        group: Group::Vosh,
        search: &["day"],
        ..BASE
    },
    Entry {
        name: "target",
        aliases: &["tar"],
        label: "Your target",
        group: Group::Vosh,
        sample: "a Blackwatch guard",
        search: &["tar"],
        ..BASE
    },
    Entry {
        name: "profile",
        label: "Profile",
        group: Group::Vosh,
        sample: "Default",
        ..BASE
    },
    Entry {
        name: "raw",
        label: "The game's prompt",
        kind: Kind::Raw,
        group: Group::Vosh,
        sample: "[1020/1020hp 800/800mn 930/930mv]",
        search: &["prompt", "raw"],
        ..BASE
    },
    // Building
    Entry {
        name: "olc",
        label: "OLC editor",
        group: Group::Building,
        codes: &["%o"],
        sample: "REdit",
        search: &["edit"],
        ..BASE
    },
    Entry {
        name: "olc_vnum",
        label: "Editing vnum",
        kind: Kind::Num,
        group: Group::Building,
        codes: &["%O"],
        sample: "5279",
        search: &["vnum", "edit"],
        ..BASE
    },
    Entry {
        name: "pacify",
        label: "Pacify",
        group: Group::Building,
        codes: &["%u"],
        sample: "not pacified",
        ..BASE
    },
    Entry {
        name: "queue",
        label: "A queue",
        kind: Kind::Num,
        group: Group::Building,
        // Imm.Queues
        package: Some(IMM_QUEUES),
        sample: "3",
        search: &["bugs", "notes", "applications"],
        param: true,
        ..BASE
    },
    // More from the game
    Entry {
        name: "gmcp",
        label: "More from the game",
        group: Group::More,
        search: &["package", "path"],
        param: true,
        ..BASE
    },
];

const fn slot(name: &'static str, label: &'static str, code: &'static [&'static str]) -> Entry {
    Entry {
        name,
        label,
        kind: Kind::Slot,
        group: Group::Affects,
        codes: code,
        sample: "12",
        search: &["slot", "timer"],
        ..BASE
    }
}

const fn moon(
    name: &'static str,
    label: &'static str,
    code: &'static [&'static str],
    sample: &'static str,
) -> Entry {
    Entry {
        name,
        label,
        kind: Kind::Moon,
        group: Group::TimeAndSky,
        // World.Moons moons
        package: Some(WORLD_MOONS),
        codes: code,
        sample,
        search: &["moon", "phase"],
        ..BASE
    }
}

/// The catalog entry for a name or one of its aliases. Fields written
/// with a parameter are found only by `field`.
pub fn entry(name: &str) -> Option<&'static Entry> {
    CATALOG
        .iter()
        .find(|e| !e.param && (e.name == name || e.aliases.contains(&name)))
}

/// The catalog entry for a field as a template reads it: a name or an
/// alias, or a field written with a parameter, `aff:sanctuary` among
/// them. None for a name only scripts set.
pub(crate) fn entry_for(field: &FieldRef) -> Option<&'static Entry> {
    match &field.param {
        Some(_) => CATALOG.iter().find(|e| e.param && e.name == field.name),
        None => entry(&field.name),
    }
}

/// Names the capture fills that are no field of their own: the percents
/// of `%K %k %E` and the tank's health from `%p` and `%P`.
const CAPTURE_KEYS: [&str; 5] = ["hp_pct", "mana_pct", "move_pct", "tank_pct", "tank_bar"];

/// True for a name a capture or GMCP also supplies, so a script value for
/// it lasts one pulse. Every catalog field has one of them but Vosh's own
/// (the tick, your target, the clock and the profile), whose script value
/// lasts until a script changes it, as any other name's does. The raw
/// prompt comes from the capture.
pub fn is_sourced(name: &str) -> bool {
    CAPTURE_KEYS.contains(&name)
        || entry(name).is_some_and(|e| e.group != Group::Vosh || e.kind == Kind::Raw)
}

/// True for a name Vosh knows: a catalog field, another spelling of one,
/// or one of the percents and tank health a capture fills. Every name a
/// PROMPT code fills is one of them, so a value under any other name
/// comes only from a pattern or a script.
pub fn known(name: &str) -> bool {
    CAPTURE_KEYS.contains(&name) || entry(name).is_some()
}

/// Where a field's value comes from now, for the picker's source line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// `mud.set_prompt_var`.
    Script,
    /// Your prompt as Vosh read it.
    Capture,
    /// The latest packet of its package.
    Gmcp,
    /// Vosh itself: the tick, the clock, your target, the profile.
    Vosh,
}

/// The names a prompt value for `e` may sit under: its own, its aliases,
/// and for a vital its max and percent, for the tank's health what `%p`
/// and `%P` fill.
pub(super) fn capture_keys(e: &Entry) -> Vec<&'static str> {
    let mut keys = vec![e.name];
    keys.extend(e.aliases.iter().copied());
    if let Some(pair) = Pair::of(e.name).filter(|p| p.cur() == e.name) {
        keys.push(pair.pct());
    }
    if e.name == "tank_hp" {
        keys.extend(["tank_pct", "tank_bar"]);
    }
    keys
}

/// The field a name the capture fills feeds, `hp` for `hp_pct`.
pub(crate) fn feeds(name: &str) -> &str {
    match name {
        "hp_pct" => "hp",
        "mana_pct" => "mana",
        "move_pct" => "move",
        "tank_pct" | "tank_bar" => "tank_hp",
        other => other,
    }
}

/// A vital pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pair {
    Hp,
    Mana,
    Move,
}

impl Pair {
    pub(crate) const ALL: [Pair; 3] = [Pair::Hp, Pair::Mana, Pair::Move];

    /// The current value's name.
    pub(crate) fn cur(self) -> &'static str {
        match self {
            Pair::Hp => "hp",
            Pair::Mana => "mana",
            Pair::Move => "move",
        }
    }

    /// The max's name.
    pub(crate) fn max(self) -> &'static str {
        match self {
            Pair::Hp => "maxhp",
            Pair::Mana => "maxmana",
            Pair::Move => "maxmove",
        }
    }

    /// The percent a `%K %k %E` capture fills.
    pub(crate) fn pct(self) -> &'static str {
        match self {
            Pair::Hp => "hp_pct",
            Pair::Mana => "mana_pct",
            Pair::Move => "move_pct",
        }
    }

    /// The pair a name reads, current, max or percent.
    pub(crate) fn of(name: &str) -> Option<Pair> {
        Pair::ALL.into_iter().find(|p| {
            name == p.cur()
                || name == p.pct()
                || entry(p.max()).is_some_and(|e| e.name == name || e.aliases.contains(&name))
        })
    }

    pub(super) fn gmcp(self, v: &gmcp::Vitals) -> (Option<i64>, Option<i64>) {
        match self {
            Pair::Hp => (v.hp, v.maxhp),
            Pair::Mana => (v.mana, v.maxmana),
            Pair::Move => (v.moves, v.maxmove),
        }
    }
}

/// A member value, `member_hp:<who>` and the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MemberStat {
    Hp,
    Mana,
    Move,
    Level,
    Class,
    Tnl,
}

impl MemberStat {
    fn of(name: &str) -> Option<Self> {
        Some(match name {
            "member_hp" => MemberStat::Hp,
            "member_mana" => MemberStat::Mana,
            "member_move" => MemberStat::Move,
            "member_level" => MemberStat::Level,
            "member_class" => MemberStat::Class,
            "member_tnl" => MemberStat::Tnl,
            _ => return None,
        })
    }

    pub(super) fn word(self) -> &'static str {
        match self {
            MemberStat::Hp => "health",
            MemberStat::Mana => "mana",
            MemberStat::Move => "moves",
            MemberStat::Level => "level",
            MemberStat::Class => "class",
            MemberStat::Tnl => "to next level",
        }
    }
}

/// A field reference read against the catalog.
#[derive(Debug, Clone, Copy)]
pub(super) enum Field<'a> {
    Entry(&'static Entry),
    Aff(&'a str),
    Member(MemberStat, &'a str),
    Queue(&'a str),
    Gmcp(&'a str),
}

pub(super) fn field(f: &FieldRef) -> Option<Field<'_>> {
    match &f.param {
        Some(param) => match f.name.as_str() {
            "aff" => Some(Field::Aff(param)),
            "queue" => Some(Field::Queue(param)),
            "gmcp" => Some(Field::Gmcp(param)),
            name => MemberStat::of(name).map(|stat| Field::Member(stat, param)),
        },
        None => entry(&f.name).map(Field::Entry),
    }
}

impl Entry {
    /// The sample as a value, for previews with no live data. `now` fills
    /// the clock and date.
    pub(crate) fn sample_value(&self, now: NaiveDateTime) -> Resolved {
        value_of(self.kind, self.label, self.sample, now)
    }
}

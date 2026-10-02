//! The variables a template reads (section 1 of the build spec).
//!
//! [`CATALOG`] names every field Vosh knows, with its label, kind, group,
//! sources, a sample for previews and the words the picker searches.
//! [`Vars`] holds what feeds them in a session: script values, the last
//! recognized prompt (the capture), the GMCP snapshot, and whether the
//! Forsaken Lands rules hold. [`Resolver`] answers the renderer with each
//! field's state.
//!
//! Sources, first fresh one wins.
//!
//! 1. Script values from `mud.set_prompt_var`. A value for a name the
//!    capture or GMCP also supplies lasts for the pulse it was set in. A
//!    name neither supplies keeps its value, Vosh's own among them.
//! 2. The capture, replaced whole by each recognized prompt, and fresh
//!    while no pulse has started since.
//! 3. GMCP, the latest packet per package.
//! 4. Vosh itself, the tick, your target, the clock and the profile.
//!
//! A value the game hides is Hidden whatever the sources hold, and Vosh
//! never fills it from another one. [`Hidden`] is worked out from the
//! latest packets and what the prompt read this pulse, through the
//! capture or a prompt trigger's script values, never stored.

use std::collections::BTreeMap;

use chrono::{DateTime, FixedOffset, NaiveDateTime};
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use serde_json::Value as Json;

use crate::aabahran;
use crate::format::{lang_game, tank_bar_cells, Position, Resolved, Value, MOON_CODES};
use crate::gmcp::{
    self, Find, Observed, Snapshot, CHAR_COMBAT, CHAR_STATE, CHAR_STATUS, CHAR_VITALS, CHAR_WORTH,
    GROUP_INFO, IMM_QUEUES, ROOM_CHARS, ROOM_INFO, ROOM_ITEMS, ROOM_WEATHER, WORLD_MOONS,
    WORLD_TIME,
};
use crate::render::Values;
use crate::template::FieldRef;

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

/// A format a field offers in the picker (section 1.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatId {
    Value,
    Max,
    Pct,
    Bar,
    Game,
    Word,
    Name,
    Grouped,
    Short,
    Thousands,
    Unit,
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
            Kind::Hour => &[F::Value, F::Word],
            Kind::Temp => &[F::Value, F::Unit],
            Kind::Seconds => &[F::Value, F::Unit, F::Bar],
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
    /// The GMCP source as the picker shows it, package and fields.
    pub gmcp: Option<&'static str>,
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
    gmcp: None,
    package: None,
    new_build: false,
    codes: &[],
    sample: "",
    search: &[],
    param: false,
    listed: true,
};

/// Every field Vosh knows, in the picker's order.
pub static CATALOG: &[Entry] = &[
    // Vitals
    Entry {
        name: "hp",
        label: "Health",
        kind: Kind::Gauge,
        group: Group::Vitals,
        gmcp: Some("Char.Vitals hp maxhp"),
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
        gmcp: Some("Char.Vitals maxhp"),
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
        gmcp: Some("Char.Vitals mana maxmana"),
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
        gmcp: Some("Char.Vitals maxmana"),
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
        gmcp: Some("Char.Vitals move maxmove"),
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
        gmcp: Some("Char.Vitals maxmove"),
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
        gmcp: Some("Char.Combat target"),
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
        gmcp: Some("Char.Combat target"),
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
        gmcp: Some("Char.Combat hp_pct"),
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
        gmcp: Some("Char.Combat condition"),
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
        gmcp: Some("Char.Combat tank.name"),
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
        gmcp: Some("Char.Combat tank.hp_pct"),
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
        gmcp: Some("Char.State position"),
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
        gmcp: Some("Group.Info leader"),
        package: Some(GROUP_INFO),
        sample: "Ketterly",
        ..BASE
    },
    Entry {
        name: "group_size",
        label: "Group size",
        kind: Kind::Count,
        group: Group::Group,
        gmcp: Some("Group.Info members"),
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
        gmcp: Some("Group.Info members"),
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
        gmcp: Some("Group.Info members[].hp_pct"),
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
        gmcp: Some("Group.Info members[].mana_pct"),
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
        gmcp: Some("Group.Info members[].move_pct"),
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
        gmcp: Some("Group.Info members[].level"),
        package: Some(GROUP_INFO),
        sample: "50",
        param: true,
        ..BASE
    },
    Entry {
        name: "member_class",
        label: "A member's class",
        group: Group::Group,
        gmcp: Some("Group.Info members[].class"),
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
        gmcp: Some("Group.Info members[].tnl"),
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
        gmcp: Some("Char.Status name"),
        package: Some(CHAR_STATUS),
        sample: "Wystan",
        ..BASE
    },
    Entry {
        name: "level",
        label: "Level",
        kind: Kind::Num,
        group: Group::Character,
        gmcp: Some("Char.Status level"),
        package: Some(CHAR_STATUS),
        sample: "50",
        ..BASE
    },
    Entry {
        name: "race",
        label: "Race",
        group: Group::Character,
        gmcp: Some("Char.Status race"),
        package: Some(CHAR_STATUS),
        sample: "human",
        ..BASE
    },
    Entry {
        name: "class",
        label: "Class",
        group: Group::Character,
        gmcp: Some("Char.Status class"),
        package: Some(CHAR_STATUS),
        sample: "warrior",
        ..BASE
    },
    Entry {
        name: "lang",
        label: "Language",
        kind: Kind::Lang,
        group: Group::Character,
        gmcp: Some("Char.State language"),
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
        gmcp: Some("Char.Worth gold"),
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
        gmcp: Some("Char.Worth bank"),
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
        gmcp: Some("Char.Worth exp"),
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
        gmcp: Some("Char.Worth tnl"),
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
        gmcp: Some("Char.Worth trains"),
        package: Some(CHAR_WORTH),
        sample: "3",
        ..BASE
    },
    Entry {
        name: "pracs",
        label: "Practices",
        kind: Kind::Num,
        group: Group::Worth,
        gmcp: Some("Char.Worth practices"),
        package: Some(CHAR_WORTH),
        sample: "12",
        ..BASE
    },
    Entry {
        name: "cp",
        label: "Cabal points",
        kind: Kind::Num,
        group: Group::Worth,
        gmcp: Some("Char.Worth cps"),
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
        gmcp: Some("Char.Worth rps"),
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
        gmcp: Some("Char.Worth cabal"),
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
        gmcp: Some("Char.Affects"),
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
        gmcp: Some("Char.Affects"),
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
        gmcp: Some("Room.Info name"),
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
        gmcp: Some("Room.Info num"),
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
        gmcp: Some("Room.Info area"),
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
        gmcp: Some("Room.Info exits"),
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
        gmcp: Some("Room.Info terrain"),
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
        gmcp: Some("Room.Weather region, then Room.Info climate"),
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
        gmcp: Some("Room.Info sector"),
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
        gmcp: Some("Room.Info region"),
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
        gmcp: Some("Room.Weather temp unit"),
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
        gmcp: Some("Room.Weather sky"),
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
        gmcp: Some("Room.Chars"),
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
        gmcp: Some("Room.Items"),
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
        gmcp: Some("World.Time hour"),
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
        gmcp: Some("World.Time day"),
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
        gmcp: Some("World.Time month"),
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
        gmcp: Some("World.Time year"),
        package: Some(WORLD_TIME),
        sample: "812",
        search: &["date"],
        ..BASE
    },
    Entry {
        name: "sun",
        label: "Sunlight",
        group: Group::TimeAndSky,
        gmcp: Some("World.Time sunlight"),
        package: Some(WORLD_TIME),
        sample: "light",
        search: &["day", "night", "dark"],
        ..BASE
    },
    Entry {
        name: "sky",
        label: "Sky",
        group: Group::TimeAndSky,
        gmcp: Some("World.Time sky"),
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
        gmcp: Some("World.Moons eclipse"),
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
        gmcp: Some("World.Moons triad"),
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
        gmcp: Some("World.Moons near_alignment"),
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
        gmcp: Some("Imm.Queues"),
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
        gmcp: Some("World.Moons moons"),
        package: Some(WORLD_MOONS),
        codes: code,
        sample,
        search: &["moon", "phase"],
        ..BASE
    }
}

/// The catalog entry for a name or one of its aliases. Fields written
/// with a parameter are found only by [`field`].
pub fn entry(name: &str) -> Option<&'static Entry> {
    CATALOG
        .iter()
        .find(|e| !e.param && (e.name == name || e.aliases.contains(&name)))
}

/// The catalog entry for a field as a template reads it: a name or an
/// alias, or a field written with a parameter, `aff:sanctuary` among
/// them. None for a name only scripts set.
pub fn entry_for(field: &FieldRef) -> Option<&'static Entry> {
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
fn capture_keys(e: &Entry) -> Vec<&'static str> {
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
pub fn feeds(name: &str) -> &str {
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
pub enum Pair {
    Hp,
    Mana,
    Move,
}

impl Pair {
    pub const ALL: [Pair; 3] = [Pair::Hp, Pair::Mana, Pair::Move];

    /// The current value's name.
    pub fn cur(self) -> &'static str {
        match self {
            Pair::Hp => "hp",
            Pair::Mana => "mana",
            Pair::Move => "move",
        }
    }

    /// The max's name.
    pub fn max(self) -> &'static str {
        match self {
            Pair::Hp => "maxhp",
            Pair::Mana => "maxmana",
            Pair::Move => "maxmove",
        }
    }

    /// The percent a `%K %k %E` capture fills.
    pub fn pct(self) -> &'static str {
        match self {
            Pair::Hp => "hp_pct",
            Pair::Mana => "mana_pct",
            Pair::Move => "move_pct",
        }
    }

    /// The pair a name reads, current, max or percent.
    pub fn of(name: &str) -> Option<Pair> {
        Pair::ALL.into_iter().find(|p| {
            name == p.cur()
                || name == p.pct()
                || entry(p.max()).is_some_and(|e| e.name == name || e.aliases.contains(&name))
        })
    }

    fn gmcp(self, v: &gmcp::Vitals) -> (Option<i64>, Option<i64>) {
        match self {
            Pair::Hp => (v.hp, v.maxhp),
            Pair::Mana => (v.mana, v.maxmana),
            Pair::Move => (v.moves, v.maxmove),
        }
    }
}

/// A member value, `member_hp:<who>` and the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemberStat {
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

    fn word(self) -> &'static str {
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
enum Field<'a> {
    Entry(&'static Entry),
    Aff(&'a str),
    Member(MemberStat, &'a str),
    Queue(&'a str),
    Gmcp(&'a str),
}

fn field(f: &FieldRef) -> Option<Field<'_>> {
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

// ---------------------------------------------------------------------
// Samples
// ---------------------------------------------------------------------

impl Entry {
    /// The sample as a value, for previews with no live data. `now` fills
    /// the clock and date.
    pub fn sample_value(&self, now: NaiveDateTime) -> Resolved {
        value_of(self.kind, self.label, self.sample, now)
    }
}

/// A value of `kind` from text in the form the catalog's samples take:
/// `1020/1020` for a gauge, `14/60` for the tick, a word for a position,
/// a phase number for a moon, names joined by commas for a count. Empty
/// text is Absent, but a clock reads `now`. `label` names an immortal
/// level, `Wizi` or `Incog`.
pub fn value_of(kind: Kind, label: &str, text: &str, now: NaiveDateTime) -> Resolved {
    let s = text;
    let num = |t: &str| t.trim().parse::<i64>().ok();
    let value = match kind {
        Kind::Clock | Kind::Date => Value::Clock {
            at: now,
            date: kind == Kind::Date,
        },
        _ if s.is_empty() => return Resolved::Absent,
        Kind::Gauge => {
            let (cur, max) = s.split_once('/').unwrap_or((s, ""));
            Value::Gauge {
                cur: num(cur).unwrap_or(0),
                max: num(max),
                pct: None,
            }
        }
        Kind::Seconds => {
            let (secs, max) = s.split_once('/').unwrap_or((s, ""));
            Value::Seconds {
                secs: num(secs).unwrap_or(0),
                max: num(max),
            }
        }
        Kind::Num => Value::Num(num(s).unwrap_or(0)),
        Kind::Pct => Value::Pct(num(s).unwrap_or(0)),
        Kind::TankPct => Value::TankHp(num(s).unwrap_or(0)),
        Kind::Text => Value::Text(s.to_string()),
        Kind::Raw => Value::Styled(s.to_string()),
        Kind::Flag => Value::Flag,
        Kind::Count => Value::List(s.split(',').map(str::to_string).collect()),
        Kind::Position => return Position::from_word(s).map_or(Resolved::Absent, pos_value),
        Kind::Lang => Value::Lang(s.to_string()),
        Kind::Moon => return moon_code_value(MOON_CODES[num(s).unwrap_or(0) as usize % 8]),
        Kind::Exits => exits_value(s),
        Kind::Level => Value::Level {
            word: label.to_string(),
            level: num(s).unwrap_or(0),
        },
        Kind::Slot => Value::Slot(s.to_string()),
        Kind::Hour => Value::Hour(num(s).unwrap_or(0).clamp(0, 23) as u8),
        Kind::Temp => {
            let digits = s.trim_end_matches(|c: char| c.is_ascii_alphabetic());
            Value::Temp {
                degrees: num(digits).unwrap_or(0),
                unit: s[digits.len()..].chars().next(),
            }
        }
        Kind::Ticks => Value::Ticks(num(s).unwrap_or(0)),
        Kind::Member => {
            let (name, pct) = s.rsplit_once(' ').unwrap_or((s, "0"));
            Value::Member {
                name: name.to_string(),
                pct: num(pct).unwrap_or(0),
            }
        }
    };
    Resolved::Value(value)
}

/// Every field drawn from the catalog's samples, for previews with no
/// live data.
pub struct Samples {
    pub now: NaiveDateTime,
}

impl Values for Samples {
    fn resolve(&self, f: &FieldRef) -> Resolved {
        match field(f) {
            Some(Field::Entry(e)) => e.sample_value(self.now),
            Some(Field::Aff(_)) => Resolved::Value(Value::Ticks(12)),
            Some(Field::Queue(_)) => Resolved::Value(Value::Num(3)),
            Some(Field::Member(stat, _)) => {
                let name = match stat {
                    MemberStat::Hp => "member_hp",
                    MemberStat::Mana => "member_mana",
                    MemberStat::Move => "member_move",
                    MemberStat::Level => "member_level",
                    MemberStat::Class => "member_class",
                    MemberStat::Tnl => "member_tnl",
                };
                CATALOG
                    .iter()
                    .find(|e| e.name == name)
                    .map_or(Resolved::Missing, |e| e.sample_value(self.now))
            }
            Some(Field::Gmcp(_)) => Resolved::Missing,
            None => Resolved::Unknown,
        }
    }

    fn label(&self, f: &FieldRef) -> String {
        label(f, None)
    }
}

// ---------------------------------------------------------------------
// The session's variables
// ---------------------------------------------------------------------

/// The last recognized prompt. Every group the matched shape has is a
/// key. A group that printed nothing, such as `%p` under lamented tears
/// or the `(Wizi N)` prefix when you are visible, is an empty string. A
/// name the shape does not have is no key at all.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Capture {
    pub values: BTreeMap<String, String>,
    /// The block as the game sent it, colors included, for `%{raw}`.
    pub raw: Option<String>,
}

/// A script value and the pulse it was set in. None for a name only
/// scripts supply, which never goes stale.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Scripted {
    value: String,
    pulse: Option<u64>,
}

/// What Vosh itself supplies.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Vosh {
    pub tick: Option<Tick>,
    /// Your target, the client one (`#target`), not Char.Combat's.
    pub target: Option<String>,
    /// The active profile's display name.
    pub profile: Option<String>,
    /// The clock, None for the local time now.
    pub now: Option<NaiveDateTime>,
    /// The profile's tracked affects, for `missing`.
    pub tracked: Vec<String>,
}

/// The tick timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tick {
    /// Whole seconds until the next tick.
    pub remaining: i64,
    /// Seconds between ticks, when known.
    pub interval: Option<i64>,
}

/// Which values the game hides right now. Worked out from the latest
/// packets and the fresh prompt values, never stored (D23).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hidden {
    pub hp: bool,
    pub mana: bool,
    pub moves: bool,
    /// The tank's health.
    pub tank: bool,
    /// Your opponent's health and condition.
    pub opponent: bool,
    /// Every affect field and the Affects pane.
    pub affects: bool,
    /// Every group field and the Group pane.
    pub group: bool,
}

impl Hidden {
    pub fn pair(&self, pair: Pair) -> bool {
        match pair {
            Pair::Hp => self.hp,
            Pair::Mana => self.mana,
            Pair::Move => self.moves,
        }
    }

    /// Any of the three vitals.
    pub fn vitals(&self) -> bool {
        self.hp || self.mana || self.moves
    }

    /// Nothing is hidden.
    pub fn none(&self) -> bool {
        *self == Hidden::default()
    }
}

/// The `session://hidden` payload, `{vitals, tank, opponent, affects,
/// group}`.
impl Serialize for Hidden {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("Hidden", 5)?;
        s.serialize_field("vitals", &self.vitals())?;
        s.serialize_field("tank", &self.tank)?;
        s.serialize_field("opponent", &self.opponent)?;
        s.serialize_field("affects", &self.affects)?;
        s.serialize_field("group", &self.group)?;
        s.end()
    }
}

/// True when the Forsaken Lands rules hold (D17). They hold when the host
/// is The Forsaken Lands or the active capture reads Aabahran's codes.
pub fn forsaken_lands(known_host: bool, aabahran_capture: bool) -> bool {
    known_host || aabahran_capture
}

/// The variables of one session.
#[derive(Debug, Clone, Default)]
pub struct Vars {
    forsaken: bool,
    gmcp: Snapshot,
    capture: Option<(Capture, u64)>,
    script: BTreeMap<String, Scripted>,
    hidden: Hidden,
    emitted: Hidden,
    disagreements: u64,
}

impl Vars {
    /// Empty variables. `forsaken` is whether the Forsaken Lands rules
    /// hold.
    pub fn new(forsaken: bool) -> Self {
        Self {
            forsaken,
            ..Self::default()
        }
    }

    pub fn forsaken(&self) -> bool {
        self.forsaken
    }

    /// Change whether the Forsaken Lands rules hold, as a profile switch
    /// or a new capture kind can.
    pub fn set_forsaken(&mut self, forsaken: bool) {
        self.forsaken = forsaken;
        self.recompute();
    }

    pub fn gmcp(&self) -> &Snapshot {
        &self.gmcp
    }

    /// The server is the new build: the Forsaken Lands rules hold and a
    /// Char.Prompt has come since the socket connected (D24).
    pub fn new_build(&self) -> bool {
        self.forsaken && self.gmcp.prompt_seen()
    }

    /// Keep a GMCP packet.
    pub fn observe(&mut self, package: &str, data: Json, at: DateTime<FixedOffset>) -> Observed {
        let observed = self.gmcp.observe(package, data, at);
        self.recompute();
        observed
    }

    /// Note one of your own sends. It starts a pulse on a server that has
    /// sent no Char.Vitals.
    pub fn on_send(&mut self) -> bool {
        let pulse = self.gmcp.on_send();
        if pulse {
            self.recompute();
        }
        pulse
    }

    /// Take a recognized prompt's values. They are fresh until the next
    /// pulse starts. Returns the names whose value disagrees with GMCP,
    /// for the `vosh::prompt` log, and counts them.
    pub fn capture(&mut self, capture: Capture) -> Vec<&'static str> {
        self.capture = Some((capture, self.gmcp.pulse()));
        self.recompute();
        let disagree = self.disagreements_now();
        self.disagreements += disagree.len() as u64;
        disagree
    }

    /// How many disagreements between a fresh capture and GMCP this
    /// session.
    pub fn disagreements(&self) -> u64 {
        self.disagreements
    }

    /// A value from `mud.set_prompt_var`. A name a capture or GMCP also
    /// supplies lasts until the next pulse. A prompt trigger reads the
    /// prompt this way, so the hidden state follows it.
    pub fn set_script(&mut self, name: &str, value: &str) {
        let pulse = is_sourced(name).then(|| self.gmcp.pulse());
        self.script.insert(
            name.to_string(),
            Scripted {
                value: value.to_string(),
                pulse,
            },
        );
        self.recompute();
    }

    /// Clear a script value.
    pub fn remove_script(&mut self, name: &str) -> bool {
        let removed = self.script.remove(name).is_some();
        if removed {
            self.recompute();
        }
        removed
    }

    /// A profile switch keeps the GMCP snapshot and the new build sign,
    /// and clears the capture and script values.
    pub fn switch_profile(&mut self, forsaken: bool) {
        self.capture = None;
        self.script.clear();
        self.forsaken = forsaken;
        self.recompute();
    }

    /// A disconnect clears everything but the rules and what was last
    /// emitted, so the next [`Vars::take_hidden_change`] clears the panes.
    pub fn disconnect(&mut self) {
        self.capture = None;
        self.script.clear();
        self.gmcp.clear();
        self.recompute();
    }

    /// What the game hides right now.
    pub fn hidden(&self) -> Hidden {
        self.hidden
    }

    /// The hidden state when it changed since the last call, for one
    /// `session://hidden` per socket read.
    pub fn take_hidden_change(&mut self) -> Option<Hidden> {
        (self.hidden != self.emitted).then(|| {
            self.emitted = self.hidden;
            self.hidden
        })
    }

    /// The hidden state the last [`Vars::take_hidden_change`] reported,
    /// which every open window has heard. A window that opens or reloads
    /// later reads this, since each change is reported once.
    pub fn reported(&self) -> Hidden {
        self.emitted
    }

    /// The fresh capture and script values for `session://prompt-vars`, a
    /// hidden one as `?`. Stale values are left out.
    pub fn prompt_vars(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        if let Some(capture) = self.fresh_capture() {
            for (name, value) in &capture.values {
                out.insert(name.clone(), value.clone());
            }
        }
        for (name, scripted) in &self.script {
            if self.script_fresh(scripted) {
                out.insert(name.clone(), scripted.value.clone());
            }
        }
        for (name, value) in &mut out {
            if self.name_hidden(name) {
                *value = "?".to_string();
            }
        }
        out
    }

    /// A resolver over these variables and what Vosh supplies.
    pub fn resolver<'a>(&'a self, vosh: &'a Vosh) -> Resolver<'a> {
        Resolver { vars: self, vosh }
    }

    /// Which source a field reads from now, in the order the resolver
    /// tries them: a fresh script value, the fresh capture, the latest
    /// packet of its package, then Vosh. None when none has it yet.
    pub fn source(&self, e: &Entry, vosh: &Vosh) -> Option<Source> {
        let keys = capture_keys(e);
        let fresh_script = |key: &str| self.script.get(key).is_some_and(|s| self.script_fresh(s));
        if keys.iter().any(|k| fresh_script(k)) {
            return Some(Source::Script);
        }
        let captured = self.fresh_capture().is_some_and(|c| {
            keys.iter()
                .any(|k| c.values.get(*k).is_some_and(|v| !v.trim().is_empty()))
        });
        if captured {
            return Some(Source::Capture);
        }
        let sent = match e.name {
            "region" => self.gmcp.has(ROOM_WEATHER) || self.gmcp.has(ROOM_INFO),
            "exits" => self.new_build() && self.gmcp.has(ROOM_INFO),
            _ => e.package.is_some_and(|p| self.gmcp.has(p)),
        };
        if sent {
            return Some(Source::Gmcp);
        }
        let vosh_has = match e.name {
            "tick" => vosh.tick.is_some(),
            "time" | "date" => true,
            "target" => vosh.target.as_deref().is_some_and(|t| !t.trim().is_empty()),
            "profile" => vosh
                .profile
                .as_deref()
                .is_some_and(|p| !p.trim().is_empty()),
            _ => false,
        };
        vosh_has.then_some(Source::Vosh)
    }

    /// The names a script set that no catalog field has, each with a
    /// fresh value, for the picker's Your scripts group.
    pub fn script_names(&self) -> Vec<&str> {
        self.script
            .iter()
            .filter(|(name, scripted)| self.script_fresh(scripted) && !known(name))
            .map(|(name, _)| name.as_str())
            .collect()
    }

    fn recompute(&mut self) {
        self.hidden = self.work_out_hidden();
    }

    fn fresh_capture(&self) -> Option<&Capture> {
        self.capture
            .as_ref()
            .filter(|(_, pulse)| *pulse == self.gmcp.pulse())
            .map(|(capture, _)| capture)
    }

    fn script_fresh(&self, scripted: &Scripted) -> bool {
        scripted.pulse.map_or(true, |p| p == self.gmcp.pulse())
    }

    /// A fresh prompt value: a script value, else the capture's. An empty
    /// string means the prompt printed nothing there.
    fn var(&self, name: &str) -> Option<&str> {
        if let Some(scripted) = self.script.get(name).filter(|s| self.script_fresh(s)) {
            return Some(&scripted.value);
        }
        self.fresh_capture()
            .and_then(|c| c.values.get(name))
            .map(String::as_str)
    }

    /// A pair's fresh max from the prompt, under the first of its
    /// spellings that has one, the order the resolver reads them in.
    fn max_var(&self, pair: Pair) -> Option<&str> {
        let aliases: &[&str] = entry(pair.max()).map_or(&[], |e| e.aliases);
        std::iter::once(pair.max())
            .chain(aliases.iter().copied())
            .find_map(|name| self.var(name))
    }

    /// True when a script or any capture, stale or not, has had the name,
    /// so it is known even without a fresh value.
    fn known_var(&self, name: &str) -> bool {
        self.script.contains_key(name)
            || self
                .capture
                .as_ref()
                .is_some_and(|(c, _)| c.values.contains_key(name))
    }

    /// True when the name reads a value the game hides.
    fn name_hidden(&self, name: &str) -> bool {
        if let Some(pair) = Pair::of(name) {
            return self.hidden.pair(pair);
        }
        match name {
            "tank_hp" | "tank_pct" | "tank_bar" => self.hidden.tank,
            "opponent_hp" | "opponent_cond" => self.hidden.opponent,
            "missing" => self.hidden.affects,
            "leader" | "group_size" | "group_low" => self.hidden.group,
            _ => false,
        }
    }

    // -----------------------------------------------------------------
    // Hidden (section 1.2)
    // -----------------------------------------------------------------

    fn work_out_hidden(&self) -> Hidden {
        let vitals = self.gmcp.vitals();
        let affects = self.gmcp.affects();
        let group = self.gmcp.group();
        let combat = self.gmcp.combat();
        let v_flag = vitals.as_ref().is_some_and(|v| v.hidden);
        let a_flag = affects.as_ref().is_some_and(|a| a.hidden);
        let g_flag = group.as_ref().is_some_and(|g| g.hidden);
        let k_flag = combat.as_ref().is_some_and(|k| k.hidden);
        let tank_without_health = combat
            .as_ref()
            .and_then(|k| k.tank.as_ref())
            .is_some_and(|t| t.hp_pct.is_none());

        if !self.forsaken {
            // Another game. A packet that says hidden means it, as the
            // panes read it on every host. The derived rules below read
            // Aabahran's own habits, a max of 0 or a named song, so
            // they stay on the Forsaken Lands.
            return Hidden {
                hp: v_flag,
                mana: v_flag,
                moves: v_flag,
                tank: false,
                opponent: k_flag,
                affects: a_flag,
                group: g_flag,
            };
        }

        if self.new_build() {
            // The packets' own flags decide, and nothing else does.
            return Hidden {
                hp: v_flag,
                mana: v_flag,
                moves: v_flag,
                tank: tank_without_health,
                opponent: k_flag,
                affects: a_flag,
                group: g_flag,
            };
        }

        // What the prompt read this pulse. A prompt trigger that hands
        // its groups to `mud.set_prompt_var` reads the prompt as surely
        // as a capture does, and its values last the same one pulse.
        let captured = |name: &str| self.var(name);
        // H1, the prompt read a max of 0.
        let h1 = |pair: Pair| {
            self.max_var(pair)
                .is_some_and(|m| m.trim().parse() == Ok(0_i64))
        };
        // H2, the prompt read a tank but %p and %P printed nothing.
        let h2 = captured("tank").is_some_and(|t| !t.trim().is_empty()) && {
            let health: Vec<&str> = ["tank_pct", "tank_bar"]
                .iter()
                .filter_map(|k| captured(k))
                .collect();
            !health.is_empty() && health.iter().all(|h| h.trim().is_empty())
        };
        // H3, Char.Vitals sent a max of 0.
        let h3 = |pair: Pair| vitals.as_ref().is_some_and(|v| pair.gmcp(v).1 == Some(0));
        // H4, Char.Vitals carries the flag.
        let h4 = v_flag;
        // H5, Char.Combat names a target but leaves out its health or
        // condition.
        let h5 = combat
            .as_ref()
            .is_some_and(|k| k.target.is_some() && (k.hp_pct.is_none() || k.condition.is_none()));
        // H6, Group.Info carries the flag.
        let h6 = g_flag;
        // H7, Char.Affects names the song or carries the flag.
        let h7 = a_flag || affects.as_ref().is_some_and(aabahran::names_lament);
        // Z, Group.Info is {}.
        let z = group.as_ref().is_some_and(|g| g.empty);

        let pair = |p: Pair| h1(p) || h3(p) || h4 || h7;
        let any_h1_h3 = Pair::ALL.into_iter().any(|p| h1(p) || h3(p));
        let affects_empty = affects.as_ref().is_some_and(|a| a.list.is_empty());
        Hidden {
            hp: pair(Pair::Hp),
            mana: pair(Pair::Mana),
            moves: pair(Pair::Move),
            tank: h2 || h4 || h7 || tank_without_health,
            opponent: h5 || h7 || k_flag,
            affects: h7 || (affects_empty && any_h1_h3),
            group: h6 || h7 || (z && (any_h1_h3 || h4)),
        }
    }

    /// The names whose fresh captured value disagrees with GMCP, after
    /// the `%s` and `%S` mappings. Hidden values are not compared.
    fn disagreements_now(&self) -> Vec<&'static str> {
        let Some(capture) = self.fresh_capture() else {
            return Vec::new();
        };
        let got = |name: &str| {
            capture
                .values
                .get(name)
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
        };
        let num = |name: &str| got(name).and_then(|v| v.parse::<i64>().ok());
        let packet = |package: &str, key: &str| {
            self.gmcp
                .get(package)
                .and_then(|d| d.get(key))
                .and_then(gmcp::int)
        };
        let word = |package: &str, key: &str| {
            self.gmcp
                .get(package)
                .and_then(|d| d.get(key))
                .and_then(gmcp::text)
                .map(str::to_string)
        };
        let mut out = Vec::new();
        let mut check = |name: &'static str, same: Option<bool>| {
            if same == Some(false) && !self.name_hidden(name) {
                out.push(name);
            }
        };
        let both = |a: Option<i64>, b: Option<i64>| Some(a? == b?);
        if let Some(v) = self.gmcp.vitals() {
            for pair in Pair::ALL {
                let (cur, max) = pair.gmcp(&v);
                check(pair.cur(), both(num(pair.cur()), cur));
                check(pair.max(), both(num(pair.max()), max));
            }
        }
        for (name, key) in [
            ("gold", "gold"),
            ("exp", "exp"),
            ("tnl", "tnl"),
            ("cp", "cps"),
            ("rp", "rps"),
        ] {
            check(name, both(num(name), packet(CHAR_WORTH, key)));
        }
        check("hour", both(num("hour"), packet(WORLD_TIME, "hour")));
        check("temp", both(num("temp"), packet(ROOM_WEATHER, "temp")));
        let text_pair = |a: Option<&str>, b: Option<String>| Some(a? == b?);
        check(
            "weather",
            text_pair(got("weather"), word(ROOM_WEATHER, "sky")),
        );
        check(
            "region",
            text_pair(got("region"), word(ROOM_WEATHER, "region")),
        );
        if let Some(abbrev) = capture.values.get("pos") {
            let game = word(CHAR_STATE, "position").and_then(|w| Position::from_word(&w));
            check("pos", game.map(|p| p.abbrev() == abbrev.trim()));
        }
        check(
            "lang",
            text_pair(
                got("lang"),
                word(CHAR_STATE, "language").map(|l| lang_game(&l)),
            ),
        );
        let tank = self
            .gmcp
            .combat()
            .and_then(|k| k.tank)
            .and_then(|t| t.hp_pct);
        check("tank_hp", both(num("tank_pct"), tank));
        out
    }
}

// ---------------------------------------------------------------------
// The resolver
// ---------------------------------------------------------------------

/// Answers the renderer for one draw.
pub struct Resolver<'a> {
    vars: &'a Vars,
    vosh: &'a Vosh,
}

/// What one source says about a field.
enum Got {
    /// It decides the field.
    Is(Resolved),
    /// The prompt printed nothing there. Later sources may still fill it,
    /// and without one the field is Absent.
    Blank,
    /// It has nothing to say.
    Nothing,
}

/// The first source that decides, in order.
fn first(sources: &[&dyn Fn() -> Got]) -> Resolved {
    let mut blank = false;
    for source in sources {
        match source() {
            Got::Is(resolved) => return resolved,
            Got::Blank => blank = true,
            Got::Nothing => {}
        }
    }
    if blank {
        Resolved::Absent
    } else {
        Resolved::Missing
    }
}

fn is(value: Value) -> Got {
    Got::Is(Resolved::Value(value))
}

/// A source that answers with a whole state, where Missing means it has
/// nothing to say yet.
fn got(resolved: Resolved) -> Got {
    match resolved {
        Resolved::Missing => Got::Nothing,
        other => Got::Is(other),
    }
}

fn truthy(s: &str) -> bool {
    !matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "" | "0" | "false" | "no" | "off"
    )
}

fn pos_value(p: Position) -> Resolved {
    Resolved::Value(Value::Position(p))
}

/// A moon from what `%j` printed, `-` when it is not up.
fn moon_code_value(code: &str) -> Resolved {
    let code = code.trim();
    if code == "-" {
        return Resolved::Value(Value::Moon {
            phase: 0,
            active: false,
            name: None,
        });
    }
    match MOON_CODES.iter().position(|c| *c == code) {
        Some(phase) => Resolved::Value(Value::Moon {
            phase: phase as u8,
            active: true,
            name: None,
        }),
        None => Resolved::Value(Value::Text(code.to_string())),
    }
}

/// Exits from what `%e` printed, `[Exits: N E (S) W]`, or bare letters.
fn exits_value(text: &str) -> Value {
    let t = text.trim();
    let letters = t
        .strip_prefix("[Exits:")
        .and_then(|rest| rest.strip_suffix(']'))
        .map_or(t, str::trim)
        .to_string();
    let game = format!("[Exits: {letters}]");
    Value::Exits { letters, game }
}

/// Room.Info exits as `%e` letters, in the game's door order.
fn room_exits(exits: &Json) -> Value {
    const DOORS: [(&str, &str); 6] = [
        ("north", "N"),
        ("east", "E"),
        ("south", "S"),
        ("west", "W"),
        ("up", "U"),
        ("down", "D"),
    ];
    let letters: Vec<&str> = DOORS
        .iter()
        .filter(|(dir, _)| {
            exits
                .as_object()
                .is_some_and(|o| o.keys().any(|k| k.eq_ignore_ascii_case(dir)))
        })
        .map(|(_, letter)| *letter)
        .collect();
    let letters = if letters.is_empty() {
        "none".to_string()
    } else {
        letters.join(" ")
    };
    let game = format!("[Exits: {letters}]");
    Value::Exits { letters, game }
}

/// True when an affect row with `next` ticks left outlasts one with
/// `prev`. A permanent row (-1) outlasts every other, and a row that
/// gives no duration outlasts none. The Affects pane folds rows the same
/// way.
fn outlasts(next: Option<i64>, prev: Option<i64>) -> bool {
    match (next, prev) {
        (None, _) => false,
        (Some(_), None) => true,
        (Some(_), Some(p)) if p < 0 => false,
        (Some(n), Some(p)) => n < 0 || n > p,
    }
}

/// A GMCP value as a field value.
fn json_value(value: &Json) -> Resolved {
    match value {
        Json::Null | Json::Bool(false) => Resolved::Absent,
        Json::Bool(true) => Resolved::Value(Value::Flag),
        Json::Number(n) => Resolved::Value(
            n.as_i64()
                .map_or_else(|| Value::Text(n.to_string()), Value::Num),
        ),
        Json::String(s) if s.trim().is_empty() => Resolved::Absent,
        Json::String(s) => Resolved::Value(Value::Text(s.clone())),
        other => Resolved::Value(Value::Text(other.to_string())),
    }
}

impl<'a> Resolver<'a> {
    fn gmcp(&self) -> &'a Snapshot {
        &self.vars.gmcp
    }

    /// A prompt value read as a number, whole or with a decimal point,
    /// and text when it is no number.
    fn var_num(&self, name: &str) -> Got {
        match self.vars.var(name).map(str::trim) {
            None => Got::Nothing,
            Some("") => Got::Blank,
            Some(t) => is(Value::parse_number(t).unwrap_or_else(|| Value::Text(t.to_string()))),
        }
    }

    /// A prompt value read as text.
    fn var_text(&self, name: &str) -> Got {
        match self.vars.var(name).map(str::trim) {
            None => Got::Nothing,
            Some("") => Got::Blank,
            Some(t) => is(Value::Text(t.to_string())),
        }
    }

    /// A prompt value read as a flag.
    fn var_flag(&self, name: &str) -> Got {
        match self.vars.var(name) {
            None => Got::Nothing,
            Some(t) if truthy(t) => is(Value::Flag),
            Some(_) => Got::Is(Resolved::Absent),
        }
    }

    /// A prompt value read as a percent, text when it is not a number.
    fn var_pct(&self, name: &str) -> Got {
        match self.vars.var(name).map(str::trim) {
            None => Got::Nothing,
            Some("") => Got::Blank,
            Some(t) => is(t
                .parse::<i64>()
                .map_or_else(|_| Value::Text(t.to_string()), Value::Pct)),
        }
    }

    /// A prompt value read as a count. Zero is Absent, and a list of
    /// names prints as the script wrote it.
    fn var_count(&self, name: &str) -> Got {
        match self.var_num(name) {
            Got::Is(Resolved::Value(Value::Num(0))) => Got::Is(Resolved::Absent),
            other => other,
        }
    }

    /// A prompt value read as text with the game's colors, for `%{raw}`.
    fn var_styled(&self, name: &str) -> Got {
        match self.vars.var(name) {
            None => Got::Nothing,
            Some(t) if t.trim().is_empty() => Got::Blank,
            Some(t) => is(Value::Styled(t.to_string())),
        }
    }

    /// A field of a package. Once the package has come, a field it leaves
    /// out is Absent.
    fn packet(&self, package: &str, key: &str, read: fn(&Json) -> Resolved) -> Got {
        match self.gmcp().get(package) {
            None => Got::Nothing,
            Some(data) => Got::Is(data.get(key).map_or(Resolved::Absent, read)),
        }
    }

    fn packet_num(&self, package: &str, key: &str) -> Got {
        self.packet(package, key, |v| match gmcp::int(v) {
            Some(n) => Resolved::Value(Value::Num(n)),
            None => json_value(v),
        })
    }

    fn packet_text(&self, package: &str, key: &str) -> Got {
        self.packet(package, key, json_value)
    }

    /// The max of a pair from the first source with a number.
    fn max(&self, pair: Pair) -> Resolved {
        let aliases: &[&str] = entry(pair.max()).map_or(&[], |e| e.aliases);
        let spellings = std::iter::once(pair.max()).chain(aliases.iter().copied());
        let from_var = || {
            spellings
                .clone()
                .map(|name| self.var_num(name))
                .find(|g| !matches!(g, Got::Nothing))
                .unwrap_or(Got::Nothing)
        };
        let from_gmcp = || match self.gmcp().vitals() {
            None => Got::Nothing,
            Some(v) => match pair.gmcp(&v).1 {
                Some(n) => is(Value::Num(n)),
                None => Got::Is(Resolved::Absent),
            },
        };
        first(&[&from_var, &from_gmcp])
    }

    fn gauge(&self, pair: Pair, want_max: bool) -> Resolved {
        let max = match self.max(pair) {
            Resolved::Value(v) if v.number().is_some() => Some(v),
            _ => None,
        };
        // A max of 0 means the pair does not apply, as for a class with
        // no mana on another game. Under the Forsaken Lands rules a max
        // of 0 hides the pair first (H1, H3), so this is reached there
        // only on the new build, whose flags alone decide, and it draws
        // nothing rather than a percent with no max.
        if max.as_ref().is_some_and(Value::is_zero) {
            return Resolved::Absent;
        }
        if want_max {
            return self.max(pair);
        }
        let cur = first(&[
            &|| self.var_num(pair.cur()),
            &|| match self.gmcp().vitals() {
                None => Got::Nothing,
                Some(v) => match pair.gmcp(&v).0 {
                    Some(n) => is(Value::Num(n)),
                    None => Got::Is(Resolved::Absent),
                },
            },
        ]);
        let pct = self
            .vars
            .var(pair.pct())
            .and_then(|p| p.trim().parse::<i64>().ok());
        match (cur, max) {
            (Resolved::Value(Value::Num(cur)), None) => Resolved::Value(Value::Gauge {
                cur,
                max: None,
                pct,
            }),
            (Resolved::Value(cur), Some(max)) => {
                Resolved::Value(cur.over(&max, pct).unwrap_or(cur))
            }
            (other, _) => other,
        }
    }

    fn combat(&self) -> Option<gmcp::Combat> {
        self.gmcp().combat()
    }

    /// A Char.Combat field while you fight. `{}` is Absent.
    fn opponent(&self, read: fn(&gmcp::Combat) -> Option<Value>) -> Got {
        match self.combat() {
            None => Got::Nothing,
            Some(k) if k.target.is_none() => Got::Is(Resolved::Absent),
            Some(k) => Got::Is(read(&k).map_or(Resolved::Absent, Resolved::Value)),
        }
    }

    /// No tank in Char.Combat. Out of a fight, or on the new build, that
    /// means none. Elsewhere Char.Combat may simply not name one.
    fn no_tank(&self, k: &gmcp::Combat) -> Got {
        if k.target.is_none() || self.vars.new_build() {
            Got::Is(Resolved::Absent)
        } else {
            Got::Nothing
        }
    }

    fn tank(&self) -> Resolved {
        first(&[&|| self.var_text("tank"), &|| match self.combat() {
            None => Got::Nothing,
            Some(k) => match &k.tank {
                Some(t) => is(Value::Text(t.name.clone())),
                None => self.no_tank(&k),
            },
        }])
    }

    fn tank_hp(&self) -> Resolved {
        let from_var = || match self.vars.var("tank_hp").map(str::trim) {
            Some(t) if !t.is_empty() => t.parse().map_or(Got::Nothing, |n| is(Value::TankHp(n))),
            _ => Got::Nothing,
        };
        let from_pct = || match self.vars.var("tank_pct").map(str::trim) {
            None => Got::Nothing,
            Some("") => Got::Blank,
            Some(t) => t.parse().map_or(Got::Nothing, |n| is(Value::TankHp(n))),
        };
        // `%P` in twelfths. Char.Combat goes out on the same pulse with the
        // whole percent the bar was drawn from (`gmcp_send_combat`), so
        // when its percent fills as many cells it is the tank's health.
        // Otherwise the fresh bar wins, read back to the highest percent
        // that fills as many cells.
        let from_bar = || match self.vars.var("tank_bar").map(str::trim) {
            None => Got::Nothing,
            Some("") => Got::Blank,
            Some(t) => {
                let cells = t.chars().filter(|c| *c == '=').count();
                let fills =
                    |pct: &i64| tank_bar_cells(*pct).iter().filter(|c| **c).count() == cells;
                let exact = self
                    .combat()
                    .and_then(|k| k.tank)
                    .and_then(|t| t.hp_pct)
                    .filter(fills);
                is(Value::TankHp(exact.unwrap_or(cells as i64 * 25 / 3)))
            }
        };
        let from_gmcp = || match self.combat() {
            None => Got::Nothing,
            Some(k) => match &k.tank {
                Some(t) => t
                    .hp_pct
                    .map_or(Got::Is(Resolved::Absent), |p| is(Value::TankHp(p))),
                None => self.no_tank(&k),
            },
        };
        first(&[&from_var, &from_pct, &from_bar, &from_gmcp])
    }

    fn pos(&self) -> Resolved {
        let from_var = || match self.vars.var("pos") {
            None => Got::Nothing,
            Some(t) => {
                let t = t.trim();
                Position::from_abbrev(t)
                    .or_else(|| Position::from_word(t))
                    .map_or(Got::Nothing, |p| Got::Is(pos_value(p)))
            }
        };
        let from_gmcp = || match self.gmcp().state() {
            None => Got::Nothing,
            Some(state) => Got::Is(
                state
                    .position
                    .as_deref()
                    .and_then(Position::from_word)
                    .map_or(Resolved::Absent, pos_value),
            ),
        };
        first(&[&from_var, &from_gmcp])
    }

    /// `%s` lowercases the first letter, so a capture that matches
    /// Char.State that way takes Char.State's spelling.
    fn lang(&self) -> Resolved {
        let state = self.gmcp().state().map(|s| s.language);
        let from_var = || match self.vars.var("lang").map(str::trim) {
            None => Got::Nothing,
            Some("") => Got::Blank,
            Some(t) => match state.clone().flatten() {
                Some(game) if lang_game(&game) == t || game == t => is(Value::Lang(game)),
                _ => is(Value::Lang(t.to_string())),
            },
        };
        let from_gmcp = || match state.clone() {
            None => Got::Nothing,
            Some(None) => Got::Is(Resolved::Absent),
            Some(Some(game)) => is(Value::Lang(game)),
        };
        first(&[&from_var, &from_gmcp])
    }

    fn level(&self, name: &str, word: &str) -> Resolved {
        match self.vars.var(name).map(str::trim) {
            None => Resolved::Missing,
            Some("") => Resolved::Absent,
            Some(t) => Resolved::Value(t.parse().map_or_else(
                |_| Value::Text(t.to_string()),
                |level| Value::Level {
                    word: word.to_string(),
                    level,
                },
            )),
        }
    }

    fn group(&self) -> Option<gmcp::Group> {
        self.gmcp().group()
    }

    /// Your own name, from Char.Status.
    fn me(&self) -> Option<String> {
        self.gmcp()
            .get(CHAR_STATUS)
            .and_then(|d| d.get("name"))
            .and_then(gmcp::text)
            .map(str::to_string)
    }

    /// The roster, every member named. Solo is Absent.
    fn group_size(&self) -> Resolved {
        match self.group() {
            None => Resolved::Missing,
            Some(g) if g.members.is_empty() => Resolved::Absent,
            Some(g) => Resolved::Value(Value::List(
                g.members
                    .iter()
                    .map(|m| m.name.clone().unwrap_or_else(|| "someone".to_string()))
                    .collect(),
            )),
        }
    }

    /// The tick, a script value first, with Vosh's interval as its max.
    fn tick(&self) -> Resolved {
        let interval = self.vosh.tick.and_then(|t| t.interval);
        first(&[
            &|| match self.vars.var("tick").map(str::trim) {
                None => Got::Nothing,
                Some("") => Got::Blank,
                Some(t) => t.parse().map_or_else(
                    |_| is(Value::Text(t.to_string())),
                    |secs| {
                        is(Value::Seconds {
                            secs,
                            max: interval,
                        })
                    },
                ),
            },
            &|| match self.vosh.tick {
                Some(t) => is(Value::Seconds {
                    secs: t.remaining,
                    max: t.interval,
                }),
                None => Got::Nothing,
            },
        ])
    }

    fn group_low(&self) -> Resolved {
        let Some(group) = self.group() else {
            return Resolved::Missing;
        };
        let me = self.me();
        group
            .members
            .iter()
            .filter(|m| match (&me, &m.name) {
                (Some(me), Some(name)) => !name.eq_ignore_ascii_case(me),
                _ => true,
            })
            .filter_map(|m| Some((m, m.hp_pct?)))
            .fold(
                None,
                |low: Option<(&gmcp::Member, i64)>, (m, pct)| match low {
                    Some((_, lowest)) if lowest <= pct => low,
                    _ => Some((m, pct)),
                },
            )
            .map_or(Resolved::Absent, |(m, pct)| {
                Resolved::Value(Value::Member {
                    name: m.name.clone().unwrap_or_else(|| "someone".to_string()),
                    pct,
                })
            })
    }

    fn member(&self, stat: MemberStat, who: &str) -> Resolved {
        let Some(group) = self.group() else {
            return Resolved::Missing;
        };
        let wanted = who.trim();
        let found = match wanted.strip_prefix("id=") {
            Some(id) => {
                let id = id.trim().parse::<i64>().ok();
                group.members.iter().find(|m| m.id.is_some() && m.id == id)
            }
            None => group.members.iter().find(|m| {
                m.name
                    .as_deref()
                    .is_some_and(|n| gmcp::same_words(n, wanted))
            }),
        };
        let Some(m) = found else {
            return Resolved::Absent;
        };
        let value = match stat {
            MemberStat::Hp => m.hp_pct.map(Value::Pct),
            MemberStat::Mana => m.mana_pct.map(Value::Pct),
            MemberStat::Move => m.move_pct.map(Value::Pct),
            MemberStat::Level => m.level.map(Value::Num),
            MemberStat::Class => m.class.clone().map(Value::Text),
            MemberStat::Tnl => m.tnl.map(Value::Num),
        };
        value.map_or(Resolved::Absent, Resolved::Value)
    }

    fn missing(&self) -> Resolved {
        let Some(affects) = self.gmcp().affects() else {
            return Resolved::Missing;
        };
        let missing: Vec<String> = self
            .vosh
            .tracked
            .iter()
            .filter(|t| !affects.list.iter().any(|a| gmcp::same_words(&a.name, t)))
            .cloned()
            .collect();
        if missing.is_empty() {
            Resolved::Absent
        } else {
            Resolved::Value(Value::List(missing))
        }
    }

    /// An affect by name. The game sends one row per thing it modifies,
    /// and the name stays up until the last of them drops, so the longest
    /// row wins, as in the Affects pane.
    fn aff(&self, name: &str) -> Resolved {
        let Some(affects) = self.gmcp().affects() else {
            return Resolved::Missing;
        };
        let mut rows = affects
            .list
            .iter()
            .filter(|a| gmcp::same_words(&a.name, name));
        let Some(first) = rows.next() else {
            return Resolved::Absent;
        };
        let duration = rows.fold(first.duration, |longest, a| {
            if outlasts(a.duration, longest) {
                a.duration
            } else {
                longest
            }
        });
        Resolved::Value(duration.map_or(Value::Flag, Value::Ticks))
    }

    fn exits(&self) -> Resolved {
        let from_var = || match self.vars.var("exits").map(str::trim) {
            None => Got::Nothing,
            Some("") => Got::Blank,
            Some(t) => is(exits_value(t)),
        };
        // Room.Info only on the new build, which leaves out the exits %e
        // would not show (decision 7).
        let from_room = || {
            if !self.vars.new_build() {
                return Got::Nothing;
            }
            match self.gmcp().get(ROOM_INFO) {
                None => Got::Nothing,
                Some(d) => d
                    .get("exits")
                    .map_or(Got::Is(Resolved::Absent), |e| is(room_exits(e))),
            }
        };
        first(&[&from_var, &from_room])
    }

    /// Room.Weather first once it has come, then Room.Info's climate.
    fn region(&self) -> Resolved {
        first(&[
            &|| self.var_text("region"),
            &|| self.packet_text(ROOM_WEATHER, "region"),
            &|| self.packet_text(ROOM_INFO, "climate"),
        ])
    }

    fn temp(&self) -> Resolved {
        let weather = self.gmcp().weather();
        let unit = weather.as_ref().and_then(|w| w.unit);
        let from_var = || match self.vars.var("temp").map(str::trim) {
            None => Got::Nothing,
            Some("") => Got::Blank,
            Some(t) => t.parse().map_or_else(
                |_| is(Value::Text(t.to_string())),
                |degrees| is(Value::Temp { degrees, unit }),
            ),
        };
        let from_gmcp = || match &weather {
            None => Got::Nothing,
            Some(w) => w.temp.map_or(Got::Is(Resolved::Absent), |degrees| {
                is(Value::Temp { degrees, unit })
            }),
        };
        first(&[&from_var, &from_gmcp])
    }

    fn names(&self, package: &str) -> Resolved {
        match self.gmcp().names(package) {
            None => Resolved::Missing,
            Some(names) if names.is_empty() => Resolved::Absent,
            Some(names) => Resolved::Value(Value::List(names)),
        }
    }

    fn hour(&self) -> Resolved {
        let hour = |n: i64| is(Value::Hour(n.clamp(0, 23) as u8));
        let from_var = || match self.vars.var("hour").map(str::trim) {
            None => Got::Nothing,
            Some("") => Got::Blank,
            Some(t) => t.parse().map_or(Got::Nothing, hour),
        };
        let from_gmcp = || match self.gmcp().get(WORLD_TIME) {
            None => Got::Nothing,
            Some(d) => d
                .get("hour")
                .and_then(gmcp::int)
                .map_or(Got::Is(Resolved::Absent), hour),
        };
        first(&[&from_var, &from_gmcp])
    }

    fn moon(&self, index: usize) -> Resolved {
        let name = format!("moon{}", index + 1);
        let from_var = || match self.vars.var(&name).map(str::trim) {
            None => Got::Nothing,
            Some("") => Got::Blank,
            Some(code) => Got::Is(moon_code_value(code)),
        };
        let from_gmcp = || match self.gmcp().get(WORLD_MOONS) {
            None => Got::Nothing,
            Some(d) => {
                let moon = d
                    .get("moons")
                    .and_then(Json::as_array)
                    .and_then(|m| m.get(index));
                match moon {
                    None => Got::Is(Resolved::Absent),
                    Some(m) => is(Value::Moon {
                        phase: m.get("phase").and_then(gmcp::int).unwrap_or(0).clamp(0, 7) as u8,
                        active: m.get("active").and_then(Json::as_bool).unwrap_or(false),
                        name: m.get("phase_name").and_then(gmcp::text).map(str::to_string),
                    }),
                }
            }
        };
        first(&[&from_var, &from_gmcp])
    }

    /// The game sends `none` for no cabal. A script value prints as
    /// written.
    fn cabal(&self) -> Resolved {
        first(&[
            &|| self.var_text("cabal"),
            &|| match self.packet_text(CHAR_WORTH, "cabal") {
                Got::Is(Resolved::Value(Value::Text(t)))
                    if t.trim().eq_ignore_ascii_case("none") =>
                {
                    Got::Is(Resolved::Absent)
                }
                other => other,
            },
        ])
    }

    fn queue(&self, key: &str) -> Resolved {
        match self.gmcp().get(IMM_QUEUES) {
            None => Resolved::Missing,
            Some(d) => d
                .as_object()
                .and_then(|o| o.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)))
                .and_then(|(_, v)| gmcp::int(v))
                .map_or(Resolved::Absent, |n| Resolved::Value(Value::Num(n))),
        }
    }

    fn gmcp_path(&self, path: &str) -> Resolved {
        match self.gmcp().find(path) {
            Find::NoPacket => Resolved::Missing,
            Find::Missing { .. } => Resolved::Absent,
            Find::Found { value, .. } => json_value(value),
        }
    }

    /// True when a path reads a value the game hides.
    fn path_hidden(&self, path: &str) -> bool {
        let (package, keys) = match self.gmcp().find(path) {
            Find::NoPacket => return false,
            Find::Missing { package, keys } | Find::Found { package, keys, .. } => (package, keys),
        };
        let hidden = self.vars.hidden;
        let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
        if package.eq_ignore_ascii_case(CHAR_VITALS) {
            match keys.first().and_then(|k| Pair::of(k)) {
                Some(pair) => hidden.pair(pair),
                None => hidden.vitals(),
            }
        } else if package.eq_ignore_ascii_case(gmcp::CHAR_AFFECTS) {
            hidden.affects
        } else if package.eq_ignore_ascii_case(GROUP_INFO) {
            hidden.group
        } else if package.eq_ignore_ascii_case(CHAR_COMBAT) {
            match keys.as_slice() {
                [] => hidden.opponent || hidden.tank,
                ["hp_pct" | "condition", ..] => hidden.opponent,
                ["tank"] | ["tank", "hp_pct", ..] => hidden.tank,
                _ => false,
            }
        } else {
            false
        }
    }

    fn resolve_entry(&self, e: &'static Entry) -> Resolved {
        let name = e.name;
        if let Some(pair) = Pair::of(name) {
            return self.gauge(pair, name == pair.max());
        }
        match name {
            "fight" => first(&[&|| self.var_flag("fight"), &|| match self.combat() {
                None => Got::Nothing,
                Some(k) if k.target.is_some() => is(Value::Flag),
                Some(_) => Got::Is(Resolved::Absent),
            }]),
            "opponent" => first(&[&|| self.var_text("opponent"), &|| {
                self.opponent(|k| k.target.clone().map(Value::Text))
            }]),
            "opponent_hp" => first(&[&|| self.var_pct("opponent_hp"), &|| {
                self.opponent(|k| k.hp_pct.map(Value::Pct))
            }]),
            "opponent_cond" => first(&[&|| self.var_text("opponent_cond"), &|| {
                self.opponent(|k| k.condition.clone().map(Value::Text))
            }]),
            "tank" => self.tank(),
            "tank_hp" => self.tank_hp(),
            "pos" => self.pos(),
            "leader" => first(&[&|| self.var_text("leader"), &|| {
                self.packet_text(GROUP_INFO, "leader")
            }]),
            "group_size" => first(&[&|| self.var_count("group_size"), &|| got(self.group_size())]),
            "group_low" => first(&[&|| self.var_text("group_low"), &|| got(self.group_low())]),
            "name" | "race" | "class" => first(&[&|| self.var_text(name), &|| {
                self.packet_text(CHAR_STATUS, name)
            }]),
            "level" => first(&[&|| self.var_num("level"), &|| {
                self.packet_num(CHAR_STATUS, "level")
            }]),
            "lang" => self.lang(),
            "stallion" | "olc" | "pacify" => first(&[&|| self.var_text(name)]),
            "area_num" | "olc_vnum" => first(&[&|| self.var_num(name)]),
            "wizi" => self.level("wizi", "Wizi"),
            "incog" => self.level("incog", "Incog"),
            "afk" => first(&[&|| self.var_flag("afk")]),
            "gold" | "exp" | "tnl" => first(&[&|| self.var_num(name), &|| {
                self.packet_num(CHAR_WORTH, name)
            }]),
            "cp" => first(&[&|| self.var_num("cp"), &|| {
                self.packet_num(CHAR_WORTH, "cps")
            }]),
            "rp" => first(&[&|| self.var_num("rp"), &|| {
                self.packet_num(CHAR_WORTH, "rps")
            }]),
            "bank" | "trains" => first(&[&|| self.var_num(name), &|| {
                self.packet_num(CHAR_WORTH, name)
            }]),
            "pracs" => first(&[&|| self.var_num("pracs"), &|| {
                self.packet_num(CHAR_WORTH, "practices")
            }]),
            "cabal" => self.cabal(),
            "missing" => first(&[&|| self.var_count("missing"), &|| got(self.missing())]),
            "room" => first(&[&|| self.var_text("room"), &|| {
                self.packet_text(ROOM_INFO, "name")
            }]),
            "room_num" => first(&[&|| self.var_num("room_num"), &|| {
                self.packet_num(ROOM_INFO, "num")
            }]),
            "area" => first(&[&|| self.var_text("area"), &|| {
                self.packet_text(ROOM_INFO, "area")
            }]),
            "exits" => self.exits(),
            "terrain" => first(&[&|| self.var_text("terrain"), &|| {
                self.packet_text(ROOM_INFO, "terrain")
            }]),
            "sector" => first(&[&|| self.var_num("sector"), &|| {
                self.packet_num(ROOM_INFO, "sector")
            }]),
            "region_num" => first(&[&|| self.var_num("region_num"), &|| {
                self.packet_num(ROOM_INFO, "region")
            }]),
            "region" => self.region(),
            "temp" => self.temp(),
            "weather" => first(&[&|| self.var_text("weather"), &|| {
                self.packet_text(ROOM_WEATHER, "sky")
            }]),
            "people" => first(&[
                &|| self.var_count("people"),
                &|| got(self.names(ROOM_CHARS)),
            ]),
            "things" => first(&[
                &|| self.var_count("things"),
                &|| got(self.names(ROOM_ITEMS)),
            ]),
            "hour" => self.hour(),
            "day" | "month" | "year" => first(&[&|| self.var_num(name), &|| {
                self.packet_num(WORLD_TIME, name)
            }]),
            "sun" => first(&[&|| self.var_text("sun"), &|| {
                self.packet_text(WORLD_TIME, "sunlight")
            }]),
            "sky" => first(&[&|| self.var_text("sky"), &|| {
                self.packet_text(WORLD_TIME, "sky")
            }]),
            "moon1" => self.moon(0),
            "moon2" => self.moon(1),
            "moon3" => self.moon(2),
            "eclipse" | "triad" => first(&[&|| self.var_flag(name), &|| {
                self.packet(WORLD_MOONS, name, json_value)
            }]),
            "near" => first(&[&|| self.var_flag("near"), &|| {
                self.packet(WORLD_MOONS, "near_alignment", json_value)
            }]),
            "tick" => self.tick(),
            // The clock reads no script value, as the first renderer read
            // none.
            "time" | "date" => Resolved::Value(Value::Clock {
                at: self
                    .vosh
                    .now
                    .unwrap_or_else(|| chrono::Local::now().naive_local()),
                date: name == "date",
            }),
            "target" => first(&[&|| self.var_text("target"), &|| match self
                .vosh
                .target
                .as_deref()
                .map(str::trim)
            {
                Some(t) if !t.is_empty() => is(Value::Text(t.to_string())),
                _ => Got::Is(Resolved::Absent),
            }]),
            "profile" => first(&[&|| self.var_text("profile"), &|| match self
                .vosh
                .profile
                .as_deref()
                .map(str::trim)
            {
                Some(p) if !p.is_empty() => is(Value::Text(p.to_string())),
                _ => Got::Nothing,
            }]),
            "raw" => first(&[&|| self.var_styled("raw"), &|| match self
                .vars
                .fresh_capture()
                .and_then(|c| c.raw.clone())
            {
                Some(raw) => is(Value::Styled(raw)),
                None => Got::Nothing,
            }]),
            slot if slot.starts_with("slot") => first(&[&|| self.var_text(slot)]),
            _ => Resolved::Unknown,
        }
    }

    /// A name no catalog entry has: a script or capture value, read the
    /// way the first renderer read prompt vars.
    fn resolve_other(&self, name: &str) -> Resolved {
        let Some(raw) = self.vars.var(name) else {
            return if self.vars.known_var(name) {
                Resolved::Missing
            } else {
                Resolved::Unknown
            };
        };
        let raw = raw.trim();
        if raw.is_empty() {
            return Resolved::Absent;
        }
        let Some(cur) = Value::parse_number(raw) else {
            return Resolved::Value(Value::Text(raw.to_string()));
        };
        let max = crate::render::max_spellings(name)
            .iter()
            .find_map(|key| self.vars.var(key).and_then(Value::parse_number));
        Resolved::Value(match max {
            Some(max) => cur.over(&max, None).unwrap_or(cur),
            None => cur,
        })
    }
}

/// A field's label for placeholders and the `on` and `off` formats.
fn label(f: &FieldRef, gmcp: Option<&Snapshot>) -> String {
    let words = |s: &str| s.trim().replace('_', " ");
    match field(f) {
        Some(Field::Entry(e)) => {
            if let Some(index) = e
                .name
                .strip_prefix("moon")
                .and_then(|n| n.parse::<usize>().ok())
            {
                let sent = gmcp
                    .and_then(|g| g.get(WORLD_MOONS))
                    .and_then(|d| d.get("moons"))
                    .and_then(Json::as_array)
                    .and_then(|m| m.get(index.saturating_sub(1)))
                    .and_then(|m| m.get("name"))
                    .and_then(gmcp::text);
                if let Some(name) = sent {
                    return name.to_string();
                }
            }
            e.label.to_string()
        }
        Some(Field::Aff(name)) => words(name),
        Some(Field::Member(stat, who)) => format!("{} {}", words(who), stat.word()),
        Some(Field::Queue(key)) => words(key),
        Some(Field::Gmcp(path)) => path.rsplit('.').next().map_or_else(
            || path.to_string(),
            |last| words(last.split('[').next().unwrap_or(last)),
        ),
        None => f.to_string(),
    }
}

impl Values for Resolver<'_> {
    fn resolve(&self, f: &FieldRef) -> Resolved {
        let hidden = self.vars.hidden;
        match field(f) {
            Some(Field::Entry(e)) => {
                if self.vars.name_hidden(e.name) {
                    return Resolved::Hidden;
                }
                self.resolve_entry(e)
            }
            Some(Field::Aff(name)) => {
                if hidden.affects {
                    return Resolved::Hidden;
                }
                self.aff(name)
            }
            Some(Field::Member(stat, who)) => {
                if hidden.group {
                    return Resolved::Hidden;
                }
                self.member(stat, who)
            }
            Some(Field::Queue(key)) => self.queue(key),
            Some(Field::Gmcp(path)) => {
                if self.path_hidden(path) {
                    return Resolved::Hidden;
                }
                self.gmcp_path(path)
            }
            None if f.param.is_some() => Resolved::Unknown,
            None => {
                if self.vars.name_hidden(&f.name) {
                    return Resolved::Hidden;
                }
                self.resolve_other(&f.name)
            }
        }
    }

    fn label(&self, f: &FieldRef) -> String {
        label(f, Some(self.gmcp()))
    }
}

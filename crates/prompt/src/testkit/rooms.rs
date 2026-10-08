//! Two rooms of The Eastern Road for the fake game to walk and goto.
//!
//! Room 6909, The Crossroads, and room 6910, Nearing the Crossroads, from
//! area/eastroad.are, as fixtures/room-colors/looks.json shows them. The
//! Crossroads holds Knight Fortress (army 12 in area/limbo.are), a sickle
//! node from `assign_gather_node` in skills3.c and mob 6904, which resets
//! there. Nearing the Crossroads holds nothing. Each look is the text of
//! `do_look` (`act_info.c`) up to its people, in the backtick codes
//! [`super::game::send_to_char`] turns into ANSI, and the Room.Info,
//! Room.Chars and Room.Items packets `do_look` sends right after the
//! people.

use std::fmt::Write as _;

use super::gmcp;

/// A room the fake knows.
pub(super) struct Room {
    pub(super) vnum: u32,
    /// The name with its color code, as the area file gives it.
    name: &'static str,
    /// The 256 color `do_look` puts before the name for the sector.
    tint: u8,
    /// The description as the area file wraps it.
    description: &'static [&'static str],
    exits: &'static str,
    /// The armies, things and people, in the order the look lists them.
    contents: &'static [&'static str],
    info: &'static str,
    chars: &'static str,
    items: &'static str,
    pub(super) east: Option<u32>,
    pub(super) west: Option<u32>,
    /// What the room prints after the look when you walk in, in the same
    /// pulse, the werebeast's greeting or the line `explore_room`
    /// (`explore.c`) prints on your first visit, which the fake prints on
    /// each one.
    pub(super) greet: &'static str,
}

const ROOMS: [Room; 2] = [
    Room {
        vnum: 6909,
        name: "`3The Crossroads``",
        tint: 82,
        description: &[
            "Here, the great eastern road intersects with a small, oder road that runs",
            "southward to Fort Blackwatch.  The ground around you is covered with young",
            "but healthy growth, mostly grasses and a few small bushes.  The outskirts of",
            "Val Miran lie to your west.  Eastward, the great Dragon's Teeth rise up",
            "almost immediately.  ",
        ],
        exits: "[Exits: north east south west]",
        contents: &[
            "A mighty Fortress looms over the area.``",
            "     A stand of blue dried leaves grows in the wild here.",
            "A young werebeast stands here, leaning on his spear.",
        ],
        info: r#"{"num":6909,"name":"The Crossroads","area":"The Eastern Road","terrain":"field","sector":2,"region":0,"climate":"Temperate","exits":{"north":6925,"east":6910,"south":5261,"west":6908}}"#,
        chars: r#"[{"name":"a werebeast","npc":true}]"#,
        items: r#"[{"name":"a stand of blue dried leaves","type":"plant"}]"#,
        east: Some(6910),
        west: None,
        greet: "A werebeast looks into the sky.\n\r",
    },
    Room {
        vnum: 6910,
        name: "`#Nearing the Crossroads``",
        tint: 220,
        description: &[
            "A rough gravel path leads on to the mountains to the east, rising up to",
            "the heavens, and to the long stretch of plains that eventually lead to Val",
            "Miran in the west.  ",
        ],
        exits: "[Exits: east west]",
        contents: &[],
        info: r#"{"num":6910,"name":"Nearing the Crossroads","area":"The Eastern Road","terrain":"desert","sector":10,"region":0,"climate":"Temperate","exits":{"east":6911,"west":6909}}"#,
        chars: "[]",
        items: "[]",
        east: None,
        west: Some(6909),
        greet: "`8You have explored a quarter of The Eastern Road.``\n\r",
    },
];

/// The room `vnum`, if the fake knows it.
pub(super) fn find(vnum: u32) -> Option<&'static Room> {
    ROOMS.iter().find(|room| room.vnum == vnum)
}

impl Room {
    /// The text of the look up to its people. An immortal with holylight
    /// sees the vnum after the name.
    pub(super) fn look(&self, holylight: bool) -> String {
        let mut text = format!("`({:03}){}``", self.tint, self.name);
        if holylight {
            let _ = write!(text, " [Room {}]", self.vnum);
        }
        text.push_str("\n\r  ");
        text.push_str(&self.description.join("\n\r"));
        text.push_str("\n\r\n\r");
        text.push_str(self.exits);
        text.push_str("\n\r");
        for line in self.contents {
            text.push_str(line);
            text.push_str("\n\r");
        }
        text
    }

    /// The packets `do_look` sends after the people.
    pub(super) fn packets(&self) -> Vec<u8> {
        let mut out = gmcp("Room.Info", self.info);
        out.extend(gmcp("Room.Chars", self.chars));
        out.extend(gmcp("Room.Items", self.items));
        out
    }
}

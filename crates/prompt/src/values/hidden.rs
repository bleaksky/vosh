//! What the game hides right now, worked out from the latest packets and
//! the fresh prompt values, and where a fresh capture disagrees with
//! GMCP.
//!
//! The lamented tears rule (H7 below) lives
//! here. The older server build sends true values under the song, and
//! only Char.Affects naming it tells Vosh the game means to hide them.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use super::catalog::Pair;
use super::changes::change_of;
use super::Vars;
use crate::aabahran::codes::Position;
use crate::values::format::lang_game;
use crate::values::gmcp::{self, Affects, CHAR_STATE, CHAR_WORTH, ROOM_WEATHER, WORLD_TIME};

/// Which values the game hides right now. Worked out from the latest
/// packets and the fresh prompt values, never stored.
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
    pub(crate) fn pair(&self, pair: Pair) -> bool {
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

    /// Nothing is hidden. Test only. The tests in `tests/` reach it
    /// through the `testkit` feature.
    #[cfg(any(test, feature = "testkit"))]
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

impl Vars {
    /// True when the name reads a value the game hides.
    pub(super) fn name_hidden(&self, name: &str) -> bool {
        if let Some(pair) = Pair::of(name).or_else(|| change_of(name).map(|(pair, _)| pair)) {
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
    // Hidden
    // -----------------------------------------------------------------

    pub(super) fn work_out_hidden(&self) -> Hidden {
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
        //
        // The Lament preview hides by a list of its own, `lament_hides` in
        // overrides.rs, since it shows what the song hides while no packet
        // names the song. The two differ on Char.Combat. Here a path that
        // reads the whole packet or the whole tank hides too, through the
        // resolver's `path_hidden`, and the preview hides only their health
        // and condition keys. Both stay as they are, since a merge would
        // change what the live prompt or the preview draws.
        let h7 = a_flag || affects.as_ref().is_some_and(names_lament);
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
    pub(super) fn disagreements_now(&self) -> Vec<&'static str> {
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

/// The song that hides your vitals, affects, group and your opponent's
/// condition while it is on you.
const LAMENT: &str = "lamented tears";

/// True when Char.Affects names lamented tears, in any case.
fn names_lament(affects: &Affects) -> bool {
    affects
        .list
        .iter()
        .any(|a| a.name.trim().eq_ignore_ascii_case(LAMENT))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::gmcp::Affect;

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
    fn the_song_is_found_by_name_in_any_case() {
        assert!(names_lament(&affects(&["bless", "lamented tears"])));
        assert!(names_lament(&affects(&["Lamented Tears"])));
        assert!(!names_lament(&affects(&["bless", "tears"])));
        assert!(!names_lament(&affects(&[])));
    }
}

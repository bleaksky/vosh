//! The resolver, which answers the renderer with each field's state from
//! the session's sources in order.

use serde_json::Value as Json;

use super::catalog::{entry, field, Entry, Field, MemberStat, Pair};
use super::changes::{change_of, Over};
use super::{max_spellings, since_of, ClientValues, Values, Vars};
use crate::aabahran::codes::{Position, PHASES};
use crate::design::FieldRef;
use crate::values::format::{lang_game, tank_bar_cells, Resolved, Value};
use crate::values::gmcp::{
    self, Find, Snapshot, CHAR_COMBAT, CHAR_STATUS, CHAR_VITALS, CHAR_WORTH, GROUP_INFO,
    IMM_QUEUES, ROOM_CHARS, ROOM_INFO, ROOM_ITEMS, ROOM_WEATHER, WORLD_MOONS, WORLD_TIME,
};

// ---------------------------------------------------------------------
// The resolver
// ---------------------------------------------------------------------

/// Answers the renderer for one draw.
pub struct Resolver<'a> {
    pub(super) vars: &'a Vars,
    pub(super) client: &'a ClientValues,
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

pub(super) fn pos_value(p: Position) -> Resolved {
    Resolved::Value(Value::Position(p))
}

/// A moon from what `%j` printed, `-` when it is not up.
pub(super) fn moon_code_value(code: &str) -> Resolved {
    let code = code.trim();
    if code == "-" {
        return Resolved::Value(Value::Moon {
            phase: 0,
            active: false,
            name: None,
        });
    }
    match PHASES[..8].iter().position(|c| *c == code) {
        Some(phase) => Resolved::Value(Value::Moon {
            phase: phase as u8,
            active: true,
            name: None,
        }),
        None => Resolved::Value(Value::Text(code.to_string())),
    }
}

/// Exits from what `%e` printed, `[Exits: N E (S) W]`, or bare letters.
pub(super) fn exits_value(text: &str) -> Value {
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

    pub(super) fn gauge(&self, pair: Pair, want_max: bool) -> Resolved {
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
        let interval = self.client.tick.and_then(|t| t.interval);
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
                            since: since_of(secs, interval),
                        })
                    },
                ),
            },
            &|| match self.client.tick {
                Some(t) => is(Value::Seconds {
                    secs: t.remaining,
                    max: t.interval,
                    since: t.since,
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
            .client
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
        // would not show.
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

    /// A change of a vital, a script value first. A change over a tick
    /// needs the tick running, and none is Absent.
    fn change(&self, name: &str, pair: Pair, over: Over) -> Resolved {
        first(&[
            &|| match self.vars.var(name).map(str::trim) {
                None => Got::Nothing,
                Some("") => Got::Blank,
                Some(t) => t
                    .parse()
                    .map_or_else(|_| is(Value::Text(t.to_string())), |n| is(Value::Change(n))),
            },
            &|| {
                let ticks = over == Over::Pulse || self.client.tick.is_some();
                match self.vars.change(pair, over).filter(|_| ticks) {
                    Some(n) => is(Value::Change(n)),
                    None => Got::Is(Resolved::Absent),
                }
            },
        ])
    }

    fn resolve_entry(&self, e: &'static Entry) -> Resolved {
        let name = e.name;
        if let Some(pair) = Pair::of(name) {
            return self.gauge(pair, name == pair.max());
        }
        if let Some((pair, over)) = change_of(name) {
            return self.change(name, pair, over);
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
                    .client
                    .now
                    .unwrap_or_else(|| chrono::Local::now().naive_local()),
                date: name == "date",
            }),
            "target" => first(&[&|| self.var_text("target"), &|| match self
                .client
                .target
                .as_deref()
                .map(str::trim)
            {
                Some(t) if !t.is_empty() => is(Value::Text(t.to_string())),
                _ => Got::Is(Resolved::Absent),
            }]),
            "profile" => first(&[&|| self.var_text("profile"), &|| match self
                .client
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
        let max = max_spellings(name)
            .iter()
            .find_map(|key| self.vars.var(key).and_then(Value::parse_number));
        Resolved::Value(match max {
            Some(max) => cur.over(&max, None).unwrap_or(cur),
            None => cur,
        })
    }
}

/// A field's label for placeholders and the `on` and `off` formats.
pub(super) fn label(f: &FieldRef, gmcp: Option<&Snapshot>) -> String {
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

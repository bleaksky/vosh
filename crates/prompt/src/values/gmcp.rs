//! The latest GMCP packet of each package, for the resolver.
//!
//! Each packet replaces the one before it whole, so a field a packet
//! leaves out is gone. Package names match in any case. The snapshot also
//! keeps the pulse, which a Char.Vitals packet starts, and the latest
//! Char.Prompt with the time it came and whether it was the first since
//! the socket connected. A Char.Prompt this session is the sign of the
//! new server build, which [`crate::values`] reads behind the
//! Forsaken Lands rules.
//!
//! Paths reach into any packet, `Char.Affects.affects[name=sanctuary].level`,
//! with `[key=value]` picking the first array entry whose `key` matches and
//! `[N]` picking an entry by index.

use std::collections::BTreeMap;

use chrono::{DateTime, FixedOffset};
use serde_json::Value;

/// Aabahran's vitals, one packet per prompt. It starts a pulse.
pub(crate) const CHAR_VITALS: &str = "Char.Vitals";
pub(crate) const CHAR_AFFECTS: &str = "Char.Affects";
pub(crate) const CHAR_COMBAT: &str = "Char.Combat";
pub(crate) const CHAR_PROMPT: &str = "Char.Prompt";
pub(crate) const CHAR_STATE: &str = "Char.State";
pub(crate) const CHAR_STATUS: &str = "Char.Status";
pub(crate) const CHAR_WORTH: &str = "Char.Worth";
pub(crate) const GROUP_INFO: &str = "Group.Info";
pub(crate) const ROOM_INFO: &str = "Room.Info";
pub(crate) const ROOM_WEATHER: &str = "Room.Weather";
pub(crate) const ROOM_CHARS: &str = "Room.Chars";
pub(crate) const ROOM_ITEMS: &str = "Room.Items";
pub(crate) const WORLD_TIME: &str = "World.Time";
pub(crate) const WORLD_MOONS: &str = "World.Moons";
pub(crate) const IMM_QUEUES: &str = "Imm.Queues";

/// One packet as it came.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Packet {
    /// The package name as the game spelled it.
    pub package: String,
    pub data: Value,
}

/// The game's prompt settings, from Char.Prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharPrompt {
    /// The text prompt is on. `prompt off` sends false.
    pub enabled: bool,
    /// The PROMPT string as the game stores it, backtick colors kept.
    pub prompt: String,
    /// The fight prompt, empty when none is set.
    pub fprompt: String,
    /// When it came.
    pub at: DateTime<FixedOffset>,
    /// The first Char.Prompt since the socket connected, which the game
    /// sends at login.
    pub at_login: bool,
}

/// What a packet did besides replacing its package.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Observed {
    /// It started a pulse.
    pub pulse: bool,
    /// It was a Char.Prompt, parsed.
    pub prompt: Option<CharPrompt>,
}

/// The latest packet per package, the pulse and the latest Char.Prompt.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    packets: BTreeMap<String, Packet>,
    pulse: u64,
    vitals_seen: bool,
    prompt: Option<CharPrompt>,
}

impl Snapshot {
    /// An empty snapshot. Test only. The tests in `tests/` reach it
    /// through the `testkit` feature.
    #[cfg(any(test, feature = "testkit"))]
    pub fn new() -> Self {
        Self::default()
    }

    /// Keep a packet, replacing the last one of its package. A Char.Vitals
    /// starts a pulse. A Char.Prompt is parsed and kept with `at`.
    pub fn observe(&mut self, package: &str, data: Value, at: DateTime<FixedOffset>) -> Observed {
        let mut observed = Observed::default();
        if package.eq_ignore_ascii_case(CHAR_VITALS) {
            self.pulse += 1;
            self.vitals_seen = true;
            observed.pulse = true;
        }
        if package.eq_ignore_ascii_case(CHAR_PROMPT) {
            if let Some(obj) = data.as_object() {
                let string = |key: &str| {
                    obj.get(key)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string()
                };
                let prompt = CharPrompt {
                    enabled: obj.get("enabled").and_then(Value::as_bool).unwrap_or(true),
                    prompt: string("prompt"),
                    fprompt: string("fprompt"),
                    at,
                    at_login: self.prompt.is_none(),
                };
                self.prompt = Some(prompt.clone());
                observed.prompt = Some(prompt);
            }
        }
        self.packets.insert(
            package.to_ascii_lowercase(),
            Packet {
                package: package.to_string(),
                data,
            },
        );
        observed
    }

    /// Note one of your own sends. On a server that has sent no Char.Vitals
    /// this session each send starts a pulse. True when it did.
    pub(crate) fn on_send(&mut self) -> bool {
        if self.vitals_seen {
            return false;
        }
        self.pulse += 1;
        true
    }

    /// The pulse count this session. A capture is fresh while it carries
    /// the current pulse.
    pub fn pulse(&self) -> u64 {
        self.pulse
    }

    /// The latest packet of a package, in any case.
    pub(crate) fn packet(&self, package: &str) -> Option<&Packet> {
        self.packets.get(&package.to_ascii_lowercase())
    }

    /// The data of the latest packet of a package, in any case.
    pub fn get(&self, package: &str) -> Option<&Value> {
        self.packet(package).map(|p| &p.data)
    }

    /// True once a package has arrived this session.
    pub fn has(&self, package: &str) -> bool {
        self.packet(package).is_some()
    }

    /// The packages seen this session, as the game spelled them.
    pub(crate) fn packages(&self) -> impl Iterator<Item = &str> {
        self.packets.values().map(|p| p.package.as_str())
    }

    /// The latest Char.Prompt.
    pub fn char_prompt(&self) -> Option<&CharPrompt> {
        self.prompt.as_ref()
    }

    /// A Char.Prompt has come since the socket connected.
    pub fn prompt_seen(&self) -> bool {
        self.prompt.is_some()
    }

    /// Forget everything, as a disconnect does.
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    /// Follow a path into the packets. The longest run of leading names
    /// that is a package seen this session picks the packet, and the
    /// rest walks its fields. Names match in any case.
    pub(crate) fn find(&self, path: &str) -> Find<'_> {
        let segments = split_path(path);
        for split in (1..=segments.len()).rev() {
            let name = segments[..split]
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
                .join(".");
            let Some(packet) = self.packet(&name) else {
                continue;
            };
            let keys: Vec<String> = segments[split..]
                .iter()
                .filter(|s| !s.name.is_empty())
                .map(|s| s.name.to_ascii_lowercase())
                .collect();
            let mut value = select(&packet.data, &segments[split - 1].selectors);
            for segment in &segments[split..] {
                value = value
                    .and_then(|v| {
                        if segment.name.is_empty() {
                            Some(v)
                        } else {
                            key(v, &segment.name)
                        }
                    })
                    .and_then(|v| select(v, &segment.selectors));
            }
            return match value {
                Some(value) => Find::Found {
                    package: &packet.package,
                    keys,
                    value,
                },
                None => Find::Missing {
                    package: &packet.package,
                    keys,
                },
            };
        }
        Find::NoPacket
    }

    /// Char.Vitals, parsed.
    pub fn vitals(&self) -> Option<Vitals> {
        let data = self.get(CHAR_VITALS)?;
        let n = |key| data.get(key).and_then(int);
        Some(Vitals {
            hp: n("hp"),
            maxhp: n("maxhp"),
            mana: n("mana"),
            maxmana: n("maxmana"),
            moves: n("move"),
            maxmove: n("maxmove"),
            hidden: hidden_flag(data),
        })
    }

    /// Char.Combat, parsed.
    pub fn combat(&self) -> Option<Combat> {
        let data = self.get(CHAR_COMBAT)?;
        let tank = data.get("tank").and_then(|t| {
            Some(Tank {
                name: text(t.get("name")?)?.to_string(),
                hp_pct: t.get("hp_pct").and_then(int),
            })
        });
        Some(Combat {
            target: data.get("target").and_then(text).map(str::to_string),
            condition: data.get("condition").and_then(text).map(str::to_string),
            hp_pct: data.get("hp_pct").and_then(int),
            hidden: hidden_flag(data),
            tank,
            empty: data.as_object().map_or(true, serde_json::Map::is_empty),
        })
    }

    /// True while the latest Char.Combat names a target, so you are in a
    /// fight. The game sends `{}` once the fight is over.
    pub fn fighting(&self) -> bool {
        self.get(CHAR_COMBAT)
            .and_then(|data| data.get("target"))
            .and_then(text)
            .is_some()
    }

    /// Group.Info, parsed. Rows that repeat a member (the same id, or the
    /// same name without an id) fold into one, keeping the last row in
    /// the place of the first, as the Group pane does.
    pub fn group(&self) -> Option<Group> {
        let data = self.get(GROUP_INFO)?;
        let mut members: Vec<Member> = Vec::new();
        for row in data
            .get("members")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if !row.is_object() {
                continue;
            }
            let member = Member {
                id: row.get("id").and_then(int),
                name: row.get("name").and_then(text).map(str::to_string),
                level: row.get("level").and_then(int),
                class: row.get("class").and_then(text).map(str::to_string),
                hp_pct: row.get("hp_pct").and_then(int),
                mana_pct: row.get("mana_pct").and_then(int),
                move_pct: row.get("move_pct").and_then(int),
                tnl: row.get("tnl").and_then(int),
            };
            match members.iter_mut().find(|m| m.key() == member.key()) {
                Some(seen) => *seen = member,
                None => members.push(member),
            }
        }
        Some(Group {
            leader: data.get("leader").and_then(text).map(str::to_string),
            members,
            hidden: hidden_flag(data),
            empty: data.as_object().map_or(true, serde_json::Map::is_empty),
        })
    }

    /// Char.Affects, parsed.
    pub fn affects(&self) -> Option<Affects> {
        let data = self.get(CHAR_AFFECTS)?;
        let list = data
            .get("affects")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|a| {
                Some(Affect {
                    name: text(a.get("name")?)?.to_string(),
                    kind: a.get("kind").and_then(text).map(str::to_string),
                    duration: a.get("duration").and_then(int),
                })
            })
            .collect();
        Some(Affects {
            list,
            hidden: hidden_flag(data),
        })
    }

    /// Char.State, parsed. The new build sends it with every prompt.
    pub fn state(&self) -> Option<State> {
        let data = self.get(CHAR_STATE)?;
        Some(State {
            position: data.get("position").and_then(text).map(str::to_string),
            language: data.get("language").and_then(text).map(str::to_string),
        })
    }

    /// Room.Weather, parsed. The new build sends it with every prompt.
    pub fn weather(&self) -> Option<Weather> {
        let data = self.get(ROOM_WEATHER)?;
        Some(Weather {
            sky: data.get("sky").and_then(text).map(str::to_string),
            temp: data.get("temp").and_then(int),
            unit: data
                .get("unit")
                .and_then(text)
                .and_then(|u| u.chars().next()),
            region: data.get("region").and_then(text).map(str::to_string),
        })
    }

    /// The names in a list package such as Room.Chars or Room.Items.
    pub(crate) fn names(&self, package: &str) -> Option<Vec<String>> {
        let data = self.get(package)?;
        Some(
            data.as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| e.get("name").and_then(text).map(str::to_string))
                .collect(),
        )
    }
}

/// Where a path led.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Find<'a> {
    /// No package on the path has arrived this session.
    NoPacket,
    /// The packet came but has nothing at that path.
    Missing { package: &'a str, keys: Vec<String> },
    Found {
        /// The package as the game spelled it.
        package: &'a str,
        /// The field names walked after the package, lowercased.
        keys: Vec<String>,
        value: &'a Value,
    },
}

/// Char.Vitals.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Vitals {
    pub hp: Option<i64>,
    pub maxhp: Option<i64>,
    pub mana: Option<i64>,
    pub maxmana: Option<i64>,
    pub moves: Option<i64>,
    pub maxmove: Option<i64>,
    pub hidden: bool,
}

/// Char.Combat. `target` is who you fight, the opponent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Combat {
    pub target: Option<String>,
    pub condition: Option<String>,
    pub hp_pct: Option<i64>,
    pub hidden: bool,
    pub tank: Option<Tank>,
    /// The packet was `{}`, the fight is over.
    pub empty: bool,
}

/// The groupmate your opponent hits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tank {
    pub name: String,
    pub hp_pct: Option<i64>,
}

/// Group.Info.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Group {
    pub leader: Option<String>,
    pub members: Vec<Member>,
    pub hidden: bool,
    /// The packet was `{}`, which is solo, and under lamented tears on the
    /// build that sent no flag.
    pub empty: bool,
}

/// One row of Group.Info.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Member {
    pub id: Option<i64>,
    pub name: Option<String>,
    pub level: Option<i64>,
    pub class: Option<String>,
    pub hp_pct: Option<i64>,
    pub mana_pct: Option<i64>,
    pub move_pct: Option<i64>,
    pub tnl: Option<i64>,
}

impl Member {
    /// The id when the row has one, else the name.
    fn key(&self) -> (Option<i64>, Option<&str>) {
        match self.id {
            Some(id) => (Some(id), None),
            None => (None, Some(self.name.as_deref().unwrap_or("?"))),
        }
    }
}

/// Char.Affects.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Affects {
    pub list: Vec<Affect>,
    pub hidden: bool,
}

/// One affect.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Affect {
    pub name: String,
    pub kind: Option<String>,
    /// Ticks left, -1 for permanent.
    pub duration: Option<i64>,
}

/// Char.State. `position` is the game's word, `standing` or
/// `mortally wounded`, and `language` is spelled as the game stores it,
/// `Thsu'ul`. A switched immortal sends no language.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub position: Option<String>,
    pub language: Option<String>,
}

/// Room.Weather, what the prompt's `%W`, `%w` and `%G` print. `sky` has
/// `%W`'s words, `indoors` inside, and `unit` is `F` or `C`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Weather {
    pub sky: Option<String>,
    pub temp: Option<i64>,
    pub unit: Option<char>,
    pub region: Option<String>,
}

/// True when a packet carries `"hidden": true`.
pub(crate) fn hidden_flag(data: &Value) -> bool {
    data.get("hidden").and_then(Value::as_bool) == Some(true)
}

/// A whole number sent as a number or a numeric string.
pub(crate) fn int(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n
            .as_i64()
            .or_else(|| n.as_f64().filter(|f| f.is_finite()).map(|f| f as i64)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// A string with something in it, trimmed.
pub(crate) fn text(value: &Value) -> Option<&str> {
    value.as_str().map(str::trim).filter(|s| !s.is_empty())
}

/// A field of an object by name, exact first and then in any case.
fn key<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
    let obj = value.as_object()?;
    obj.get(name).or_else(|| {
        obj.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v)
    })
}

/// Words match in any case, with `_` standing for a space.
pub(crate) fn same_words(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.trim().replace('_', " ").to_lowercase();
    norm(a) == norm(b)
}

/// Apply `[N]` and `[key=value]` selectors in turn.
fn select<'a>(value: &'a Value, selectors: &[String]) -> Option<&'a Value> {
    let mut value = value;
    for selector in selectors {
        let items = value.as_array()?;
        value = match selector.split_once('=') {
            Some((field, want)) => items.iter().find(|item| {
                key(item, field).is_some_and(|v| match v {
                    Value::String(s) => same_words(s, want),
                    Value::Number(_) => int(v).is_some() && int(v) == want.trim().parse().ok(),
                    Value::Bool(b) => want.trim().parse::<bool>().ok() == Some(*b),
                    _ => false,
                })
            })?,
            None => items.get(selector.trim().parse::<usize>().ok()?)?,
        };
    }
    Some(value)
}

/// One `.` separated part of a path, its name and any selectors after it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Segment {
    name: String,
    selectors: Vec<String>,
}

/// Split a path at the dots outside brackets.
fn split_path(path: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut name = String::new();
    let mut selectors = Vec::new();
    let mut chars = path.trim().chars();
    while let Some(c) = chars.next() {
        match c {
            '.' => segments.push(Segment {
                name: std::mem::take(&mut name),
                selectors: std::mem::take(&mut selectors),
            }),
            '[' => {
                let selector: String = chars.by_ref().take_while(|c| *c != ']').collect();
                selectors.push(selector);
            }
            _ => name.push(c),
        }
    }
    segments.push(Segment { name, selectors });
    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn at(minute: u32) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(&format!("2026-09-29T12:{minute:02}:00-05:00"))
            .expect("a valid time")
    }

    #[test]
    fn each_packet_replaces_its_package_whole() {
        let mut s = Snapshot::new();
        s.observe(
            "Char.Combat",
            json!({"target": "a guard", "hp_pct": 54}),
            at(0),
        );
        s.observe("Char.Combat", json!({"target": "a guard"}), at(0));
        assert_eq!(s.get("char.combat"), Some(&json!({"target": "a guard"})));
        assert_eq!(s.combat().and_then(|c| c.hp_pct), None);
        assert!(s.has("CHAR.COMBAT"));
        assert!(!s.has("Char.Vitals"));
    }

    #[test]
    fn you_fight_while_char_combat_names_a_target() {
        let packet = |line: &str| {
            let (package, data) = line.trim().split_once(' ').expect("a packet");
            (
                package.to_string(),
                serde_json::from_str::<Value>(data).expect("json"),
            )
        };
        let mut s = Snapshot::new();
        assert!(!s.fighting(), "no Char.Combat yet");
        for (fixture, fighting) in [
            (
                include_str!("../../../../fixtures/gmcp/aabahran/char-combat.gmcp"),
                true,
            ),
            (
                include_str!("../../../../fixtures/gmcp/aabahran/char-combat-hidden.gmcp"),
                true,
            ),
            (
                include_str!("../../../../fixtures/gmcp/aabahran/char-combat-withheld.gmcp"),
                true,
            ),
            (
                include_str!("../../../../fixtures/gmcp/aabahran/char-combat-tank.gmcp"),
                true,
            ),
            (
                include_str!("../../../../fixtures/gmcp/aabahran/char-combat-end.gmcp"),
                false,
            ),
        ] {
            let (package, data) = packet(fixture);
            s.observe(&package, data, at(0));
            assert_eq!(s.fighting(), fighting, "{fixture}");
        }
        s.observe("Char.Combat", json!({"target": "  "}), at(0));
        assert!(!s.fighting(), "a blank target names no one");
    }

    #[test]
    fn char_vitals_starts_a_pulse_and_sends_stand_down() {
        let mut s = Snapshot::new();
        // Before any Char.Vitals, each send starts a pulse.
        assert!(s.on_send());
        assert_eq!(s.pulse(), 1);
        let observed = s.observe("Char.Vitals", json!({"hp": 1}), at(0));
        assert!(observed.pulse);
        assert_eq!(s.pulse(), 2);
        // Once Char.Vitals has come, only Char.Vitals starts one.
        assert!(!s.on_send());
        assert_eq!(s.pulse(), 2);
        assert!(!s.observe("Char.Worth", json!({"gold": 5}), at(0)).pulse);
        assert_eq!(s.pulse(), 2);
    }

    #[test]
    fn char_prompt_keeps_its_time_and_whether_it_came_at_login() {
        let mut s = Snapshot::new();
        assert!(!s.prompt_seen());
        let first = s
            .observe(
                "Char.Prompt",
                json!({"enabled": true, "prompt": "%h ", "fprompt": ""}),
                at(1),
            )
            .prompt
            .expect("a prompt");
        assert!(first.at_login);
        assert_eq!(first.prompt, "%h ");
        assert_eq!(first.at, at(1));
        let later = s
            .observe(
                "Char.Prompt",
                json!({"enabled": false, "prompt": "%h ", "fprompt": ""}),
                at(9),
            )
            .prompt
            .expect("a prompt");
        assert!(!later.at_login);
        assert!(!later.enabled);
        assert_eq!(s.char_prompt(), Some(&later));
        assert!(s.prompt_seen());
        s.clear();
        assert!(!s.prompt_seen());
        assert_eq!(s.pulse(), 0);
        assert!(!s.has("Char.Prompt"));
    }

    #[test]
    fn paths_reach_fields_array_entries_and_any_case() {
        let mut s = Snapshot::new();
        s.observe(
            "Char.Affects",
            json!({"affects": [
                {"name": "bless", "level": 50},
                {"name": "giant strength", "level": 45, "modifier": 2}
            ]}),
            at(0),
        );
        s.observe(
            "Imm.Queues",
            json!({"bugs": 3, "journals": {"unread": 2}}),
            at(0),
        );
        s.observe("Room.Chars", json!([{"name": "a guard"}]), at(0));

        let found = |path: &str| match s.find(path) {
            Find::Found { value, .. } => Some(value.clone()),
            _ => None,
        };
        assert_eq!(
            found("Char.Affects.affects[name=giant_strength].level"),
            Some(json!(45))
        );
        assert_eq!(
            found("char.affects.AFFECTS[NAME=Giant_Strength].modifier"),
            Some(json!(2))
        );
        assert_eq!(found("Char.Affects.affects[0].name"), Some(json!("bless")));
        assert_eq!(found("Imm.Queues.journals.unread"), Some(json!(2)));
        assert_eq!(found("Imm.Queues.bugs"), Some(json!(3)));
        assert_eq!(found("Room.Chars[0].name"), Some(json!("a guard")));
        assert_eq!(
            s.find("Char.Affects.affects[name=sanctuary].level"),
            Find::Missing {
                package: "Char.Affects",
                keys: vec!["affects".to_string(), "level".to_string()],
            }
        );
        assert_eq!(s.find("Char.Vitals.hp"), Find::NoPacket);
        match s.find("Char.Affects.affects[1].level") {
            Find::Found { package, keys, .. } => {
                assert_eq!(package, "Char.Affects");
                assert_eq!(keys, vec!["affects".to_string(), "level".to_string()]);
            }
            other => panic!("expected a value, got {other:?}"),
        }
    }

    #[test]
    fn group_rows_that_repeat_a_member_fold_into_one() {
        let mut s = Snapshot::new();
        s.observe(
            "Group.Info",
            json!({"leader": "Tester", "members": [
                {"id": 1, "name": "Tester", "hp_pct": 90},
                {"id": 2, "name": "someone", "hp_pct": 50},
                {"id": 3, "name": "someone", "hp_pct": 40},
                {"id": 1, "name": "Tester", "hp_pct": 80}
            ]}),
            at(0),
        );
        let group = s.group().expect("a group");
        let rows: Vec<(Option<i64>, Option<i64>)> =
            group.members.iter().map(|m| (m.id, m.hp_pct)).collect();
        assert_eq!(
            rows,
            vec![
                (Some(1), Some(80)),
                (Some(2), Some(50)),
                (Some(3), Some(40))
            ]
        );
        assert!(!group.empty);
        s.observe("Group.Info", json!({}), at(0));
        assert!(s.group().is_some_and(|g| g.empty && g.members.is_empty()));
    }

    #[test]
    fn state_and_weather_read_their_fields() {
        let mut s = Snapshot::new();
        assert_eq!(s.state(), None);
        s.observe(
            "Char.State",
            json!({"position": "mortally wounded", "language": "Thsu'ul"}),
            at(0),
        );
        s.observe(
            "Room.Weather",
            json!({"sky": "rainy", "temp": 60, "unit": "F", "region": "Coastal North"}),
            at(0),
        );
        let state = s.state().expect("a state");
        assert_eq!(state.position.as_deref(), Some("mortally wounded"));
        assert_eq!(state.language.as_deref(), Some("Thsu'ul"));
        let weather = s.weather().expect("the weather");
        assert_eq!(weather.sky.as_deref(), Some("rainy"));
        assert_eq!(weather.temp, Some(60));
        assert_eq!(weather.unit, Some('F'));
        assert_eq!(weather.region.as_deref(), Some("Coastal North"));
        s.observe(
            "Char.State",
            json!({"position": "standing", "language": ""}),
            at(0),
        );
        assert_eq!(s.state().and_then(|st| st.language), None);
    }

    #[test]
    fn numbers_come_as_numbers_or_numeric_strings() {
        assert_eq!(int(&json!(5)), Some(5));
        assert_eq!(int(&json!("12")), Some(12));
        assert_eq!(int(&json!(" -3 ")), Some(-3));
        assert_eq!(int(&json!(2.9)), Some(2));
        assert_eq!(int(&json!("x")), None);
        assert_eq!(int(&json!(true)), None);
        assert_eq!(text(&json!("  a  ")), Some("a"));
        assert_eq!(text(&json!("")), None);
        assert!(same_words("Giant_Strength", "giant strength"));
    }
}

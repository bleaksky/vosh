//! The catalog's samples as values, for previews with no live data.

use chrono::NaiveDateTime;

use super::catalog::{field, Field, Kind, MemberStat, CATALOG};
use super::resolver::{exits_value, label, moon_code_value, pos_value};
use super::{since_of, Values};
use crate::aabahran::codes::{Position, PHASES};
use crate::design::FieldRef;
use crate::values::format::{Resolved, Value};

// ---------------------------------------------------------------------
// Samples
// ---------------------------------------------------------------------

/// A value of `kind` from text in the form the catalog's samples take:
/// `1020/1020` for a gauge, `14/60` for the tick, a word for a position,
/// a phase number for a moon, names joined by commas for a count. Empty
/// text is Absent, but a clock reads `now`. `label` names an immortal
/// level, `Wizi` or `Incog`.
pub(crate) fn value_of(kind: Kind, label: &str, text: &str, now: NaiveDateTime) -> Resolved {
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
            let secs = num(secs).unwrap_or(0);
            let max = num(max);
            Value::Seconds {
                secs,
                max,
                since: since_of(secs, max),
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
        Kind::Moon => return moon_code_value(PHASES[num(s).unwrap_or(0) as usize % 8]),
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
        Kind::Change => Value::Change(num(s).unwrap_or(0)),
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

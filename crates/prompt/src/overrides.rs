//! Values a render uses in place of the live ones (sections 4 and 6):
//! the card's previews, Low health, Fight and Lament, and the samples
//! the start list draws. Overrides never reach `session://prompt-vars`,
//! so the panes keep the live values.

use std::collections::BTreeMap;

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::format::{Resolved, Value};
use crate::render::Values;
use crate::template::FieldRef;
use crate::vars::{self, Kind, Pair};

/// What a preview draws in place of the live values.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Overrides {
    /// Values by field, `hp` or `aff:sanctuary`, in the form the
    /// catalog's samples take: `180` for health keeps its live max,
    /// `180/1020` sets both, a word for a position, names joined by commas
    /// for a count. A number or `true` reads as text. `"?"` draws the
    /// field hidden, and `null`, `false` or empty text draws it absent.
    #[serde(default)]
    pub values: BTreeMap<String, Json>,
    /// Hide every value lamented tears hides.
    #[serde(default)]
    pub lament: bool,
}

impl Overrides {
    /// True when the overrides change nothing.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty() && !self.lament
    }

    /// The override for `field`, matched by name or alias in any case,
    /// and by parameter in any case.
    fn find(&self, field: &FieldRef) -> Option<&Json> {
        let same_name = |key: &str| {
            let key = key.to_ascii_lowercase();
            key == field.name
                || matches!(
                    (vars::entry(&key), vars::entry(&field.name)),
                    (Some(a), Some(b)) if a.name == b.name
                )
        };
        self.values.iter().find_map(|(key, value)| {
            let (name, param) = match key.split_once(':') {
                Some((name, param)) => (name, Some(param)),
                None => (key.as_str(), None),
            };
            let param_matches = match (param, &field.param) {
                (None, None) => true,
                (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                _ => false,
            };
            (param_matches && same_name(name)).then_some(value)
        })
    }
}

/// True for a field lamented tears hides: your vitals, your tank's and
/// your opponent's health, your affects and your group (section 1.2).
pub fn lament_hides(field: &FieldRef) -> bool {
    let name = field.name.as_str();
    if Pair::of(name).is_some() {
        return true;
    }
    match (&field.param, name) {
        (
            None,
            "tank_hp" | "tank_pct" | "tank_bar" | "opponent_hp" | "opponent_cond" | "missing"
            | "leader" | "group_size" | "group_low",
        )
        | (Some(_), "aff") => true,
        (Some(_), member) if member.starts_with("member_") => true,
        (Some(path), "gmcp") => {
            let path = path.to_ascii_lowercase();
            ["char.vitals", "char.affects", "group.info"]
                .iter()
                .any(|p| path.starts_with(p))
                || [
                    "char.combat.hp_pct",
                    "char.combat.condition",
                    "char.combat.tank.hp_pct",
                ]
                .iter()
                .any(|p| path.starts_with(p))
        }
        _ => false,
    }
}

/// Values with overrides on top.
pub struct Overridden<'a> {
    inner: &'a dyn Values,
    overrides: &'a Overrides,
    now: NaiveDateTime,
}

impl<'a> Overridden<'a> {
    /// `inner` with `overrides` on top. `now` fills a clock an override
    /// names.
    pub fn new(inner: &'a dyn Values, overrides: &'a Overrides, now: NaiveDateTime) -> Self {
        Self {
            inner,
            overrides,
            now,
        }
    }

    /// An override's text as a value of the field's kind.
    fn value(&self, field: &FieldRef, json: &Json) -> Resolved {
        let text = match json {
            Json::Null | Json::Bool(false) => return Resolved::Absent,
            Json::Bool(true) => "1".to_string(),
            Json::Number(n) => n.to_string(),
            Json::String(s) if s.trim() == "?" => return Resolved::Hidden,
            Json::String(s) => s.trim().to_string(),
            other => other.to_string(),
        };
        if text.is_empty() {
            return Resolved::Absent;
        }
        let Some(entry) = vars::entry_for(field) else {
            return Resolved::Value(
                Value::parse_number(&text).unwrap_or_else(|| Value::Text(text.clone())),
            );
        };
        match entry.kind {
            Kind::Flag => {
                let on = !matches!(
                    text.to_ascii_lowercase().as_str(),
                    "0" | "false" | "no" | "off"
                );
                if on {
                    Resolved::Value(Value::Flag)
                } else {
                    Resolved::Absent
                }
            }
            Kind::Count if text == "0" => Resolved::Absent,
            Kind::Gauge if !text.contains('/') => {
                let cur = vars::value_of(Kind::Gauge, entry.label, &text, self.now);
                let Resolved::Value(Value::Gauge { cur, .. }) = cur else {
                    return cur;
                };
                // The live max stays, as Low health keeps it.
                let max = match self.inner.resolve(field) {
                    Resolved::Value(Value::Gauge { max, .. }) => max,
                    _ => Pair::of(&field.name).and_then(|pair| {
                        match self.inner.resolve(&FieldRef::new(pair.max())) {
                            Resolved::Value(Value::Num(max)) => Some(max),
                            _ => None,
                        }
                    }),
                };
                Resolved::Value(Value::Gauge {
                    cur,
                    max,
                    pct: None,
                })
            }
            kind => vars::value_of(kind, entry.label, &text, self.now),
        }
    }
}

impl Values for Overridden<'_> {
    fn resolve(&self, field: &FieldRef) -> Resolved {
        if let Some(json) = self.overrides.find(field) {
            return self.value(field, json);
        }
        if self.overrides.lament && lament_hides(field) {
            return Resolved::Hidden;
        }
        self.inner.resolve(field)
    }

    fn label(&self, field: &FieldRef) -> String {
        self.inner.label(field)
    }
}

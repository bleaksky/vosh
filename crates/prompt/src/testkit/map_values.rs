//! Values from a plain map of prompt vars, for tests that draw a template
//! without a session.

use std::collections::BTreeMap;

use chrono::NaiveDateTime;

use crate::design::FieldRef;
use crate::render::{max_spellings, Values};
use crate::values::format::{Resolved, Value};

/// Values from a plain map of prompt vars, the way the first renderer read
/// them. A number with a max under any of its spellings (`mhp`, `hp_max`,
/// `max_hp`, `maxhp`) is a gauge. A name the map lacks is unknown, so its
/// token prints as written. `time` and `date` read the clock given.
pub struct MapValues<'a> {
    vars: &'a BTreeMap<String, String>,
    now: NaiveDateTime,
}

impl<'a> MapValues<'a> {
    pub fn new(vars: &'a BTreeMap<String, String>, now: NaiveDateTime) -> Self {
        Self { vars, now }
    }

    fn number(&self, key: &str) -> Option<Value> {
        Value::parse_number(self.vars.get(key)?)
    }

    fn max_of(&self, name: &str) -> Option<Value> {
        max_spellings(name).iter().find_map(|key| self.number(key))
    }
}

impl Values for MapValues<'_> {
    fn resolve(&self, field: &FieldRef) -> Resolved {
        if field.param.is_some() {
            return Resolved::Unknown;
        }
        let name = field.name.as_str();
        match name {
            "time" | "date" => {
                return Resolved::Value(Value::Clock {
                    at: self.now,
                    date: name == "date",
                })
            }
            _ => {}
        }
        let Some(raw) = self.vars.get(name) else {
            return Resolved::Unknown;
        };
        Resolved::Value(match Value::parse_number(raw) {
            Some(cur) => match self.max_of(name) {
                Some(max) => cur.over(&max, None).unwrap_or(cur),
                None => cur,
            },
            None => Value::Text(raw.clone()),
        })
    }
}

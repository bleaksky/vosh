//! Values a render uses in place of the live ones (sections 4 and 6):
//! the card's previews, Low health, Fight and Lament, and the samples
//! the start list draws. Overrides never reach `session://prompt-vars`,
//! so the panes keep the live values.
//!
//! [`PromptPreview`] is what the open card shows on your prompt in place
//! of the live render: one of its [`Preview`]s, values on top, the labels
//! of values with nothing to show, or the game's own line.

use std::collections::BTreeMap;

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::design::FieldRef;
use crate::values::format::{Resolved, Value};
use crate::values::{self, Kind, Pair, Values};

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
                    (values::entry(&key), values::entry(&field.name)),
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

/// One of the previews the card's footer offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preview {
    /// The live values.
    Now,
    /// Health at 180, the maxes kept.
    LowHealth,
    /// A sample fight: the opponent Blackwatch Guard at 60 percent with
    /// quite a few wounds, and you the tank.
    Fight,
    /// Every value lamented tears hides, hidden.
    Lament,
}

/// The health Low health draws.
const LOW_HEALTH: i64 = 180;

impl Preview {
    /// The values the preview draws in place of `live`'s. Fight makes you
    /// the tank: your name, and your health as the game counts a tank's,
    /// from `live`, or the catalog's samples when it has neither.
    pub(crate) fn overrides(self, live: &dyn Values) -> Overrides {
        let mut values = BTreeMap::new();
        let mut lament = false;
        match self {
            Preview::Now => {}
            Preview::LowHealth => {
                values.insert("hp".to_string(), Json::from(LOW_HEALTH));
            }
            Preview::Fight => {
                let sample =
                    |name: &str| values::entry(name).map_or(Json::Null, |e| Json::from(e.sample));
                values.insert("fight".to_string(), Json::Bool(true));
                for name in ["opponent", "opponent_hp", "opponent_cond"] {
                    values.insert(name.to_string(), sample(name));
                }
                let name = match live.resolve(&FieldRef::new("name")) {
                    Resolved::Value(Value::Text(name)) if !name.is_empty() => Json::from(name),
                    _ => sample("name"),
                };
                let health = match live.resolve(&FieldRef::new("hp")) {
                    Resolved::Hidden => Json::from("?"),
                    Resolved::Value(value) => {
                        tank_pct(&value).map_or(sample("tank_hp"), Json::from)
                    }
                    _ => sample("tank_hp"),
                };
                values.insert("tank".to_string(), name);
                values.insert("tank_hp".to_string(), health);
            }
            Preview::Lament => lament = true,
        }
        Overrides { values, lament }
    }
}

/// Your health as the game counts a tank's, `100 * hit / max` with
/// integer division, or the game's own percent when no
/// max is known.
fn tank_pct(value: &Value) -> Option<i64> {
    match value {
        Value::Gauge {
            cur,
            max: Some(max),
            ..
        } => Some(100 * cur / (*max).max(1)),
        Value::Gauge { pct: Some(pct), .. } => Some(*pct),
        Value::Decimal {
            value,
            max: Some(max),
            ..
        } if *max > 0.0 => Some((100.0 * value / max) as i64),
        _ => None,
    }
}

/// What the open card shows on your prompt in place of the live render
/// (sections 4 and 7): one of its previews, values on top of it, the
/// labels of values with nothing to show while the card is open, or the
/// game's own line while the card reads your codes. It never saves, and
/// it goes with the connection.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PromptPreview {
    /// The footer's choice.
    #[serde(default)]
    pub preview: Option<Preview>,
    /// Values on top of the preview's.
    #[serde(default)]
    pub overrides: Option<Overrides>,
    /// Draw each value with nothing to show as its label, so the card can
    /// point at it.
    #[serde(default)]
    pub placeholders: bool,
    /// Show the lines the game sent in place of your design, so the
    /// card's marks sit on them.
    #[serde(default)]
    pub raw: bool,
}

impl PromptPreview {
    /// True when it draws the live prompt as it is.
    pub fn is_live(&self) -> bool {
        !self.placeholders
            && !self.raw
            && matches!(self.preview, None | Some(Preview::Now))
            && self.overrides.as_ref().map_or(true, Overrides::is_empty)
    }

    /// The values it draws in place of `live`'s: the preview's, then the
    /// values on top.
    pub fn overrides(&self, live: &dyn Values) -> Overrides {
        let mut out = self
            .preview
            .map(|preview| preview.overrides(live))
            .unwrap_or_default();
        if let Some(over) = &self.overrides {
            out.values
                .extend(over.values.iter().map(|(k, v)| (k.clone(), v.clone())));
            out.lament |= over.lament;
        }
        out
    }
}

/// True for a field lamented tears hides: your vitals, your tank's and
/// your opponent's health, your affects and your group.
///
/// The live prompt works this out from the packets instead, in
/// `work_out_hidden` (H7 in hidden.rs). The preview keeps this list because
/// it shows what the song hides while no packet names the song. The two
/// differ on Char.Combat. This list hides only its health and condition
/// keys, and the live rule also hides a path that reads the whole packet or
/// the whole tank. Both stay as they are, since a merge would change what
/// the preview or the live prompt draws.
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
        let Some(entry) = values::entry_for(field) else {
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
                let cur = values::value_of(Kind::Gauge, entry.label, &text, self.now);
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
            kind => values::value_of(kind, entry.label, &text, self.now),
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

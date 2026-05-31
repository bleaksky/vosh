//! Trigger store. Owns the user-defined triggers, compiles their regex on
//! insert, and exposes them in priority order.

use regex::Regex;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use crate::action::TriggerAction;

/// A single pattern row inside a trigger. Mirrors Mudlet's per-pattern
/// editor: each row carries its own enable flag so a user can toggle
/// individual mob names on/off without editing a long pipe-delineated
/// regex.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerPattern {
    pub pattern: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// User-visible trigger record. Serializes cleanly to JSON for the editor UI
/// and for import or export. A trigger fires every action in `actions` in
/// order whenever ANY of its enabled patterns matches the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trigger {
    pub name: String,
    /// One or more patterns. Each row has its own enable flag; the
    /// trigger-level `enabled` gates the whole set. Disabled rows
    /// are skipped during matching.
    pub patterns: Vec<TriggerPattern>,
    pub priority: i32,
    pub enabled: bool,
    /// One or more actions; the engine fires each on every match.
    pub actions: Vec<TriggerAction>,
    /// Optional preset identifier. Triggers installed by the
    /// "Highlights" preset library tag themselves with the preset's
    /// id so the UI can list/remove them as a group. User-authored
    /// triggers leave this empty.
    pub preset: Option<String>,
}

fn default_enabled() -> bool {
    true
}

/// Wire format for [`Trigger`] that accepts:
/// - Legacy single-pattern shape: `pattern: "..."`
/// - New multi-pattern shape: `patterns: [{pattern, enabled}, ...]`
/// - Both action shapes: `action: {...}` (legacy) or `actions: [...]`
///
/// Serializes only the new `patterns` + `actions` shapes; the legacy
/// `pattern` field is also emitted so older Vosh builds can still
/// read profiles written by newer ones.
#[derive(Deserialize)]
struct TriggerRaw {
    name: String,
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default)]
    patterns: Option<Vec<TriggerPattern>>,
    #[serde(default)]
    priority: i32,
    #[serde(default = "default_enabled")]
    enabled: bool,
    #[serde(default)]
    action: Option<TriggerAction>,
    #[serde(default)]
    actions: Option<Vec<TriggerAction>>,
    #[serde(default)]
    preset: Option<String>,
}

impl<'de> Deserialize<'de> for Trigger {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = TriggerRaw::deserialize(deserializer)?;
        let actions = match (raw.action, raw.actions) {
            (Some(single), None) => vec![single],
            (None, Some(many)) => many,
            (Some(single), Some(mut many)) => {
                many.insert(0, single);
                many
            }
            (None, None) => {
                return Err(serde::de::Error::custom(
                    "trigger needs either `action` or `actions`",
                ));
            }
        };
        let patterns = match (raw.pattern, raw.patterns) {
            (_, Some(list)) if !list.is_empty() => list,
            (Some(p), _) => vec![TriggerPattern {
                pattern: p,
                enabled: true,
            }],
            (None, _) => {
                return Err(serde::de::Error::custom(
                    "trigger needs either `pattern` or non-empty `patterns`",
                ));
            }
        };
        Ok(Trigger {
            name: raw.name,
            patterns,
            priority: raw.priority,
            enabled: raw.enabled,
            actions,
            preset: raw.preset,
        })
    }
}

impl Serialize for Trigger {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Emit BOTH `pattern` (first entry, for older Vosh builds /
        // tools that only know the legacy shape) and `patterns` (the
        // canonical list).
        let field_count = 6 + usize::from(self.preset.is_some());
        let mut state = serializer.serialize_struct("Trigger", field_count)?;
        state.serialize_field("name", &self.name)?;
        let first_pattern = self.patterns.first().map_or("", |p| p.pattern.as_str());
        state.serialize_field("pattern", first_pattern)?;
        state.serialize_field("patterns", &self.patterns)?;
        state.serialize_field("priority", &self.priority)?;
        state.serialize_field("enabled", &self.enabled)?;
        state.serialize_field("actions", &self.actions)?;
        if let Some(preset) = &self.preset {
            state.serialize_field("preset", preset)?;
        }
        state.end()
    }
}

impl Trigger {
    /// Convenience accessor for the first pattern's text — used by
    /// older call sites + UI summaries that just need "what does this
    /// trigger match on?" at a glance.
    pub fn first_pattern(&self) -> &str {
        self.patterns.first().map_or("", |p| p.pattern.as_str())
    }
}

#[derive(Debug, Error)]
pub enum TriggerError {
    #[error("invalid regex `{pattern}`: {source}")]
    InvalidRegex {
        pattern: String,
        #[source]
        source: regex::Error,
    },
    #[error("trigger `{0}` not found")]
    NotFound(String),
    #[error("invalid json: {0}")]
    InvalidJson(#[from] serde_json::Error),
}

/// Compiled trigger held inside the store. Each enabled pattern
/// compiles to its own Regex on insert so matching does not pay a
/// parsing cost per line. The Vec is parallel to the user-facing
/// `Trigger.patterns` list, but only includes ENABLED entries.
pub(crate) struct CompiledTrigger {
    pub trigger: Trigger,
    pub regexes: Vec<Regex>,
}

#[derive(Default)]
pub struct TriggerStore {
    items: Vec<CompiledTrigger>,
}

impl TriggerStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace a trigger by name. Compiles every enabled
    /// pattern; returns an error on the first one that does not parse
    /// (the error names the offending pattern so the user can fix it).
    /// Disabled patterns are skipped — flipping them on later requires
    /// re-saving the trigger.
    pub fn set(&mut self, trigger: Trigger) -> Result<(), TriggerError> {
        let mut regexes = Vec::with_capacity(trigger.patterns.len());
        for entry in &trigger.patterns {
            if !entry.enabled {
                continue;
            }
            let regex = Regex::new(&entry.pattern).map_err(|e| TriggerError::InvalidRegex {
                pattern: entry.pattern.clone(),
                source: e,
            })?;
            regexes.push(regex);
        }
        self.items.retain(|t| t.trigger.name != trigger.name);
        self.items.push(CompiledTrigger { trigger, regexes });
        self.items
            .sort_by_key(|t| std::cmp::Reverse(t.trigger.priority));
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|t| t.trigger.name != name);
        before != self.items.len()
    }

    /// Remove every trigger tagged with the given preset id. Returns the
    /// number removed.
    pub fn remove_by_preset(&mut self, preset_id: &str) -> usize {
        let before = self.items.len();
        self.items
            .retain(|t| t.trigger.preset.as_deref() != Some(preset_id));
        before - self.items.len()
    }

    /// List preset ids currently present in the store, deduplicated.
    pub fn preset_ids(&self) -> Vec<String> {
        let mut seen = std::collections::BTreeSet::new();
        for t in &self.items {
            if let Some(id) = t.trigger.preset.as_deref() {
                seen.insert(id.to_string());
            }
        }
        seen.into_iter().collect()
    }

    pub fn get(&self, name: &str) -> Option<&Trigger> {
        self.items
            .iter()
            .find(|t| t.trigger.name == name)
            .map(|t| &t.trigger)
    }

    pub fn list(&self) -> Vec<Trigger> {
        self.items.iter().map(|t| t.trigger.clone()).collect()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Iterate the compiled triggers in priority order (high to low).
    pub(crate) fn iter_compiled(&self) -> impl Iterator<Item = &CompiledTrigger> {
        self.items.iter()
    }

    /// Replace every trigger from a JSON array. Returns the new count.
    pub fn import_json(&mut self, json: &str) -> Result<usize, TriggerError> {
        let triggers: Vec<Trigger> = serde_json::from_str(json)?;
        let mut next = TriggerStore::new();
        for t in triggers {
            next.set(t)?;
        }
        *self = next;
        Ok(self.items.len())
    }

    pub fn export_json(&self) -> Result<String, TriggerError> {
        let list = self.list();
        Ok(serde_json::to_string_pretty(&list)?)
    }
}

impl std::fmt::Debug for TriggerStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TriggerStore")
            .field("count", &self.items.len())
            .finish()
    }
}

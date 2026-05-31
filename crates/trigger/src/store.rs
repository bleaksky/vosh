//! Trigger store. Owns the user-defined triggers, compiles their regex on
//! insert, and exposes them in priority order.

use std::collections::BTreeSet;

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
    /// Optional user-facing group tag. Triggers sharing a group can
    /// be toggled on/off in bulk via `TriggerStore::set_group_enabled`
    /// without losing their individual `enabled` flags. Distinct
    /// from `preset` — `preset` is set automatically by the highlight
    /// preset library and removed when the preset is uninstalled,
    /// while `group` is user-authored and persists across edits.
    pub group: Option<String>,
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
    #[serde(default)]
    group: Option<String>,
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
            group: raw.group,
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
        // canonical list). `group` is omitted when unset so older
        // builds and grep-friendly diffs stay clean.
        let field_count =
            6 + usize::from(self.preset.is_some()) + usize::from(self.group.is_some());
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
        if let Some(group) = &self.group {
            state.serialize_field("group", group)?;
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
    /// Group names the user has bulk-disabled. A trigger whose
    /// `group` is in this set is skipped in matching regardless of
    /// its own `enabled` flag. Stored as the disabled inverse so a
    /// freshly-tagged group defaults to ON.
    disabled_groups: BTreeSet<String>,
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

    /// Iterate the compiled triggers in priority order (high to low),
    /// filtering out anything whose group is in the disabled set. The
    /// engine consumes this directly; per-trigger and per-pattern
    /// enable flags still apply downstream of the group check.
    pub(crate) fn iter_compiled(&self) -> impl Iterator<Item = &CompiledTrigger> {
        self.items.iter().filter(|c| {
            c.trigger
                .group
                .as_deref()
                .is_none_or(|g| !self.disabled_groups.contains(g))
        })
    }

    /// True when the named group is effectively enabled. Empty / missing
    /// group names are always "enabled" since ungrouped triggers do not
    /// participate in the bulk-disable mechanism.
    pub fn is_group_enabled(&self, group: &str) -> bool {
        group.is_empty() || !self.disabled_groups.contains(group)
    }

    /// Toggle a whole group. Calling with `true` removes the group
    /// from the disabled set; `false` adds it. No-op for an empty
    /// group name.
    pub fn set_group_enabled(&mut self, group: &str, enabled: bool) {
        if group.is_empty() {
            return;
        }
        if enabled {
            self.disabled_groups.remove(group);
        } else {
            self.disabled_groups.insert(group.to_string());
        }
    }

    /// Sorted list of every group referenced by at least one trigger,
    /// paired with its current enabled state.
    pub fn groups(&self) -> Vec<(String, bool)> {
        let mut names: BTreeSet<String> = BTreeSet::new();
        for t in &self.items {
            if let Some(g) = &t.trigger.group {
                if !g.is_empty() {
                    names.insert(g.clone());
                }
            }
        }
        names
            .into_iter()
            .map(|n| {
                let enabled = !self.disabled_groups.contains(&n);
                (n, enabled)
            })
            .collect()
    }

    /// Persistence accessor — returns the disabled group names.
    pub fn disabled_groups(&self) -> Vec<String> {
        self.disabled_groups.iter().cloned().collect()
    }

    /// Persistence inverse — replaces the disabled-group set.
    pub fn set_disabled_groups<I, S>(&mut self, groups: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.disabled_groups = groups
            .into_iter()
            .map(Into::into)
            .filter(|s| !s.is_empty())
            .collect();
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

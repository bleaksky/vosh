//! Trigger store. Owns the user-defined triggers, compiles their regex on
//! insert, and exposes them in priority order.

use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::action::TriggerAction;

/// User-visible trigger record. Serializes cleanly to JSON for the editor UI
/// and for import or export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trigger {
    pub name: String,
    pub pattern: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub action: TriggerAction,
}

fn default_enabled() -> bool {
    true
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

/// Compiled trigger held inside the store. The regex compiles once on insert
/// so matching does not pay a parsing cost per line.
pub(crate) struct CompiledTrigger {
    pub trigger: Trigger,
    pub regex: Regex,
}

#[derive(Default)]
pub struct TriggerStore {
    items: Vec<CompiledTrigger>,
}

impl TriggerStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace a trigger by name. Compiles the regex; returns an
    /// error when the pattern does not parse.
    pub fn set(&mut self, trigger: Trigger) -> Result<(), TriggerError> {
        let regex = Regex::new(&trigger.pattern).map_err(|e| TriggerError::InvalidRegex {
            pattern: trigger.pattern.clone(),
            source: e,
        })?;
        self.items.retain(|t| t.trigger.name != trigger.name);
        self.items.push(CompiledTrigger { trigger, regex });
        self.items
            .sort_by_key(|t| std::cmp::Reverse(t.trigger.priority));
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|t| t.trigger.name != name);
        before != self.items.len()
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

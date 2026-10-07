//! Your edits to the presets, the `[preset_edits]` table of the Presets
//! review (Q1, Q2). Each row you changed keeps your value and the preset's
//! value you changed it from, so a fix to the preset still reaches every
//! row you left alone, and Vosh can tell when a fix lands on a row you
//! changed. The table sits beside the list of presets that are on, in the
//! profile file in per profile mode and in catalog.toml in loadout mode,
//! as the `[alerts]` table does.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Your edits, by preset id. A preset with none has no entry.
pub(crate) type PresetEdits = BTreeMap<String, PresetEdit>;

/// Your edits to one preset: its swatches by color key, and its triggers'
/// rows by trigger name, then by row key. A row in a list keys by what
/// the preset holds there, a pattern by its text.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct PresetEdit {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) colors: BTreeMap<String, EditRow>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) triggers: BTreeMap<String, BTreeMap<String, EditRow>>,
}

/// One row you changed: your value and the preset's value you changed it
/// from, as TOML values so a swatch, a switch, a number, a text and a
/// list all fit. `seen` is the preset's value a launch notice already
/// named when a fix changed this row, so the notice tells each fix once.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct EditRow {
    pub(crate) value: toml::Value,
    pub(crate) was: toml::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) seen: Option<toml::Value>,
}

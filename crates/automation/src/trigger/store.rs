//! Trigger store. Owns the user-defined triggers, compiles their regex on
//! insert, and exposes them in priority order.

use std::borrow::Cow;

use regex::Regex;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use crate::groups::GroupSwitch;
use crate::revision::next_revision;
use crate::trigger::action::TriggerAction;

/// A single pattern row inside a trigger. Mirrors Mudlet's per-pattern
/// editor: each row carries its own enable flag so a user can toggle
/// individual mob names on/off without editing a long pipe-delineated
/// regex.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerPattern {
    pub pattern: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// How the store reads `pattern`. A row with no mode reads as Regex,
    /// and a Regex row leaves the field out, so a file written by an
    /// older build reads the same and an older build reads every trigger.
    #[serde(default, skip_serializing_if = "MatchMode::is_regex")]
    pub mode: MatchMode,
}

impl TriggerPattern {
    /// An enabled row that reads `pattern` as a regex.
    pub fn regex(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            enabled: true,
            mode: MatchMode::Regex,
        }
    }

    /// The regex the store compiles for this row, see [`MatchMode`].
    pub fn regex_source(&self) -> Cow<'_, str> {
        self.mode.regex_source(&self.pattern)
    }
}

/// How a pattern matches a line. Text and Starts with take the line as
/// you copy it out of the game, with no escaping, and Regex takes a
/// regular expression.
///
/// - `Text` matches a line that is exactly the text, with any spaces at
///   either end of the line and of the text skipped. It compiles to
///   `^\s*<text>\s*$`.
/// - `StartsWith` matches a line that starts with the text, after any
///   spaces at the start of the line and of the text. It compiles to
///   `^\s*<text>.*`, so the match runs to the end of the line and a
///   highlight colors the whole line. Spaces at the end of the text stay,
///   since they can mark the end of a word.
/// - `Regex` compiles the pattern as typed. Its groups fill `$1` on, and
///   Text and Starts with have none.
///
/// The game prints each thing in a look after five spaces, so a line you
/// copy with or without them matches in both of the first two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    Text,
    StartsWith,
    #[default]
    Regex,
}

impl MatchMode {
    /// Regex, the mode a row with none reads as. serde hands the field
    /// by reference.
    pub fn is_regex(&self) -> bool {
        *self == MatchMode::Regex
    }

    /// The regex `pattern` compiles to in this mode.
    pub fn regex_source(self, pattern: &str) -> Cow<'_, str> {
        match self {
            MatchMode::Regex => Cow::Borrowed(pattern),
            MatchMode::Text => Cow::Owned(format!(r"^\s*{}\s*$", regex::escape(pattern.trim()))),
            MatchMode::StartsWith => {
                Cow::Owned(format!(r"^\s*{}.*", regex::escape(pattern.trim_start())))
            }
        }
    }
}

/// A mode this build does not know, from a hand edit or a later build,
/// reads as Regex like a missing one, so one row never fails the whole
/// file. The next save leaves it out.
impl<'de> Deserialize<'de> for MatchMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let name = String::deserialize(deserializer)?;
        Ok(match name.as_str() {
            "text" => MatchMode::Text,
            "starts_with" => MatchMode::StartsWith,
            _ => MatchMode::Regex,
        })
    }
}

/// Which side of the line pipeline a trigger matches against.
///
/// - `Line` (default) — runs once per completed line of MUD output,
///   the historical behavior every trigger used.
/// - `Prompt` — runs against the partial-prompt buffer the telnet
///   parser flushes on `GA` / `EOR`. Used by tintin-style `#prompt`
///   triggers that need to capture from prompt text that arrives
///   without a trailing newline.
/// - `Room` — runs only on the lines a room look lists after its exits
///   line, the armies, the things and the people in the room. The
///   session tells those lines apart (see `room_block` in the app crate)
///   and runs them with [`crate::trigger::MatchScope::Room`], which fires `Line`
///   and `Room` triggers in one pass.
/// - `RoomTarget` — runs only on the line of the person you target with
///   `tar`, among the people a room look lists. The session runs that
///   line with [`crate::trigger::MatchScope::RoomTarget`], which fires `Line`,
///   `Room` and `RoomTarget` triggers in one pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TriggerTarget {
    #[default]
    Line,
    Prompt,
    Room,
    RoomTarget,
}

impl TriggerTarget {
    /// A target that matches lines of a room look, which builds up to
    /// 0.8.0 do not read.
    pub fn is_room(self) -> bool {
        matches!(self, TriggerTarget::Room | TriggerTarget::RoomTarget)
    }
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
    /// Which dispatch lane this trigger runs in. Defaults to `Line`;
    /// flipping to `Prompt` makes it fire against the partial-prompt
    /// buffer (telnet GA/EOR) instead of completed lines. See
    /// [`TriggerTarget`] for details.
    pub target: TriggerTarget,
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
    #[serde(default)]
    target: TriggerTarget,
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
            (Some(p), _) => vec![TriggerPattern::regex(p)],
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
            target: raw.target,
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
        // builds and grep-friendly diffs stay clean. `target` is
        // omitted when it is the default Line so the on-disk shape
        // for the overwhelming majority of triggers stays unchanged.
        let emit_target = self.target != TriggerTarget::default();
        let field_count = 6
            + usize::from(self.preset.is_some())
            + usize::from(self.group.is_some())
            + usize::from(emit_target);
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
        if emit_target {
            state.serialize_field("target", &self.target)?;
        }
        state.end()
    }
}

impl Trigger {
    /// A trigger with one enabled pattern and one action, on at priority
    /// 0, in no preset or group, matching completed lines. Set any other
    /// field with struct update syntax.
    pub fn new(name: impl Into<String>, pattern: impl Into<String>, action: TriggerAction) -> Self {
        Self {
            name: name.into(),
            patterns: vec![TriggerPattern::regex(pattern)],
            priority: 0,
            enabled: true,
            actions: vec![action],
            preset: None,
            group: None,
            target: TriggerTarget::Line,
        }
    }

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
    /// The groups you turned off. A trigger in an off group is
    /// skipped in matching whatever its own `enabled` flag says.
    groups: GroupSwitch,
    /// See [`TriggerStore::revision`].
    revision: u64,
}

impl TriggerStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Moves each time a trigger is added, replaced, or removed, so a
    /// caller can tell whether the list changed across a step without
    /// comparing it. Turning a group on or off leaves it alone, since
    /// the list itself stays the same.
    pub fn revision(&self) -> u64 {
        self.revision
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
            let regex =
                Regex::new(&entry.regex_source()).map_err(|e| TriggerError::InvalidRegex {
                    pattern: entry.pattern.clone(),
                    source: e,
                })?;
            regexes.push(regex);
        }
        self.items.retain(|t| t.trigger.name != trigger.name);
        self.items.push(CompiledTrigger { trigger, regexes });
        self.items
            .sort_by_key(|t| std::cmp::Reverse(t.trigger.priority));
        self.revision = next_revision();
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|t| t.trigger.name != name);
        let removed = before != self.items.len();
        if removed {
            self.revision = next_revision();
        }
        removed
    }

    /// Remove every trigger tagged with the given preset id. Returns the
    /// number removed.
    pub fn remove_by_preset(&mut self, preset_id: &str) -> usize {
        let before = self.items.len();
        self.items
            .retain(|t| t.trigger.preset.as_deref() != Some(preset_id));
        let removed = before - self.items.len();
        if removed > 0 {
            self.revision = next_revision();
        }
        removed
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

    /// Test only. The app's tests count triggers through the `testkit`
    /// feature.
    #[cfg(any(test, feature = "testkit"))]
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
        self.items
            .iter()
            .filter(|c| self.groups.allows(c.trigger.group.as_deref()))
    }

    /// True when the named group is effectively enabled. Empty / missing
    /// group names are always "enabled" since ungrouped triggers do not
    /// participate in the bulk-disable mechanism.
    pub fn is_group_enabled(&self, group: &str) -> bool {
        self.groups.is_enabled(group)
    }

    /// Toggle a whole group. Calling with `true` removes the group
    /// from the disabled set; `false` adds it. No-op for an empty
    /// group name. Returns whether the group turned.
    pub fn set_group_enabled(&mut self, group: &str, enabled: bool) -> bool {
        self.groups.set_enabled(group, enabled)
    }

    /// Sorted list of every group referenced by at least one trigger,
    /// paired with its current enabled state.
    pub fn groups(&self) -> Vec<(String, bool)> {
        self.groups
            .list(self.items.iter().filter_map(|t| t.trigger.group.as_deref()))
    }

    /// Persistence accessor — returns the disabled group names.
    pub fn disabled_groups(&self) -> Vec<String> {
        self.groups.disabled()
    }

    /// Persistence inverse — replaces the disabled-group set.
    pub fn set_disabled_groups<I, S>(&mut self, groups: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.groups.set_disabled(groups);
    }

    /// Replace every trigger from a JSON array. Returns the new count.
    /// The disabled-groups set carries over: it is user state about
    /// GROUPS, not item state, and wiping it on import silently
    /// re-enabled every disabled group each time the Settings editor
    /// saved (the editor saves through a full import).
    pub fn import_json(&mut self, json: &str) -> Result<usize, TriggerError> {
        let triggers: Vec<Trigger> = serde_json::from_str(json)?;
        let mut next = TriggerStore::new();
        for t in triggers {
            next.set(t)?;
        }
        // Take only after the fallible build succeeded: an early return on
        // a bad pattern must leave self (including its disabled set)
        // untouched.
        next.groups = std::mem::take(&mut self.groups);
        next.revision = next_revision();
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
            .field("groups", &self.groups)
            .field("revision", &self.revision)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trigger(name: &str, pattern: &str) -> Trigger {
        Trigger {
            group: Some("combat".into()),
            ..Trigger::new(name, pattern, TriggerAction::Gag)
        }
    }

    #[test]
    fn revision_moves_when_the_list_changes() {
        let mut store = TriggerStore::new();
        let empty = store.revision();
        store.set(trigger("flee", "^You flee")).unwrap();
        let after_add = store.revision();
        assert_ne!(after_add, empty);
        store.set(trigger("flee", "^You run")).unwrap();
        let after_replace = store.revision();
        assert_ne!(after_replace, after_add);
        assert!(store.remove("flee"));
        assert_ne!(store.revision(), after_replace);
    }

    #[test]
    fn revision_stays_when_nothing_in_the_list_changes() {
        let mut store = TriggerStore::new();
        store.set(trigger("flee", "^You flee")).unwrap();
        let rev = store.revision();
        assert!(!store.remove("missing"));
        assert_eq!(store.remove_by_preset("highlights"), 0);
        assert!(store.set(trigger("bad", "(")).is_err());
        store.set_group_enabled("combat", false);
        store.set_disabled_groups(["combat"]);
        assert!(store.import_json("not json").is_err());
        assert_eq!(store.revision(), rev);
    }

    /// A trigger whose rows are `rows`, each a pattern with its mode.
    fn with_modes(name: &str, rows: &[(&str, MatchMode)]) -> Trigger {
        Trigger {
            patterns: rows
                .iter()
                .map(|&(pattern, mode)| TriggerPattern {
                    mode,
                    ..TriggerPattern::regex(pattern)
                })
                .collect(),
            ..Trigger::new(name, "", TriggerAction::Gag)
        }
    }

    #[test]
    fn each_mode_compiles_to_its_regex() {
        // The preset line cure.feel_better, as the board shows each mode
        // holding it.
        let row = |pattern: &str, mode| TriggerPattern {
            mode,
            ..TriggerPattern::regex(pattern)
        };
        assert_eq!(
            row("You feel better.", MatchMode::Text).regex_source(),
            r"^\s*You feel better\.\s*$"
        );
        assert_eq!(
            row("You feel better", MatchMode::StartsWith).regex_source(),
            r"^\s*You feel better.*"
        );
        assert_eq!(
            row(r"You feel better\.$", MatchMode::Regex).regex_source(),
            r"You feel better\.$"
        );
        // Text skips the spaces at either end of what you typed, and Starts
        // with the spaces at its start. A space at the end of a Starts with
        // text stays, since it can mark the end of a word.
        for copy in [
            "You feel better. ",
            "     You feel better.",
            " You feel better.  ",
        ] {
            assert_eq!(
                row(copy, MatchMode::Text).regex_source(),
                r"^\s*You feel better\.\s*$",
                "{copy:?}"
            );
        }
        assert_eq!(
            row("     You feel ", MatchMode::StartsWith).regex_source(),
            r"^\s*You feel .*"
        );
        assert_eq!(
            row(" ^You feel ", MatchMode::Regex).regex_source(),
            " ^You feel "
        );
        // Text and Starts with escape what they hold, so a bracket or a
        // brace never fails the trigger.
        let mut store = TriggerStore::new();
        store
            .set(with_modes(
                "afk",
                &[("[AFK] (", MatchMode::Text), ("{x", MatchMode::StartsWith)],
            ))
            .unwrap();
        assert!(store
            .set(with_modes("bad", &[("[AFK] (", MatchMode::Regex)]))
            .is_err());
    }

    #[test]
    fn a_row_with_no_mode_reads_as_regex_and_writes_none() {
        let old = r#"{"name":"flee","pattern":"^You flee","actions":[{"kind":"gag"}]}"#;
        let t: Trigger = serde_json::from_str(old).unwrap();
        assert_eq!(t.patterns[0].mode, MatchMode::Regex);
        let rows = r#"{"name":"flee","patterns":[{"pattern":"^You flee"}],"actions":[]}"#;
        let t: Trigger = serde_json::from_str(rows).unwrap();
        assert_eq!(t.patterns[0].mode, MatchMode::Regex);
        // A Regex row leaves the field out, so the shape an older build
        // reads stays the same.
        let written = serde_json::to_string(&t).unwrap();
        assert!(!written.contains("mode"), "{written}");
    }

    #[test]
    fn text_and_starts_with_round_trip_through_json() {
        let t = with_modes(
            "feel",
            &[
                ("You feel better.", MatchMode::Text),
                ("You feel", MatchMode::StartsWith),
                ("better\\.$", MatchMode::Regex),
            ],
        );
        let json = serde_json::to_value(&t).unwrap();
        assert_eq!(json["patterns"][0]["mode"], "text");
        assert_eq!(json["patterns"][1]["mode"], "starts_with");
        assert!(json["patterns"][2].get("mode").is_none());
        let back: Trigger = serde_json::from_value(json).unwrap();
        assert_eq!(back, t);
        // Through the store too, as the Settings editor saves.
        let mut store = TriggerStore::new();
        store.set(t.clone()).unwrap();
        let text = store.export_json().unwrap();
        let mut again = TriggerStore::new();
        again.import_json(&text).unwrap();
        assert_eq!(again.list(), [t]);
    }

    #[test]
    fn a_mode_this_build_does_not_know_reads_as_regex() {
        let later = r#"{"name":"x","patterns":[{"pattern":"^a","mode":"glob"}],"actions":[]}"#;
        let t: Trigger = serde_json::from_str(later).unwrap();
        assert_eq!(t.patterns[0].mode, MatchMode::Regex);
    }

    #[test]
    fn a_replacement_store_reads_as_a_change() {
        let mut first = TriggerStore::new();
        first.set(trigger("flee", "^You flee")).unwrap();
        let mut second = TriggerStore::new();
        second.set(trigger("flee", "^You flee")).unwrap();
        assert_ne!(first.revision(), second.revision());
        let rev = first.revision();
        first.import_json("[]").unwrap();
        assert_ne!(first.revision(), rev);
    }
}

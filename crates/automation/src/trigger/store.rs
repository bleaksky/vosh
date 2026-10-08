//! Trigger store. Owns the user-defined triggers, compiles their regex on
//! insert, and exposes them in priority order.

use std::borrow::Cow;

use regex::Regex;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use crate::alert::AlertParts;
use crate::groups::GroupSwitch;
use crate::revision::next_revision;
use crate::stops::{StopKey, Stops};
use crate::trigger::action::TriggerAction;

/// A single pattern row inside a trigger. Mirrors Mudlet's per-pattern
/// editor: each row carries its own enable flag so a user can toggle
/// individual mob names on/off without editing a long pipe-delineated
/// regex.
///
/// On disk and on the wire a Regex row is `pattern` and `enabled`, as
/// every build writes it. A Text or Starts with row adds `mode`, and
/// keeps what you typed in `text` and the regex it compiles to in
/// `pattern`. Builds up to 0.8.1 know no mode and read `pattern` as a
/// regex, so they match the same lines and their next save keeps the
/// trigger, as a Regex row. `PatternRaw` says how a row reads.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "PatternRaw")]
pub struct TriggerPattern {
    /// What you typed: the regex of a Regex row, the text of a Text or
    /// Starts with row.
    pub pattern: String,
    pub enabled: bool,
    /// How the store reads `pattern`.
    pub mode: MatchMode,
}

/// A pattern row as a file or the page holds it.
///
/// - A row with no mode is Regex and reads `pattern`. It drops any
///   `text`, so a mode this build does not know reads the regex its
///   build saved beside the text, which matches the same lines.
/// - A Text or Starts with row reads what you typed from `text`, and
///   needs no `pattern`, so a hand edit can leave it out.
/// - A Text or Starts with row with no `text`, which builds wrote before
///   the field, reads `pattern` as the text.
///
/// A row with nothing to read, a Regex row with no `pattern` or a Text
/// row with neither field, fails as a missing `pattern` always has.
#[derive(Deserialize)]
struct PatternRaw {
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default = "default_enabled")]
    enabled: bool,
    #[serde(default)]
    mode: MatchMode,
    #[serde(default)]
    text: Option<String>,
}

impl TryFrom<PatternRaw> for TriggerPattern {
    type Error = &'static str;

    fn try_from(raw: PatternRaw) -> Result<Self, Self::Error> {
        let pattern = match (raw.mode, raw.text, raw.pattern) {
            (MatchMode::Text | MatchMode::StartsWith, Some(text), _) => text,
            (_, _, Some(pattern)) => pattern,
            (_, _, None) => return Err("missing field `pattern`"),
        };
        Ok(Self {
            pattern,
            enabled: raw.enabled,
            mode: raw.mode,
        })
    }
}

impl Serialize for TriggerPattern {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let typed = !self.mode.is_regex();
        let mut state = serializer.serialize_struct("TriggerPattern", if typed { 4 } else { 2 })?;
        state.serialize_field("pattern", &self.regex_source())?;
        state.serialize_field("enabled", &self.enabled)?;
        if typed {
            state.serialize_field("mode", &self.mode)?;
            state.serialize_field("text", &self.pattern)?;
        }
        state.end()
    }
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
    /// Regex, the mode a row with none reads as.
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
/// reads as Regex like a missing one, and so does a value that is not a
/// name at all, such as `mode = 1`. One row never fails the whole file,
/// and the next save leaves the field out.
impl<'de> Deserialize<'de> for MatchMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        /// What a file holds in `mode`, a name or anything else.
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Name(String),
            Other(serde::de::IgnoredAny),
        }
        Ok(match Raw::deserialize(deserializer)? {
            Raw::Name(name) => match name.as_str() {
                "text" => MatchMode::Text,
                "starts_with" => MatchMode::StartsWith,
                _ => MatchMode::Regex,
            },
            Raw::Other(_) => MatchMode::Regex,
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
    /// The alert the trigger rings when it matches, kept in a table of
    /// its own beside the actions, so a build that knows no alert skips
    /// it and still reads the trigger. It rides on the match, not on the
    /// line showing, so a trigger that hides its line still rings.
    pub alert: Option<AlertParts>,
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
    #[serde(default)]
    alert: Option<AlertRaw>,
}

/// A trigger's `alert` table, or anything else in its place, which reads
/// as no alert so one bad value never fails the trigger.
#[derive(Deserialize)]
#[serde(untagged)]
enum AlertRaw {
    Parts(AlertParts),
    Other(serde::de::IgnoredAny),
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
            alert: match raw.alert {
                Some(AlertRaw::Parts(parts)) => Some(parts),
                Some(AlertRaw::Other(_)) | None => None,
            },
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
        // for the overwhelming majority of triggers stays unchanged, and
        // so is `alert` when the trigger rings none.
        let emit_target = self.target != TriggerTarget::default();
        let field_count = 6
            + usize::from(self.preset.is_some())
            + usize::from(self.group.is_some())
            + usize::from(emit_target)
            + usize::from(self.alert.is_some());
        let mut state = serializer.serialize_struct("Trigger", field_count)?;
        state.serialize_field("name", &self.name)?;
        // Those builds read it as a regex, so a Text or Starts with row
        // gives the regex it compiles to.
        let first_pattern = self
            .patterns
            .first()
            .map_or(Cow::Borrowed(""), TriggerPattern::regex_source);
        state.serialize_field("pattern", &first_pattern)?;
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
        if let Some(alert) = &self.alert {
            state.serialize_field("alert", alert)?;
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
            alert: None,
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
    /// The triggers whose Lua Vosh stopped, each under the key of the
    /// session it stopped in. A stopped trigger matches nothing under its
    /// key until you save it again.
    stopped: Stops,
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
        // Saving a trigger Vosh stopped turns it back on everywhere.
        self.stopped.clear(&trigger.name);
        self.items.push(CompiledTrigger { trigger, regexes });
        self.items
            .sort_by_key(|t| std::cmp::Reverse(t.trigger.priority));
        self.revision = next_revision();
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> bool {
        self.stopped.clear(name);
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

    /// Turn the trigger `name` off under `key`, after Vosh stopped its Lua
    /// there. It stays off there until you save it again.
    pub fn stop(&mut self, name: &str, key: StopKey) {
        if self.get(name).is_some() {
            self.stopped.stop(name, key);
        }
    }

    /// True while Vosh holds the trigger `name` off under `key` after a
    /// stop.
    pub fn is_stopped(&self, name: &str, key: StopKey) -> bool {
        self.stopped.contains(name, key)
    }

    /// Drop every stop under `key`, as the session it names closes.
    pub fn forget_stops(&mut self, key: StopKey) {
        self.stopped.forget(key);
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
    /// filtering out anything whose group is in the disabled set and
    /// any trigger Vosh stopped under `key`. The
    /// engine consumes this directly; per-trigger and per-pattern
    /// enable flags still apply downstream of the group check.
    pub(crate) fn iter_compiled(&self, key: StopKey) -> impl Iterator<Item = &CompiledTrigger> {
        let stopped = self.stopped.under(key);
        self.items.iter().filter(move |c| {
            self.groups.allows(c.trigger.group.as_deref())
                && !stopped.is_some_and(|names| names.contains(&c.trigger.name))
        })
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
        // The editor saves the whole list at once, so a trigger Vosh
        // stopped stays off, under each key, unless this save changed it.
        next.stopped = self.stopped.kept(|name| {
            let now = next.get(name);
            now.is_some() && now == self.get(name)
        });
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
            .field("stopped", &self.stopped)
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
    fn an_alert_rides_beside_the_actions_and_a_bad_one_reads_as_none() {
        let json = r#"{"name":"visitor","pattern":"walks in","actions":[{"kind":"gag"}],
            "alert":{"banner":true,"sound":"chime","attention":"once"}}"#;
        let t: Trigger = serde_json::from_str(json).unwrap();
        let alert = t.alert.clone().expect("the alert table");
        assert!(alert.banner && alert.background && !alert.words);
        assert_eq!(alert.sound.as_deref(), Some("chime"));
        let again: Trigger = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        assert_eq!(again, t);
        // A value no build wrote, such as a banner that is no switch,
        // leaves the trigger with no alert and its actions whole.
        let json = r#"{"name":"visitor","pattern":"walks in","actions":[{"kind":"gag"}],
            "alert":{"banner":"yes"}}"#;
        let t: Trigger = serde_json::from_str(json).unwrap();
        assert_eq!(t.alert, None);
        assert_eq!(t.actions, [TriggerAction::Gag]);
        // A trigger with no alert writes no key for it.
        let text = serde_json::to_string(&Trigger::new("x", "x", TriggerAction::Gag)).unwrap();
        assert!(!text.contains("alert"), "{text}");
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

    /// The session the lines here come to, and another.
    const SESSION: StopKey = StopKey(1);
    const OTHER: StopKey = StopKey(2);

    /// Whether `store` gags `line` under `key`, the one action the test
    /// triggers take.
    fn gags(store: &TriggerStore, line: &str, key: StopKey) -> bool {
        crate::trigger::process(store, line.as_bytes(), key)
            .display
            .is_none()
    }

    #[test]
    fn a_stopped_trigger_stays_off_in_its_session_until_you_save_it() {
        let mut store = TriggerStore::new();
        store.set(trigger("hunger", "^You are hungry")).unwrap();
        store.stop("hunger", SESSION);
        assert!(store.is_stopped("hunger", SESSION));
        assert!(!gags(&store, "You are hungry.", SESSION));
        // The other session still matches it.
        assert!(!store.is_stopped("hunger", OTHER));
        assert!(gags(&store, "You are hungry.", OTHER));
        // The list and its revision stay as they were, so Settings and
        // the saved profile still hold the trigger as you wrote it.
        assert_eq!(store.list().len(), 1);
        store.stop("hunger", OTHER);
        store.set(trigger("hunger", "^You are hungry")).unwrap();
        for key in [SESSION, OTHER] {
            assert!(!store.is_stopped("hunger", key));
            assert!(gags(&store, "You are hungry.", key));
        }
        // No trigger of that name, nothing to stop.
        store.stop("missing", SESSION);
        assert!(!store.is_stopped("missing", SESSION));
        // The stops of a session that closed go with it.
        store.stop("hunger", SESSION);
        store.forget_stops(SESSION);
        assert!(gags(&store, "You are hungry.", SESSION));
    }

    #[test]
    fn a_whole_list_save_keeps_only_the_unchanged_stops_in_each_session() {
        let mut store = TriggerStore::new();
        store.set(trigger("hunger", "^You are hungry")).unwrap();
        store.set(trigger("day", "^The day has begun")).unwrap();
        store.stop("hunger", SESSION);
        store.stop("day", SESSION);
        store.stop("day", OTHER);
        let mut edited = store.list();
        for t in &mut edited {
            if t.name == "day" {
                t.priority = 5;
            }
        }
        store
            .import_json(&serde_json::to_string(&edited).unwrap())
            .unwrap();
        assert!(store.is_stopped("hunger", SESSION));
        assert!(!gags(&store, "You are hungry.", SESSION));
        assert!(!store.is_stopped("hunger", OTHER));
        for key in [SESSION, OTHER] {
            assert!(!store.is_stopped("day", key));
            assert!(gags(&store, "The day has begun.", key));
        }
        assert!(store.remove("hunger"));
        assert!(!store.is_stopped("hunger", SESSION));
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
        // The preset line cure.feel_better, held in each mode.
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
        // Each Text and Starts with row holds the regex it compiles to in
        // `pattern`, which older builds read, and what you typed in `text`.
        assert_eq!(
            json["patterns"],
            serde_json::json!([
                {
                    "pattern": r"^\s*You feel better\.\s*$",
                    "enabled": true,
                    "mode": "text",
                    "text": "You feel better.",
                },
                {
                    "pattern": r"^\s*You feel.*",
                    "enabled": true,
                    "mode": "starts_with",
                    "text": "You feel",
                },
                { "pattern": r"better\.$", "enabled": true },
            ])
        );
        // The pattern before the rows, which builds older than the rows
        // read, is the regex of the first row too.
        assert_eq!(json["pattern"], r"^\s*You feel better\.\s*$");
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

    /// The rows of the one trigger in `json`, as this build reads them.
    fn rows_of(json: &str) -> Vec<TriggerPattern> {
        serde_json::from_str::<Trigger>(json).unwrap().patterns
    }

    #[test]
    fn a_text_or_starts_with_row_reads_what_you_typed_from_text() {
        // `text` wins over `pattern`, whatever `pattern` holds, so a page
        // that leaves the old regex in `pattern` after an edit saves the
        // new text.
        let rows = rows_of(
            r#"{"name":"x","patterns":[
                {"pattern":"^\\s*You feel better\\.\\s*$","mode":"text","text":"You feel better."},
                {"pattern":"stale","mode":"starts_with","text":"*** Too Dark ***"}
            ],"actions":[]}"#,
        );
        assert_eq!(
            rows,
            [
                TriggerPattern {
                    mode: MatchMode::Text,
                    ..TriggerPattern::regex("You feel better.")
                },
                TriggerPattern {
                    mode: MatchMode::StartsWith,
                    ..TriggerPattern::regex("*** Too Dark ***")
                },
            ]
        );
    }

    #[test]
    fn a_text_row_with_no_text_reads_pattern_as_the_text() {
        // The shape builds wrote before `text`, with what you typed in
        // `pattern`.
        let rows = rows_of(
            r#"{"name":"x","patterns":[
                {"pattern":"You feel better.","mode":"text"},
                {"pattern":"*** Too Dark ***","enabled":false,"mode":"starts_with"}
            ],"actions":[]}"#,
        );
        assert_eq!(rows[0].pattern, "You feel better.");
        assert_eq!(rows[0].mode, MatchMode::Text);
        assert_eq!(rows[1].pattern, "*** Too Dark ***");
        assert!(!rows[1].enabled);
        // The next save moves the text to `text` and puts its regex in
        // `pattern`.
        let t = Trigger {
            patterns: rows,
            ..Trigger::new("x", "", TriggerAction::Gag)
        };
        let json = serde_json::to_value(&t).unwrap();
        assert_eq!(json["patterns"][0]["text"], "You feel better.");
        assert_eq!(json["patterns"][0]["pattern"], r"^\s*You feel better\.\s*$");
        assert_eq!(json["patterns"][1]["text"], "*** Too Dark ***");
        assert_eq!(
            json["patterns"][1]["pattern"],
            r"^\s*\*\*\* Too Dark \*\*\*.*"
        );
    }

    #[test]
    fn a_text_row_with_only_text_reads_and_saves_its_regex() {
        // A hand edit that writes only `mode` and `text`, since a Text or
        // Starts with row reads `text`.
        let rows = rows_of(
            r#"{"name":"x","patterns":[
                {"mode":"text","text":"You are thirsty."},
                {"enabled":false,"mode":"starts_with","text":"You are hungry"}
            ],"actions":[]}"#,
        );
        assert_eq!(
            rows,
            [
                TriggerPattern {
                    mode: MatchMode::Text,
                    ..TriggerPattern::regex("You are thirsty.")
                },
                TriggerPattern {
                    enabled: false,
                    mode: MatchMode::StartsWith,
                    ..TriggerPattern::regex("You are hungry")
                },
            ]
        );
        // The next save writes the regex in `pattern`, which older builds
        // read.
        let t = Trigger {
            patterns: rows,
            ..Trigger::new("x", "", TriggerAction::Gag)
        };
        assert_eq!(
            serde_json::to_value(&t).unwrap()["patterns"],
            serde_json::json!([
                {
                    "pattern": r"^\s*You are thirsty\.\s*$",
                    "enabled": true,
                    "mode": "text",
                    "text": "You are thirsty.",
                },
                {
                    "pattern": r"^\s*You are hungry.*",
                    "enabled": false,
                    "mode": "starts_with",
                    "text": "You are hungry",
                },
            ])
        );
    }

    #[test]
    fn a_row_with_nothing_to_read_still_fails() {
        // A Regex row reads `pattern`, so one with none fails as it always
        // has, and so does a Text row with neither field.
        for row in [
            r#"{"enabled":true}"#,
            r#"{"text":"You are thirsty."}"#,
            r#"{"mode":"glob","text":"You are thirsty."}"#,
            r#"{"mode":"text"}"#,
            r#"{"mode":"starts_with","enabled":false}"#,
        ] {
            let json = format!(r#"{{"name":"x","patterns":[{row}],"actions":[]}}"#);
            let err = serde_json::from_str::<Trigger>(&json).unwrap_err();
            assert!(
                err.to_string().contains("missing field `pattern`"),
                "{row}: {err}"
            );
        }
    }

    #[test]
    fn a_regex_row_reads_pattern_and_drops_text() {
        let rows = rows_of(
            r#"{"name":"x","patterns":[{"pattern":"^You feel","text":"You feel better."}],"actions":[]}"#,
        );
        assert_eq!(rows, [TriggerPattern::regex("^You feel")]);
        let t = Trigger {
            patterns: rows,
            ..Trigger::new("x", "", TriggerAction::Gag)
        };
        assert_eq!(
            serde_json::to_value(&t).unwrap()["patterns"],
            serde_json::json!([{ "pattern": "^You feel", "enabled": true }])
        );
    }

    #[test]
    fn a_mode_this_build_does_not_know_reads_as_regex() {
        let later = r#"{"name":"x","patterns":[{"pattern":"^a","mode":"glob"}],"actions":[]}"#;
        let t: Trigger = serde_json::from_str(later).unwrap();
        assert_eq!(t.patterns[0].mode, MatchMode::Regex);
        // A later build that saves a regex in `pattern` beside its own
        // text matches the same lines here.
        let rows = rows_of(
            r#"{"name":"x","patterns":[{"pattern":"^\\s*You feel","mode":"glob","text":"You feel*"}],"actions":[]}"#,
        );
        assert_eq!(rows, [TriggerPattern::regex(r"^\s*You feel")]);
        // A value that is not a name reads as Regex too, and the rows
        // around it keep their modes.
        for value in ["1", "true", "null", "[]", "{}", "1.5"] {
            let json = format!(
                r#"{{"name":"x","patterns":[{{"pattern":"^a","mode":{value}}},{{"pattern":"a","mode":"text"}}],"actions":[]}}"#
            );
            let t: Trigger = serde_json::from_str(&json).unwrap();
            assert_eq!(t.patterns[0].mode, MatchMode::Regex, "{value}");
            assert_eq!(t.patterns[1].mode, MatchMode::Text, "{value}");
        }
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

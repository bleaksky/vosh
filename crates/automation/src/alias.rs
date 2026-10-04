//! Alias engine with positional substitution and a recursion-depth guard.
//!
//! An alias matches the first whitespace-separated word of a command. The
//! expansion may contain:
//!
//! - `%0` — the entire arg tail.
//! - `%1` through `%9` — the Nth whitespace-separated word.
//! - `%1-` through `%9-` — word N and the rest of the input, preserving the
//!   original whitespace between words. `%1-` is equivalent to `%0`. `%N-`
//!   with fewer than N words present expands to empty.
//! - `%%` — a literal `%`.
//!
//! Multiple commands separated by `;` in an expansion are split and each is
//! re-fed through the engine, bounded by a maximum recursion depth.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::groups::GroupSwitch;
use crate::revision::next_revision;
use crate::split::split_commands;
use crate::ScriptCall;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alias {
    pub name: String,
    pub expansion: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Optional group tag. Aliases sharing a group can be turned
    /// on or off together via `AliasStore::set_group_enabled` without
    /// losing their individual `enabled` flags. `None` means the
    /// alias is ungrouped and only its own `enabled` controls it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Optional Lua body. When `Some`, the alias evaluates the body
    /// via the session's `ScriptEngine` with the words typed after its
    /// name bound to a local `captures` table, and `expansion` is
    /// ignored. Lets aliases branch / loop / call `mud.send` rather
    /// than just expand a template. `None` keeps the legacy
    /// template-substitution path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
}

fn default_enabled() -> bool {
    true
}

impl Alias {
    pub fn new(name: impl Into<String>, expansion: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            expansion: expansion.into(),
            enabled: true,
            group: None,
            script: None,
        }
    }

    /// Builder-style setter for the optional Lua script body. Only tests
    /// call it, the app's tests through the `testkit` feature.
    #[cfg(any(test, feature = "testkit"))]
    #[must_use]
    pub fn with_script(mut self, script: impl Into<String>) -> Self {
        self.script = Some(script.into());
        self
    }

    /// Builder-style setter for the optional group tag. Test only, like
    /// `with_script`.
    #[cfg(any(test, feature = "testkit"))]
    #[must_use]
    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }
}

/// One step of an expanded line. A line expands to its steps in the
/// order you typed them, so a script alias runs between the commands
/// around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpandStep {
    /// A command to send to the game.
    Command(String),
    /// The Lua body of a script alias, to run at this point in the line.
    Script(ScriptCall),
}

#[derive(Debug, Error)]
pub enum ExpandError {
    #[error("alias recursion limit exceeded ({0})")]
    RecursionLimit(usize),
}

/// Default cap on alias recursion depth. Matches the `TinTin++` default.
pub const DEFAULT_MAX_DEPTH: usize = 16;

#[derive(Debug, Clone)]
pub struct AliasStore {
    aliases: HashMap<String, Alias>,
    max_depth: usize,
    /// The groups you turned off. An alias in an off group passes
    /// through whatever its own `enabled` flag says.
    groups: GroupSwitch,
    /// See [`AliasStore::revision`].
    revision: u64,
    /// The aliases whose Lua Vosh stopped this session. A stopped alias
    /// passes through, as an off one does, until you save it again. Vosh
    /// never saves this, so a restart turns them all back on.
    stopped: HashSet<String>,
}

impl Default for AliasStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AliasStore {
    pub fn new() -> Self {
        Self {
            aliases: HashMap::new(),
            max_depth: DEFAULT_MAX_DEPTH,
            groups: GroupSwitch::default(),
            revision: 0,
            stopped: HashSet::new(),
        }
    }

    /// Moves each time an alias is added, replaced, or removed, so a
    /// caller can tell whether the list changed across a step without
    /// comparing it. Turning a group on or off leaves it alone, since
    /// the list itself stays the same.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// True when the named group is effectively enabled. Returns true
    /// for an empty / missing group name (ungrouped aliases never
    /// participate in the group-disable mechanism).
    pub fn is_group_enabled(&self, group: &str) -> bool {
        self.groups.is_enabled(group)
    }

    /// Toggle a whole group. Calling with `enabled = true` removes
    /// the group from the disabled set; with `false` adds it.
    pub fn set_group_enabled(&mut self, group: &str, enabled: bool) {
        self.groups.set_enabled(group, enabled);
    }

    /// Sorted list of every group name referenced by at least one
    /// alias, paired with whether that group is currently enabled.
    /// Used by the Settings UI to render the per-group toggle row.
    pub fn groups(&self) -> Vec<(String, bool)> {
        self.groups
            .list(self.aliases.values().filter_map(|a| a.group.as_deref()))
    }

    /// Persistence accessor for the disabled-groups set. Returns the
    /// names that should be saved alongside the alias list.
    pub fn disabled_groups(&self) -> Vec<String> {
        self.groups.disabled()
    }

    /// Persistence inverse of `disabled_groups()`. Replaces the
    /// current disabled-set with the supplied names.
    pub fn set_disabled_groups<I, S>(&mut self, groups: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.groups.set_disabled(groups);
    }

    /// Lowers the recursion cap so a test can reach it in a few steps.
    #[cfg(test)]
    #[must_use]
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// Insert or replace an alias. Saving an alias Vosh stopped turns
    /// it back on.
    pub fn set(&mut self, alias: Alias) {
        self.stopped.remove(&alias.name);
        self.aliases.insert(alias.name.clone(), alias);
        self.revision = next_revision();
    }

    pub fn remove(&mut self, name: &str) -> bool {
        self.stopped.remove(name);
        let removed = self.aliases.remove(name).is_some();
        if removed {
            self.revision = next_revision();
        }
        removed
    }

    pub fn get(&self, name: &str) -> Option<&Alias> {
        self.aliases.get(name)
    }

    /// Turn the alias `name` off for the rest of the session, after Vosh
    /// stopped its Lua. It stays off until you save it again.
    pub fn stop(&mut self, name: &str) {
        if self.aliases.contains_key(name) {
            self.stopped.insert(name.to_string());
        }
    }

    /// True while Vosh holds the alias `name` off after a stop.
    pub fn is_stopped(&self, name: &str) -> bool {
        self.stopped.contains(name)
    }

    /// Keep the stops of `old` for each alias this store holds as `old`
    /// held it. The Settings editor saves the whole list at once, so
    /// only the alias you changed comes back on.
    pub fn keep_stops_from(&mut self, old: &AliasStore) {
        for name in &old.stopped {
            if self.aliases.get(name) == old.aliases.get(name) {
                self.stopped.insert(name.clone());
            }
        }
    }

    pub fn list(&self) -> Vec<&Alias> {
        let mut out: Vec<&Alias> = self.aliases.values().collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Expand a single command line and return only the resulting send
    /// commands. Script aliases that fire during expansion are discarded.
    /// Test only, since the input path runs script bodies and so calls
    /// [`expand_line_full`](Self::expand_line_full).
    #[cfg(test)]
    pub fn expand_line(&self, line: &str) -> Result<Vec<String>, ExpandError> {
        Ok(self
            .expand_line_full(line)?
            .into_iter()
            .filter_map(|step| match step {
                ExpandStep::Command(command) => Some(command),
                ExpandStep::Script(_) => None,
            })
            .collect())
    }

    /// Full expansion result: the commands to send and the Lua bodies
    /// script aliases queue, in the order you typed them. The input
    /// pipeline runs each body where it stands, so what a body sends goes
    /// out between the commands around it.
    pub fn expand_line_full(&self, line: &str) -> Result<Vec<ExpandStep>, ExpandError> {
        let mut steps = Vec::new();
        for raw in split_commands(line) {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            self.expand_into(trimmed, 0, &mut steps)?;
        }
        Ok(steps)
    }

    fn expand_into(
        &self,
        command: &str,
        depth: usize,
        out: &mut Vec<ExpandStep>,
    ) -> Result<(), ExpandError> {
        if depth >= self.max_depth {
            return Err(ExpandError::RecursionLimit(self.max_depth));
        }

        let (name, rest) = split_first_word(command);
        // The alias fires only when:
        //   * the named entry exists, AND
        //   * its own `enabled` flag is true, AND
        //   * its group is enabled (or it is ungrouped), AND
        //   * Vosh has not stopped its Lua this session.
        // Disabled groups short-circuit to pass-through so the user
        // can flip whole "Combat" / "Crafting" loadouts off without
        // editing each row.
        let Some(alias) = self.aliases.get(name).filter(|a| {
            a.enabled && self.groups.allows(a.group.as_deref()) && !self.stopped.contains(&a.name)
        }) else {
            out.push(ExpandStep::Command(command.to_string()));
            return Ok(());
        };

        // Script-bodied aliases bypass template expansion entirely.
        // Lua reads the words after the name as `captures[1]`,
        // `captures[2]`, ..., the words `%1`, `%2`, ... would take.
        if let Some(body) = &alias.script {
            out.push(ExpandStep::Script(ScriptCall {
                source: alias.name.clone(),
                body: body.clone(),
                captures: rest.split_whitespace().map(str::to_string).collect(),
            }));
            return Ok(());
        }

        let expanded = substitute_params(&alias.expansion, rest);
        for raw in split_commands(&expanded) {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            self.expand_into(trimmed, depth + 1, out)?;
        }
        Ok(())
    }
}

fn split_first_word(input: &str) -> (&str, &str) {
    let trimmed = input.trim_start();
    match trimmed.find(char::is_whitespace) {
        Some(idx) => {
            let (head, tail) = trimmed.split_at(idx);
            (head, tail.trim_start())
        }
        None => (trimmed, ""),
    }
}

fn substitute_params(expansion: &str, args: &str) -> String {
    let words: Vec<&str> = args.split_whitespace().collect();
    let mut out = String::with_capacity(expansion.len());
    let mut chars = expansion.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        match chars.peek().copied() {
            Some('%') => {
                out.push('%');
                chars.next();
            }
            Some(d @ '0'..='9') => {
                chars.next();
                let idx = (d as u8 - b'0') as usize;
                // Range form `%N-` means "word N through the end of the
                // input, with the original whitespace between words
                // preserved". Detected as a trailing `-` after the digit.
                // Mirrors TinTin's `%1.-9` shorthand without the explicit
                // upper bound.
                if chars.peek().copied() == Some('-') {
                    chars.next();
                    out.push_str(args_from_word(args, idx));
                } else if idx == 0 {
                    out.push_str(args);
                } else if let Some(word) = words.get(idx - 1) {
                    out.push_str(word);
                }
            }
            _ => out.push('%'),
        }
    }
    out
}

/// Return the slice of `args` that starts at the Nth whitespace-separated
/// word (1-based). N=0 returns the full args string. When fewer than N
/// words are present, returns an empty slice.
///
/// Walks the original `args` byte-by-char so the whitespace between words
/// is preserved verbatim — `%2-` on input `"a   b\tc"` yields `"b\tc"`,
/// not `"b c"`. Used by the `%N-` range form so the user can write
/// `tell %1 %2-` and have the message text keep its original spacing.
fn args_from_word(args: &str, n: usize) -> &str {
    if n == 0 {
        return args;
    }
    let mut word_count = 0;
    let mut in_word = false;
    for (i, ch) in args.char_indices() {
        if ch.is_whitespace() {
            in_word = false;
            continue;
        }
        if !in_word {
            in_word = true;
            word_count += 1;
            if word_count == n {
                return &args[i..];
            }
        }
    }
    ""
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(entries: &[(&str, &str)]) -> AliasStore {
        let mut s = AliasStore::new();
        for (n, e) in entries {
            s.set(Alias::new(*n, *e));
        }
        s
    }

    #[test]
    fn no_alias_passes_through() {
        let s = AliasStore::new();
        assert_eq!(s.expand_line("look").unwrap(), vec!["look".to_string()]);
    }

    #[test]
    fn simple_alias_expands() {
        let s = store(&[("greet", "wave")]);
        assert_eq!(s.expand_line("greet").unwrap(), vec!["wave".to_string()]);
    }

    #[test]
    fn alias_with_param_zero_takes_full_args() {
        let s = store(&[("chat", "say %0")]);
        assert_eq!(
            s.expand_line("chat hello there").unwrap(),
            vec!["say hello there".to_string()]
        );
    }

    #[test]
    fn alias_with_positional_params() {
        let s = store(&[("kill", "attack %1 with %2")]);
        assert_eq!(
            s.expand_line("kill goblin sword").unwrap(),
            vec!["attack goblin with sword".to_string()]
        );
    }

    #[test]
    fn missing_positional_param_substitutes_empty() {
        // %1 with no args expands to nothing. Trailing whitespace is trimmed.
        let s = store(&[("strike", "kick %1")]);
        assert_eq!(s.expand_line("strike").unwrap(), vec!["kick".to_string()]);
    }

    #[test]
    fn alias_self_recursion_hits_limit() {
        // `#alias say {say %0}` is the classic infinite loop. The depth
        // guard catches it instead of running away.
        let s = store(&[("say", "say %0")]);
        assert!(matches!(
            s.expand_line("say hello"),
            Err(ExpandError::RecursionLimit(_))
        ));
    }

    #[test]
    fn range_param_takes_from_nth_word_onward() {
        let s = store(&[("tell", "%1 says: %2-")]);
        assert_eq!(
            s.expand_line("tell bob hello there friend").unwrap(),
            vec!["bob says: hello there friend".to_string()]
        );
    }

    #[test]
    fn range_param_preserves_original_whitespace() {
        // `%2-` must keep the literal spacing between words instead of
        // collapsing to single spaces via `split_whitespace`. This is
        // why `args_from_word` walks `args` directly.
        let s = store(&[("echo", "[%2-]")]);
        assert_eq!(
            s.expand_line("echo skip  foo   bar").unwrap(),
            vec!["[foo   bar]".to_string()]
        );
    }

    #[test]
    fn range_param_one_dash_is_full_args() {
        // `%1-` is the same as `%0`: every word starting from the first.
        let s = store(&[("a", "%1-"), ("b", "%0")]);
        assert_eq!(
            s.expand_line("a foo bar baz").unwrap(),
            vec!["foo bar baz".to_string()]
        );
        assert_eq!(
            s.expand_line("b foo bar baz").unwrap(),
            vec!["foo bar baz".to_string()]
        );
    }

    #[test]
    fn range_param_zero_dash_is_full_args() {
        // `%0-` is a degenerate but harmless form: it carries the same
        // meaning as `%0` and `%1-`. Documented so the parser is
        // predictable rather than rejecting it.
        let s = store(&[("a", "%0-")]);
        assert_eq!(
            s.expand_line("a foo bar baz").unwrap(),
            vec!["foo bar baz".to_string()]
        );
    }

    #[test]
    fn range_param_with_too_few_words_expands_empty() {
        let s = store(&[("tell", "%1 says: %3-")]);
        assert_eq!(
            s.expand_line("tell bob hi").unwrap(),
            vec!["bob says:".to_string()]
        );
    }

    #[test]
    fn range_param_with_no_args_expands_empty() {
        let s = store(&[("emote", "[%1-]")]);
        assert_eq!(s.expand_line("emote").unwrap(), vec!["[]".to_string()]);
    }

    #[test]
    fn double_percent_is_literal() {
        let s = store(&[("scream", "say 100%% effort")]);
        assert_eq!(
            s.expand_line("scream").unwrap(),
            vec!["say 100% effort".to_string()]
        );
    }

    #[test]
    fn semicolon_separated_expansion_yields_multiple_commands() {
        let s = store(&[("morning", "wave;bow;say good morning")]);
        assert_eq!(
            s.expand_line("morning").unwrap(),
            vec![
                "wave".to_string(),
                "bow".to_string(),
                "say good morning".to_string()
            ]
        );
    }

    #[test]
    fn recursive_alias_expands_chain() {
        let s = store(&[("a", "b"), ("b", "c")]);
        assert_eq!(s.expand_line("a").unwrap(), vec!["c".to_string()]);
    }

    #[test]
    fn cyclic_alias_hits_recursion_limit() {
        let s = store(&[("a", "b"), ("b", "a")]);
        assert!(matches!(
            s.expand_line("a"),
            Err(ExpandError::RecursionLimit(_))
        ));
    }

    #[test]
    fn disabled_alias_passes_through() {
        let mut s = store(&[("greet", "wave")]);
        let mut alias = s.get("greet").unwrap().clone();
        alias.enabled = false;
        s.set(alias);
        assert_eq!(s.expand_line("greet").unwrap(), vec!["greet".to_string()]);
    }

    #[test]
    fn user_input_with_semicolons_splits_first() {
        let s = AliasStore::new();
        assert_eq!(
            s.expand_line("look;sip water").unwrap(),
            vec!["look".to_string(), "sip water".to_string()]
        );
    }

    #[test]
    fn escaped_semicolon_stays_literal() {
        let s = AliasStore::new();
        assert_eq!(
            s.expand_line("say hello\\;world").unwrap(),
            vec!["say hello;world".to_string()]
        );
    }

    #[test]
    fn disabled_group_passes_alias_through() {
        let mut s = AliasStore::new();
        s.set(Alias::new("kk", "kick %1").with_group("Combat"));
        // Group enabled by default — the alias fires.
        assert_eq!(
            s.expand_line("kk goblin").unwrap(),
            vec!["kick goblin".to_string()]
        );
        // Disable the whole group and the alias passes through as
        // typed (no expansion, no error).
        s.set_group_enabled("Combat", false);
        assert_eq!(
            s.expand_line("kk goblin").unwrap(),
            vec!["kk goblin".to_string()]
        );
        // Re-enable and it fires again.
        s.set_group_enabled("Combat", true);
        assert_eq!(
            s.expand_line("kk goblin").unwrap(),
            vec!["kick goblin".to_string()]
        );
    }

    #[test]
    fn ungrouped_alias_ignores_group_state() {
        // Sanity check that the disabled-set never affects ungrouped
        // aliases (a buggy is_group_enabled check might reach for an
        // empty-string entry).
        let mut s = AliasStore::new();
        s.set(Alias::new("greet", "wave"));
        s.set_disabled_groups([String::new(), "Combat".to_string()]);
        assert_eq!(s.expand_line("greet").unwrap(), vec!["wave".to_string()]);
    }

    #[test]
    fn groups_lists_referenced_names_with_enabled_state() {
        let mut s = AliasStore::new();
        s.set(Alias::new("kk", "kick %1").with_group("Combat"));
        s.set(Alias::new("forge", "smith %1").with_group("Crafting"));
        s.set(Alias::new("greet", "wave"));
        s.set_group_enabled("Combat", false);
        let mut groups = s.groups();
        groups.sort();
        assert_eq!(
            groups,
            vec![
                ("Combat".to_string(), false),
                ("Crafting".to_string(), true)
            ]
        );
    }

    #[test]
    fn disabled_groups_round_trip_through_setters() {
        let mut s = AliasStore::new();
        s.set_disabled_groups(["Combat", "Crafting"]);
        let mut listed = s.disabled_groups();
        listed.sort();
        assert_eq!(listed, vec!["Combat".to_string(), "Crafting".to_string()]);
    }

    #[test]
    fn remove_alias() {
        let mut s = store(&[("greet", "wave")]);
        assert!(s.remove("greet"));
        assert!(!s.remove("greet"));
        assert_eq!(s.expand_line("greet").unwrap(), vec!["greet".to_string()]);
    }

    #[test]
    fn revision_moves_only_when_the_list_changes() {
        let mut s = AliasStore::new();
        let empty = s.revision();
        s.set(Alias::new("greet", "wave").with_group("social"));
        let after_add = s.revision();
        assert_ne!(after_add, empty);
        s.set_group_enabled("social", false);
        s.set_disabled_groups(["social"]);
        assert!(!s.remove("missing"));
        assert_eq!(s.revision(), after_add);
        s.set(Alias::new("greet", "bow"));
        let after_replace = s.revision();
        assert_ne!(after_replace, after_add);
        assert!(s.remove("greet"));
        assert_ne!(s.revision(), after_replace);
    }

    #[test]
    fn a_replacement_store_reads_as_a_change() {
        let first = store(&[("greet", "wave")]);
        let second = store(&[("greet", "wave")]);
        assert_ne!(first.revision(), second.revision());
    }

    #[test]
    fn set_overwrites_existing_alias() {
        let mut s = store(&[("greet", "wave")]);
        s.set(Alias::new("greet", "bow"));
        assert_eq!(s.expand_line("greet").unwrap(), vec!["bow".to_string()]);
    }

    #[test]
    fn a_stopped_alias_passes_through_until_you_save_it() {
        let mut s = store(&[("hl", "cast heal")]);
        s.stop("hl");
        assert!(s.is_stopped("hl"));
        assert_eq!(s.expand_line("hl").unwrap(), vec!["hl".to_string()]);
        s.set(Alias::new("hl", "cast heal"));
        assert!(!s.is_stopped("hl"));
        assert_eq!(s.expand_line("hl").unwrap(), vec!["cast heal".to_string()]);
    }

    #[test]
    fn a_whole_list_save_keeps_only_the_unchanged_alias_stops() {
        let mut old = store(&[("hl", "cast heal"), ("kk", "kick")]);
        old.stop("hl");
        old.stop("kk");
        let mut saved = store(&[("hl", "cast heal"), ("kk", "kick %1")]);
        saved.keep_stops_from(&old);
        assert!(saved.is_stopped("hl"));
        assert!(!saved.is_stopped("kk"));
        assert!(saved.remove("hl"));
        assert!(!saved.is_stopped("hl"));
    }

    #[test]
    fn list_returns_sorted_aliases() {
        let s = store(&[("zeta", "z"), ("alpha", "a"), ("mu", "m")]);
        let names: Vec<_> = s.list().iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "mu", "zeta"]);
    }

    #[test]
    fn a_script_alias_queues_its_body_with_the_words_after_its_name() {
        let mut s = store(&[("hunt", "kk %1;flee")]);
        let body = "mud.send('kick ' .. captures[1])";
        s.set(Alias::new("kk", "ignored").with_script(body));
        let kick = |captures: &[&str]| {
            ExpandStep::Script(ScriptCall {
                source: "kk".into(),
                body: body.into(),
                captures: captures.iter().map(|c| (*c).to_string()).collect(),
            })
        };
        // Each body stands where its alias was typed, between the
        // commands around it, at any depth.
        assert_eq!(
            s.expand_line_full("kk  big   dragon;look;hunt rat;wave")
                .unwrap(),
            vec![
                kick(&["big", "dragon"]),
                ExpandStep::Command("look".into()),
                kick(&["rat"]),
                ExpandStep::Command("flee".into()),
                ExpandStep::Command("wave".into()),
            ]
        );
        assert_eq!(
            s.expand_line("kk dragon;look;hunt rat").unwrap(),
            vec!["look".to_string(), "flee".to_string()]
        );
    }

    #[test]
    fn recursion_limit_can_be_lowered() {
        let s = store(&[("a", "a")]).with_max_depth(2);
        assert!(matches!(
            s.expand_line("a"),
            Err(ExpandError::RecursionLimit(2))
        ));
    }
}

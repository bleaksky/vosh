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

use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use thiserror::Error;

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
        }
    }

    /// Builder-style setter for the optional group tag.
    #[must_use]
    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }
}

#[derive(Debug, Error)]
pub enum ExpandError {
    #[error("alias recursion limit exceeded ({0})")]
    RecursionLimit(usize),
}

/// Default cap on alias recursion depth. Matches the `TinTin++` default.
pub const DEFAULT_MAX_DEPTH: usize = 16;

/// Default command separator inside an expansion.
pub const DEFAULT_SEPARATOR: char = ';';

#[derive(Debug, Clone)]
pub struct AliasStore {
    aliases: HashMap<String, Alias>,
    max_depth: usize,
    separator: char,
    /// Group names the user has turned OFF as a bulk override. An
    /// alias whose `group` is in this set is treated as disabled
    /// regardless of its own `enabled` flag. Stored as the inverse
    /// (disabled list) rather than enabled list so a freshly added
    /// group defaults to enabled.
    disabled_groups: BTreeSet<String>,
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
            separator: DEFAULT_SEPARATOR,
            disabled_groups: BTreeSet::new(),
        }
    }

    /// True when the named group is effectively enabled. Returns true
    /// for an empty / missing group name (ungrouped aliases never
    /// participate in the group-disable mechanism).
    pub fn is_group_enabled(&self, group: &str) -> bool {
        group.is_empty() || !self.disabled_groups.contains(group)
    }

    /// Toggle a whole group. Calling with `enabled = true` removes
    /// the group from the disabled set; with `false` adds it.
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

    /// Sorted list of every group name referenced by at least one
    /// alias, paired with whether that group is currently enabled.
    /// Used by the Settings UI to render the per-group toggle row.
    pub fn groups(&self) -> Vec<(String, bool)> {
        let mut names: BTreeSet<String> = BTreeSet::new();
        for a in self.aliases.values() {
            if let Some(g) = &a.group {
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

    /// Persistence accessor for the disabled-groups set. Returns the
    /// names that should be saved alongside the alias list.
    pub fn disabled_groups(&self) -> Vec<String> {
        self.disabled_groups.iter().cloned().collect()
    }

    /// Persistence inverse of `disabled_groups()`. Replaces the
    /// current disabled-set with the supplied names.
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

    #[must_use]
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    pub fn set(&mut self, alias: Alias) {
        self.aliases.insert(alias.name.clone(), alias);
    }

    pub fn remove(&mut self, name: &str) -> bool {
        self.aliases.remove(name).is_some()
    }

    pub fn get(&self, name: &str) -> Option<&Alias> {
        self.aliases.get(name)
    }

    pub fn list(&self) -> Vec<&Alias> {
        let mut out: Vec<&Alias> = self.aliases.values().collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub fn separator(&self) -> char {
        self.separator
    }

    /// Expand a single command line. Splits on the configured separator,
    /// then resolves each piece through aliases until either no alias
    /// matches or the recursion limit is hit. Returns the resulting list of
    /// commands ready to send to the server.
    pub fn expand_line(&self, line: &str) -> Result<Vec<String>, ExpandError> {
        let mut out = Vec::new();
        for raw in split_commands(line, self.separator) {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            self.expand_into(trimmed, 0, &mut out)?;
        }
        Ok(out)
    }

    fn expand_into(
        &self,
        command: &str,
        depth: usize,
        out: &mut Vec<String>,
    ) -> Result<(), ExpandError> {
        if depth >= self.max_depth {
            return Err(ExpandError::RecursionLimit(self.max_depth));
        }

        let (name, rest) = split_first_word(command);
        // The alias fires only when:
        //   * the named entry exists, AND
        //   * its own `enabled` flag is true, AND
        //   * its group is enabled (or it is ungrouped).
        // Disabled groups short-circuit to pass-through so the user
        // can flip whole "Combat" / "Crafting" loadouts off without
        // editing each row.
        let Some(alias) = self.aliases.get(name).filter(|a| {
            a.enabled
                && a.group
                    .as_deref()
                    .map_or(true, |g| !self.disabled_groups.contains(g))
        }) else {
            out.push(command.to_string());
            return Ok(());
        };

        let expanded = substitute_params(&alias.expansion, rest);
        for raw in split_commands(&expanded, self.separator) {
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

/// Split on a single-byte separator, treating `\;` as a literal `;`. Returns
/// non-empty pieces with the escape removed.
fn split_commands(input: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(&next) = chars.peek() {
                if next == sep || next == '\\' {
                    current.push(next);
                    chars.next();
                    continue;
                }
            }
            current.push(ch);
            continue;
        }
        if ch == sep {
            out.push(std::mem::take(&mut current));
            continue;
        }
        current.push(ch);
    }
    out.push(current);
    out
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
    fn set_overwrites_existing_alias() {
        let mut s = store(&[("greet", "wave")]);
        s.set(Alias::new("greet", "bow"));
        assert_eq!(s.expand_line("greet").unwrap(), vec!["bow".to_string()]);
    }

    #[test]
    fn list_returns_sorted_aliases() {
        let s = store(&[("zeta", "z"), ("alpha", "a"), ("mu", "m")]);
        let names: Vec<_> = s.list().iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "mu", "zeta"]);
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

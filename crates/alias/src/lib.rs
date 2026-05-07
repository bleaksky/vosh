//! Alias engine with positional substitution and a recursion-depth guard.
//!
//! An alias matches the first whitespace-separated word of a command. The
//! expansion may contain `%0` (the entire arg tail), `%1` through `%9` (the
//! Nth space-separated word in the arg tail), and `%%` (a literal `%`).
//! Multiple commands separated by `;` in an expansion are split and each is
//! re-fed through the engine, bounded by a maximum recursion depth.

use std::collections::HashMap;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias {
    pub name: String,
    pub expansion: String,
    pub enabled: bool,
}

impl Alias {
    pub fn new(name: impl Into<String>, expansion: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            expansion: expansion.into(),
            enabled: true,
        }
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
        }
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
        let Some(alias) = self.aliases.get(name).filter(|a| a.enabled) else {
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
                if idx == 0 {
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

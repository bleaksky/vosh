//! Variable store with two scopes and `$name` interpolation.
//!
//! Profile scope persists across sessions (Phase 9 lands disk persistence).
//! Session scope clears on reconnect. Lookups resolve session before profile,
//! so a session set hides the profile value until cleared.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Profile,
    Session,
}

#[derive(Debug, Default, Clone)]
pub struct VariableStore {
    profile: HashMap<String, String>,
    session: HashMap<String, String>,
}

impl VariableStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, scope: Scope, name: impl Into<String>, value: impl Into<String>) {
        let name = name.into();
        let value = value.into();
        match scope {
            Scope::Profile => {
                self.profile.insert(name, value);
            }
            Scope::Session => {
                self.session.insert(name, value);
            }
        }
    }

    /// Remove from both scopes. Returns true if anything changed.
    pub fn remove(&mut self, name: &str) -> bool {
        let s = self.session.remove(name).is_some();
        let p = self.profile.remove(name).is_some();
        s || p
    }

    /// Resolve a variable. Session wins, then profile.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.session
            .get(name)
            .or_else(|| self.profile.get(name))
            .map(String::as_str)
    }

    /// Drop every session-scoped value. Call on disconnect.
    pub fn clear_session(&mut self) {
        self.session.clear();
    }

    /// Iterate session and profile entries together. Session entries shadow
    /// profile entries with the same name.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str, Scope)> + '_ {
        self.session
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str(), Scope::Session))
            .chain(self.profile.iter().filter_map(move |(k, v)| {
                if self.session.contains_key(k) {
                    None
                } else {
                    Some((k.as_str(), v.as_str(), Scope::Profile))
                }
            }))
    }

    /// Substitute `$name` and `${name}` references in a string with their
    /// current values. `$$` becomes a literal `$`. Unknown names pass through
    /// verbatim including the leading `$`, matching `TinTin++` behavior.
    pub fn interpolate(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if b != b'$' {
                let ch_end = next_char_boundary(text, i);
                out.push_str(&text[i..ch_end]);
                i = ch_end;
                continue;
            }
            // We are at a $.
            if i + 1 >= bytes.len() {
                out.push('$');
                i += 1;
                continue;
            }
            let next = bytes[i + 1];
            if next == b'$' {
                out.push('$');
                i += 2;
                continue;
            }
            if next == b'{' {
                if let Some(close) = bytes[i + 2..].iter().position(|&c| c == b'}') {
                    let name = &text[i + 2..i + 2 + close];
                    if is_valid_name(name) {
                        if let Some(val) = self.get(name) {
                            out.push_str(val);
                        } else {
                            out.push_str(&text[i..i + 3 + close]);
                        }
                        i = i + 3 + close;
                        continue;
                    }
                }
                out.push('$');
                i += 1;
                continue;
            }
            if next.is_ascii_alphabetic() || next == b'_' {
                let mut end = i + 1;
                while end < bytes.len()
                    && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
                {
                    end += 1;
                }
                let name = &text[i + 1..end];
                if let Some(val) = self.get(name) {
                    out.push_str(val);
                } else {
                    out.push_str(&text[i..end]);
                }
                i = end;
                continue;
            }
            out.push('$');
            i += 1;
        }
        out
    }
}

fn is_valid_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn next_char_boundary(s: &str, start: usize) -> usize {
    let mut i = start + 1;
    while !s.is_char_boundary(i) && i < s.len() {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(pairs: &[(Scope, &str, &str)]) -> VariableStore {
        let mut v = VariableStore::new();
        for (scope, k, val) in pairs {
            v.set(*scope, *k, *val);
        }
        v
    }

    #[test]
    fn session_shadows_profile() {
        let v = store(&[
            (Scope::Profile, "name", "Adan"),
            (Scope::Session, "name", "Aleph"),
        ]);
        assert_eq!(v.get("name"), Some("Aleph"));
    }

    #[test]
    fn falls_back_to_profile() {
        let v = store(&[(Scope::Profile, "name", "Adan")]);
        assert_eq!(v.get("name"), Some("Adan"));
    }

    #[test]
    fn clear_session_does_not_touch_profile() {
        let mut v = store(&[
            (Scope::Profile, "name", "Adan"),
            (Scope::Session, "hp", "100"),
        ]);
        v.clear_session();
        assert_eq!(v.get("hp"), None);
        assert_eq!(v.get("name"), Some("Adan"));
    }

    #[test]
    fn interpolate_simple_name() {
        let v = store(&[(Scope::Session, "target", "goblin")]);
        assert_eq!(v.interpolate("kick $target"), "kick goblin");
    }

    #[test]
    fn interpolate_braced_name() {
        let v = store(&[(Scope::Session, "x", "abc")]);
        assert_eq!(v.interpolate("a${x}b"), "aabcb");
    }

    #[test]
    fn double_dollar_escapes() {
        let v = VariableStore::new();
        assert_eq!(v.interpolate("price $$50"), "price $50");
    }

    #[test]
    fn unknown_name_passes_through() {
        let v = VariableStore::new();
        assert_eq!(v.interpolate("hello $stranger!"), "hello $stranger!");
    }

    #[test]
    fn dollar_followed_by_punctuation_passes_through() {
        let v = VariableStore::new();
        assert_eq!(v.interpolate("cost $100"), "cost $100");
    }

    #[test]
    fn interpolate_with_utf8() {
        let v = store(&[(Scope::Session, "drag", "\u{1f409}")]);
        assert_eq!(v.interpolate("see $drag now"), "see \u{1f409} now");
    }

    #[test]
    fn interpolate_underscores_and_digits_in_name() {
        let v = store(&[(Scope::Session, "weapon_2", "axe")]);
        assert_eq!(v.interpolate("wield $weapon_2"), "wield axe");
    }

    #[test]
    fn iter_combines_scopes_with_session_priority() {
        let v = store(&[
            (Scope::Profile, "a", "p"),
            (Scope::Profile, "b", "p"),
            (Scope::Session, "a", "s"),
        ]);
        let mut entries: Vec<_> = v
            .iter()
            .map(|(k, val, _)| (k.to_string(), val.to_string()))
            .collect();
        entries.sort();
        assert_eq!(
            entries,
            vec![
                ("a".to_string(), "s".to_string()),
                ("b".to_string(), "p".to_string()),
            ]
        );
    }
}

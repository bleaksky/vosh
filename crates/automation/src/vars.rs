//! Variables in two scopes, and `$name` interpolation.
//!
//! Profile scope persists and saves in the profile file, and every session
//! on the profile reads it. Session scope belongs to one session and
//! clears as it connects. Each scope keeps a [`VariableStore`] of its
//! own, and a [`VarView`] reads the two together, session first, so a
//! session value hides the profile value of the same name until it clears.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Profile,
    Session,
}

/// The variables of one scope.
#[derive(Debug, Default, Clone)]
pub struct VariableStore {
    vars: HashMap<String, String>,
}

impl VariableStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.vars.insert(name.into(), value.into());
    }

    /// Remove `name`. Returns true if it was set.
    pub fn remove(&mut self, name: &str) -> bool {
        self.vars.remove(name).is_some()
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.vars.get(name).map(String::as_str)
    }

    pub fn clear(&mut self) {
        self.vars.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> + '_ {
        self.vars.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

/// A session's variables over its profile's, as a lookup reads them.
#[derive(Debug, Clone, Copy)]
pub struct VarView<'a> {
    pub session: &'a VariableStore,
    pub profile: &'a VariableStore,
}

impl<'a> VarView<'a> {
    /// Resolve a variable. Session wins, then profile.
    pub fn get(self, name: &str) -> Option<&'a str> {
        self.session.get(name).or_else(|| self.profile.get(name))
    }

    /// Every variable a lookup finds, the session entries first, then
    /// each profile entry no session entry of its name hides.
    pub fn iter(self) -> impl Iterator<Item = (&'a str, &'a str, Scope)> {
        let session = self.session;
        session.iter().map(|(k, v)| (k, v, Scope::Session)).chain(
            self.profile
                .iter()
                .filter(move |(k, _)| session.get(k).is_none())
                .map(|(k, v)| (k, v, Scope::Profile)),
        )
    }

    /// Substitute `$name` and `${name}` references in a string with their
    /// current values. `$$` becomes a literal `$`. Unknown names pass through
    /// verbatim including the leading `$`, matching `TinTin++` behavior.
    pub fn interpolate(self, text: &str) -> String {
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

    /// The session's store and the profile's, from `pairs`.
    fn stores(pairs: &[(Scope, &str, &str)]) -> [VariableStore; 2] {
        let [mut session, mut profile] = [VariableStore::new(), VariableStore::new()];
        for (scope, k, val) in pairs {
            match scope {
                Scope::Session => session.set(*k, *val),
                Scope::Profile => profile.set(*k, *val),
            }
        }
        [session, profile]
    }

    fn view(stores: &[VariableStore; 2]) -> VarView<'_> {
        VarView {
            session: &stores[0],
            profile: &stores[1],
        }
    }

    #[test]
    fn session_shadows_profile() {
        let v = stores(&[
            (Scope::Profile, "name", "Adan"),
            (Scope::Session, "name", "Aleph"),
        ]);
        assert_eq!(view(&v).get("name"), Some("Aleph"));
        assert_eq!(v[1].get("name"), Some("Adan"));
    }

    #[test]
    fn falls_back_to_profile() {
        let v = stores(&[(Scope::Profile, "name", "Adan")]);
        assert_eq!(view(&v).get("name"), Some("Adan"));
    }

    #[test]
    fn a_session_value_that_goes_shows_the_profile_value_again() {
        let mut v = stores(&[
            (Scope::Profile, "name", "Adan"),
            (Scope::Session, "name", "Aleph"),
            (Scope::Session, "hp", "100"),
        ]);
        assert!(v[0].remove("name"));
        assert!(!v[0].remove("name"));
        assert_eq!(view(&v).get("name"), Some("Adan"));
        v[0].clear();
        assert_eq!(view(&v).get("hp"), None);
        assert_eq!(view(&v).get("name"), Some("Adan"));
    }

    #[test]
    fn interpolate_simple_name() {
        let v = stores(&[(Scope::Session, "target", "goblin")]);
        assert_eq!(view(&v).interpolate("kick $target"), "kick goblin");
    }

    #[test]
    fn interpolate_braced_name() {
        let v = stores(&[(Scope::Session, "x", "abc")]);
        assert_eq!(view(&v).interpolate("a${x}b"), "aabcb");
    }

    #[test]
    fn interpolate_reads_both_scopes_session_first() {
        let v = stores(&[
            (Scope::Profile, "target", "orc"),
            (Scope::Profile, "home", "Hollow"),
            (Scope::Session, "target", "goblin"),
        ]);
        assert_eq!(
            view(&v).interpolate("kick $target, recall to ${home}"),
            "kick goblin, recall to Hollow"
        );
    }

    #[test]
    fn double_dollar_escapes() {
        let v = stores(&[]);
        assert_eq!(view(&v).interpolate("price $$50"), "price $50");
    }

    #[test]
    fn unknown_name_passes_through() {
        let v = stores(&[]);
        assert_eq!(view(&v).interpolate("hello $stranger!"), "hello $stranger!");
    }

    #[test]
    fn dollar_followed_by_punctuation_passes_through() {
        let v = stores(&[]);
        assert_eq!(view(&v).interpolate("cost $100"), "cost $100");
    }

    #[test]
    fn interpolate_with_utf8() {
        let v = stores(&[(Scope::Session, "drag", "\u{1f409}")]);
        assert_eq!(view(&v).interpolate("see $drag now"), "see \u{1f409} now");
    }

    #[test]
    fn interpolate_underscores_and_digits_in_name() {
        let v = stores(&[(Scope::Session, "weapon_2", "axe")]);
        assert_eq!(view(&v).interpolate("wield $weapon_2"), "wield axe");
    }

    #[test]
    fn iter_lists_the_session_first_and_each_name_once() {
        let v = stores(&[
            (Scope::Profile, "a", "p"),
            (Scope::Profile, "b", "p"),
            (Scope::Profile, "c", "p"),
            (Scope::Session, "a", "s"),
            (Scope::Session, "d", "s"),
        ]);
        let entries: Vec<_> = view(&v).iter().collect();
        let scopes: Vec<Scope> = entries.iter().map(|(_, _, scope)| *scope).collect();
        assert_eq!(
            scopes,
            [
                Scope::Session,
                Scope::Session,
                Scope::Profile,
                Scope::Profile
            ]
        );
        let mut found: Vec<_> = entries.iter().map(|(k, val, _)| (*k, *val)).collect();
        found.sort_unstable();
        assert_eq!(found, [("a", "s"), ("b", "p"), ("c", "p"), ("d", "s")]);
    }
}

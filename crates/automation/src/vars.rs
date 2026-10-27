//! Variables and lists in two scopes, and `$name` and `$name[1]`
//! interpolation.
//!
//! Profile scope persists and saves in the profile file, and every session
//! on the profile reads it. Session scope belongs to one session and
//! clears as it connects. Each scope keeps a [`VariableStore`] of its
//! own, and a [`VarView`] reads the two together, session first, so a
//! session value hides the profile value of the same name until it clears.

use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Profile,
    Session,
}

/// The most items a list holds.
pub const LIST_ITEMS: usize = 500;

/// The variables and lists of one scope. A name is a variable or a list,
/// never both, so setting one takes away the other of that name. A list
/// is shared, so the snapshot Lua reads takes it without a copy, and a
/// change copies it only while a snapshot still holds it.
#[derive(Debug, Default, Clone)]
pub struct VariableStore {
    vars: HashMap<String, String>,
    lists: HashMap<String, Arc<Vec<String>>>,
}

impl VariableStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name = name.into();
        self.lists.remove(&name);
        self.vars.insert(name, value.into());
    }

    /// Remove the variable or the list `name`. Returns true if one was set.
    pub fn remove(&mut self, name: &str) -> bool {
        let var = self.vars.remove(name).is_some();
        let list = self.lists.remove(name).is_some();
        var || list
    }

    /// Put the list `name` in place, taking away a variable of that name.
    pub fn set_list(&mut self, name: impl Into<String>, items: Vec<String>) {
        let name = name.into();
        self.vars.remove(&name);
        self.lists.insert(name, Arc::new(items));
    }

    pub fn list(&self, name: &str) -> Option<&[String]> {
        self.lists.get(name).map(|items| items.as_slice())
    }

    /// The shared list `name`, as a snapshot takes it.
    pub fn shared_list(&self, name: &str) -> Option<&Arc<Vec<String>>> {
        self.lists.get(name)
    }

    pub fn list_mut(&mut self, name: &str) -> Option<&mut Vec<String>> {
        self.lists.get_mut(name).map(Arc::make_mut)
    }

    /// Every list, by name.
    pub fn lists(&self) -> impl Iterator<Item = (&str, &[String])> + '_ {
        self.lists.iter().map(|(k, v)| (k.as_str(), v.as_slice()))
    }

    /// Take the list `name` out of this scope.
    pub fn take_list(&mut self, name: &str) -> Option<Vec<String>> {
        self.lists
            .remove(name)
            .map(|items| Arc::try_unwrap(items).unwrap_or_else(|shared| (*shared).clone()))
    }

    /// Whether this scope holds `name`, as a variable or as a list.
    pub fn holds(&self, name: &str) -> bool {
        self.vars.contains_key(name) || self.lists.contains_key(name)
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.vars.get(name).map(String::as_str)
    }

    pub fn clear(&mut self) {
        self.vars.clear();
        self.lists.clear();
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
    /// The scope that decides `name`: the session when it holds the
    /// name as a variable or a list, else the profile. So a session
    /// variable hides a profile list of its name, and the reverse.
    fn scope_of(self, name: &str) -> &'a VariableStore {
        if self.session.holds(name) {
            self.session
        } else {
            self.profile
        }
    }

    /// Resolve a variable. Session wins, then profile.
    pub fn get(self, name: &str) -> Option<&'a str> {
        self.scope_of(name).get(name)
    }

    /// Resolve a list, with the scope that holds it. Session wins, then
    /// profile.
    pub fn list(self, name: &str) -> Option<(&'a [String], Scope)> {
        let scope = if self.session.holds(name) {
            Scope::Session
        } else {
            Scope::Profile
        };
        self.scope_of(name).list(name).map(|items| (items, scope))
    }

    /// Every list a lookup finds, the session's first, each shared.
    pub fn shared_lists(self) -> impl Iterator<Item = (&'a str, &'a Arc<Vec<String>>)> {
        let session = self.session;
        let ours = session.lists.iter().map(|(k, v)| (k.as_str(), v));
        let theirs = self
            .profile
            .lists
            .iter()
            .filter(move |(k, _)| !session.holds(k))
            .map(|(k, v)| (k.as_str(), v));
        ours.chain(theirs)
    }

    /// Every list a lookup finds, the session's first.
    pub fn lists(self) -> impl Iterator<Item = (&'a str, &'a [String], Scope)> {
        let session = self.session;
        session.lists().map(|(k, v)| (k, v, Scope::Session)).chain(
            self.profile
                .lists()
                .filter(move |(k, _)| !session.holds(k))
                .map(|(k, v)| (k, v, Scope::Profile)),
        )
    }

    /// Every variable a lookup finds, the session entries first, then
    /// each profile entry no session entry of its name hides.
    pub fn iter(self) -> impl Iterator<Item = (&'a str, &'a str, Scope)> {
        let session = self.session;
        session.iter().map(|(k, v)| (k, v, Scope::Session)).chain(
            self.profile
                .iter()
                .filter(move |(k, _)| !session.holds(k))
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
                if let Some((value, used)) = self.list_item(name, &text[end..]) {
                    out.push_str(&value);
                    i = end + used;
                    continue;
                }
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

impl VarView<'_> {
    /// What `[index]` at the start of `after` reads from the list `name`,
    /// and how many bytes of `after` it took: an item counted from 1, or
    /// from the end below 0, or `#` for how many items it holds. An index
    /// past the end reads empty. None when `name` is no list or `after`
    /// holds no index.
    fn list_item(self, name: &str, after: &str) -> Option<(String, usize)> {
        let rest = after.strip_prefix('[')?;
        let close = rest.find(']')?;
        let index = rest[..close].trim();
        let (items, _) = self.list(name)?;
        let used = close + 2;
        if index == "#" {
            return Some((items.len().to_string(), used));
        }
        let n: i64 = index.parse().ok()?;
        let len = i64::try_from(items.len()).ok()?;
        let at = if n < 0 { len + n } else { n - 1 };
        let item = usize::try_from(at).ok().and_then(|at| items.get(at));
        Some((item.cloned().unwrap_or_default(), used))
    }
}

/// The items of `args`, words apart, or a run in braces as one item, as
/// `#list name add Orla {the guard}` gives `Orla` and `the guard`. A `\}`
/// inside braces is a brace of the item.
pub fn split_items(args: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut rest = args.trim_start();
    while !rest.is_empty() {
        if let Some(inner) = rest.strip_prefix('{') {
            let mut item = String::new();
            let mut chars = inner.char_indices();
            let mut end = inner.len();
            while let Some((at, c)) = chars.next() {
                match c {
                    '\\' if inner[at + 1..].starts_with('}') => {
                        item.push('}');
                        chars.next();
                    }
                    '}' => {
                        end = at + 1;
                        break;
                    }
                    _ => item.push(c),
                }
            }
            items.push(item);
            rest = inner[end.min(inner.len())..].trim_start();
        } else {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            items.push(rest[..end].to_string());
            rest = rest[end..].trim_start();
        }
    }
    items
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

    fn foes() -> Vec<String> {
        vec!["Orla".into(), "the guard".into(), "Maren".into()]
    }

    #[test]
    fn a_list_reads_by_place_from_either_end_and_by_count() {
        let mut session = VariableStore::new();
        session.set_list("foes", foes());
        let profile = VariableStore::new();
        let view = VarView {
            session: &session,
            profile: &profile,
        };
        assert_eq!(view.interpolate("kill $foes[1]"), "kill Orla");
        assert_eq!(view.interpolate("kill $foes[-1]"), "kill Maren");
        assert_eq!(view.interpolate("say $foes[#] foes"), "say 3 foes");
        assert_eq!(view.interpolate("kill $foes[9]."), "kill .");
        // Not an index, and not a list, pass through as typed.
        assert_eq!(view.interpolate("say $foes[x]"), "say $foes[x]");
        assert_eq!(view.interpolate("say $foes"), "say $foes");
        assert_eq!(view.interpolate("say $nope[1]"), "say $nope[1]");
    }

    #[test]
    fn a_name_is_a_variable_or_a_list_never_both() {
        let mut store = VariableStore::new();
        store.set("foes", "Orla");
        store.set_list("foes", foes());
        assert_eq!(store.get("foes"), None);
        assert_eq!(store.list("foes").map(<[String]>::len), Some(3));
        store.set("foes", "Orla");
        assert_eq!(store.list("foes"), None);
        assert!(store.remove("foes"));
        assert!(!store.remove("foes"));
    }

    #[test]
    fn a_session_list_hides_the_profile_list_of_its_name() {
        let mut session = VariableStore::new();
        let mut profile = VariableStore::new();
        profile.set_list("foes", foes());
        profile.set_list("friends", vec!["Tolliver".into()]);
        session.set_list("foes", vec!["rat".into()]);
        let view = VarView {
            session: &session,
            profile: &profile,
        };
        assert_eq!(view.interpolate("$foes[1] $friends[1]"), "rat Tolliver");
        let mut names: Vec<_> = view.lists().map(|(n, _, scope)| (n, scope)).collect();
        names.sort_unstable_by_key(|(n, _)| *n);
        assert_eq!(
            names,
            [("foes", Scope::Session), ("friends", Scope::Profile)]
        );
    }

    #[test]
    fn items_split_at_spaces_and_braces_hold_one() {
        assert_eq!(
            split_items("Orla {the guard} Maren"),
            ["Orla", "the guard", "Maren"]
        );
        assert_eq!(split_items(r"{a \} b}"), ["a } b"]);
        assert_eq!(split_items("  "), Vec::<String>::new());
        assert_eq!(split_items("{open"), ["open"]);
    }

    #[test]
    fn a_session_variable_hides_a_profile_list_of_its_name() {
        let mut session = VariableStore::new();
        let mut profile = VariableStore::new();
        profile.set_list("foes", vec!["Orla".into()]);
        session.set("foes", "x");
        let view = VarView {
            session: &session,
            profile: &profile,
        };
        // The variable reads as a variable does, the brackets after it
        // left as typed.
        assert_eq!(view.interpolate("$foes $foes[1]"), "x x[1]");
        assert_eq!(view.lists().count(), 0);
        // And the reverse.
        let mut session = VariableStore::new();
        let mut profile = VariableStore::new();
        profile.set("foes", "x");
        session.set_list("foes", vec!["Orla".into()]);
        let view = VarView {
            session: &session,
            profile: &profile,
        };
        assert_eq!(
            view.interpolate("$foes $foes[1] 日本$foes[#]"),
            "$foes Orla 日本1"
        );
        assert_eq!(view.iter().count(), 0);
    }
}

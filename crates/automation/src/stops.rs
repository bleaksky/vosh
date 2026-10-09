//! The Lua stops the alias and trigger stores share.

use std::collections::{HashMap, HashSet};

use crate::groups::clean_group;

/// The text the stops know an alias or a trigger by, its group and its
/// name, since two groups may each hold one of a name.
pub(crate) fn stop_id(group: Option<&str>, name: &str) -> String {
    format!("{}\u{1f}{name}", group.unwrap_or(""))
}

/// The group and the name a [`stop_id`] stands for.
pub(crate) fn split_stop_id(id: &str) -> (Option<&str>, &str) {
    let (group, name) = id.split_once('\u{1f}').unwrap_or(("", id));
    (clean_group(Some(group)), name)
}

/// Whose Lua stops a call reads or changes. The app passes the id of the
/// session the call runs in, so an alias or a trigger Vosh stopped in one
/// session stays on in every other session that plays the profile. The
/// crate only compares keys.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct StopKey(pub u32);

/// The aliases or triggers whose Lua Vosh stopped, by the key each stop
/// came under. A stopped item stays off under its key until you save or
/// remove it, which turns it back on under every key. Vosh never saves
/// stops, so a restart turns them all back on.
#[derive(Debug, Clone, Default)]
pub(crate) struct Stops {
    by_key: HashMap<StopKey, HashSet<String>>,
}

impl Stops {
    /// Stop `name` under `key`. True when it was not stopped there yet.
    pub(crate) fn stop(&mut self, name: &str, key: StopKey) -> bool {
        self.by_key.entry(key).or_default().insert(name.to_string())
    }

    /// The names stopped under `key`, so a pass over every item looks the
    /// key up once.
    pub(crate) fn under(&self, key: StopKey) -> Option<&HashSet<String>> {
        self.by_key.get(&key)
    }

    pub(crate) fn contains(&self, name: &str, key: StopKey) -> bool {
        self.under(key).is_some_and(|names| names.contains(name))
    }

    /// Turn `name` back on under every key.
    pub(crate) fn clear(&mut self, name: &str) {
        for names in self.by_key.values_mut() {
            names.remove(name);
        }
    }

    pub(crate) fn forget(&mut self, key: StopKey) {
        self.by_key.remove(&key);
    }

    /// The stops whose item `unchanged` says a whole list save left as it
    /// was, each under the key it came under.
    pub(crate) fn kept(&self, unchanged: impl Fn(&str) -> bool) -> Self {
        let by_key = self
            .by_key
            .iter()
            .map(|(key, names)| {
                let names = names.iter().filter(|n| unchanged(n)).cloned().collect();
                (*key, names)
            })
            .collect();
        Self { by_key }
    }
}

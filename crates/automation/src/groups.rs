//! The group switch the alias and trigger stores share.

use std::collections::BTreeSet;

/// The groups you turned off. It keeps the off names rather than the on
/// ones, so a group you just named starts on. An empty name means no
/// group, so it is always on and never stored.
#[derive(Debug, Clone, Default)]
pub(crate) struct GroupSwitch {
    off: BTreeSet<String>,
}

impl GroupSwitch {
    pub(crate) fn is_enabled(&self, group: &str) -> bool {
        group.is_empty() || !self.off.contains(group)
    }

    /// Whether the switch lets an item tagged with `group` fire. An
    /// untagged item always passes. The caller checks the item's own
    /// enabled flag.
    pub(crate) fn allows(&self, group: Option<&str>) -> bool {
        group.map_or(true, |g| self.is_enabled(g))
    }

    /// Turn `group` on or off. Returns whether it turned, false for a
    /// group already in that state and for no group.
    pub(crate) fn set_enabled(&mut self, group: &str, enabled: bool) -> bool {
        if group.is_empty() {
            return false;
        }
        if enabled {
            self.off.remove(group)
        } else {
            self.off.insert(group.to_string())
        }
    }

    /// Each group the items name, sorted and once each, paired with
    /// whether it is on. Settings lists these as one row per group.
    pub(crate) fn list<'a>(&self, names: impl IntoIterator<Item = &'a str>) -> Vec<(String, bool)> {
        let names: BTreeSet<&str> = names.into_iter().filter(|n| !n.is_empty()).collect();
        names
            .into_iter()
            .map(|n| (n.to_string(), self.is_enabled(n)))
            .collect()
    }

    /// The off groups in sorted order, which is the order the profile
    /// saves them in.
    pub(crate) fn disabled(&self) -> Vec<String> {
        self.off.iter().cloned().collect()
    }

    pub(crate) fn set_disabled<I, S>(&mut self, groups: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.off = groups
            .into_iter()
            .map(Into::into)
            .filter(|s| !s.is_empty())
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::GroupSwitch;

    #[test]
    fn a_switch_says_whether_the_group_turned() {
        let mut groups = GroupSwitch::default();
        assert!(groups.set_enabled("combat", false));
        assert!(!groups.set_enabled("combat", false));
        assert!(!groups.is_enabled("combat"));
        assert!(groups.set_enabled("combat", true));
        assert!(!groups.set_enabled("combat", true));
        // No group is always on and never turns.
        assert!(!groups.set_enabled("", false));
        assert!(groups.allows(None));
        let leftover = &groups.disabled();
        assert!(leftover.is_empty(), "{leftover:?}");
    }
}

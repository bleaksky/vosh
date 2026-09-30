//! Tell every window when the trigger or alias list changes, so an open
//! Settings page follows an edit made anywhere else: `#trigger`,
//! `#alias`, `#untrigger`, `#unalias`, `#endrec`, `#import-tintin`,
//! `#profile load`, a Lua `mud.alias` or `mud.unalias`, an import, or a
//! preset. Each store keeps a revision that moves with its list. A step
//! reads [`ListRevisions`] under the profile lock before it runs and
//! again after, and [`broadcast_list_changes`] sends one event for each
//! list that moved.
//!
//! The profile's `[prompt]` table rides along the same way, so a
//! `#prompt` or `#unprompt` line tells Settings to read the prompt
//! switch and design again. Settings saves its whole config, and a copy
//! it read before would otherwise put the old ones back.

use tauri::AppHandle;

use crate::profile::Profile;

/// Sent to every window when the trigger list changed. The payload is an
/// empty string.
pub(crate) const TRIGGERS_CHANGED: &str = "vosh://triggers-changed";
/// Sent to every window when the alias list changed. The payload is an
/// empty string.
pub(crate) const ALIASES_CHANGED: &str = "vosh://aliases-changed";
/// Sent to every window when the active profile's `[prompt]` table
/// changed. The payload is an empty string.
pub(crate) const PROMPT_CONFIG_CHANGED: &str = "vosh://prompt-config-changed";

/// The trigger and alias list revisions, and the prompt table's, at one
/// moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ListRevisions {
    triggers: u64,
    aliases: u64,
    prompt: u64,
}

impl ListRevisions {
    pub(crate) fn of(profile: &Profile) -> Self {
        Self {
            triggers: profile.triggers.revision(),
            aliases: profile.aliases.revision(),
            prompt: profile.prompt.revision(),
        }
    }
}

/// Which lists a step changed, and whether it changed the prompt table.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ListChanges {
    pub(crate) triggers: bool,
    pub(crate) aliases: bool,
    pub(crate) prompt: bool,
}

impl ListChanges {
    pub(crate) fn between(before: ListRevisions, after: ListRevisions) -> Self {
        Self {
            triggers: before.triggers != after.triggers,
            aliases: before.aliases != after.aliases,
            prompt: before.prompt != after.prompt,
        }
    }

    /// The lists `profile` changed since `before` was read.
    pub(crate) fn since(before: ListRevisions, profile: &Profile) -> Self {
        Self::between(before, ListRevisions::of(profile))
    }

    /// The trigger list alone, for a step that only writes triggers.
    pub(crate) const TRIGGERS: Self = Self {
        triggers: true,
        aliases: false,
        prompt: false,
    };

    /// The alias list alone.
    pub(crate) const ALIASES: Self = Self {
        triggers: false,
        aliases: true,
        prompt: false,
    };

    /// The events these changes send, triggers first.
    pub(crate) fn events(self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.triggers {
            out.push(TRIGGERS_CHANGED);
        }
        if self.aliases {
            out.push(ALIASES_CHANGED);
        }
        if self.prompt {
            out.push(PROMPT_CONFIG_CHANGED);
        }
        out
    }
}

/// Send one event to every window for each list that changed. Call it
/// after the profile lock is released.
pub(crate) fn broadcast_list_changes<R: tauri::Runtime>(app: &AppHandle<R>, changes: ListChanges) {
    for event in changes.events() {
        crate::commands::broadcast(app, event, &"");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input;

    fn changes(profile: &mut Profile, line: &str) -> ListChanges {
        let before = ListRevisions::of(profile);
        let _ = input::process(profile, line);
        ListChanges::since(before, profile)
    }

    #[test]
    fn slash_commands_report_the_list_they_change() {
        let mut p = Profile::default();
        assert_eq!(
            changes(&mut p, "#alias greet wave").events(),
            [ALIASES_CHANGED]
        );
        assert_eq!(
            changes(&mut p, "#trigger flee {^You flee} send look").events(),
            [TRIGGERS_CHANGED]
        );
        assert_eq!(
            changes(&mut p, "#untrigger flee").events(),
            [TRIGGERS_CHANGED]
        );
        assert_eq!(
            changes(&mut p, "#unalias greet").events(),
            [ALIASES_CHANGED]
        );
    }

    #[test]
    fn lua_alias_edits_report_the_alias_list() {
        let mut p = Profile::default();
        assert_eq!(
            changes(&mut p, "#lua mud.alias('k', 'kick')").events(),
            [ALIASES_CHANGED]
        );
        assert_eq!(
            changes(&mut p, "#lua mud.unalias('k')").events(),
            [ALIASES_CHANGED]
        );
    }

    #[test]
    fn prompt_commands_report_the_prompt_table() {
        let mut p = Profile::default();
        assert_eq!(
            changes(&mut p, r"#prompt {^<(?<hp>\d+)hp> $}").events(),
            [PROMPT_CONFIG_CHANGED]
        );
        assert_eq!(
            changes(&mut p, "#unprompt").events(),
            [PROMPT_CONFIG_CHANGED]
        );
        // With nothing to stop, the table stays as it is.
        assert!(changes(&mut p, "#unprompt").events().is_empty());
    }

    #[test]
    fn steps_that_leave_the_lists_alone_report_nothing() {
        let mut p = Profile::default();
        let _ = input::process(&mut p, "#alias greet wave");
        assert!(changes(&mut p, "look").events().is_empty());
        assert!(changes(&mut p, "greet").events().is_empty());
        assert!(changes(&mut p, "#unalias missing").events().is_empty());
        assert!(changes(&mut p, "#aliases").events().is_empty());
    }

    #[test]
    fn each_list_sends_its_own_event() {
        assert_eq!(ListChanges::TRIGGERS.events(), [TRIGGERS_CHANGED]);
        assert_eq!(ListChanges::ALIASES.events(), [ALIASES_CHANGED]);
        let all = ListChanges {
            triggers: true,
            aliases: true,
            prompt: true,
        };
        assert_eq!(
            all.events(),
            [TRIGGERS_CHANGED, ALIASES_CHANGED, PROMPT_CONFIG_CHANGED]
        );
        assert!(ListChanges::default().events().is_empty());
    }

    #[test]
    fn a_lua_apply_reports_the_lists_it_changed() {
        let mut p = Profile::default();
        let outcome = vosh_script::ScriptOutcome {
            actions: vec![vosh_script::Action::SetAlias {
                name: "k".into(),
                expansion: "kick".into(),
            }],
        };
        let apply = crate::script_state::apply_actions(&mut p, outcome);
        assert_eq!(apply.lists, ListChanges::ALIASES);
    }
}

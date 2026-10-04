//! Side-effect intents produced by Lua callbacks. The script engine queues
//! these into the per-Lua-state app data; the session loop drains the queue
//! after each callback returns and applies them under the profile lock.

use std::time::Duration;

use vosh_automation::vars::Scope;

use crate::owner::Owner;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Bytes to send to the server, with CRLF appended by the session.
    Send(String),
    /// Run text through the input pipeline (vars, aliases, slash commands).
    Input(String),
    /// Echo a line locally to the terminal pane.
    Echo(String),
    /// Insert or replace an alias you keep, which Vosh saves.
    SetAlias {
        name: String,
        expansion: String,
    },
    RemoveAlias(String),
    /// Make an alias for the plugin `plugin`, which lasts for the
    /// session and is never saved.
    SetPluginAlias {
        plugin: String,
        name: String,
        expansion: String,
    },
    /// Remove the alias `name` when the plugin `plugin` made it.
    RemovePluginAlias {
        plugin: String,
        name: String,
    },
    /// Remove every alias the plugin of this name made. Vosh adds it
    /// itself when the plugin turns off, stops or loads again.
    DropPluginAliases(String),
    /// Insert or replace a variable.
    SetVar {
        scope: Scope,
        name: String,
        value: String,
    },
    RemoveVar(String),
    /// Insert or replace a session-scoped "prompt var" — a key/value pair
    /// the vitals template resolver reads with priority over GMCP. Used
    /// by `#prompt`-style triggers to feed hp/mn/mv etc. directly from
    /// parsed prompt text.
    SetPromptVar {
        name: String,
        value: String,
    },
    /// Remove a prompt var by name.
    RemovePromptVar(String),
    /// Toggle a group across the trigger / alias / macro stores. Same
    /// as the `#group <name> on|off` slash command. Mirrors the
    /// unified scope so a single Lua call flips every store sharing
    /// that group name.
    SetGroupEnabled {
        name: String,
        enabled: bool,
    },
    /// Insert or replace a regex trigger that fires a Lua callback by id.
    /// It replaces the trigger of its name that `owner` shares names
    /// with, so two plugins can each have a trigger of the same name.
    SetLuaTrigger {
        owner: Owner,
        name: String,
        pattern: String,
        callback_id: i64,
    },
    /// Remove the Lua trigger `name` that `owner` shares names with.
    RemoveLuaTrigger {
        owner: Owner,
        name: String,
    },
    /// Subscribe a Lua callback to a GMCP package.
    SubscribeGmcp {
        package: String,
        callback_id: i64,
    },
    /// Schedule a one-shot Lua callback after a duration.
    Timer {
        delay: Duration,
        callback_id: i64,
        timer_id: u32,
    },
    /// Cancel a previously scheduled timer.
    CancelTimer(u32),
    /// A line from `print` or `mud.log`, for the terminal under the
    /// `[lua]` tag.
    Log(String),
    /// A Lua error, with its file and line where Lua knows them, or a
    /// line about a stop or the action cap. The terminal shows it under
    /// the `[lua]` tag in red. Vosh adds these itself, so no call's
    /// action cap counts them.
    Error(String),
}

impl Action {
    /// How many bytes of text the action holds, which the text a call
    /// may queue counts.
    pub(crate) fn text_len(&self) -> usize {
        match self {
            Action::Send(text)
            | Action::Input(text)
            | Action::Echo(text)
            | Action::RemoveAlias(text)
            | Action::DropPluginAliases(text)
            | Action::RemoveVar(text)
            | Action::RemovePromptVar(text)
            | Action::Log(text)
            | Action::Error(text) => text.len(),
            Action::SetAlias { name, expansion } => name.len() + expansion.len(),
            Action::SetPluginAlias {
                plugin,
                name,
                expansion,
            } => plugin.len() + name.len() + expansion.len(),
            Action::RemovePluginAlias { plugin, name } => plugin.len() + name.len(),
            Action::SetVar { name, value, .. } | Action::SetPromptVar { name, value } => {
                name.len() + value.len()
            }
            Action::SetGroupEnabled { name, .. } | Action::RemoveLuaTrigger { name, .. } => {
                name.len()
            }
            Action::SetLuaTrigger { name, pattern, .. } => name.len() + pattern.len(),
            Action::SubscribeGmcp { package, .. } => package.len(),
            Action::Timer { .. } | Action::CancelTimer(_) => 0,
        }
    }

    /// True for an action that registers something for its owner or
    /// takes a registration away, which a failed load takes back so the
    /// owner keeps what it had.
    pub(crate) fn registers(&self) -> bool {
        matches!(
            self,
            Action::SetLuaTrigger { .. }
                | Action::RemoveLuaTrigger { .. }
                | Action::SubscribeGmcp { .. }
                | Action::Timer { .. }
                | Action::SetPluginAlias { .. }
                | Action::RemovePluginAlias { .. }
        )
    }
}

//! Side-effect intents produced by Lua callbacks. The script engine queues
//! these into the per-Lua-state app data; the session loop drains the queue
//! after each callback returns and applies them under the profile lock.

use std::time::Duration;

use vosh_automation::alert::AlertParts;
use vosh_automation::vars::Scope;

use crate::owner::Owner;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Bytes to send to the server, with CRLF appended by the session.
    Send(String),
    /// Run text through the input pipeline (vars, aliases, slash
    /// commands), for the Lua of `owner`, which decides the slash
    /// commands the line may run.
    Input {
        owner: Owner,
        line: String,
    },
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
    /// End what the plugin of this name left with the session, its
    /// aliases and its alerts. Vosh adds it itself when the plugin turns
    /// off, stops or loads again.
    DropPlugin(String),
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
    /// Toggle a group across the trigger, alias, macro and timer stores.
    /// Same as the `#group <name> on|off` slash command. Mirrors the
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
    /// `[lua]` tag, from the Lua of `owner`, whose call printed it.
    Log {
        owner: Owner,
        text: String,
    },
    /// A Lua error, with its file and line where Lua knows them, or a
    /// line about a stop or the action cap, about the Lua of `owner`.
    /// The terminal shows it under the `[lua]` tag in red. `at` is the
    /// place Lua names, which the Scripts page marks in its editor. Vosh
    /// adds these itself, so no call's action cap counts them.
    Error {
        owner: Owner,
        text: String,
        at: Option<Place>,
    },
    /// A sentence Vosh writes about the Lua of `owner`, such as how long
    /// a stopped plugin stays off. The terminal shows it under the
    /// `[lua]` tag in the default color. Vosh adds these itself, so no
    /// call's action cap counts them.
    Note {
        owner: Owner,
        text: String,
    },
    /// Ring an alert, from `mud.alert`, for the Lua of `owner`, which
    /// ends the alerts of a plugin as the plugin turns off.
    Alert {
        owner: Owner,
        title: String,
        /// What a banner shows under the title with Title and words.
        text: Option<String>,
        parts: AlertParts,
    },
    /// A pane the plugin `plugin` draws, from `mud.pane`. Add a pane
    /// lists it by `title`, and a layout keeps it by `plugin` and `id`.
    Pane {
        plugin: String,
        id: String,
        title: String,
    },
    /// Replace what the pane `id` of the plugin `plugin` shows.
    PaneSet {
        plugin: String,
        id: String,
        blocks: Vec<PaneBlock>,
    },
    /// The words beside the name of the pane `id` of `plugin`.
    PaneMeta {
        plugin: String,
        id: String,
        text: String,
    },
}

/// One block of a pane a plugin draws, which the page shows as text.
#[derive(Debug, Clone, PartialEq)]
pub enum PaneBlock {
    /// A label and a value on one row.
    Row { label: String, value: String },
    /// A row with a meter, filled `value` of `max`.
    Gauge { label: String, value: f64, max: f64 },
    /// A line in the terminal font, with Vosh color codes.
    Line(String),
    /// A thin line between blocks.
    Rule,
}

impl PaneBlock {
    /// How many bytes of text the block holds.
    fn text_len(&self) -> usize {
        match self {
            PaneBlock::Row { label, value } => label.len() + value.len(),
            PaneBlock::Gauge { label, .. } | PaneBlock::Line(label) => label.len(),
            PaneBlock::Rule => 0,
        }
    }
}

/// Where in your Lua an error or a stop happened: the chunk as Lua
/// names it in a message, like `vitals_alert/main.lua` or `#lua`, and
/// the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub source: String,
    pub line: u32,
}

impl Action {
    /// How many bytes of text the action holds, which the text a call
    /// may queue counts.
    pub(crate) fn text_len(&self) -> usize {
        match self {
            Action::Send(text)
            | Action::Input { line: text, .. }
            | Action::Echo(text)
            | Action::RemoveAlias(text)
            | Action::DropPlugin(text)
            | Action::RemoveVar(text)
            | Action::RemovePromptVar(text)
            | Action::Log { text, .. }
            | Action::Error { text, .. }
            | Action::Note { text, .. } => text.len(),
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
            Action::Alert {
                title, text, parts, ..
            } => {
                title.len()
                    + text.as_ref().map_or(0, String::len)
                    + parts.sound.as_ref().map_or(0, String::len)
            }
            Action::Pane { plugin, id, title } => plugin.len() + id.len() + title.len(),
            Action::PaneSet { plugin, id, blocks } => {
                plugin.len() + id.len() + blocks.iter().map(PaneBlock::text_len).sum::<usize>()
            }
            Action::PaneMeta { plugin, id, text } => plugin.len() + id.len() + text.len(),
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
                | Action::Pane { .. }
        )
    }
}

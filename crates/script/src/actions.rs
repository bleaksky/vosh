//! Side-effect intents produced by Lua callbacks. The script engine queues
//! these into the per-Lua-state app data; the session loop drains the queue
//! after each callback returns and applies them under the profile lock.

use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VarScope {
    Profile,
    Session,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Bytes to send to the server, with CRLF appended by the session.
    Send(String),
    /// Run text through the input pipeline (vars, aliases, slash commands).
    Input(String),
    /// Echo a line locally to the terminal pane.
    Echo(String),
    /// Insert or replace an alias.
    SetAlias {
        name: String,
        expansion: String,
    },
    RemoveAlias(String),
    /// Insert or replace a variable.
    SetVar {
        scope: VarScope,
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
    SetLuaTrigger {
        name: String,
        pattern: String,
        callback_id: i64,
        priority: i32,
    },
    RemoveLuaTrigger(String),
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
    /// Free a registry slot when its owner cancels itself.
    DropCallback(i64),
    /// Log a debug line (currently echoes to the terminal in a faint color).
    Log(String),
}

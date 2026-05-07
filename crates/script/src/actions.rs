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

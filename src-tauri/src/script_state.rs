//! Glue between [`mudclient_script::ScriptEngine`] and the rest of the app.
//!
//! Lua callbacks return [`mudclient_script::Action`] values; this module
//! applies them to the profile (vars, aliases, triggers), forwards
//! send/echo to the session, and tracks pending one-shot timers so the
//! session loop can fire them at the right time.

use std::sync::Arc;

use mudclient_alias::Alias;
use mudclient_script::{Action, ScriptEngine, ScriptOutcome, VarScope};
use mudclient_trigger::{Trigger, TriggerAction};
use mudclient_vars::{Scope, VariableStore};
use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::profile::Profile;

/// One pending one-shot Lua timer.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PendingTimer {
    pub deadline: Instant,
    pub callback_id: i64,
    pub timer_id: u32,
}

/// Shared list of pending Lua timers. Polled from the `io_loop` on each
/// 250 ms tick; expired entries fire their callbacks.
pub(crate) type SharedTimers = Arc<Mutex<Vec<PendingTimer>>>;

/// Refresh the script engine's view of session vars so `mud.var(name)`
/// returns up-to-date values.
pub(crate) fn snapshot_vars(script: &ScriptEngine, vars: &VariableStore) {
    let snapshot: std::collections::HashMap<String, String> = vars
        .iter()
        .map(|(k, v, _)| (k.to_string(), v.to_string()))
        .collect();
    script.set_var_snapshot(snapshot);
}

/// Result of applying a [`ScriptOutcome`]: bytes to send, lines to echo,
/// timers to schedule. The session loop owns the actual stream and event
/// handle so it does the real IO.
#[derive(Debug, Default)]
pub(crate) struct ApplyResult {
    pub send_bytes: Vec<u8>,
    pub echoes: Vec<String>,
    /// Lines of input to feed back through the input pipeline.
    pub inputs: Vec<String>,
    pub new_timers: Vec<PendingTimer>,
    pub cancel_timers: Vec<u32>,
}

/// Apply Lua-produced actions to the profile. The caller is responsible
/// for performing the IO listed in the returned [`ApplyResult`].
pub(crate) fn apply_actions(profile: &mut Profile, outcome: ScriptOutcome) -> ApplyResult {
    let mut result = ApplyResult::default();
    for action in outcome.actions {
        match action {
            Action::Send(line) => {
                result.send_bytes.extend_from_slice(line.as_bytes());
                result.send_bytes.extend_from_slice(b"\r\n");
            }
            Action::Input(line) => {
                result.inputs.push(line);
            }
            Action::Echo(line) => {
                result.echoes.push(line);
            }
            Action::Log(line) => {
                result.echoes.push(format!("[lua] {line}"));
            }
            Action::SetAlias { name, expansion } => {
                profile.aliases.set(Alias::new(name, expansion));
            }
            Action::RemoveAlias(name) => {
                profile.aliases.remove(&name);
            }
            Action::SetVar { scope, name, value } => {
                profile.vars.set(scope_to_internal(scope), name, value);
            }
            Action::RemoveVar(name) => {
                profile.vars.remove(&name);
            }
            Action::SetLuaTrigger { .. }
            | Action::RemoveLuaTrigger(_)
            | Action::SubscribeGmcp { .. }
            | Action::DropCallback(_) => {
                // The script engine consumes these in its own drain loop;
                // they should not reach here. Ignore defensively.
            }
            Action::Timer {
                delay,
                callback_id,
                timer_id,
            } => {
                result.new_timers.push(PendingTimer {
                    deadline: Instant::now() + delay,
                    callback_id,
                    timer_id,
                });
            }
            Action::CancelTimer(id) => {
                result.cancel_timers.push(id);
            }
        }
    }
    result
}

/// Push a structured trigger from a Lua-produced action into the profile.
/// Used by the regex trigger engine when the action carries plain data
/// (highlight, gag, etc.); not needed for the Lua-callback trigger path
/// since `ScriptEngine` owns those internally.
#[allow(dead_code)]
pub(crate) fn push_trigger(profile: &mut Profile, trigger: Trigger) -> Option<String> {
    match profile.triggers.set(trigger) {
        Ok(()) => None,
        Err(e) => Some(e.to_string()),
    }
}

#[allow(dead_code)]
pub(crate) fn highlight_trigger_marker() -> TriggerAction {
    TriggerAction::Gag
}

fn scope_to_internal(scope: VarScope) -> Scope {
    match scope {
        VarScope::Profile => Scope::Profile,
        VarScope::Session => Scope::Session,
    }
}

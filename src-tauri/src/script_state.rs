//! Glue between [`vosh_script::ScriptEngine`] and the rest of the app.
//!
//! Lua callbacks return [`vosh_script::Action`] values; this module
//! applies them to the profile (vars, aliases, triggers), forwards
//! send/echo to the session, and tracks pending one-shot timers so the
//! session loop can fire them at the right time.

use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::time::Instant;
use vosh_alias::Alias;
use vosh_script::{Action, ScriptEngine, ScriptOutcome, VarScope};
use vosh_vars::{Scope, VariableStore};

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

/// Evaluate a body with `captures` bound as a local table. Used by
/// `TriggerAction::Script` and script-bodied aliases — the user's
/// Lua body sees `captures[1]`, `captures[2]`, ... as if the body
/// were the inside of a callback. `chunk_name` is shown in error
/// traces. Returns the script's outcome (a list of `Action`s) so the
/// caller can fold it into the surrounding apply.
pub(crate) fn eval_with_captures(
    engine: &mut ScriptEngine,
    body: &str,
    captures: &[String],
    chunk_name: &str,
) -> Result<ScriptOutcome, vosh_script::ScriptError> {
    let mut buf = String::with_capacity(body.len() + 64 + captures.len() * 8);
    buf.push_str("do local captures = {");
    for (i, c) in captures.iter().enumerate() {
        if i > 0 {
            buf.push_str(", ");
        }
        push_lua_string(&mut buf, c);
    }
    buf.push_str("}\n");
    buf.push_str(body);
    buf.push_str("\nend");
    engine.eval(&buf, chunk_name)
}

fn push_lua_string(buf: &mut String, s: &str) {
    buf.push('"');
    for c in s.chars() {
        match c {
            '\\' => buf.push_str("\\\\"),
            '"' => buf.push_str("\\\""),
            '\n' => buf.push_str("\\n"),
            '\r' => buf.push_str("\\r"),
            '\t' => buf.push_str("\\t"),
            '\0' => buf.push_str("\\0"),
            _ => buf.push(c),
        }
    }
    buf.push('"');
}

/// Refresh the script engine's view of session vars so `mud.var(name)`
/// returns up-to-date values. Skipped entirely when the engine has
/// no registered triggers / GMCP subs / loaded scripts — without
/// any consumer of `mud.var(name)`, cloning the var map per line
/// is wasted work. This is the common case for users who don't
/// write Lua, and matches what the engine's match/dispatch paths
/// would do anyway (no-op when nothing is registered).
pub(crate) fn snapshot_vars(script: &ScriptEngine, vars: &VariableStore) {
    if !script.has_handlers() {
        return;
    }
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
    /// True when any prompt var changed during apply — the session
    /// emits a `session://prompt-vars` snapshot to the frontend
    /// once per apply rather than once per individual set.
    pub prompt_vars_changed: bool,
    /// True when the apply touched DURABLE profile state (aliases,
    /// vars, group toggles) that should reach disk. Drives the
    /// debounced profile persist; ephemeral runtime state (prompt
    /// vars, timers, echoes) does not set it.
    pub durable_changed: bool,
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
                result.durable_changed = true;
            }
            Action::RemoveAlias(name) => {
                profile.aliases.remove(&name);
                result.durable_changed = true;
            }
            Action::SetVar { scope, name, value } => {
                let internal = scope_to_internal(scope);
                // Only profile-scoped vars are persisted; session vars
                // marking durable would reset the persist debounce on
                // every combat line for busy Lua triggers.
                if matches!(internal, Scope::Profile) {
                    result.durable_changed = true;
                }
                profile.vars.set(internal, name, value);
            }
            Action::RemoveVar(name) => {
                profile.vars.remove(&name);
                result.durable_changed = true;
            }
            Action::SetPromptVar { name, value } => {
                profile.prompt_vars.insert(name, value);
                // Always flag as changed. The previous "only fire on
                // value change" semantics suppressed every emit after
                // the first one when the player was at full vitals
                // and the prompt repeated unchanged — which means the
                // frontend never re-rendered the custom prompt template
                // on subsequent prompts. The change-gate optimization
                // saved a few IPC events per second; the trade-off is
                // not worth losing the prompt-as-ready-indicator UX.
                result.prompt_vars_changed = true;
            }
            Action::RemovePromptVar(name) => {
                if profile.prompt_vars.remove(&name).is_some() {
                    result.prompt_vars_changed = true;
                }
            }
            Action::SetGroupEnabled { name, enabled } => {
                toggle_group(profile, &name, enabled);
                result.durable_changed = true;
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

fn scope_to_internal(scope: VarScope) -> Scope {
    match scope {
        VarScope::Profile => Scope::Profile,
        VarScope::Session => Scope::Session,
    }
}

/// Outcome of `toggle_group`. Reports which stores actually carried
/// at least one entry tagged with the requested group, so callers
/// (the `#group` slash command, the `mud.set_group_enabled` Lua API)
/// can echo something useful rather than silently no-op.
#[derive(Debug, Default)]
pub(crate) struct GroupToggleReport {
    pub aliases: bool,
    pub triggers: bool,
    pub macros: bool,
}

impl GroupToggleReport {
    pub(crate) fn touched(&self) -> bool {
        self.aliases || self.triggers || self.macros
    }
}

/// Flip a group's enabled state across triggers, aliases, and macros
/// in one shot. The group lives independently in each store, so this
/// only touches the stores that actually have a matching entry —
/// asking to disable a group that exists only in triggers won't
/// stamp an empty group name into the macro disabled set.
pub(crate) fn toggle_group(profile: &mut Profile, name: &str, enabled: bool) -> GroupToggleReport {
    let mut report = GroupToggleReport::default();
    if profile.aliases.groups().iter().any(|(g, _)| g == name) {
        profile.aliases.set_group_enabled(name, enabled);
        report.aliases = true;
    }
    if profile.triggers.groups().iter().any(|(g, _)| g == name) {
        profile.triggers.set_group_enabled(name, enabled);
        report.triggers = true;
    }
    let has_macro_group = profile
        .macros
        .iter()
        .any(|m| m.group.as_deref() == Some(name));
    if has_macro_group {
        if enabled {
            profile.disabled_macro_groups.remove(name);
        } else {
            profile.disabled_macro_groups.insert(name.to_string());
        }
        report.macros = true;
    }
    report
}

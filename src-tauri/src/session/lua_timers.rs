//! The Lua timers of a session. `mud.timer` puts a timer on the shared
//! list, and each poll of the session loop fires the ones whose deadline
//! passed as one round, then applies what their callbacks ask for. A
//! timer whose plugin or loose script used its time for the round goes
//! back at the front of the list, so the next poll fires it first.

use std::sync::Arc;

use tauri::AppHandle;
use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::profile::live::Profile;
use crate::script::{self, ApplyResult, PendingTimer, SharedTimers};

use super::connection::{Connection, SharedConnection};
use super::effects::{apply_script_result, OutputSink, ScriptIo};
use super::socket::Stream;
use super::walk::Walker;

/// Fire the Lua timers whose deadline passed, then apply what their
/// callbacks ask for.
pub(super) async fn fire_due<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stream: &mut Stream,
    walker: &mut Walker,
    profile: &Arc<Mutex<Profile>>,
    connection: &SharedConnection,
    lua_timers: &SharedTimers,
) -> std::io::Result<()> {
    let now = Instant::now();
    let due: Vec<PendingTimer> = {
        let mut guard = lua_timers.lock().await;
        let (ready, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut *guard)
            .into_iter()
            .partition(|t| t.deadline <= now);
        *guard = keep;
        ready
    };
    if due.is_empty() {
        return Ok(());
    }
    let (apply, held) = {
        let mut p = profile.lock().await;
        let mut c = connection.lock();
        fire_round(&mut p, &mut c, due)
    };
    // Before the apply, so a cancel among its actions finds them.
    if !held.is_empty() {
        hold(&mut *lua_timers.lock().await, held);
    }
    let mut sink = OutputSink::Direct;
    let mut io = ScriptIo::Session(stream, &mut sink, walker);
    apply_script_result(app, &mut io, profile, lua_timers, apply).await
}

/// Put `held`, the timers a round had no time for, back at the front of
/// `list`, in their order. They were due before anything still on the
/// list, so the next poll fires them first, ahead of a later timer that
/// came due since.
pub(super) fn hold(list: &mut Vec<PendingTimer>, held: Vec<PendingTimer>) {
    list.splice(0..0, held);
}

/// Fire `due` as one round under the profile lock and the connection's,
/// which the caller holds. Returns what the callbacks ask of the profile,
/// and the timers whose owner used its time for the round, which never
/// ran and wait for the next.
pub(super) fn fire_round(
    p: &mut Profile,
    c: &mut Connection,
    due: Vec<PendingTimer>,
) -> (ApplyResult, Vec<PendingTimer>) {
    script::snapshot_vars(&p.script, &p.vars);
    let ids: Vec<i64> = due.iter().map(|t| t.callback_id).collect();
    let fired = p.script.fire_timers(&ids);
    let held = due
        .into_iter()
        .filter(|t| fired.held.contains(&t.callback_id))
        .collect();
    (script::apply_actions(p, c, fired.outcome), held)
}

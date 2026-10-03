//! The Lua timers of a session. `mud.timer` puts a timer on the shared
//! list, and each poll of the session loop fires the ones whose deadline
//! passed, then applies what their callbacks ask for.

use std::sync::Arc;

use tauri::AppHandle;
use tokio::sync::Mutex;
use tokio::time::Instant;
use tracing::warn;

use crate::profile::Profile;
use crate::script::{self, PendingTimer, SharedTimers};

use super::connection::Stream;
use super::effects::{apply_script_result, OutputSink, ScriptIo};

pub(super) async fn fire_due_script_timers<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stream: &mut Stream,
    profile: &Arc<Mutex<Profile>>,
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
    let apply = {
        let mut p = profile.lock().await;
        script::snapshot_vars(&p.script, &p.vars);
        let mut outcome = vosh_script::ScriptOutcome::default();
        for t in due {
            match p.script.fire_timer(t.callback_id) {
                Ok(o) => outcome.actions.extend(o.actions),
                Err(err) => warn!(error = %err, "lua timer fire failed"),
            }
        }
        script::apply_actions(&mut p, outcome)
    };
    let mut sink = OutputSink::Direct;
    let mut io = ScriptIo::Session(stream, &mut sink);
    apply_script_result(app, &mut io, profile, lua_timers, apply).await
}

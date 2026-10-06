//! The commands of the Scripts page in Settings: the Output ring of a
//! session and the console. Each takes the session it acts on, or the
//! selected one, since each session runs Lua of its own.

use tauri::{AppHandle, State};

use crate::app::state::SharedState;
use crate::script::output::LuaLine;
use crate::sessions::SessionId;

/// The lines in the Output ring of `session`, oldest first.
#[tauri::command]
pub(crate) async fn lua_output_get(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<Vec<LuaLine>, String> {
    let session = state.session(session)?;
    let lines = session.connection.lock().lua_output.lines();
    Ok(lines)
}

/// Clear the lines of `owner`, a tag like `plugin:vitals_alert`, from
/// the Output ring of `session`, or every line with no owner.
#[tauri::command]
pub(crate) async fn lua_output_clear(
    state: State<'_, SharedState>,
    owner: Option<String>,
    session: Option<SessionId>,
) -> Result<(), String> {
    let session = state.session(session)?;
    session.connection.lock().lua_output.clear(owner.as_deref());
    Ok(())
}

/// Run `code` from the console in `session`: inside the plugin `plugin`,
/// or in the global environment as a `#lua` line runs. What it prints
/// shows in Output and in the terminal, and what it sends goes to the
/// game when the session runs a connection.
#[tauri::command]
pub(crate) async fn lua_run<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    code: String,
    plugin: Option<String>,
    session: Option<SessionId>,
) -> Result<(), String> {
    let session = state.session(session)?;
    let apply = {
        let mut p = session.lock_profile().await;
        let mut c = session.connection.lock();
        crate::script::run_console(&mut p, &mut c, &code, plugin.as_deref()).ran_under(p.open())
    };
    crate::session::effects::deliver_detached(&app, &session, apply).await;
    Ok(())
}

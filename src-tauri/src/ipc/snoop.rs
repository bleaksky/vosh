//! The commands for the players a session snoops (Snoop SN5). The page
//! reads every tab with its text the first time it shows a session. Each
//! acts on the session it names, or on the selected session when it
//! names none.

use tauri::State;

use crate::app::state::SharedState;
use crate::session::snoop::SnoopTabText;
use crate::sessions::SessionId;

/// Every player `session` snoops, or the selected session does, in the
/// order they started, each with its text as the game sent it.
#[tauri::command]
pub(crate) async fn snoop_get(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<Vec<SnoopTabText>, String> {
    let session = state.session(session)?;
    let tabs = session.connection.lock().snoops.all();
    Ok(tabs)
}

//! The commands for your vitals. A window that opens between two
//! `Char.Vitals` packets, Settings among them, reads the last vitals and
//! fight of a session, so it shows your numbers at once. A footer or the
//! status line that draws your vitals text watches it, so the session
//! sends each render on `session://vitals-text`.

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, State};

use crate::app::state::SharedState;
use crate::session::last_packages::{VitalsSample, COMBAT_PACKAGE, VITALS_PACKAGE};
use crate::session::vitals_text;
use crate::sessions::SessionId;

/// The last `Char.Vitals` and `Char.Combat` of a connection, raw as the
/// MUD sent them, each null before the first one and after the
/// connection ends, and the last `Char.Vitals` the game showed with
/// their times, oldest first.
#[derive(Debug, Serialize)]
pub(crate) struct VitalsSnapshot {
    vitals: Option<Value>,
    combat: Option<Value>,
    history: Vec<VitalsSample>,
}

/// The last vitals and fight of the session's connection.
#[tauri::command]
pub(crate) async fn vitals_snapshot_get(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<VitalsSnapshot, String> {
    let last = &state.session(session)?.last_packages;
    Ok(VitalsSnapshot {
        vitals: last.get(VITALS_PACKAGE),
        combat: last.get(COMBAT_PACKAGE),
        history: last.history(),
    })
}

/// A footer or the status line draws your vitals text `cols` terminal
/// cells wide, or stops drawing it with None. While a watch is on, the
/// session sends the text at once and again whenever it moves.
#[tauri::command]
pub(crate) async fn vitals_text_watch<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    session: Option<SessionId>,
    cols: Option<usize>,
) -> Result<(), String> {
    let session = state.session(session)?;
    session.vitals_watch.watch(cols);
    let drawn = {
        let p = session.lock_profile().await;
        let c = session.connection.lock();
        vitals_text::render(&session, &p, &c, tokio::time::Instant::now())
    };
    vitals_text::emit(&app, &session, drawn);
    Ok(())
}

//! The command for your vitals. A window that opens between two
//! `Char.Vitals` packets, Settings among them, reads the last vitals and
//! fight of a session through it, so it shows your numbers at once.

use serde::Serialize;
use serde_json::Value;
use tauri::State;

use crate::app::state::SharedState;
use crate::session::last_packages::{COMBAT_PACKAGE, VITALS_PACKAGE};
use crate::sessions::SessionId;

/// The last `Char.Vitals` and `Char.Combat` of a connection, raw as the
/// MUD sent them, each null before the first one and after the
/// connection ends.
#[derive(Debug, Serialize)]
pub(crate) struct VitalsSnapshot {
    vitals: Option<Value>,
    combat: Option<Value>,
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
    })
}

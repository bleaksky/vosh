//! The commands for the players a session snoops. The page reads every
//! tab with its text the first time it shows a
//! session, then follows `session://snoop` and `session://snoop-output`.
//! Stop asks the game to end a snoop, or every snoop, and Close drops an
//! ended tab, or every ended tab, with its text. Each acts on the session
//! it names, or on the selected session when it names none.

use tauri::{AppHandle, State};

use crate::app::state::SharedState;
use crate::input::{command_echo, NOT_CONNECTED};
use crate::output;
use crate::session::snoop::{self, SnoopSnapshot};
use crate::sessions::SessionId;

/// Every player `session` snoops, or the selected session does, in the
/// order they started, each with its text as the game sent it, and
/// whether they show in the snoop window.
#[tauri::command]
pub(crate) async fn snoop_get(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<SnoopSnapshot, String> {
    let session = state.session(session)?;
    let snapshot = session.connection.lock().snoops.snapshot();
    Ok(snapshot)
}

/// What Stop sends the game: `snoop stop` and the player's name, or
/// `snoop stop` alone for every snoop (`do_snoop` in `act_wiz.c`).
fn stop_line(name: Option<&str>) -> String {
    match name {
        Some(name) => format!("snoop stop {name}"),
        None => "snoop stop".to_string(),
    }
}

/// Stop the snoop of `name`, or every snoop with no name. The line goes
/// to the game the way a quick key's does, with its echo and its row in
/// the session log, and no alias sees it. The tab waits for the game to
/// say the snoop ended, then goes.
#[tauri::command]
pub(crate) async fn snoop_stop<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    session: Option<SessionId>,
    name: Option<String>,
) -> Result<(), String> {
    let session = state.session(session)?;
    let name = name.as_deref().map(str::trim).filter(|n| !n.is_empty());
    let line = stop_line(name);
    let echo = command_echo(&line, &session.lock_profile().await.ui);
    session.connection.lock().snoops.stopping(name);
    snoop::emit_changes(&app, &session);
    output::echo_command(&app, &session, &[echo]);
    let current = session.slot.lock().await;
    let sent = current
        .as_ref()
        .is_some_and(|handle| handle.send(format!("{line}\r\n").into_bytes()));
    if !sent {
        output::emit_output(&app, &session, NOT_CONNECTED.to_vec());
    }
    Ok(())
}

/// Close the ended tab of `name`, or every ended tab with no name, and
/// drop its text. A live tab stays.
#[tauri::command]
pub(crate) async fn snoop_close<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    session: Option<SessionId>,
    name: Option<String>,
) -> Result<(), String> {
    let session = state.session(session)?;
    session.connection.lock().snoops.close(name.as_deref());
    snoop::emit_changes(&app, &session);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::stop_line;

    #[test]
    fn stop_names_the_player_or_stops_every_snoop() {
        assert_eq!(stop_line(Some("Tolliver")), "snoop stop Tolliver");
        assert_eq!(stop_line(None), "snoop stop");
    }
}

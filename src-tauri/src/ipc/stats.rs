//! The commands for the Stats pane. A window that opens after the game
//! sent `Char.Stats` reads the last one a session kept, since the game
//! sends it only at login and after a number moves. The pane's own menu
//! picks how it draws.

use serde_json::Value;
use tauri::{AppHandle, State};

use crate::app::events::STATS_STYLE_CHANGED;
use crate::app::state::SharedState;
use crate::disk::save::{save_then_broadcast, SavePolicy};
use crate::profile::ui::coerce_stats_style;
use crate::session::last_packages::STATS_PACKAGE;
use crate::sessions::SessionId;

/// The last `Char.Stats` of the session's connection, raw as the MUD
/// sent it, or null before the first one and after the connection ends.
#[tauri::command]
pub(crate) async fn stats_snapshot_get(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<Option<Value>, String> {
    Ok(state.session(session)?.last_packages.get(STATS_PACKAGE))
}

/// Save the Stats pane's style, a pick from the pane menu, onto the
/// profile named `profile` or the selected session's, and tell every
/// window. An unknown style saves as the score sheet, and a pick that
/// changes nothing saves and sends nothing.
#[tauri::command]
pub(crate) async fn ui_set_stats_style(
    app: AppHandle,
    state: State<'_, SharedState>,
    style: String,
    profile: Option<String>,
) -> Result<(), String> {
    let style = coerce_stats_style(style);
    let open = {
        let mut p = state.lock_named(profile).await?;
        if p.ui.stats_style == style {
            return Ok(());
        }
        p.ui.stats_style.clone_from(&style);
        p.open().clone()
    };
    save_then_broadcast(
        &app,
        &state,
        &open,
        SavePolicy::Now,
        STATS_STYLE_CHANGED,
        &style,
    )
    .await;
    Ok(())
}

//! The commands for the pane layout. The main window reads the tree of
//! the profile in front and writes it back as you split, resize and close
//! panes. The command palette and Settings > Characters put a profile's
//! panes back to the stock tree. The main window also reads the panes a
//! session's plugins draw.

use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::app::events::{pane_layout_envelope, PaneLayoutEnvelope, PANE_LAYOUT_CHANGED};
use crate::app::state::SharedState;
use crate::disk::save::{save_by, save_then_broadcast, SavePolicy};
use crate::profile::inactive::{
    broadcast_profile_changed, reset_inactive_panes, reset_open_panes, Stored,
};
use crate::profile::panes::PaneLayoutPersist;
use crate::script::panes::LuaPane;
use crate::sessions::SessionId;

/// Read the pane layout of the profile `profile` names, which a session
/// must play, or of the selected session's. A profile that has never
/// saved one gets a tree migrated from its dock layout (or the
/// default), with nothing written to disk until the first edit.
#[tauri::command]
pub(crate) async fn pane_layout_get(
    state: State<'_, SharedState>,
    profile: Option<String>,
) -> Result<PaneLayoutEnvelope, String> {
    let p = state.lock_named(profile).await?;
    Ok(pane_layout_envelope(&state, &p))
}

/// Replace the pane layout of the profile `profile` names, which a
/// session must play, or of the selected session's, and broadcast the
/// sanitized tree as `vosh://pane-layout-changed` to every window while
/// that profile is in front.
/// Splitter drags land here several times a second even after the
/// frontend debounce, so the disk write goes through the debounced
/// `schedule_profile_persist` rather than rotating a backup per drag step.
/// A profile switch or quit flushes it right away.
///
/// `generation` is the one the edited tree was read at. A write made
/// against a profile that has since been swapped out is refused and
/// returns false, and the caller reads the current tree again. An
/// untagged write (a tree that never came from the backend) applies.
#[tauri::command]
pub(crate) async fn pane_layout_set(
    app: AppHandle,
    state: State<'_, SharedState>,
    layout: PaneLayoutPersist,
    generation: Option<u64>,
    profile: Option<String>,
) -> Result<bool, String> {
    let mut layout = layout;
    layout.sanitize();
    let (open, current) = {
        let mut p = state.lock_named(profile).await?;
        let current = state.panes_generation();
        if generation.is_some_and(|g| g != current) {
            return Ok(false);
        }
        p.ui.panes = Some(layout.clone());
        (p.open().clone(), current)
    };
    // A layout tweak after `#profile reset` must not save the blanked
    // profile, so this schedules without clearing the suppression.
    save_then_broadcast(
        &app,
        &state,
        &open,
        SavePolicy::SoonUnlessHeld,
        PANE_LAYOUT_CHANGED,
        &PaneLayoutEnvelope {
            layout,
            generation: Some(current),
        },
    )
    .await;
    Ok(true)
}

/// Put a profile's panes back to the stock map over affects tree,
/// keeping whether its panel shows and how wide it is. The selected
/// session's profile when `profile` is absent.
///
/// A profile a session plays resets in memory and saves at once, unless
/// `#profile reset` or `load` left it diverged from disk. The selected
/// session's profile bumps the pane generation under the profile lock, so
/// a splitter drag still in flight is refused rather than undoing the
/// reset, and broadcasts `vosh://pane-layout-changed`. Any other profile
/// a session plays sends only `vosh://profile-changed`, and one no session
/// plays has its file rewritten and sends the same.
#[tauri::command]
pub(crate) async fn pane_layout_reset(
    app: AppHandle,
    state: State<'_, SharedState>,
    profile: Option<String>,
) -> Result<PaneLayoutEnvelope, String> {
    let shared: SharedState = state.inner().clone();
    let named = match profile.as_deref() {
        Some(name) => match reset_inactive_panes(&shared, name).await? {
            Stored::File(layout) => {
                broadcast_profile_changed(&app, name);
                return Ok(PaneLayoutEnvelope {
                    layout,
                    generation: None,
                });
            }
            Stored::Open(open) => Some(open),
        },
        None => None,
    };
    let selected = shared.selected_session();
    let (open, shown, envelope) = {
        let mut p = match &named {
            Some(open) => open.lock().await,
            None => selected.lock_profile().await,
        };
        let shown = Arc::ptr_eq(p.open(), &selected.profile());
        let envelope = reset_open_panes(&shared, &mut p, shown);
        (p.open().clone(), shown, envelope)
    };
    if shown {
        save_then_broadcast(
            &app,
            &shared,
            &open,
            SavePolicy::NowUnlessHeld,
            PANE_LAYOUT_CHANGED,
            &envelope,
        )
        .await;
    } else {
        save_by(&app, &shared, &open, SavePolicy::NowUnlessHeld).await;
    }
    if let Some(name) = open.name() {
        broadcast_profile_changed(&app, &name);
    }
    Ok(envelope)
}

/// Every pane the plugins of `session` draw, or of the selected session,
/// by plugin and then id. `session://lua-panes` carries what changes
/// after.
#[tauri::command]
pub(crate) async fn lua_panes_get(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<Vec<LuaPane>, String> {
    let session = state.session(session)?;
    let panes = session.connection.lock().lua_panes.all();
    Ok(panes)
}

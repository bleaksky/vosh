//! The commands for the pane layout. The main window reads the active
//! profile's tree and writes it back as you split, resize and close
//! panes. The command palette and Settings > Characters put a profile's
//! panes back to the stock tree.

use tauri::{AppHandle, State};

use crate::app::events::{pane_layout_envelope, PaneLayoutEnvelope, PANE_LAYOUT_CHANGED};
use crate::app::state::{panes_generation, SharedState};
use crate::disk::save::{save_then_broadcast, SavePolicy};
use crate::profile::inactive::{
    active_name, broadcast_profile_changed, reset_inactive_panes, reset_live_panes,
};
use crate::profile::panes::PaneLayoutPersist;

/// Read the active profile's pane layout. A profile that has never
/// saved one gets a tree migrated from its dock layout (or the
/// default), with nothing written to disk until the first edit.
#[tauri::command]
pub(crate) async fn pane_layout_get(
    state: State<'_, SharedState>,
) -> Result<PaneLayoutEnvelope, String> {
    let p = state.profile.lock().await;
    Ok(pane_layout_envelope(&p))
}

/// Replace the active profile's pane layout and broadcast the
/// sanitized tree as `vosh://pane-layout-changed` to every window.
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
) -> Result<bool, String> {
    let mut layout = layout;
    layout.sanitize();
    let current = {
        let mut p = state.profile.lock().await;
        let current = panes_generation();
        if generation.is_some_and(|g| g != current) {
            return Ok(false);
        }
        p.ui.panes = Some(layout.clone());
        current
    };
    // A layout tweak after `#profile reset` must not save the blanked
    // profile, so this schedules without clearing the suppression.
    save_then_broadcast(
        &app,
        &state,
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
/// keeping whether its panel shows and how wide it is. The active
/// profile when `profile` is absent.
///
/// The live path bumps the pane generation under the profile lock, so
/// a splitter drag still in flight is refused rather than undoing the
/// reset, persists at once (unless `#profile reset` or `load` left the
/// profile diverged from disk), and broadcasts
/// `vosh://pane-layout-changed`. An inactive profile has its file
/// rewritten and only `vosh://profile-changed` goes out.
#[tauri::command]
pub(crate) async fn pane_layout_reset(
    app: AppHandle,
    state: State<'_, SharedState>,
    profile: Option<String>,
) -> Result<PaneLayoutEnvelope, String> {
    let shared: SharedState = state.inner().clone();
    if let Some(name) = profile.as_deref() {
        if let Some(layout) = reset_inactive_panes(&shared, name).await? {
            broadcast_profile_changed(&app, name);
            return Ok(PaneLayoutEnvelope {
                layout,
                generation: None,
            });
        }
    }
    let envelope = reset_live_panes(&shared).await;
    save_then_broadcast(
        &app,
        &shared,
        SavePolicy::NowUnlessHeld,
        PANE_LAYOUT_CHANGED,
        &envelope,
    )
    .await;
    if let Some(active) = active_name(&shared).await {
        broadcast_profile_changed(&app, &active);
    }
    Ok(envelope)
}

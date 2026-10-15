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
    broadcast_profile_changed, edit_inactive_panes, edit_open_panes, Stored,
};
use crate::profile::layout_history::{self, Earlier, Why};
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
        // The keys this build does not know are the stored layout's, never
        // the page's, whose own such as `generation` must not reach the
        // file.
        layout.extra =
            p.ui.panes
                .as_ref()
                .map(|stored| stored.extra.clone())
                .unwrap_or_default();
        if let (Some(app_data), Some(name)) = (state.app_data.get(), p.open().name()) {
            let before = p.ui.pane_layout();
            let now = layout_history::now_ms();
            layout_history::note(app_data, &name, &before, &layout, Why::Change, now);
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
    edit_panes(
        &app,
        state.inner(),
        profile,
        Why::Reset,
        PaneLayoutPersist::with_default_tree,
    )
    .await
}

/// Give the profile `profile` names the panel and sidebar trees of
/// `layout`, keeping whether its panel shows and how wide it is. A move in
/// Settings > Characters lands here, for a profile a session plays or
/// not, and saves and broadcasts as [`pane_layout_reset`] does. So does
/// Done in arrange mode, with `arranged` set, which always keeps the
/// layout it replaces among the earlier ones.
#[tauri::command]
pub(crate) async fn pane_trees_set(
    app: AppHandle,
    state: State<'_, SharedState>,
    profile: String,
    layout: PaneLayoutPersist,
    arranged: Option<bool>,
) -> Result<PaneLayoutEnvelope, String> {
    let why = if arranged == Some(true) {
        Why::Arrange
    } else {
        Why::Change
    };
    edit_panes(&app, state.inner(), Some(profile), why, |current| {
        current.with_trees(&layout)
    })
    .await
}

/// The name of the profile `profile` names, or of the selected session's.
fn profile_name(state: &SharedState, profile: Option<String>) -> Result<String, String> {
    profile
        .or_else(|| state.selected_session().profile().name())
        .ok_or_else(|| "No profile to read the earlier layouts of.".to_string())
}

/// The layouts the profile `profile` names had before, or the selected
/// session's, newest first. Settings > Characters > Panel layout and
/// arrange mode list them.
#[tauri::command]
pub(crate) async fn pane_history_get(
    state: State<'_, SharedState>,
    profile: Option<String>,
) -> Result<Vec<Earlier>, String> {
    let name = profile_name(&state, profile)?;
    let Some(app_data) = state.app_data.get() else {
        return Ok(Vec::new());
    };
    Ok(layout_history::read(app_data, &name))
}

/// Put back the earlier layout `until` names on the profile `profile`
/// names, or the selected session's, keeping whether its panel shows and
/// how wide it is. The layout it replaces joins the earlier ones, so a
/// restore can be undone the same way. Saves and broadcasts as
/// [`pane_layout_reset`] does.
#[tauri::command]
pub(crate) async fn pane_history_restore(
    app: AppHandle,
    state: State<'_, SharedState>,
    profile: Option<String>,
    until: i64,
) -> Result<PaneLayoutEnvelope, String> {
    let name = profile_name(&state, profile.clone())?;
    let app_data = state
        .app_data
        .get()
        .ok_or_else(|| "Vosh has no data folder to read.".to_string())?;
    let earlier = layout_history::read(app_data, &name)
        .into_iter()
        .find(|e| e.until == until)
        .ok_or_else(|| "That earlier layout is gone.".to_string())?;
    edit_panes(&app, state.inner(), profile, Why::Restore, |current| {
        current.with_trees(&earlier.panes)
    })
    .await
}

/// Edit the panes of the profile `profile` names, or of the selected
/// session's, with `edit`, then save and tell the windows, see
/// [`pane_layout_reset`]. The layout `edit` replaces joins the earlier
/// ones as `why` says.
async fn edit_panes(
    app: &AppHandle,
    shared: &SharedState,
    profile: Option<String>,
    why: Why,
    edit: impl Fn(&PaneLayoutPersist) -> PaneLayoutPersist + Send + Sync,
) -> Result<PaneLayoutEnvelope, String> {
    let app_data = shared.app_data.get().cloned();
    let keeping = |name: Option<String>| {
        let app_data = app_data.clone();
        let edit = &edit;
        move |before: &PaneLayoutPersist| {
            let after = edit(before);
            if let (Some(app_data), Some(name)) = (&app_data, &name) {
                let now = layout_history::now_ms();
                layout_history::note(app_data, name, before, &after, why, now);
            }
            after
        }
    };
    let named = match profile.as_deref() {
        Some(name) => {
            match edit_inactive_panes(shared, name, keeping(Some(name.to_string()))).await? {
                Stored::File(layout) => {
                    broadcast_profile_changed(app, name);
                    return Ok(PaneLayoutEnvelope {
                        layout,
                        generation: None,
                    });
                }
                Stored::Open(open) => Some(open),
            }
        }
        None => None,
    };
    let selected = shared.selected_session();
    let (open, shown, envelope) = {
        let mut p = match &named {
            Some(open) => open.lock().await,
            None => selected.lock_profile().await,
        };
        let shown = Arc::ptr_eq(p.open(), &selected.profile());
        let name = p.open().name();
        let envelope = edit_open_panes(shared, &mut p, shown, keeping(name));
        (p.open().clone(), shown, envelope)
    };
    if shown {
        save_then_broadcast(
            app,
            shared,
            &open,
            SavePolicy::NowUnlessHeld,
            PANE_LAYOUT_CHANGED,
            &envelope,
        )
        .await;
    } else {
        save_by(app, shared, &open, SavePolicy::NowUnlessHeld).await;
    }
    if let Some(name) = open.name() {
        broadcast_profile_changed(app, &name);
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

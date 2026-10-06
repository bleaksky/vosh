//! The commands for your profiles. Settings > Characters lists them and
//! creates, renames, copies, deletes and switches them, and Settings >
//! General chooses which settings every character shares. Before it
//! connects, the page asks which profile claims the login and switches
//! to it.
//!
//! `profiles.toml` indexes the profiles, each one saves to
//! `profiles/<name>.toml`, and `AppState.profile_set` holds the live set.

use tauri::{AppHandle, State};

use crate::app::events::{broadcast, CUSTOM_THEMES_CHANGED, PROFILES_CHANGED};
use crate::app::state::SharedState;
use crate::disk::save::{persist_state, PERSIST_LOCK};
use crate::profile::set::{create_profile, delete_profile, duplicate_profile, rename_profile};
use crate::profile::shared::change_scope_locked;
use crate::profile::switch::apply_profile_switch;
use crate::sessions::SessionId;

#[derive(serde::Serialize)]
pub(crate) struct ProfilesListPayload {
    pub active: String,
    pub profiles: Vec<crate::profile::set::ProfileEntry>,
}

#[tauri::command]
pub(crate) async fn profiles_list(
    state: State<'_, SharedState>,
) -> Result<ProfilesListPayload, String> {
    let set = state.loaded_profile_set().await?;
    Ok(ProfilesListPayload {
        active: set.active_name().to_string(),
        profiles: set.list().to_vec(),
    })
}

/// Create a profile with `auto_match` as its login claim, starting as a
/// copy of `copy_from` when given. Returns the new entry and does not
/// switch. The claim takes nothing from other profiles, so a caller
/// that wants the character for itself follows with
/// `profile_set_login`.
#[tauri::command]
pub(crate) async fn profile_create(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
    copy_from: Option<String>,
    auto_match: Option<crate::profile::login_match::AutoMatch>,
) -> Result<crate::profile::set::ProfileEntry, String> {
    let entry = create_profile(state.inner(), &name, copy_from.as_deref(), auto_match).await?;
    broadcast(&app, PROFILES_CHANGED, &entry.name);
    Ok(entry)
}

#[tauri::command]
pub(crate) async fn profile_delete(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
) -> Result<(), String> {
    delete_profile(state.inner(), &name).await?;
    broadcast(&app, PROFILES_CHANGED, &name);
    Ok(())
}

#[tauri::command]
pub(crate) async fn profile_rename(
    app: AppHandle,
    state: State<'_, SharedState>,
    old: String,
    new: String,
) -> Result<(), String> {
    rename_profile(state.inner(), &old, &new).await?;
    broadcast(&app, PROFILES_CHANGED, &new);
    // The row of each session on the profile names it anew.
    crate::sessions::broadcast_sessions(&app, state.inner());
    Ok(())
}

/// Copy `source` under a new name without its login claim. Duplicating
/// the live profile writes it first, so the copy holds your latest
/// changes.
#[tauri::command]
pub(crate) async fn profile_duplicate(
    app: AppHandle,
    state: State<'_, SharedState>,
    source: String,
    new: String,
) -> Result<(), String> {
    duplicate_profile(state.inner(), &source, &new).await?;
    broadcast(&app, PROFILES_CHANGED, &new);
    Ok(())
}

/// Read the per-category scope map. Frontend uses this to render
/// the toggle row in the Profiles tab.
#[tauri::command]
pub(crate) async fn profile_get_scope(
    state: State<'_, SharedState>,
) -> Result<crate::profile::shared::ScopeConfig, String> {
    let set = state.loaded_profile_set().await?;
    Ok(*set.scope())
}

/// Update the per-category scope map. After the index is updated,
/// persist the active profile so values move to the correct file
/// (a category flipped Global -> Profile lands in the per-profile
/// file on next save; Profile -> Global lands in global.toml).
///
/// Turning the theme category global also folds the custom themes the
/// other profile files hold into the shared list and clears them from
/// those files, or a switch to one of those profiles would lose them.
/// When the list grows, `vosh://custom-themes-changed` carries it to
/// every window.
///
/// Turning a category per profile first copies the shared values into
/// every other profile file that holds none of its own, since the save
/// drops them from global.toml.
#[tauri::command]
pub(crate) async fn profile_set_scope(
    app: AppHandle,
    state: State<'_, SharedState>,
    scope: crate::profile::shared::ScopeConfig,
) -> Result<(), String> {
    // Held from the scope change through the persist, so no other
    // profile file write lands between the moves and the save.
    let persist_guard = PERSIST_LOCK.lock().await;
    let shared: SharedState = state.inner().clone();
    let gained = change_scope_locked(&shared, scope).await?;
    persist_state(&shared, &shared.selected_session().profile()).await;
    drop(persist_guard);
    if let Some(list) = gained {
        broadcast(&app, CUSTOM_THEMES_CHANGED, &list);
    }
    broadcast(&app, PROFILES_CHANGED, &"scope");
    Ok(())
}

/// Given a connect target, find the first profile whose `auto_match`
/// claims it. Returns the profile name or null. The frontend calls
/// this right before invoking `session_connect` so a matching
/// profile can be switched to ahead of the connection. With
/// `any_character`, a claim that names characters counts as if one of
/// them logged in, which the New session form asks before anyone logs in
/// (Sessions Q2), see [`ProfileSet::resolve_before_login`].
///
/// [`ProfileSet::resolve_before_login`]: crate::profile::set::ProfileSet::resolve_before_login
#[tauri::command]
pub(crate) async fn profile_resolve_match(
    state: State<'_, SharedState>,
    host: String,
    port: u16,
    character: Option<String>,
    any_character: Option<bool>,
) -> Result<Option<String>, String> {
    let guard = state.profile_set.lock().await;
    let Some(set) = guard.as_ref() else {
        return Ok(None);
    };
    Ok(if any_character == Some(true) {
        set.resolve_before_login(&host, port)
    } else {
        set.resolve_match(&host, port, character.as_deref())
    })
}

/// Keep the profile `profile` names open while a Settings page holds
/// unsaved edits on it, so a Save still finds it after its last session
/// leaves it. With no profile, Save or Discard lets go, and the profile
/// that held saves and closes when no session plays it.
#[tauri::command]
pub(crate) async fn profile_hold_edits(
    state: State<'_, SharedState>,
    profile: Option<String>,
) -> Result<(), String> {
    hold_edits(state.inner(), profile.as_deref()).await;
    Ok(())
}

/// The body of [`profile_hold_edits`], which the Settings window also
/// runs with no profile as it closes.
pub(crate) async fn hold_edits(state: &SharedState, profile: Option<&str>) {
    // Under the save lock, as every step that opens or closes a profile.
    let _persist_guard = PERSIST_LOCK.lock().await;
    if let Some(left) = state.edit_hold() {
        let keeps = profile.is_some_and(|name| left.name().as_deref() == Some(name));
        if !keeps && state.players(&left) == 0 && state.is_open(&left) && !left.held() {
            persist_state(state, &left).await;
        }
    }
    if let Some(left) = state.hold_edits(profile) {
        crate::profile::switch::leave_file(state, &left).await;
    }
}

/// Switch the session to the profile `name`.
#[tauri::command]
pub(crate) async fn profile_switch(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
    session: Option<SessionId>,
) -> Result<(), String> {
    let shared: SharedState = state.inner().clone();
    let session = shared.session(session)?;
    apply_profile_switch(&app, &shared, &session, &name).await
}

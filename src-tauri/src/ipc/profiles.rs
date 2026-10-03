//! The commands for your profiles. Settings > Characters lists them and
//! creates, renames, copies, deletes and switches them, and Settings >
//! General chooses which settings every character shares. Before it
//! connects, the page asks which profile claims the login and switches
//! to it.
//!
//! `profiles.toml` indexes the profiles, each one saves to
//! `profiles/<name>.toml`, and `AppState.profile_set` holds the live set.

use tauri::{AppHandle, Manager, State};

use crate::app::events::{broadcast, CUSTOM_THEMES_CHANGED, PROFILES_CHANGED};
use crate::app::state::{SharedState, AUTO_PERSIST_SUPPRESSED, MIGRATION_RELAUNCH_PENDING};
use crate::disk::save::{persist_profile_locked, persist_state, PERSIST_LOCK};
use crate::profile::shared::change_scope_locked;
use crate::profile::switch::apply_profile_switch;

#[derive(serde::Serialize)]
pub(crate) struct ProfilesListPayload {
    pub active: String,
    pub profiles: Vec<crate::profile_set::ProfileEntry>,
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

/// Write the live profile to its file before a copy of `source` reads
/// that file, when `source` is the live profile. Call with
/// [`PERSIST_LOCK`] held across this and the copy, so the copy reads
/// what the flush wrote and no persist rewrites the source mid copy.
async fn flush_before_copy(shared: &SharedState, app_data: Option<&std::path::Path>, source: &str) {
    let copying_live = shared
        .profile_set
        .lock()
        .await
        .as_ref()
        .is_some_and(|set| set.active_name() == source);
    // The live profile can run two seconds ahead of its file. After
    // `#profile reset` or `load` it is deliberately diverged, and the
    // copy takes the file as it stands.
    if copying_live && !AUTO_PERSIST_SUPPRESSED.load(std::sync::atomic::Ordering::Acquire) {
        persist_state(shared, app_data).await;
    }
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
    auto_match: Option<crate::profile_set::AutoMatch>,
) -> Result<crate::profile_set::ProfileEntry, String> {
    let app_data = app.path().app_data_dir().ok();
    let entry = create_profile(
        state.inner(),
        app_data.as_deref(),
        &name,
        copy_from.as_deref(),
        auto_match,
        &MIGRATION_RELAUNCH_PENDING,
    )
    .await?;
    broadcast(&app, PROFILES_CHANGED, &entry.name);
    Ok(entry)
}

/// The body of [`profile_create`] over the app data folder `app_data`,
/// with `relaunch_pending` in place of [`MIGRATION_RELAUNCH_PENDING`], so
/// a test can run it after the wizard.
pub(crate) async fn create_profile(
    state: &SharedState,
    app_data: Option<&std::path::Path>,
    name: &str,
    copy_from: Option<&str>,
    auto_match: Option<crate::profile_set::AutoMatch>,
    relaunch_pending: &std::sync::atomic::AtomicBool,
) -> Result<crate::profile_set::ProfileEntry, String> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    if let Some(source) = copy_from {
        // Read under the lock, which the wizard holds until it sets the
        // flag.
        if relaunch_pending.load(std::sync::atomic::Ordering::Acquire) {
            return Err(COPY_MIGRATION_PENDING.into());
        }
        flush_before_copy(state, app_data, source).await;
    }
    let mut set = state.loaded_profile_set().await?;
    set.create_from(name, copy_from, auto_match)
        .map_err(|e| e.to_string())
}

/// Why a profile cannot be renamed between `migration_apply` and the
/// relaunch that finishes it, or while launch could not finish a wizard
/// run. The next launch writes each profile file the run names under the
/// name it had, and the renamed file would keep what the move took out.
const RENAME_MIGRATION_PENDING: &str =
    "Quit Vosh and open it again to finish the move to loadouts, then rename the profile.";

/// Why a profile cannot be copied in the same window. The copy would take
/// a file the next launch has yet to finish, or the live profile a copy of
/// it saves first, which still holds the items the move took out.
const COPY_MIGRATION_PENDING: &str =
    "Quit Vosh and open it again to finish the move to loadouts, then copy the profile.";

#[tauri::command]
pub(crate) async fn profile_delete(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
) -> Result<(), String> {
    {
        let _persist_guard = PERSIST_LOCK.lock().await;
        let mut set = state.loaded_profile_set().await?;
        set.delete(&name).map_err(|e| e.to_string())?;
    }
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
    rename_profile(state.inner(), &old, &new, &MIGRATION_RELAUNCH_PENDING).await?;
    broadcast(&app, PROFILES_CHANGED, &new);
    Ok(())
}

/// The body of [`profile_rename`], with `relaunch_pending` in place of
/// [`MIGRATION_RELAUNCH_PENDING`], so a test can run it after the wizard.
pub(crate) async fn rename_profile(
    state: &SharedState,
    old: &str,
    new: &str,
    relaunch_pending: &std::sync::atomic::AtomicBool,
) -> Result<(), String> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    // Read under the lock, which the wizard holds until it sets the flag.
    if relaunch_pending.load(std::sync::atomic::Ordering::Acquire) {
        return Err(RENAME_MIGRATION_PENDING.into());
    }
    let live = {
        let mut set = state.loaded_profile_set().await?;
        let renames_live = set.active_name() == old;
        set.rename(old, new).map_err(|e| e.to_string())?;
        if renames_live {
            state.note_active_profile(set.active_name());
        }
        renames_live.then(|| crate::profile_set::display_name(set.active_name()))
    };
    // The custom prompt draws the live profile's new name.
    if let Some(name) = live {
        state.profile.lock().await.display_name = Some(name);
    }
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
    let app_data = app.path().app_data_dir().ok();
    duplicate_profile(
        state.inner(),
        app_data.as_deref(),
        &source,
        &new,
        &MIGRATION_RELAUNCH_PENDING,
    )
    .await?;
    broadcast(&app, PROFILES_CHANGED, &new);
    Ok(())
}

/// The body of [`profile_duplicate`] over the app data folder `app_data`,
/// with `relaunch_pending` in place of [`MIGRATION_RELAUNCH_PENDING`], so
/// a test can run it after the wizard.
pub(crate) async fn duplicate_profile(
    state: &SharedState,
    app_data: Option<&std::path::Path>,
    source: &str,
    new: &str,
    relaunch_pending: &std::sync::atomic::AtomicBool,
) -> Result<(), String> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    // Read under the lock, which the wizard holds until it sets the flag.
    if relaunch_pending.load(std::sync::atomic::Ordering::Acquire) {
        return Err(COPY_MIGRATION_PENDING.into());
    }
    flush_before_copy(state, app_data, source).await;
    let mut set = state.loaded_profile_set().await?;
    set.duplicate(source, new).map_err(|e| e.to_string())
}

/// Read the per-category scope map. Frontend uses this to render
/// the toggle row in the Profiles tab.
#[tauri::command]
pub(crate) async fn profile_get_scope(
    state: State<'_, SharedState>,
) -> Result<crate::profile_set::ScopeConfig, String> {
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
    scope: crate::profile_set::ScopeConfig,
) -> Result<(), String> {
    // Held from the scope change through the persist, so no other
    // profile file write lands between the moves and the save.
    let persist_guard = PERSIST_LOCK.lock().await;
    let shared: SharedState = state.inner().clone();
    let gained = change_scope_locked(&shared, scope).await?;
    persist_profile_locked(&app, &shared).await;
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
/// profile can be switched to ahead of the connection.
#[tauri::command]
pub(crate) async fn profile_resolve_match(
    state: State<'_, SharedState>,
    host: String,
    port: u16,
    character: Option<String>,
) -> Result<Option<String>, String> {
    let guard = state.profile_set.lock().await;
    let Some(set) = guard.as_ref() else {
        return Ok(None);
    };
    Ok(set.resolve_match(&host, port, character.as_deref()))
}

#[tauri::command]
pub(crate) async fn profile_switch(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
) -> Result<(), String> {
    let shared: SharedState = state.inner().clone();
    apply_profile_switch(&app, &shared, &name).await
}

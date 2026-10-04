//! The commands for Settings > Characters. The page reads any profile
//! through them, active or not, turns a character's login on or off,
//! points a profile at a world and saves a profile's settings to your
//! Downloads folder. Settings and the prompt card also ask who is
//! logged in.

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tracing::warn;

use crate::app::events::{broadcast, PROFILES_CHANGED};
use crate::app::state::SharedState;
use crate::profile::inactive::{export_path, profile_detail, profile_toml, ProfileDetail};
use crate::profile::login_match::LoginClaim;
use crate::profile::set::{display_name, ProfileEntry};
use crate::session::identity::{session_identity, SessionIdentity};
use crate::sessions::SessionId;

/// Read one profile for the Characters group.
#[tauri::command]
pub(crate) async fn profile_detail_get(
    state: State<'_, SharedState>,
    name: String,
) -> Result<ProfileDetail, String> {
    profile_detail(state.inner(), &name).await
}

/// Turn the login toggle for `name` on or off for `character`. On takes
/// the character from every other profile on the same world and names
/// them in `released_from`, or pins an older claim on the host alone to
/// the world's own port and names it in `pinned`. Never switches the
/// live profile, since the toggle applies at the next login. See
/// [`ProfileSet::set_login`].
///
/// [`ProfileSet::set_login`]: crate::profile::set::ProfileSet::set_login
#[tauri::command]
pub(crate) async fn profile_set_login(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
    character: String,
    on: bool,
) -> Result<LoginClaim, String> {
    let claim = {
        let mut set = state.loaded_profile_set().await?;
        set.set_login(&name, &character, on)
            .map_err(|e| e.to_string())?
    };
    broadcast(&app, PROFILES_CHANGED, &name);
    Ok(claim)
}

/// Point `name` at a world. Edits only the host and port, so it cannot
/// overwrite a description or characters the other window holds. See
/// [`ProfileSet::set_world`].
///
/// [`ProfileSet::set_world`]: crate::profile::set::ProfileSet::set_world
#[tauri::command]
pub(crate) async fn profile_set_world(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
    host: Option<String>,
    port: Option<u16>,
) -> Result<ProfileEntry, String> {
    let entry = {
        let mut set = state.loaded_profile_set().await?;
        set.set_world(&name, host, port)
            .map_err(|e| e.to_string())?
    };
    broadcast(&app, PROFILES_CHANGED, &name);
    Ok(entry)
}

/// Where an export went, for the sentence Settings shows.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProfileExport {
    pub path: String,
    pub file_name: String,
}

/// Save a profile's settings as a TOML file in your Downloads folder,
/// active or not, and say where it went. Settings has no save panel,
/// so the file takes a name that never replaces another.
#[tauri::command]
pub(crate) async fn profile_export_file(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
) -> Result<ProfileExport, String> {
    let toml = profile_toml(state.inner(), &name).await?;
    let dir = app
        .path()
        .download_dir()
        .map_err(|_| "Vosh could not find your Downloads folder.".to_string())?;
    let path = export_path(&dir, &name);
    std::fs::write(&path, toml).map_err(|e| {
        warn!(error = %e, path = %path.display(), "profile export write failed");
        format!(
            "Vosh could not save the {} profile in your Downloads folder.",
            display_name(&name)
        )
    })?;
    Ok(ProfileExport {
        file_name: path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: path.display().to_string(),
    })
}

/// Who is logged in on the session: the connection, the character once
/// known, the live profile, and which profile claims that character. Null
/// while the session runs no connection.
#[tauri::command]
pub(crate) async fn session_identity_get(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<Option<SessionIdentity>, String> {
    let session = state.session(session)?;
    Ok(session_identity(state.inner(), &session).await)
}

//! The commands for Settings > Characters. The page reads any profile
//! through them, active or not, turns a character's login on or off,
//! points a profile at a world, saves a profile's settings to your
//! Downloads folder, and reads and imports a profile export you pick.
//! Settings and the prompt card also ask who is logged in.

use serde::Serialize;
use tauri::{AppHandle, State};
use tracing::warn;

use crate::app::events::{broadcast, PROFILES_CHANGED};
use crate::app::state::SharedState;
use crate::import::vosh::apply::{apply_import, restore_profile, AddAs, ImportResult};
use crate::import::vosh::{preview, ImportPreview};
use crate::profile::backups::{self, ProfileBackup};
use crate::profile::inactive::{export_path, export_text, profile_detail, ProfileDetail};
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
/// so the file takes a name that never replaces another. The file ends
/// with the `[vosh_export]` table, which names the profile's world and,
/// of the characters it claims, those in `characters`, the ones you
/// ticked, so a profile you share names your alts only by choice.
#[tauri::command]
pub(crate) async fn profile_export_file(
    app: AppHandle,
    state: State<'_, SharedState>,
    name: String,
    characters: Vec<String>,
) -> Result<ProfileExport, String> {
    let toml = export_text(state.inner(), &name, &characters).await?;
    let path = export_path(&super::downloads_dir(&app)?, &name);
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

/// Read `text`, the file you picked as `file_name`, as a Vosh profile
/// export, and say what it holds and where it would go. Changes nothing.
/// See [`preview`].
#[tauri::command]
pub(crate) async fn profile_import_read(
    state: State<'_, SharedState>,
    file_name: String,
    text: String,
) -> Result<ImportPreview, String> {
    let loadout_mode = state.global_catalog.lock().await.is_some();
    let set = state.loaded_profile_set().await?;
    preview(&set, &file_name, &text, loadout_mode)
}

/// Import `text`, the Vosh profile export you picked as `file_name`, as a
/// new profile named `name` or over your profile `name`, and say what
/// happened. `logins` names each character of the file you left on,
/// which the new profile takes, from another profile if one has it. See
/// [`apply_import`].
#[tauri::command]
pub(crate) async fn profile_import_apply<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    file_name: String,
    text: String,
    add_as: AddAs,
    name: String,
    logins: Vec<String>,
) -> Result<ImportResult, String> {
    apply_import(
        &app,
        state.inner(),
        &file_name,
        &text,
        add_as,
        &name,
        &logins,
    )
    .await
}

/// The backups Vosh kept of the profile `name`'s file, newest first, see
/// [`backups::list`].
#[tauri::command]
pub(crate) async fn profile_backups(
    state: State<'_, SharedState>,
    name: String,
) -> Result<Vec<ProfileBackup>, String> {
    let path = profile_file(state.inner(), &name).await?;
    Ok(backups::list(&path))
}

/// Put the profile `name` back as its backup `file` holds it, see
/// [`restore_profile`].
#[tauri::command]
pub(crate) async fn profile_restore<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    name: String,
    file: String,
) -> Result<(), String> {
    restore_profile(&app, state.inner(), &name, &file).await
}

/// Show the profile `name`'s file in the file manager, or the profiles
/// folder when the profile never saved one.
#[tauri::command]
pub(crate) async fn profile_reveal(
    state: State<'_, SharedState>,
    name: String,
) -> Result<(), String> {
    let path = profile_file(state.inner(), &name).await?;
    let shown = if path.is_file() {
        crate::app::plugins::reveal::reveal_file(&path)
    } else {
        crate::app::plugins::reveal::reveal(path.parent().unwrap_or(&path))
    };
    shown.map_err(|e| {
        warn!(profile = %name, error = %e, "could not open the file manager");
        format!(
            "Vosh could not show the file of the {} profile.",
            display_name(&name)
        )
    })
}

/// Where the profile `name` keeps its file, once the set knows it.
async fn profile_file(state: &SharedState, name: &str) -> Result<std::path::PathBuf, String> {
    let set = state.loaded_profile_set().await?;
    if set.get(name).is_none() {
        return Err(crate::profile::set::ProfileSetError::NotFound(name.to_string()).to_string());
    }
    Ok(set.profile_path(name))
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

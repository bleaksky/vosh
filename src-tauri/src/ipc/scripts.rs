//! The commands of the Scripts page in Settings: your plugins, the Output
//! ring of a session and the console. Each takes the session it acts on,
//! or the selected one, since each session runs Lua of its own and plays
//! the profile that turns plugins on. Each command that changes a plugin
//! tells every window on `vosh://plugins-changed`, and returns the list
//! as its session sees it then.

use serde::Serialize;
use tauri::{AppHandle, State};
use tracing::warn;
use vosh_script::{Owner, StopReason};

use crate::app::events::{broadcast, PLUGINS_CHANGED};
use crate::app::plugins::archive::{self, DroppedFile, Package, Source};
use crate::app::plugins::folder::{self, plugin_name_ok, PluginFolder};
use crate::app::plugins::{live, plugins_dir_of, reveal, PluginManifest};
use crate::app::state::SharedState;
use crate::script::output::LuaLine;
use crate::sessions::{Session, SessionId};

/// One plugin in the list Scripts shows, as one session sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PluginRow {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) author: String,
    pub(crate) description: String,
    /// The file it runs first.
    pub(crate) entry: String,
    /// The profile the session plays turns it on.
    pub(crate) on: bool,
    /// Why Vosh stopped it in the session, while it holds it off.
    pub(crate) stopped: Option<PluginStop>,
    /// When it last loaded in the session, in milliseconds since the Unix
    /// epoch, so the page marks only an error of the code that runs now.
    pub(crate) loaded_ms: Option<i64>,
    /// Its name breaks the rule New plugin shows, which only a folder
    /// you named by hand can do. It loads all the same, and the page
    /// cannot open it but can turn it off.
    pub(crate) misnamed: bool,
}

/// Why Vosh stopped a plugin, as its stop line tells you.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PluginStop {
    /// One call ran past 100 ms.
    Time,
    /// One call used more than 32 MB.
    CallMemory,
    /// Your scripts held more than 128 MB.
    StateMemory,
}

impl From<StopReason> for PluginStop {
    fn from(reason: StopReason) -> Self {
        match reason {
            StopReason::Time => Self::Time,
            StopReason::CallMemory => Self::CallMemory,
            StopReason::StateMemory => Self::StateMemory,
        }
    }
}

/// The plugins in your plugins folder as `session` sees them, sorted by
/// name. A folder whose name breaks the rule New plugin shows, which only
/// a hand edit makes, still loads at launch, so the list shows it too,
/// as one the page cannot open.
async fn plugin_rows(state: &SharedState, session: &Session) -> Result<Vec<PluginRow>, String> {
    let plugins_dir = plugins_dir_of(state)?;
    // The manager is held alone, and lets go before the profile locks.
    let manifests = {
        let mut manager = state.plugins.lock().await;
        manager.set_plugins_dir(plugins_dir);
        manager.discover().map_err(|e| {
            warn!(error = %e, "could not list the plugins");
            "Vosh could not read your plugins folder.".to_string()
        })?;
        manager.list().to_vec()
    };
    let enabled = session.lock_profile().await.plugins.enabled.clone();
    let c = session.connection.lock();
    Ok(manifests
        .into_iter()
        .map(|manifest| {
            let owner = Owner::Plugin(manifest.name.clone());
            PluginRow {
                on: enabled.contains(&manifest.name),
                stopped: c.script.stop_reason(&owner).map(PluginStop::from),
                loaded_ms: c.lua_output.loaded_at(&owner),
                misnamed: !plugin_name_ok(&manifest.name),
                name: manifest.name,
                version: manifest.version,
                author: manifest.author,
                description: manifest.description,
                entry: manifest.entry,
            }
        })
        .collect())
}

/// Tell every window the plugins changed, then hand back the list as
/// `session` sees it now.
async fn changed<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    session: &Session,
) -> Result<Vec<PluginRow>, String> {
    broadcast(app, PLUGINS_CHANGED, &());
    plugin_rows(state, session).await
}

/// The plugins in your plugins folder as `session` sees them: whether
/// its profile turns each on, and why Vosh stopped one in it.
#[tauri::command]
pub(crate) async fn plugins_list(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<Vec<PluginRow>, String> {
    let session = state.session(session)?;
    plugin_rows(&state, &session).await
}

/// The plugin `name` as its folder holds it: the manifest, the code of
/// the file it runs first, or of `file` when Runs first picks another,
/// and every Lua file in the folder.
#[tauri::command]
pub(crate) async fn plugin_read(
    state: State<'_, SharedState>,
    name: String,
    file: Option<String>,
) -> Result<PluginFolder, String> {
    folder::read(&plugins_dir_of(&state)?, &name, file.as_deref())
}

/// Make the plugin `name` with the two files New plugin writes, turn it
/// on in the profile `session` plays, and load it in every session on
/// that profile.
#[tauri::command]
pub(crate) async fn plugin_create<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    name: String,
    session: Option<SessionId>,
) -> Result<Vec<PluginRow>, String> {
    let session = state.session(session)?;
    folder::create(&plugins_dir_of(&state)?, &name)?;
    live::set_enabled(&app, &state, &session, &name, true).await?;
    changed(&app, &state, &session).await
}

/// Save `code` and `manifest` to the plugin `name`, then load it again
/// in every session whose profile turns it on, which clears a stop.
#[tauri::command]
pub(crate) async fn plugin_save<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    name: String,
    manifest: PluginManifest,
    code: String,
    session: Option<SessionId>,
) -> Result<Vec<PluginRow>, String> {
    let session = state.session(session)?;
    folder::save(&plugins_dir_of(&state)?, &name, manifest, &code)?;
    live::reload_everywhere(&app, &state, &name).await?;
    changed(&app, &state, &session).await
}

/// Turn the plugin `name` on or off in the profile `session` plays, which
/// loads or unloads it in every session on that profile, and save the
/// profile. Off takes any name, so a plugin whose folder you named by hand
/// can always be turned off.
#[tauri::command]
pub(crate) async fn plugin_set_enabled<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    name: String,
    on: bool,
    session: Option<SessionId>,
) -> Result<Vec<PluginRow>, String> {
    let session = state.session(session)?;
    live::set_enabled(&app, &state, &session, &name, on).await?;
    changed(&app, &state, &session).await
}

/// Read the plugin `name` from its folder again and load it in every
/// session whose profile turns it on, which clears a stop, for Reload in
/// a plugin's menu and for edits made in another editor.
#[tauri::command]
pub(crate) async fn plugin_reload<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    name: String,
    session: Option<SessionId>,
) -> Result<Vec<PluginRow>, String> {
    let session = state.session(session)?;
    live::reload_everywhere(&app, &state, &name).await?;
    changed(&app, &state, &session).await
}

/// Show the folder of the plugin `name` in the system's file manager.
#[tauri::command]
pub(crate) async fn plugin_reveal(
    state: State<'_, SharedState>,
    name: String,
) -> Result<(), String> {
    let dir = folder::existing(&plugins_dir_of(&state)?, &name)?;
    reveal::reveal(&dir).map_err(|e| {
        warn!(plugin = %name, error = %e, "could not open the file manager");
        format!("Vosh could not show the folder of {name}.")
    })
}

/// What Install asks about before it installs a plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PluginInstallCheck {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) author: String,
    /// The plugin of that name you have, which the install replaces.
    pub(crate) existing: Option<InstalledPlugin>,
}

/// A plugin an install would replace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct InstalledPlugin {
    pub(crate) version: String,
    /// The profiles that turn it on, by the names Settings shows.
    pub(crate) on_in: Vec<String>,
}

/// What Install reads: the bytes of a .zip, or else the files of a
/// dropped folder. Neither reads as an empty folder, which holds no
/// manifest.
fn source(bytes: Option<Vec<u8>>, files: Option<Vec<DroppedFile>>) -> Source {
    match bytes {
        Some(bytes) => Source::Zip(bytes),
        None => Source::Folder(files.unwrap_or_default()),
    }
}

/// Read and check the plugin in `file_name`, a .zip as `bytes` or a
/// dropped folder as `files`. Returns it with the version of the plugin
/// of that name you have, if you have one. One you have in another case
/// is refused, as New plugin refuses it.
fn package(
    plugins_dir: &std::path::Path,
    file_name: &str,
    bytes: Option<Vec<u8>>,
    files: Option<Vec<DroppedFile>>,
) -> Result<(Package, Option<String>), String> {
    let package = archive::read(file_name, source(bytes, files))?;
    let existing = folder::installed(plugins_dir, &package.manifest.name)?;
    Ok((package, existing))
}

/// Check the plugin in `file_name`, a .zip as `bytes` or a dropped folder
/// as `files`, for the question Install asks: its name, version and
/// author, and the plugin of that name you have, with the profiles that
/// turn it on. A plugin Vosh refuses is the sentence that says why.
#[tauri::command]
pub(crate) async fn plugin_install_check(
    state: State<'_, SharedState>,
    file_name: String,
    bytes: Option<Vec<u8>>,
    files: Option<Vec<DroppedFile>>,
) -> Result<PluginInstallCheck, String> {
    let (package, existing) = package(&plugins_dir_of(&state)?, &file_name, bytes, files)?;
    let manifest = package.manifest;
    let existing = match existing {
        Some(version) => Some(InstalledPlugin {
            version,
            on_in: live::turned_on_in(&state, &manifest.name).await?,
        }),
        None => None,
    };
    Ok(PluginInstallCheck {
        name: manifest.name,
        version: manifest.version,
        author: manifest.author,
        existing,
    })
}

/// Install the plugin in `file_name`, a .zip as `bytes` or a dropped
/// folder as `files`, in place of the plugin of that name you have. It
/// starts off in every profile, so a profile that turns the name on
/// turns it off first and its sessions unload it, so its new code never
/// runs until you turn it on.
#[tauri::command]
pub(crate) async fn plugin_install<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    file_name: String,
    bytes: Option<Vec<u8>>,
    files: Option<Vec<DroppedFile>>,
    session: Option<SessionId>,
) -> Result<Vec<PluginRow>, String> {
    let session = state.session(session)?;
    let plugins_dir = plugins_dir_of(&state)?;
    let (package, _) = package(&plugins_dir, &file_name, bytes, files)?;
    live::off_everywhere(&app, &state, &package.manifest.name).await?;
    let installed = {
        // Held alone, so a listing never finds the folder halfway.
        let _manager = state.plugins.lock().await;
        archive::install(&plugins_dir, &package)
    };
    // The plugin is off everywhere now, even when its folder stayed.
    let rows = changed(&app, &state, &session).await;
    installed.and(rows)
}

/// Write the plugin `name` to a .zip in your Downloads folder. Returns
/// the name of the file, for the line that says where it went.
#[tauri::command]
pub(crate) async fn plugin_export<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    name: String,
) -> Result<String, String> {
    let path = archive::export(
        &plugins_dir_of(&state)?,
        &name,
        &super::downloads_dir(&app)?,
    )?;
    Ok(path
        .file_name()
        .map(|file| file.to_string_lossy().into_owned())
        .unwrap_or_default())
}

/// Remove the plugin `name`: unload it in every session, take it off the
/// list of every profile, so no profile names a plugin you no longer
/// have, and delete its folder.
#[tauri::command]
pub(crate) async fn plugin_remove<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    name: String,
    session: Option<SessionId>,
) -> Result<Vec<PluginRow>, String> {
    let session = state.session(session)?;
    let plugins_dir = plugins_dir_of(&state)?;
    folder::existing(&plugins_dir, &name)?;
    live::off_everywhere(&app, &state, &name).await?;
    let removed = {
        let _manager = state.plugins.lock().await;
        folder::remove(&plugins_dir, &name)
    };
    let rows = changed(&app, &state, &session).await;
    removed.and(rows)
}

/// The lines in the Output ring of `session`, oldest first.
#[tauri::command]
pub(crate) async fn lua_output_get(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<Vec<LuaLine>, String> {
    let session = state.session(session)?;
    let lines = session.connection.lock().lua_output.lines();
    Ok(lines)
}

/// Clear the lines of `owner`, a tag like `plugin:vitals_alert`, from
/// the Output ring of `session`, or every line with no owner.
#[tauri::command]
pub(crate) async fn lua_output_clear(
    state: State<'_, SharedState>,
    owner: Option<String>,
    session: Option<SessionId>,
) -> Result<(), String> {
    let session = state.session(session)?;
    session.connection.lock().lua_output.clear(owner.as_deref());
    Ok(())
}

/// Run `code` from the console in `session`: inside the plugin `plugin`,
/// or in the global environment as a `#lua` line runs. What it prints
/// shows in Output and in the terminal, and what it sends goes to the
/// game when the session runs a connection.
#[tauri::command]
pub(crate) async fn lua_run<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    code: String,
    plugin: Option<String>,
    session: Option<SessionId>,
) -> Result<(), String> {
    let session = state.session(session)?;
    let apply = {
        let mut p = session.lock_profile().await;
        let mut c = session.connection.lock();
        crate::script::run_console(&mut p, &mut c, &code, plugin.as_deref()).ran_under(p.open())
    };
    crate::session::effects::deliver_detached(&app, &session, apply).await;
    Ok(())
}

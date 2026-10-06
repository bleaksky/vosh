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
}

/// Why Vosh stopped a plugin, as the stop lines of the Scripts design
/// tell it.
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
/// a hand edit makes, still loads at launch, but the list leaves it out.
async fn plugin_rows(state: &SharedState, session: &Session) -> Result<Vec<PluginRow>, String> {
    let plugins_dir = plugins_dir_of(state)?;
    // The manager is held alone, and lets go before the profile locks.
    let records = {
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
    Ok(records
        .into_iter()
        .filter(|record| plugin_name_ok(&record.manifest.name))
        .map(|record| {
            let manifest = record.manifest;
            let owner = Owner::Plugin(manifest.name.clone());
            PluginRow {
                on: enabled.contains(&manifest.name),
                stopped: c.script.stop_reason(&owner).map(PluginStop::from),
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
/// the file it runs first, and every Lua file in the folder.
#[tauri::command]
pub(crate) async fn plugin_read(
    state: State<'_, SharedState>,
    name: String,
) -> Result<PluginFolder, String> {
    folder::read(&plugins_dir_of(&state)?, &name)
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
/// profile.
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

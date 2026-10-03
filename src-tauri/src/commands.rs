//! Tauri commands invoked by the frontend.

use tauri::{AppHandle, Manager, State};

use crate::app::state::{SharedState, MIGRATION_RELAUNCH_PENDING};
use crate::app::windows::{open_aux_window, HELP_WINDOW, SETTINGS_WINDOW};
use crate::loadouts::wizard::apply::{
    analyze_migration, announce_migration_applied, apply_migration, ConflictResolution,
};
use crate::prompt::{prompt_show_state, reported_hidden, PromptShowState};

/// What launch has to tell you, for the main window to show once in the
/// terminal and as a toast.
#[tauri::command]
pub(crate) fn launch_notices_take(state: State<'_, SharedState>) -> Vec<String> {
    state.take_launch_notices()
}

/// Open (or focus, if already open) the standalone settings window,
/// where the React entry renders `SettingsApp`.
#[tauri::command]
pub(crate) async fn open_settings_window(app: AppHandle) -> Result<(), String> {
    open_aux_window(&app, &SETTINGS_WINDOW)
}

/// Open (or focus, if already open) the Help window, where the React
/// entry renders `HelpApp`. The page that asked leaves the topic or the
/// search it should land on (src/lib/helpLink.ts).
#[tauri::command]
pub(crate) async fn open_help_window(app: AppHandle) -> Result<(), String> {
    open_aux_window(&app, &HELP_WINDOW)
}

/// Which values the game hides, as every open window last heard it on
/// `session://hidden`. The session reports each change once, so a window
/// that opens or reloads while the game hides something, Settings among
/// them, reads the state here. Nothing is hidden with no connection.
#[tauri::command]
pub(crate) async fn hidden_get(
    state: State<'_, SharedState>,
) -> Result<vosh_prompt::values::Hidden, String> {
    Ok(reported_hidden(state.inner()).await)
}

/// Where the active profile's prompt shows, and whether it reads one.
/// Every window reads it again on `vosh://prompt-config-changed`.
#[tauri::command]
pub(crate) async fn prompt_show_get(
    state: State<'_, SharedState>,
) -> Result<PromptShowState, String> {
    Ok(prompt_show_state(&*state.profile.lock().await))
}

/// The triggers that hid your prompt this session while the profile
/// reads no prompt, so Vosh drew nothing in its place. The session names
/// each one once on `session://prompt-gag-without-reader`, so a window
/// that opens later, Settings among them, reads the list here. Empty
/// with no connection.
#[tauri::command]
pub(crate) async fn prompt_gags_without_reader(
    state: State<'_, SharedState>,
) -> Result<Vec<String>, String> {
    let p = state.profile.lock().await;
    Ok(p.prompt
        .stage
        .gags_without_reader()
        .map(str::to_string)
        .collect())
}

#[derive(serde::Serialize)]
pub(crate) struct UpdateCheckResult {
    pub available: bool,
    pub version: Option<String>,
    pub notes: Option<String>,
}

#[tauri::command]
pub(crate) async fn updater_check(app: AppHandle) -> Result<UpdateCheckResult, String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => Ok(UpdateCheckResult {
            available: true,
            version: Some(update.version.clone()),
            notes: update.body.clone(),
        }),
        Ok(None) => Ok(UpdateCheckResult {
            available: false,
            version: None,
            notes: None,
        }),
        Err(e) => Err(e.to_string()),
    }
}

/// Read-only Path B migration preview. Walks the current profile set,
/// loads each per-profile [`ProfileConfig`] off disk, the active one as
/// the save apply runs first would write it, and runs the analyzer in
/// [`crate::migration`]. Returns the full plan: every
/// auto-resolved item, every conflict (one entry per name with two or
/// more diverging variants), and the per-source-profile loadouts the
/// migration would generate. Nothing is written to disk; the wizard
/// uses this for the preview pane only. The companion
/// [`migration_apply`] command commits the plan once the user picks
/// winners for any conflicts. Refused while a profile file did not read
/// at launch, while an earlier run is unfinished, while catalog.toml or
/// loadouts.toml is on disk, while profiles/legacy holds copies from
/// an earlier run, or in a session that runs in loadout mode, see
/// [`migration_refusal`].
///
/// [`ProfileConfig`]: crate::profile_config::ProfileConfig
/// [`migration_refusal`]: crate::loadouts::wizard::apply::migration_refusal
#[tauri::command]
pub(crate) async fn migration_analyze(
    app: AppHandle,
    state: State<'_, SharedState>,
    library: Vec<String>,
) -> Result<crate::migration::MigrationPlan, String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let library: Vec<&str> = library.iter().map(String::as_str).collect();
    analyze_migration(&state, &app_data, &library).await
}

/// Commit the Path B migration. Saves the live profile, re-runs the
/// analyzer, applies the user's per-conflict resolutions (or the
/// default version for any missing resolution), copies every
/// existing per-profile file into
/// `profiles/legacy/`, writes `catalog.toml` + `loadouts.toml`, takes the
/// aliases, triggers, and macros out of each profile file, which keeps
/// every other setting, and asks for a relaunch so the startup hook
/// picks up Path B mode. No loadout is on at first, so the group
/// checkboxes in each profile file decide what is on for that profile,
/// at launch and at every switch, the way each profile had it. A write
/// that fails puts back every file the run changed, so you stay in per
/// profile mode and can run it again. A run that stops partway, or
/// cannot put every file back, finishes at the next launch from the
/// journal it saved first. Refused while a profile file did not read at
/// launch, while an earlier run is unfinished, while catalog.toml or
/// loadouts.toml is on disk, while profiles/legacy holds copies from
/// an earlier run, or in a session that runs in loadout mode, see
/// [`migration_refusal`].
///
/// [`migration_refusal`]: crate::loadouts::wizard::apply::migration_refusal
#[tauri::command]
pub(crate) async fn migration_apply(
    app: AppHandle,
    state: State<'_, SharedState>,
    resolutions: Vec<ConflictResolution>,
    library: Vec<String>,
) -> Result<(), String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let library: Vec<&str> = library.iter().map(String::as_str).collect();
    apply_migration(&state, &app_data, &resolutions, &library, || {
        // Path B is now on disk but the live session still holds the
        // pre-migration profile. Block every persist until the relaunch
        // loads the catalog, and flip the input layer into Path B mode
        // so the legacy #profile trio stops writing files.
        MIGRATION_RELAUNCH_PENDING.store(true, std::sync::atomic::Ordering::Release);
        crate::input::PATH_B_ACTIVE.store(true, std::sync::atomic::Ordering::Release);
    })
    .await?;

    // Returning Ok rather than calling `app.restart()` here. Restart
    // is fragile in dev mode: it tears down the binary out from under
    // the `tauri dev` watcher and leaves the next process trying to
    // load a frontend whose Vite dev server may have been killed
    // with the parent, ending in a hidden window with no JS reveal.
    // The frontend shows a "migration complete, please relaunch"
    // banner and offers an explicit [Quit Vosh] button (handled
    // separately by app_quit) that cleanly exits the process. The
    // user re-opens Vosh and the Path B startup hook picks the new
    // catalog up. Path B mode is durable on disk either way. The main
    // window hears the event and says that nothing saves until then, in
    // the terminal and in a toast that stays up.
    announce_migration_applied(&app);
    Ok(())
}

/// Cleanly exit the app. Surfaces a "quit" event first so any window
/// can flush state, then calls `app.exit(0)`. Used by the post-
/// migration prompt to take the user out of the legacy-mode session
/// in one click; on relaunch the Path B startup hook picks up the
/// new catalog.
#[tauri::command]
pub(crate) async fn app_quit(app: AppHandle) -> Result<(), String> {
    // No explicit persist here: `app.exit` raises `RunEvent::ExitRequested`,
    // whose handler asks the windows for their pending writes and then
    // flushes the profile exactly once (with a timeout), see app/exit.rs.
    app.exit(0);
    Ok(())
}

/// Download + install the pending update and restart the app. Errors
/// surface to the frontend; the relaunch is a hard exit so any UI
/// confirmation has to happen before this call returns.
#[tauri::command]
pub(crate) async fn updater_install_and_relaunch(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no update available".to_string())?;
    // Progress callbacks are no-ops at this stage; can be wired to
    // session://event later for a download progress bar.
    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    app.restart();
}

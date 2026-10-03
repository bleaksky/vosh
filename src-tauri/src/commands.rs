//! Tauri commands invoked by the frontend.

use tauri::{AppHandle, Emitter, Manager, State};
use tracing::warn;

use crate::app::events::{broadcast, LOADOUTS_CHANGED, MACRO_GROUPS_CHANGED};
use crate::app::state::{SharedState, MIGRATION_RELAUNCH_PENDING};
use crate::app::windows::{open_aux_window, HELP_WINDOW, SETTINGS_WINDOW};
use crate::disk::save::mark_profile_dirty;
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

/// One loadout as the frontend cares about it: the user-visible
/// identifying fields, the `enabled_groups` list (chips for the picker),
/// and the auto-match block.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct LoadoutSummary {
    pub name: String,
    pub description: Option<String>,
    pub enabled_groups: Vec<String>,
    pub auto_match: Option<crate::profile_set::AutoMatch>,
}

/// Shape returned by [`loadouts_get_state`]. Carries the active list,
/// the full loadout summaries, and a `path_b_active` flag so the
/// frontend can decide whether to render the Loadouts tab at all.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct LoadoutsState {
    pub path_b_active: bool,
    pub active: Vec<String>,
    pub loadouts: Vec<LoadoutSummary>,
}

/// Snapshot the current Path B loadout state for the Settings UI.
/// In legacy mode returns `path_b_active: false` plus empty lists so
/// the frontend can hide the Loadouts tab. In Path B mode the
/// active list and every loadout's summary come from the
/// `state.loadout_set` mutex.
#[tauri::command]
pub(crate) async fn loadouts_get_state(
    state: State<'_, SharedState>,
) -> Result<LoadoutsState, String> {
    let guard = state.loadout_set.lock().await;
    let Some(set) = guard.as_ref() else {
        return Ok(LoadoutsState {
            path_b_active: false,
            active: Vec::new(),
            loadouts: Vec::new(),
        });
    };
    let summaries: Vec<LoadoutSummary> = set
        .loadouts
        .iter()
        .map(|l| LoadoutSummary {
            name: l.name.clone(),
            description: l.description.clone(),
            enabled_groups: l.enabled_groups.clone(),
            auto_match: l.auto_match.clone(),
        })
        .collect();
    Ok(LoadoutsState {
        path_b_active: true,
        active: set.active.clone(),
        loadouts: summaries,
    })
}

/// Replace the active-loadouts list and reapply group state: the
/// union rule while loadouts are active, full dormancy when the user
/// deactivates everything. Persists the loadout set to disk and emits
/// a state-changed event so other windows, such as the Loadouts editor
/// in Settings, see the update.
#[tauri::command]
pub(crate) async fn loadouts_set_active(app: AppHandle, active: Vec<String>) -> Result<(), String> {
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    set_active_loadouts(&app, &app_data, active).await?;
    // The recomputed (or dormant) disabled lists live in the profile
    // snapshot on disk; queue a persist so a crash before the exit
    // flush cannot leave loadouts.toml and per-profile state
    // disagreeing. Also clears any stale persist suppression — this is
    // a durable change the user asked for.
    mark_profile_dirty(&app);
    let _ = app.emit(LOADOUTS_CHANGED, &());
    Ok(())
}

/// The part of [`loadouts_set_active`] that runs under the loadout and
/// profile locks: take the new active list, lay the group state it
/// imposes over the live profile, and save loadouts.toml in `app_data`.
/// The command looks up the app data folder and queues the profile
/// save, so a test can run this against a mock app and a scratch folder.
/// When the switch turned a macro group on or off, every window hears it
/// once the locks are released, since the command line keeps its own map
/// of the macro keys that fire.
pub(crate) async fn set_active_loadouts<R: tauri::Runtime>(
    app: &AppHandle<R>,
    app_data: &std::path::Path,
    active: Vec<String>,
) -> Result<(), String> {
    let state: SharedState = app.state::<SharedState>().inner().clone();
    let macro_groups_changed = {
        let mut guard = state.loadout_set.lock().await;
        let Some(set) = guard.as_mut() else {
            return Err("Path B not active".into());
        };
        // Filter to known loadout names. A stale name (e.g. from a
        // future-truncated payload) is silently dropped rather than
        // returning an error.
        set.active = active
            .into_iter()
            .filter(|n| set.loadouts.iter().any(|l| &l.name == n))
            .collect();
        // Deactivate-all is the documented kill switch ("Activate none
        // to keep the catalog dormant"). Recorded as an explicit flag:
        // an empty active list on its own is ambiguous with "loadouts
        // have no opinion", and the other apply points (startup,
        // profile switch) must be able to re-impose dormancy.
        set.dormant = set.active.is_empty();
        let snapshot = set.clone();
        let mut p = state.profile.lock().await;
        let macro_groups_before = p.disabled_macro_groups.clone();
        crate::loadout_store::apply_effective_state(&snapshot, &mut p);
        if let Err(e) = crate::loadout_store::save_loadout_set(app_data, &snapshot) {
            warn!(error = %e, "loadouts.toml save failed");
        }
        p.disabled_macro_groups != macro_groups_before
    };
    if macro_groups_changed {
        broadcast(app, MACRO_GROUPS_CHANGED, &"");
    }
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

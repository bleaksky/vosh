//! The commands for the shared catalog wizard. The wizard previews the
//! plan that moves your profiles to loadout mode, then applies it once
//! you pick a winner for each conflict.

use tauri::{AppHandle, Manager, State};

use crate::app::state::{SharedState, MIGRATION_RELAUNCH_PENDING};
use crate::loadouts::wizard::apply::{
    analyze_migration, announce_migration_applied, apply_migration, ConflictResolution,
};
use crate::loadouts::wizard::plan::MigrationPlan;

/// Read-only Path B migration preview. Walks the current profile set,
/// loads each per-profile [`ProfileConfig`] off disk, the active one as
/// the save apply runs first would write it, and runs the analyzer in
/// [`crate::loadouts::wizard::plan`]. Returns the full plan: every
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
/// [`ProfileConfig`]: crate::profile::file::ProfileConfig
/// [`migration_refusal`]: crate::loadouts::wizard::apply::migration_refusal
#[tauri::command]
pub(crate) async fn migration_analyze(
    app: AppHandle,
    state: State<'_, SharedState>,
    library: Vec<String>,
) -> Result<MigrationPlan, String> {
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

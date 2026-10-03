//! What the shared catalog wizard does for its two commands. The preview
//! plans the move from the profile files and writes nothing. The apply
//! step saves the journal, then writes catalog.toml, loadouts.toml, and
//! each profile file without its aliases, triggers, and macros.

use std::path::{Path, PathBuf};

use tauri::AppHandle;
use tracing::warn;

use super::journal::{
    drop_wizard_journal, journal_path, save_wizard_journal, JournalFile, WizardJournal,
};
use crate::app::events::{broadcast, MIGRATION_APPLIED};
use crate::app::state::{SharedState, AUTO_PERSIST_SUPPRESSED};
use crate::disk::save::{active_profile_file, persist_state, PERSIST_LOCK};
use crate::loadouts::catalog::catalog_path;
use crate::loadouts::set::loadouts_path;
use crate::profile::file::ProfileConfig;

/// [`migration_analyze`] over the app data folder `app_data`, so a test
/// can run it over a folder of its own. `library` holds the id of every
/// preset in the library the frontend installs from.
///
/// [`migration_analyze`]: crate::ipc::wizard::migration_analyze
pub(crate) async fn analyze_migration(
    state: &SharedState,
    app_data: &std::path::Path,
    library: &[&str],
) -> Result<crate::migration::MigrationPlan, String> {
    analyze_migration_with(state, app_data, library, &AUTO_PERSIST_SUPPRESSED).await
}

/// [`analyze_migration`] with `suppressed` in place of
/// [`AUTO_PERSIST_SUPPRESSED`], so a test can preview after a `#profile
/// reset` without touching the flag every other test reads. Apply saves
/// the live profile before it reads the files, unless `suppressed` holds,
/// see [`apply_migration_with`]. The preview reads the active profile as
/// that save would write it, so it shows what apply builds, the presets of
/// a profile you switched to before anything saved it among them.
pub(crate) async fn analyze_migration_with(
    state: &SharedState,
    app_data: &std::path::Path,
    library: &[&str],
    suppressed: &std::sync::atomic::AtomicBool,
) -> Result<crate::migration::MigrationPlan, String> {
    // No switch or save lands between the read of the live profile and
    // the read of the files, as in apply.
    let _persist_guard = PERSIST_LOCK.lock().await;
    if let Some(reason) = migration_refusal(state, app_data).await {
        return Err(reason.into());
    }
    let scope = state.profile_set.lock().await.as_ref().map(|s| *s.scope());
    let (live, live_presets) = {
        let p = state.profile.lock().await;
        let live = (!suppressed.load(std::sync::atomic::Ordering::Acquire))
            .then(|| active_profile_file(&p, scope.as_ref()));
        (live, p.ui.enabled_presets.clone())
    };
    let set = state.loaded_profile_set().await?;
    let sources = migration_sources(&set, live.as_ref())?;
    Ok(plan_migration(&sources, &live_presets, library))
}

/// The plan for `sources`, with the preset list every character shares
/// in loadout mode and the list each profile has now, so the preview can
/// say who gains or loses a preset. The catalog takes the shared list by
/// the rule launch uses, see
/// [`crate::loadout_store::first_catalog_presets`], so the first launch
/// in loadout mode keeps on every preset any saved profile had on and
/// nothing more. `live_presets` is the live profile's list.
fn plan_migration(
    sources: &MigrationSources,
    live_presets: &[String],
    library: &[&str],
) -> crate::migration::MigrationPlan {
    let mut plan = crate::migration::analyze_profiles(&sources.profiles, library);
    plan.shared_presets =
        crate::loadout_store::first_catalog_presets(&sources.preset_lists, live_presets);
    plan.profile_presets = sources
        .profiles
        .iter()
        .map(|(_, config)| config.ui.enabled_presets.clone())
        .collect();
    plan
}

/// Why the shared catalog wizard may not run, or None when it may. It
/// refuses a session that runs in loadout mode first. Its profile files
/// hold no items, so a run would build a catalog with none, even once
/// catalog.toml has left the folder. The save at quit writes catalog.toml
/// and loadouts.toml back from the session, so the refusal says to quit
/// before you follow the steps for a new catalog in the help. The
/// refusals over what the app data folder holds come after it, see
/// [`folder_refusal`]. Those say what to do with Vosh closed, and in this
/// session the save at quit would undo it, as a catalog written back
/// beside the backups you copied over lays their old items over it.
pub(crate) async fn migration_refusal(
    state: &SharedState,
    app_data: &std::path::Path,
) -> Option<&'static str> {
    if state.global_catalog.lock().await.is_some() {
        return Some(
            "This session runs on a shared catalog, and Vosh saves it to catalog.toml again when \
             you quit, so Vosh will not build another one now. To build a new catalog, quit \
             Vosh first, then follow the steps for a new catalog under Set up loadouts in the \
             help.",
        );
    }
    folder_refusal(app_data)
}

/// The folder the shared catalog wizard copies each profile file into
/// before it changes any.
pub(crate) fn legacy_dir(app_data: &Path) -> PathBuf {
    app_data.join("profiles").join("legacy")
}

/// True when the legacy folder holds a copy of a profile file.
fn legacy_copies_present(app_data: &Path) -> bool {
    std::fs::read_dir(legacy_dir(app_data)).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            entry.path().extension().is_some_and(|ext| ext == "toml")
                && entry.file_type().is_ok_and(|t| t.is_file())
        })
    })
}

/// Why the shared catalog wizard may not write catalog.toml and
/// loadouts.toml, or None when it may. It refuses while the journal of
/// an earlier run is on disk, since that run took the items out of some
/// profile files and the next launch finishes it from the journal, which
/// a new run would write over. The wizard builds the catalog
/// from the profile files, and in loadout mode those hold no aliases or
/// triggers, so it only writes where neither file is on disk yet. A file
/// Vosh could not read at launch stays refused even once it is gone. It
/// also refuses while profiles/legacy holds a copy from an earlier run,
/// the backup of each profile as it was before that run, since a run over
/// files an earlier run took the items out of would copy those over it.
/// The catalog.toml of that run holds the items, with every change since,
/// so that refusal says to put it back to keep them. It says what a
/// backup copied back brings back and drops for a new catalog, since one
/// built from the files without their items would leave every character
/// with nothing. It says never to do both, since launch lays the items of
/// a profile file over the catalog, and the next save shares them.
fn folder_refusal(app_data: &Path) -> Option<&'static str> {
    if journal_path(app_data).exists() {
        return Some(
            "Vosh has not finished an earlier move to loadouts. Quit Vosh and open it again to \
             finish it.",
        );
    }
    let catalog = catalog_path(app_data);
    let loadouts = loadouts_path(app_data);
    if crate::disk::atomic::is_unread(&catalog) || crate::disk::atomic::is_unread(&loadouts) {
        return Some(
            "Vosh could not read your shared catalog at launch, so it will not build a new one \
             over it. Fix catalog.toml or loadouts.toml and restart Vosh.",
        );
    }
    if catalog.exists() {
        return Some(
            "You already have a shared catalog, so Vosh will not build another one over it.",
        );
    }
    if loadouts.exists() {
        return Some(
            "Vosh found loadouts.toml from an earlier shared catalog and will not save over it. \
             Move the file out of the Vosh folder to build a new catalog.",
        );
    }
    if legacy_copies_present(app_data) {
        return Some(
            "Vosh found copies of your profile files in profiles/legacy from an earlier move to \
             loadouts and will not save over them. Each copy is a backup of its profile as it \
             was before that move. Your aliases, triggers, and macros are in the catalog.toml \
             that move wrote, with every change you made since. To keep them, quit Vosh and put \
             catalog.toml and loadouts.toml back in the Vosh folder. To build a new catalog from \
             the backups instead, quit Vosh, copy each backup over its file in the profiles \
             folder, and move the legacy folder out of the profiles folder. A backup brings back \
             every setting of its profile as it was before the move and drops every change you \
             made since. Never do both, since a backup copied back beside catalog.toml lays its \
             old items over the catalog for every character.",
        );
    }
    None
}

/// What the shared catalog wizard reads, see [`migration_sources`].
struct MigrationSources {
    /// Every profile in index order with what its file holds.
    profiles: Vec<(String, ProfileConfig)>,
    /// The enabled preset list of each profile that saved a file.
    preset_lists: Vec<Vec<String>>,
    /// The file of every profile, in index order.
    files: Vec<MigrationFile>,
}

/// The file of one profile, as the wizard read it.
struct MigrationFile {
    name: String,
    path: std::path::PathBuf,
    /// What the file held, or None for a profile that never saved one.
    text: Option<String>,
}

/// Every profile in index order with what its file holds, for the shared
/// catalog wizard, the enabled preset list of each profile that saved a
/// file, and the text of each file. A profile that never saved a file
/// brings what a switch to it loads, [`ProfileConfig::fresh`], and no
/// preset list, the way launch leaves it out.
/// A file that does not read stops the wizard, since the catalog would
/// miss its items. So does a file Vosh could not read at launch, since
/// the wizard rewrites every profile file and Vosh never saves over one
/// of those. `live`, when given, stands for the file of the active
/// profile, as the save apply runs first would write it.
fn migration_sources(
    set: &crate::profile::set::ProfileSet,
    live: Option<&ProfileConfig>,
) -> Result<MigrationSources, String> {
    let mut sources = MigrationSources {
        profiles: Vec::with_capacity(set.list().len()),
        preset_lists: Vec::new(),
        files: Vec::new(),
    };
    for entry in set.list() {
        let path = set.profile_path(&entry.name);
        if crate::disk::atomic::is_unread(&path) {
            return Err(format!(
                "Vosh could not read the {} profile file when it started, so it will not change \
                 the file. Restart Vosh and try again.",
                crate::profile::set::display_name(&entry.name)
            ));
        }
        let text = if let Some(live) = live.filter(|_| entry.name == set.active_name()) {
            Some(live.to_toml().map_err(|e| e.to_string())?)
        } else if path.exists() {
            // The error goes to the log. Its text can hold colons and a
            // path, which a sentence for you leaves out.
            Some(std::fs::read_to_string(&path).map_err(|e| {
                warn!(error = %e, path = %path.display(), "wizard could not read a profile file");
                format!(
                    "Vosh could not read the {} profile file, so it changed nothing. Check that \
                     you can open the file, then try again.",
                    crate::profile::set::display_name(&entry.name)
                )
            })?)
        } else {
            None
        };
        let cfg = match &text {
            Some(text) => {
                let cfg = ProfileConfig::from_toml(text).map_err(|e| e.to_string())?;
                sources.preset_lists.push(cfg.ui.enabled_presets.clone());
                cfg
            }
            None => ProfileConfig::fresh(),
        };
        sources.profiles.push((entry.name.clone(), cfg));
        sources.files.push(MigrationFile {
            name: entry.name.clone(),
            path,
            text,
        });
    }
    Ok(sources)
}

/// One conflict resolution from the wizard. Identifies a single
/// conflicted item (kind + name) and the source profile whose variant
/// should win. A conflict with no resolution in the list keeps the
/// version of its `default_source`, see [`crate::migration::Conflict`].
#[derive(Debug, serde::Deserialize)]
pub(crate) struct ConflictResolution {
    pub kind: crate::migration::ItemKind,
    pub name: String,
    pub source_profile: String,
}

/// Tell every window once that the wizard wrote its files. The settings
/// window runs the wizard and the main window listens, and puts the
/// notice up once for each time it hears the event.
pub(crate) fn announce_migration_applied<R: tauri::Runtime>(app: &AppHandle<R>) {
    broadcast(app, MIGRATION_APPLIED, &());
}

/// [`migration_apply`] over the app data folder `app_data`, so a test
/// can run it over a folder of its own. `library` holds the id of every
/// preset in the library the frontend installs from. `written` runs once catalog.toml,
/// loadouts.toml, and every profile file are on disk. A write that fails
/// puts back every file the run changed and skips `written`, unless a
/// file stays changed. The journal then stays for the next launch to
/// finish the run, and `written` runs.
///
/// [`migration_apply`]: crate::ipc::wizard::migration_apply
pub(crate) async fn apply_migration(
    state: &SharedState,
    app_data: &std::path::Path,
    resolutions: &[ConflictResolution],
    library: &[&str],
    written: impl FnOnce(),
) -> Result<(), String> {
    apply_migration_with(
        state,
        app_data,
        resolutions,
        library,
        &AUTO_PERSIST_SUPPRESSED,
        written,
    )
    .await
}

/// [`apply_migration`] with `suppressed` in place of
/// [`AUTO_PERSIST_SUPPRESSED`], so a test can run it after a `#profile
/// reset` without touching the flag every other test reads.
pub(crate) async fn apply_migration_with(
    state: &SharedState,
    app_data: &std::path::Path,
    resolutions: &[ConflictResolution],
    library: &[&str],
    suppressed: &std::sync::atomic::AtomicBool,
    written: impl FnOnce(),
) -> Result<(), String> {
    // Every save of a profile file takes this lock, so none lands
    // between the read of a file below and its rewrite without the items.
    let _persist_guard = PERSIST_LOCK.lock().await;
    if let Some(reason) = migration_refusal(state, app_data).await {
        return Err(reason.into());
    }

    // The live profile can run two seconds ahead of its file, with a
    // variable a script set or a splitter you dragged, and once this run
    // is done nothing saves it until the relaunch. Write it first, as a
    // switch does, so the wizard reads it. After `#profile reset` or
    // `load` the live profile is deliberately diverged from its file,
    // and the file stands as it is.
    if !suppressed.load(std::sync::atomic::Ordering::Acquire) {
        persist_state(state, Some(app_data)).await;
    }

    // Re-load sources from disk — the analyze call has to walk the
    // same set the user just previewed, but a few seconds may have
    // passed and we want the fresh snapshot rather than caching across
    // commands.
    let sources = {
        let set = state.loaded_profile_set().await?;
        migration_sources(&set, None)?
    };
    let live_presets = state.profile.lock().await.ui.enabled_presets.clone();

    let plan = plan_migration(&sources, &live_presets, library);
    let mut catalog = plan.auto_resolved.clone();
    // The catalog owns which presets are on, and takes the list the
    // preview showed.
    catalog.enabled_presets = Some(plan.shared_presets.clone());
    for conflict in &plan.conflicts {
        let chosen_source = resolutions
            .iter()
            .find(|r| r.kind == conflict.kind && r.name == conflict.name)
            .map_or(conflict.default_source.as_str(), |r| {
                r.source_profile.as_str()
            });
        let chosen = conflict
            .variants
            .iter()
            .find(|v| v.source_profile == chosen_source)
            .ok_or_else(|| {
                format!(
                    "resolution for `{}` points to unknown source `{}`",
                    conflict.name, chosen_source
                )
            })?;
        match &chosen.item {
            crate::migration::ItemPayload::Alias { item } => catalog.aliases.push(item.clone()),
            crate::migration::ItemPayload::Trigger { item } => catalog.triggers.push(item.clone()),
            crate::migration::ItemPayload::Macro { item } => catalog.macros.push(item.clone()),
        }
    }

    // Every loadout starts off. An active loadout imposes its groups on
    // every profile, at launch and at every switch, so the loadout of the
    // profile you use now would turn off the items of every other
    // character you switch to. With none on, the loadouts have no opinion
    // and the group checkboxes each profile file keeps below decide.
    let loadout_set = crate::loadout::LoadoutSet {
        loadouts: plan.loadouts.clone(),
        active: Vec::new(),
        dormant: false,
    };

    // Each profile file stays where it is and keeps every setting of its
    // profile, its timers, variables, tick, panels, theme, and vitals
    // among them, since loadout mode reads them from there at launch and
    // on a switch. Only the aliases, triggers, and macros leave it, as
    // the catalog holds them now. A file that kept them would lay its
    // copies, with their old group names, over the catalog at launch.
    // Its group checkbox lists name the catalog groups of each kind that
    // are off for the profile, and its folder map names the catalog groups
    // each of its folders became, see `migration::profile_file_for_catalog`.
    // A profile that never saved a file gets one when it has lists or a
    // map to keep. The file keeps its own enabled preset list, which loadout
    // mode replaces with the catalog's at every load. Everything is built
    // before the first write, so a file that does not serialize changes
    // nothing.
    let mut kept = Vec::with_capacity(sources.files.len());
    for file in &sources.files {
        let mut config = match &file.text {
            Some(text) => ProfileConfig::from_toml(text).map_err(|e| e.to_string())?,
            None => ProfileConfig::fresh(),
        };
        crate::migration::profile_file_for_catalog(&mut config, &file.name, &plan);
        let lists = !config.disabled_alias_groups.is_empty()
            || !config.disabled_trigger_groups.is_empty()
            || !config.disabled_macro_groups.is_empty()
            || !config.group_folders.is_empty();
        if file.text.is_some() || lists {
            kept.push((file, config.to_toml().map_err(|e| e.to_string())?));
        }
    }

    // Every write the run makes, in a journal saved before the first one,
    // so a run that stops partway finishes at the next launch, see
    // `journal::finish_wizard_run`. Built before any write, so a
    // file that does not serialize changes nothing.
    let journal = WizardJournal {
        catalog: toml::to_string_pretty(&catalog).map_err(|e| e.to_string())?,
        loadouts: toml::to_string_pretty(&loadout_set).map_err(|e| e.to_string())?,
        profiles: kept
            .iter()
            .map(|(file, text)| JournalFile {
                path: file
                    .path
                    .strip_prefix(app_data)
                    .unwrap_or(&file.path)
                    .to_string_lossy()
                    .into_owned(),
                text: text.clone(),
            })
            .collect(),
    };

    // A full copy of each file first, so the files as they were wait in
    // profiles/legacy before anything changes. The wizard refuses to run
    // while profiles/legacy holds a copy from an earlier run, so no copy
    // lands over another, see `folder_refusal`.
    let legacy_dir = legacy_dir(app_data);
    let mut copies = Vec::new();
    for file in &sources.files {
        let Some(text) = &file.text else {
            continue;
        };
        let name = file.path.file_name().unwrap_or_default();
        let copy = legacy_dir.join(name);
        if let Err(e) = crate::disk::atomic::write_with_backup(&copy, text) {
            warn!(error = %e, path = %copy.display(), "wizard could not copy a profile file");
            take_out_copies(&copies);
            return Err(format!(
                "Vosh could not copy {} into profiles/legacy and changed nothing. \
                 {WIZARD_WRITE_NEXT_STEP}",
                name.to_string_lossy()
            ));
        }
        copies.push(copy);
    }

    if let Err(e) = save_wizard_journal(app_data, &journal) {
        warn!(error = %e, "wizard could not save its journal");
        take_out_copies(&copies);
        return Err(format!(
            "Vosh could not save catalog.journal.toml and changed nothing. \
             {WIZARD_WRITE_NEXT_STEP}"
        ));
    }

    // Then the catalog, the loadouts, and each profile file without its
    // items. A write that fails puts back every file this run changed,
    // so Vosh stays in per profile mode, and takes out the journal and
    // the copies in legacy, so the wizard can run again.
    let mut touched = Vec::new();
    if let Err((what, e)) = write_shared_catalog(app_data, &journal, &kept, &mut touched) {
        warn!(error = %e, file = %what, "wizard could not save a file");
        crate::profile::shared::put_back(&touched);
        let restored = touched.iter().all(|(path, before)| match before {
            Some(text) => std::fs::read_to_string(path).ok().as_deref() == Some(text.as_str()),
            None => !path.exists(),
        });
        if restored && drop_wizard_journal(app_data).is_ok() {
            take_out_copies(&copies);
            return Err(format!(
                "Vosh could not save {what}, so it put back every file it changed. Your profiles \
                 work as before, and you can try again."
            ));
        }
        // The journal stays, so the next launch writes every file the run
        // did not, and loadout mode starts over files without their
        // items. Nothing may save or switch the live profile until then.
        written();
        return Err(format!(
            "Vosh could not save {what} and could not put back every file it changed. Quit Vosh \
             and open it again to finish the move to loadouts. A full copy of each profile file \
             waits in profiles/legacy."
        ));
    }
    // Every file holds its text. A journal that stays only writes the same
    // text again at the next launch.
    if let Err(e) = drop_wizard_journal(app_data) {
        warn!(error = %e, "wizard journal could not be taken out");
    }
    written();
    Ok(())
}

/// What to do when the wizard could not write the copies in
/// profiles/legacy or its journal, before it changed anything. The error
/// itself goes to the log, since its text can hold colons and paths.
const WIZARD_WRITE_NEXT_STEP: &str =
    "Check that your disk has room and that Vosh can write to its folder, then try again.";

/// Take out the copies in profiles/legacy a wizard run wrote, when the
/// run changed nothing else in the end, so the wizard can run again. A
/// copy that stays refuses the next run, which says to move it.
fn take_out_copies(copies: &[std::path::PathBuf]) {
    for copy in copies {
        if let Err(e) = std::fs::remove_file(copy) {
            warn!(error = %e, path = %copy.display(), "legacy copy could not be taken out");
        }
    }
}

/// Save every file `journal` names, catalog.toml, then loadouts.toml,
/// then each profile file, the ones in `kept` in the same order. Notes in
/// `touched` each file it is about to write with what the file held
/// before, None for a file that was not there, so a failure can put them
/// back. On a failure, returns the file that did not save, as words for
/// you, and the error.
fn write_shared_catalog(
    app_data: &std::path::Path,
    journal: &WizardJournal,
    kept: &[(&MigrationFile, String)],
    touched: &mut Vec<(std::path::PathBuf, Option<String>)>,
) -> Result<(), (String, String)> {
    for (n, (path, text)) in journal.files(app_data).into_iter().enumerate() {
        #[cfg(test)]
        WIZARD_WRITES_BEFORE_A_CRASH.with(|left| match left.get() {
            Some(0) => panic!("the test stops the wizard here"),
            Some(more) => left.set(Some(more - 1)),
            None => {}
        });
        // The wizard refuses to run while catalog.toml or loadouts.toml
        // is on disk.
        let (what, before) = match n.checked_sub(2) {
            None => (
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                None,
            ),
            Some(i) => {
                let file = kept[i].0;
                (
                    format!(
                        "the {} profile file",
                        crate::profile::set::display_name(&file.name)
                    ),
                    file.text.clone(),
                )
            }
        };
        touched.push((path.clone(), before));
        crate::disk::atomic::write_with_backup(&path, text).map_err(|e| (what, e.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
thread_local! {
    /// How many files a wizard run on this thread writes before the test
    /// stops it where it stands, the way a crash or a force quit would.
    pub(crate) static WIZARD_WRITES_BEFORE_A_CRASH: std::cell::Cell<Option<usize>> =
        const { std::cell::Cell::new(None) };
}

//! Runtime adapter between the [`crate::loadout`] data model and the
//! live runtime stores. Phase B2.
//!
//! ## What this module provides
//!
//! Three concerns sit here together because they are the boundary
//! where Path B's static catalog + loadout types meet the runtime:
//!
//!   1. **Persistence**: `<app_data_dir>/catalog.toml` and
//!      `<app_data_dir>/loadouts.toml` load and save, both routed
//!      through the same atomic backup pipeline that `profile.toml`
//!      and `global.toml` use ([`crate::profile_config::write_with_backup`]).
//!   2. **Mode detection**: [`path_b_mode_active`] returns true iff
//!      `catalog.toml` exists. This is the trigger `AppState` reads
//!      at startup to decide whether to source items from the catalog
//!      (Path B) or from each per-profile file (legacy).
//!   3. **Runtime apply**: [`apply_loadout_state`] takes a
//!      [`LoadoutSet`] plus the live [`AliasStore`] / [`TriggerStore`]
//!      / [`Profile`] handles and writes each store's
//!      `disabled_groups` from the union of every active loadout's
//!      `enabled_groups`. The macro store has no wrapper so the
//!      profile's `disabled_macro_groups` set is updated in place.
//!
//! ## Apply semantics
//!
//! `disabled_groups` is the user's durable Settings-checkbox state in
//! BOTH modes. Loadouts only impose group state when at least one
//! active loadout actually declares `enabled_groups`: then the disabled
//! set becomes every cataloged group NOT in the union of active
//! loadouts' `enabled_groups`. When no active loadout declares any
//! groups, the loadouts have no opinion and the user's checkbox state
//! is left untouched (and persists via the per-profile snapshot).
//! Ungrouped items (whose `group` is `None` or empty) are never
//! disabled because the store already treats them as always-on.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::loadout::{GlobalCatalog, LoadoutSet};
use crate::profile::Profile;
use crate::profile_config::{write_with_backup, ProfileConfig};
use crate::profile_set::ProfileSet;

/// Filename of the global catalog inside the app data directory.
const CATALOG_FILE: &str = "catalog.toml";
/// Filename of the loadout collection inside the app data directory.
const LOADOUTS_FILE: &str = "loadouts.toml";
/// Filename of the journal the shared catalog wizard keeps while it
/// writes, see [`WizardJournal`].
const JOURNAL_FILE: &str = "catalog.journal.toml";

#[derive(Debug, Error)]
pub(crate) enum LoadoutStoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("toml parse error: {0}")]
    Deserialize(#[from] toml::de::Error),
}

/// Path to `catalog.toml` under the given app data directory.
pub(crate) fn catalog_path(app_data: &Path) -> PathBuf {
    app_data.join(CATALOG_FILE)
}

/// Path to `loadouts.toml` under the given app data directory.
pub(crate) fn loadouts_path(app_data: &Path) -> PathBuf {
    app_data.join(LOADOUTS_FILE)
}

/// Path to the shared catalog wizard's journal, see [`WizardJournal`].
pub(crate) fn journal_path(app_data: &Path) -> PathBuf {
    app_data.join(JOURNAL_FILE)
}

/// Every file the shared catalog wizard writes, with its new text. The
/// wizard saves it before its first write and takes it out after its
/// last, so a run that stops in between, from a crash, a force quit, or
/// a failed write it could not undo, leaves it behind. Launch then
/// writes every file it names before anything loads, see
/// [`finish_wizard_run`]. Without it, catalog.toml would start loadout
/// mode over profile files that still hold their items under their old
/// group names, and every character would get every item.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct WizardJournal {
    /// catalog.toml as the wizard writes it.
    pub(crate) catalog: String,
    /// loadouts.toml as the wizard writes it.
    pub(crate) loadouts: String,
    /// Each profile file as the wizard writes it.
    #[serde(default)]
    pub(crate) profiles: Vec<JournalFile>,
}

/// One profile file in a [`WizardJournal`].
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct JournalFile {
    /// The file, relative to the app data folder.
    pub(crate) path: String,
    /// What the file holds once the wizard is done.
    pub(crate) text: String,
}

impl WizardJournal {
    /// Each file in the order the wizard writes them, catalog.toml, then
    /// loadouts.toml, then each profile file, with its text.
    pub(crate) fn files(&self, app_data: &Path) -> Vec<(PathBuf, &str)> {
        let mut files = vec![
            (catalog_path(app_data), self.catalog.as_str()),
            (loadouts_path(app_data), self.loadouts.as_str()),
        ];
        files.extend(
            self.profiles
                .iter()
                .map(|f| (app_data.join(&f.path), f.text.as_str())),
        );
        files
    }
}

/// Save `journal` before the wizard writes anything it names.
pub(crate) fn save_wizard_journal(
    app_data: &Path,
    journal: &WizardJournal,
) -> Result<(), LoadoutStoreError> {
    let text = toml::to_string_pretty(journal)?;
    write_with_backup(&journal_path(app_data), &text)?;
    Ok(())
}

/// Take the journal out once every file it names holds its text, or once
/// a failed run put every file back.
pub(crate) fn drop_wizard_journal(app_data: &Path) -> std::io::Result<()> {
    std::fs::remove_file(journal_path(app_data))
}

/// What launch tells you when it finished a wizard run that stopped.
pub(crate) const WIZARD_FINISHED_NOTICE: &str =
    "Vosh finished the move to loadouts that stopped before it was done. Your aliases, triggers, \
     and macros are in the shared catalog.";

/// What launch tells you when it could not finish a wizard run that
/// stopped.
pub(crate) const WIZARD_UNFINISHED_NOTICE: &str =
    "Vosh could not finish the move to loadouts that stopped before it was done, so it saves \
     nothing until it does. A full copy of each profile file waits in profiles/legacy. Quit Vosh \
     and open it again to try once more.";

/// What launch found of a shared catalog wizard run, see
/// [`finish_wizard_run`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WizardRun {
    /// No run was left, or the one left had written every file and only
    /// its journal was still there.
    Done,
    /// Launch wrote the files a run that stopped had not.
    Finished,
    /// The run is still not done. The journal did not read, a file did
    /// not save, or the journal would not go. Launch then stays out of
    /// loadout mode, since a profile file may still hold its items under
    /// their old group names, and holds every save, since the next launch
    /// writes the journal again over anything this session saved.
    Unfinished,
}

impl WizardRun {
    /// What launch tells you about the run.
    pub(crate) fn notices(self) -> Vec<String> {
        match self {
            Self::Done => Vec::new(),
            Self::Finished => vec![WIZARD_FINISHED_NOTICE.to_string()],
            Self::Unfinished => vec![WIZARD_UNFINISHED_NOTICE.to_string()],
        }
    }
}

/// Finish at launch a shared catalog wizard run that stopped before it
/// took its journal out: write every file the journal names that does
/// not hold its text yet, then take the journal out. Runs before any
/// file loads, so loadout mode never starts over profile files that
/// still hold their items. A journal that does not read, a file that
/// does not save, or a journal that will not go leaves the run
/// unfinished, and the journal stays for the next launch.
pub(crate) fn finish_wizard_run(app_data: &Path) -> WizardRun {
    let path = journal_path(app_data);
    if !path.exists() {
        return WizardRun::Done;
    }
    let journal: WizardJournal = match std::fs::read_to_string(&path)
        .map_err(LoadoutStoreError::from)
        .and_then(|text| Ok(toml::from_str(&text)?))
    {
        Ok(journal) => journal,
        Err(e) => {
            tracing::error!(error = %e, path = %path.display(), "wizard journal unreadable");
            return WizardRun::Unfinished;
        }
    };
    let mut finished = true;
    let mut wrote = false;
    for (file, text) in journal.files(app_data) {
        if std::fs::read_to_string(&file).ok().as_deref() == Some(text) {
            continue;
        }
        match write_with_backup(&file, text) {
            Ok(()) => wrote = true,
            Err(e) => {
                tracing::error!(error = %e, path = %file.display(), "wizard file could not be finished");
                finished = false;
            }
        }
    }
    if !finished {
        return WizardRun::Unfinished;
    }
    // A journal that stays would write its text again at the next launch,
    // over whatever this session saved.
    if let Err(e) = drop_wizard_journal(app_data) {
        tracing::error!(error = %e, path = %path.display(), "wizard journal could not be taken out");
        return WizardRun::Unfinished;
    }
    if !wrote {
        return WizardRun::Done;
    }
    tracing::info!("finished a shared catalog wizard run that stopped");
    WizardRun::Finished
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

/// True iff `catalog.toml` exists at the app data root. The wizard in
/// Phase B3 is what writes that file the first time; until then this
/// returns false and `AppState` stays on the legacy per-profile path.
pub(crate) fn path_b_mode_active(app_data: &Path) -> bool {
    catalog_path(app_data).exists()
}

/// Load the global catalog. Missing file yields an empty catalog
/// rather than an error so first-launch and Path-A-only installs do
/// not have to special-case absent state.
pub(crate) fn load_global_catalog(app_data: &Path) -> Result<GlobalCatalog, LoadoutStoreError> {
    let path = catalog_path(app_data);
    if !path.exists() {
        return Ok(GlobalCatalog::default());
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(toml::from_str(&text)?)
}

/// Persist the global catalog atomically with a rolling backup. See
/// [`crate::profile_config::write_with_backup`] for the rename and
/// retention guarantees.
pub(crate) fn save_global_catalog(
    app_data: &Path,
    catalog: &GlobalCatalog,
) -> Result<(), LoadoutStoreError> {
    let text = toml::to_string_pretty(catalog)?;
    write_with_backup(&catalog_path(app_data), &text)?;
    Ok(())
}

/// Load the loadout collection. Missing file yields an empty
/// `LoadoutSet` (no loadouts, no active stack) so callers can treat
/// "fresh install" and "user wiped their loadouts" identically.
pub(crate) fn load_loadout_set(app_data: &Path) -> Result<LoadoutSet, LoadoutStoreError> {
    let path = loadouts_path(app_data);
    if !path.exists() {
        return Ok(LoadoutSet::default());
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(toml::from_str(&text)?)
}

/// Persist the loadout collection atomically with a rolling backup.
pub(crate) fn save_loadout_set(app_data: &Path, set: &LoadoutSet) -> Result<(), LoadoutStoreError> {
    let text = toml::to_string_pretty(set)?;
    write_with_backup(&loadouts_path(app_data), &text)?;
    Ok(())
}

/// What Vosh tells you at launch when catalog.toml does not read.
pub(crate) const UNREAD_CATALOG_NOTICE: &str =
    "Vosh could not read catalog.toml, which holds your shared aliases, triggers, and macros, so \
     they are off and Vosh will not save over it. Fix the file and restart Vosh.";

/// What Vosh tells you at launch when loadouts.toml does not read.
pub(crate) const UNREAD_LOADOUTS_NOTICE: &str =
    "Vosh could not read loadouts.toml, so your shared aliases, triggers, and macros are off and \
     Vosh will not save over it or catalog.toml. Fix the file and restart Vosh.";

/// Read catalog.toml and loadouts.toml at launch in loadout mode. When
/// either one does not read, the session runs on the profile files alone,
/// so a save from it would write a catalog without your shared items.
/// Vosh then holds both files with [`crate::profile_config::hold_unread`],
/// since the pair only makes sense together, and the error carries the
/// sentences that tell you so.
pub(crate) fn load_path_b_at_launch(
    app_data: &Path,
) -> Result<(GlobalCatalog, LoadoutSet), Vec<String>> {
    let catalog = load_global_catalog(app_data);
    let set = load_loadout_set(app_data);
    let mut notices = Vec::new();
    if let Err(e) = &catalog {
        tracing::error!(
            error = %e,
            path = %catalog_path(app_data).display(),
            "catalog.toml unreadable at startup; it will not be saved over",
        );
        notices.push(UNREAD_CATALOG_NOTICE.to_string());
    }
    if let Err(e) = &set {
        tracing::error!(
            error = %e,
            path = %loadouts_path(app_data).display(),
            "loadouts.toml unreadable at startup; it will not be saved over",
        );
        notices.push(UNREAD_LOADOUTS_NOTICE.to_string());
    }
    match (catalog, set) {
        (Ok(catalog), Ok(set)) => Ok((catalog, set)),
        _ => {
            crate::profile_config::hold_unread(&catalog_path(app_data));
            crate::profile_config::hold_unread(&loadouts_path(app_data));
            Err(notices)
        }
    }
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
pub(crate) fn migration_refusal(app_data: &Path) -> Option<&'static str> {
    if journal_path(app_data).exists() {
        return Some(
            "Vosh has not finished an earlier move to loadouts. Quit Vosh and open it again to \
             finish it.",
        );
    }
    let catalog = catalog_path(app_data);
    let loadouts = loadouts_path(app_data);
    if crate::profile_config::is_unread(&catalog) || crate::profile_config::is_unread(&loadouts) {
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

/// What `ui.enabled_presets` holds when you turned every preset off. An
/// empty list means the defaults. Mirrors `PRESETS_OFF_MARKER` in
/// src/lib/automationRecords.ts, and a test here reads that line.
pub(crate) const PRESETS_OFF: &str = "none";

/// The enabled preset lists of the profile files, for a catalog that
/// takes the list for the first time. See [`profile_preset_lists`].
#[derive(Debug, Default)]
pub(crate) struct ProfilePresetLists {
    /// The list of every profile file that reads, in index order. A
    /// profile that never saved a file holds no list and is left out.
    pub(crate) lists: Vec<Vec<String>>,
    /// The profiles whose file does not read, in index order. What their
    /// lists hold is unknown.
    pub(crate) unread: Vec<String>,
}

impl ProfilePresetLists {
    /// The lists the catalog takes, or None while it waits. A file that
    /// does not read is left out, so every preset a character whose file
    /// reads had on stays on. With no file that reads but one that does
    /// not, there is nothing to take, and the catalog waits rather than
    /// fall back to the live profile, which holds the defaults when its
    /// own file is the one that does not read.
    fn usable(&self) -> Option<&[Vec<String>]> {
        if self.lists.is_empty() && !self.unread.is_empty() {
            None
        } else {
            Some(&self.lists)
        }
    }

    /// What launch tells you about each profile file that did not read.
    /// `adopted` is true when the catalog took its list from the files
    /// that did.
    pub(crate) fn unread_notices(&self, adopted: bool) -> Vec<String> {
        self.unread
            .iter()
            .map(|name| {
                let name = crate::profile_set::display_name(name);
                if adopted {
                    format!(
                        "Vosh could not read the {name} profile file and left its presets out of \
                         the shared list. Turn on any you miss under Presets in Automation \
                         settings."
                    )
                } else {
                    format!(
                        "Vosh could not read the {name} profile file and will build the shared \
                         preset list once the file reads."
                    )
                }
            })
            .collect()
    }
}

/// The enabled preset list of every profile file in `set`, for a catalog
/// that takes the list for the first time. A file that does not read is
/// named in `unread` and never written.
pub(crate) fn profile_preset_lists(set: &ProfileSet) -> ProfilePresetLists {
    let mut found = ProfilePresetLists::default();
    for entry in set.list() {
        let path = set.profile_path(&entry.name);
        if !path.exists() {
            continue;
        }
        match ProfileConfig::load(&path) {
            Ok(config) => found.lists.push(config.ui.enabled_presets),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    path = %path.display(),
                    "profile file unreadable; its enabled presets are unknown",
                );
                found.unread.push(entry.name.clone());
            }
        }
    }
    found
}

/// Every preset that is on in any of `lists`, in the `enabled_presets`
/// shape. An empty list means the defaults, and every preset in the
/// library is on by default (a test in this module checks
/// src/lib/presets.ts), so the defaults hold every preset a list can
/// name and the union is the defaults. A list that turned every preset
/// off adds none.
fn presets_on_in_any(lists: &[Vec<String>]) -> Vec<String> {
    if lists.iter().any(Vec::is_empty) {
        return Vec::new();
    }
    let on: BTreeSet<&str> = lists
        .iter()
        .flatten()
        .map(String::as_str)
        .filter(|id| *id != PRESETS_OFF)
        .collect();
    if on.is_empty() {
        return vec![PRESETS_OFF.to_string()];
    }
    on.into_iter().map(str::to_string).collect()
}

/// The enabled preset list a catalog takes the first time, from `lists`,
/// the lists of the profile files that hold one. Every preset that any of
/// them had on stays on. With no list at all it takes `live`, the live
/// profile's list. Launch and the shared catalog wizard both use it, so a
/// catalog starts from the same list either way.
pub(crate) fn first_catalog_presets(lists: &[Vec<String>], live: &[String]) -> Vec<String> {
    if lists.is_empty() {
        live.to_vec()
    } else {
        presets_on_in_any(lists)
    }
}

/// Make the catalog own the list of trigger presets that are on. The
/// preset triggers live in the catalog, which every profile shares, so
/// a list kept per profile let a launch as another character put back a
/// preset you had turned off. A catalog written before the list moved
/// here has none, so it takes every preset that any profile file had on,
/// once, from `lists` (see [`profile_preset_lists`]). The launch that
/// follows removes every preset that is off, so a list from one profile
/// alone would take away presets another character used. With no profile
/// file at all it takes the live profile's list. A profile file that does
/// not read is left out (see [`ProfilePresetLists::usable`]). When there
/// is nothing to take (`lists` is None, or no file reads) the catalog
/// waits for a later launch, and the live profile keeps its own list
/// meanwhile. Otherwise the live profile then holds the catalog's list.
/// The profile files keep their own lists as they are. Returns true when
/// the catalog took a list and needs saving.
pub(crate) fn adopt_catalog_presets(
    catalog: &mut GlobalCatalog,
    profile: &mut Profile,
    lists: Option<&ProfilePresetLists>,
) -> bool {
    if let Some(list) = &catalog.enabled_presets {
        profile.ui.enabled_presets.clone_from(list);
        return false;
    }
    let Some(lists) = lists.and_then(ProfilePresetLists::usable) else {
        return false;
    };
    let adopted = first_catalog_presets(lists, &profile.ui.enabled_presets);
    profile.ui.enabled_presets.clone_from(&adopted);
    catalog.enabled_presets = Some(adopted);
    true
}

/// Apply whatever group state the loadout set actually calls for:
/// explicit dormancy wins, otherwise the union rules run (including
/// the no-opinion guard). Every apply point (startup, profile switch,
/// active-list change) routes through here so the deactivate-all kill
/// switch cannot be undone by a later rebuild.
pub(crate) fn apply_effective_state(set: &LoadoutSet, profile: &mut Profile) {
    if set.dormant {
        apply_dormant_state(profile);
    } else {
        apply_loadout_state(set, profile);
    }
}

/// Disable every group in every store: the deactivate-all "keep the
/// catalog dormant" kill switch. Persisted like any checkbox state, so
/// dormancy survives restart (and the no-opinion guard in
/// `apply_loadout_state` will not undo it).
pub(crate) fn apply_dormant_state(profile: &mut Profile) {
    let alias_groups: Vec<String> = profile
        .aliases
        .groups()
        .into_iter()
        .map(|(name, _)| name)
        .filter(|n| !n.is_empty())
        .collect();
    profile.aliases.set_disabled_groups(alias_groups);
    let trigger_groups: Vec<String> = profile
        .triggers
        .groups()
        .into_iter()
        .map(|(name, _)| name)
        .filter(|n| !n.is_empty())
        .collect();
    profile.triggers.set_disabled_groups(trigger_groups);
    profile.disabled_macro_groups = profile
        .macros
        .iter()
        .filter_map(|m| m.group.clone())
        .filter(|g| !g.is_empty())
        .collect();
}

/// Write the per-store `disabled_groups` on a live `Profile` from the
/// active loadouts' union.
///
/// For each store the rule is: gather every group name that appears
/// on at least one item; the disabled set is that universe minus the
/// [`LoadoutSet::effective_enabled_groups`] output. Ungrouped items
/// stay live because the stores already treat empty / `None` groups
/// as always-on.
///
/// This is idempotent and does not touch the items themselves, only
/// the per-store bookkeeping the runtime gates on.
pub(crate) fn apply_loadout_state(set: &LoadoutSet, profile: &mut Profile) {
    let enabled: HashSet<String> = set.effective_enabled_groups().into_iter().collect();

    // No active loadout declares any enabled_groups: the loadouts have no
    // opinion about groups, so leave the user's Settings checkbox state
    // alone. The old behavior treated the empty union as "disable every
    // group", which force-disabled everything at startup for users whose
    // loadouts do not manage groups at all — and made the Settings group
    // checkboxes impossible to persist.
    if enabled.is_empty() {
        return;
    }

    // Aliases. `AliasStore::groups()` returns (name, enabled), we
    // only need the names to build the universe.
    let alias_groups: HashSet<String> = profile
        .aliases
        .groups()
        .into_iter()
        .map(|(name, _)| name)
        .filter(|n| !n.is_empty())
        .collect();
    let alias_disabled: Vec<String> = alias_groups.difference(&enabled).cloned().collect();
    profile.aliases.set_disabled_groups(alias_disabled);

    // Triggers, same shape.
    let trigger_groups: HashSet<String> = profile
        .triggers
        .groups()
        .into_iter()
        .map(|(name, _)| name)
        .filter(|n| !n.is_empty())
        .collect();
    let trigger_disabled: Vec<String> = trigger_groups.difference(&enabled).cloned().collect();
    profile.triggers.set_disabled_groups(trigger_disabled);

    // Macros. No wrapper store, the profile owns the set directly.
    let macro_groups: HashSet<String> = profile
        .macros
        .iter()
        .filter_map(|m| m.group.as_ref())
        .filter(|g| !g.is_empty())
        .cloned()
        .collect();
    profile.disabled_macro_groups = macro_groups
        .difference(&enabled)
        .cloned()
        .collect::<BTreeSet<String>>();
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use vosh_alias::Alias;
    use vosh_trigger::{Trigger, TriggerAction, TriggerPattern, TriggerTarget};

    use crate::loadout::Loadout;
    use crate::profile::Macro;

    fn make_trigger(name: &str, pattern: &str, group: Option<&str>) -> Trigger {
        Trigger {
            name: name.to_string(),
            patterns: vec![TriggerPattern {
                pattern: pattern.to_string(),
                enabled: true,
            }],
            priority: 0,
            enabled: true,
            actions: vec![TriggerAction::Send {
                template: "noop".to_string(),
            }],
            preset: None,
            group: group.map(String::from),
            target: TriggerTarget::Line,
        }
    }

    fn alias_with_group(name: &str, expansion: &str, group: Option<&str>) -> Alias {
        let mut a = Alias::new(name, expansion);
        a.group = group.map(String::from);
        a
    }

    fn tmpdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "vosh-loadout-store-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn path_b_mode_active_false_when_catalog_missing() {
        let dir = tmpdir();
        assert!(!path_b_mode_active(&dir));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn path_b_mode_active_true_after_save() {
        let dir = tmpdir();
        save_global_catalog(&dir, &GlobalCatalog::default()).unwrap();
        assert!(path_b_mode_active(&dir));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_path_b_file_that_does_not_read_holds_both_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut catalog = GlobalCatalog::default();
        catalog.aliases.push(Alias::new("kk", "kick %1"));
        save_global_catalog(dir.path(), &catalog).unwrap();
        fs::write(loadouts_path(dir.path()), "active = = [\n").unwrap();
        let catalog_text = fs::read_to_string(catalog_path(dir.path())).unwrap();

        assert_eq!(
            load_path_b_at_launch(dir.path()).unwrap_err(),
            [UNREAD_LOADOUTS_NOTICE]
        );
        // The session runs without your shared items, so a save from it
        // would write an empty catalog over them.
        assert!(save_global_catalog(dir.path(), &GlobalCatalog::default()).is_err());
        assert!(save_loadout_set(dir.path(), &LoadoutSet::default()).is_err());
        assert_eq!(
            fs::read_to_string(catalog_path(dir.path())).unwrap(),
            catalog_text
        );
        assert_eq!(
            fs::read_to_string(loadouts_path(dir.path())).unwrap(),
            "active = = [\n"
        );
    }

    #[test]
    fn a_catalog_that_does_not_read_is_named_in_the_notice() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(catalog_path(dir.path()), "aliases = = [\n").unwrap();
        assert_eq!(
            load_path_b_at_launch(dir.path()).unwrap_err(),
            [UNREAD_CATALOG_NOTICE]
        );
        assert!(save_global_catalog(dir.path(), &GlobalCatalog::default()).is_err());
        // Nothing wrote loadouts.toml, and nothing can while the catalog
        // is held.
        assert!(save_loadout_set(dir.path(), &LoadoutSet::default()).is_err());
        assert!(!loadouts_path(dir.path()).exists());
    }

    #[test]
    fn load_missing_files_yields_defaults() {
        let dir = tmpdir();
        let catalog = load_global_catalog(&dir).unwrap();
        let set = load_loadout_set(&dir).unwrap();
        let leftover = &catalog.aliases;
        assert!(leftover.is_empty(), "{leftover:?}");
        let leftover = &catalog.triggers;
        assert!(leftover.is_empty(), "{leftover:?}");
        let leftover = &catalog.macros;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(set.loadouts.is_empty());
        let leftover = &set.active;
        assert!(leftover.is_empty(), "{leftover:?}");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn loadout_set_round_trips_through_disk() {
        let dir = tmpdir();
        let mut set = LoadoutSet::default();
        let mut warrior = Loadout::empty("warrior");
        warrior.enabled_groups = vec!["combat-melee".into(), "wartools".into()];
        set.loadouts = vec![warrior];
        set.active = vec!["warrior".into()];

        save_loadout_set(&dir, &set).unwrap();
        let loaded = load_loadout_set(&dir).unwrap();
        assert_eq!(loaded.active, vec!["warrior".to_string()]);
        assert_eq!(loaded.loadouts.len(), 1);
        assert_eq!(loaded.loadouts[0].name, "warrior");
        assert_eq!(
            loaded.loadouts[0].enabled_groups,
            vec!["combat-melee", "wartools"]
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn catalog_round_trips_through_disk() {
        let dir = tmpdir();
        let mut catalog = GlobalCatalog::default();
        let mut alias = Alias::new("kk", "kick %1");
        alias.group = Some("combat".into());
        catalog.aliases.push(alias);
        save_global_catalog(&dir, &catalog).unwrap();
        let loaded = load_global_catalog(&dir).unwrap();
        assert_eq!(loaded.aliases.len(), 1);
        assert_eq!(loaded.aliases[0].name, "kk");
        assert_eq!(loaded.aliases[0].group.as_deref(), Some("combat"));
        fs::remove_dir_all(&dir).ok();
    }

    fn profile_with_items(
        aliases: Vec<Alias>,
        triggers: Vec<Trigger>,
        macros: Vec<Macro>,
    ) -> Profile {
        let mut p = Profile {
            macros,
            ..Profile::default()
        };
        for a in aliases {
            p.aliases.set(a);
        }
        for t in triggers {
            p.triggers.set(t).unwrap();
        }
        p
    }

    #[test]
    fn apply_with_no_declared_groups_leaves_state_untouched() {
        // When nothing is active (or no active loadout declares
        // enabled_groups), the loadouts have no opinion: nothing gets
        // disabled and the user's checkbox state stands. The old
        // semantics disabled every group here, which force-disabled
        // everything at startup for users whose loadouts do not manage
        // groups.
        let mut profile = profile_with_items(
            vec![alias_with_group("kk", "kick %1", Some("combat"))],
            vec![make_trigger("dot", r"burning", Some("buffs"))],
            vec![Macro {
                key: "F1".into(),
                command: "north".into(),
                group: Some("movement".into()),
                enabled: true,
            }],
        );

        let set = LoadoutSet::default();
        apply_loadout_state(&set, &mut profile);

        let leftover = &profile.aliases.disabled_groups();
        assert!(leftover.is_empty(), "{leftover:?}");
        let leftover = &profile.triggers.disabled_groups();
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(profile.disabled_macro_groups.is_empty());
    }

    #[test]
    fn apply_with_one_active_loadout_enables_its_groups_only() {
        let mut profile = profile_with_items(
            vec![
                alias_with_group("kk", "kick %1", Some("combat")),
                alias_with_group("smelt", "smelt iron ore", Some("crafting")),
            ],
            vec![make_trigger("dot", r"burning", Some("buffs"))],
            vec![Macro {
                key: "F1".into(),
                command: "north".into(),
                group: Some("movement".into()),
                enabled: true,
            }],
        );

        let mut set = LoadoutSet::default();
        let mut warrior = Loadout::empty("warrior");
        warrior.enabled_groups = vec!["combat".into(), "movement".into()];
        set.loadouts = vec![warrior];
        set.active = vec!["warrior".into()];

        apply_loadout_state(&set, &mut profile);

        let alias_disabled = profile.aliases.disabled_groups();
        assert!(!alias_disabled.contains(&"combat".to_string()));
        assert!(alias_disabled.contains(&"crafting".to_string()));
        assert!(profile
            .triggers
            .disabled_groups()
            .contains(&"buffs".to_string()));
        assert!(!profile.disabled_macro_groups.contains("movement"));
    }

    #[test]
    fn apply_unions_multiple_active_loadouts() {
        let mut profile = profile_with_items(
            vec![
                alias_with_group("kk", "kick %1", Some("combat")),
                alias_with_group("smelt", "smelt iron ore", Some("crafting")),
                alias_with_group("hb", "say hello there", Some("social")),
            ],
            vec![],
            vec![],
        );

        let mut warrior = Loadout::empty("warrior");
        warrior.enabled_groups = vec!["combat".into()];
        let mut crafter = Loadout::empty("crafter");
        crafter.enabled_groups = vec!["crafting".into()];
        let set = LoadoutSet {
            dormant: false,
            loadouts: vec![warrior, crafter],
            active: vec!["warrior".into(), "crafter".into()],
        };

        apply_loadout_state(&set, &mut profile);

        let disabled = profile.aliases.disabled_groups();
        assert!(!disabled.contains(&"combat".to_string()));
        assert!(!disabled.contains(&"crafting".to_string()));
        assert!(disabled.contains(&"social".to_string()));
    }

    #[test]
    fn apply_leaves_checkbox_state_alone_when_no_loadout_declares_groups() {
        // Loadouts with empty enabled_groups have no opinion about
        // groups: the user's Settings checkbox state must survive both
        // startup and loadout activation. The old semantics treated the
        // empty union as "disable every group", which made group
        // checkboxes impossible to persist for users whose loadouts do
        // not manage groups.
        let mut profile = profile_with_items(
            vec![
                alias_with_group("kk", "kick %1", Some("combat")),
                alias_with_group("hh", "heal %1", Some("heals")),
            ],
            vec![],
            vec![],
        );
        profile
            .aliases
            .set_disabled_groups(vec!["combat".to_string()]);
        let set = LoadoutSet {
            dormant: false,
            loadouts: vec![Loadout::empty("default"), Loadout::empty("Healer")],
            active: vec!["default".into(), "Healer".into()],
        };
        apply_loadout_state(&set, &mut profile);
        assert_eq!(
            profile.aliases.disabled_groups(),
            vec!["combat".to_string()]
        );
    }

    #[test]
    fn apply_still_authoritative_when_a_loadout_declares_groups() {
        // A loadout that DOES declare enabled_groups keeps the original
        // semantics: the disabled set becomes the universe minus the
        // union, overriding checkbox state.
        let mut profile = profile_with_items(
            vec![
                alias_with_group("kk", "kick %1", Some("combat")),
                alias_with_group("hh", "heal %1", Some("heals")),
            ],
            vec![],
            vec![],
        );
        profile
            .aliases
            .set_disabled_groups(vec!["heals".to_string()]);
        let mut warrior = Loadout::empty("warrior");
        warrior.enabled_groups = vec!["heals".into()];
        let set = LoadoutSet {
            dormant: false,
            loadouts: vec![warrior],
            active: vec!["warrior".into()],
        };
        apply_loadout_state(&set, &mut profile);
        assert_eq!(
            profile.aliases.disabled_groups(),
            vec!["combat".to_string()]
        );
    }

    #[test]
    fn dormant_state_disables_every_group_in_every_store() {
        // Deactivating the last loadout is an explicit "make the catalog
        // dormant" request. apply_loadout_state's no-opinion guard would
        // leave everything running, so loadouts_set_active calls this
        // instead: disable the full group universe across all three
        // stores.
        let mut profile = profile_with_items(
            vec![
                alias_with_group("kk", "kick %1", Some("combat")),
                alias_with_group("ungrouped", "look", None),
            ],
            vec![make_trigger("dot", r"burning", Some("buffs"))],
            vec![Macro {
                key: "F1".into(),
                command: "north".into(),
                group: Some("movement".into()),
                enabled: true,
            }],
        );

        apply_dormant_state(&mut profile);

        assert_eq!(
            profile.aliases.disabled_groups(),
            vec!["combat".to_string()]
        );
        assert_eq!(
            profile.triggers.disabled_groups(),
            vec!["buffs".to_string()]
        );
        assert!(profile.disabled_macro_groups.contains("movement"));
    }

    #[test]
    fn effective_state_honors_dormant_over_empty_active() {
        // dormant=true with active=[] must disable everything even
        // though apply_loadout_state alone treats the empty union as
        // no-opinion. This is what re-imposes the kill switch at
        // startup and across profile switches.
        let mut profile = profile_with_items(
            vec![alias_with_group("kk", "kick %1", Some("combat"))],
            vec![],
            vec![],
        );
        let set = LoadoutSet {
            dormant: true,
            ..Default::default()
        };
        apply_effective_state(&set, &mut profile);
        assert_eq!(
            profile.aliases.disabled_groups(),
            vec!["combat".to_string()]
        );
    }

    #[test]
    fn dormant_flag_round_trips_through_disk() {
        let dir = tmpdir();
        let set = LoadoutSet {
            dormant: true,
            ..Default::default()
        };
        save_loadout_set(&dir, &set).unwrap();
        assert!(load_loadout_set(&dir).unwrap().dormant);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn apply_is_idempotent() {
        // Running apply twice in a row produces identical state. Catches
        // accidental accumulation bugs where the function appended to
        // disabled_groups instead of replacing it.
        let mut profile = profile_with_items(
            vec![alias_with_group("kk", "kick %1", Some("combat"))],
            vec![],
            vec![],
        );
        let mut warrior = Loadout::empty("warrior");
        warrior.enabled_groups = vec!["combat".into()];
        let set = LoadoutSet {
            dormant: false,
            loadouts: vec![warrior],
            active: vec!["warrior".into()],
        };

        apply_loadout_state(&set, &mut profile);
        let first = profile.aliases.disabled_groups();
        apply_loadout_state(&set, &mut profile);
        let second = profile.aliases.disabled_groups();
        assert_eq!(first, second);
    }

    #[test]
    fn apply_ignores_ungrouped_items() {
        // Items with no group must never appear in disabled_groups (the
        // stores treat empty group as always-on independently, but the
        // apply function should not surface "" into the set either).
        let mut profile = profile_with_items(
            vec![Alias::new("loose", "look")], // group: None
            vec![],
            vec![],
        );
        let set = LoadoutSet::default();
        apply_loadout_state(&set, &mut profile);
        let leftover = &profile.aliases.disabled_groups();
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    fn presets(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    /// Startup in loadout mode over the profile files in `dir` the way
    /// lib.rs runs it: load the active profile, then let the catalog take
    /// or hand out the preset list, and save the catalog when it took
    /// one. Hands back the notices launch keeps for you too.
    fn launch_with_notices(dir: &Path, set: &ProfileSet) -> (Profile, Vec<String>) {
        let mut profile = Profile::default();
        let mut notices = crate::profile_config::load_at_launch(set, &mut profile);
        let mut catalog = load_global_catalog(dir).unwrap();
        let lists = catalog
            .enabled_presets
            .is_none()
            .then(|| profile_preset_lists(set));
        let adopted = adopt_catalog_presets(&mut catalog, &mut profile, lists.as_ref());
        if adopted {
            save_global_catalog(dir, &catalog).unwrap();
        }
        if let Some(lists) = &lists {
            notices.extend(lists.unread_notices(adopted));
        }
        (profile, notices)
    }

    fn launch(dir: &Path, set: &ProfileSet) -> Profile {
        launch_with_notices(dir, set).0
    }

    /// Save `name`'s file with `list` as its enabled presets.
    fn write_presets(set: &ProfileSet, name: &str, list: &[&str]) {
        let mut config = ProfileConfig::default();
        config.ui.enabled_presets = presets(list);
        config.save(&set.profile_path(name)).unwrap();
    }

    /// Each profile file holds its own list from before the move. Ilsabet
    /// (default) turned the potion labels off. Healer never did.
    fn two_profiles(dir: &Path) -> ProfileSet {
        let set = crate::profile_set::tests::james_like_set(dir);
        write_presets(
            &set,
            crate::profile_set::DEFAULT_PROFILE_NAME,
            &["healing_basics"],
        );
        write_presets(&set, "Healer", &["healing_basics", "potion_labels"]);
        // A catalog saved before the list moved into it.
        save_global_catalog(dir, &GlobalCatalog::default()).unwrap();
        set
    }

    #[test]
    fn the_first_launch_keeps_every_preset_any_character_had_on() {
        let dir = tempfile::tempdir().unwrap();
        let set = two_profiles(dir.path());

        // A launch as Ilsabet takes Healer's potion labels too, so the
        // launch plan does not take them away from Healer.
        let ilsabet = launch(dir.path(), &set);
        let both = presets(&["healing_basics", "potion_labels"]);
        assert_eq!(ilsabet.ui.enabled_presets, both);
        assert_eq!(
            load_global_catalog(dir.path()).unwrap().enabled_presets,
            Some(both)
        );
        // The profile files keep their own lists.
        let file = ProfileConfig::load(&set.active_path()).unwrap();
        assert_eq!(file.ui.enabled_presets, presets(&["healing_basics"]));
    }

    #[test]
    fn a_launch_as_another_character_keeps_the_presets_you_turned_off() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = two_profiles(dir.path());
        let _ilsabet = launch(dir.path(), &set);
        // After the move you turn the potion labels off for everyone.
        save_global_catalog(
            dir.path(),
            &GlobalCatalog {
                enabled_presets: Some(presets(&["healing_basics"])),
                ..GlobalCatalog::default()
            },
        )
        .unwrap();

        // A launch as Healer keeps the catalog's list. Before, Healer's
        // own list turned the potion labels back on for everyone.
        set.switch("Healer").unwrap();
        let healer = launch(dir.path(), &set);
        assert_eq!(healer.ui.enabled_presets, presets(&["healing_basics"]));
        assert_eq!(
            load_global_catalog(dir.path()).unwrap().enabled_presets,
            Some(presets(&["healing_basics"]))
        );
    }

    #[test]
    fn a_profile_on_the_defaults_keeps_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let set = two_profiles(dir.path());
        // Healer never changed a preset, so its list means the defaults.
        write_presets(&set, "Healer", &[]);
        // Test-Prompt turned every preset off.
        write_presets(&set, "Test-Prompt", &["none"]);
        let ilsabet = launch(dir.path(), &set);
        let leftover = &ilsabet.ui.enabled_presets;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(
            load_global_catalog(dir.path()).unwrap().enabled_presets,
            Some(Vec::new())
        );
    }

    #[test]
    fn profiles_that_turned_every_preset_off_keep_them_off() {
        assert_eq!(
            presets_on_in_any(&[presets(&["none"]), presets(&["none"])]),
            presets(&["none"])
        );
        assert_eq!(
            presets_on_in_any(&[presets(&["none"]), presets(&["herb_labels"])]),
            presets(&["herb_labels"])
        );
    }

    #[test]
    fn a_profile_file_that_does_not_read_is_left_out_of_the_shared_list() {
        let dir = tempfile::tempdir().unwrap();
        let set = two_profiles(dir.path());
        write_presets(&set, "Test-Prompt", &["healing_basics", "herb_labels"]);
        // Healer is not the profile you launch as, so no other notice
        // tells you its file does not read.
        std::fs::write(set.profile_path("Healer"), "presets = = [\n").unwrap();

        let (ilsabet, notices) = launch_with_notices(dir.path(), &set);
        // The catalog takes every preset a character whose file reads had
        // on, so Test-Prompt keeps its herb labels.
        let on = presets(&["healing_basics", "herb_labels"]);
        assert_eq!(ilsabet.ui.enabled_presets, on);
        assert_eq!(
            load_global_catalog(dir.path()).unwrap().enabled_presets,
            Some(on)
        );
        assert_eq!(
            notices,
            [
                "Vosh could not read the Healer profile file and left its presets out of the \
                 shared list. Turn on any you miss under Presets in Automation settings."
            ]
        );
        // The file that does not read stays as it is.
        assert_eq!(
            std::fs::read_to_string(set.profile_path("Healer")).unwrap(),
            "presets = = [\n"
        );
    }

    #[test]
    fn the_catalog_waits_while_no_profile_file_reads() {
        let dir = tempfile::tempdir().unwrap();
        let set = crate::profile_set::tests::james_like_set(dir.path());
        save_global_catalog(dir.path(), &GlobalCatalog::default()).unwrap();
        // The only saved file is the one you launch as, and it does not
        // read, so there is no list to take.
        std::fs::write(set.active_path(), "presets = = [\n").unwrap();

        let (ilsabet, notices) = launch_with_notices(dir.path(), &set);
        let leftover = &ilsabet.ui.enabled_presets;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(
            load_global_catalog(dir.path()).unwrap().enabled_presets,
            None
        );
        assert_eq!(
            notices,
            [
                crate::profile_config::unread_profile_notice(
                    crate::profile_set::DEFAULT_PROFILE_NAME
                ),
                "Vosh could not read the Default profile file and will build the shared preset \
                 list once the file reads."
                    .to_string(),
            ]
        );
        assert_eq!(
            std::fs::read_to_string(set.active_path()).unwrap(),
            "presets = = [\n"
        );
    }

    /// Save and return the journal of a run over the Healer profile.
    fn journal(dir: &Path) -> WizardJournal {
        let journal = WizardJournal {
            catalog: "[[aliases]]\nname = \"kk\"\nexpansion = \"kick %1\"\n".into(),
            loadouts: "active = []\n".into(),
            profiles: vec![JournalFile {
                path: "profiles/Healer.toml".into(),
                text: "[profile_vars]\ntarget = \"orc\"\n".into(),
            }],
        };
        save_wizard_journal(dir, &journal).unwrap();
        journal
    }

    #[test]
    fn a_journal_writes_each_file_it_names_once() {
        let dir = tempfile::tempdir().unwrap();
        let journal = journal(dir.path());
        assert_eq!(finish_wizard_run(dir.path()), WizardRun::Finished);
        for (path, text) in journal.files(dir.path()) {
            assert_eq!(fs::read_to_string(path).unwrap(), text);
        }
        assert!(!journal_path(dir.path()).exists());
        // Nothing is left to finish at the next launch.
        assert_eq!(finish_wizard_run(dir.path()), WizardRun::Done);
    }

    #[test]
    fn a_journal_whose_files_all_landed_goes_without_a_word() {
        let dir = tempfile::tempdir().unwrap();
        let journal = journal(dir.path());
        for (path, text) in journal.files(dir.path()) {
            write_with_backup(&path, text).unwrap();
        }
        assert_eq!(finish_wizard_run(dir.path()), WizardRun::Done);
        let leftover = &WizardRun::Done.notices();
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(!journal_path(dir.path()).exists());
    }

    // Read only folders are a Unix permission bit.
    #[cfg(unix)]
    #[test]
    fn a_journal_that_will_not_go_leaves_the_run_unfinished() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let journal = journal(dir.path());
        for (path, text) in journal.files(dir.path()) {
            write_with_backup(&path, text).unwrap();
        }
        // The folder takes no changes, so the journal cannot go. It would
        // write its text again at the next launch, over what this session
        // saved.
        let mode = |m| fs::Permissions::from_mode(m);
        fs::set_permissions(dir.path(), mode(0o555)).unwrap();
        let run = finish_wizard_run(dir.path());
        fs::set_permissions(dir.path(), mode(0o755)).unwrap();
        assert_eq!(run, WizardRun::Unfinished);
        assert!(journal_path(dir.path()).exists());
    }

    #[test]
    fn a_journal_that_does_not_read_stays_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(journal_path(dir.path()), "catalog = = [\n").unwrap();
        assert_eq!(finish_wizard_run(dir.path()), WizardRun::Unfinished);
        assert_eq!(WizardRun::Unfinished.notices(), [WIZARD_UNFINISHED_NOTICE]);
        assert!(journal_path(dir.path()).exists());
        assert!(!catalog_path(dir.path()).exists());
    }

    #[test]
    fn every_preset_in_the_library_is_on_by_default() {
        // `presets_on_in_any` takes the defaults as holding every preset
        // a profile list can name. A preset that is off by default needs
        // its id listed there instead.
        let library = include_str!("../../src/lib/presets.ts");
        assert!(library.contains("defaultEnabled: true"));
        assert!(!library.contains("defaultEnabled: false"));
    }

    #[test]
    fn presets_off_is_the_marker_the_page_stores() {
        // Settings stores PRESETS_OFF_MARKER when you turn every preset
        // off, and launch reads it back here as PRESETS_OFF.
        let records = include_str!("../../src/lib/automationRecords.ts");
        let marker = regex::Regex::new(r"export const PRESETS_OFF_MARKER = '([^']*)';")
            .unwrap()
            .captures(records)
            .expect("automationRecords.ts declares PRESETS_OFF_MARKER");
        assert_eq!(&marker[1], PRESETS_OFF);
        // No preset may take the marker as its id.
        let library = include_str!("../../src/lib/presets.ts");
        assert!(!library.contains(&format!("id: '{PRESETS_OFF}'")));
    }

    #[test]
    fn a_catalog_list_wins_over_the_profile_list() {
        let mut catalog = GlobalCatalog {
            enabled_presets: Some(presets(&["none"])),
            ..GlobalCatalog::default()
        };
        let mut profile = Profile::default();
        profile.ui.enabled_presets = presets(&["healing_basics"]);
        assert!(!adopt_catalog_presets(
            &mut catalog,
            &mut profile,
            Some(&ProfilePresetLists {
                lists: vec![presets(&["potion_labels"])],
                unread: Vec::new(),
            })
        ));
        assert_eq!(profile.ui.enabled_presets, presets(&["none"]));
        assert_eq!(catalog.enabled_presets, Some(presets(&["none"])));
    }
}

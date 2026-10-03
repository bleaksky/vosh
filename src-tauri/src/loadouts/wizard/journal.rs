//! The shared catalog wizard's crash journal. The wizard saves it before
//! its first write and takes it out after its last, and launch finishes a
//! run that stopped in between from it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::disk::atomic::write_with_backup;
use crate::loadout_store::{catalog_path, loadouts_path, LoadoutStoreError};

/// Filename of the journal the shared catalog wizard keeps while it
/// writes, see [`WizardJournal`].
const JOURNAL_FILE: &str = "catalog.journal.toml";

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

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

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
}

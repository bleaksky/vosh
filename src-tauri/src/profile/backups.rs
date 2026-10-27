//! The backups Vosh keeps of each profile file, `<name>.toml.bak.<ms>`
//! beside it, which Restore from a backup… lists and puts back. See
//! [`write_with_backup`](crate::disk::atomic::write_with_backup), which
//! makes them.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tracing::warn;

use crate::profile::file::ProfileConfig;

/// One backup as the dialog lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ProfileBackup {
    /// The backup's file name, which a restore takes back.
    pub file: String,
    /// When the file was set aside, in ms since the epoch. It held the
    /// profile as it stood until then.
    pub saved_ms: u64,
    pub triggers: usize,
    pub aliases: usize,
    pub macros: usize,
    pub timers: usize,
}

/// Whether `config` holds what the shared catalog keeps in loadout
/// mode: aliases, triggers, macros or edits to the presets. A profile
/// file saved since the move to loadouts never does, and one from before
/// the move does, so restoring it would lay its old items over the
/// catalog for every character.
pub(crate) fn holds_catalog_items(config: &ProfileConfig) -> bool {
    !config.aliases.is_empty()
        || !config.triggers.is_empty()
        || !config.macros.is_empty()
        || !config.preset_edits.is_empty()
}

/// The backups of the profile file at `path`, newest first. Two that
/// hold the same text show once, the newer, and one that holds what the
/// file holds now does not show, since restoring it changes nothing. A
/// backup that does not read as a profile file does not show either,
/// and in `loadout_mode` neither does one from before the move, see
/// [`holds_catalog_items`].
pub(crate) fn list(path: &Path, loadout_mode: bool) -> Vec<ProfileBackup> {
    let now = std::fs::read(path).ok();
    let mut found = backups_of(path);
    found.sort_by_key(|(ms, _)| std::cmp::Reverse(*ms));
    let mut seen: Vec<Vec<u8>> = now.into_iter().collect();
    let mut listed = Vec::new();
    for (ms, backup) in found {
        let Ok(bytes) = std::fs::read(&backup) else {
            continue;
        };
        if seen.contains(&bytes) {
            continue;
        }
        seen.push(bytes);
        let config = match ProfileConfig::load(&backup) {
            Ok(config) => config,
            Err(e) => {
                warn!(path = %backup.display(), error = %e, "profile backup unreadable");
                continue;
            }
        };
        if loadout_mode && holds_catalog_items(&config) {
            continue;
        }
        let Some(file) = backup.file_name().map(|f| f.to_string_lossy().into_owned()) else {
            continue;
        };
        listed.push(ProfileBackup {
            file,
            saved_ms: ms,
            triggers: config.triggers.len(),
            aliases: config.aliases.len(),
            macros: config.macros.len(),
            timers: config.timers.len(),
        });
    }
    listed
}

/// The backup `file` names beside the profile file at `path`, when it
/// names one that is there. A name with anything but the file's own name,
/// `.bak.` and digits names none, so no restore reads outside the folder.
pub(crate) fn backup_of(path: &Path, file: &str) -> Option<PathBuf> {
    stamp_of(path, file)?;
    let backup = path.with_file_name(file);
    backup.is_file().then_some(backup)
}

/// Move the backups of the profile file `from` to sit beside `to` under
/// its name, as a rename moves the file.
pub(crate) fn follow_rename(from: &Path, to: &Path) {
    let Some(to_name) = to.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return;
    };
    for (ms, backup) in backups_of(from) {
        let moved = to.with_file_name(format!("{to_name}.bak.{ms}"));
        if let Err(e) = std::fs::rename(&backup, &moved) {
            warn!(path = %backup.display(), error = %e, "profile backup not moved");
        }
    }
}

/// Delete the backups of the profile file `path`, as a delete deletes
/// the file.
pub(crate) fn delete_all(path: &Path) {
    for (_, backup) in backups_of(path) {
        if let Err(e) = std::fs::remove_file(&backup) {
            warn!(path = %backup.display(), error = %e, "profile backup not deleted");
        }
    }
}

/// The time in the name `file` of a backup of `path`, when it is one.
fn stamp_of(path: &Path, file: &str) -> Option<u64> {
    let stem = path.file_name()?.to_str()?;
    let digits = file.strip_prefix(stem)?.strip_prefix(".bak.")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// Each backup beside `path` with its time.
fn backups_of(path: &Path) -> Vec<(u64, PathBuf)> {
    let Some(parent) = path.parent() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let ms = stamp_of(path, &name)?;
            Some((ms, entry.path()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{backup_of, delete_all, follow_rename, list};

    fn profile(aliases: usize) -> String {
        use std::fmt::Write;
        let mut text = String::new();
        for n in 0..aliases {
            let _ = write!(
                text,
                "[[aliases]]\nname = \"a{n}\"\nexpansion = \"look\"\n\n"
            );
        }
        text
    }

    #[test]
    fn backups_list_newest_first_once_each_without_the_file_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Maren.toml");
        std::fs::write(&path, profile(3)).unwrap();
        std::fs::write(dir.path().join("Maren.toml.bak.100"), profile(1)).unwrap();
        std::fs::write(dir.path().join("Maren.toml.bak.200"), profile(2)).unwrap();
        std::fs::write(dir.path().join("Maren.toml.bak.300"), profile(2)).unwrap();
        std::fs::write(dir.path().join("Maren.toml.bak.400"), profile(3)).unwrap();
        std::fs::write(dir.path().join("Maren.toml.bak.500"), "not = [toml").unwrap();
        std::fs::write(dir.path().join("Orla.toml.bak.600"), profile(5)).unwrap();
        let listed = list(&path, false);
        let seen: Vec<(u64, usize)> = listed.iter().map(|b| (b.saved_ms, b.aliases)).collect();
        assert_eq!(seen, [(300, 2), (100, 1)]);
        assert_eq!(listed[0].file, "Maren.toml.bak.300");
    }

    #[test]
    fn only_a_backup_of_the_file_is_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Maren.toml");
        std::fs::write(dir.path().join("Maren.toml.bak.100"), "").unwrap();
        assert!(backup_of(&path, "Maren.toml.bak.100").is_some());
        assert!(backup_of(&path, "Maren.toml.bak.101").is_none());
        assert!(backup_of(&path, "Maren.toml").is_none());
        assert!(backup_of(&path, "Orla.toml.bak.100").is_none());
        assert!(backup_of(&path, "Maren.toml.bak.").is_none());
        assert!(backup_of(&path, "Maren.toml.bak.1/../../x").is_none());
        assert!(backup_of(&path, "../Maren.toml.bak.100").is_none());
    }

    #[test]
    fn loadout_mode_leaves_out_a_backup_from_before_the_move() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Maren.toml");
        std::fs::write(&path, "").unwrap();
        std::fs::write(dir.path().join("Maren.toml.bak.100"), profile(2)).unwrap();
        std::fs::write(
            dir.path().join("Maren.toml.bak.200"),
            "[tick]\ninterval_secs = 47\n",
        )
        .unwrap();
        let stamps =
            |loadouts| -> Vec<u64> { list(&path, loadouts).iter().map(|b| b.saved_ms).collect() };
        assert_eq!(stamps(false), [200, 100]);
        assert_eq!(stamps(true), [200]);
    }

    #[test]
    fn backups_follow_a_rename_and_go_with_a_delete() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("Maren.toml");
        let new = dir.path().join("Orla.toml");
        std::fs::write(dir.path().join("Maren.toml.bak.100"), profile(1)).unwrap();
        std::fs::write(dir.path().join("Maren.toml.bak.200"), profile(2)).unwrap();
        follow_rename(&old, &new);
        assert!(backup_of(&new, "Orla.toml.bak.100").is_some());
        assert!(backup_of(&new, "Orla.toml.bak.200").is_some());
        assert!(backup_of(&old, "Maren.toml.bak.100").is_none());
        delete_all(&new);
        assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
    }
}

//! Safe file writes. [`write_with_backup`] swaps the new text in whole
//! and keeps a few timestamped backups beside the file. It refuses every
//! file on the unread file list, so a save never writes the defaults over
//! settings Vosh could not read.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Number of timestamped backups to retain alongside each profile /
/// global config file. Each call to a top-level `save()` rotates the
/// pre-write file off to a new backup before the new content lands,
/// so the user has a recovery window of the last N saves. 10 covers
/// enough launches that an upgrade-time bad save survives even if the
/// next few launches also touch the file.
pub(crate) const BACKUP_RETENTION: usize = 10;

/// Atomically write `contents` to `path`, snapshotting the current
/// on-disk file (if any) to a timestamped `.bak.<unix-ms>` sibling
/// first. After the write, prune older backups so at most
/// `BACKUP_RETENTION` remain.
///
/// Steps in order:
///   1. Ensure parent dir exists.
///   2. Write the new content to `<path>.tmp`.
///   3. Copy the existing file (if any) to `<path>.bak.<unix-ms>`.
///   4. Rename the temp file over `path`. The rename is atomic on
///      every platform we ship, so `path` holds either the old text or
///      the new, never nothing.
///   5. Prune backups: keep the `BACKUP_RETENTION` newest, delete
///      the rest.
///
/// Errors during pruning are swallowed — they should not block the
/// save from being reported as successful. A failure during steps 2 to
/// 4 is fatal, takes away the temp file, and leaves the original file
/// in place, since nothing moves it before the rename.
///
/// A file held by [`hold_unread`] is refused before any step, so no
/// save writes the defaults over settings Vosh could not read.
pub(crate) fn write_with_backup(path: &Path, contents: &str) -> std::io::Result<()> {
    if is_unread(path) {
        return Err(std::io::Error::other(format!(
            "Vosh could not read {} at launch, so it will not save over it",
            path.display()
        )));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = tmp_path_for(path);
    if let Err(e) = swap_in(path, &tmp, contents) {
        // A temp file the write left behind goes, and the original stays.
        if tmp.is_file() {
            let _ = std::fs::remove_file(&tmp);
        }
        return Err(e);
    }
    prune_backups(path, BACKUP_RETENTION);
    Ok(())
}

/// Steps 2 to 4 of [`write_with_backup`]. Nothing here moves or changes
/// `path` before the last step, the rename that swaps `tmp` in.
fn swap_in(path: &Path, tmp: &Path, contents: &str) -> std::io::Result<()> {
    std::fs::write(tmp, contents)?;
    if path.exists() {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let backup = backup_path_for(path, now_ms);
        if let Err(e) = std::fs::copy(path, &backup) {
            // A copy cut short is no backup.
            let _ = std::fs::remove_file(&backup);
            return Err(e);
        }
    }
    std::fs::rename(tmp, path)
}

fn tmp_path_for(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    name.push(".tmp");
    path.with_file_name(name)
}

fn backup_path_for(path: &Path, when_ms: u128) -> PathBuf {
    let mut name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    name.push(format!(".bak.{when_ms}"));
    path.with_file_name(name)
}

/// Delete every `<file>.bak.<digits>` sibling beyond the `keep` most
/// recent. Older backups vanish silently; nothing here is fatal.
fn prune_backups(path: &Path, keep: usize) {
    let Some(parent) = path.parent() else {
        return;
    };
    let Some(stem) = path.file_name().and_then(|s| s.to_str()) else {
        return;
    };
    let prefix = format!("{stem}.bak.");
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    let mut backups: Vec<(u128, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let suffix = name.strip_prefix(&prefix)?;
            let ts: u128 = suffix.parse().ok()?;
            Some((ts, e.path()))
        })
        .collect();
    // Newest first so the head of the list is what we keep.
    backups.sort_by_key(|(ts, _)| std::cmp::Reverse(*ts));
    for (_, path) in backups.into_iter().skip(keep) {
        let _ = std::fs::remove_file(path);
    }
}

/// The profile file and global.toml when Vosh could not read them at
/// launch. The live profile holds the defaults where their settings
/// belong, so a save would write those defaults over your settings, and
/// ten more saves would rotate the last good copy out of the backups.
/// [`write_with_backup`] refuses every path held here. A switch that
/// reads the files again, or a `#profile load` that reads the profile
/// file, lets them go. Held by path, so tests over their own folders
/// never meet.
static UNREAD_FILES: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

fn unread_files() -> std::sync::MutexGuard<'static, Vec<PathBuf>> {
    UNREAD_FILES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Refuse every write to `path` until [`release_unread`] lets it go.
pub(crate) fn hold_unread(path: &Path) {
    let mut files = unread_files();
    if !files.iter().any(|held| held == path) {
        files.push(path.to_path_buf());
    }
}

/// Let writes to `path` resume. The live profile holds what the file
/// says again, or no longer stands in for it.
pub(crate) fn release_unread(path: &Path) {
    unread_files().retain(|held| held != path);
}

/// True when Vosh could not read `path` at launch and still holds it.
pub(crate) fn is_unread(path: &Path) -> bool {
    unread_files().iter().any(|held| held == path)
}

/// Keep holding a file that a rename moved from `from` to `to`.
pub(crate) fn follow_unread(from: &Path, to: &Path) {
    for held in unread_files().iter_mut() {
        if held == from {
            *held = to.to_path_buf();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_with_backup_creates_initial_file_without_backup() {
        // First write to a fresh path produces just the file; no
        // backup yet because there was nothing to roll off.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_with_backup(&path, "initial = true\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "initial = true\n");
        let backups = list_backups(&path);
        assert!(backups.is_empty(), "no backups expected on first write");
    }

    #[test]
    fn write_with_backup_rolls_existing_file_off() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_with_backup(&path, "v1\n").unwrap();
        // A small sleep guarantees a distinct timestamp on the
        // second write; the millisecond resolution is normally
        // enough but back-to-back calls on a fast machine can land
        // in the same ms.
        std::thread::sleep(std::time::Duration::from_millis(2));
        write_with_backup(&path, "v2\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "v2\n");
        let backups = list_backups(&path);
        assert_eq!(backups.len(), 1, "previous version should be backed up");
        let backup_content = std::fs::read_to_string(&backups[0]).unwrap();
        assert_eq!(backup_content, "v1\n");
    }

    #[test]
    fn write_with_backup_keeps_at_most_retention_backups() {
        // Write N+5 times where N = BACKUP_RETENTION. Only the
        // BACKUP_RETENTION most-recent pre-write states should
        // remain on disk; the rest are pruned.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let count = BACKUP_RETENTION + 5;
        for i in 0..count {
            write_with_backup(&path, &format!("v{i}\n")).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let backups = list_backups(&path);
        assert_eq!(
            backups.len(),
            BACKUP_RETENTION,
            "expected exactly BACKUP_RETENTION backups; got {}",
            backups.len()
        );
    }

    #[test]
    fn write_with_backup_creates_parent_dir() {
        // Writing to a path whose parent does not yet exist must
        // create the directory tree; this mirrors how the live
        // `<app_data_dir>/profiles/` subdir is created on first
        // launch.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/under/here/config.toml");
        write_with_backup(&path, "ok\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "ok\n");
    }

    #[test]
    fn a_save_that_fails_leaves_the_old_file_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Bard.toml");
        write_with_backup(&path, "v1\n").unwrap();
        // Something holds the name of the temp file, the way a full disk
        // or a lock stops the write.
        std::fs::create_dir(tmp_path_for(&path)).unwrap();
        assert!(write_with_backup(&path, "v2\n").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "v1\n");
        assert!(tmp_path_for(&path).is_dir());

        // Once the write can land, it does, and the old text waits in a
        // backup.
        std::fs::remove_dir(tmp_path_for(&path)).unwrap();
        write_with_backup(&path, "v2\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "v2\n");
        let backups = list_backups(&path);
        assert_eq!(
            std::fs::read_to_string(backups.last().unwrap()).unwrap(),
            "v1\n"
        );
    }

    #[test]
    fn a_file_held_as_unread_is_never_written_or_moved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Healer.toml");
        std::fs::write(&path, "tracked = = [\n").unwrap();
        hold_unread(&path);
        assert!(write_with_backup(&path, "defaults = true\n").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "tracked = = [\n");
        let leftover = &list_backups(&path);
        assert!(leftover.is_empty(), "{leftover:?}");

        // A rename carries the hold to the new name.
        let renamed = dir.path().join("Cleric.toml");
        follow_unread(&path, &renamed);
        assert!(!is_unread(&path));
        assert!(write_with_backup(&renamed, "defaults = true\n").is_err());

        release_unread(&renamed);
        write_with_backup(&renamed, "fixed = true\n").unwrap();
        assert_eq!(std::fs::read_to_string(&renamed).unwrap(), "fixed = true\n");
    }

    /// Helper for the backup tests: list every `<file>.bak.<digits>`
    /// sibling of `path` in chronological order (oldest first).
    fn list_backups(path: &Path) -> Vec<std::path::PathBuf> {
        let Some(parent) = path.parent() else {
            return Vec::new();
        };
        let Some(stem) = path.file_name().and_then(|s| s.to_str()) else {
            return Vec::new();
        };
        let prefix = format!("{stem}.bak.");
        let mut out: Vec<(u128, std::path::PathBuf)> = std::fs::read_dir(parent)
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let suffix = name.strip_prefix(&prefix)?;
                let ts: u128 = suffix.parse().ok()?;
                Some((ts, e.path()))
            })
            .collect();
        out.sort_by_key(|(ts, _)| *ts);
        out.into_iter().map(|(_, p)| p).collect()
    }
}

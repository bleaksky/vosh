//! A debug build, the one `npm run tauri dev` runs, keeps its data in a
//! folder of its own beside the installed app's, `com.aabahran.vosh.dev`
//! next to `com.aabahran.vosh`. Its first launch fills that folder with a
//! copy of the installed app's, without the logs and the backups, and
//! marks it with `.copied`. From then on neither touches the other's
//! files, so an installed Vosh older than the dev build never reads a
//! setting the dev build added and saves the file back without it. A dev
//! folder without the mark, which a launch whose copy failed ran on, is
//! set aside and copied again. Delete the dev folder to start again from
//! a fresh copy.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The dev build's folder beside `installed`, the installed app's.
pub(crate) fn dev_folder(installed: &Path) -> PathBuf {
    let mut name = installed
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    name.push(".dev");
    installed.with_file_name(name)
}

/// The mark a whole copy leaves in the dev folder.
const COPIED: &str = ".copied";

/// Fill `dev` with a copy of `installed` unless it holds a whole copy
/// already, and say whether it did. The copy goes to a sibling first and
/// takes the dev folder's name once whole and marked, so a launch that
/// stops partway leaves no half copy behind. A dev folder with no mark
/// moves aside to `<dev>.unmarked-<unix ms>` first. Without an installed
/// folder the dev folder starts empty.
pub(crate) fn seed(installed: &Path, dev: &Path) -> io::Result<bool> {
    if dev.join(COPIED).is_file() {
        return Ok(false);
    }
    let sibling = |suffix: &str| {
        let mut name = dev.as_os_str().to_os_string();
        name.push(suffix);
        PathBuf::from(name)
    };
    let partial = sibling(".partial");
    if partial.exists() {
        fs::remove_dir_all(&partial)?;
    }
    if installed.is_dir() {
        copy_tree(installed, &partial)?;
    } else {
        fs::create_dir_all(&partial)?;
    }
    fs::write(partial.join(COPIED), "")?;
    if dev.exists() {
        let when = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        fs::rename(dev, sibling(&format!(".unmarked-{when}")))?;
    }
    fs::rename(&partial, dev)?;
    Ok(true)
}

/// Whether the copy leaves out the file named `name`: the logs and the
/// retired map store, which can be large and stay open in the installed
/// app, and the backups of each safe write.
fn left_out(name: &str) -> bool {
    name.ends_with(".sqlite")
        || name.ends_with(".sqlite-wal")
        || name.ends_with(".sqlite-shm")
        || name.contains(".bak.")
}

fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if left_out(&name.to_string_lossy()) {
            continue;
        }
        let kind = entry.file_type()?;
        let target = to.join(&name);
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn the_dev_folder_sits_beside_the_installed_one() {
        let installed = Path::new("/data/com.aabahran.vosh");
        assert_eq!(
            dev_folder(installed),
            Path::new("/data/com.aabahran.vosh.dev")
        );
    }

    #[test]
    fn the_first_launch_copies_the_profiles_and_leaves_the_logs_and_backups() {
        let root = tempfile::tempdir().unwrap();
        let installed = root.path().join("com.aabahran.vosh");
        write(&installed.join("profiles.toml"), "active = \"Tolliver\"");
        write(&installed.join("profiles/Tolliver.toml"), "[ui]");
        write(
            &installed.join("profiles/Tolliver.toml.bak.1700000000000"),
            "old",
        );
        write(
            &installed.join("plugins/stats/manifest.toml"),
            "name = \"stats\"",
        );
        write(&installed.join("logs.sqlite"), "db");
        write(&installed.join("logs.sqlite-wal"), "wal");
        let dev = dev_folder(&installed);

        assert!(seed(&installed, &dev).unwrap());

        assert_eq!(
            fs::read_to_string(dev.join("profiles/Tolliver.toml")).unwrap(),
            "[ui]"
        );
        assert!(dev.join("profiles.toml").is_file());
        assert!(dev.join("plugins/stats/manifest.toml").is_file());
        assert!(!dev
            .join("profiles/Tolliver.toml.bak.1700000000000")
            .exists());
        assert!(!dev.join("logs.sqlite").exists());
        assert!(!dev.join("logs.sqlite-wal").exists());
        assert!(!root.path().join("com.aabahran.vosh.dev.partial").exists());
    }

    #[test]
    fn a_dev_folder_with_a_whole_copy_is_left_alone() {
        let root = tempfile::tempdir().unwrap();
        let installed = root.path().join("com.aabahran.vosh");
        write(&installed.join("profiles/Tolliver.toml"), "installed");
        let dev = dev_folder(&installed);
        assert!(seed(&installed, &dev).unwrap());
        write(&dev.join("profiles/Tolliver.toml"), "dev");

        assert!(!seed(&installed, &dev).unwrap());

        assert_eq!(
            fs::read_to_string(dev.join("profiles/Tolliver.toml")).unwrap(),
            "dev"
        );
    }

    #[test]
    fn a_dev_folder_a_failed_copy_left_is_set_aside_and_copied_again() {
        let root = tempfile::tempdir().unwrap();
        let installed = root.path().join("com.aabahran.vosh");
        write(&installed.join("profiles/Tolliver.toml"), "installed");
        let dev = dev_folder(&installed);
        // A launch whose copy failed ran on an empty folder.
        write(&dev.join("scripts/.keep"), "");

        assert!(seed(&installed, &dev).unwrap());

        assert_eq!(
            fs::read_to_string(dev.join("profiles/Tolliver.toml")).unwrap(),
            "installed"
        );
        let aside = fs::read_dir(root.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains(".dev.unmarked-"))
            .count();
        assert_eq!(aside, 1);
    }

    #[test]
    fn without_an_installed_folder_the_dev_folder_starts_empty() {
        let root = tempfile::tempdir().unwrap();
        let installed = root.path().join("com.aabahran.vosh");
        let dev = dev_folder(&installed);

        assert!(seed(&installed, &dev).unwrap());

        assert!(dev.is_dir());
        let names: Vec<_> = fs::read_dir(&dev)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name())
            .collect();
        assert_eq!(names, [COPIED]);
    }

    #[test]
    fn a_copy_a_launch_left_partway_is_thrown_away() {
        let root = tempfile::tempdir().unwrap();
        let installed = root.path().join("com.aabahran.vosh");
        write(&installed.join("global.toml"), "theme = \"triad\"");
        let dev = dev_folder(&installed);
        write(
            &root.path().join("com.aabahran.vosh.dev.partial/stray.toml"),
            "half",
        );

        assert!(seed(&installed, &dev).unwrap());

        assert!(dev.join("global.toml").is_file());
        assert!(!dev.join("stray.toml").exists());
    }
}

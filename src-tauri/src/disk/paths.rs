//! The name of every file and folder Vosh keeps in the app data folder.
//! Each function takes that folder, so a test can point it at a folder
//! of its own. The name an export takes in your Downloads folder, and
//! how an import reads it back, sit at the end.

use std::path::{Path, PathBuf};

use crate::sessions::SessionId;

/// The profile index, which lists the profiles and names the active one.
pub(crate) fn profiles_index_path(app_data: &Path) -> PathBuf {
    app_data.join("profiles.toml")
}

/// The folder that holds one file per profile.
pub(crate) fn profiles_dir(app_data: &Path) -> PathBuf {
    app_data.join("profiles")
}

/// The file of the profile `name`, whether or not it exists yet.
pub(crate) fn profile_path(app_data: &Path, name: &str) -> PathBuf {
    profiles_dir(app_data).join(format!("{name}.toml"))
}

/// The one profile file of the builds before named profiles, which
/// [`crate::profile::set::ProfileSet::load_or_migrate`] moves into
/// `profiles/` as the default profile.
pub(crate) fn root_profile_path(app_data: &Path) -> PathBuf {
    app_data.join("profile.toml")
}

/// The settings every profile shares, such as the theme and the font.
pub(crate) fn global_path(app_data: &Path) -> PathBuf {
    app_data.join("global.toml")
}

/// The shared catalog, which puts Vosh in loadout mode while it is there.
pub(crate) fn catalog_path(app_data: &Path) -> PathBuf {
    app_data.join("catalog.toml")
}

/// The loadouts, and which of them are on.
pub(crate) fn loadouts_path(app_data: &Path) -> PathBuf {
    app_data.join("loadouts.toml")
}

/// The folder the shared catalog wizard copies each profile file into
/// before it changes any.
pub(crate) fn legacy_dir(app_data: &Path) -> PathBuf {
    profiles_dir(app_data).join("legacy")
}

/// The journal the shared catalog wizard keeps while it writes, see
/// [`crate::loadouts::wizard::journal::WizardJournal`].
pub(crate) fn journal_path(app_data: &Path) -> PathBuf {
    app_data.join("catalog.journal.toml")
}

/// The game log.
pub(crate) fn log_db_path(app_data: &Path) -> PathBuf {
    app_data.join("logs.sqlite")
}

/// The scrollback `session` leaves for the next launch to show. The first
/// session keeps scrollback.txt, which an older build reads too, and each
/// other one a file with its number.
pub(crate) fn scrollback_path(app_data: &Path, session: SessionId) -> PathBuf {
    if session == SessionId::FIRST {
        app_data.join("scrollback.txt")
    } else {
        app_data.join(format!("scrollback-{session}.txt"))
    }
}

/// How full each affect was cast, per character, for the Affects pane.
pub(crate) fn affect_full_path(app_data: &Path) -> PathBuf {
    app_data.join("affect_full.toml")
}

/// Your drafts and posts for the writing card, per character, see
/// [`crate::writing`].
pub(crate) fn writing_path(app_data: &Path) -> PathBuf {
    app_data.join("writing.toml")
}

/// The folder `#script load` reads Lua files from.
pub(crate) fn scripts_dir(app_data: &Path) -> PathBuf {
    app_data.join("scripts")
}

/// The folder of plugins, one folder each.
pub(crate) fn plugins_dir(app_data: &Path) -> PathBuf {
    app_data.join("plugins")
}

/// Where a file you export lands in `dir`, your Downloads folder:
/// `<stem>.<ext>`, or `<stem> (2).<ext>` and on when that file is there,
/// so an export never replaces a file you have. Settings has no save
/// panel to ask you.
pub(crate) fn export_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() {
        return first;
    }
    let mut n = 2u32;
    loop {
        let path = dir.join(format!("{stem} ({n}).{ext}"));
        if !path.exists() {
            return path;
        }
        n += 1;
    }
}

/// `stem` without the ` (n)` [`export_path`] adds to a second export of
/// one name, so an import reads `Healer profile (2)` as `Healer profile`.
pub(crate) fn drop_copy_number(stem: &str) -> &str {
    let Some(inner) = stem.strip_suffix(')') else {
        return stem;
    };
    let Some(at) = inner.rfind(" (") else {
        return stem;
    };
    let digits = &inner[at + 2..];
    if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
        &stem[..at]
    } else {
        stem
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_export_never_replaces_a_file_and_an_import_reads_its_name_back() {
        let dir = tempfile::tempdir().unwrap();
        let first = export_path(dir.path(), "wait_full", "zip");
        assert_eq!(first, dir.path().join("wait_full.zip"));
        std::fs::write(&first, "x").unwrap();
        let second = export_path(dir.path(), "wait_full", "zip");
        assert_eq!(second, dir.path().join("wait_full (2).zip"));
        std::fs::write(&second, "x").unwrap();
        assert_eq!(
            export_path(dir.path(), "wait_full", "zip"),
            dir.path().join("wait_full (3).zip")
        );
        // Another kind of file keeps a count of its own.
        assert_eq!(
            export_path(dir.path(), "wait_full", "toml"),
            dir.path().join("wait_full.toml")
        );
        for (stem, read) in [
            ("wait_full (2)", "wait_full"),
            ("Tank 2 profile (12)", "Tank 2 profile"),
            ("Healer (b)", "Healer (b)"),
            ("Healer ()", "Healer ()"),
            ("Healer", "Healer"),
        ] {
            assert_eq!(drop_copy_number(stem), read, "{stem}");
        }
    }
}

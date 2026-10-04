//! The name of every file and folder Vosh keeps in the app data folder.
//! Each function takes that folder, so a test can point it at a folder
//! of its own.

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

/// The folder `#script load` reads Lua files from.
pub(crate) fn scripts_dir(app_data: &Path) -> PathBuf {
    app_data.join("scripts")
}

/// The folder of plugins, one folder each.
pub(crate) fn plugins_dir(app_data: &Path) -> PathBuf {
    app_data.join("plugins")
}

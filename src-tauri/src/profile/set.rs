//! Named-profile collection for the multi-profile system.
//!
//! ## Layout on disk
//!
//! ```text
//! <app_data_dir>/
//!   profiles.toml         index: active profile name + list of profile entries
//!   profiles/
//!     default.toml        per-profile snapshot (same shape as the old
//!                         single profile.toml)
//!     aabahran-ilsabet.toml
//!     ...
//! ```
//!
//! ## Migration
//!
//! First launch after the multi-profile upgrade:
//!
//! - If `profiles.toml` exists already, load it normally.
//! - Else if the legacy `profile.toml` exists at the data-dir root, move
//!   it to `profiles/default.toml` and create a `profiles.toml` index
//!   pointing at "default" as the active profile. Existing users keep
//!   their setup with zero action.
//! - Else create an empty index with one "default" profile entry (its
//!   file is created on the first save). A folder that holds nothing of
//!   yours is a new install, which also gets a global.toml that starts it
//!   on Triad and a profiles/default.toml with every preset off.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::app::state::SharedState;
use crate::disk::paths;
use crate::disk::save::{persist_state, PERSIST_LOCK};
use crate::loadouts::presets::PRESETS_OFF;
use crate::profile::file::{ConfigError, ProfileConfig};
use crate::profile::login_match::AutoMatch;
use crate::profile::shared::{GlobalConfig, ScopeConfig};
use crate::sessions::{SessionId, SessionRow};

#[derive(Debug, Error)]
pub(crate) enum ProfileSetError {
    #[error("Vosh could not read or write a profile file ({0}).")]
    Io(#[from] std::io::Error),
    #[error("Vosh could not read profiles.toml because it is not valid TOML.")]
    Deserialize(#[from] toml::de::Error),
    #[error("Vosh could not save the profile list.")]
    Serialize(#[from] toml::ser::Error),
    #[error("You already have a profile named {0}.")]
    AlreadyExists(String),
    #[error("Your profiles folder already holds a file named {0}.toml. Choose another name.")]
    FileExists(String),
    #[error("Vosh cannot find a profile named {0}.")]
    NotFound(String),
    /// A session plays the profile, the selected one or any other.
    #[error(
        "A session plays {name}. Switch that session to another profile or close it, then delete {name}.",
        name = display_name(.0)
    )]
    CannotDeletePlayed(String),
    #[error("Give the profile a name.")]
    EmptyName,
    #[error(
        "You cannot name a profile {0}. Use letters, numbers, spaces, hyphens, and underscores."
    )]
    InvalidName(String),
    #[error("Vosh needs a character name to use this profile when you log in.")]
    NoCharacter,
    #[error("Choose a world for {0} first.")]
    NoWorld(String),
}

/// Per-profile entry in the index. The full per-profile payload lives in
/// `profiles/<name>.toml` (a `ProfileConfig`); this struct only holds the
/// directory-level metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ProfileEntry {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Stage 2: auto-load this profile when the connect dialog
    /// matches the host/port/character below. Stage 1 only persists
    /// the field — no auto-load wiring yet.
    ///
    /// `auto_match` shape: see [`AutoMatch`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_match: Option<AutoMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct ProfilesIndex {
    /// Name of the active profile. Always corresponds to one of the
    /// entries in `profiles`.
    pub active: String,
    /// Every profile registered. The active entry's per-profile file
    /// is what gets saved on every `persist_profile` call.
    #[serde(default, rename = "profile")]
    pub profiles: Vec<ProfileEntry>,
    /// Per-category scope map. Decides which UI categories survive
    /// profile switches (Global) and which travel with the active
    /// profile (Profile). Defaults match v1: all five UI categories
    /// global, everything else profile-scoped.
    #[serde(default)]
    pub scope: ScopeConfig,
    /// One-time moves across every profile that already ran, such as
    /// `prompt-capture-to-profile`, so none runs twice. An older build
    /// drops the list on its next save, and a move then runs again on
    /// return, which each one allows for.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub migrations: Vec<String>,
    /// Sentences a session left for the next launch to show once, such as
    /// the Line triggers that matched your prompt and no longer see it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<String>,
    /// The sessions you had open, in order, for the next launch to
    /// restore. Left out with `selected` while they say nothing `active`
    /// does not, see [`SessionEntry::list`]. An older build drops both on
    /// its next save, and the launch after it opens one session on
    /// `active` (D14).
    #[serde(default, rename = "session", skip_serializing_if = "Vec::is_empty")]
    pub sessions: Vec<SessionEntry>,
    /// The session that was selected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<SessionId>,
    /// Where you are in Get started. Only a new install writes it, so an
    /// index with none never opens the card at launch. An older build
    /// drops it on its next save.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub get_started: Option<GetStarted>,
    /// Keep logs for, in days, once for the whole install, since every
    /// profile shares logs.sqlite (D34). None keeps logs forever and
    /// stays out of the file. An older build drops it on its next save.
    /// A hand edit outside [`KEEP_DAYS`] reads as forever.
    #[serde(
        default,
        deserialize_with = "lenient_keep_logs_days",
        skip_serializing_if = "Option::is_none"
    )]
    pub keep_logs_days: Option<u32>,
}

/// The spans Keep logs for offers besides forever, in days.
pub(crate) const KEEP_DAYS: [u32; 3] = [365, 90, 30];

/// Read Keep logs for as one of [`KEEP_DAYS`], or forever for anything
/// else a hand edit left, such as 0, 45 or "90", so it never deletes
/// every log or loses the rest of profiles.toml. The next save drops it.
fn lenient_keep_logs_days<'de, D>(deser: D) -> Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = toml::Value::deserialize(deser)?;
    let days = raw
        .as_integer()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|d| KEEP_DAYS.contains(d));
    if days.is_none() {
        tracing::warn!(value = %raw, "keep_logs_days is not a span Vosh offers, keeping logs forever");
    }
    Ok(days)
}

/// Get started as profiles.toml keeps it, once for the whole install.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GetStarted {
    /// The card opens at launch.
    pub at_launch: bool,
    /// The steps you finished, by id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub done: Vec<String>,
}

/// A session as profiles.toml keeps it for the next launch: its id, which
/// names its scrollback file, the name you gave it, where it dials and
/// the profile it last played.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SessionEntry {
    pub id: SessionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tls: bool,
    pub profile: String,
}

impl SessionEntry {
    /// The list profiles.toml keeps for `rows`, the sessions in order,
    /// with the id of the selected one. Both stay empty while the one
    /// session open is the first and has no name, since `active` then
    /// says all a launch needs and the file stays as today.
    pub(crate) fn list(rows: &[SessionRow]) -> (Vec<Self>, Option<SessionId>) {
        if let [row] = rows {
            if row.id == SessionId::FIRST && row.name.is_none() {
                return (Vec::new(), None);
            }
        }
        let entries = rows
            .iter()
            .map(|row| Self {
                id: row.id,
                name: row.name.clone(),
                host: row.host.clone(),
                port: row.port,
                tls: row.tls,
                profile: row.profile.clone().unwrap_or_default(),
            })
            .collect();
        (
            entries,
            rows.iter().find(|row| row.selected).map(|row| row.id),
        )
    }
}

pub(crate) const DEFAULT_PROFILE_NAME: &str = "default";

/// The theme a new install starts on, and its light theme (Themes review
/// Q3 and Q4). `UiConfig` keeps Obsidian Ember and Vellum as its
/// defaults, which a file without these keys still reads.
const NEW_INSTALL_THEME: &str = "triad";
const NEW_INSTALL_LIGHT_THEME: &str = "rubric";

/// Live in-memory view of the profile collection. Held in `AppState`
/// behind a `Mutex` so commands can mutate it. The active in-memory
/// `Profile` is still the canonical runtime state; this struct is the
/// catalog around it.
#[derive(Debug)]
pub(crate) struct ProfileSet {
    root: PathBuf,
    pub(super) index: ProfilesIndex,
}

/// One profile as [`ProfileSet::read_all`] reads it.
pub(crate) struct StoredProfile<'a> {
    pub(crate) name: &'a str,
    pub(crate) path: PathBuf,
    /// None for a profile that never saved a file.
    pub(crate) file: Option<Result<SavedFile, ConfigError>>,
}

/// A profile file that reads, with the text it holds.
pub(crate) struct SavedFile {
    pub(crate) text: String,
    pub(crate) config: ProfileConfig,
}

/// Read the profile file at `path` the way [`ProfileConfig::load`] does,
/// keeping its text.
fn read_saved(path: &Path) -> Result<SavedFile, ConfigError> {
    let text = std::fs::read_to_string(path)?;
    let config = ProfileConfig::from_toml(&text)?;
    Ok(SavedFile { text, config })
}

impl ProfileSet {
    /// Load (or migrate-and-load) the profile set rooted at the given
    /// app data directory. Always returns a valid set; on a fresh
    /// install it returns a single-entry "default" set whose profile
    /// file has every preset off, beside a global.toml that holds the
    /// theme a new install starts on.
    pub(crate) fn load_or_migrate(root: PathBuf) -> Result<Self, ProfileSetError> {
        let index_path = paths::profiles_index_path(&root);

        if index_path.exists() {
            let text = std::fs::read_to_string(&index_path)?;
            let mut index: ProfilesIndex = toml::from_str(&text)?;
            // Defensive: ensure the active entry actually exists in
            // the list. Repair instead of erroring out.
            if !index.profiles.iter().any(|p| p.name == index.active) {
                if let Some(first) = index.profiles.first() {
                    index.active = first.name.clone();
                } else {
                    index.profiles.push(ProfileEntry {
                        name: DEFAULT_PROFILE_NAME.to_string(),
                        description: None,
                        auto_match: None,
                    });
                    index.active = DEFAULT_PROFILE_NAME.to_string();
                }
            }
            return Ok(Self { root, index });
        }

        // No index file. Migrate the legacy single-profile layout if
        // present, otherwise seed a fresh empty index.
        let new_install = holds_nothing_of_yours(&root);
        std::fs::create_dir_all(paths::profiles_dir(&root))?;
        let legacy = paths::root_profile_path(&root);
        if legacy.exists() {
            let target = paths::profile_path(&root, DEFAULT_PROFILE_NAME);
            std::fs::rename(&legacy, &target)?;
        }
        // global.toml holds the new install's theme and profiles/default.toml
        // its presets, all off, before the index names a profile, so a
        // crash before the first save keeps both. When either does not
        // save, the folder stays new for the next launch.
        if new_install {
            write_new_install(&root)?;
        }

        let index = ProfilesIndex {
            active: DEFAULT_PROFILE_NAME.to_string(),
            profiles: vec![ProfileEntry {
                name: DEFAULT_PROFILE_NAME.to_string(),
                description: None,
                auto_match: None,
            }],
            scope: ScopeConfig::default(),
            migrations: Vec::new(),
            notices: Vec::new(),
            sessions: Vec::new(),
            selected: None,
            get_started: new_install.then_some(GetStarted {
                at_launch: true,
                done: Vec::new(),
            }),
            keep_logs_days: None,
        };
        let set = Self { root, index };
        set.save_index()?;
        Ok(set)
    }

    pub(crate) fn save_index(&self) -> Result<(), ProfileSetError> {
        let path = paths::profiles_index_path(&self.root);
        let body = toml::to_string_pretty(&self.index)?;
        // Atomic write with rotating backups; same protection the
        // per-profile and global config files get. A botched index
        // write would orphan every profile, so the rollback safety
        // net matters here too.
        crate::disk::atomic::write_with_backup(&path, &body)?;
        Ok(())
    }

    pub(crate) fn active_name(&self) -> &str {
        &self.index.active
    }

    /// The sessions a launch restores, with the one selected.
    pub(crate) fn sessions(&self) -> (&[SessionEntry], Option<SessionId>) {
        (&self.index.sessions, self.index.selected)
    }

    pub(crate) fn active_path(&self) -> PathBuf {
        self.profile_path(&self.index.active)
    }

    pub(crate) fn profile_path(&self, name: &str) -> PathBuf {
        paths::profile_path(&self.root, name)
    }

    /// Path to the shared global.toml. Holds UI preferences (theme,
    /// font, dock layout, keep-last, auto-update) that survive
    /// profile switches.
    pub(crate) fn global_path(&self) -> PathBuf {
        paths::global_path(&self.root)
    }

    pub(crate) fn list(&self) -> &[ProfileEntry] {
        &self.index.profiles
    }

    pub(crate) fn get(&self, name: &str) -> Option<&ProfileEntry> {
        self.index.profiles.iter().find(|p| p.name == name)
    }

    /// Every profile in index order with its file. Each file reads only
    /// when the caller asks for its profile, so a caller that stops at a
    /// file that does not read reads no further.
    pub(crate) fn read_all(&self) -> impl Iterator<Item = StoredProfile<'_>> + '_ {
        self.index.profiles.iter().map(|entry| {
            let path = self.profile_path(&entry.name);
            let file = path.exists().then(|| read_saved(&path));
            StoredProfile {
                name: &entry.name,
                path,
                file,
            }
        })
    }

    /// Refuse `name` for a new or renamed profile when another profile
    /// has it in any case, or when its file is already on disk. Profile
    /// files live on disks that ignore case, as macOS and Windows do by
    /// default, so `Default.toml` is the file of `default`, and a name
    /// that differs only in case would write over that profile. A
    /// rename passes the profile's current name as `renaming`, which
    /// may keep its own name in another case.
    fn check_free(&self, name: &str, renaming: Option<&str>) -> Result<(), ProfileSetError> {
        if let Some(other) = self
            .index
            .profiles
            .iter()
            .find(|p| Some(p.name.as_str()) != renaming && p.name.eq_ignore_ascii_case(name))
        {
            return Err(ProfileSetError::AlreadyExists(display_name(&other.name)));
        }
        let own_file = renaming.is_some_and(|old| old.eq_ignore_ascii_case(name));
        if !own_file && self.profile_path(name).exists() {
            return Err(ProfileSetError::FileExists(name.to_string()));
        }
        Ok(())
    }

    /// Save the index after a change to its entries. When the save
    /// fails, put `before` back so memory keeps matching the file.
    pub(super) fn save_profiles_or_restore(
        &mut self,
        before: Vec<ProfileEntry>,
    ) -> Result<(), ProfileSetError> {
        let saved = self.save_index();
        if saved.is_err() {
            self.index.profiles = before;
        }
        saved
    }

    /// Create an empty entry. The per-profile file is created on the
    /// next save (so a brand-new profile inherits whatever
    /// `ProfileConfig::fresh()` produces on first persist). A test
    /// shorthand for `create_from` with no source and no claim.
    #[cfg(test)]
    pub(crate) fn create(&mut self, name: &str) -> Result<(), ProfileSetError> {
        self.create_from(name, None, None).map(|_| ())
    }

    /// Create `name` with `auto_match` as its login claim. With
    /// `copy_from` it starts as a copy of that profile's file and
    /// description. The copy reads the file as it sits on disk, so the
    /// caller persists the live profile first when copying it. The
    /// claim takes nothing from other profiles, which is what
    /// `set_login` is for. Does not switch.
    pub(crate) fn create_from(
        &mut self,
        name: &str,
        copy_from: Option<&str>,
        auto_match: Option<AutoMatch>,
    ) -> Result<ProfileEntry, ProfileSetError> {
        let name = sanitize_name(name)?;
        let description = match copy_from {
            Some(source) => {
                let Some(source_entry) = self.get(source) else {
                    return Err(ProfileSetError::NotFound(source.to_string()));
                };
                source_entry.description.clone()
            }
            None => None,
        };
        self.check_free(&name, None)?;
        if let Some(source) = copy_from {
            let src_path = self.profile_path(source);
            if src_path.exists() {
                std::fs::copy(&src_path, self.profile_path(&name))?;
            }
        }
        let entry = ProfileEntry {
            name,
            description,
            auto_match: auto_match.map(AutoMatch::cleaned),
        };
        self.index.profiles.push(entry.clone());
        self.save_index()?;
        Ok(entry)
    }

    /// Create `name` from `file`, a profile file an import planned, which
    /// goes to disk through the safe write, with `auto_match` as its login
    /// claim. As with [`Self::create_from`], the claim takes nothing from
    /// other profiles and nothing switches. When the profile list does not
    /// save, the file goes again, so the name stays free.
    pub(crate) fn create_from_file(
        &mut self,
        name: &str,
        file: &ProfileConfig,
        auto_match: Option<AutoMatch>,
    ) -> Result<ProfileEntry, ProfileSetError> {
        let name = sanitize_name(name)?;
        self.check_free(&name, None)?;
        let path = self.profile_path(&name);
        crate::disk::atomic::write_with_backup(&path, &toml::to_string_pretty(file)?)?;
        let entry = ProfileEntry {
            name,
            description: None,
            auto_match: auto_match.map(AutoMatch::cleaned),
        };
        let before = self.index.profiles.clone();
        self.index.profiles.push(entry.clone());
        if let Err(e) = self.save_profiles_or_restore(before) {
            if let Err(removed) = std::fs::remove_file(&path) {
                tracing::error!(error = %removed, path = %path.display(), "imported profile file stayed after the list did not save");
            }
            return Err(e);
        }
        Ok(entry)
    }

    /// Delete a non-active profile's entry + per-profile file.
    pub(crate) fn delete(&mut self, name: &str) -> Result<(), ProfileSetError> {
        if name == self.index.active {
            return Err(ProfileSetError::CannotDeletePlayed(name.to_string()));
        }
        let Some(idx) = self.index.profiles.iter().position(|p| p.name == name) else {
            return Err(ProfileSetError::NotFound(name.to_string()));
        };
        self.index.profiles.remove(idx);
        let path = self.profile_path(name);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        self.save_index()?;
        Ok(())
    }

    /// Rename an entry. Moves the per-profile file too. A new name that
    /// differs only in case from the old one keeps the same file.
    pub(crate) fn rename(&mut self, old: &str, new: &str) -> Result<(), ProfileSetError> {
        let new = sanitize_name(new)?;
        let Some(idx) = self.index.profiles.iter().position(|p| p.name == old) else {
            return Err(ProfileSetError::NotFound(old.to_string()));
        };
        self.check_free(&new, Some(old))?;
        let old_path = self.profile_path(old);
        let new_path = self.profile_path(&new);
        if old_path.exists() {
            std::fs::rename(&old_path, &new_path)?;
            // A file Vosh could not read at launch stays refused under
            // its new name.
            crate::disk::atomic::follow_unread(&old_path, &new_path);
        }
        if self.index.active == old {
            self.index.active.clone_from(&new);
        }
        for session in &mut self.index.sessions {
            if session.profile == old {
                session.profile.clone_from(&new);
            }
        }
        self.index.profiles[idx].name = new;
        self.save_index()?;
        Ok(())
    }

    /// Copy an existing profile under a new name. Does not switch.
    pub(crate) fn duplicate(&mut self, source: &str, new: &str) -> Result<(), ProfileSetError> {
        // Auto-match deliberately NOT copied: the duplicate is usually
        // a starting point for a NEW character/MUD pairing.
        self.create_from(new, Some(source), None).map(|_| ())
    }

    /// Set the active profile. Caller is responsible for writing the
    /// previous active profile to disk BEFORE calling switch (so the
    /// in-memory state is not lost), and for reading the new profile's
    /// files before it, so a file that does not read never leaves the
    /// index naming a profile the live state does not hold. An index
    /// that does not save keeps the old active name in memory too.
    pub(crate) fn switch(&mut self, name: &str) -> Result<(), ProfileSetError> {
        if self.get(name).is_none() {
            return Err(ProfileSetError::NotFound(name.to_string()));
        }
        let previous = std::mem::replace(&mut self.index.active, name.to_string());
        if let Err(e) = self.save_index() {
            self.index.active = previous;
            return Err(e);
        }
        Ok(())
    }

    /// Keep `sessions` and `selected`, the list a launch restores, and
    /// name `active`, the profile the selected session plays, as the
    /// active one while the set lists it. The index saves only when one of
    /// them changed, and one that does not save keeps what it had in
    /// memory too.
    pub(crate) fn keep_sessions(
        &mut self,
        active: Option<&str>,
        sessions: Vec<SessionEntry>,
        selected: Option<SessionId>,
    ) -> Result<(), ProfileSetError> {
        let active = active
            .filter(|name| self.get(name).is_some())
            .unwrap_or(&self.index.active)
            .to_string();
        let index = &self.index;
        if active == index.active && sessions == index.sessions && selected == index.selected {
            return Ok(());
        }
        let before = self.index.clone();
        self.index.active = active;
        self.index.sessions = sessions;
        self.index.selected = selected;
        if let Err(e) = self.save_index() {
            self.index = before;
            return Err(e);
        }
        Ok(())
    }

    /// True when the one-time move `id` already ran.
    pub(crate) fn migrated(&self, id: &str) -> bool {
        self.index.migrations.iter().any(|m| m == id)
    }

    /// Record that the one-time move `id` ran, and save the index. An
    /// index that does not save forgets it again, so the move runs at the
    /// next launch.
    pub(crate) fn record_migration(&mut self, id: &str) -> Result<(), ProfileSetError> {
        if self.migrated(id) {
            return Ok(());
        }
        self.index.migrations.push(id.to_string());
        if let Err(e) = self.save_index() {
            self.index.migrations.retain(|m| m != id);
            return Err(e);
        }
        Ok(())
    }

    /// Record that the one-time step `id` ran and leave `notice` for the
    /// next launch to show, in one save of the index. An index that does
    /// not save forgets both, so the step runs again.
    pub(crate) fn record_with_notice(
        &mut self,
        id: &str,
        notice: Option<String>,
    ) -> Result<(), ProfileSetError> {
        if self.migrated(id) {
            return Ok(());
        }
        let before = self.index.clone();
        self.index.migrations.push(id.to_string());
        self.index.notices.extend(notice);
        if let Err(e) = self.save_index() {
            self.index = before;
            return Err(e);
        }
        Ok(())
    }

    /// Take the notices a session left, to show once at launch. They
    /// leave the index, which saves. When it does not save they show
    /// anyway, and again at the next launch.
    pub(crate) fn take_notices(&mut self) -> Vec<String> {
        if self.index.notices.is_empty() {
            return Vec::new();
        }
        let notices = std::mem::take(&mut self.index.notices);
        if let Err(e) = self.save_index() {
            tracing::error!(error = %e, "could not clear the launch notices in profiles.toml");
            self.index.notices.clone_from(&notices);
        }
        notices
    }

    /// Where you are in Get started, or None when it never opened.
    pub(crate) fn get_started(&self) -> Option<&GetStarted> {
        self.index.get_started.as_ref()
    }

    /// Keep where you are in Get started and save the index. An index
    /// that does not save keeps what it held.
    pub(crate) fn set_get_started(
        &mut self,
        at_launch: bool,
        done: Vec<String>,
    ) -> Result<(), ProfileSetError> {
        let before = self
            .index
            .get_started
            .replace(GetStarted { at_launch, done });
        if let Err(e) = self.save_index() {
            self.index.get_started = before;
            return Err(e);
        }
        Ok(())
    }

    /// How many days Vosh keeps a log, or None to keep it forever.
    pub(crate) fn keep_logs_days(&self) -> Option<u32> {
        self.index.keep_logs_days
    }

    /// Keep logs for `days`, or forever with None, and save the index.
    /// An index that does not save keeps what it held.
    pub(crate) fn set_keep_logs_days(&mut self, days: Option<u32>) -> Result<(), ProfileSetError> {
        let before = std::mem::replace(&mut self.index.keep_logs_days, days);
        if let Err(e) = self.save_index() {
            self.index.keep_logs_days = before;
            return Err(e);
        }
        Ok(())
    }

    /// Read the per-category scope map.
    pub(crate) fn scope(&self) -> &ScopeConfig {
        &self.index.scope
    }

    /// Replace the scope map and persist the index. Caller is
    /// responsible for re-persisting the active profile right after
    /// so values get re-written to the correct file (global vs
    /// per-profile).
    pub(crate) fn set_scope(&mut self, scope: ScopeConfig) -> Result<(), ProfileSetError> {
        self.index.scope = scope;
        self.save_index()?;
        Ok(())
    }
}

/// Write what a new install starts with in the app data folder `root`:
/// global.toml on Triad, and profiles/default.toml with every preset off.
/// When the profile file does not save, global.toml goes too, so the
/// folder still holds nothing of yours.
fn write_new_install(root: &Path) -> Result<(), ProfileSetError> {
    let global = GlobalConfig {
        theme: Some(NEW_INSTALL_THEME.to_string()),
        light_theme: Some(NEW_INSTALL_LIGHT_THEME.to_string()),
        ..GlobalConfig::default()
    };
    let mut profile = ProfileConfig::fresh();
    profile.ui.enabled_presets = vec![PRESETS_OFF.to_string()];
    let global_body = toml::to_string_pretty(&global)?;
    let profile_body = toml::to_string_pretty(&profile)?;
    let global_path = paths::global_path(root);
    crate::disk::atomic::write_with_backup(&global_path, &global_body)?;
    let profile_path = paths::profile_path(root, DEFAULT_PROFILE_NAME);
    if let Err(e) = crate::disk::atomic::write_with_backup(&profile_path, &profile_body) {
        let _ = std::fs::remove_file(&global_path);
        return Err(e.into());
    }
    Ok(())
}

/// True when the app data folder `root` holds no profiles.toml, no root
/// profile.toml, no global.toml and nothing in profiles/, which only a
/// new install does. A folder whose profiles.toml you deleted to recover
/// still holds its profile files, and keeps the theme it has.
fn holds_nothing_of_yours(root: &Path) -> bool {
    let profiles_empty = match std::fs::read_dir(paths::profiles_dir(root)) {
        Ok(mut entries) => entries.next().is_none(),
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    };
    profiles_empty
        && !paths::profiles_index_path(root).exists()
        && !paths::root_profile_path(root).exists()
        && !paths::global_path(root).exists()
}

/// The name Vosh shows for a profile. The reserved `default` profile
/// reads `Default`, and every other name shows as typed.
pub(crate) fn display_name(name: &str) -> String {
    if name == DEFAULT_PROFILE_NAME {
        "Default".to_string()
    } else {
        name.to_string()
    }
}

/// Profile names are filesystem-bound. Allow only a conservative set
/// of characters; reject empty or path-bound names so we never reach
/// outside the profiles/ directory.
pub(crate) fn sanitize_name(name: &str) -> Result<String, ProfileSetError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ProfileSetError::EmptyName);
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.starts_with('.') {
        return Err(ProfileSetError::InvalidName(trimmed.to_string()));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ' ')
    {
        return Err(ProfileSetError::InvalidName(trimmed.to_string()));
    }
    Ok(trimmed.to_string())
}

/// Write `source` to its file before a copy reads that file, when a
/// session plays it. Call with [`PERSIST_LOCK`] held across this and the
/// copy, so the copy reads what the flush wrote and no persist rewrites
/// the source mid copy.
async fn flush_before_copy(shared: &SharedState, source: &str) {
    // An open profile can run two seconds ahead of its file. After
    // `#profile reset` or `load` it is deliberately diverged, and the
    // copy takes the file as it stands.
    if let Some(open) = shared.open_profile(source).filter(|open| !open.held()) {
        persist_state(shared, &open).await;
    }
}

/// Keep the sessions in profiles.toml for the next launch to restore,
/// with the profile the selected one plays as active, see
/// [`ProfileSet::keep_sessions`]. Call with [`PERSIST_LOCK`] held, so no
/// step opens, closes or moves a session between the read of the
/// sessions and the save, and with no profile held, since each row takes
/// its session's connection lock. An index that does not save is logged,
/// and the next change to the sessions saves the list again.
pub(crate) async fn save_sessions(state: &SharedState) {
    let rows = state.session_rows();
    let active = rows
        .iter()
        .find(|row| row.selected)
        .and_then(|row| row.profile.clone());
    let (sessions, selected) = SessionEntry::list(&rows);
    if let Some(set) = state.profile_set.lock().await.as_mut() {
        if let Err(e) = set.keep_sessions(active.as_deref(), sessions, selected) {
            tracing::error!(error = %e, "could not save the sessions to profiles.toml");
        }
    }
}

/// The body of [`profile_create`].
///
/// [`profile_create`]: crate::ipc::profiles::profile_create
pub(crate) async fn create_profile(
    state: &SharedState,
    name: &str,
    copy_from: Option<&str>,
    auto_match: Option<AutoMatch>,
) -> Result<ProfileEntry, String> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    if let Some(source) = copy_from {
        // Read under the lock, which the wizard holds until it sets the
        // flag.
        if state
            .relaunch_pending
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return Err(COPY_MIGRATION_PENDING.into());
        }
        flush_before_copy(state, source).await;
    }
    let mut set = state.loaded_profile_set().await?;
    set.create_from(name, copy_from, auto_match)
        .map_err(|e| e.to_string())
}

/// Why a profile cannot be renamed between `migration_apply` and the
/// relaunch that finishes it, or while launch could not finish a wizard
/// run. The next launch writes each profile file the run names under the
/// name it had, and the renamed file would keep what the move took out.
const RENAME_MIGRATION_PENDING: &str =
    "Quit Vosh and open it again to finish the move to loadouts, then rename the profile.";

/// Why a profile cannot be copied in the same window. The copy would take
/// a file the next launch has yet to finish, or the live profile a copy of
/// it saves first, which still holds the items the move took out.
const COPY_MIGRATION_PENDING: &str =
    "Quit Vosh and open it again to finish the move to loadouts, then copy the profile.";

/// The body of [`profile_rename`].
///
/// [`profile_rename`]: crate::ipc::profiles::profile_rename
pub(crate) async fn rename_profile(
    state: &SharedState,
    old: &str,
    new: &str,
) -> Result<(), String> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    // Read under the lock, which the wizard holds until it sets the flag.
    if state
        .relaunch_pending
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return Err(RENAME_MIGRATION_PENDING.into());
    }
    let new = sanitize_name(new).map_err(|e| e.to_string())?;
    let open = state.open_profile(old);
    state
        .loaded_profile_set()
        .await?
        .rename(old, &new)
        .map_err(|e| e.to_string())?;
    // Every session on it plays it under the new name, and the custom
    // prompt draws that name. A restored session that has yet to open it
    // opens it under the new name.
    for open in open.into_iter().chain(state.waiting_on(old)) {
        open.lock().await.set_name(&new);
    }
    crate::loadouts::set::follow_profile_name(state, old, Some(&new)).await;
    Ok(())
}

/// The body of [`profile_delete`]. A profile a session plays stays, with
/// its file.
///
/// [`profile_delete`]: crate::ipc::profiles::profile_delete
pub(crate) async fn delete_profile(state: &SharedState, name: &str) -> Result<(), String> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    if state.open_profile(name).is_some() {
        return Err(ProfileSetError::CannotDeletePlayed(name.to_string()).to_string());
    }
    state
        .loaded_profile_set()
        .await?
        .delete(name)
        .map_err(|e| e.to_string())?;
    crate::loadouts::set::follow_profile_name(state, name, None).await;
    Ok(())
}

/// The body of [`profile_duplicate`].
///
/// [`profile_duplicate`]: crate::ipc::profiles::profile_duplicate
pub(crate) async fn duplicate_profile(
    state: &SharedState,
    source: &str,
    new: &str,
) -> Result<(), String> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    // Read under the lock, which the wizard holds until it sets the flag.
    if state
        .relaunch_pending
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return Err(COPY_MIGRATION_PENDING.into());
    }
    flush_before_copy(state, source).await;
    let mut set = state.loaded_profile_set().await?;
    set.duplicate(source, new).map_err(|e| e.to_string())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use tempfile::tempdir;

    use crate::profile::tests::james_like_set;

    #[test]
    fn creates_fresh_set_when_no_files_exist() {
        let dir = tempdir().unwrap();
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert_eq!(set.active_name(), DEFAULT_PROFILE_NAME);
        assert_eq!(set.list().len(), 1);
        assert!(paths::profiles_index_path(dir.path()).exists());
        assert!(paths::profiles_dir(dir.path()).exists());
    }

    #[test]
    fn an_empty_folder_starts_on_triad() {
        let dir = tempdir().unwrap();
        ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        let global = paths::global_path(dir.path());
        let text = std::fs::read_to_string(&global).unwrap();
        assert_eq!(text, "theme = \"triad\"\nlight_theme = \"rubric\"\n");
        // The next launch finds the folder in use and leaves it alone.
        std::fs::write(&global, "theme = \"nord\"\n").unwrap();
        ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert_eq!(
            std::fs::read_to_string(&global).unwrap(),
            "theme = \"nord\"\n"
        );
    }

    #[test]
    fn a_folder_in_use_keeps_its_theme() {
        // Bug 5 recovery deletes profiles.toml and leaves the profile
        // files, and the oldest builds kept one profile.toml at the root.
        for kept in ["profiles/default.toml", "profile.toml", "global.toml"] {
            let dir = tempdir().unwrap();
            let path = dir.path().join(kept);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "marker = 1\n").unwrap();
            ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
            let global = std::fs::read_to_string(paths::global_path(dir.path())).ok();
            let want = (kept == "global.toml").then(|| "marker = 1\n".to_string());
            assert_eq!(global, want, "{kept}");
        }
    }

    #[test]
    fn an_empty_folder_starts_with_every_preset_off() {
        let dir = tempdir().unwrap();
        ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        let path = paths::profile_path(dir.path(), DEFAULT_PROFILE_NAME);
        let config = ProfileConfig::load(&path).unwrap();
        assert_eq!(config.ui.enabled_presets, [PRESETS_OFF]);
        // Get started opens at launch, and the next launch reads it.
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        let open = GetStarted {
            at_launch: true,
            done: Vec::new(),
        };
        assert_eq!(set.get_started(), Some(&open));
    }

    #[test]
    fn get_started_keeps_the_steps_you_finished() {
        let dir = tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.set_get_started(false, vec!["connect".into()]).unwrap();
        let text = std::fs::read_to_string(paths::profiles_index_path(dir.path())).unwrap();
        assert!(
            text.ends_with("[get_started]\nat_launch = false\ndone = [\"connect\"]\n"),
            "{text}"
        );
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        let get_started = set.get_started().unwrap();
        assert!(!get_started.at_launch);
        assert_eq!(get_started.done, ["connect"]);
    }

    #[test]
    fn a_folder_with_a_profile_keeps_its_presets_and_get_started_shut() {
        // Bug 5 recovery leaves the profile files, and the oldest builds
        // kept one profile.toml at the root, which moves to default.toml.
        for (kept, default_text) in [
            ("profiles/foo.toml", None),
            ("profile.toml", Some("marker = 1\n")),
        ] {
            let dir = tempdir().unwrap();
            let path = dir.path().join(kept);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "marker = 1\n").unwrap();
            let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
            let default = paths::profile_path(dir.path(), DEFAULT_PROFILE_NAME);
            let text = std::fs::read_to_string(default).ok();
            assert_eq!(text.as_deref(), default_text, "{kept}");
            assert_eq!(set.get_started(), None, "{kept}");
        }
    }

    #[test]
    fn a_notice_a_session_leaves_shows_once_at_the_next_launch() {
        let dir = tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.record_with_notice("step", Some("Once.".to_string()))
            .unwrap();
        // A step that already ran leaves nothing more.
        set.record_with_notice("step", Some("Twice.".to_string()))
            .unwrap();
        let text = std::fs::read_to_string(paths::profiles_index_path(dir.path())).unwrap();
        assert!(text.contains("notices = [\"Once.\"]"), "{text}");

        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert!(set.migrated("step"));
        assert_eq!(set.take_notices(), ["Once."]);
        let leftover = &set.take_notices();
        assert!(leftover.is_empty(), "{leftover:?}");
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert!(set.index.notices.is_empty(), "the take saved");
        let text = std::fs::read_to_string(paths::profiles_index_path(dir.path())).unwrap();
        assert!(!text.contains("notices"), "{text}");

        // A step with nothing to say is still recorded.
        let mut set = set;
        set.record_with_notice("quiet", None).unwrap();
        assert!(set.migrated("quiet"));
        let leftover = &set.take_notices();
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn migrates_legacy_profile_toml() {
        let dir = tempdir().unwrap();
        let legacy = paths::root_profile_path(dir.path());
        std::fs::write(&legacy, "# pretend this is a profile\n").unwrap();

        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert_eq!(set.active_name(), DEFAULT_PROFILE_NAME);
        assert!(!legacy.exists(), "legacy file should be moved");
        let new = paths::profiles_dir(dir.path()).join(format!("{DEFAULT_PROFILE_NAME}.toml"));
        assert!(new.exists(), "should be at profiles/default.toml now");
    }

    #[test]
    fn create_rename_delete_round_trip() {
        let dir = tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.create("scratch").unwrap();
        assert_eq!(set.list().len(), 2);

        set.rename("scratch", "bench").unwrap();
        assert!(set.get("scratch").is_none());
        assert!(set.get("bench").is_some());

        set.delete("bench").unwrap();
        assert_eq!(set.list().len(), 1);
    }

    #[test]
    fn the_session_list_saves_and_follows_a_profile_rename() {
        let dir = tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.create("scratch").unwrap();
        let two = SessionId::numbered(2);
        let on = |profile: &str| SessionEntry {
            id: two,
            name: Some("Alt".into()),
            host: None,
            port: None,
            tls: false,
            profile: profile.into(),
        };
        set.keep_sessions(Some("scratch"), vec![on("scratch")], Some(two))
            .unwrap();
        set.rename("scratch", "bench").unwrap();
        let reloaded = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert_eq!(reloaded.index.sessions, [on("bench")]);
        assert_eq!(reloaded.index.selected, Some(two));
        assert_eq!(reloaded.active_name(), "bench");
    }

    #[test]
    fn cannot_delete_the_profile_a_session_plays() {
        let dir = tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        let err = set.delete(DEFAULT_PROFILE_NAME).unwrap_err();
        assert!(matches!(err, ProfileSetError::CannotDeletePlayed(_)));
        set.create("Build").unwrap();
        set.switch("Build").unwrap();
        assert_eq!(
            set.delete("Build").unwrap_err().to_string(),
            "A session plays Build. Switch that session to another profile or close it, then delete Build."
        );
    }

    #[test]
    fn errors_read_as_sentences() {
        let dir = tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert_eq!(
            set.delete(DEFAULT_PROFILE_NAME).unwrap_err().to_string(),
            "A session plays Default. Switch that session to another profile or close it, then delete Default."
        );
        assert_eq!(
            set.create("with:colon").unwrap_err().to_string(),
            "You cannot name a profile with:colon. Use letters, numbers, spaces, hyphens, and underscores."
        );
        assert_eq!(
            set.create("  ").unwrap_err().to_string(),
            "Give the profile a name."
        );
        assert_eq!(
            set.create(DEFAULT_PROFILE_NAME).unwrap_err().to_string(),
            "You already have a profile named Default."
        );
        assert_eq!(
            set.switch("Nobody").unwrap_err().to_string(),
            "Vosh cannot find a profile named Nobody."
        );
    }

    #[test]
    fn default_profile_displays_as_default() {
        assert_eq!(display_name(DEFAULT_PROFILE_NAME), "Default");
        assert_eq!(display_name("Test-Prompt"), "Test-Prompt");
    }

    fn read_profile(set: &ProfileSet, name: &str) -> String {
        std::fs::read_to_string(set.profile_path(name)).unwrap()
    }

    #[test]
    fn profile_names_compare_without_case() {
        let dir = tempdir().unwrap();
        let mut set = james_like_set(dir.path());
        std::fs::write(set.profile_path(DEFAULT_PROFILE_NAME), "marker = 1\n").unwrap();
        std::fs::write(set.profile_path("Healer"), "marker = 2\n").unwrap();

        // Characters shows default as Default, and a disk that ignores
        // case keeps Default.toml and default.toml as one file.
        for name in ["Default", "DEFAULT", "healer", " HEALER "] {
            assert!(
                matches!(
                    set.create_from(name, Some("Healer"), None),
                    Err(ProfileSetError::AlreadyExists(_))
                ),
                "create {name}"
            );
            assert!(
                matches!(
                    set.duplicate("Test-Prompt", name),
                    Err(ProfileSetError::AlreadyExists(_))
                ),
                "duplicate {name}"
            );
        }
        assert_eq!(
            set.rename("Healer", "Default").unwrap_err().to_string(),
            "You already have a profile named Default."
        );
        assert_eq!(
            set.rename("Test-Prompt", "HEALER").unwrap_err().to_string(),
            "You already have a profile named Healer."
        );

        // Nothing was written over or added.
        assert_eq!(read_profile(&set, DEFAULT_PROFILE_NAME), "marker = 1\n");
        assert_eq!(read_profile(&set, "Healer"), "marker = 2\n");
        assert_eq!(set.list().len(), 3);
        assert!(set.get("Healer").is_some());

        // A profile may change the case of its own name and keeps its
        // file.
        set.rename("Healer", "healer").unwrap();
        assert!(set.get("Healer").is_none());
        assert_eq!(read_profile(&set, "healer"), "marker = 2\n");
    }

    #[test]
    fn a_name_whose_file_is_on_disk_is_refused() {
        let dir = tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        std::fs::write(set.profile_path("Orphan"), "marker = 3\n").unwrap();
        assert_eq!(
            set.create("Orphan").unwrap_err().to_string(),
            "Your profiles folder already holds a file named Orphan.toml. Choose another name."
        );
        set.create("Kept").unwrap();
        assert!(matches!(
            set.rename("Kept", "Orphan"),
            Err(ProfileSetError::FileExists(_))
        ));
        assert!(matches!(
            set.duplicate(DEFAULT_PROFILE_NAME, "Orphan"),
            Err(ProfileSetError::FileExists(_))
        ));
        assert_eq!(read_profile(&set, "Orphan"), "marker = 3\n");
        assert!(set.get("Orphan").is_none());
        assert!(set.get("Kept").is_some());
    }

    #[test]
    fn create_from_without_a_source_starts_empty() {
        let dir = tempdir().unwrap();
        let mut set = james_like_set(dir.path());
        let entry = set.create_from("Fresh", None, None).unwrap();
        assert!(entry.description.is_none());
        assert!(entry.auto_match.is_none());
        assert!(!set.profile_path("Fresh").exists());
        assert!(matches!(
            set.create_from("Other", Some("Nobody"), None),
            Err(ProfileSetError::NotFound(_))
        ));
        assert!(matches!(
            set.create_from("Fresh", None, None),
            Err(ProfileSetError::AlreadyExists(_))
        ));
        assert!(set.get("Other").is_none());
    }

    /// New profile copies the profile you play, your preset edits with
    /// the list of presets that are on (Presets board 5).
    #[tokio::test]
    async fn a_new_profile_copies_the_preset_edits_with_the_list() {
        use crate::loadouts::preset_edits::lilac_line;
        let dir = tempdir().unwrap();
        let state: SharedState = std::sync::Arc::new(crate::app::state::AppState::default());
        state.set_profiles(james_like_set(dir.path())).await;
        {
            let mut p = state.selected_profile().await;
            p.ui.enabled_presets = vec!["disarm_buff_fade".into()];
            p.preset_edits = lilac_line();
        }
        create_profile(&state, "Orla", Some(DEFAULT_PROFILE_NAME), None)
            .await
            .unwrap();
        let set = state.loaded_profile_set().await.unwrap();
        let copy = ProfileConfig::load(&set.profile_path("Orla")).unwrap();
        assert_eq!(copy.ui.enabled_presets, ["disarm_buff_fade"]);
        assert_eq!(copy.preset_edits, lilac_line());
    }

    #[test]
    fn duplicate_copies_per_profile_file() {
        let dir = tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        let src_path = set.profile_path(DEFAULT_PROFILE_NAME);
        std::fs::write(&src_path, "marker = true\n").unwrap();

        set.duplicate(DEFAULT_PROFILE_NAME, "copy").unwrap();
        let dst = set.profile_path("copy");
        assert!(dst.exists());
        assert_eq!(std::fs::read_to_string(&dst).unwrap(), "marker = true\n");
    }

    #[test]
    fn switch_updates_active() {
        let dir = tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.create("alt").unwrap();
        set.switch("alt").unwrap();
        assert_eq!(set.active_name(), "alt");

        let reloaded = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert_eq!(reloaded.active_name(), "alt");
    }

    #[test]
    fn a_switch_whose_index_does_not_save_keeps_the_active_name() {
        let dir = tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.create("alt").unwrap();
        // A folder where the index writes its temporary file makes the
        // save fail.
        std::fs::create_dir(dir.path().join("profiles.toml.tmp")).unwrap();
        assert!(set.switch("alt").is_err());
        assert_eq!(set.active_name(), DEFAULT_PROFILE_NAME);
    }

    #[test]
    fn rejects_invalid_names() {
        let dir = tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert!(set.create("../escape").is_err());
        assert!(set.create("with/slash").is_err());
        assert!(set.create("").is_err());
        assert!(set.create("    ").is_err());
        assert!(set.create("with:colon").is_err());
    }

    #[test]
    fn a_hand_edited_keep_logs_days_reads_as_forever() {
        let load = |days: &str| {
            let dir = tempdir().unwrap();
            std::fs::write(
                paths::profiles_index_path(dir.path()),
                format!(
                    "active = \"Healer\"\nkeep_logs_days = {days}\n\n[[profile]]\nname = \"default\"\n\n[[profile]]\nname = \"Healer\"\n"
                ),
            )
            .unwrap();
            ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap()
        };
        for days in ["0", "45", "\"90\"", "-1"] {
            let set = load(days);
            assert_eq!(set.keep_logs_days(), None, "{days}");
            assert_eq!(set.active_name(), "Healer", "{days}");
            assert_eq!(set.list().len(), 2, "{days}");
        }
        assert_eq!(load("30").keep_logs_days(), Some(30));
    }
}

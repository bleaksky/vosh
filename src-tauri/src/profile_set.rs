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
//!   file is created on the first save).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// Callers outside this file still reach these here, until they point at
// crate::profile::shared.
pub(crate) use crate::profile::shared::{Scope, ScopeConfig};

// Callers outside this file still reach these here, until they point at
// crate::profile::worlds.
pub(crate) use crate::profile::worlds::{is_forsaken_lands, known_world, world_name};

// Callers outside this file still reach these here, until they point at
// crate::profile::login_match.
pub(crate) use crate::profile::login_match::{AutoMatch, LoginClaim};

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
    #[error("You cannot delete the profile you are using. Switch to another profile first.")]
    CannotDeleteActive(String),
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
}

const INDEX_FILENAME: &str = "profiles.toml";
const PROFILES_DIR: &str = "profiles";
const LEGACY_PROFILE_FILENAME: &str = "profile.toml";
const GLOBAL_FILENAME: &str = "global.toml";
pub(crate) const DEFAULT_PROFILE_NAME: &str = "default";

/// Live in-memory view of the profile collection. Held in `AppState`
/// behind a `Mutex` so commands can mutate it. The active in-memory
/// `Profile` is still the canonical runtime state; this struct is the
/// catalog around it.
#[derive(Debug)]
pub(crate) struct ProfileSet {
    root: PathBuf,
    pub(super) index: ProfilesIndex,
}

impl ProfileSet {
    /// Load (or migrate-and-load) the profile set rooted at the given
    /// app data directory. Always returns a valid set; on a fresh
    /// install it returns a single-entry "default" set whose
    /// profile file does not exist yet.
    pub(crate) fn load_or_migrate(root: PathBuf) -> Result<Self, ProfileSetError> {
        let index_path = root.join(INDEX_FILENAME);
        let profiles_dir = root.join(PROFILES_DIR);

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
        std::fs::create_dir_all(&profiles_dir)?;
        let legacy = root.join(LEGACY_PROFILE_FILENAME);
        if legacy.exists() {
            let target = profiles_dir.join(format!("{DEFAULT_PROFILE_NAME}.toml"));
            std::fs::rename(&legacy, &target)?;
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
        };
        let set = Self { root, index };
        set.save_index()?;
        Ok(set)
    }

    pub(crate) fn save_index(&self) -> Result<(), ProfileSetError> {
        let path = self.root.join(INDEX_FILENAME);
        let body = toml::to_string_pretty(&self.index)?;
        // Atomic write with rotating backups; same protection the
        // per-profile and global config files get. A botched index
        // write would orphan every profile, so the rollback safety
        // net matters here too.
        crate::profile_config::write_with_backup(&path, &body)?;
        Ok(())
    }

    pub(crate) fn active_name(&self) -> &str {
        &self.index.active
    }

    pub(crate) fn active_path(&self) -> PathBuf {
        self.profile_path(&self.index.active)
    }

    pub(crate) fn profile_path(&self, name: &str) -> PathBuf {
        self.root.join(PROFILES_DIR).join(format!("{name}.toml"))
    }

    /// Path to the shared global.toml. Holds UI preferences (theme,
    /// font, dock layout, keep-last, auto-update) that survive
    /// profile switches.
    pub(crate) fn global_path(&self) -> PathBuf {
        self.root.join(GLOBAL_FILENAME)
    }

    pub(crate) fn list(&self) -> &[ProfileEntry] {
        &self.index.profiles
    }

    pub(crate) fn get(&self, name: &str) -> Option<&ProfileEntry> {
        self.index.profiles.iter().find(|p| p.name == name)
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

    /// Delete a non-active profile's entry + per-profile file.
    pub(crate) fn delete(&mut self, name: &str) -> Result<(), ProfileSetError> {
        if name == self.index.active {
            return Err(ProfileSetError::CannotDeleteActive(name.to_string()));
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
            crate::profile_config::follow_unread(&old_path, &new_path);
        }
        if self.index.active == old {
            self.index.active.clone_from(&new);
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use tempfile::tempdir;

    use crate::profile::tests::set_with_profiles;
    // Tests in other files still reach it here, until they point at
    // crate::profile::tests.
    pub(crate) use crate::profile::tests::james_like_set;

    #[test]
    fn creates_fresh_set_when_no_files_exist() {
        let dir = tempdir().unwrap();
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert_eq!(set.active_name(), DEFAULT_PROFILE_NAME);
        assert_eq!(set.list().len(), 1);
        assert!(dir.path().join(INDEX_FILENAME).exists());
        assert!(dir.path().join(PROFILES_DIR).exists());
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
        let text = std::fs::read_to_string(dir.path().join(INDEX_FILENAME)).unwrap();
        assert!(text.contains("notices = [\"Once.\"]"), "{text}");

        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert!(set.migrated("step"));
        assert_eq!(set.take_notices(), ["Once."]);
        let leftover = &set.take_notices();
        assert!(leftover.is_empty(), "{leftover:?}");
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert!(set.index.notices.is_empty(), "the take saved");
        let text = std::fs::read_to_string(dir.path().join(INDEX_FILENAME)).unwrap();
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
        let legacy = dir.path().join(LEGACY_PROFILE_FILENAME);
        std::fs::write(&legacy, "# pretend this is a profile\n").unwrap();

        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert_eq!(set.active_name(), DEFAULT_PROFILE_NAME);
        assert!(!legacy.exists(), "legacy file should be moved");
        let new = dir
            .path()
            .join(PROFILES_DIR)
            .join(format!("{DEFAULT_PROFILE_NAME}.toml"));
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
    fn cannot_delete_active() {
        let dir = tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        let err = set.delete(DEFAULT_PROFILE_NAME).unwrap_err();
        assert!(matches!(err, ProfileSetError::CannotDeleteActive(_)));
    }

    #[test]
    fn errors_read_as_sentences() {
        let dir = tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert_eq!(
            set.delete(DEFAULT_PROFILE_NAME).unwrap_err().to_string(),
            "You cannot delete the profile you are using. Switch to another profile first."
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
        std::fs::create_dir(dir.path().join(format!("{INDEX_FILENAME}.tmp"))).unwrap();
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
    fn resolve_match_returns_none_when_no_profile_pins_host() {
        let set = set_with_profiles(vec![]);
        assert_eq!(set.resolve_match("a.b.c", 1234, None), None);
    }

    #[test]
    fn resolve_match_host_only_picks_host_match() {
        let set = set_with_profiles(vec![(
            DEFAULT_PROFILE_NAME,
            AutoMatch {
                host: Some("h".into()),
                port: None,
                characters: vec![],
                enabled: true,
            },
        )]);
        assert_eq!(
            set.resolve_match("h", 0, None),
            Some(DEFAULT_PROFILE_NAME.to_string())
        );
    }

    #[test]
    fn resolve_match_host_case_insensitive() {
        let set = set_with_profiles(vec![(
            DEFAULT_PROFILE_NAME,
            AutoMatch {
                host: Some("PLAY.example.com".into()),
                port: None,
                characters: vec![],
                enabled: true,
            },
        )]);
        assert!(set.resolve_match("play.EXAMPLE.com", 0, None).is_some());
    }

    #[test]
    fn resolve_match_with_pinned_character_skips_when_none_supplied() {
        // A profile that pins characters must NOT match when the caller
        // supplied no character: the Char.Status-driven path leans on
        // this so the pre-login resolver does not lock to a
        // character-pinned profile before Char.Status arrives.
        let set = set_with_profiles(vec![(
            DEFAULT_PROFILE_NAME,
            AutoMatch {
                host: Some("h".into()),
                port: None,
                characters: vec!["Ilsabet".into()],
                enabled: true,
            },
        )]);
        assert_eq!(set.resolve_match("h", 0, None), None);
        assert_eq!(
            set.resolve_match("h", 0, Some("Ilsabet")),
            Some(DEFAULT_PROFILE_NAME.to_string())
        );
    }

    #[test]
    fn resolve_match_character_pinned_beats_host_only() {
        // host-only profile + character-pinned profile on the same host
        // and the supplied character matches the pin → the pinned one
        // wins on score (host=1 vs host+char=3).
        let set = set_with_profiles(vec![
            (
                DEFAULT_PROFILE_NAME,
                AutoMatch {
                    host: Some("h".into()),
                    port: None,
                    characters: vec![],
                    enabled: true,
                },
            ),
            (
                "warrior",
                AutoMatch {
                    host: Some("h".into()),
                    port: None,
                    characters: vec!["Ilsabet".into()],
                    enabled: true,
                },
            ),
        ]);
        assert_eq!(
            set.resolve_match("h", 0, Some("Ilsabet")),
            Some("warrior".to_string())
        );
        // Without the character, the host-only profile wins.
        assert_eq!(
            set.resolve_match("h", 0, None),
            Some(DEFAULT_PROFILE_NAME.to_string())
        );
    }

    #[test]
    fn resolve_match_any_of_characters_list() {
        let set = set_with_profiles(vec![(
            DEFAULT_PROFILE_NAME,
            AutoMatch {
                host: Some("h".into()),
                port: None,
                characters: vec!["A".into(), "B".into(), "C".into()],
                enabled: true,
            },
        )]);
        assert!(set.resolve_match("h", 0, Some("a")).is_some());
        assert!(set.resolve_match("h", 0, Some("B")).is_some());
        assert!(set.resolve_match("h", 0, Some("D")).is_none());
    }

    #[test]
    fn resolve_match_port_pin_required_when_set() {
        let set = set_with_profiles(vec![(
            DEFAULT_PROFILE_NAME,
            AutoMatch {
                host: Some("h".into()),
                port: Some(1848),
                characters: vec![],
                enabled: true,
            },
        )]);
        assert!(set.resolve_match("h", 1848, None).is_some());
        // Wrong port: profile skipped.
        assert!(set.resolve_match("h", 4000, None).is_none());
    }
}

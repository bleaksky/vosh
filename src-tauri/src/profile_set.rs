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
//!     aabahran-erelei.toml
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

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum ProfileSetError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml parse error: {0}")]
    Deserialize(#[from] toml::de::Error),
    #[error("toml serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("profile `{0}` already exists")]
    AlreadyExists(String),
    #[error("profile `{0}` not found")]
    NotFound(String),
    #[error("cannot delete the active profile (`{0}`); switch first")]
    CannotDeleteActive(String),
    #[error("profile name cannot be empty")]
    EmptyName,
    #[error("profile name `{0}` is invalid")]
    InvalidName(String),
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

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AutoMatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Characters this profile claims. A connect call carrying any of
    /// these names matches (case-insensitive). Empty list means the
    /// profile is character-agnostic and matches purely on
    /// host (plus port when pinned). Persisted as a JSON array;
    /// legacy single-string `character: "Name"` shape is accepted on
    /// load and promoted to a one-element list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub characters: Vec<String>,
}

/// Custom deserializer that accepts both the legacy shape
/// (`character: "Name"`) and the new shape (`characters: ["A", "B"]`)
/// without making callers run a migration. Mirrors the same
/// dual-shape strategy used by multi-pattern triggers and tracked
/// affect labels.
impl<'de> Deserialize<'de> for AutoMatch {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default)]
            host: Option<String>,
            #[serde(default)]
            port: Option<u16>,
            #[serde(default)]
            character: Option<String>,
            #[serde(default)]
            characters: Vec<String>,
        }
        let raw = Raw::deserialize(deserializer)?;
        let mut characters = raw.characters;
        if let Some(c) = raw.character {
            let trimmed = c.trim();
            if !trimmed.is_empty() && !characters.iter().any(|x| x == trimmed) {
                characters.insert(0, trimmed.to_string());
            }
        }
        Ok(AutoMatch {
            host: raw.host,
            port: raw.port,
            characters,
        })
    }
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
}

/// Per-category scope choice. Per-profile fields move with the
/// active profile; global fields are shared across every profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Scope {
    /// Lives in `profiles/<active>.toml`. Changes when the active
    /// profile changes.
    #[default]
    Profile,
    /// Lives in `global.toml`. Identical across every profile.
    Global,
}

/// User-controllable mapping of UI categories to scope. `font`
/// covers both `font_family` and `font_size` since they always
/// move together visually.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(crate) struct ScopeConfig {
    #[serde(default = "scope_default_global")]
    pub theme: Scope,
    #[serde(default = "scope_default_global")]
    pub font: Scope,
    #[serde(default = "scope_default_global")]
    pub dock_layout: Scope,
    #[serde(default = "scope_default_global")]
    pub keep_last_command: Scope,
    #[serde(default = "scope_default_global")]
    pub auto_update: Scope,
}

fn scope_default_global() -> Scope {
    Scope::Global
}

impl Default for ScopeConfig {
    fn default() -> Self {
        Self {
            theme: Scope::Global,
            font: Scope::Global,
            dock_layout: Scope::Global,
            keep_last_command: Scope::Global,
            auto_update: Scope::Global,
        }
    }
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
    index: ProfilesIndex,
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

    /// Find the best-matching profile for a connect target plus optional
    /// character name. Pure function over the entry list so both the
    /// `profile_resolve_match` Tauri command and the Char.Status auto-
    /// switch path in the session loop can share the same scoring
    /// logic.
    ///
    /// Match rules. A profile's `auto_match.host` is required and must
    /// match case-insensitively. `port`, if specified on the profile,
    /// must equal the connect port. The `characters` list, if non-
    /// empty, requires `character` to be supplied AND to match (case-
    /// insensitively) at least one entry. Higher scores win:
    ///
    ///   - host match: 1
    ///   - port match: +1
    ///   - character match: +2
    ///
    /// So a profile pinned to (host, port, character) beats one pinned
    /// to (host, port) which beats one pinned to (host) alone.
    pub(crate) fn resolve_match(
        &self,
        host: &str,
        port: u16,
        character: Option<&str>,
    ) -> Option<String> {
        let host_l = host.trim().to_ascii_lowercase();
        let character_l = character.map(str::trim).map(str::to_ascii_lowercase);
        let mut best: Option<(&str, u8)> = None;
        for entry in &self.index.profiles {
            let Some(am) = &entry.auto_match else {
                continue;
            };
            let Some(am_host) = &am.host else { continue };
            if am_host.trim().to_ascii_lowercase() != host_l {
                continue;
            }
            if let Some(p) = am.port {
                if p != port {
                    continue;
                }
            }
            let mut score: u8 = 1;
            if am.port.is_some() {
                score += 1;
            }
            if !am.characters.is_empty() {
                let Some(connect_char) = &character_l else {
                    continue;
                };
                let any_match = am
                    .characters
                    .iter()
                    .any(|name| name.trim().to_ascii_lowercase() == *connect_char);
                if !any_match {
                    continue;
                }
                score += 2;
            }
            if best.map_or(true, |(_, b)| score > b) {
                best = Some((entry.name.as_str(), score));
            }
        }
        best.map(|(name, _)| name.to_string())
    }

    /// Create an empty entry. The per-profile file is created on the
    /// next save (so a brand-new profile inherits whatever defaults
    /// `ProfileConfig::default()` produces on first persist).
    pub(crate) fn create(&mut self, name: &str) -> Result<(), ProfileSetError> {
        let name = sanitize_name(name)?;
        if self.get(&name).is_some() {
            return Err(ProfileSetError::AlreadyExists(name));
        }
        self.index.profiles.push(ProfileEntry {
            name,
            description: None,
            auto_match: None,
        });
        self.save_index()?;
        Ok(())
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

    /// Rename an entry. Moves the per-profile file too.
    pub(crate) fn rename(&mut self, old: &str, new: &str) -> Result<(), ProfileSetError> {
        let new = sanitize_name(new)?;
        if self.get(&new).is_some() && new != old {
            return Err(ProfileSetError::AlreadyExists(new));
        }
        let Some(idx) = self.index.profiles.iter().position(|p| p.name == old) else {
            return Err(ProfileSetError::NotFound(old.to_string()));
        };
        let old_path = self.profile_path(old);
        let new_path = self.profile_path(&new);
        if old_path.exists() {
            std::fs::rename(&old_path, &new_path)?;
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
        let new = sanitize_name(new)?;
        if self.get(source).is_none() {
            return Err(ProfileSetError::NotFound(source.to_string()));
        }
        if self.get(&new).is_some() {
            return Err(ProfileSetError::AlreadyExists(new));
        }
        let src_path = self.profile_path(source);
        let dst_path = self.profile_path(&new);
        if src_path.exists() {
            std::fs::copy(&src_path, &dst_path)?;
        }
        let source_entry = self
            .get(source)
            .expect("source presence checked above")
            .clone();
        self.index.profiles.push(ProfileEntry {
            name: new,
            description: source_entry.description.clone(),
            // Auto-match deliberately NOT copied: the duplicate is
            // usually a starting point for a NEW character/MUD pairing.
            auto_match: None,
        });
        self.save_index()?;
        Ok(())
    }

    /// Set the active profile. Caller is responsible for writing the
    /// previous active profile to disk BEFORE calling switch (so the
    /// in-memory state is not lost).
    pub(crate) fn switch(&mut self, name: &str) -> Result<(), ProfileSetError> {
        if self.get(name).is_none() {
            return Err(ProfileSetError::NotFound(name.to_string()));
        }
        self.index.active = name.to_string();
        self.save_index()?;
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

    /// Update an entry's metadata (description, auto-match). Used by
    /// Stage 2's Settings UI; Stage 1 just exposes the plumbing.
    #[allow(dead_code)]
    pub(crate) fn set_metadata(
        &mut self,
        name: &str,
        description: Option<String>,
        auto_match: Option<AutoMatch>,
    ) -> Result<(), ProfileSetError> {
        let Some(entry) = self.index.profiles.iter_mut().find(|p| p.name == name) else {
            return Err(ProfileSetError::NotFound(name.to_string()));
        };
        entry.description = description;
        entry.auto_match = auto_match;
        self.save_index()?;
        Ok(())
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
mod tests {
    use super::*;
    use tempfile::tempdir;

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
    fn auto_match_accepts_legacy_single_character_shape() {
        // Older profile.toml files (pre-multi-character feature) saved
        // `character = "Erelei"`. Loading must still succeed and the
        // resulting struct must hold one entry in the new `characters`
        // list so the resolver treats it identically.
        let toml = r#"
host = "play.theforsakenlands.com"
port = 1848
character = "Erelei"
"#;
        let am: AutoMatch = toml::from_str(toml).unwrap();
        assert_eq!(am.host.as_deref(), Some("play.theforsakenlands.com"));
        assert_eq!(am.port, Some(1848));
        assert_eq!(am.characters, vec!["Erelei".to_string()]);
    }

    #[test]
    fn auto_match_accepts_characters_list_shape() {
        let toml = r#"
host = "play.theforsakenlands.com"
characters = ["Erelei", "Akletus", "Vanek"]
"#;
        let am: AutoMatch = toml::from_str(toml).unwrap();
        assert_eq!(am.characters, vec!["Erelei", "Akletus", "Vanek"]);
    }

    #[test]
    fn auto_match_merges_legacy_and_new_when_both_present() {
        // A hand-edited profile.toml could carry both fields; the
        // legacy `character` should be folded into the list without
        // duplicating an existing entry.
        let toml = r#"
host = "h"
character = "Erelei"
characters = ["Akletus", "Vanek"]
"#;
        let am: AutoMatch = toml::from_str(toml).unwrap();
        assert_eq!(am.characters, vec!["Erelei", "Akletus", "Vanek"]);

        let toml_with_dup = r#"
host = "h"
character = "Erelei"
characters = ["Erelei", "Vanek"]
"#;
        let am: AutoMatch = toml::from_str(toml_with_dup).unwrap();
        // Dedup keeps the existing position; legacy entry is not
        // re-inserted.
        assert_eq!(am.characters, vec!["Erelei", "Vanek"]);
    }

    #[test]
    fn auto_match_round_trips_through_toml() {
        let am = AutoMatch {
            host: Some("h".into()),
            port: Some(1848),
            characters: vec!["A".into(), "B".into()],
        };
        let text = toml::to_string_pretty(&am).unwrap();
        let parsed: AutoMatch = toml::from_str(&text).unwrap();
        assert_eq!(parsed.host, am.host);
        assert_eq!(parsed.port, am.port);
        assert_eq!(parsed.characters, am.characters);
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
    fn rejects_invalid_names() {
        let dir = tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert!(set.create("../escape").is_err());
        assert!(set.create("with/slash").is_err());
        assert!(set.create("").is_err());
        assert!(set.create("    ").is_err());
        assert!(set.create("with:colon").is_err());
    }

    fn set_with_profiles(profiles: Vec<(&str, AutoMatch)>) -> ProfileSet {
        let dir = tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        for (name, am) in profiles {
            if name != DEFAULT_PROFILE_NAME {
                set.create(name).unwrap();
            }
            set.set_metadata(name, None, Some(am)).unwrap();
        }
        set
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
                characters: vec!["Erelei".into()],
            },
        )]);
        assert_eq!(set.resolve_match("h", 0, None), None);
        assert_eq!(
            set.resolve_match("h", 0, Some("Erelei")),
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
                },
            ),
            (
                "warrior",
                AutoMatch {
                    host: Some("h".into()),
                    port: None,
                    characters: vec!["Erelei".into()],
                },
            ),
        ]);
        assert_eq!(
            set.resolve_match("h", 0, Some("Erelei")),
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
            },
        )]);
        assert!(set.resolve_match("h", 1848, None).is_some());
        // Wrong port: profile skipped.
        assert!(set.resolve_match("h", 4000, None).is_none());
    }
}

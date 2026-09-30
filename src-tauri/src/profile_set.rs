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
    /// The login toggle. Off keeps the world and the character names
    /// but stops `resolve_match` from picking this profile, so turning
    /// the toggle off never leaves a host-only entry that would become
    /// the whole world's fallback. On by default and left out of the
    /// file while on, so older files load unchanged.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub enabled: bool,
}

impl Default for AutoMatch {
    fn default() -> Self {
        Self {
            host: None,
            port: None,
            characters: Vec::new(),
            enabled: true,
        }
    }
}

fn default_true() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
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
            #[serde(default = "default_true")]
            enabled: bool,
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
            enabled: raw.enabled,
        })
    }
}

/// What turning a login toggle on or off did: the profile as it now
/// reads, and every profile the character was taken from.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct LoginClaim {
    pub entry: ProfileEntry,
    pub released_from: Vec<String>,
}

impl AutoMatch {
    /// Trim the host and the names, and drop blank names and names
    /// repeated in another case.
    fn cleaned(mut self) -> Self {
        self.host = self
            .host
            .map(|h| h.trim().to_string())
            .filter(|h| !h.is_empty());
        let mut seen = std::collections::HashSet::new();
        self.characters = self
            .characters
            .into_iter()
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty() && seen.insert(c.to_ascii_lowercase()))
            .collect();
        self
    }

    /// Whether this entry lists `character`, ignoring case.
    fn names(&self, character: &str) -> bool {
        let wanted = character.trim().to_ascii_lowercase();
        self.characters
            .iter()
            .any(|c| c.trim().to_ascii_lowercase() == wanted)
    }

    /// Whether `other` names the same world and the same characters in
    /// the same order, ignoring case. The toggle is not compared.
    fn same_claim(&self, other: &AutoMatch) -> bool {
        let same_host = match (self.host.as_deref(), other.host.as_deref()) {
            (Some(a), Some(b)) => a.trim().eq_ignore_ascii_case(b.trim()),
            (None, None) => true,
            _ => false,
        };
        same_host
            && self.port == other.port
            && self.characters.len() == other.characters.len()
            && self
                .characters
                .iter()
                .zip(&other.characters)
                .all(|(a, b)| a.trim().eq_ignore_ascii_case(b.trim()))
    }

    /// Whether this entry sits on the world at `host` and `port`. Hosts
    /// match without case, and a port pinned on only one side still
    /// matches, since either could log in there.
    fn on_world(&self, host: &str, port: Option<u16>) -> bool {
        let Some(own) = self.host.as_deref() else {
            return false;
        };
        own.trim().eq_ignore_ascii_case(host.trim())
            && match (self.port, port) {
                (Some(a), Some(b)) => a == b,
                _ => true,
            }
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

/// User-controllable mapping of UI categories to scope. `theme`
/// covers `theme`, the follow switch, the light and dark pair, and
/// `custom_themes`, so a custom theme travels with the theme that
/// names it. `font` covers `font_family`, `font_size`, and
/// `terminal_line_height` since they move together visually.
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

impl ScopeConfig {
    /// The categories `self` shares that `next` keeps per profile, marked
    /// `Global`, with every other category `Profile`. None when no
    /// category stops being shared.
    pub(crate) fn stopped_sharing(&self, next: &ScopeConfig) -> Option<ScopeConfig> {
        let stop = |was: Scope, now: Scope| {
            if was == Scope::Global && now == Scope::Profile {
                Scope::Global
            } else {
                Scope::Profile
            }
        };
        let stopped = ScopeConfig {
            theme: stop(self.theme, next.theme),
            font: stop(self.font, next.font),
            dock_layout: stop(self.dock_layout, next.dock_layout),
            keep_last_command: stop(self.keep_last_command, next.keep_last_command),
            auto_update: stop(self.auto_update, next.auto_update),
        };
        let any = [
            stopped.theme,
            stopped.font,
            stopped.dock_layout,
            stopped.keep_last_command,
            stopped.auto_update,
        ]
        .contains(&Scope::Global);
        any.then_some(stopped)
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
    /// to (host, port) which beats one pinned to (host) alone. An entry
    /// whose login toggle is off never matches.
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
            if !am.enabled {
                continue;
            }
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

    /// Whether the login toggle for `name` reads on: its first character
    /// logging in on its world would load this profile. That needs the
    /// toggle on, a world, a character, and `resolve_match` picking this
    /// profile over any other that claims the same character, so a
    /// profile that loses a tie in index order reads off. A profile
    /// that pins no port is checked at its known world's port.
    pub(crate) fn login_on(&self, name: &str) -> bool {
        let Some(am) = self.get(name).and_then(|e| e.auto_match.as_ref()) else {
            return false;
        };
        let (Some(host), Some(character)) = (am.host.as_deref(), am.characters.first()) else {
            return false;
        };
        if !am.enabled {
            return false;
        }
        let port = am
            .port
            .or_else(|| known_world(host).map(|w| w.port))
            .unwrap_or(0);
        self.resolve_match(host, port, Some(character)).as_deref() == Some(name)
    }

    /// The profile that claims `character` at login on this connection:
    /// the one `resolve_match` picks, when that profile lists the
    /// character. None when no profile with its toggle on lists it, even
    /// if a host wide fallback would load, so Characters can offer a
    /// new profile for that character.
    pub(crate) fn claimed_by(&self, host: &str, port: u16, character: &str) -> Option<String> {
        let name = self.resolve_match(host, port, Some(character))?;
        let am = self.get(&name)?.auto_match.as_ref()?;
        am.names(character).then_some(name)
    }

    /// Turn the login toggle for `name` on or off for `character`.
    ///
    /// On lists the character first if the profile does not list it
    /// yet, turns the toggle on, and takes the character away from
    /// every other profile on the same world, because a character
    /// belongs to one profile per world. A profile left with no
    /// characters has its toggle turned off so it does not become the
    /// host wide fallback. Off keeps the world and every name and only
    /// turns the toggle off. Either way the index is written once and
    /// nothing switches, since the toggle applies at the next login.
    pub(crate) fn set_login(
        &mut self,
        name: &str,
        character: &str,
        on: bool,
    ) -> Result<LoginClaim, ProfileSetError> {
        let character = character.trim();
        let Some(idx) = self.index.profiles.iter().position(|p| p.name == name) else {
            return Err(ProfileSetError::NotFound(name.to_string()));
        };
        let before = self.index.profiles.clone();
        let mut released_from = Vec::new();
        if on {
            if character.is_empty() {
                return Err(ProfileSetError::NoCharacter);
            }
            let world = self.index.profiles[idx].auto_match.as_ref().and_then(|am| {
                let host = am.host.as_deref()?.trim();
                (!host.is_empty()).then(|| (host.to_string(), am.port))
            });
            let Some((host, port)) = world else {
                return Err(ProfileSetError::NoWorld(display_name(name)));
            };
            self.release_character(idx, &host, port, character, &mut released_from);
            let am = self.index.profiles[idx]
                .auto_match
                .as_mut()
                .expect("the world check found auto_match");
            if !am.names(character) {
                am.characters.insert(0, character.to_string());
            }
            am.enabled = true;
        } else if let Some(am) = self.index.profiles[idx].auto_match.as_mut() {
            am.enabled = false;
        }
        self.save_profiles_or_restore(before)?;
        Ok(LoginClaim {
            entry: self.index.profiles[idx].clone(),
            released_from,
        })
    }

    /// Take `character` from every profile but the one at `keep` whose
    /// claim sits on the world at `host` and `port`, because a character
    /// belongs to one profile per world. A profile left with no
    /// characters has its toggle turned off so it does not become the
    /// host wide fallback. Adds each profile it took the character from
    /// to `released_from` once. Writes nothing, so the caller saves the
    /// index.
    fn release_character(
        &mut self,
        keep: usize,
        host: &str,
        port: Option<u16>,
        character: &str,
        released_from: &mut Vec<String>,
    ) {
        let wanted = character.trim().to_ascii_lowercase();
        for (i, other) in self.index.profiles.iter_mut().enumerate() {
            let Some(am) = other.auto_match.as_mut() else {
                continue;
            };
            if i == keep || !am.on_world(host, port) || !am.names(character) {
                continue;
            }
            am.characters
                .retain(|c| c.trim().to_ascii_lowercase() != wanted);
            if am.characters.is_empty() {
                am.enabled = false;
            }
            if !released_from.contains(&other.name) {
                released_from.push(other.name.clone());
            }
        }
    }

    /// Save the index after a change to its entries. When the save
    /// fails, put `before` back so memory keeps matching the file.
    fn save_profiles_or_restore(
        &mut self,
        before: Vec<ProfileEntry>,
    ) -> Result<(), ProfileSetError> {
        let saved = self.save_index();
        if saved.is_err() {
            self.index.profiles = before;
        }
        saved
    }

    /// Point `name` at a world, editing only the host and port of its
    /// login claim, so the description and the characters another
    /// window may hold stay as they are. A blank host clears it. A
    /// profile that had no claim gets one with its toggle off, since a
    /// world with no character must not become the fallback for every
    /// login there. A claim left with no host, port or character goes
    /// away.
    pub(crate) fn set_world(
        &mut self,
        name: &str,
        host: Option<String>,
        port: Option<u16>,
    ) -> Result<ProfileEntry, ProfileSetError> {
        let Some(entry) = self.index.profiles.iter_mut().find(|p| p.name == name) else {
            return Err(ProfileSetError::NotFound(name.to_string()));
        };
        let host = host.map(|h| h.trim().to_string()).filter(|h| !h.is_empty());
        let am = entry.auto_match.get_or_insert_with(|| AutoMatch {
            enabled: false,
            ..AutoMatch::default()
        });
        am.host = host;
        am.port = port;
        if am.host.is_none() && am.port.is_none() && am.characters.is_empty() {
            entry.auto_match = None;
        }
        let entry = entry.clone();
        self.save_index()?;
        Ok(entry)
    }

    /// Create an empty entry. The per-profile file is created on the
    /// next save (so a brand-new profile inherits whatever defaults
    /// `ProfileConfig::default()` produces on first persist). A test
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

    /// Replace an entry's description and login claim. The claim follows
    /// the rules the login toggle follows, so no caller can bring back a
    /// double claim or a host wide fallback.
    ///
    /// - A claim that names the same world and characters as the saved
    ///   one, with its toggle on, stays as saved. A description edit
    ///   sends the claim back unchanged, and the old Profiles tab sends
    ///   no toggle at all, which reads as on, so this keeps it from
    ///   turning a toggle back on or taking a character from the
    ///   profile that wins the login.
    /// - A claim with no characters has its toggle turned off.
    /// - A claim with its toggle on takes each of its characters from
    ///   every other profile on the same world, as [`Self::set_login`]
    ///   does, and names them in `released_from`.
    /// - A claim with no world, port or character goes away.
    ///
    /// Writes the index once and never switches.
    pub(crate) fn set_metadata(
        &mut self,
        name: &str,
        description: Option<String>,
        auto_match: Option<AutoMatch>,
    ) -> Result<LoginClaim, ProfileSetError> {
        let Some(idx) = self.index.profiles.iter().position(|p| p.name == name) else {
            return Err(ProfileSetError::NotFound(name.to_string()));
        };
        let before = self.index.profiles.clone();
        let stored = before[idx].auto_match.clone();
        let mut released_from = Vec::new();
        let claim = match auto_match.map(AutoMatch::cleaned) {
            None => None,
            Some(am) if am.host.is_none() && am.port.is_none() && am.characters.is_empty() => None,
            Some(am) if am.enabled && stored.as_ref().is_some_and(|s| s.same_claim(&am)) => stored,
            Some(mut am) => {
                if am.characters.is_empty() {
                    am.enabled = false;
                }
                if let (true, Some(host)) = (am.enabled, am.host.clone()) {
                    for character in &am.characters {
                        self.release_character(idx, &host, am.port, character, &mut released_from);
                    }
                }
                Some(am)
            }
        };
        let entry = &mut self.index.profiles[idx];
        entry.description = description;
        entry.auto_match = claim;
        let entry = entry.clone();
        self.save_profiles_or_restore(before)?;
        Ok(LoginClaim {
            entry,
            released_from,
        })
    }
}

/// A world Vosh knows by name. Mirrors `KNOWN_WORLDS` in
/// src/lib/useConnection.ts.
pub(crate) struct KnownWorld {
    /// A host matches this domain or any subdomain of it.
    pub domain: &'static str,
    pub name: &'static str,
    /// The port you connect to it on.
    pub port: u16,
}

/// The domain of The Forsaken Lands.
const FORSAKEN_LANDS: &str = "theforsakenlands.com";

pub(crate) const KNOWN_WORLDS: &[KnownWorld] = &[KnownWorld {
    domain: FORSAKEN_LANDS,
    name: "The Forsaken Lands",
    port: 1848,
}];

/// The known world a host belongs to, if any.
pub(crate) fn known_world(host: &str) -> Option<&'static KnownWorld> {
    let lower = host.trim().to_ascii_lowercase();
    let clean = lower.strip_suffix('.').unwrap_or(&lower);
    KNOWN_WORLDS.iter().find(|w| {
        clean == w.domain
            || clean
                .strip_suffix(w.domain)
                .is_some_and(|rest| rest.ends_with('.'))
    })
}

/// True when the host is The Forsaken Lands, so the custom prompt
/// follows Aabahran's rules there, the hidden values of lamented tears
/// among them.
pub(crate) fn is_forsaken_lands(host: &str) -> bool {
    known_world(host).is_some_and(|w| w.domain == FORSAKEN_LANDS)
}

/// The name Vosh shows for a host, like `The Forsaken Lands` for
/// `play.theforsakenlands.com`. Unknown hosts show as typed. Mirrors
/// `worldName` in src/lib/useConnection.ts.
pub(crate) fn world_name(host: &str) -> String {
    known_world(host).map_or_else(|| host.trim().to_string(), |w| w.name.to_string())
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
            enabled: true,
        };
        let text = toml::to_string_pretty(&am).unwrap();
        let parsed: AutoMatch = toml::from_str(&text).unwrap();
        assert_eq!(parsed.host, am.host);
        assert_eq!(parsed.port, am.port);
        assert_eq!(parsed.characters, am.characters);
    }

    #[test]
    fn auto_match_login_toggle_defaults_on_and_stays_out_of_the_file_while_on() {
        let am: AutoMatch = toml::from_str("host = \"h\"\ncharacters = [\"Erelei\"]\n").unwrap();
        assert!(am.enabled, "files written before the toggle load as on");
        let text = toml::to_string_pretty(&am).unwrap();
        assert!(!text.contains("enabled"), "{text}");

        let off = AutoMatch {
            enabled: false,
            ..am
        };
        let text = toml::to_string_pretty(&off).unwrap();
        assert!(text.contains("enabled = false"), "{text}");
        let parsed: AutoMatch = toml::from_str(&text).unwrap();
        assert!(!parsed.enabled);
        assert_eq!(parsed.characters, vec!["Erelei"]);
        assert_eq!(parsed.host.as_deref(), Some("h"));
    }

    #[test]
    fn resolve_match_skips_an_entry_whose_login_toggle_is_off() {
        let set = set_with_profiles(vec![
            (
                DEFAULT_PROFILE_NAME,
                AutoMatch {
                    host: Some("h".into()),
                    port: Some(1848),
                    characters: vec!["Erelei".into()],
                    enabled: false,
                },
            ),
            (
                "fallback",
                AutoMatch {
                    host: Some("h".into()),
                    port: None,
                    characters: vec![],
                    enabled: false,
                },
            ),
        ]);
        assert_eq!(set.resolve_match("h", 1848, Some("Erelei")), None);
        // A host-only entry that is off is no fallback at connect.
        assert_eq!(set.resolve_match("h", 1848, None), None);
    }

    #[test]
    fn the_forsaken_lands_rules_hold_on_its_hosts_alone() {
        assert!(is_forsaken_lands("play.theforsakenlands.com"));
        assert!(is_forsaken_lands(" TheForsakenLands.com. "));
        assert!(!is_forsaken_lands("127.0.0.1"));
        assert!(!is_forsaken_lands("nottheforsakenlands.com"));
    }

    #[test]
    fn world_name_knows_the_forsaken_lands_by_any_subdomain() {
        assert_eq!(
            world_name("play.theforsakenlands.com"),
            "The Forsaken Lands"
        );
        assert_eq!(world_name(" TheForsakenLands.com. "), "The Forsaken Lands");
        assert_eq!(world_name("mud.example.org"), "mud.example.org");
        assert_eq!(
            world_name("nottheforsakenlands.com"),
            "nottheforsakenlands.com"
        );
        let world = known_world("play.theforsakenlands.com").unwrap();
        assert_eq!(world.port, 1848);
    }

    pub(crate) fn claim(host: &str, port: Option<u16>, characters: &[&str]) -> AutoMatch {
        AutoMatch {
            host: Some(host.into()),
            port,
            characters: characters.iter().map(ToString::to_string).collect(),
            enabled: true,
        }
    }

    /// Write `name`'s description and claim as they stand, the way an
    /// index saved before the login rules holds them, so a test can set
    /// up a double claim or a host wide fallback.
    pub(crate) fn put_claim(
        set: &mut ProfileSet,
        name: &str,
        description: Option<&str>,
        auto_match: AutoMatch,
    ) {
        let entry = set
            .index
            .profiles
            .iter_mut()
            .find(|p| p.name == name)
            .unwrap();
        entry.description = description.map(ToString::to_string);
        entry.auto_match = Some(auto_match);
        set.save_index().unwrap();
    }

    /// James's index: default and Test-Prompt both claim Erelei on the
    /// same world, and Healer claims Caelaor.
    pub(crate) fn james_like_set(dir: &std::path::Path) -> ProfileSet {
        let mut set = ProfileSet::load_or_migrate(dir.to_path_buf()).unwrap();
        let world = "play.theforsakenlands.com";
        put_claim(
            &mut set,
            DEFAULT_PROFILE_NAME,
            Some("Immortal"),
            claim(world, Some(1848), &["Erelei"]),
        );
        set.create("Healer").unwrap();
        put_claim(
            &mut set,
            "Healer",
            None,
            claim(world, Some(1848), &["Caelaor"]),
        );
        set.create("Test-Prompt").unwrap();
        put_claim(
            &mut set,
            "Test-Prompt",
            None,
            claim(world, Some(1848), &["Erelei"]),
        );
        set
    }

    #[test]
    fn login_on_reads_on_only_for_the_profile_that_wins_the_login() {
        let dir = tempdir().unwrap();
        let mut set = james_like_set(dir.path());
        assert!(set.login_on(DEFAULT_PROFILE_NAME));
        assert!(set.login_on("Healer"));
        // Test-Prompt claims Erelei too but loses the tie in index order.
        assert!(!set.login_on("Test-Prompt"));

        // Off, or with no world or no character, reads off.
        let mut off = claim("play.theforsakenlands.com", Some(1848), &["Caelaor"]);
        off.enabled = false;
        set.set_metadata("Healer", None, Some(off)).unwrap();
        assert!(!set.login_on("Healer"));
        set.create("Blank").unwrap();
        assert!(!set.login_on("Blank"));
        set.set_metadata("Blank", None, Some(claim("h", None, &[])))
            .unwrap();
        assert!(!set.login_on("Blank"));
        assert!(!set.login_on("Nobody"));
    }

    #[test]
    fn login_on_checks_a_portless_claim_at_its_known_world_port() {
        let dir = tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        put_claim(
            &mut set,
            DEFAULT_PROFILE_NAME,
            None,
            claim("play.theforsakenlands.com", None, &["Vanek"]),
        );
        assert!(set.login_on(DEFAULT_PROFILE_NAME));
        // A profile pinned to the real port outscores it at login.
        set.create("Pinned").unwrap();
        put_claim(
            &mut set,
            "Pinned",
            None,
            claim("play.theforsakenlands.com", Some(1848), &["Vanek"]),
        );
        assert!(!set.login_on(DEFAULT_PROFILE_NAME));
        assert!(set.login_on("Pinned"));
    }

    fn characters_of(set: &ProfileSet, name: &str) -> Vec<String> {
        set.get(name)
            .and_then(|e| e.auto_match.as_ref())
            .map(|am| am.characters.clone())
            .unwrap_or_default()
    }

    fn enabled(set: &ProfileSet, name: &str) -> bool {
        set.get(name)
            .and_then(|e| e.auto_match.as_ref())
            .is_some_and(|am| am.enabled)
    }

    #[test]
    fn turning_login_on_takes_erelei_from_every_other_profile_on_the_world() {
        let dir = tempdir().unwrap();
        let mut set = james_like_set(dir.path());
        let claim = set.set_login("Test-Prompt", "erelei", true).unwrap();
        assert_eq!(claim.released_from, vec![DEFAULT_PROFILE_NAME.to_string()]);
        assert_eq!(characters_of(&set, "Test-Prompt"), vec!["Erelei"]);
        assert!(claim.entry.auto_match.as_ref().unwrap().enabled);

        // default kept its world but lost its only character, so its
        // toggle went off rather than leaving a host wide fallback.
        assert!(characters_of(&set, DEFAULT_PROFILE_NAME).is_empty());
        assert!(!enabled(&set, DEFAULT_PROFILE_NAME));
        let am = set.get(DEFAULT_PROFILE_NAME).unwrap().auto_match.clone();
        assert_eq!(
            am.unwrap().host.as_deref(),
            Some("play.theforsakenlands.com")
        );
        assert_eq!(
            set.resolve_match("play.theforsakenlands.com", 1848, None),
            None
        );

        // Erelei now loads Test-Prompt, Caelaor still loads Healer, and
        // nothing switched.
        assert_eq!(
            set.resolve_match("play.theforsakenlands.com", 1848, Some("Erelei")),
            Some("Test-Prompt".into())
        );
        assert!(set.login_on("Test-Prompt"));
        assert!(!set.login_on(DEFAULT_PROFILE_NAME));
        assert!(set.login_on("Healer"));
        assert_eq!(set.active_name(), DEFAULT_PROFILE_NAME);

        // The index on disk took the whole change.
        let reloaded = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert!(reloaded.login_on("Test-Prompt"));
        assert!(!enabled(&reloaded, DEFAULT_PROFILE_NAME));
    }

    #[test]
    fn turning_login_on_keeps_other_characters_and_other_worlds() {
        let dir = tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        let world = "play.theforsakenlands.com";
        put_claim(
            &mut set,
            DEFAULT_PROFILE_NAME,
            None,
            claim(world, Some(1848), &["Erelei", "Akletus"]),
        );
        set.create("Elsewhere").unwrap();
        put_claim(
            &mut set,
            "Elsewhere",
            None,
            claim("mud.example.org", None, &["Erelei"]),
        );
        set.create("Portless").unwrap();
        put_claim(&mut set, "Portless", None, claim(world, None, &["Erelei"]));
        set.create("New").unwrap();
        put_claim(&mut set, "New", None, claim(world, Some(1848), &[]));

        let claim = set.set_login("New", "Erelei", true).unwrap();
        assert_eq!(
            claim.released_from,
            vec![DEFAULT_PROFILE_NAME.to_string(), "Portless".to_string()]
        );
        // default keeps Akletus and its toggle.
        assert_eq!(characters_of(&set, DEFAULT_PROFILE_NAME), vec!["Akletus"]);
        assert!(enabled(&set, DEFAULT_PROFILE_NAME));
        // Another world keeps its own Erelei.
        assert_eq!(characters_of(&set, "Elsewhere"), vec!["Erelei"]);
        assert_eq!(characters_of(&set, "New"), vec!["Erelei"]);
    }

    #[test]
    fn turning_login_off_keeps_the_world_and_the_name() {
        let dir = tempdir().unwrap();
        let mut set = james_like_set(dir.path());
        let claim = set.set_login("Healer", "Caelaor", false).unwrap();
        assert!(claim.released_from.is_empty());
        let am = claim.entry.auto_match.unwrap();
        assert!(!am.enabled);
        assert_eq!(am.characters, vec!["Caelaor"]);
        assert_eq!(am.host.as_deref(), Some("play.theforsakenlands.com"));
        assert_eq!(am.port, Some(1848));
        assert!(!set.login_on("Healer"));
        assert_eq!(
            set.resolve_match("play.theforsakenlands.com", 1848, Some("Caelaor")),
            None
        );
        // On again restores it without taking anything from anyone.
        let claim = set.set_login("Healer", "Caelaor", true).unwrap();
        assert!(claim.released_from.is_empty());
        assert!(set.login_on("Healer"));
    }

    #[test]
    fn turning_login_on_needs_a_character_and_a_world() {
        let dir = tempdir().unwrap();
        let mut set = james_like_set(dir.path());
        set.create("Blank").unwrap();
        assert!(matches!(
            set.set_login("Blank", "Erelei", true),
            Err(ProfileSetError::NoWorld(_))
        ));
        assert!(set.get("Blank").unwrap().auto_match.is_none());
        assert!(matches!(
            set.set_login("Healer", "  ", true),
            Err(ProfileSetError::NoCharacter)
        ));
        assert!(matches!(
            set.set_login("Nobody", "Erelei", true),
            Err(ProfileSetError::NotFound(_))
        ));
        assert_eq!(
            set.set_login("Blank", "Erelei", true)
                .unwrap_err()
                .to_string(),
            "Choose a world for Blank first."
        );
        // Nothing was taken from default along the way.
        assert!(set.login_on(DEFAULT_PROFILE_NAME));
    }

    #[test]
    fn set_world_edits_only_the_host_and_port() {
        let dir = tempdir().unwrap();
        let mut set = james_like_set(dir.path());
        let entry = set
            .set_world(
                DEFAULT_PROFILE_NAME,
                Some(" mud.example.org ".into()),
                Some(4000),
            )
            .unwrap();
        assert_eq!(entry.description.as_deref(), Some("Immortal"));
        let am = entry.auto_match.unwrap();
        assert_eq!(am.host.as_deref(), Some("mud.example.org"));
        assert_eq!(am.port, Some(4000));
        assert_eq!(am.characters, vec!["Erelei"]);
        assert!(am.enabled);
        // Erelei on the old world now loads Test-Prompt.
        assert_eq!(
            set.resolve_match("play.theforsakenlands.com", 1848, Some("Erelei")),
            Some("Test-Prompt".into())
        );
        let reloaded = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        let am = reloaded
            .get(DEFAULT_PROFILE_NAME)
            .unwrap()
            .auto_match
            .clone();
        assert_eq!(am.unwrap().port, Some(4000));
    }

    #[test]
    fn set_world_on_a_new_profile_leaves_its_toggle_off() {
        let dir = tempdir().unwrap();
        let mut set = james_like_set(dir.path());
        set.create("Blank").unwrap();
        let entry = set
            .set_world(
                "Blank",
                Some("play.theforsakenlands.com".into()),
                Some(1848),
            )
            .unwrap();
        let am = entry.auto_match.unwrap();
        assert!(!am.enabled);
        assert!(am.characters.is_empty());
        // No fallback appeared for logins with no claim.
        assert_eq!(
            set.resolve_match("play.theforsakenlands.com", 1848, None),
            None
        );
        // Clearing the world of a profile with no character drops the
        // claim, and an unknown name is an error.
        let entry = set.set_world("Blank", Some("  ".into()), None).unwrap();
        assert!(entry.auto_match.is_none());
        assert!(set.set_world("Nobody", None, None).is_err());
    }

    #[test]
    fn create_from_copies_the_source_and_takes_the_claim_as_given() {
        let dir = tempdir().unwrap();
        let mut set = james_like_set(dir.path());
        std::fs::write(set.profile_path(DEFAULT_PROFILE_NAME), "marker = true\n").unwrap();

        let mut seed = claim(
            " play.theforsakenlands.com ",
            Some(1848),
            &[" Caelaor ", "caelaor", ""],
        );
        seed.enabled = true;
        let entry = set
            .create_from(" Caelaor ", Some(DEFAULT_PROFILE_NAME), Some(seed))
            .unwrap();
        assert_eq!(entry.name, "Caelaor");
        assert_eq!(entry.description.as_deref(), Some("Immortal"));
        let am = entry.auto_match.unwrap();
        assert_eq!(am.host.as_deref(), Some("play.theforsakenlands.com"));
        assert_eq!(am.characters, vec!["Caelaor"]);
        assert_eq!(
            std::fs::read_to_string(set.profile_path("Caelaor")).unwrap(),
            "marker = true\n"
        );
        // Creating claims nothing away: Healer keeps Caelaor and still
        // wins the tie in index order.
        assert_eq!(characters_of(&set, "Healer"), vec!["Caelaor"]);
        assert!(set.login_on("Healer"));
        assert!(!set.login_on("Caelaor"));
        assert_eq!(set.active_name(), DEFAULT_PROFILE_NAME);
        let reloaded = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert!(reloaded.get("Caelaor").is_some());
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

    #[test]
    fn claimed_by_names_the_profile_a_login_loads() {
        let dir = tempdir().unwrap();
        let mut set = james_like_set(dir.path());
        let world = "play.theforsakenlands.com";
        assert_eq!(
            set.claimed_by(world, 1848, "erelei"),
            Some(DEFAULT_PROFILE_NAME.into())
        );
        assert_eq!(
            set.claimed_by(world, 1848, "Caelaor"),
            Some("Healer".into())
        );
        set.set_login("Test-Prompt", "Erelei", true).unwrap();
        assert_eq!(
            set.claimed_by(world, 1848, "Erelei"),
            Some("Test-Prompt".into())
        );

        // A host wide fallback from an older index loads for Vanek but
        // does not claim him.
        set.create("Fallback").unwrap();
        put_claim(&mut set, "Fallback", None, claim(world, None, &[]));
        assert_eq!(
            set.resolve_match(world, 1848, Some("Vanek")),
            Some("Fallback".into())
        );
        assert_eq!(set.claimed_by(world, 1848, "Vanek"), None);
        assert_eq!(set.claimed_by("mud.example.org", 4000, "Erelei"), None);
    }

    #[test]
    fn metadata_takes_a_new_character_from_every_other_profile_on_the_world() {
        let dir = tempdir().unwrap();
        let mut set = james_like_set(dir.path());
        let world = "play.theforsakenlands.com";
        // The old Profiles tab sends a claim with no toggle, which reads
        // as on.
        let result = set
            .set_metadata(
                "Healer",
                Some("Both".into()),
                Some(claim(world, Some(1848), &["Caelaor", "erelei"])),
            )
            .unwrap();
        assert_eq!(
            result.released_from,
            vec![DEFAULT_PROFILE_NAME.to_string(), "Test-Prompt".to_string()]
        );
        assert!(result.entry.auto_match.as_ref().unwrap().enabled);
        assert_eq!(characters_of(&set, "Healer"), vec!["Caelaor", "erelei"]);
        assert_eq!(
            set.get("Healer").unwrap().description.as_deref(),
            Some("Both")
        );
        // default and Test-Prompt lost their only character, so their
        // toggles went off and neither is a host wide fallback.
        for name in [DEFAULT_PROFILE_NAME, "Test-Prompt"] {
            assert!(characters_of(&set, name).is_empty(), "{name}");
            assert!(!enabled(&set, name), "{name}");
        }
        assert_eq!(
            set.resolve_match(world, 1848, Some("Erelei")),
            Some("Healer".into())
        );
        assert_eq!(set.resolve_match(world, 1848, Some("Vanek")), None);
        let reloaded = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert!(!enabled(&reloaded, DEFAULT_PROFILE_NAME));
        assert!(reloaded.login_on("Healer"));
    }

    #[test]
    fn metadata_turns_off_a_claim_with_no_character() {
        let dir = tempdir().unwrap();
        let mut set = james_like_set(dir.path());
        let world = "play.theforsakenlands.com";
        set.create("Blank").unwrap();
        set.set_metadata("Blank", None, Some(claim(world, None, &[])))
            .unwrap();
        let am = set.get("Blank").unwrap().auto_match.clone().unwrap();
        assert_eq!(am.host.as_deref(), Some(world));
        assert!(!am.enabled);
        // No fallback appeared for logins nobody claims.
        assert_eq!(set.resolve_match(world, 1848, None), None);
        assert_eq!(set.resolve_match(world, 1848, Some("Vanek")), None);

        // Clearing a character list turns the toggle off and keeps the
        // world.
        set.set_metadata("Healer", None, Some(claim(world, Some(1848), &[" "])))
            .unwrap();
        assert!(!enabled(&set, "Healer"));
        assert_eq!(set.resolve_match(world, 1848, Some("Caelaor")), None);

        // A claim with nothing in it goes away.
        let empty = AutoMatch {
            host: Some("  ".into()),
            ..AutoMatch::default()
        };
        set.set_metadata("Blank", None, Some(empty)).unwrap();
        assert!(set.get("Blank").unwrap().auto_match.is_none());
    }

    #[test]
    fn metadata_keeps_an_unchanged_claim_as_it_is() {
        let dir = tempdir().unwrap();
        let mut set = james_like_set(dir.path());
        let world = "play.theforsakenlands.com";
        set.set_login("Healer", "Caelaor", false).unwrap();

        // A description edit sends the same claim back with no toggle.
        // It neither turns Healer back on nor takes Erelei from default
        // for Test-Prompt.
        set.set_metadata(
            "Healer",
            Some("Resting".into()),
            Some(claim(world, Some(1848), &["Caelaor"])),
        )
        .unwrap();
        assert!(!enabled(&set, "Healer"));
        assert_eq!(
            set.get("Healer").unwrap().description.as_deref(),
            Some("Resting")
        );
        let result = set
            .set_metadata(
                "Test-Prompt",
                Some("Prompt tests".into()),
                Some(claim(world, Some(1848), &["Erelei"])),
            )
            .unwrap();
        assert!(result.released_from.is_empty());
        assert_eq!(characters_of(&set, DEFAULT_PROFILE_NAME), vec!["Erelei"]);
        assert!(set.login_on(DEFAULT_PROFILE_NAME));

        // An explicit off still turns a claim off.
        let mut off = claim(world, Some(1848), &["Erelei"]);
        off.enabled = false;
        set.set_metadata(DEFAULT_PROFILE_NAME, None, Some(off))
            .unwrap();
        assert!(!enabled(&set, DEFAULT_PROFILE_NAME));
        assert_eq!(characters_of(&set, "Test-Prompt"), vec!["Erelei"]);
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

    fn set_with_profiles(profiles: Vec<(&str, AutoMatch)>) -> ProfileSet {
        let dir = tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        for (name, am) in profiles {
            if name != DEFAULT_PROFILE_NAME {
                set.create(name).unwrap();
            }
            put_claim(&mut set, name, None, am);
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
                characters: vec!["Erelei".into()],
                enabled: true,
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
                    enabled: true,
                },
            ),
            (
                "warrior",
                AutoMatch {
                    host: Some("h".into()),
                    port: None,
                    characters: vec!["Erelei".into()],
                    enabled: true,
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

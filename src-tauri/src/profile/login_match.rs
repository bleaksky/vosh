//! Which profile a login loads. A profile claims characters on a world,
//! and when one of them logs in there Vosh loads the profile whose claim
//! fits the login best. A character belongs to one profile wherever it
//! logs in, so turning a claim on takes the character from every other
//! profile that could load there. A claim on another port of a world
//! Vosh knows first pins an older claim on the host alone to the world's
//! own port, so one name can load a profile of its own on each port.

use serde::{Deserialize, Deserializer, Serialize};

use super::ui::{default_true, is_true};
use super::worlds::{host_key, known_world};
use crate::profile::set::{display_name, ProfileEntry, ProfileSet, ProfileSetError};

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
/// reads, every profile the character was taken from, and every claim
/// pinned to its world's own port so it could keep the character.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct LoginClaim {
    pub entry: ProfileEntry,
    pub released_from: Vec<String>,
    pub pinned: Vec<PinnedClaim>,
}

/// Whom a claim must name to match, see [`ProfileSet::resolve_match`].
#[derive(Clone, Copy)]
enum Claimant<'a> {
    /// The character a login named, or none yet, when a claim that names
    /// characters never matches.
    Login(Option<&'a str>),
    /// Whoever logs in, for a session that has not logged in yet, so a
    /// claim that names characters counts as one on its host and port.
    Anyone,
}

/// A claim on the host alone that Vosh pinned to its world's own port.
/// Its whole list moved with it, since a profile holds one claim for all
/// its characters, so Characters names each one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PinnedClaim {
    pub profile: String,
    pub port: u16,
    pub characters: Vec<String>,
}

impl AutoMatch {
    /// Trim the host and the names, and drop blank names and names
    /// repeated in another case.
    pub(crate) fn cleaned(mut self) -> Self {
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
    #[cfg(test)]
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

impl ProfileSet {
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
    /// whose login toggle is off never matches. Hosts compare through
    /// [`host_key`], as the page compares them.
    pub(crate) fn resolve_match(
        &self,
        host: &str,
        port: u16,
        character: Option<&str>,
    ) -> Option<String> {
        self.resolve(host, port, Claimant::Login(character))
    }

    /// The profile a new session on `host` and `port` starts on, before
    /// anyone logs in: one whose claim is pinned to that host and port,
    /// else one that claims the host on any port. A claim that names
    /// characters counts too, since the New session form picks before a
    /// character logs in. None leaves the profile in front.
    pub(crate) fn resolve_before_login(&self, host: &str, port: u16) -> Option<String> {
        self.resolve(host, port, Claimant::Anyone)
    }

    /// The best claim on `host` and `port` for `who`, see
    /// [`ProfileSet::resolve_match`].
    fn resolve(&self, host: &str, port: u16, who: Claimant<'_>) -> Option<String> {
        let host_l = host_key(host);
        let mut best: Option<(&str, u8)> = None;
        for entry in &self.index.profiles {
            let Some(am) = &entry.auto_match else {
                continue;
            };
            if !am.enabled {
                continue;
            }
            let Some(am_host) = &am.host else { continue };
            if host_key(am_host) != host_l {
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
                match who {
                    Claimant::Anyone => {}
                    Claimant::Login(None) => continue,
                    Claimant::Login(Some(character)) => {
                        let character = character.trim().to_ascii_lowercase();
                        let any_match = am
                            .characters
                            .iter()
                            .any(|name| name.trim().to_ascii_lowercase() == character);
                        if !any_match {
                            continue;
                        }
                        score += 2;
                    }
                }
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

    /// The first profile whose claim on the world at `host` and `port`
    /// lists `character`, with its login toggle on or off. A character
    /// belongs to one profile wherever it logs in, and turning the toggle
    /// on anywhere else takes it from this one, so an import leaves the
    /// character here until you move it.
    pub(crate) fn claimant(&self, host: &str, port: Option<u16>, character: &str) -> Option<&str> {
        self.index
            .profiles
            .iter()
            .find(|p| {
                p.auto_match
                    .as_ref()
                    .is_some_and(|am| am.on_world(host, port) && am.names(character))
            })
            .map(|p| p.name.as_str())
    }

    /// Turn the login toggle for `name` on or off for `character`.
    ///
    /// On lists the character first if the profile does not list it
    /// yet, turns the toggle on, and settles every other profile on the
    /// same world that lists it, as [`Self::release_character`] says.
    /// Off keeps the world and every name and only turns the toggle off.
    /// Either way the index is written once and nothing switches, since
    /// the toggle applies at the next login.
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
        let mut pinned = Vec::new();
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
            self.release_character(idx, &host, port, character, &mut released_from, &mut pinned);
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
            pinned,
        })
    }

    /// Settle `character` on every profile but the one at `keep` whose
    /// claim sits on the world at `host` and `port`, because a character
    /// belongs to one profile wherever it logs in.
    ///
    /// When the claim at `keep` pins a port other than the world's own,
    /// on a world Vosh knows, a claim on the host alone is pinned to the
    /// world's own port with every character it lists, so the two
    /// claims never meet. Each one lands in `pinned`.
    ///
    /// Every other claim loses the character. A profile left with no
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
        pinned: &mut Vec<PinnedClaim>,
    ) {
        let wanted = character.trim().to_ascii_lowercase();
        // Pinning to the port the new claim takes would leave both
        // claims on it, so the pin needs a port of the world's own.
        let pin_to = port.and_then(|p| known_world(host).map(|w| w.port).filter(|&own| own != p));
        for (i, other) in self.index.profiles.iter_mut().enumerate() {
            let Some(am) = other.auto_match.as_mut() else {
                continue;
            };
            if i == keep || !am.on_world(host, port) || !am.names(character) {
                continue;
            }
            if let (None, Some(own)) = (am.port, pin_to) {
                am.port = Some(own);
                pinned.push(PinnedClaim {
                    profile: other.name.clone(),
                    port: own,
                    characters: am.characters.clone(),
                });
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
    /// - A claim with its toggle on settles each of its characters on
    ///   every other profile on the same world, as [`Self::set_login`]
    ///   does, and names them in `released_from` and `pinned`.
    /// - A claim with no world, port or character goes away.
    ///
    /// Writes the index once and never switches. No command calls it
    /// since `profile_set_metadata` went, and the field stays so your
    /// text survives. Tests use it to set up
    /// claims under the login rules.
    #[cfg(test)]
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
        let mut pinned = Vec::new();
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
                        self.release_character(
                            idx,
                            &host,
                            am.port,
                            character,
                            &mut released_from,
                            &mut pinned,
                        );
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
            pinned,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    use crate::profile::set::DEFAULT_PROFILE_NAME;
    use crate::profile::tests::{claim, james_like_set, put_claim, set_with_profiles};

    #[test]
    fn auto_match_accepts_legacy_single_character_shape() {
        // Older profile.toml files (pre-multi-character feature) saved
        // `character = "Ilsabet"`. Loading must still succeed and the
        // resulting struct must hold one entry in the new `characters`
        // list so the resolver treats it identically.
        let toml = r#"
host = "play.theforsakenlands.com"
port = 1848
character = "Ilsabet"
"#;
        let am: AutoMatch = toml::from_str(toml).unwrap();
        assert_eq!(am.host.as_deref(), Some("play.theforsakenlands.com"));
        assert_eq!(am.port, Some(1848));
        assert_eq!(am.characters, vec!["Ilsabet".to_string()]);
    }

    #[test]
    fn auto_match_accepts_characters_list_shape() {
        let toml = r#"
host = "play.theforsakenlands.com"
characters = ["Ilsabet", "Thessamy", "Ondrevar"]
"#;
        let am: AutoMatch = toml::from_str(toml).unwrap();
        assert_eq!(am.characters, vec!["Ilsabet", "Thessamy", "Ondrevar"]);
    }

    #[test]
    fn auto_match_merges_legacy_and_new_when_both_present() {
        // A hand-edited profile.toml could carry both fields; the
        // legacy `character` should be folded into the list without
        // duplicating an existing entry.
        let toml = r#"
host = "h"
character = "Ilsabet"
characters = ["Thessamy", "Ondrevar"]
"#;
        let am: AutoMatch = toml::from_str(toml).unwrap();
        assert_eq!(am.characters, vec!["Ilsabet", "Thessamy", "Ondrevar"]);

        let toml_with_dup = r#"
host = "h"
character = "Ilsabet"
characters = ["Ilsabet", "Ondrevar"]
"#;
        let am: AutoMatch = toml::from_str(toml_with_dup).unwrap();
        // Dedup keeps the existing position; legacy entry is not
        // re-inserted.
        assert_eq!(am.characters, vec!["Ilsabet", "Ondrevar"]);
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
        let am: AutoMatch = toml::from_str("host = \"h\"\ncharacters = [\"Ilsabet\"]\n").unwrap();
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
        assert_eq!(parsed.characters, vec!["Ilsabet"]);
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
                    characters: vec!["Ilsabet".into()],
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
        assert_eq!(set.resolve_match("h", 1848, Some("Ilsabet")), None);
        // A host-only entry that is off is no fallback at connect.
        assert_eq!(set.resolve_match("h", 1848, None), None);
    }

    #[test]
    fn before_login_a_claim_pinned_to_the_port_wins_then_one_on_the_host() {
        let claim = |port, characters: &[&str], enabled| AutoMatch {
            host: Some("play.theforsakenlands.com".into()),
            port,
            characters: characters.iter().map(|c| (*c).to_string()).collect(),
            enabled,
        };
        let set = set_with_profiles(vec![
            (DEFAULT_PROFILE_NAME, claim(Some(1848), &["Tolliver"], true)),
            ("Build", claim(Some(1825), &["Orla"], true)),
            ("Healer", claim(None, &["Maren"], true)),
            ("Spare", claim(Some(1825), &[], false)),
        ]);
        let pick = |host: &str, port| set.resolve_before_login(host, port);
        // A claim that names characters counts before anyone logs in.
        assert_eq!(
            pick("play.theforsakenlands.com", 1825).as_deref(),
            Some("Build")
        );
        assert_eq!(
            pick("Play.TheForsakenLands.com.", 1848).as_deref(),
            Some(DEFAULT_PROFILE_NAME)
        );
        // Then a claim on the host on any port.
        assert_eq!(
            pick("play.theforsakenlands.com", 4000).as_deref(),
            Some("Healer")
        );
        assert_eq!(pick("mud.example.org", 4000), None);
        // At a login the same claims ask for the character.
        assert_eq!(
            set.resolve_match("play.theforsakenlands.com", 1825, None),
            None
        );
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

    #[test]
    fn login_on_reads_on_only_for_the_profile_that_wins_the_login() {
        let dir = tempdir().unwrap();
        let mut set = james_like_set(dir.path());
        assert!(set.login_on(DEFAULT_PROFILE_NAME));
        assert!(set.login_on("Healer"));
        // Test-Prompt claims Ilsabet too but loses the tie in index order.
        assert!(!set.login_on("Test-Prompt"));

        // Off, or with no world or no character, reads off.
        let mut off = claim("play.theforsakenlands.com", Some(1848), &["Corvanne"]);
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
            claim("play.theforsakenlands.com", None, &["Ondrevar"]),
        );
        assert!(set.login_on(DEFAULT_PROFILE_NAME));
        // A profile pinned to the real port outscores it at login.
        set.create("Pinned").unwrap();
        put_claim(
            &mut set,
            "Pinned",
            None,
            claim("play.theforsakenlands.com", Some(1848), &["Ondrevar"]),
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
    fn turning_login_on_takes_ilsabet_from_every_other_profile_on_the_world() {
        let dir = tempdir().unwrap();
        let mut set = james_like_set(dir.path());
        let claim = set.set_login("Test-Prompt", "ilsabet", true).unwrap();
        assert_eq!(claim.released_from, vec![DEFAULT_PROFILE_NAME.to_string()]);
        assert_eq!(characters_of(&set, "Test-Prompt"), vec!["Ilsabet"]);
        assert!(claim.entry.auto_match.as_ref().unwrap().enabled);

        // default kept its world but lost its only character, so its
        // toggle went off rather than leaving a host wide fallback.
        let leftover = &characters_of(&set, DEFAULT_PROFILE_NAME);
        assert!(leftover.is_empty(), "{leftover:?}");
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

        // Ilsabet now loads Test-Prompt, Corvanne still loads Healer, and
        // nothing switched.
        assert_eq!(
            set.resolve_match("play.theforsakenlands.com", 1848, Some("Ilsabet")),
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
            claim(world, Some(1848), &["Ilsabet", "Thessamy"]),
        );
        set.create("Elsewhere").unwrap();
        put_claim(
            &mut set,
            "Elsewhere",
            None,
            claim("mud.example.org", None, &["Ilsabet"]),
        );
        set.create("Portless").unwrap();
        put_claim(&mut set, "Portless", None, claim(world, None, &["Ilsabet"]));
        set.create("New").unwrap();
        put_claim(&mut set, "New", None, claim(world, Some(1848), &[]));

        let claim = set.set_login("New", "Ilsabet", true).unwrap();
        assert_eq!(
            claim.released_from,
            vec![DEFAULT_PROFILE_NAME.to_string(), "Portless".to_string()]
        );
        // default keeps Thessamy and its toggle.
        assert_eq!(characters_of(&set, DEFAULT_PROFILE_NAME), vec!["Thessamy"]);
        assert!(enabled(&set, DEFAULT_PROFILE_NAME));
        // Another world keeps its own Ilsabet.
        assert_eq!(characters_of(&set, "Elsewhere"), vec!["Ilsabet"]);
        assert_eq!(characters_of(&set, "New"), vec!["Ilsabet"]);
    }

    #[test]
    fn a_port_claim_pins_an_older_claim_on_the_host_alone_to_the_worlds_own_port() {
        let dir = tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        let world = "play.theforsakenlands.com";
        put_claim(
            &mut set,
            DEFAULT_PROFILE_NAME,
            None,
            claim(world, None, &["Tolliver", "Wrenna"]),
        );
        set.create("Build").unwrap();
        set.set_world("Build", Some(world.into()), Some(1825))
            .unwrap();

        let result = set.set_login("Build", "Tolliver", true).unwrap();
        let taken = &result.released_from;
        assert!(taken.is_empty(), "{taken:?}");
        let pin = PinnedClaim {
            profile: DEFAULT_PROFILE_NAME.into(),
            port: 1848,
            characters: vec!["Tolliver".into(), "Wrenna".into()],
        };
        assert_eq!(result.pinned, vec![pin]);
        let answer = serde_json::to_value(&result).unwrap();
        assert_eq!(answer["pinned"][0]["port"], 1848);

        // Default keeps both names and its toggle, now on 1848.
        let am = set.get(DEFAULT_PROFILE_NAME).unwrap().auto_match.clone();
        let am = am.unwrap();
        assert_eq!(am.port, Some(1848));
        assert_eq!(am.characters, vec!["Tolliver", "Wrenna"]);
        assert!(am.enabled);
        assert_eq!(
            set.resolve_match(world, 1848, Some("Tolliver")),
            Some(DEFAULT_PROFILE_NAME.into())
        );
        assert_eq!(
            set.resolve_match(world, 1825, Some("Tolliver")),
            Some("Build".into())
        );
        // Wrenna moved to 1848 with the claim.
        assert_eq!(set.resolve_match(world, 1825, Some("Wrenna")), None);
        assert!(set.login_on(DEFAULT_PROFILE_NAME));
        assert!(set.login_on("Build"));

        let reloaded = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        let am = reloaded
            .get(DEFAULT_PROFILE_NAME)
            .unwrap()
            .auto_match
            .clone();
        assert_eq!(am.unwrap().port, Some(1848));
    }

    #[test]
    fn a_port_claim_on_a_host_vosh_does_not_know_takes_the_character() {
        let dir = tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        let world = "mud.example.org";
        put_claim(
            &mut set,
            DEFAULT_PROFILE_NAME,
            None,
            claim(world, None, &["Tolliver", "Wrenna"]),
        );
        set.create("Build").unwrap();
        set.set_world("Build", Some(world.into()), Some(4001))
            .unwrap();

        let result = set.set_login("Build", "Tolliver", true).unwrap();
        assert_eq!(result.released_from, vec![DEFAULT_PROFILE_NAME.to_string()]);
        let pinned = &result.pinned;
        assert!(pinned.is_empty(), "{pinned:?}");
        let am = set.get(DEFAULT_PROFILE_NAME).unwrap().auto_match.clone();
        let am = am.unwrap();
        assert_eq!(am.port, None);
        assert_eq!(am.characters, vec!["Wrenna"]);
        assert_eq!(set.resolve_match(world, 4000, Some("Tolliver")), None);
        assert_eq!(
            set.resolve_match(world, 4001, Some("Tolliver")),
            Some("Build".into())
        );
    }

    #[test]
    fn turning_login_off_keeps_the_world_and_the_name() {
        let dir = tempdir().unwrap();
        let mut set = james_like_set(dir.path());
        let claim = set.set_login("Healer", "Corvanne", false).unwrap();
        let leftover = &claim.released_from;
        assert!(leftover.is_empty(), "{leftover:?}");
        let am = claim.entry.auto_match.unwrap();
        assert!(!am.enabled);
        assert_eq!(am.characters, vec!["Corvanne"]);
        assert_eq!(am.host.as_deref(), Some("play.theforsakenlands.com"));
        assert_eq!(am.port, Some(1848));
        assert!(!set.login_on("Healer"));
        assert_eq!(
            set.resolve_match("play.theforsakenlands.com", 1848, Some("Corvanne")),
            None
        );
        // On again restores it without taking anything from anyone.
        let claim = set.set_login("Healer", "Corvanne", true).unwrap();
        let leftover = &claim.released_from;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(set.login_on("Healer"));
    }

    #[test]
    fn turning_login_on_needs_a_character_and_a_world() {
        let dir = tempdir().unwrap();
        let mut set = james_like_set(dir.path());
        set.create("Blank").unwrap();
        assert!(matches!(
            set.set_login("Blank", "Ilsabet", true),
            Err(ProfileSetError::NoWorld(_))
        ));
        assert!(set.get("Blank").unwrap().auto_match.is_none());
        assert!(matches!(
            set.set_login("Healer", "  ", true),
            Err(ProfileSetError::NoCharacter)
        ));
        assert!(matches!(
            set.set_login("Nobody", "Ilsabet", true),
            Err(ProfileSetError::NotFound(_))
        ));
        assert_eq!(
            set.set_login("Blank", "Ilsabet", true)
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
        assert_eq!(am.characters, vec!["Ilsabet"]);
        assert!(am.enabled);
        // Ilsabet on the old world now loads Test-Prompt.
        assert_eq!(
            set.resolve_match("play.theforsakenlands.com", 1848, Some("Ilsabet")),
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
        let leftover = &am.characters;
        assert!(leftover.is_empty(), "{leftover:?}");
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
            &[" Corvanne ", "corvanne", ""],
        );
        seed.enabled = true;
        let entry = set
            .create_from(" Corvanne ", Some(DEFAULT_PROFILE_NAME), Some(seed))
            .unwrap();
        assert_eq!(entry.name, "Corvanne");
        assert_eq!(entry.description.as_deref(), Some("Immortal"));
        let am = entry.auto_match.unwrap();
        assert_eq!(am.host.as_deref(), Some("play.theforsakenlands.com"));
        assert_eq!(am.characters, vec!["Corvanne"]);
        assert_eq!(
            std::fs::read_to_string(set.profile_path("Corvanne")).unwrap(),
            "marker = true\n"
        );
        // Creating claims nothing away: Healer keeps Corvanne and still
        // wins the tie in index order.
        assert_eq!(characters_of(&set, "Healer"), vec!["Corvanne"]);
        assert!(set.login_on("Healer"));
        assert!(!set.login_on("Corvanne"));
        assert_eq!(set.active_name(), DEFAULT_PROFILE_NAME);
        let reloaded = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert!(reloaded.get("Corvanne").is_some());
    }

    #[test]
    fn claimed_by_names_the_profile_a_login_loads() {
        let dir = tempdir().unwrap();
        let mut set = james_like_set(dir.path());
        let world = "play.theforsakenlands.com";
        assert_eq!(
            set.claimed_by(world, 1848, "ilsabet"),
            Some(DEFAULT_PROFILE_NAME.into())
        );
        assert_eq!(
            set.claimed_by(world, 1848, "Corvanne"),
            Some("Healer".into())
        );
        set.set_login("Test-Prompt", "Ilsabet", true).unwrap();
        assert_eq!(
            set.claimed_by(world, 1848, "Ilsabet"),
            Some("Test-Prompt".into())
        );

        // A host wide fallback from an older index loads for Ondrevar but
        // does not claim him.
        set.create("Fallback").unwrap();
        put_claim(&mut set, "Fallback", None, claim(world, None, &[]));
        assert_eq!(
            set.resolve_match(world, 1848, Some("Ondrevar")),
            Some("Fallback".into())
        );
        assert_eq!(set.claimed_by(world, 1848, "Ondrevar"), None);
        assert_eq!(set.claimed_by("mud.example.org", 4000, "Ilsabet"), None);
    }

    #[test]
    fn the_claimant_lists_the_character_on_the_world_with_its_toggle_on_or_off() {
        let world = "play.theforsakenlands.com";
        let mut off = claim(world, None, &["Maren"]);
        off.enabled = false;
        let set = set_with_profiles(vec![
            ("Healer", claim(world, Some(1848), &["Orla"])),
            ("Tank", off),
            ("Away", claim("mud.example.org", Some(4000), &["Tolliver"])),
        ]);
        assert_eq!(set.claimant(world, Some(1848), "orla"), Some("Healer"));
        assert_eq!(set.claimant(world, None, "Orla"), Some("Healer"));
        // Healer pins its port, so another port on the host is free.
        assert_eq!(set.claimant(world, Some(4000), "Orla"), None);
        // Tank still lists Maren on every port of the host, toggle off.
        assert_eq!(set.claimant(world, Some(4000), "Maren"), Some("Tank"));
        assert_eq!(set.claimant(world, Some(1848), "Tolliver"), None);
        assert_eq!(
            set.claimant("mud.example.org", Some(4000), "Tolliver"),
            Some("Away")
        );
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
                Some(claim(world, Some(1848), &["Corvanne", "ilsabet"])),
            )
            .unwrap();
        assert_eq!(
            result.released_from,
            vec![DEFAULT_PROFILE_NAME.to_string(), "Test-Prompt".to_string()]
        );
        assert!(result.entry.auto_match.as_ref().unwrap().enabled);
        assert_eq!(characters_of(&set, "Healer"), vec!["Corvanne", "ilsabet"]);
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
            set.resolve_match(world, 1848, Some("Ilsabet")),
            Some("Healer".into())
        );
        assert_eq!(set.resolve_match(world, 1848, Some("Ondrevar")), None);
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
        assert_eq!(set.resolve_match(world, 1848, Some("Ondrevar")), None);

        // Clearing a character list turns the toggle off and keeps the
        // world.
        set.set_metadata("Healer", None, Some(claim(world, Some(1848), &[" "])))
            .unwrap();
        assert!(!enabled(&set, "Healer"));
        assert_eq!(set.resolve_match(world, 1848, Some("Corvanne")), None);

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
        set.set_login("Healer", "Corvanne", false).unwrap();

        // A description edit sends the same claim back with no toggle.
        // It neither turns Healer back on nor takes Ilsabet from default
        // for Test-Prompt.
        set.set_metadata(
            "Healer",
            Some("Resting".into()),
            Some(claim(world, Some(1848), &["Corvanne"])),
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
                Some(claim(world, Some(1848), &["Ilsabet"])),
            )
            .unwrap();
        let leftover = &result.released_from;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(characters_of(&set, DEFAULT_PROFILE_NAME), vec!["Ilsabet"]);
        assert!(set.login_on(DEFAULT_PROFILE_NAME));

        // An explicit off still turns a claim off.
        let mut off = claim(world, Some(1848), &["Ilsabet"]);
        off.enabled = false;
        set.set_metadata(DEFAULT_PROFILE_NAME, None, Some(off))
            .unwrap();
        assert!(!enabled(&set, DEFAULT_PROFILE_NAME));
        assert_eq!(characters_of(&set, "Test-Prompt"), vec!["Ilsabet"]);
    }
}

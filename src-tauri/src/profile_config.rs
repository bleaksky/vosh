//! Per-profile TOML serialization. Phase 9.
//!
//! [`ProfileConfig`] is a serde-friendly snapshot of the parts of a
//! [`crate::profile::Profile`] that survive across app launches. The runtime
//! Profile holds extra state (compiled regex, Lua engine, tick deadlines)
//! that does not belong in the on-disk file.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use mudclient_alias::Alias;
use mudclient_trigger::Trigger;
use mudclient_vars::Scope;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::profile::Profile;
use crate::tick::{TickConfig, TickRuntime};

#[derive(Debug, Error)]
pub(crate) enum ConfigError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("toml parse error: {0}")]
    Deserialize(#[from] toml::de::Error),
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub(crate) struct ProfileConfig {
    #[serde(default)]
    pub connection: ConnectionConfig,
    #[serde(default)]
    pub aliases: Vec<Alias>,
    #[serde(default)]
    pub profile_vars: BTreeMap<String, String>,
    #[serde(default)]
    pub triggers: Vec<Trigger>,
    #[serde(default)]
    pub tick: TickPersistConfig,
    /// Script names to load automatically on startup. Each name resolves
    /// to `<app_data_dir>/scripts/<name>.lua`.
    #[serde(default)]
    pub autoload_scripts: Vec<String>,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub plugins: PluginsPersist,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub(crate) struct PluginsPersist {
    /// Names of plugins to load on startup.
    #[serde(default)]
    pub enabled: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct UiConfig {
    /// `default`, `high-contrast`, or `system`. `system` follows the OS
    /// `prefers-contrast` media query.
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Opt in to background update checks. Off by default.
    #[serde(default)]
    pub auto_update: bool,
    /// CSS font-family stack used by the terminal, status bar, and input.
    /// Falls back to `default_font_family` when not set.
    #[serde(default = "default_font_family")]
    pub font_family: String,
    /// Terminal font size in pixels.
    #[serde(default = "default_font_size")]
    pub font_size: u32,
    /// Affect names rendered as pills in the status bar. Present affects
    /// show their remaining duration; absent ones render as a struck-out
    /// red-bordered pill so the player notices the gap at a glance.
    #[serde(default)]
    pub tracked_affects: Vec<String>,
    /// Preset ids enabled in the Highlights drawer. On startup the
    /// frontend re-installs these presets' bundled triggers so users
    /// don't have to re-toggle each launch.
    #[serde(default)]
    pub enabled_presets: Vec<String>,
    /// Persistent dock layout for the side-panel sections. Authored
    /// in the standalone Layout Editor window; the main window reads
    /// this at startup and listens for `mudclient://dock-layout-changed`
    /// to pick up live edits without a relaunch.
    #[serde(default)]
    pub dock_layout: Vec<DockEntryPersist>,
}

/// On-disk representation of a single docked bar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DockEntryPersist {
    pub id: String,
    pub zone: String,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            auto_update: false,
            font_family: default_font_family(),
            font_size: default_font_size(),
            tracked_affects: Vec::new(),
            enabled_presets: Vec::new(),
            dock_layout: Vec::new(),
        }
    }
}

fn default_theme() -> String {
    "default".to_string()
}

fn default_font_family() -> String {
    "BerkeleyMono Nerd Font, JetBrains Mono, Fira Code, Menlo, Consolas, ui-monospace, monospace"
        .to_string()
}

fn default_font_size() -> u32 {
    14
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ConnectionConfig {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub tls: bool,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            host: "play.theforsakenlands.com".to_string(),
            port: 1848,
            tls: false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct TickPersistConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    #[serde(default)]
    pub auto_fire: Option<String>,
    #[serde(default = "default_true")]
    pub sound: bool,
    #[serde(default)]
    pub reset_pattern: Option<String>,
    /// Seconds before the next fire to print the warning echo. None
    /// disables the warning entirely.
    #[serde(default)]
    pub warn_at_secs: Option<u64>,
    #[serde(default)]
    pub warn_message: Option<String>,
    #[serde(default)]
    pub warn_color: Option<String>,
}

impl Default for TickPersistConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: 30,
            auto_fire: None,
            sound: true,
            reset_pattern: None,
            warn_at_secs: None,
            warn_message: None,
            warn_color: None,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_interval() -> u64 {
    30
}

impl ProfileConfig {
    /// Build a snapshot from the live profile.
    pub(crate) fn from_profile(profile: &Profile) -> Self {
        let aliases: Vec<Alias> = profile.aliases.list().into_iter().cloned().collect();

        let mut profile_vars: BTreeMap<String, String> = BTreeMap::new();
        for (name, value, scope) in profile.vars.iter() {
            if matches!(scope, Scope::Profile) {
                profile_vars.insert(name.to_string(), value.to_string());
            }
        }

        let triggers = profile.triggers.list();

        let tick = TickPersistConfig {
            enabled: profile.tick.config.enabled,
            interval_secs: profile.tick.config.interval.as_secs().max(1),
            auto_fire: profile.tick.config.auto_fire.clone(),
            sound: profile.tick.config.sound,
            reset_pattern: profile.tick.config.reset_pattern.clone(),
            warn_at_secs: profile.tick.config.warn_at_secs,
            warn_message: profile.tick.config.warn_message.clone(),
            warn_color: profile.tick.config.warn_color.clone(),
        };

        let ui = UiConfig {
            theme: profile.ui.theme.clone(),
            auto_update: profile.ui.auto_update,
            font_family: profile.ui.font_family.clone(),
            font_size: profile.ui.font_size,
            tracked_affects: profile.ui.tracked_affects.clone(),
            enabled_presets: profile.ui.enabled_presets.clone(),
            dock_layout: profile.ui.dock_layout.clone(),
        };

        let plugins = PluginsPersist {
            enabled: profile.plugins.enabled.clone(),
        };

        Self {
            connection: ConnectionConfig::default(),
            aliases,
            profile_vars,
            triggers,
            tick,
            autoload_scripts: Vec::new(),
            ui,
            plugins,
        }
    }

    /// Apply a snapshot onto a live profile, replacing the relevant pieces.
    /// Triggers with invalid regex are reported and skipped.
    pub(crate) fn apply_to(&self, profile: &mut Profile) -> Vec<String> {
        let mut warnings = Vec::new();

        // Aliases: replace the store entirely.
        let mut aliases = mudclient_alias::AliasStore::new();
        for alias in &self.aliases {
            aliases.set(alias.clone());
        }
        profile.aliases = aliases;

        // Profile-scoped vars: clear existing profile-scoped, then set.
        // Session-scoped values stay alone.
        let session_only: Vec<(String, String)> = profile
            .vars
            .iter()
            .filter_map(|(k, v, scope)| {
                if matches!(scope, Scope::Session) {
                    Some((k.to_string(), v.to_string()))
                } else {
                    None
                }
            })
            .collect();
        let mut vars = mudclient_vars::VariableStore::new();
        for (k, v) in &self.profile_vars {
            vars.set(Scope::Profile, k.clone(), v.clone());
        }
        for (k, v) in session_only {
            vars.set(Scope::Session, k, v);
        }
        profile.vars = vars;

        // Triggers: replace, surfacing invalid regex.
        let mut triggers = mudclient_trigger::TriggerStore::new();
        for t in &self.triggers {
            if let Err(e) = triggers.set(t.clone()) {
                warnings.push(format!("trigger `{}` rejected: {e}", t.name));
            }
        }
        profile.triggers = triggers;

        // Tick: build a fresh TickRuntime around the persisted config.
        let mut tick = TickRuntime {
            config: TickConfig {
                enabled: self.tick.enabled,
                interval: Duration::from_secs(self.tick.interval_secs.max(1)),
                auto_fire: self.tick.auto_fire.clone(),
                sound: self.tick.sound,
                reset_pattern: self.tick.reset_pattern.clone(),
                warn_at_secs: self.tick.warn_at_secs,
                warn_message: self.tick.warn_message.clone(),
                warn_color: self.tick.warn_color.clone(),
            },
            ..Default::default()
        };
        if let Err(e) = tick.set_reset_pattern(self.tick.reset_pattern.clone()) {
            warnings.push(format!("tick reset pattern rejected: {e}"));
        }
        profile.tick = tick;

        // UI preferences carry across.
        profile.ui = UiConfig {
            theme: self.ui.theme.clone(),
            auto_update: self.ui.auto_update,
            font_family: self.ui.font_family.clone(),
            font_size: self.ui.font_size,
            tracked_affects: self.ui.tracked_affects.clone(),
            enabled_presets: self.ui.enabled_presets.clone(),
            dock_layout: self.ui.dock_layout.clone(),
        };

        // Plugin enabled-set is persisted; the actual load happens in the
        // PluginManager wired into AppState.
        profile.plugins = PluginsPersist {
            enabled: self.plugins.enabled.clone(),
        };

        warnings
    }

    pub(crate) fn save(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let toml_str = toml::to_string_pretty(self)?;
        std::fs::write(path, toml_str)?;
        Ok(())
    }

    pub(crate) fn load(path: &Path) -> Result<Self, ConfigError> {
        let toml_str = std::fs::read_to_string(path)?;
        let config: ProfileConfig = toml::from_str(&toml_str)?;
        Ok(config)
    }

    pub(crate) fn to_toml(&self) -> Result<String, ConfigError> {
        Ok(toml::to_string_pretty(self)?)
    }

    pub(crate) fn from_toml(text: &str) -> Result<Self, ConfigError> {
        Ok(toml::from_str(text)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mudclient_trigger::{HighlightStyle, NamedColor, TriggerAction};

    #[test]
    fn round_trip_through_toml() {
        let mut config = ProfileConfig::default();
        config.aliases.push(Alias::new("greet", "wave;bow"));
        config.profile_vars.insert("target".into(), "goblin".into());
        config.triggers.push(Trigger {
            name: "tells".into(),
            pattern: r"\w+ tells you".into(),
            priority: 0,
            enabled: true,
            action: TriggerAction::Highlight {
                style: HighlightStyle {
                    fg: Some(NamedColor::Cyan),
                    ..Default::default()
                },
            },
            preset: None,
        });
        let text = config.to_toml().unwrap();
        let parsed = ProfileConfig::from_toml(&text).unwrap();
        assert_eq!(parsed.aliases.len(), 1);
        assert_eq!(parsed.aliases[0].name, "greet");
        assert_eq!(
            parsed.profile_vars.get("target").map(String::as_str),
            Some("goblin")
        );
        assert_eq!(parsed.triggers.len(), 1);
        assert_eq!(parsed.triggers[0].name, "tells");
    }

    #[test]
    fn apply_to_profile_round_trips_aliases() {
        let mut config = ProfileConfig::default();
        config.aliases.push(Alias::new("greet", "wave"));
        let mut profile = Profile::default();
        let warnings = config.apply_to(&mut profile);
        assert!(warnings.is_empty());
        let snapshot = ProfileConfig::from_profile(&profile);
        assert_eq!(snapshot.aliases.len(), 1);
        assert_eq!(snapshot.aliases[0].name, "greet");
    }

    #[test]
    fn invalid_trigger_regex_warns_but_continues() {
        let mut config = ProfileConfig::default();
        config.triggers.push(Trigger {
            name: "bad".into(),
            pattern: "[unclosed".into(),
            priority: 0,
            enabled: true,
            action: TriggerAction::Gag,
            preset: None,
        });
        let mut profile = Profile::default();
        let warnings = config.apply_to(&mut profile);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("rejected"));
    }
}

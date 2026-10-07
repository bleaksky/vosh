//! Per-profile TOML serialization. Phase 9.
//!
//! [`ProfileConfig`] is a serde-friendly snapshot of the parts of a
//! [`crate::profile::live::Profile`] that survive across app launches.
//! The runtime Profile holds extra state (compiled regex, Lua engine, tick
//! deadlines) that does not belong in the on-disk file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use vosh_automation::alert::AlertParts;
use vosh_automation::alias::Alias;
use vosh_automation::trigger::Trigger;

use crate::disk::atomic::{hold_unread, is_unread, write_with_backup};
use crate::loadouts::preset_edits::PresetEdits;
use crate::profile::live::{Macro, Profile, Timer};
use crate::profile::set::ProfileSet;
use crate::profile::shared::GlobalConfig;
use crate::profile::ui::{
    coerce_affects_thresholds, read_theme_follow, set_theme_follow, UiConfig,
};
use crate::tick::{TickConfig, TickSettings};

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
    pub aliases: Vec<Alias>,
    #[serde(default)]
    pub profile_vars: BTreeMap<String, String>,
    /// Every trigger. Room triggers go under `room_triggers` on disk,
    /// see [`trigger_lists`].
    #[serde(flatten, with = "trigger_lists")]
    pub triggers: Vec<Trigger>,
    #[serde(default)]
    pub tick: TickConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub plugins: PluginsPersist,
    /// Keyboard macro bindings.
    #[serde(default)]
    pub macros: Vec<Macro>,
    /// Interval timers (Settings timers tab).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub timers: Vec<Timer>,
    /// Group folders the user bulk-disabled. One list per type so a
    /// "Combat" alias group is independent of a "Combat" trigger
    /// group — the UX is per-type, matching the existing tab split.
    /// Empty by default. Skip-serialize so profile.toml stays clean
    /// for users who do not use groups.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_alias_groups: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_trigger_groups: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_macro_groups: Vec<String>,
    /// Timer groups turned off. Timers stay in the profile file in
    /// loadout mode, so this list does too.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_timer_groups: Vec<String>,
    /// The catalog groups each of your folders became in the shared
    /// catalog, which `#group` follows. See [`GroupFolders`].
    #[serde(default, skip_serializing_if = "GroupFolders::is_empty")]
    pub group_folders: GroupFolders,
    /// The `[prompt]` table: the switch, the design, earlier designs and
    /// how Vosh reads the game's prompt. None in a file an older build
    /// wrote, and left out of a file while it says nothing a default one
    /// does not. Set it with [`ProfileConfig::set_prompt`], which keeps
    /// the `[ui]` copy of the switch and the design in step. Whether the
    /// design follows the game reads and writes as
    /// [`vosh_prompt::config::file_table`] says.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "vosh_prompt::config::file_table"
    )]
    pub prompt: Option<vosh_prompt::PromptConfig>,
    /// What each alert preset does, by preset id, the `[alerts]` table
    /// of Alerts Q5. Whether a preset rings is in `ui.enabled_presets`,
    /// as for any preset. Left out while it holds none, and a build that
    /// knows no alert skips it (D14).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub alerts: BTreeMap<String, AlertParts>,
    /// Your edits to the presets, the `[preset_edits]` table of the
    /// Presets review (Q1, Q2), beside `ui.enabled_presets` as `[alerts]`
    /// is. Left out while it holds none, and a build that knows no edit
    /// skips it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub preset_edits: PresetEdits,
    /// Vosh dials again after the link drops while you play (Alerts Q13
    /// and Q14). On for every profile, so the file says
    /// `reconnect = false` only once you turn it off.
    #[serde(default, skip_serializing_if = "OnSwitch::is_on")]
    pub reconnect: OnSwitch,
}

/// A switch that stays on until you turn it off, such as Reconnect when
/// the link drops. A file writes it only while it is off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct OnSwitch(pub(crate) bool);

impl Default for OnSwitch {
    fn default() -> Self {
        Self(true)
    }
}

impl OnSwitch {
    pub(crate) fn is_on(&self) -> bool {
        self.0
    }
}

/// The catalog groups each folder of one profile became when the shared
/// catalog wizard built the catalog, one map per kind. The catalog is
/// shared, so a folder two characters filled differently lands in more
/// than one catalog group, such as `combat` for the items both had and
/// `combat (Healer)` for the ones only the Healer had. `#group combat`
/// and `mud.set_group_enabled` then turn on or off every catalog group in
/// the profile's `combat` entry, which is exactly what the profile had
/// in its combat folder, and an empty entry turns nothing on, since the
/// profile had no such folder. A folder with no entry is its catalog
/// group of the same name. Empty in per profile mode.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GroupFolders {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub aliases: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub triggers: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub macros: BTreeMap<String, Vec<String>>,
}

impl GroupFolders {
    pub(crate) fn is_empty(&self) -> bool {
        self.aliases.is_empty() && self.triggers.is_empty() && self.macros.is_empty()
    }
}

/// The trigger list of a profile file or catalog.toml, as two keys on
/// disk. Line and Prompt triggers go under `triggers`, and Room and Your
/// target triggers under `room_triggers`. Builds up to 0.8.0 read
/// `triggers` with `line` and `prompt` as the only targets, and a `room`
/// or a `room_target` there would fail the whole file, so a rollback
/// would start on defaults (D14).
/// They skip the key they do not know, and a load here puts the two
/// lists back together, so the field holds every trigger in memory. Use
/// it on a `Vec<Trigger>` field with
/// `#[serde(flatten, with = "trigger_lists")]`.
pub(crate) mod trigger_lists {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use vosh_automation::trigger::Trigger;

    #[derive(Serialize)]
    struct Written<'a> {
        triggers: Vec<&'a Trigger>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        room_triggers: Vec<&'a Trigger>,
    }

    #[derive(Deserialize)]
    struct Read {
        #[serde(default)]
        triggers: Vec<Trigger>,
        #[serde(default)]
        room_triggers: Vec<Trigger>,
    }

    pub(crate) fn serialize<S: Serializer>(list: &[Trigger], s: S) -> Result<S::Ok, S::Error> {
        let (room_triggers, triggers) = list.iter().partition(|t| t.target.is_room());
        Written {
            triggers,
            room_triggers,
        }
        .serialize(s)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Trigger>, D::Error> {
        let Read {
            mut triggers,
            room_triggers,
        } = Read::deserialize(d)?;
        triggers.extend(room_triggers);
        Ok(triggers)
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub(crate) struct PluginsPersist {
    /// Names of plugins to load on startup.
    #[serde(default)]
    pub enabled: Vec<String>,
}

impl ProfileConfig {
    /// Build a snapshot from the live profile.
    pub(crate) fn from_profile(profile: &Profile) -> Self {
        let aliases: Vec<Alias> = profile.aliases.list().into_iter().cloned().collect();

        let profile_vars: BTreeMap<String, String> = profile
            .vars
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();

        let triggers = profile.triggers.list();

        let tick = profile.tick.config.clone();

        // `UiConfig` is Clone, so the snapshot is a direct copy. Keeping
        // this a single clone (rather than a hand-listed field copy)
        // means a new UI setting only needs to be added to the struct
        // definition, not mirrored here and in `apply_to`.
        let ui = profile.ui.clone();

        let plugins = PluginsPersist {
            enabled: profile.plugins.enabled.clone(),
        };

        let disabled_alias_groups = profile.aliases.disabled_groups();
        let disabled_trigger_groups = profile.triggers.disabled_groups();
        let disabled_macro_groups: Vec<String> =
            profile.disabled_macro_groups.iter().cloned().collect();
        let disabled_timer_groups: Vec<String> =
            profile.disabled_timer_groups.iter().cloned().collect();

        let mut config = Self {
            aliases,
            profile_vars,
            triggers,
            tick,
            ui,
            plugins,
            macros: profile.macros.clone(),
            timers: profile.timers.clone(),
            disabled_alias_groups,
            disabled_trigger_groups,
            disabled_macro_groups,
            disabled_timer_groups,
            group_folders: profile.group_folders.clone(),
            prompt: None,
            alerts: profile.alerts.clone(),
            preset_edits: profile.preset_edits.clone(),
            reconnect: profile.reconnect,
        };
        config.set_prompt(profile.prompt.clone());
        config
    }

    /// What a profile that never saved a file stands for: the defaults,
    /// with a design that follows the game for when you turn drawing on.
    /// A switch to such a profile loads it.
    pub(crate) fn fresh() -> Self {
        let mut config = Self::default();
        config.set_prompt(vosh_prompt::PromptConfig::fresh());
        config
    }

    /// The `[prompt]` table this file stands for: its own, or for a file
    /// with none, the switch and the design older builds kept in `[ui]`.
    /// A design there that counts as none of your own, empty or a default
    /// Vosh shipped, follows the game, and with no codes to follow it is
    /// empty.
    pub(crate) fn prompt_config(&self) -> vosh_prompt::PromptConfig {
        match &self.prompt {
            Some(prompt) => prompt.clone(),
            None => {
                let mut prompt = vosh_prompt::PromptConfig::from_legacy(
                    self.ui.prompt_template_enabled,
                    &self.ui.prompt_template,
                );
                if vosh_prompt::PromptConfig::counts_as_no_design(&prompt.template) {
                    prompt.follow_game(vosh_prompt::aabahran::Who::default());
                }
                prompt
            }
        }
    }

    /// Set the `[prompt]` table and its `[ui]` copy of the switch and the
    /// design, which every save writes so an older build still draws
    /// your prompt. A table that says nothing a default one does
    /// not stays out of the file.
    pub(crate) fn set_prompt(&mut self, prompt: vosh_prompt::PromptConfig) {
        self.ui.prompt_template_enabled = prompt.draw;
        self.ui.prompt_template.clone_from(&prompt.template);
        self.prompt = (!prompt.is_default()).then_some(prompt);
    }

    /// What every load does before anything reads the file. A file with
    /// no `[prompt]` takes the switch and the design from `[ui]`, and a
    /// file with one puts its copy in `[ui]` back in step. A design that
    /// counts as none of your own, such as a default Vosh shipped, follows
    /// the game, see [`vosh_prompt::config::file_table`].
    fn merge_legacy_prompt(&mut self) {
        let prompt = self.prompt_config();
        self.set_prompt(prompt);
    }

    /// Apply a snapshot onto a live profile, replacing the relevant pieces.
    /// Triggers with invalid regex are reported and skipped. The caller
    /// then hands the tick settings and the `[prompt]` table to the
    /// connection, through [`crate::profile::switch::hand_to_connection`].
    pub(crate) fn apply_to(&self, profile: &mut Profile) -> Vec<String> {
        let mut warnings = Vec::new();

        // Aliases: replace the store entirely.
        let mut aliases = vosh_automation::alias::AliasStore::new();
        for alias in &self.aliases {
            aliases.set(alias.clone());
        }
        aliases.set_disabled_groups(self.disabled_alias_groups.iter().cloned());
        profile.aliases = aliases;

        // Profile-scoped vars: replace. Each session keeps its own.
        let mut vars = vosh_automation::vars::VariableStore::new();
        for (k, v) in &self.profile_vars {
            vars.set(k.clone(), v.clone());
        }
        profile.vars = vars;

        // Triggers: replace, surfacing invalid regex.
        let mut triggers = vosh_automation::trigger::TriggerStore::new();
        for t in &self.triggers {
            if let Err(e) = triggers.set(t.clone()) {
                warnings.push(format!("trigger `{}` rejected: {e}", t.name));
            }
        }
        triggers.set_disabled_groups(self.disabled_trigger_groups.iter().cloned());
        profile.triggers = triggers;

        // Tick: take the persisted settings. The running count stays on
        // the connection, which follows them (see `hand_to_connection`).
        let reset_regex = crate::tick::compile_reset_pattern(self.tick.reset_pattern.as_deref())
            .unwrap_or_else(|e| {
                warnings.push(format!("tick reset pattern rejected: {e}"));
                None
            });
        profile.tick = TickSettings {
            config: TickConfig {
                interval_secs: self.tick.interval_secs.max(1),
                ..self.tick.clone()
            },
            reset_regex,
        };

        // UI preferences carry across as a single clone (see the
        // matching note in `from_profile`).
        profile.ui = self.ui.clone();
        // The custom prompt takes the file's [prompt] table, or the
        // switch and the design an older file kept in [ui], which
        // `hand_to_connection` hands the prompt engine.
        profile.prompt = self.prompt_config();

        // The list of plugins the profile turns on. They load from it in
        // src-tauri/src/app/plugins.rs.
        profile.plugins = PluginsPersist {
            enabled: self.plugins.enabled.clone(),
        };

        // Macros round-trip whole; the disabled-groups set lives
        // directly on Profile because there is no MacroStore wrapper.
        profile.macros.clone_from(&self.macros);
        profile.timers.clone_from(&self.timers);
        profile.disabled_macro_groups = self
            .disabled_macro_groups
            .iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect();
        profile.disabled_timer_groups = self
            .disabled_timer_groups
            .iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect();
        profile.group_folders.clone_from(&self.group_folders);
        profile.alerts.clone_from(&self.alerts);
        profile.preset_edits.clone_from(&self.preset_edits);
        profile.reconnect = self.reconnect;

        warnings
    }

    /// Take out the aliases, triggers, and macros, which the shared
    /// catalog holds in loadout mode. A profile file that kept a copy
    /// would lay it over the catalog at the next launch, bringing back an
    /// item you deleted or an older version of one you changed. The group
    /// checkbox lists and the folder map stay, since the profile file is
    /// where they persist.
    pub(crate) fn clear_catalog_items(&mut self) {
        self.aliases.clear();
        self.triggers.clear();
        self.macros.clear();
        self.alerts.clear();
        self.preset_edits.clear();
    }

    /// Write the profile file at `path`. The first save that writes a
    /// `[prompt]` table into it keeps the file as it was, see
    /// [`keep_before_prompt_editor`].
    pub(crate) fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let toml_str = toml::to_string_pretty(self)?;
        if self.prompt.is_some() && !is_unread(path) {
            keep_before_prompt_editor(path);
        }
        write_with_backup(path, &toml_str)?;
        Ok(())
    }

    pub(crate) fn load(path: &Path) -> Result<Self, ConfigError> {
        let toml_str = std::fs::read_to_string(path)?;
        Self::from_toml(&toml_str)
    }

    pub(crate) fn to_toml(&self) -> Result<String, ConfigError> {
        Ok(toml::to_string_pretty(self)?)
    }

    pub(crate) fn from_toml(text: &str) -> Result<Self, ConfigError> {
        let mut config: ProfileConfig = toml::from_str(text)?;
        config.merge_legacy_prompt();
        let mode = read_theme_follow(config.ui.follow_system_appearance, &config.ui.theme_follow);
        set_theme_follow(&mut config.ui, mode);
        // A hand edit can set almost gone above running out. Read it as
        // running out, as a save would write it.
        (
            config.ui.affects_running_out_hours,
            config.ui.affects_almost_gone_hours,
        ) = coerce_affects_thresholds(
            config.ui.affects_running_out_hours,
            config.ui.affects_almost_gone_hours,
        );
        Ok(config)
    }
}

/// The copy of a profile file as it was before Vosh first wrote a
/// `[prompt]` table into it, `<file>.before-prompt-editor`.
pub(crate) fn before_prompt_editor_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    name.push(".before-prompt-editor");
    path.with_file_name(name)
}

/// Keep the profile file at `path` as it was, once, before a save writes
/// the first `[prompt]` table into it, so the design and switch it held
/// in `[ui]` survive whatever you try in the prompt editor. Backup
/// rotation never removes the copy, since `prune_backups` only takes
/// `.bak.<digits>` names. Nothing happens when the copy exists, when
/// there is no file yet, or when the file already has a `[prompt]`
/// table. A copy that fails is logged and the save goes on, since the
/// rotating backups still hold the file.
fn keep_before_prompt_editor(path: &Path) {
    let copy = before_prompt_editor_path(path);
    if copy.exists() || !path.is_file() {
        return;
    }
    let has_prompt = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .is_some_and(|table| table.contains_key("prompt"));
    if has_prompt {
        return;
    }
    match std::fs::copy(path, &copy) {
        Ok(_) => {
            tracing::info!(
                copy = %copy.display(),
                "kept the profile file before the prompt editor",
            );
        }
        Err(e) => {
            // A copy cut short is no copy, and would stop the next save
            // from making a whole one.
            let _ = std::fs::remove_file(&copy);
            tracing::warn!(
                error = %e,
                copy = %copy.display(),
                "could not keep the profile file before the prompt editor",
            );
        }
    }
}

/// What Vosh tells you at launch when the active profile file does not
/// read.
pub(crate) fn unread_profile_notice(name: &str) -> String {
    format!(
        "Vosh could not read the {} profile file, so it will not save over it. Fix the file or \
         switch to another profile.",
        crate::profile::set::display_name(name)
    )
}

/// What Vosh tells you at launch when global.toml does not read.
pub(crate) const UNREAD_GLOBAL_NOTICE: &str = "Vosh could not read global.toml, which holds your \
     shared settings, so it will not save over it. Fix the file and restart Vosh.";

/// Load the active profile file and the shared part of global.toml into
/// `profile` at launch. A file that does not read stays as it is on
/// disk. Vosh holds it with [`hold_unread`], keeps the defaults in its
/// place for this session, and returns the sentence that tells you so.
/// With no file yet, the profile is fresh. It follows the game with
/// drawing off.
pub(crate) fn load_at_launch(set: &ProfileSet, profile: &mut Profile) -> Vec<String> {
    let mut notices = Vec::new();
    let active_path = set.active_path();
    if active_path.exists() {
        match ProfileConfig::load(&active_path) {
            Ok(snapshot) => {
                for warning in snapshot.apply_to(profile) {
                    tracing::info!(warning = %warning, "profile apply warning");
                }
                tracing::info!(
                    path = %active_path.display(),
                    active = %set.active_name(),
                    "loaded profile",
                );
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    path = %active_path.display(),
                    "active profile unreadable at startup; it will not be saved over",
                );
                hold_unread(&active_path);
                notices.push(unread_profile_notice(set.active_name()));
            }
        }
    } else {
        // A profile that never saved a file is fresh, see
        // [`ProfileConfig::fresh`].
        profile.prompt = vosh_prompt::PromptConfig::fresh();
    }
    let global_path = set.global_path();
    match GlobalConfig::load_shared(&global_path, set.scope()) {
        Ok(Some(global)) => {
            global.apply_to(profile);
            tracing::info!(path = %global_path.display(), "loaded global config");
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!(
                error = %e,
                path = %global_path.display(),
                "global.toml unreadable at startup; it will not be saved over",
            );
            hold_unread(&global_path);
            notices.push(UNREAD_GLOBAL_NOTICE.to_string());
        }
    }
    notices
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::session::connection::Connection;
    use vosh_automation::trigger::{HighlightStyle, NamedColor, TriggerAction};

    #[test]
    fn a_saved_profile_leaves_out_the_connection_table() {
        // Profile files used to carry [connection] with the default game
        // address on every save, whatever they held. Nothing reads it:
        // Vosh dials the target the window keeps, and a profile's world
        // lives in profiles.toml.
        let text = toml::to_string(&ProfileConfig::default()).unwrap();
        assert!(!text.contains("[connection]"), "{text}");
        assert!(!text.contains("theforsakenlands"), "{text}");
        // A file an older build wrote still loads, and saving it drops
        // the table.
        let older =
            "[connection]\nhost = \"mud.example\"\nport = 4000\n\n[tick]\ninterval_secs = 45\n";
        let read: ProfileConfig = toml::from_str(older).unwrap();
        assert_eq!(read.tick.interval_secs, 45);
        assert!(!toml::to_string(&read).unwrap().contains("[connection]"));
    }

    #[test]
    fn reconnect_is_on_until_you_turn_it_off_and_only_off_reaches_the_file() {
        // A file from before the switch, the old [connection] table among
        // it, reconnects.
        let older =
            "[connection]\nhost = \"mud.example\"\nport = 4000\n\n[tick]\ninterval_secs = 45\n";
        let read: ProfileConfig = toml::from_str(older).unwrap();
        assert_eq!(read.reconnect, OnSwitch(true));
        assert!(!read.to_toml().unwrap().contains("reconnect"));
        let mut profile = Profile::default();
        assert!(profile.reconnect.is_on(), "a new profile reconnects");
        profile.reconnect = OnSwitch(false);
        let text = ProfileConfig::from_profile(&profile).to_toml().unwrap();
        assert!(
            text.lines().any(|line| line == "reconnect = false"),
            "{text}"
        );
        let mut again = Profile::default();
        ProfileConfig::from_toml(&text)
            .unwrap()
            .apply_to(&mut again);
        assert_eq!(again.reconnect, OnSwitch(false));
    }

    #[test]
    fn the_alert_presets_round_trip_and_leave_the_file_alone_while_empty() {
        let mut profile = Profile::default();
        assert!(!ProfileConfig::from_profile(&profile)
            .to_toml()
            .unwrap()
            .contains("[alerts"));
        profile.alerts.insert(
            "alert_tells".into(),
            AlertParts {
                banner: true,
                ..AlertParts::default()
            },
        );
        let text = ProfileConfig::from_profile(&profile).to_toml().unwrap();
        assert!(
            text.contains("[alerts.alert_tells]\nbanner = true\n"),
            "{text}"
        );
        let mut again = Profile::default();
        ProfileConfig::from_toml(&text)
            .unwrap()
            .apply_to(&mut again);
        assert_eq!(again.alerts, profile.alerts);
        // Loadout mode keeps them in catalog.toml.
        let mut config = ProfileConfig::from_profile(&profile);
        config.clear_catalog_items();
        let leftover = &config.alerts;
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn the_preset_edits_round_trip_and_leave_the_file_alone_while_empty() {
        use crate::loadouts::preset_edits::{EditRow, PresetEdit};
        let mut profile = Profile::default();
        let text = ProfileConfig::from_profile(&profile).to_toml().unwrap();
        assert!(!text.contains("preset_edits"), "{text}");
        let send = EditRow {
            value: "".into(),
            was: "get 1.;wield 1.".into(),
            seen: Some("get 1.;dual 1.".into()),
        };
        let edit = PresetEdit {
            triggers: BTreeMap::from([(
                "disarm.secondary".into(),
                BTreeMap::from([("send".into(), send)]),
            )]),
            ..PresetEdit::default()
        };
        profile.preset_edits.insert("disarm_buff_fade".into(), edit);
        let text = ProfileConfig::from_profile(&profile).to_toml().unwrap();
        assert!(
            text.contains(
                "[preset_edits.disarm_buff_fade.triggers.\"disarm.secondary\".send]\n\
                 value = \"\"\nwas = \"get 1.;wield 1.\"\nseen = \"get 1.;dual 1.\"\n"
            ),
            "{text}"
        );
        let mut again = Profile::default();
        ProfileConfig::from_toml(&text)
            .unwrap()
            .apply_to(&mut again);
        assert_eq!(again.preset_edits, profile.preset_edits);
        // Loadout mode keeps them in catalog.toml.
        let mut config = ProfileConfig::from_profile(&profile);
        config.clear_catalog_items();
        let leftover = &config.preset_edits;
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn round_trip_through_toml() {
        let mut config = ProfileConfig::default();
        config.aliases.push(Alias::new("greet", "wave;bow"));
        config.profile_vars.insert("target".into(), "goblin".into());
        config.triggers.push(Trigger::new(
            "tells",
            r"\w+ tells you",
            TriggerAction::Highlight {
                style: HighlightStyle {
                    fg: Some(NamedColor::Cyan),
                    ..Default::default()
                },
            },
        ));
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
        let leftover = &warnings;
        assert!(leftover.is_empty(), "{leftover:?}");
        let snapshot = ProfileConfig::from_profile(&profile);
        assert_eq!(snapshot.aliases.len(), 1);
        assert_eq!(snapshot.aliases[0].name, "greet");
    }

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    /// A live profile and its connection in a session whose 30 second
    /// tick synced to the game's tick 3 seconds after it connected, with
    /// the world hour primed and the warning already printed this cycle.
    fn synced_profile(start: tokio::time::Instant) -> (Profile, Connection) {
        let mut profile = Profile::default();
        let mut c = Connection::default();
        c.tick.start_session(&mut profile.tick, start);
        let _ = c.tick.observe_world_hour("9");
        assert!(c
            .tick
            .on_game_tick(&profile.tick, start + secs(3))
            .is_some());
        c.tick.warned_this_cycle = true;
        (profile, c)
    }

    /// Lay `incoming` over the live profile and hand it to the
    /// connection, as `#profile load` does.
    pub(super) fn lay_over(
        incoming: &ProfileConfig,
        profile: &mut Profile,
        c: &mut Connection,
    ) -> Vec<String> {
        let before = profile.tick.config.clone();
        let warnings = incoming.apply_to(profile);
        crate::profile::switch::hand_to_connection(profile, c, &before);
        warnings
    }

    #[test]
    fn a_profile_switch_mid_count_keeps_the_running_tick() {
        let t0 = tokio::time::Instant::now();
        let (mut profile, mut c) = synced_profile(t0);
        let next_fire = c.tick.next_fire(&profile.tick);

        let mut incoming = ProfileConfig::default();
        incoming.tick.auto_fire = Some("score".into());
        incoming.tick.sound = false;
        incoming.tick.reset_pattern = Some("^You feel".into());
        incoming.tick.warn_at_secs = Some(8);
        incoming.tick.warn_message = Some("Tick soon".into());
        let leftover = &lay_over(&incoming, &mut profile, &mut c);
        assert!(leftover.is_empty(), "{leftover:?}");

        let (settings, tick) = (&profile.tick, &c.tick);
        assert_eq!(tick.last_tick, Some(t0 + secs(3)));
        assert_eq!(tick.next_fire(settings), next_fire);
        assert!(tick.synced);
        assert!(tick.in_session);
        assert!(tick.warned_this_cycle);
        assert_eq!(tick.last_world_hour.as_deref(), Some("9"));
        // The new profile's settings.
        assert_eq!(settings.config.auto_fire.as_deref(), Some("score"));
        assert!(!settings.config.sound);
        assert_eq!(settings.config.warn_at_secs, Some(8));
        assert_eq!(settings.config.warn_message.as_deref(), Some("Tick soon"));
        assert!(settings.check_reset_match("You feel less tired."));
        // The same tick again is still the same tick, and the next one
        // fires with the new command.
        assert!(c.tick.on_game_tick(&profile.tick, t0 + secs(4)).is_none());
        let step = c
            .tick
            .on_game_tick(&profile.tick, t0 + secs(31))
            .expect("the next tick");
        assert_eq!(step.command.as_deref(), Some("score"));
    }

    #[test]
    fn a_profile_switch_to_another_interval_keeps_the_count() {
        let t0 = tokio::time::Instant::now();
        let (mut profile, mut c) = synced_profile(t0);
        let mut incoming = ProfileConfig::default();
        incoming.tick.interval_secs = 40;
        let _ = lay_over(&incoming, &mut profile, &mut c);
        assert_eq!(c.tick.last_tick, Some(t0 + secs(3)));
        assert_eq!(c.tick.next_fire(&profile.tick), Some(t0 + secs(43)));
        assert!(c.tick.synced);
    }

    #[test]
    fn a_switch_to_a_profile_saved_with_the_tick_off_keeps_the_running_tick() {
        // Earlier builds saved the tick off whenever the game had
        // disconnected, so many files say off that you never turned off.
        let t0 = tokio::time::Instant::now();
        let (mut profile, mut c) = synced_profile(t0);
        let next_fire = c.tick.next_fire(&profile.tick);
        let mut incoming = ProfileConfig::default();
        incoming.tick.enabled = false;
        incoming.tick.auto_fire = Some("score".into());
        let _ = lay_over(&incoming, &mut profile, &mut c);
        assert!(profile.tick.config.enabled);
        assert_eq!(c.tick.next_fire(&profile.tick), next_fire);
        assert!(c.tick.synced);
        assert_eq!(profile.tick.config.auto_fire.as_deref(), Some("score"));
        let step = c
            .tick
            .on_game_tick(&profile.tick, t0 + secs(33))
            .expect("the next tick");
        assert!(step.payload.fired);
        // Saved again, the profile now keeps the tick on.
        assert!(ProfileConfig::from_profile(&profile).tick.enabled);
    }

    #[test]
    fn a_profile_saved_with_the_tick_off_loads_it_off_between_sessions() {
        let mut profile = Profile::default();
        let mut c = Connection::default();
        let mut incoming = ProfileConfig::default();
        incoming.tick.enabled = false;
        let _ = lay_over(&incoming, &mut profile, &mut c);
        assert!(!profile.tick.config.enabled);
        assert_eq!(c.tick.next_fire(&profile.tick), None);
    }

    #[test]
    fn a_tick_you_turned_off_stays_off_across_a_switch_to_one_saved_off() {
        let t0 = tokio::time::Instant::now();
        let (mut profile, mut c) = synced_profile(t0);
        c.tick.disable(&mut profile.tick);
        let mut off = ProfileConfig::default();
        off.tick.enabled = false;
        let _ = lay_over(&off, &mut profile, &mut c);
        assert!(!profile.tick.config.enabled);
        assert_eq!(c.tick.next_fire(&profile.tick), None);
        assert!(c.tick.on_game_tick(&profile.tick, t0 + secs(20)).is_none());
    }

    #[test]
    fn a_profile_switch_that_turns_the_tick_on_arms_it() {
        let t0 = tokio::time::Instant::now();
        let (mut profile, mut c) = synced_profile(t0);
        // You turned the tick off with #tick disable or in Settings.
        c.tick.disable(&mut profile.tick);
        assert_eq!(c.tick.next_fire(&profile.tick), None);

        let before = tokio::time::Instant::now();
        let _ = lay_over(&ProfileConfig::default(), &mut profile, &mut c);
        let armed = c.tick.last_tick.expect("the tick runs again");
        assert!(armed >= before);
        assert_eq!(c.tick.next_fire(&profile.tick), Some(armed + secs(30)));
        assert!(!c.tick.synced);
        // It counts on its own until the game's next tick syncs it.
        assert!(c.tick.try_consume_fire(&profile.tick, armed + secs(30)));
    }

    #[test]
    fn a_profile_load_between_sessions_leaves_the_tick_stopped() {
        let t0 = tokio::time::Instant::now();
        let (mut profile, mut c) = synced_profile(t0);
        c.tick.end_session();
        let _ = lay_over(&ProfileConfig::default(), &mut profile, &mut c);
        assert!(profile.tick.config.enabled);
        assert_eq!(c.tick.next_fire(&profile.tick), None);
        // The next connection starts it.
        c.tick.start_session(&mut profile.tick, t0 + secs(100));
        assert_eq!(c.tick.next_fire(&profile.tick), Some(t0 + secs(130)));
    }

    #[test]
    fn a_profile_saved_after_the_game_disconnects_keeps_the_tick_on() {
        let t0 = tokio::time::Instant::now();
        let (profile, mut c) = synced_profile(t0);
        c.tick.end_session();
        // The exit flush, or any save while you are not connected.
        let saved = ProfileConfig::from_profile(&profile);
        assert!(saved.tick.enabled);
        let toml = saved.to_toml().expect("serializes");
        let back = ProfileConfig::from_toml(&toml).expect("parses");
        assert!(back.tick.enabled);
    }

    #[test]
    fn profile_reset_keeps_the_running_tick() {
        let t0 = tokio::time::Instant::now();
        let (mut profile, mut c) = synced_profile(t0);
        let next_fire = c.tick.next_fire(&profile.tick);
        let state = crate::app::state::AppState::default();
        let ran = crate::input::run_line(&state, &mut profile, &mut c, "#profile reset");
        assert!(ran.replaced);
        assert_eq!(c.tick.next_fire(&profile.tick), next_fire);
        assert!(c.tick.synced);
    }

    #[test]
    fn a_bad_reset_pattern_on_switch_warns_and_keeps_the_count() {
        let t0 = tokio::time::Instant::now();
        let (mut profile, mut c) = synced_profile(t0);
        let mut incoming = ProfileConfig::default();
        incoming.tick.reset_pattern = Some("[bad".into());
        let warnings = lay_over(&incoming, &mut profile, &mut c);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].starts_with("tick reset pattern rejected"));
        assert_eq!(profile.tick.config.reset_pattern.as_deref(), Some("[bad"));
        assert!(!profile.tick.check_reset_match("[bad"));
        assert_eq!(c.tick.last_tick, Some(t0 + secs(3)));
    }

    #[test]
    fn invalid_trigger_regex_warns_but_continues() {
        let mut config = ProfileConfig::default();
        config
            .triggers
            .push(Trigger::new("bad", "[unclosed", TriggerAction::Gag));
        let mut profile = Profile::default();
        let warnings = config.apply_to(&mut profile);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("rejected"));
    }
}

/// The `[prompt]` table in profile files: the legacy merge, the `[ui]`
/// copy, and the copy kept before the first table.
#[cfg(test)]
mod prompt_tests {
    use super::tests::lay_over;
    use super::*;
    use crate::disk::atomic::{release_unread, BACKUP_RETENTION};
    use crate::session::connection::Connection;
    use vosh_prompt::config::{AabahranCapture, CaptureSource, RegexCapture};
    use vosh_prompt::{CaptureConfig, PromptConfig};

    /// James's design as his profile file keeps it in `[ui]`.
    const TEMPLATE: &str = vosh_prompt::testkit::designs::JAMES;

    /// The pattern the old capture trigger held.
    const PATTERN: &str = r"\[(?<hp>\d+)/(?<maxhp>\d+)hp (?<mana>\d+)/(?<maxmana>\d+)mn (?<move>\d+)/(?<maxmove>\d+)mv\]";

    /// A profile file an older build wrote, with the switch and the
    /// design in `[ui]` and no `[prompt]`.
    fn older_file() -> String {
        format!(
            "[ui]\ntheme = \"vellum\"\nprompt_template_enabled = true\nprompt_template = {}\n",
            toml::Value::String(TEMPLATE.to_string())
        )
    }

    fn migrated() -> PromptConfig {
        PromptConfig {
            draw: true,
            template: TEMPLATE.to_string(),
            previous_templates: Vec::new(),
            capture: CaptureConfig::Regex(RegexCapture {
                lines: vec![PATTERN.to_string()],
                settle: false,
                names: BTreeMap::new(),
                seen_at: None,
                source: Some(CaptureSource::Migrated),
            }),
            ..PromptConfig::default()
        }
    }

    #[test]
    fn a_file_with_no_prompt_table_takes_the_switch_and_design_from_ui() {
        let config = ProfileConfig::from_toml(&older_file()).unwrap();
        let prompt = config.prompt.as_ref().expect("the merged table");
        assert!(prompt.draw);
        assert_eq!(prompt.template, TEMPLATE);
        assert!(prompt.capture.is_none());
        assert_eq!(config.prompt_config(), *prompt);
    }

    #[test]
    fn every_load_path_merges_the_legacy_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Healer.toml");
        std::fs::write(&path, older_file()).unwrap();
        let config = ProfileConfig::load(&path).unwrap();
        assert_eq!(config.prompt_config().template, TEMPLATE);
        assert!(config.prompt_config().draw);

        // A file with neither says nothing, and keeps saying nothing.
        let bare = ProfileConfig::from_toml("[ui]\ntheme = \"nord\"\n").unwrap();
        assert_eq!(bare.prompt, None);
        assert!(!bare.to_toml().unwrap().contains("[prompt"));
    }

    #[test]
    fn the_prompt_table_wins_over_a_ui_copy_that_drifted() {
        let text = format!(
            "{}\n[prompt]\ndraw = false\ntemplate = \"%hp\"\n",
            older_file()
        );
        let config = ProfileConfig::from_toml(&text).unwrap();
        assert_eq!(
            config.prompt_config(),
            PromptConfig::from_legacy(false, "%hp")
        );
        assert!(!config.ui.prompt_template_enabled);
        assert_eq!(config.ui.prompt_template, "%hp");
    }

    #[test]
    fn every_save_writes_the_ui_copy_beside_the_table() {
        let mut live = Profile::default();
        crate::prompt::take_config(&mut live, &mut Connection::default(), migrated());
        let text = ProfileConfig::from_profile(&live).to_toml().unwrap();
        let table: toml::Table = text.parse().unwrap();
        assert_eq!(table["ui"]["prompt_template_enabled"].as_bool(), Some(true));
        assert_eq!(table["ui"]["prompt_template"].as_str(), Some(TEMPLATE));
        assert_eq!(table["prompt"]["draw"].as_bool(), Some(true));
        assert_eq!(table["prompt"]["template"].as_str(), Some(TEMPLATE));
        let capture = &table["prompt"]["capture"];
        assert_eq!(capture["kind"].as_str(), Some("regex"));
        assert_eq!(capture["settle"].as_bool(), Some(false));
        assert_eq!(capture["source"].as_str(), Some("migrated"));
        assert_eq!(capture["lines"][0].as_str(), Some(PATTERN));
    }

    #[test]
    fn the_table_and_the_ui_copy_round_trip() {
        let mut live = Profile::default();
        crate::prompt::take_config(&mut live, &mut Connection::default(), migrated());
        let text = ProfileConfig::from_profile(&live).to_toml().unwrap();
        let back = ProfileConfig::from_toml(&text).unwrap();
        assert_eq!(back.prompt_config(), migrated());

        let mut next = Profile::default();
        let _ = back.apply_to(&mut next);
        assert_eq!(next.prompt, migrated());
        assert!(next.ui.prompt_template_enabled);
        assert_eq!(next.ui.prompt_template, TEMPLATE);
    }

    #[test]
    fn an_older_build_that_drops_the_table_still_draws_the_design() {
        let mut live = Profile::default();
        crate::prompt::take_config(&mut live, &mut Connection::default(), migrated());
        let text = ProfileConfig::from_profile(&live).to_toml().unwrap();
        // An older build reads [ui] alone and writes the file back without
        // the [prompt] table it does not know.
        let mut table: toml::Table = text.parse().unwrap();
        assert!(table.remove("prompt").is_some());
        let older = toml::to_string_pretty(&table).unwrap();

        let back = ProfileConfig::from_toml(&older).unwrap();
        let prompt = back.prompt_config();
        assert!(prompt.draw);
        assert_eq!(prompt.template, TEMPLATE);
        assert!(prompt.capture.is_none(), "the capture goes with the table");
    }

    #[test]
    fn a_profile_that_draws_nothing_leaves_the_table_out() {
        let text = ProfileConfig::from_profile(&Profile::default())
            .to_toml()
            .unwrap();
        let table: toml::Table = text.parse().unwrap();
        assert!(!table.contains_key("prompt"));
        assert_eq!(
            table["ui"]["prompt_template_enabled"].as_bool(),
            Some(false)
        );
        assert_eq!(table["ui"]["prompt_template"].as_str(), Some(""));
    }

    #[test]
    fn a_fresh_profile_file_follows_the_game_with_drawing_off() {
        let fresh = ProfileConfig::fresh();
        assert_eq!(fresh.prompt_config(), PromptConfig::fresh());
        assert!(fresh.prompt_config().mirror);
        // A file with no [prompt] says the same, so the file is the
        // defaults, and an older build reads no design.
        let text = fresh.to_toml().unwrap();
        assert_eq!(text, ProfileConfig::default().to_toml().unwrap());
        let table: toml::Table = text.parse().unwrap();
        assert!(!table.contains_key("prompt"));
        assert_eq!(table["ui"]["prompt_template"].as_str(), Some(""));

        let mut live = Profile::default();
        let mut c = Connection::default();
        let _ = lay_over(&ProfileConfig::from_toml(&text).unwrap(), &mut live, &mut c);
        assert_eq!(live.prompt, PromptConfig::fresh());
        assert!(!c.prompt.draws(), "it waits for you to turn it on");
    }

    #[test]
    fn a_launch_with_no_profile_file_starts_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert!(!set.active_path().exists());
        let mut live = Profile::default();
        let leftover = &load_at_launch(&set, &mut live);
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(live.prompt, PromptConfig::fresh());
        assert_eq!(ProfileConfig::from_profile(&live).ui.prompt_template, "");

        // A file of its own keeps what it says, a design or none. No
        // design follows the game.
        for (design, follows) in [("%hp", false), ("", true)] {
            let mut file = ProfileConfig::default();
            file.set_prompt(PromptConfig::from_legacy(false, design));
            file.save(&set.active_path()).unwrap();
            let mut live = Profile::default();
            let _ = load_at_launch(&set, &mut live);
            assert_eq!(live.prompt.template, design);
            assert_eq!(live.prompt.mirror, follows, "{design:?}");
        }
    }

    /// Same as the game for the test kit's PROMPT, for a mortal.
    fn same_as_the_game() -> String {
        vosh_prompt::card::presets::game(
            vosh_prompt::testkit::mud::PROMPT,
            "",
            vosh_prompt::aabahran::Who::default(),
        )
        .expect("the codes compile")
    }

    /// A profile file an earlier build wrote with `design` in its
    /// `[prompt]` table, your codes, drawing off, pinned, and your design
    /// before it.
    fn earlier_file(design: &str) -> String {
        let text = |s: &str| toml::Value::String(s.to_string());
        format!(
            "[ui]\nprompt_template_enabled = false\nprompt_template = {design}\n\n\
             [prompt]\ndraw = false\ntemplate = {design}\nprevious_templates = [{yours}]\n\
             show = \"pinned\"\n\n[prompt.capture]\nkind = \"aabahran\"\nprompt = {codes}\n\
             fprompt = \"\"\nsource = \"gmcp\"\n",
            design = text(design),
            yours = text(TEMPLATE),
            codes = text(vosh_prompt::testkit::mud::PROMPT),
        )
    }

    #[test]
    fn a_file_with_a_default_vosh_shipped_follows_the_game() {
        let dir = tempfile::tempdir().unwrap();
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        let mut olds = vec![vosh_prompt::DEFAULT_DESIGN];
        olds.extend(vosh_prompt::config::RETIRED_DEFAULTS);
        for old in olds {
            std::fs::write(set.active_path(), earlier_file(old)).unwrap();
            let mut live = Profile::default();
            let leftover = &load_at_launch(&set, &mut live);
            assert!(leftover.is_empty(), "{leftover:?}");
            let prompt = &live.prompt;
            assert!(prompt.mirror);
            assert_eq!(prompt.template, same_as_the_game());
            assert_eq!(live.ui.prompt_template, same_as_the_game());
            // Everything else in the table stays, and the old text is not
            // kept, since nobody chose it.
            assert!(!prompt.draw);
            assert_eq!(prompt.show, vosh_prompt::PromptShow::Pinned);
            assert_eq!(prompt.previous_templates, [TEMPLATE]);

            // The next save keeps the design in template, so an older
            // build draws it. Same as the game for the codes says the
            // design follows the game, so the file needs no mirror.
            let text = ProfileConfig::from_profile(&live).to_toml().unwrap();
            let table: toml::Table = text.parse().unwrap();
            assert!(!table["prompt"].as_table().unwrap().contains_key("mirror"));
            assert_eq!(
                table["prompt"]["template"].as_str(),
                Some(same_as_the_game().as_str())
            );
            assert_eq!(
                table["ui"]["prompt_template"].as_str(),
                Some(same_as_the_game().as_str())
            );
            let again = ProfileConfig::from_toml(&text).unwrap().prompt_config();
            assert_eq!(again, live.prompt);

            // A file older builds wrote, with the design only in [ui] and
            // no codes, follows the game with no design.
            let older = format!(
                "[ui]\nprompt_template_enabled = true\nprompt_template = {}\n",
                toml::Value::String(old.to_string())
            );
            let config = ProfileConfig::from_toml(&older).unwrap();
            let prompt = config.prompt_config();
            assert!(prompt.mirror);
            assert_eq!(prompt.template, "");
            assert_eq!(config.ui.prompt_template, "");
            assert!(prompt.draw);
        }

        // A design of your own loads as you saved it.
        let mut file = ProfileConfig::default();
        file.set_prompt(PromptConfig::from_legacy(true, TEMPLATE));
        file.save(&set.active_path()).unwrap();
        let mut live = Profile::default();
        let _ = load_at_launch(&set, &mut live);
        assert_eq!(live.prompt.template, TEMPLATE);
        assert!(!live.prompt.mirror);
    }

    #[test]
    fn vosh_default_you_chose_stays_through_a_save() {
        let mut live = Profile::default();
        let mut c = Connection::default();
        crate::prompt::take_config(
            &mut live,
            &mut c,
            PromptConfig {
                draw: true,
                capture: CaptureConfig::Aabahran(AabahranCapture {
                    prompt: vosh_prompt::testkit::mud::PROMPT.into(),
                    ..AabahranCapture::default()
                }),
                ..PromptConfig::fresh()
            },
        );
        assert_eq!(live.prompt.template, same_as_the_game());
        let mut chosen = live.prompt.clone();
        assert!(chosen.use_default_design());
        crate::prompt::take_config(&mut live, &mut c, chosen.clone());

        let text = ProfileConfig::from_profile(&live).to_toml().unwrap();
        let table: toml::Table = text.parse().unwrap();
        assert_eq!(table["prompt"]["mirror"].as_bool(), Some(false));
        let mut next = Profile::default();
        let _ = ProfileConfig::from_toml(&text).unwrap().apply_to(&mut next);
        assert_eq!(next.prompt, chosen);
        assert_eq!(next.prompt.template, vosh_prompt::DEFAULT_DESIGN);
    }

    #[test]
    fn a_design_that_follows_the_game_round_trips_and_an_older_build_draws_it() {
        let mut live = Profile::default();
        crate::prompt::take_config(
            &mut live,
            &mut Connection::default(),
            PromptConfig {
                draw: true,
                capture: CaptureConfig::Aabahran(AabahranCapture {
                    prompt: vosh_prompt::testkit::mud::PROMPT.into(),
                    ..AabahranCapture::default()
                }),
                ..PromptConfig::fresh()
            },
        );
        let follows = live.prompt.clone();
        assert!(follows.mirror);
        let text = ProfileConfig::from_profile(&live).to_toml().unwrap();
        let back = ProfileConfig::from_toml(&text).unwrap();
        assert_eq!(back.prompt_config(), follows);

        // An older build that knows the table drops mirror when it saves,
        // and the design it drew still follows the game.
        let mut table: toml::Table = text.parse().unwrap();
        let mut kept = table.clone();
        kept["prompt"].as_table_mut().unwrap().remove("mirror");
        let older = toml::to_string_pretty(&kept).unwrap();
        let prompt = ProfileConfig::from_toml(&older).unwrap().prompt_config();
        assert_eq!(prompt, follows);

        // A build from before the table reads [ui] alone and writes the
        // file back without the table. With no codes to compare, the
        // design it drew becomes yours.
        assert!(table.remove("prompt").is_some());
        let older = toml::to_string_pretty(&table).unwrap();
        let prompt = ProfileConfig::from_toml(&older).unwrap().prompt_config();
        assert!(prompt.draw);
        assert_eq!(prompt.template, same_as_the_game());
        assert!(!prompt.mirror);
    }

    #[test]
    fn a_load_hands_the_live_prompt_its_table_and_rules() {
        let mut aabahran = PromptConfig::from_legacy(true, "%hp");
        aabahran.capture = CaptureConfig::Aabahran(AabahranCapture {
            prompt: "%n%P%C[%h/%Hhp]%c".into(),
            ..AabahranCapture::default()
        });
        let mut config = ProfileConfig::default();
        config.set_prompt(aabahran.clone());
        let text = config.to_toml().unwrap();

        let mut live = Profile::default();
        let mut c = Connection::default();
        let _ = lay_over(&ProfileConfig::from_toml(&text).unwrap(), &mut live, &mut c);
        assert_eq!(*c.prompt.config(), aabahran);
        assert!(
            c.prompt.forsaken(),
            "an Aabahran capture holds the rules on any host"
        );

        // A reset hands back the default table.
        let _ = lay_over(&ProfileConfig::default(), &mut live, &mut c);
        assert!(c.prompt.config().is_default());
        assert!(!c.prompt.forsaken());
        assert!(!live.ui.prompt_template_enabled);
    }

    fn save_prompt(path: &Path, template: &str) {
        let mut config = ProfileConfig::from_toml(&older_file()).unwrap();
        config.set_prompt(PromptConfig::from_legacy(true, template));
        config.save(path).unwrap();
    }

    #[test]
    fn the_first_prompt_table_keeps_the_file_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("default.toml");
        std::fs::write(&path, older_file()).unwrap();
        let copy = before_prompt_editor_path(&path);
        assert_eq!(
            copy.file_name().unwrap().to_str(),
            Some("default.toml.before-prompt-editor")
        );

        save_prompt(&path, TEMPLATE);
        assert_eq!(std::fs::read_to_string(&copy).unwrap(), older_file());
        assert!(std::fs::read_to_string(&path).unwrap().contains("[prompt]"));

        // A dozen later saves rotate the backups and never touch the copy.
        for n in 0..(BACKUP_RETENTION + 2) {
            save_prompt(&path, &format!("%hp {n}"));
        }
        assert_eq!(std::fs::read_to_string(&copy).unwrap(), older_file());
        // Saves in the same millisecond share a backup name, so the count
        // is at most the retention.
        let backups = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains(".bak."))
            .count();
        assert!((1..=BACKUP_RETENTION).contains(&backups), "{backups}");
    }

    #[test]
    fn the_copy_is_made_once_and_never_over_a_file_that_had_the_table() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Healer.toml");
        let copy = before_prompt_editor_path(&path);

        // No file yet, nothing to keep.
        save_prompt(&path, "%hp");
        assert!(!copy.exists());
        // The file already holds a [prompt] table, so there is nothing
        // from before the prompt editor to keep.
        save_prompt(&path, "%mana");
        assert!(!copy.exists());

        // A save with no table keeps nothing either.
        let other = dir.path().join("Bard.toml");
        std::fs::write(&other, "[ui]\ntheme = \"nord\"\n").unwrap();
        ProfileConfig::from_toml("[ui]\ntheme = \"nord\"\n")
            .unwrap()
            .save(&other)
            .unwrap();
        assert!(!before_prompt_editor_path(&other).exists());
    }

    #[test]
    fn a_file_held_unread_gets_no_copy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("default.toml");
        std::fs::write(&path, "not [ toml").unwrap();
        hold_unread(&path);
        let mut config = ProfileConfig::default();
        config.set_prompt(PromptConfig::from_legacy(true, "%hp"));
        assert!(config.save(&path).is_err());
        release_unread(&path);
        assert!(!before_prompt_editor_path(&path).exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "not [ toml");
    }
}

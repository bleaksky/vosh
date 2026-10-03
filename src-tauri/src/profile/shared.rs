//! global.toml and the sharing scope. Each profile file keeps its own
//! aliases, triggers, macros and the rest, but you want the theme and
//! the font to stay put when you switch to another character. The
//! categories the scope map in profiles.toml shares live in one
//! global.toml instead, which every load lays over the profile file.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::disk::atomic::write_with_backup;
use crate::disk::custom_themes::{follow_moved_ids, merge_custom_themes};
use crate::profile::panes::DockEntryPersist;
use crate::profile::Profile;
use crate::profile_config::{
    is_default_font_family, ConfigError, CustomTheme, ProfileConfig, UiConfig,
};
use crate::profile_set::ProfileSet;

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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct GlobalConfig {
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub auto_update: Option<bool>,
    #[serde(default)]
    pub keep_last_command: Option<bool>,
    #[serde(default)]
    pub font_family: Option<String>,
    #[serde(default)]
    pub font_size: Option<u32>,
    #[serde(default)]
    pub follow_system_appearance: Option<bool>,
    #[serde(default)]
    pub light_theme: Option<String>,
    #[serde(default)]
    pub dark_theme: Option<String>,
    #[serde(default)]
    pub terminal_line_height: Option<String>,
    #[serde(default)]
    pub dock_layout: Option<Vec<DockEntryPersist>>,
    #[serde(default)]
    pub custom_themes: Option<Vec<CustomTheme>>,
}

impl GlobalConfig {
    /// Pull the global-scoped UI fields out of a live Profile,
    /// honoring the user's scope map: a field is written here only
    /// when its scope is `Scope::Global`. Fields marked Profile-
    /// scoped show up as `None`, so global.toml stays clean.
    pub(crate) fn from_profile(profile: &Profile, scope: &ScopeConfig) -> Self {
        let theme = matches!(scope.theme, Scope::Global);
        let font = matches!(scope.font, Scope::Global);
        Self {
            theme: theme.then(|| profile.ui.theme.clone()),
            follow_system_appearance: theme.then_some(profile.ui.follow_system_appearance),
            light_theme: theme.then(|| profile.ui.light_theme.clone()),
            dark_theme: theme.then(|| profile.ui.dark_theme.clone()),
            custom_themes: theme.then(|| profile.ui.custom_themes.clone()),
            auto_update: matches!(scope.auto_update, Scope::Global)
                .then_some(profile.ui.auto_update),
            keep_last_command: matches!(scope.keep_last_command, Scope::Global)
                .then_some(profile.ui.keep_last_command),
            font_family: font.then(|| profile.ui.font_family.clone()),
            font_size: font.then_some(profile.ui.font_size),
            terminal_line_height: font.then(|| profile.ui.terminal_line_height.clone()),
            dock_layout: matches!(scope.dock_layout, Scope::Global)
                .then(|| profile.ui.dock_layout.clone()),
        }
    }

    /// Apply the global fields onto a live Profile. Only writes the
    /// fields that are Some — missing values leave the existing
    /// per-profile value in place.
    pub(crate) fn apply_to(&self, profile: &mut Profile) {
        if let Some(v) = &self.theme {
            profile.ui.theme.clone_from(v);
        }
        if let Some(v) = self.auto_update {
            profile.ui.auto_update = v;
        }
        if let Some(v) = self.keep_last_command {
            profile.ui.keep_last_command = v;
        }
        if let Some(v) = &self.font_family {
            profile.ui.font_family.clone_from(v);
        }
        if let Some(v) = self.font_size {
            profile.ui.font_size = v;
        }
        if let Some(v) = &self.dock_layout {
            profile.ui.dock_layout.clone_from(v);
        }
        if let Some(v) = self.follow_system_appearance {
            profile.ui.follow_system_appearance = v;
        }
        if let Some(v) = &self.light_theme {
            profile.ui.light_theme.clone_from(v);
        }
        if let Some(v) = &self.dark_theme {
            profile.ui.dark_theme.clone_from(v);
        }
        if let Some(v) = &self.terminal_line_height {
            profile.ui.terminal_line_height.clone_from(v);
        }
        // The shared list replaces the profile's own. A profile file
        // written before custom themes joined the `theme` scope still
        // holds a list, and `migrate_custom_themes` moves it into
        // global.toml at startup, so nothing is lost and a theme you
        // deleted does not come back from an old file.
        if let Some(v) = &self.custom_themes {
            profile.ui.custom_themes.clone_from(v);
        }
    }

    pub(crate) fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let toml_str = toml::to_string_pretty(self)?;
        write_with_backup(path, &toml_str)?;
        Ok(())
    }

    pub(crate) fn load(path: &Path) -> Result<Self, ConfigError> {
        let toml_str = std::fs::read_to_string(path)?;
        let config: GlobalConfig = toml::from_str(&toml_str)?;
        Ok(config)
    }

    /// Read global.toml for the categories `scope` shares, or None when
    /// the file does not exist yet. Every load of a profile file ends by
    /// laying this over it, at launch, on a switch, and after `#profile
    /// load` or `#profile reset`, so the shared settings stay the same
    /// in all of them.
    pub(crate) fn load_shared(
        path: &Path,
        scope: &ScopeConfig,
    ) -> Result<Option<Self>, ConfigError> {
        if !path.exists() {
            return Ok(None);
        }
        let mut global = Self::load(path)?;
        global.keep_shared(scope);
        Ok(Some(global))
    }

    /// Forget the values of every category `scope` keeps per profile, so
    /// a value global.toml still holds from before cannot cover the one
    /// the profile file owns.
    fn keep_shared(&mut self, scope: &ScopeConfig) {
        if !matches!(scope.theme, Scope::Global) {
            self.theme = None;
            self.follow_system_appearance = None;
            self.light_theme = None;
            self.dark_theme = None;
            self.custom_themes = None;
        }
        if !matches!(scope.font, Scope::Global) {
            self.font_family = None;
            self.font_size = None;
            self.terminal_line_height = None;
        }
        if !matches!(scope.keep_last_command, Scope::Global) {
            self.keep_last_command = None;
        }
        if !matches!(scope.auto_update, Scope::Global) {
            self.auto_update = None;
        }
        if !matches!(scope.dock_layout, Scope::Global) {
            self.dock_layout = None;
        }
    }
}

/// The shared settings that `#profile load` and `#profile reset` keep.
/// Both replace the whole UI config with a profile file or the defaults,
/// and a profile file holds none of the settings the scope map shares.
/// Without this the live theme, custom themes, font, and the rest drop
/// to the defaults, and the next save writes those defaults into
/// global.toml for every profile.
pub(crate) struct SharedLayer {
    scope: ScopeConfig,
    file: Option<GlobalConfig>,
}

impl SharedLayer {
    /// Read global.toml the way a switch does. When the file is missing
    /// or Vosh cannot read it, the live values are the ones to keep.
    pub(crate) fn read(global_path: &Path, scope: ScopeConfig) -> Self {
        let file = GlobalConfig::load_shared(global_path, &scope).unwrap_or_else(|e| {
            tracing::warn!(error = %e, path = %global_path.display(), "global config unreadable");
            None
        });
        Self { scope, file }
    }

    /// Run `replace`, which swaps another config into `profile`, then lay
    /// the shared settings back over the result. global.toml goes on top
    /// as it does after a switch, over the values `profile` held before
    /// for any shared field the file lacks.
    pub(crate) fn keep_across<R>(
        &self,
        profile: &mut Profile,
        replace: impl FnOnce(&mut Profile) -> R,
    ) -> R {
        let before = GlobalConfig::from_profile(profile, &self.scope);
        let out = replace(profile);
        before.apply_to(profile);
        if let Some(file) = &self.file {
            file.apply_to(profile);
        }
        out
    }
}

/// Set `field` to `value` and say whether that changed it.
fn replace_value<T: PartialEq>(field: &mut T, value: T) -> bool {
    if *field == value {
        return false;
    }
    *field = value;
    true
}

impl GlobalConfig {
    /// Give `ui` the values this holds, for each category that stops
    /// being shared, where `ui` holds none of its own. A profile file saved
    /// while a category was shared holds the defaults for it, so without
    /// this that profile opens with the defaults once the category is per
    /// profile. Values a file holds of its own stay. The shared custom
    /// themes join the file's own list through `merge_custom_themes`, so
    /// both copies stay when an id clashes, and `owner` names the profile
    /// in a label that clashes too. Returns true when `ui` changed.
    fn hand_out(&self, ui: &mut UiConfig, owner: &str) -> bool {
        let defaults = UiConfig::default();
        let mut changed = false;
        if let Some(theme) = &self.theme {
            let own_pick = ui.theme != defaults.theme
                || ui.follow_system_appearance != defaults.follow_system_appearance
                || ui.light_theme != defaults.light_theme
                || ui.dark_theme != defaults.dark_theme;
            if let Some(shared) = &self.custom_themes {
                let own = std::mem::take(&mut ui.custom_themes);
                let mut list = shared.clone();
                let landed = merge_custom_themes(&mut list, &own, owner);
                if own_pick {
                    follow_moved_ids(ui, &own, &landed);
                }
                changed |= list != own;
                ui.custom_themes = list;
            }
            if !own_pick {
                changed |= replace_value(&mut ui.theme, theme.clone());
                if let Some(v) = self.follow_system_appearance {
                    changed |= replace_value(&mut ui.follow_system_appearance, v);
                }
                if let Some(v) = &self.light_theme {
                    changed |= replace_value(&mut ui.light_theme, v.clone());
                }
                if let Some(v) = &self.dark_theme {
                    changed |= replace_value(&mut ui.dark_theme, v.clone());
                }
            }
        }
        if let Some(family) = &self.font_family {
            let own_font = !is_default_font_family(&ui.font_family)
                || ui.font_size != defaults.font_size
                || ui.terminal_line_height != defaults.terminal_line_height;
            if !own_font {
                changed |= replace_value(&mut ui.font_family, family.clone());
                if let Some(v) = self.font_size {
                    changed |= replace_value(&mut ui.font_size, v);
                }
                if let Some(v) = &self.terminal_line_height {
                    changed |= replace_value(&mut ui.terminal_line_height, v.clone());
                }
            }
        }
        if let Some(v) = self.keep_last_command {
            if ui.keep_last_command == defaults.keep_last_command {
                changed |= replace_value(&mut ui.keep_last_command, v);
            }
        }
        if let Some(v) = self.auto_update {
            if ui.auto_update == defaults.auto_update {
                changed |= replace_value(&mut ui.auto_update, v);
            }
        }
        if let Some(v) = &self.dock_layout {
            if ui.dock_layout.is_empty() && !v.is_empty() {
                ui.dock_layout.clone_from(v);
                changed = true;
            }
        }
        changed
    }
}

/// Copy the values in `shared` into the file of every profile but the
/// active one, for the categories that stop being shared, where the file
/// holds none of its own. Call with the persist lock held and before
/// global.toml drops those values. The active profile needs none of this,
/// since the save that follows writes its live values into its own file.
/// A profile that never saved a file gets one when it has values to take,
/// starting from what a switch to it loads, [`ProfileConfig::fresh`].
/// Every file is read before any is written, so a file Vosh cannot read
/// stops the move with nothing changed, and a file that does not save
/// puts back every file written before it, so a failed save changes
/// nothing either. Returns how many files it wrote, or a sentence naming
/// the profile whose file stopped the move, so the caller keeps those
/// categories shared.
pub(crate) fn hand_out_shared(set: &ProfileSet, shared: &GlobalConfig) -> Result<usize, String> {
    hand_out_shared_with(set, shared, |path, config| config.save(path))
}

/// [`hand_out_shared`] with the save given, so a test can make one fail.
fn hand_out_shared_with(
    set: &ProfileSet,
    shared: &GlobalConfig,
    mut save: impl FnMut(&Path, &ProfileConfig) -> Result<(), ConfigError>,
) -> Result<usize, String> {
    let active = set.active_name();
    let mut changed = Vec::new();
    for entry in set.list() {
        if entry.name == active {
            continue;
        }
        let path = set.profile_path(&entry.name);
        let owner = crate::profile_set::display_name(&entry.name);
        let unreadable = |e: &dyn std::fmt::Display| {
            tracing::warn!(error = %e, path = %path.display(), "profile file unreadable");
            format!(
                "Vosh could not read the {owner} profile file, so these settings stay the same for every character."
            )
        };
        // The text as it stands, so a failed save can put it back exactly.
        let before = if path.exists() {
            Some(std::fs::read_to_string(&path).map_err(|e| unreadable(&e))?)
        } else {
            None
        };
        let mut config = match &before {
            Some(text) => ProfileConfig::from_toml(text).map_err(|e| unreadable(&e))?,
            None => ProfileConfig::fresh(),
        };
        if shared.hand_out(&mut config.ui, &owner) {
            changed.push((owner, path, before, config));
        }
    }
    let written = changed.len();
    let mut touched: Vec<(PathBuf, Option<String>)> = Vec::new();
    for (owner, path, before, config) in changed {
        let saved = save(&path, &config);
        if let Err(e) = &saved {
            tracing::warn!(error = %e, path = %path.display(), "profile file kept the defaults");
        }
        // A save that fails may already have moved the file aside, so it
        // goes back with the rest.
        touched.push((path, before));
        if saved.is_err() {
            put_back(&touched);
            return Err(format!(
                "Vosh could not save the {owner} profile file, so these settings stay the same for every character."
            ));
        }
    }
    Ok(written)
}

/// Put each file in `files` back to the text it held, or take away a file
/// that did not exist before. Leaves a file that already holds its text.
pub(crate) fn put_back(files: &[(PathBuf, Option<String>)]) {
    for (path, before) in files.iter().rev() {
        let restored = match before {
            Some(text) => {
                if std::fs::read_to_string(path).ok().as_deref() == Some(text.as_str()) {
                    continue;
                }
                write_with_backup(path, text)
            }
            None if path.exists() => std::fs::remove_file(path),
            None => continue,
        };
        if let Err(e) = restored {
            tracing::error!(error = %e, path = %path.display(), "profile file could not be put back");
        }
    }
}

/// Zero out the fields whose scope is `Global` on a
/// `ProfileConfig` so the per-profile file does not duplicate
/// values that actually live in `global.toml`. Profile-scoped
/// fields are left in place. Called right before saving the per-
/// profile file.
pub(crate) fn strip_global_fields(config: &mut ProfileConfig, scope: &ScopeConfig) {
    let defaults = UiConfig::default();
    if matches!(scope.theme, Scope::Global) {
        config.ui.theme = defaults.theme;
        config.ui.follow_system_appearance = defaults.follow_system_appearance;
        config.ui.light_theme = defaults.light_theme;
        config.ui.dark_theme = defaults.dark_theme;
        config.ui.custom_themes = defaults.custom_themes;
    }
    if matches!(scope.auto_update, Scope::Global) {
        config.ui.auto_update = defaults.auto_update;
    }
    if matches!(scope.keep_last_command, Scope::Global) {
        config.ui.keep_last_command = defaults.keep_last_command;
    }
    if matches!(scope.font, Scope::Global) {
        config.ui.font_family = defaults.font_family;
        config.ui.font_size = defaults.font_size;
        config.ui.terminal_line_height = defaults.terminal_line_height;
    }
    if matches!(scope.dock_layout, Scope::Global) {
        config.ui.dock_layout = defaults.dock_layout;
    }
}

#[cfg(test)]
mod tests {
    use vosh_automation::alias::Alias;

    use super::*;
    use crate::profile::panes::tests::custom_layout;
    use crate::profile::tests::{persist_live, styled_profile, theme_ids};
    use crate::profile_config::{TrackedAffect, RETIRED_DEFAULT_FONT_FAMILY};

    #[test]
    fn global_split_round_trip() {
        // Set up a profile with both per-profile and global fields.
        let mut profile = Profile::default();
        profile.ui.theme = "tokyo-night".into();
        profile.ui.font_size = 18;
        profile.ui.keep_last_command = true;
        profile.ui.tracked_affects = vec![
            TrackedAffect {
                name: "sanc".into(),
                label: None,
            },
            TrackedAffect {
                name: "Field of Discord".into(),
                label: Some("Shroud".into()),
            },
        ];
        profile.aliases.set(Alias::new("greet", "wave"));

        // Snapshot both halves, strip the per-profile of global fields
        // (what persist_profile does before writing per-profile file).
        let scope = ScopeConfig::default();
        let mut per_profile = ProfileConfig::from_profile(&profile);
        let global = GlobalConfig::from_profile(&profile, &scope);
        strip_global_fields(&mut per_profile, &scope);

        // Per-profile lost the global fields back to defaults.
        let defaults = UiConfig::default();
        assert_eq!(per_profile.ui.theme, defaults.theme);
        assert_eq!(per_profile.ui.font_size, defaults.font_size);
        // But kept the per-profile UI fields.
        assert_eq!(per_profile.ui.tracked_affects.len(), 2);

        // Round-trip through TOML and re-apply in load order:
        // per-profile first, then global overlay.
        let per_profile_text = per_profile.to_toml().unwrap();
        let global_text = toml::to_string_pretty(&global).unwrap();
        let parsed_per = ProfileConfig::from_toml(&per_profile_text).unwrap();
        let parsed_global: GlobalConfig = toml::from_str(&global_text).unwrap();

        let mut restored = Profile::default();
        parsed_per.apply_to(&mut restored);
        parsed_global.apply_to(&mut restored);

        assert_eq!(restored.ui.theme, "tokyo-night");
        assert_eq!(restored.ui.font_size, 18);
        assert!(restored.ui.keep_last_command);
        assert_eq!(restored.ui.tracked_affects.len(), 2);
        let labeled = restored
            .ui
            .tracked_affects
            .iter()
            .find(|t| t.name == "Field of Discord")
            .expect("labeled entry survives the round-trip");
        assert_eq!(labeled.label.as_deref(), Some("Shroud"));
        assert!(restored.aliases.list().iter().any(|a| a.name == "greet"));
    }

    #[test]
    fn panes_stay_out_of_global_config() {
        let mut profile = Profile::default();
        profile.ui.panes = Some(custom_layout());
        let scope = ScopeConfig::default();
        let mut per_profile = ProfileConfig::from_profile(&profile);
        strip_global_fields(&mut per_profile, &scope);
        assert_eq!(per_profile.ui.panes, Some(custom_layout()));
        let global = GlobalConfig::from_profile(&profile, &scope);
        let global_text = toml::to_string_pretty(&global).unwrap();
        assert!(!global_text.contains("panes"), "{global_text}");
    }

    /// Mirror `persist_profile` and a reload. The stripped profile file
    /// loads first, then global.toml over it.
    fn split_and_reload(profile: &Profile, scope: &ScopeConfig) -> (ProfileConfig, Profile) {
        let mut per_profile = ProfileConfig::from_profile(profile);
        strip_global_fields(&mut per_profile, scope);
        let global_text = toml::to_string_pretty(&GlobalConfig::from_profile(profile, scope))
            .expect("global config serializes");
        let parsed_per = ProfileConfig::from_toml(&per_profile.to_toml().unwrap()).unwrap();
        let parsed_global: GlobalConfig = toml::from_str(&global_text).unwrap();
        let mut restored = Profile::default();
        parsed_per.apply_to(&mut restored);
        parsed_global.apply_to(&mut restored);
        (per_profile, restored)
    }

    #[test]
    fn theme_scope_carries_the_theme_pair_and_custom_themes() {
        let profile = styled_profile();
        let (per_profile, restored) = split_and_reload(&profile, &ScopeConfig::default());

        let defaults = UiConfig::default();
        assert!(!per_profile.ui.follow_system_appearance);
        assert_eq!(per_profile.ui.light_theme, defaults.light_theme);
        assert_eq!(per_profile.ui.dark_theme, defaults.dark_theme);
        let leftover = &per_profile.ui.custom_themes;
        assert!(leftover.is_empty(), "{leftover:?}");

        assert!(restored.ui.follow_system_appearance);
        assert_eq!(restored.ui.light_theme, "classic-vivid");
        assert_eq!(restored.ui.dark_theme, "night-ink");
        assert_eq!(restored.ui.theme, "night-ink");
        assert_eq!(restored.ui.custom_themes.len(), 1);
        assert_eq!(restored.ui.custom_themes[0].id, "night-ink");
        assert_eq!(
            restored.ui.custom_themes[0].xterm,
            profile.ui.custom_themes[0].xterm
        );
    }

    #[test]
    fn font_scope_carries_the_line_height() {
        let profile = styled_profile();
        let (per_profile, restored) = split_and_reload(&profile, &ScopeConfig::default());
        assert_eq!(per_profile.ui.terminal_line_height, "default");
        assert_eq!(restored.ui.terminal_line_height, "loose");
        assert_eq!(restored.ui.font_size, 16);
    }

    #[test]
    fn profile_scope_keeps_the_appearance_fields_in_the_profile_file() {
        use super::Scope as Kind;
        let scope = ScopeConfig {
            theme: Kind::Profile,
            font: Kind::Profile,
            ..ScopeConfig::default()
        };
        let profile = styled_profile();
        let global_text = toml::to_string_pretty(&GlobalConfig::from_profile(&profile, &scope))
            .expect("global config serializes");
        for key in [
            "follow_system_appearance",
            "light_theme",
            "dark_theme",
            "custom_themes",
            "terminal_line_height",
        ] {
            assert!(!global_text.contains(key), "{key} leaked: {global_text}");
        }
        let (per_profile, restored) = split_and_reload(&profile, &scope);
        assert!(per_profile.ui.follow_system_appearance);
        assert_eq!(per_profile.ui.custom_themes.len(), 1);
        assert_eq!(per_profile.ui.terminal_line_height, "loose");
        assert_eq!(restored.ui.dark_theme, "night-ink");
    }

    /// Every shared setting off its default, custom themes included.
    fn shared_profile() -> Profile {
        let mut profile = styled_profile();
        profile.ui.font_family = "Iosevka".into();
        profile.ui.keep_last_command = true;
        profile.ui.auto_update = true;
        profile
    }

    fn assert_shared_settings(profile: &Profile) {
        assert_eq!(profile.ui.theme, "night-ink");
        assert!(profile.ui.follow_system_appearance);
        assert_eq!(profile.ui.light_theme, "classic-vivid");
        assert_eq!(profile.ui.dark_theme, "night-ink");
        assert_eq!(theme_ids(&profile.ui.custom_themes), ["night-ink"]);
        assert_eq!(profile.ui.font_family, "Iosevka");
        assert_eq!(profile.ui.font_size, 16);
        assert_eq!(profile.ui.terminal_line_height, "loose");
        assert!(profile.ui.keep_last_command);
        assert!(profile.ui.auto_update);
    }

    /// Mirror `#profile reset` as the input command runs it.
    fn reset(set: &ProfileSet, live: &mut Profile) {
        let layer = SharedLayer::read(&set.global_path(), *set.scope());
        layer.keep_across(live, |p| crate::input::process(p, "#profile reset"));
    }

    #[test]
    fn a_profile_reset_keeps_the_shared_settings() {
        let dir = tempfile::tempdir().unwrap();
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        let mut live = shared_profile();
        live.ui.tracked_affects = vec![TrackedAffect {
            name: "Sanctuary".into(),
            label: None,
        }];
        persist_live(&set, &live);
        let global_before = std::fs::read_to_string(set.global_path()).unwrap();

        reset(&set, &mut live);

        assert_shared_settings(&live);
        // What the profile owns goes back to the defaults.
        let leftover = &live.ui.tracked_affects;
        assert!(leftover.is_empty(), "{leftover:?}");
        // The next save writes the same shared settings back.
        persist_live(&set, &live);
        let global_after = std::fs::read_to_string(set.global_path()).unwrap();
        assert_eq!(global_after, global_before);
    }

    #[test]
    fn a_profile_reset_keeps_the_live_shared_settings_without_global_toml() {
        let dir = tempfile::tempdir().unwrap();
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        let mut live = shared_profile();
        assert!(!set.global_path().exists());
        reset(&set, &mut live);
        assert_shared_settings(&live);
    }

    #[test]
    fn a_profile_reset_resets_a_category_each_profile_owns() {
        use super::Scope as Kind;
        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.set_scope(ScopeConfig {
            theme: Kind::Profile,
            ..ScopeConfig::default()
        })
        .unwrap();
        let mut live = shared_profile();
        persist_live(&set, &live);

        reset(&set, &mut live);

        let defaults = UiConfig::default();
        assert_eq!(live.ui.theme, defaults.theme);
        let leftover = &live.ui.custom_themes;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(!live.ui.follow_system_appearance);
        // The font is still shared, so it stays.
        assert_eq!(live.ui.font_size, 16);
        assert_eq!(live.ui.font_family, "Iosevka");
    }

    #[test]
    fn a_file_with_the_default_from_before_takes_the_shared_font() {
        let shared = GlobalConfig::from_profile(&shared_profile(), &ScopeConfig::default());
        // Saved while Vosh bundled Berkeley Mono, with no font picked.
        let mut ui = UiConfig {
            font_family: RETIRED_DEFAULT_FONT_FAMILY.to_string(),
            ..UiConfig::default()
        };
        assert!(shared.hand_out(&mut ui, "alt"));
        assert_eq!(ui.font_family, "Iosevka");
        assert_eq!(ui.font_size, 16);
        // A Berkeley Mono you picked is your own and stays.
        let picked = "\"BerkeleyMono Bundled\", Menlo, monospace";
        let mut own = UiConfig {
            font_family: picked.to_string(),
            ..UiConfig::default()
        };
        shared.hand_out(&mut own, "alt");
        assert_eq!(own.font_family, picked);
    }

    #[test]
    fn a_load_lays_only_the_shared_categories_over_the_profile() {
        use super::Scope as Kind;
        let dir = tempfile::tempdir().unwrap();
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        // global.toml still holds a theme from when it was shared.
        GlobalConfig::from_profile(&shared_profile(), &ScopeConfig::default())
            .save(&set.global_path())
            .unwrap();
        let scope = ScopeConfig {
            theme: Kind::Profile,
            ..ScopeConfig::default()
        };
        let global = GlobalConfig::load_shared(&set.global_path(), &scope)
            .unwrap()
            .unwrap();
        assert!(global.theme.is_none());
        assert!(global.custom_themes.is_none());
        assert!(global.light_theme.is_none());
        assert_eq!(global.font_size, Some(16));
        assert_eq!(global.keep_last_command, Some(true));
        let missing = dir.path().join("missing.toml");
        assert!(GlobalConfig::load_shared(&missing, &scope)
            .unwrap()
            .is_none());
    }

    #[test]
    fn a_failed_hand_out_puts_back_every_file_it_wrote() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        for name in ["Healer", "Test-Prompt", "Bard"] {
            set.create(name).unwrap();
        }
        // Healer and Test-Prompt saved while everything was shared, so
        // they hold the defaults. Bard never saved.
        let mut healer = ProfileConfig::default();
        healer.ui.tracked_affects = vec![TrackedAffect {
            name: "Fly".into(),
            label: None,
        }];
        healer.save(&set.profile_path("Healer")).unwrap();
        ProfileConfig::default()
            .save(&set.profile_path("Test-Prompt"))
            .unwrap();
        let read = |name: &str| std::fs::read_to_string(set.profile_path(name)).unwrap();
        let healer_before = read("Healer");
        let prompt_before = read("Test-Prompt");
        let shared = GlobalConfig::from_profile(&shared_profile(), &ScopeConfig::default());

        // The second of the three saves fails after moving its file
        // aside, the way a full disk fails `write_with_backup`.
        let mut saves = 0;
        let refused = hand_out_shared_with(&set, &shared, |path, config| {
            saves += 1;
            if saves == 2 {
                std::fs::rename(path, path.with_extension("toml.bak.1"))?;
                return Err(std::io::Error::other("disk full").into());
            }
            config.save(path)
        });

        assert_eq!(
            refused.unwrap_err(),
            "Vosh could not save the Test-Prompt profile file, so these settings stay the same for every character."
        );
        assert_eq!(saves, 2);
        assert_eq!(read("Healer"), healer_before);
        assert_eq!(read("Test-Prompt"), prompt_before);
        assert!(!set.profile_path("Bard").exists());

        // With every save working, all three take the shared settings.
        assert_eq!(hand_out_shared(&set, &shared).unwrap(), 3);
        let bard = ProfileConfig::load(&set.profile_path("Bard")).unwrap();
        assert_eq!(bard.ui.theme, "night-ink");
    }

    #[test]
    fn a_failed_hand_out_takes_away_a_file_it_made() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        // Bard never saved, so the hand out makes its file first.
        set.create("Bard").unwrap();
        set.create("Healer").unwrap();
        ProfileConfig::default()
            .save(&set.profile_path("Healer"))
            .unwrap();
        let shared = GlobalConfig::from_profile(&shared_profile(), &ScopeConfig::default());

        let mut saves = 0;
        let refused = hand_out_shared_with(&set, &shared, |path, config| {
            saves += 1;
            if saves == 2 {
                return Err(std::io::Error::other("disk full").into());
            }
            config.save(path)
        });

        assert!(refused.is_err());
        assert!(!set.profile_path("Bard").exists());
    }
}

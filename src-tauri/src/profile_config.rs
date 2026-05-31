//! Per-profile TOML serialization. Phase 9.
//!
//! [`ProfileConfig`] is a serde-friendly snapshot of the parts of a
//! [`crate::profile::Profile`] that survive across app launches. The runtime
//! Profile holds extra state (compiled regex, Lua engine, tick deadlines)
//! that does not belong in the on-disk file.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;
use vosh_alias::Alias;
use vosh_trigger::Trigger;
use vosh_vars::Scope;

use crate::profile::{Macro, Profile};
use crate::profile_set::ScopeConfig;
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
    /// Keyboard macro bindings.
    #[serde(default)]
    pub macros: Vec<Macro>,
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
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub(crate) struct PluginsPersist {
    /// Names of plugins to load on startup.
    #[serde(default)]
    pub enabled: Vec<String>,
}

/// One tracked-affect entry. `name` is what the server actually
/// pushes in the Char.Affects feed (matched case-insensitively); the
/// optional `label` is what we display in the affects bar. Without a
/// label, the name itself shows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct TrackedAffect {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Custom deserializer that accepts both the legacy bare-string list
/// (`tracked_affects = ["sanc", "haste"]`) and the new table list
/// (`[[ui.tracked_affects]] name = "sanc" label = "S"`). The Settings
/// UI emits the table form going forward; older profile.toml files
/// keep loading without manual migration.
fn deserialize_tracked_affects<'de, D>(deser: D) -> Result<Vec<TrackedAffect>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Row {
        Bare(String),
        Full(TrackedAffect),
    }
    let raw: Vec<Row> = Vec::deserialize(deser)?;
    Ok(raw
        .into_iter()
        .map(|r| match r {
            Row::Bare(name) => TrackedAffect { name, label: None },
            Row::Full(t) => t,
        })
        .collect())
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
    ///
    /// Each entry can carry an optional display `label` so the in-world
    /// name the server pushes (e.g. "Field of Discord") can be shown
    /// under a shorter chosen handle ("Shroud"). The wire format
    /// accepts either the new `{name, label}` table shape or the
    /// legacy bare-string list and promotes strings into the table
    /// shape transparently.
    #[serde(default, deserialize_with = "deserialize_tracked_affects")]
    pub tracked_affects: Vec<TrackedAffect>,
    /// Preset ids enabled in the Highlights drawer. On startup the
    /// frontend re-installs these presets' bundled triggers so users
    /// don't have to re-toggle each launch.
    #[serde(default)]
    pub enabled_presets: Vec<String>,
    /// Persistent dock layout for the side-panel sections. Authored
    /// in the standalone Layout Editor window; the main window reads
    /// this at startup and listens for `vosh://dock-layout-changed`
    /// to pick up live edits without a relaunch.
    #[serde(default)]
    pub dock_layout: Vec<DockEntryPersist>,
    /// When true, the input bar restores the last submitted line and
    /// selects it so pressing Enter resends. Useful for grinding the
    /// same command (e.g. `kill orc`) repeatedly. Off by default.
    #[serde(default)]
    pub keep_last_command: bool,
    /// When true, the chrome theme also tints the terminal's 16
    /// ANSI palette (legacy behavior). When false (the default),
    /// the terminal uses the canonical xterm-256 ANSI palette so
    /// server output reads identically across themes.
    #[serde(default)]
    pub theme_terminal_colors: bool,
    /// User-authored themes. Each entry mirrors the `AppTheme`
    /// shape the frontend ships built-in themes as; on app load
    /// these get appended to the built-in list so the user can
    /// pick them from the theme dropdown like any other theme.
    #[serde(default)]
    pub custom_themes: Vec<CustomTheme>,
    /// CSS color string applied to the 1px border that separates the
    /// split-scrollback history pane from the live pane. Empty or
    /// missing means use the theme default (`--c-border`). Any valid
    /// CSS color is accepted; e.g. `#ff00ff`, `rgb(255, 0, 0)`.
    #[serde(default)]
    pub split_divider_color: Option<String>,
    /// When true, the left and right panel zones extend the full
    /// height of the window and the terminal input + status bar live
    /// only under the terminal column. When false (default), input
    /// and status bar span the whole window width below the panels.
    #[serde(default)]
    pub side_panels_fill_height: bool,
    /// Milliseconds to wait between lines when sending a multi-line
    /// paste. MUDs often kick clients that send too many commands
    /// too fast. 0 means send back-to-back with no pacing. Default
    /// 500ms = ~2 lines/sec, safe for most worlds.
    #[serde(default = "default_paste_line_delay_ms")]
    pub paste_line_delay_ms: u32,
    /// Per-row appearance of the vitals panel. Toggles which columns
    /// render (bar / percent / numeric / delta) and overrides the
    /// bar glyphs and width. Default mirrors the historical look.
    #[serde(default)]
    pub vitals: VitalsConfig,
    /// Where to render the World.Moons phase glyphs in the status bar.
    /// Values: `"right-edge"` (the historical placement, far right of
    /// the status bar), `"before-time"` (left of the centered tick +
    /// MUD time chip), `"after-time"` (right of that same chip).
    /// Unknown values coerce back to `"right-edge"` server-side.
    #[serde(default = "default_moons_position")]
    pub moons_position: String,
}

/// Vitals row appearance. Each `show_*` toggle controls whether the
/// matching column renders; turn them all off except `show_numeric`
/// to get a prompt-style `hp 850/1000` readout with no bar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct VitalsConfig {
    #[serde(default = "default_true")]
    pub show_bar: bool,
    #[serde(default = "default_true")]
    pub show_percent: bool,
    #[serde(default = "default_true")]
    pub show_numeric: bool,
    #[serde(default = "default_true")]
    pub show_delta: bool,
    /// Glyph repeated to fill the lit portion of the bar.
    #[serde(default = "default_bar_filled")]
    pub bar_filled: String,
    /// Glyph repeated to fill the unlit portion of the bar.
    #[serde(default = "default_bar_empty")]
    pub bar_empty: String,
    /// Total cells in the bar (filled + empty). Clamped to [4, 60].
    #[serde(default = "default_bar_width")]
    pub bar_width: u32,
    /// Bar render style: "solid" (each cell renders `bar_filled` or
    /// `bar_empty`, repeated as glyphs — the historical look) or
    /// "track" (a CSS-tinted div bar that smoothly fills left-to-
    /// right at any percentage, no glyph dependency). The old
    /// "ramped" Unicode-partial-block mode looked rough in most
    /// monospace fonts and was removed; old configs get coerced to
    /// "solid" by the command handler.
    #[serde(default = "default_bar_style")]
    pub bar_style: String,
    /// Row layout: "stacked" puts each vital on its own row (the
    /// historical look); "inline" packs all three vitals into a
    /// single horizontal row like the tintin nprompt
    /// `hp 850(85%) mn 230(76%) mv 120(60%)` format. Frontend
    /// validates the value and falls back to stacked on unknown
    /// strings.
    #[serde(default = "default_vitals_layout")]
    pub layout: String,
    /// Color source for the percent text: "fill" (the per-vital
    /// color ramp; matches the bar color) or "gradient" (a 0-100
    /// red-to-green ramp; the percent itself becomes the at-a-glance
    /// health indicator regardless of which vital it belongs to).
    #[serde(default = "default_percent_color")]
    pub percent_color: String,
    /// When true, render the vitals row from `template` instead of
    /// the built-in stacked / inline layouts. Token reference lives
    /// in the Settings panel's help text; the renderer ignores all
    /// other vitals.* render fields except the bar_* settings (which
    /// drive the `%bar_*` tokens) and `percent_color` (which colors
    /// the `%pct_*` tokens).
    #[serde(default)]
    pub template_enabled: bool,
    /// Free-form template authored by the user. See the Settings UI
    /// for the token list. Tintin nprompt-like default:
    /// `%hp(%pct_hp)h %mn(%pct_mn)m %mv(%pct_mv)v - (%tick) - %time`.
    #[serde(default = "default_template")]
    pub template: String,
}

impl Default for VitalsConfig {
    fn default() -> Self {
        Self {
            show_bar: true,
            show_percent: true,
            show_numeric: true,
            show_delta: true,
            bar_filled: default_bar_filled(),
            bar_empty: default_bar_empty(),
            bar_width: default_bar_width(),
            bar_style: default_bar_style(),
            layout: default_vitals_layout(),
            percent_color: default_percent_color(),
            template_enabled: false,
            template: default_template(),
        }
    }
}

fn default_template() -> String {
    "%hp(%pct_hp)h %mn(%pct_mn)m %mv(%pct_mv)v - (%tick) - %time".to_string()
}

fn default_vitals_layout() -> String {
    "stacked".to_string()
}

fn default_percent_color() -> String {
    "fill".to_string()
}

fn default_bar_filled() -> String {
    "▰".to_string()
}

fn default_bar_empty() -> String {
    "▱".to_string()
}

fn default_bar_width() -> u32 {
    20
}

fn default_bar_style() -> String {
    "solid".to_string()
}

fn default_moons_position() -> String {
    "right-edge".to_string()
}

fn default_paste_line_delay_ms() -> u32 {
    500
}

/// User-authored theme. All fields are colors (or strings, for
/// metadata) that round-trip through serde to the frontend's
/// `AppTheme` shape verbatim. The frontend is the canonical
/// owner of which fields exist; this struct just stores them as
/// a flat map so adding a new color slot only touches the
/// frontend.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct CustomTheme {
    /// Stable identifier. Treated as a key when the user picks
    /// this theme in the settings dropdown.
    pub id: String,
    /// Display label for the picker.
    pub label: String,
    /// Optional one-line description.
    #[serde(default)]
    pub description: String,
    /// xterm palette. Keys: background, foreground, cursor,
    /// cursorAccent, selectionBackground, selectionForeground,
    /// black .. brightWhite. The Rust side just round-trips a
    /// string-to-string map so the schema stays anchored on the
    /// frontend.
    #[serde(default)]
    pub xterm: std::collections::BTreeMap<String, String>,
    /// Chrome palette. Keys: surfaceDeep, surface, surfacePane,
    /// surfaceLift, surfaceEmphasis, textStrong, text, textMuted,
    /// textFaint, textDim, borderSoft, border, borderStrong,
    /// borderHover, accent, accentSoft, warn, danger, info, success.
    #[serde(default)]
    pub chrome: std::collections::BTreeMap<String, String>,
}

/// On-disk representation of a single docked bar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DockEntryPersist {
    pub id: String,
    pub zone: String,
    /// Vertical alignment within a `left` or `right` zone: `"top"` or
    /// `"bottom"`. Ignored for `top`, `bottom`, and `hidden` zones.
    /// Missing means top (the default stacking behavior).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<String>,
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
            keep_last_command: false,
            theme_terminal_colors: false,
            custom_themes: Vec::new(),
            split_divider_color: None,
            side_panels_fill_height: false,
            paste_line_delay_ms: default_paste_line_delay_ms(),
            vitals: VitalsConfig::default(),
            moons_position: default_moons_position(),
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

fn default_interval() -> u64 {
    30
}

fn default_true() -> bool {
    true
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
            keep_last_command: profile.ui.keep_last_command,
            theme_terminal_colors: profile.ui.theme_terminal_colors,
            custom_themes: profile.ui.custom_themes.clone(),
            split_divider_color: profile.ui.split_divider_color.clone(),
            side_panels_fill_height: profile.ui.side_panels_fill_height,
            paste_line_delay_ms: profile.ui.paste_line_delay_ms,
            vitals: profile.ui.vitals.clone(),
            moons_position: profile.ui.moons_position.clone(),
        };

        let plugins = PluginsPersist {
            enabled: profile.plugins.enabled.clone(),
        };

        let disabled_alias_groups = profile.aliases.disabled_groups();
        let disabled_trigger_groups = profile.triggers.disabled_groups();
        let disabled_macro_groups: Vec<String> =
            profile.disabled_macro_groups.iter().cloned().collect();

        Self {
            connection: ConnectionConfig::default(),
            aliases,
            profile_vars,
            triggers,
            tick,
            autoload_scripts: Vec::new(),
            ui,
            plugins,
            macros: profile.macros.clone(),
            disabled_alias_groups,
            disabled_trigger_groups,
            disabled_macro_groups,
        }
    }

    /// Apply a snapshot onto a live profile, replacing the relevant pieces.
    /// Triggers with invalid regex are reported and skipped.
    pub(crate) fn apply_to(&self, profile: &mut Profile) -> Vec<String> {
        let mut warnings = Vec::new();

        // Aliases: replace the store entirely.
        let mut aliases = vosh_alias::AliasStore::new();
        for alias in &self.aliases {
            aliases.set(alias.clone());
        }
        aliases.set_disabled_groups(self.disabled_alias_groups.iter().cloned());
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
        let mut vars = vosh_vars::VariableStore::new();
        for (k, v) in &self.profile_vars {
            vars.set(Scope::Profile, k.clone(), v.clone());
        }
        for (k, v) in session_only {
            vars.set(Scope::Session, k, v);
        }
        profile.vars = vars;

        // Triggers: replace, surfacing invalid regex.
        let mut triggers = vosh_trigger::TriggerStore::new();
        for t in &self.triggers {
            if let Err(e) = triggers.set(t.clone()) {
                warnings.push(format!("trigger `{}` rejected: {e}", t.name));
            }
        }
        triggers.set_disabled_groups(self.disabled_trigger_groups.iter().cloned());
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
            keep_last_command: self.ui.keep_last_command,
            theme_terminal_colors: self.ui.theme_terminal_colors,
            custom_themes: self.ui.custom_themes.clone(),
            split_divider_color: self.ui.split_divider_color.clone(),
            side_panels_fill_height: self.ui.side_panels_fill_height,
            paste_line_delay_ms: self.ui.paste_line_delay_ms,
            vitals: self.ui.vitals.clone(),
            moons_position: self.ui.moons_position.clone(),
        };

        // Plugin enabled-set is persisted; the actual load happens in the
        // PluginManager wired into AppState.
        profile.plugins = PluginsPersist {
            enabled: self.plugins.enabled.clone(),
        };

        // Macros round-trip whole; the disabled-groups set lives
        // directly on Profile because there is no MacroStore wrapper.
        profile.macros.clone_from(&self.macros);
        profile.disabled_macro_groups = self
            .disabled_macro_groups
            .iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect();

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

// ============================================================
// Global config — UI preferences that survive profile switches.
//
// Each profile has its own aliases / triggers / macros / vars
// (those live in `profiles/<name>.toml`), but the user does not
// want their theme / font / dock layout to reset every time they
// switch to a different character or MUD. Those fields live in
// a single global.toml that is applied AFTER the per-profile
// config at load time, so it always wins for the global set.
//
// Stage 3 v1 ships with a fixed default split (the categories
// listed below are global; everything else is profile-scoped).
// A future pass can add per-category Settings toggles.
// ============================================================

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
    pub dock_layout: Option<Vec<DockEntryPersist>>,
}

impl GlobalConfig {
    /// Pull the global-scoped UI fields out of a live Profile,
    /// honoring the user's scope map: a field is written here only
    /// when its scope is `Scope::Global`. Fields marked Profile-
    /// scoped show up as `None`, so global.toml stays clean.
    pub(crate) fn from_profile(profile: &Profile, scope: &ScopeConfig) -> Self {
        use crate::profile_set::Scope;
        Self {
            theme: matches!(scope.theme, Scope::Global).then(|| profile.ui.theme.clone()),
            auto_update: matches!(scope.auto_update, Scope::Global)
                .then_some(profile.ui.auto_update),
            keep_last_command: matches!(scope.keep_last_command, Scope::Global)
                .then_some(profile.ui.keep_last_command),
            font_family: matches!(scope.font, Scope::Global)
                .then(|| profile.ui.font_family.clone()),
            font_size: matches!(scope.font, Scope::Global).then_some(profile.ui.font_size),
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
        let config: GlobalConfig = toml::from_str(&toml_str)?;
        Ok(config)
    }
}

/// Zero out the fields whose scope is `Global` on a
/// `ProfileConfig` so the per-profile file does not duplicate
/// values that actually live in `global.toml`. Profile-scoped
/// fields are left in place. Called right before saving the per-
/// profile file.
pub(crate) fn strip_global_fields(config: &mut ProfileConfig, scope: &ScopeConfig) {
    use crate::profile_set::Scope;
    let defaults = UiConfig::default();
    if matches!(scope.theme, Scope::Global) {
        config.ui.theme = defaults.theme;
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
    }
    if matches!(scope.dock_layout, Scope::Global) {
        config.ui.dock_layout = defaults.dock_layout;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vosh_trigger::{HighlightStyle, NamedColor, TriggerAction, TriggerPattern};

    fn single_pattern(p: &str) -> Vec<TriggerPattern> {
        vec![TriggerPattern {
            pattern: p.to_string(),
            enabled: true,
        }]
    }

    #[test]
    fn round_trip_through_toml() {
        let mut config = ProfileConfig::default();
        config.aliases.push(Alias::new("greet", "wave;bow"));
        config.profile_vars.insert("target".into(), "goblin".into());
        config.triggers.push(Trigger {
            name: "tells".into(),
            patterns: single_pattern(r"\w+ tells you"),
            priority: 0,
            enabled: true,
            actions: vec![TriggerAction::Highlight {
                style: HighlightStyle {
                    fg: Some(NamedColor::Cyan),
                    ..Default::default()
                },
            }],
            preset: None,
            group: None,
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
    fn tracked_affects_accept_legacy_bare_strings() {
        // A profile written by an older build (Vec<String>) must still
        // load after the schema change. Each bare string promotes to
        // a `{ name, label: None }` row at deserialize time.
        let toml = r#"
[ui]
tracked_affects = ["sanc", "haste"]
"#;
        let parsed = ProfileConfig::from_toml(toml).unwrap();
        assert_eq!(parsed.ui.tracked_affects.len(), 2);
        assert_eq!(parsed.ui.tracked_affects[0].name, "sanc");
        assert!(parsed.ui.tracked_affects[0].label.is_none());
        assert_eq!(parsed.ui.tracked_affects[1].name, "haste");
    }

    #[test]
    fn tracked_affects_accept_new_table_form() {
        let toml = r#"
[ui]
[[ui.tracked_affects]]
name = "Field of Discord"
label = "Shroud"
[[ui.tracked_affects]]
name = "haste"
"#;
        let parsed = ProfileConfig::from_toml(toml).unwrap();
        assert_eq!(parsed.ui.tracked_affects.len(), 2);
        assert_eq!(parsed.ui.tracked_affects[0].name, "Field of Discord");
        assert_eq!(
            parsed.ui.tracked_affects[0].label.as_deref(),
            Some("Shroud")
        );
        assert_eq!(parsed.ui.tracked_affects[1].name, "haste");
        assert!(parsed.ui.tracked_affects[1].label.is_none());
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
            patterns: single_pattern("[unclosed"),
            priority: 0,
            enabled: true,
            actions: vec![TriggerAction::Gag],
            preset: None,
            group: None,
        });
        let mut profile = Profile::default();
        let warnings = config.apply_to(&mut profile);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("rejected"));
    }
}

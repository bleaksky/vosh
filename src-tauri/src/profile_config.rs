//! Per-profile TOML serialization. Phase 9.
//!
//! [`ProfileConfig`] is a serde-friendly snapshot of the parts of a
//! [`crate::profile::Profile`] that survive across app launches. The runtime
//! Profile holds extra state (compiled regex, Lua engine, tick deadlines)
//! that does not belong in the on-disk file.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;
use vosh_alias::Alias;
use vosh_trigger::Trigger;
use vosh_vars::Scope;

use crate::profile::{Macro, Profile, Timer};
use crate::profile_set::{ProfileSet, ScopeConfig};
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

/// Trim each entry's name and label and drop rows whose name is empty
/// after trimming (no point tracking "", it never matches a real
/// affect). An empty label goes back to None so the pane falls
/// through to the name. A name repeated in any case keeps only its
/// first row, since the Affects pane matches names without case and
/// `haste` would otherwise track `Haste` twice.
pub(crate) fn normalize_tracked_affects(list: Vec<TrackedAffect>) -> Vec<TrackedAffect> {
    let mut seen: HashSet<String> = HashSet::new();
    list.into_iter()
        .map(|t| TrackedAffect {
            name: t.name.trim().to_string(),
            label: t
                .label
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        })
        .filter(|t| !t.name.is_empty() && seen.insert(t.name.to_lowercase()))
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct UiConfig {
    /// Active theme id. Matches a built-in theme (`kanso-zen`,
    /// `tokyo-night`, ...) or a user `custom_themes` entry. The
    /// frontend owns the theme registry and falls back to the first
    /// theme if the id no longer resolves.
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Show `light_theme` or `dark_theme` to match the OS appearance.
    /// Off by default, and `theme` stays the pick while it is off. Part
    /// of the `theme` scope category.
    #[serde(default)]
    pub follow_system_appearance: bool,
    /// The theme shown while following the system and the OS is light.
    #[serde(default = "default_light_theme")]
    pub light_theme: String,
    /// The theme shown while following the system and the OS is dark.
    /// Empty until the first save. The frontend reads empty as the
    /// current theme when that theme is dark, else Obsidian Ember.
    #[serde(default)]
    pub dark_theme: String,
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
    /// Terminal row spacing: `compact`, `default`, or `loose` (1.1,
    /// 1.2, and 1.35 times the glyph height). Part of the `font` scope
    /// category. Unknown values coerce back to `default` on save.
    #[serde(default = "default_terminal_line_height")]
    pub terminal_line_height: String,
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
    /// The one-window panel's pane tree. Always per profile: it is
    /// left out of `GlobalConfig`, `ScopeConfig` and
    /// `strip_global_fields`, so each character profile keeps its own
    /// panes. None until the first edit, and `pane_layout` migrates
    /// `dock_layout` on read in the meantime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panes: Option<PaneLayoutPersist>,
    /// When true, the input bar restores the last submitted line and
    /// selects it so pressing Enter resends. Useful for grinding the
    /// same command (e.g. `kill orc`) repeatedly. Off by default.
    #[serde(default)]
    pub keep_last_command: bool,
    /// When true, the chrome theme also tints the terminal's 16
    /// ANSI palette. When false, the terminal uses the canonical
    /// xterm-256 ANSI palette so server output reads identically
    /// across themes. When unset (None), the frontend treats it as on
    /// for every theme, since the chrome derives its status colors
    /// from the same ANSI slots. An explicit user choice always wins
    /// over the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_terminal_colors: Option<bool>,
    /// Draw bright (effective ANSI 8-15) text with the heavier bold font on
    /// the native surface. Off by default so bright reads at the normal weight
    /// like the webview; explicit bold on non-bright colors stays bold either
    /// way. Most MUDs encode bright as SGR-1 bold + a base color.
    #[serde(default)]
    pub bright_bold: bool,
    /// Custom base terminal palette: 16 CSS colors (ANSI 0-15 order)
    /// used whenever tint-output-with-theme resolves off. None means
    /// the canonical xterm-256 chart. The frontend owns validation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_base_ansi: Option<Vec<String>>,
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
    /// CSS color applied to locally-echoed sent input so the user can
    /// spot their own typed commands in the scrollback. Empty or
    /// missing means no recoloring (default terminal foreground). Any
    /// `#rrggbb` hex.
    #[serde(default)]
    pub input_echo_color: Option<String>,
    /// When true (the default), commands sent by keyboard macros echo
    /// locally like typed commands, so under lag you can tell the
    /// keybind registered even before the world responds. Turn it off
    /// when stacked macro sends make the scrollback too noisy.
    #[serde(default = "default_echo_macros")]
    pub echo_macros: bool,
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
    /// Enable the webview's native spell check on the prompt input,
    /// but only when the current line starts with a chat verb (say,
    /// tell, chat, gossip, ooc, clan, immtalk, reply, `'`, `"`).
    /// MUD verbs / aliases stay un-checked. Default off — opt-in
    /// for roleplay-heavy users.
    #[serde(default)]
    pub spellcheck_prompt: bool,
    /// Shape of the command-line caret: `block` (default),
    /// `block_outline`, `half_block`, `underline`, `underline_thick`,
    /// `pipe`, or `pipe_thick`. Every shape is painted inside the same
    /// anchor box, so switching never moves the input row. Unknown
    /// values coerce back to `block` on save.
    #[serde(default = "default_input_cursor_style")]
    pub input_cursor_style: String,
    /// When true, gagged prompts captured via prompt-vars triggers
    /// are replaced with a frontend-rendered string built from
    /// `prompt_template`. The template uses the same `%name` /
    /// `%{name}` / `%name_bar:width:color` syntax as the vitals
    /// template. Off by default — opt-in.
    #[serde(default)]
    pub prompt_template_enabled: bool,
    /// Template string for the custom prompt renderer. Empty means
    /// no rendering even if enabled. See vitalsTemplate.ts for the
    /// token syntax. Captured prompt vars (whatever the user's
    /// trigger names them) plus the bar tokens are available.
    #[serde(default)]
    pub prompt_template: String,
    /// Per-row appearance of the vitals panel. Toggles which columns
    /// render (bar / percent / numeric / delta) and overrides the
    /// bar glyphs and width. Default mirrors the historical look.
    #[serde(default)]
    pub vitals: VitalsConfig,
    /// How the vitals under the panel's panes lay out: `rows` (one row
    /// per vital, the default) or `line` (Health, Mana, and Moves side
    /// by side on one row). Per profile, like the rest of the panel.
    /// Unknown values coerce back to `rows` on save.
    #[serde(default = "default_vitals_density")]
    pub vitals_density: String,
    /// What each vital's value shows: `current-max` (`186 / 1020`, the
    /// default), `current` (`186`), or `percent` (`18%`). The status
    /// line follows it while the panel is hidden. Unknown values coerce
    /// back to `current-max` on save.
    #[serde(default = "default_vitals_values")]
    pub vitals_values: String,
    /// The meter under each vital: `line` (2 px, the default), `bar`
    /// (4 px), or `none`, which drops the meters and sets the rows at
    /// the panes' 22 px pitch. Unknown values coerce back to `line`.
    #[serde(default = "default_vitals_meter")]
    pub vitals_meter: String,
    /// Warn before a vital runs low. On, a vital warns under two thirds
    /// and turns danger under one third, like the Group pane. Off, the
    /// default, it stays quiet until it drops under 20 percent.
    #[serde(default)]
    pub vitals_warn_thirds: bool,
    /// Where to render the World.Moons phase glyphs in the status bar.
    /// Values: `"right-edge"` (the historical placement, far right of
    /// the status bar), `"before-time"` (left of the centered tick +
    /// MUD time chip), `"after-time"` (right of that same chip).
    /// Unknown values coerce back to `"right-edge"` server-side.
    #[serde(default = "default_moons_position")]
    pub moons_position: String,
    /// Rendering style for tick / mud time chips. Values:
    /// `"value_only"` (just the value, no caption — minimal chrome),
    /// `"caption_value"` (caption + value, e.g. "tick 14s"),
    /// `"icon_value"` (small unicode icon + value). Default matches
    /// today's "value only" terseness for the mud time chip; with
    /// the new chip frame the tick chip switches to the same style
    /// so both chips look consistent regardless of host. Unknown
    /// values coerce back to `"value_only"` server-side.
    #[serde(default = "default_chip_style")]
    pub chip_style: String,
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
    /// Composition layout for the bar: `"plain"` (just the bar) or
    /// `"with_history"` (the bar stacked over a braille trend grid of
    /// recent hp/mn/mv samples). Independent from `bar_style` —
    /// pair any bar style with either layout. Legacy
    /// `bar_style: "spark"` migrates to `bar_style: "solid"` +
    /// `bar_layout: "with_history"` in the command handler.
    #[serde(default = "default_bar_layout")]
    pub bar_layout: String,
    /// Row layout: "ember" (the default) draws a sidebar-pane block
    /// with a caps header plus three thin fixed-color track bars;
    /// "stacked" puts each vital on its own row (the historical
    /// look); "inline" packs all three vitals into a single
    /// horizontal row like the tintin nprompt
    /// `hp 850(85%) mn 230(76%) mv 120(60%)` format. Frontend
    /// validates the value and falls back to ember on unknown
    /// strings.
    #[serde(default = "default_vitals_layout")]
    pub layout: String,
    /// Inline-only sub-style. Applies when `layout == "inline"`:
    /// "plain" keeps the historical tintin nprompt text;
    /// "drain" renders each vital in a chip with caption +
    /// percent in the top corners and an inset drain background
    /// that glows along its leading edge as it shrinks;
    /// "badge" wraps each vital in a simpler chip with the
    /// percent floating as its own small bordered pill above the
    /// upper-right corner. Frontend falls back to "plain" on
    /// unknown strings.
    #[serde(default = "default_inline_style")]
    pub inline_style: String,
    /// How the percent text gets colored across the inline
    /// underline / badge styles. "stat" mirrors the per-stat
    /// numeric color; "drain" ramps the stat color through
    /// warn-gold and orange to danger-red as the value drops
    /// (default — the visual alarm); "accent" forces the brand
    /// pink regardless of stat or value. Independent of the
    /// existing `percent_color` field, which still governs the
    /// stacked-row percent rendering.
    #[serde(default = "default_percent_color_mode")]
    pub percent_color_mode: String,
    /// Wraps the percent value in a small styled chip when set to
    /// anything other than "none". Applies to the plain inline
    /// layout (today's `(75%)` parens become a chip) and to
    /// `%pct_*` tokens in template mode (wherever the user
    /// placed them). Values: "none" (default — historical parens
    /// text), "pill" (bordered chip with the tick/time chrome),
    /// "soft" (no border, 16% wash of the percent color),
    /// "glow" (color-tinted border + soft outer glow),
    /// "drain" (chip body drains in the percent color, doubles as
    /// a tiny horizontal bar). Drain and badge inline styles
    /// have their own built-in percent rendering and ignore this
    /// setting.
    #[serde(default = "default_pct_chip_style")]
    pub pct_chip_style: String,
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
    /// CSS color string used for the hp bar's "full" end. Empty
    /// preserves the built-in green-to-red ramp; any non-empty value
    /// becomes the per-vital identity color and (when `use_color_ramp`
    /// is true) the bar still drains through red as the value drops.
    #[serde(default)]
    pub hp_color: String,
    #[serde(default)]
    pub mn_color: String,
    #[serde(default)]
    pub mv_color: String,
    /// When true (the default), the per-vital color is the "full"
    /// stop of a ramp that drains through red as the bar empties —
    /// preserves the historical low-value-reads-as-danger cue. When
    /// false, the configured color is used flat at every fill
    /// percentage.
    #[serde(default = "default_true")]
    pub use_color_ramp: bool,
    /// CSS font-family stack used **only** for the bar glyphs (the
    /// label / percent / numeric / delta columns still use the app
    /// font). Empty means "use the app font." Useful when the user
    /// wants `Berkeley` `Mono` or `JetBrains` `Mono` just for the bar to
    /// get clean partial-block / braille rendering while keeping a
    /// different font for the rest of the UI.
    #[serde(default)]
    pub bar_font: String,
    /// Pulses a soft red peripheral vignette on the main window's
    /// edges when hp drops below 30%. Additive — sits on top of the
    /// regular vitals bar rather than replacing it. `alias` lets
    /// configs saved under the older `one_with_erelei` name keep
    /// the user's choice across the rename.
    #[serde(default, alias = "one_with_erelei")]
    pub low_hp_vignette: bool,
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
            bar_layout: default_bar_layout(),
            layout: default_vitals_layout(),
            inline_style: default_inline_style(),
            percent_color_mode: default_percent_color_mode(),
            pct_chip_style: default_pct_chip_style(),
            percent_color: default_percent_color(),
            template_enabled: false,
            template: default_template(),
            hp_color: String::new(),
            mn_color: String::new(),
            mv_color: String::new(),
            use_color_ramp: true,
            bar_font: String::new(),
            low_hp_vignette: false,
        }
    }
}

fn default_template() -> String {
    "%hp(%pct_hp)h %mn(%pct_mn)m %mv(%pct_mv)v - (%tick) - %time".to_string()
}

/// Every vitals layout the settings picker offers. `ember` is the
/// ledger, kept under its original id so saved configs keep working.
/// The save path coerces anything outside this list back to the
/// default, so a layout added to the picker MUST be added here too —
/// gauges / pips / strip were once missing, which silently reset
/// every pick back to the ledger on the next load.
pub(crate) const VITALS_LAYOUTS: [&str; 6] =
    ["ember", "gauges", "pips", "strip", "stacked", "inline"];

fn default_vitals_layout() -> String {
    "ember".to_string()
}

/// Coerce an incoming layout id to a known one. A hand-edited
/// profile.toml typo falls back to the default rather than leaving the
/// panel with a layout nothing renders.
pub(crate) fn coerce_vitals_layout(layout: String) -> String {
    if VITALS_LAYOUTS.contains(&layout.as_str()) {
        layout
    } else {
        default_vitals_layout()
    }
}

fn default_percent_color() -> String {
    "fill".to_string()
}

fn default_inline_style() -> String {
    "plain".to_string()
}

fn default_percent_color_mode() -> String {
    "drain".to_string()
}

fn default_pct_chip_style() -> String {
    "none".to_string()
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

fn default_bar_layout() -> String {
    "plain".to_string()
}

fn default_bar_style() -> String {
    "solid".to_string()
}

fn default_moons_position() -> String {
    "right-edge".to_string()
}

fn default_chip_style() -> String {
    "value_only".to_string()
}

fn default_echo_macros() -> bool {
    true
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
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

/// Content types a pane can show. The panel holds at most one of
/// each, so a type doubles as its leaf's default id. Mirrored by
/// `PANE_TYPES` in src/lib/paneLayout.ts.
pub(crate) const PANE_TYPES: [&str; 5] = ["map", "affects", "group", "chat", "imm"];

/// Schema version written into every saved pane layout.
pub(crate) const PANE_LAYOUT_VERSION: u32 = 1;

/// Bounds for the panel width in CSS pixels, so a hand edit or a
/// runaway drag cannot hide the terminal or collapse the panel.
const PANEL_WIDTH_MIN: u32 = 200;
const PANEL_WIDTH_MAX: u32 = 800;

/// Deepest depth a split may sit at, counting the root split as 0.
/// Four levels (column, row, column, row) is more than a 300 px panel
/// can show; anything deeper is a hand edit and gets flattened.
const PANE_MAX_SPLIT_DEPTH: usize = 3;

/// Weights past this are clamped so summing siblings stays finite.
const PANE_MAX_WEIGHT: f64 = 1_000_000.0;

/// The one-window panel for one profile: whether it shows, how wide
/// it is, and the tree of panes inside it. The vitals footer is
/// pinned below the tree and is not part of it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct PaneLayoutPersist {
    #[serde(default = "default_pane_layout_version")]
    pub version: u32,
    #[serde(default = "default_true")]
    pub panel_open: bool,
    /// Panel width in CSS pixels. None means the stock 300 px.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel_width: Option<u32>,
    /// Always a split after `sanitize`. An empty root is valid and
    /// means the panel shows only the pinned vitals.
    #[serde(default = "default_pane_root")]
    pub root: PaneNode,
}

/// One node of the pane tree. A leaf sets `pane`; a split sets
/// `split` and `children`. One struct rather than an enum keeps a
/// hand-edited profile.toml forgiving, since `sanitize` repairs
/// whatever shape it reads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct PaneNode {
    /// Stable id the frontend keys pane state on. `sanitize` keeps it
    /// unless it is blank or already taken.
    #[serde(default)]
    pub id: String,
    /// Leaf content, one of [`PANE_TYPES`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane: Option<String>,
    /// Split direction: `"column"` stacks children top to bottom,
    /// `"row"` sets them side by side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<String>,
    /// Share of the parent split. Siblings sum to 1 after `sanitize`.
    #[serde(default = "default_pane_weight")]
    pub weight: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<PaneNode>,
    /// Per-pane settings, such as the chat pane's channel filter.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub props: BTreeMap<String, String>,
}

fn default_pane_layout_version() -> u32 {
    PANE_LAYOUT_VERSION
}

fn default_pane_weight() -> f64 {
    1.0
}

/// The map's share of the stock layout, over affects. The approved
/// boards give the Map pane 348 px and the Affects pane 315 px at
/// 1280 by 800, which shows every Affects row the boards show.
const DEFAULT_MAP_WEIGHT: f64 = 0.525;
const DEFAULT_AFFECTS_WEIGHT: f64 = 0.475;

/// Map above affects, the stock layout in the approved mockups.
fn default_pane_root() -> PaneNode {
    PaneNode::split(
        "root",
        "column",
        vec![
            PaneNode::leaf("map", DEFAULT_MAP_WEIGHT),
            PaneNode::leaf("affects", DEFAULT_AFFECTS_WEIGHT),
        ],
    )
}

/// Where each old dock panel sat on a fresh install, as
/// `(id, zone, align)`. Copied from `PANELS` in src/lib/panels.ts so
/// the migration fills ids a saved layout never mentioned the same
/// way the old frontend did.
const OLD_DOCK_DEFAULTS: [(&str, &str, &str); 8] = [
    ("map", "right", "top"),
    ("group", "right", "top"),
    ("vitals", "right", "bottom"),
    ("roomstrip", "top", "top"),
    ("chat", "hidden", "bottom"),
    ("affects", "right", "bottom"),
    ("combat", "hidden", "bottom"),
    ("imm", "hidden", "top"),
];

/// Reading order of the old zones when they fold into one column:
/// the right zone first (top then bottom stack), then the left zone,
/// then the full width strips. Hidden panels have no rank.
fn old_zone_rank(zone: &str, align: &str) -> Option<u8> {
    match (zone, align) {
        ("right", "top") => Some(0),
        ("right", _) => Some(1),
        ("left", "top") => Some(2),
        ("left", _) => Some(3),
        ("top", _) => Some(4),
        ("bottom", _) => Some(5),
        _ => None,
    }
}

impl PaneNode {
    fn leaf(pane: &str, weight: f64) -> Self {
        Self {
            id: pane.to_string(),
            pane: Some(pane.to_string()),
            split: None,
            weight,
            children: Vec::new(),
            props: BTreeMap::new(),
        }
    }

    fn split(id: &str, dir: &str, children: Vec<PaneNode>) -> Self {
        Self {
            id: id.to_string(),
            pane: None,
            split: Some(dir.to_string()),
            weight: 1.0,
            children,
            props: BTreeMap::new(),
        }
    }
}

impl PaneLayoutPersist {
    /// Map above affects with the panel open.
    pub(crate) fn default_layout() -> Self {
        Self {
            version: PANE_LAYOUT_VERSION,
            panel_open: true,
            panel_width: None,
            root: default_pane_root(),
        }
    }

    /// This panel with the stock map over affects tree, keeping whether
    /// the panel shows and how wide it is. What Reset to default puts
    /// back.
    pub(crate) fn with_default_tree(&self) -> Self {
        Self {
            version: PANE_LAYOUT_VERSION,
            panel_open: self.panel_open,
            panel_width: self.panel_width,
            root: default_pane_root(),
        }
    }

    /// Seed a profile's tree from the old zone layout the first time
    /// the profile opens in the one-window build. Mirrors
    /// `panelLayoutFromDock` in src/lib/panels.ts: unknown ids and bad
    /// zones are skipped, and ids the list never mentions take their
    /// old default placement. Vitals is pinned now, the room strip
    /// moved into the map pane, and the combat target moved into the
    /// vitals footer, so those three never become panes. An empty list
    /// (a fresh install, or someone who never customized) gives the
    /// default layout.
    pub(crate) fn from_dock(entries: &[DockEntryPersist]) -> Self {
        if entries.is_empty() {
            return Self::default_layout();
        }
        let mut placed: Vec<(&str, &str, &str)> = Vec::new();
        for entry in entries {
            let Some(&(id, _, default_align)) =
                OLD_DOCK_DEFAULTS.iter().find(|(id, _, _)| *id == entry.id)
            else {
                continue;
            };
            let zone = entry.zone.as_str();
            let valid_zone = if id == "map" {
                matches!(zone, "left" | "right" | "hidden")
            } else {
                matches!(zone, "top" | "bottom" | "left" | "right" | "hidden")
            };
            if !valid_zone || placed.iter().any(|(seen, _, _)| *seen == id) {
                continue;
            }
            let align = match entry.align.as_deref() {
                Some("top") => "top",
                Some("bottom") => "bottom",
                _ => default_align,
            };
            placed.push((id, zone, align));
        }
        for &(id, zone, align) in &OLD_DOCK_DEFAULTS {
            if !placed.iter().any(|(seen, _, _)| *seen == id) {
                placed.push((id, zone, align));
            }
        }

        let vitals_shown = placed
            .iter()
            .any(|&(id, zone, align)| id == "vitals" && old_zone_rank(zone, align).is_some());
        let mut shown: Vec<(u8, &str)> = placed
            .iter()
            .filter(|(id, _, _)| PANE_TYPES.contains(id))
            .filter_map(|&(id, zone, align)| old_zone_rank(zone, align).map(|rank| (rank, id)))
            .collect();
        // Stable, so panes sharing a zone keep their saved order.
        shown.sort_by_key(|&(rank, _)| rank);

        let has_map = shown.iter().any(|&(_, id)| id == "map");
        let has_affects = shown.iter().any(|&(_, id)| id == "affects");
        let others = shown.len() - usize::from(has_map);
        let children: Vec<PaneNode> = shown
            .iter()
            .map(|&(_, id)| PaneNode::leaf(id, migrated_weight(id, has_map, has_affects, others)))
            .collect();

        let mut layout = Self {
            version: PANE_LAYOUT_VERSION,
            panel_open: !children.is_empty() || vitals_shown,
            panel_width: None,
            root: PaneNode::split("root", "column", children),
        };
        layout.sanitize();
        layout
    }

    /// Repair a layout read from disk or sent by the frontend. Unknown
    /// pane types and repeat panes drop out, blank or clashing ids get
    /// fresh ones, a split with one child gives way to that child, a
    /// split inside a split of the same direction merges into it,
    /// splits nested deeper than [`PANE_MAX_SPLIT_DEPTH`] flatten,
    /// weights become positive shares that sum to 1 (rounded to four
    /// places), and the root is always a split. The same rules run in
    /// `sanitize` in src/lib/paneLayout.ts, and both are checked
    /// against fixtures/pane-layout/sanitize.json.
    pub(crate) fn sanitize(&mut self) {
        self.version = PANE_LAYOUT_VERSION;
        self.panel_width = self
            .panel_width
            .map(|w| w.clamp(PANEL_WIDTH_MIN, PANEL_WIDTH_MAX));
        let root = std::mem::replace(
            &mut self.root,
            PaneNode::split("root", "column", Vec::new()),
        );
        self.root = TreeSanitizer::new(&root).root(root);
    }
}

impl UiConfig {
    /// The pane layout the panel should show: the saved tree, or one
    /// migrated from `dock_layout` while this profile has none. The
    /// migration is lazy, so nothing reaches disk until the first edit
    /// and `dock_layout` stays intact for a rollback.
    pub(crate) fn pane_layout(&self) -> PaneLayoutPersist {
        match &self.panes {
            Some(saved) => {
                let mut layout = saved.clone();
                layout.sanitize();
                layout
            }
            None => PaneLayoutPersist::from_dock(&self.dock_layout),
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn count_as_f64(n: usize) -> f64 {
    n as f64
}

/// Weight for a pane migrated from the old dock. Over a single pane the
/// map takes the default layout's share. With two or more panes under
/// it the map drops to 0.45 and affects, the longest list, takes 0.3 so
/// its rows still show, and the rest share what is left. Without a map
/// the panes split evenly. `others` counts the panes that are not the
/// map. Sanitize normalizes the weights afterward.
fn migrated_weight(id: &str, has_map: bool, has_affects: bool, others: usize) -> f64 {
    if !has_map || others == 0 {
        return 1.0;
    }
    if others == 1 {
        return if id == "map" {
            DEFAULT_MAP_WEIGHT
        } else {
            DEFAULT_AFFECTS_WEIGHT
        };
    }
    match id {
        "map" => 0.45,
        "affects" => 0.3,
        _ if has_affects => 0.25 / count_as_f64(others - 1),
        _ => 0.55 / count_as_f64(others),
    }
}

/// Walks a raw tree once, handing out ids and remembering which pane
/// types it has already placed.
struct TreeSanitizer {
    /// Every non-blank id in the raw tree, so a fresh id never steals
    /// one a later node already owns.
    reserved: HashSet<String>,
    used: HashSet<String>,
    panes: HashSet<&'static str>,
}

impl TreeSanitizer {
    fn new(root: &PaneNode) -> Self {
        fn collect(node: &PaneNode, out: &mut HashSet<String>) {
            if !node.id.trim().is_empty() {
                out.insert(node.id.clone());
            }
            for child in &node.children {
                collect(child, out);
            }
        }
        let mut reserved = HashSet::new();
        collect(root, &mut reserved);
        Self {
            reserved,
            used: HashSet::new(),
            panes: HashSet::new(),
        }
    }

    /// Keep `raw` when it is non-blank and unclaimed, otherwise hand
    /// out `base`, `base-2`, `base-3`, and so on.
    fn claim_id(&mut self, raw: &str, base: &str) -> String {
        if !raw.trim().is_empty() && !self.used.contains(raw) {
            self.used.insert(raw.to_string());
            return raw.to_string();
        }
        let mut n = 1u32;
        loop {
            let candidate = if n == 1 {
                base.to_string()
            } else {
                format!("{base}-{n}")
            };
            if !self.used.contains(&candidate) && !self.reserved.contains(&candidate) {
                self.used.insert(candidate.clone());
                return candidate;
            }
            n += 1;
        }
    }

    fn root(&mut self, raw: PaneNode) -> PaneNode {
        // A bare leaf at the root gets wrapped so the root stays a split.
        let raw = if raw.pane.is_some() {
            PaneNode::split("", "column", vec![raw])
        } else {
            raw
        };
        let mut dir = split_dir(raw.split.as_deref());
        let id = self.claim_id(&raw.id, "root");
        let mut children = self.children(dir, raw.children, 0);
        // A lone split under the root takes its place, keeping the root id.
        if children.len() == 1 && children[0].pane.is_none() {
            let only = children.remove(0);
            dir = split_dir(only.split.as_deref());
            children = only.children;
        }
        PaneNode::split(&id, dir, children)
    }

    fn node(&mut self, raw: PaneNode, depth: usize) -> Option<PaneNode> {
        let weight = clean_weight(raw.weight);
        if let Some(kind) = raw.pane.as_deref() {
            let kind = pane_type(kind)?;
            if !self.panes.insert(kind) {
                return None;
            }
            let id = self.claim_id(&raw.id, kind);
            return Some(PaneNode {
                id,
                props: raw.props,
                ..PaneNode::leaf(kind, weight)
            });
        }
        let dir = split_dir(raw.split.as_deref());
        let id = self.claim_id(&raw.id, "split");
        let mut children = self.children(dir, raw.children, depth);
        if children.len() > 1 {
            return Some(PaneNode {
                weight,
                ..PaneNode::split(&id, dir, children)
            });
        }
        let mut only = children.pop()?;
        only.weight = weight;
        Some(only)
    }

    fn children(&mut self, dir: &str, raw: Vec<PaneNode>, depth: usize) -> Vec<PaneNode> {
        let mut out = Vec::new();
        for child in raw {
            let Some(node) = self.node(child, depth + 1) else {
                continue;
            };
            if node.pane.is_some() {
                out.push(node);
            } else if node.split.as_deref() == Some(dir) {
                // Same direction as this split: lift the grandchildren,
                // whose shares already sum to 1 inside the child.
                for mut grandchild in node.children {
                    grandchild.weight *= node.weight;
                    out.push(grandchild);
                }
            } else if depth + 1 > PANE_MAX_SPLIT_DEPTH {
                let leaves = collect_leaves(node.children);
                let share = node.weight / count_as_f64(leaves.len());
                for mut leaf in leaves {
                    leaf.weight = share;
                    out.push(leaf);
                }
            } else {
                out.push(node);
            }
        }
        normalize_weights(&mut out);
        out
    }
}

fn collect_leaves(nodes: Vec<PaneNode>) -> Vec<PaneNode> {
    let mut out = Vec::new();
    for node in nodes {
        if node.pane.is_some() {
            out.push(node);
        } else {
            out.extend(collect_leaves(node.children));
        }
    }
    out
}

fn pane_type(raw: &str) -> Option<&'static str> {
    let wanted = raw.trim().to_lowercase();
    PANE_TYPES.iter().copied().find(|t| *t == wanted)
}

fn split_dir(raw: Option<&str>) -> &'static str {
    match raw.map(|s| s.trim().to_lowercase()).as_deref() {
        Some("row") => "row",
        _ => "column",
    }
}

fn clean_weight(w: f64) -> f64 {
    if w.is_finite() && w > 0.0 {
        w.min(PANE_MAX_WEIGHT)
    } else {
        1.0
    }
}

/// Scale sibling weights to sum to 1 unless they already do (within
/// 0.001, which keeps a second pass from nudging rounded values), then
/// round each to four places with a floor of 0.0001.
fn normalize_weights(nodes: &mut [PaneNode]) {
    let sum: f64 = nodes.iter().map(|n| n.weight).sum();
    if sum > 0.0 && (sum - 1.0).abs() > 1e-3 {
        for node in nodes.iter_mut() {
            node.weight /= sum;
        }
    }
    for node in nodes.iter_mut() {
        node.weight = ((node.weight * 10_000.0).round() / 10_000.0).max(0.0001);
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            follow_system_appearance: false,
            light_theme: default_light_theme(),
            dark_theme: String::new(),
            auto_update: false,
            font_family: default_font_family(),
            font_size: default_font_size(),
            terminal_line_height: default_terminal_line_height(),
            tracked_affects: Vec::new(),
            enabled_presets: Vec::new(),
            dock_layout: Vec::new(),
            panes: None,
            keep_last_command: false,
            theme_terminal_colors: None,
            bright_bold: false,
            terminal_base_ansi: None,
            custom_themes: Vec::new(),
            split_divider_color: None,
            input_echo_color: None,
            echo_macros: true,
            side_panels_fill_height: false,
            paste_line_delay_ms: default_paste_line_delay_ms(),
            spellcheck_prompt: false,
            input_cursor_style: default_input_cursor_style(),
            prompt_template_enabled: false,
            prompt_template: String::new(),
            vitals: VitalsConfig::default(),
            vitals_density: default_vitals_density(),
            vitals_values: default_vitals_values(),
            vitals_meter: default_vitals_meter(),
            vitals_warn_thirds: false,
            moons_position: default_moons_position(),
            chip_style: default_chip_style(),
        }
    }
}

fn default_theme() -> String {
    "obsidian-ember".to_string()
}

fn default_light_theme() -> String {
    "vellum".to_string()
}

/// The line height ids the terminal knows. Anything else saves as the
/// default.
pub(crate) const TERMINAL_LINE_HEIGHTS: [&str; 3] = ["compact", "default", "loose"];

fn default_terminal_line_height() -> String {
    "default".to_string()
}

/// Keep a known line height id and turn anything else into `default`.
pub(crate) fn coerce_terminal_line_height(value: String) -> String {
    if TERMINAL_LINE_HEIGHTS.contains(&value.as_str()) {
        value
    } else {
        default_terminal_line_height()
    }
}

/// The vitals densities the panel knows. Anything else saves as the
/// default.
pub(crate) const VITALS_DENSITIES: [&str; 2] = ["rows", "line"];

fn default_vitals_density() -> String {
    "rows".to_string()
}

/// Keep a known vitals density and turn anything else into `rows`.
pub(crate) fn coerce_vitals_density(value: String) -> String {
    if VITALS_DENSITIES.contains(&value.as_str()) {
        value
    } else {
        default_vitals_density()
    }
}

/// The forms a vital's value takes. Anything else saves as the default.
pub(crate) const VITALS_VALUES: [&str; 3] = ["current-max", "current", "percent"];

fn default_vitals_values() -> String {
    "current-max".to_string()
}

/// Keep a known value form and turn anything else into `current-max`.
pub(crate) fn coerce_vitals_values(value: String) -> String {
    if VITALS_VALUES.contains(&value.as_str()) {
        value
    } else {
        default_vitals_values()
    }
}

/// The meters the vitals draw. Anything else saves as the default.
pub(crate) const VITALS_METERS: [&str; 3] = ["line", "bar", "none"];

fn default_vitals_meter() -> String {
    "line".to_string()
}

/// Keep a known meter and turn anything else into `line`.
pub(crate) fn coerce_vitals_meter(value: String) -> String {
    if VITALS_METERS.contains(&value.as_str()) {
        value
    } else {
        default_vitals_meter()
    }
}

fn default_font_family() -> String {
    "BerkeleyMono Nerd Font, JetBrains Mono, Fira Code, Menlo, Consolas, ui-monospace, monospace"
        .to_string()
}

fn default_font_size() -> u32 {
    14
}

fn default_input_cursor_style() -> String {
    "block".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

        Self {
            connection: ConnectionConfig::default(),
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

        // UI preferences carry across as a single clone (see the
        // matching note in `from_profile`).
        profile.ui = self.ui.clone();

        // Plugin enabled-set is persisted; the actual load happens in the
        // PluginManager wired into AppState.
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

        warnings
    }

    /// Take out the aliases, triggers, and macros, which the shared
    /// catalog holds in loadout mode. A profile file that kept a copy
    /// would lay it over the catalog at the next launch, bringing back an
    /// item you deleted or an older version of one you changed. The group
    /// checkbox lists stay, since the profile file is where they persist.
    pub(crate) fn clear_catalog_items(&mut self) {
        self.aliases.clear();
        self.triggers.clear();
        self.macros.clear();
    }

    pub(crate) fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let toml_str = toml::to_string_pretty(self)?;
        write_with_backup(path, &toml_str)?;
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
//
// The `theme` category carries theme, follow_system_appearance,
// light_theme, dark_theme, and custom_themes, so a custom theme picked
// as the global theme exists in every profile. The `font` category
// carries font_family, font_size, and terminal_line_height.
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
        use crate::profile_set::Scope;
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
        use crate::profile_set::Scope;
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

/// True when two custom themes paint the same colors under the same
/// description. Their ids and labels may differ.
fn same_colors(a: &CustomTheme, b: &CustomTheme) -> bool {
    a.description == b.description && a.xterm == b.xterm && a.chrome == b.chrome
}

fn label_taken(list: &[CustomTheme], label: &str) -> bool {
    let wanted = label.trim().to_lowercase();
    list.iter().any(|t| t.label.trim().to_lowercase() == wanted)
}

/// `label` marked with the profile it came from, for example
/// `Ember variant (Healer)`, and clear of every label in `list`.
fn owned_label(label: &str, owner: &str, list: &[CustomTheme]) -> String {
    let marked = format!("{label} ({owner})");
    if !label_taken(list, &marked) {
        return marked;
    }
    let mut n = 2;
    loop {
        let candidate = format!("{label} ({owner} {n})");
        if !label_taken(list, &candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// `base` itself when no theme in `list` holds it, else the first free
/// `base-2`, `base-3`, and so on, the way Settings picks a new id. No
/// built in theme id ends in a number, so these never shadow one.
fn free_theme_id(base: &str, list: &[CustomTheme]) -> String {
    let taken = |id: &str| list.iter().any(|t| t.id == id);
    if !taken(base) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}-{n}");
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Add each theme in `incoming` to `list` and keep every one of them.
/// A theme equal to the one that already holds its id is the same theme
/// and merges into it. A different theme under a taken id joins under a
/// fresh id, and when its label is taken too its label names `owner`.
/// Before Settings picked ids from the label, every profile named its
/// first custom theme `custom`, so two profiles often hold different
/// themes under one id. A copy an earlier move added under a fresh id is
/// found again, so a move that runs twice adds nothing twice. Returns
/// the id each incoming theme holds in `list`, in order.
fn merge_custom_themes(
    list: &mut Vec<CustomTheme>,
    incoming: &[CustomTheme],
    owner: &str,
) -> Vec<String> {
    let mut landed = Vec::with_capacity(incoming.len());
    for theme in incoming {
        let Some(holder) = list.iter().find(|t| t.id == theme.id) else {
            list.push(theme.clone());
            landed.push(theme.id.clone());
            continue;
        };
        if holder == theme {
            landed.push(theme.id.clone());
            continue;
        }
        let marked = format!("{} ({owner})", theme.label);
        let copy = list
            .iter()
            .find(|t| same_colors(t, theme) && (t.label == theme.label || t.label == marked));
        if let Some(copy) = copy {
            landed.push(copy.id.clone());
            continue;
        }
        let id = free_theme_id(&theme.id, list);
        let label = if label_taken(list, &theme.label) {
            owned_label(&theme.label, owner, list)
        } else {
            theme.label.clone()
        };
        list.push(CustomTheme {
            id: id.clone(),
            label,
            ..theme.clone()
        });
        landed.push(id);
    }
    landed
}

/// True when every theme in `held` sits in `shared` under the id
/// `landed` gives it, with the same colors.
fn holds_every_theme(shared: &[CustomTheme], held: &[CustomTheme], landed: &[String]) -> bool {
    held.len() == landed.len()
        && held
            .iter()
            .zip(landed)
            .all(|(theme, id)| shared.iter().any(|s| s.id == *id && same_colors(s, theme)))
}

/// Point `ui`'s theme, light theme, and dark theme at the ids its own
/// custom themes `held` landed under, so a theme that moved to a fresh id
/// stays the one that profile shows.
fn follow_moved_ids(ui: &mut UiConfig, held: &[CustomTheme], landed: &[String]) {
    let mut moved: BTreeMap<&str, &str> = BTreeMap::new();
    for (theme, id) in held.iter().zip(landed) {
        moved.entry(theme.id.as_str()).or_insert(id.as_str());
    }
    for field in [&mut ui.theme, &mut ui.light_theme, &mut ui.dark_theme] {
        if let Some(id) = moved.get(field.as_str()) {
            if *id != field.as_str() {
                *field = (*id).to_string();
            }
        }
    }
}

/// Profile files that still hold their own custom themes while the
/// `theme` scope category is global. Files written before custom themes
/// joined that category carry a list, and so does every profile saved
/// while the category was per profile. Each list moves into global.toml
/// once and then leaves its file, so global.toml holds the one list and
/// a theme you delete stays deleted.
pub(crate) struct HeldCustomThemes {
    files: Vec<HeldFile>,
}

struct HeldFile {
    name: String,
    path: PathBuf,
    config: ProfileConfig,
    /// The id each held theme holds in the shared list, set by `add_to`.
    landed: Vec<String>,
}

impl HeldCustomThemes {
    /// Read every profile file in `set` except `skip` and keep the ones
    /// that hold custom themes. The active profile comes first and the
    /// rest follow in index order, so when two files hold one id the
    /// themes you see now keep it. A file Vosh cannot read stays as it is.
    pub(crate) fn find(set: &ProfileSet, skip: Option<&str>) -> Self {
        let active = set.active_name();
        let mut names: Vec<&str> = set.list().iter().map(|e| e.name.as_str()).collect();
        names.sort_by_key(|name| *name != active);
        let mut files = Vec::new();
        for name in names {
            if skip == Some(name) {
                continue;
            }
            let path = set.profile_path(name);
            if !path.exists() {
                continue;
            }
            match ProfileConfig::load(&path) {
                Ok(config) if !config.ui.custom_themes.is_empty() => files.push(HeldFile {
                    name: name.to_string(),
                    path,
                    config,
                    landed: Vec::new(),
                }),
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(error = %e, path = %path.display(), "profile file unreadable");
                }
            }
        }
        Self { files }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Add every held theme to `shared` through `merge_custom_themes`,
    /// so a theme under an id `shared` already holds for a different
    /// theme joins under a fresh id instead of being dropped.
    fn add_to(&mut self, shared: &mut Vec<CustomTheme>) {
        for file in &mut self.files {
            let owner = crate::profile_set::display_name(&file.name);
            file.landed = merge_custom_themes(shared, &file.config.ui.custom_themes, &owner);
        }
    }

    /// Clear the list from each file whose every theme now sits in
    /// `shared`, and point its theme references at the ids its themes
    /// landed under. Call only after global.toml holds `shared`. A file
    /// that fails to save keeps its list, and the next launch moves it
    /// again. Returns how many files it cleared.
    fn strip(self, shared: &[CustomTheme]) -> usize {
        let mut cleared = 0;
        for file in self.files {
            let HeldFile {
                path,
                mut config,
                landed,
                ..
            } = file;
            let held = std::mem::take(&mut config.ui.custom_themes);
            if !holds_every_theme(shared, &held, &landed) {
                tracing::warn!(path = %path.display(), "profile file kept custom themes the shared list lacks");
                continue;
            }
            follow_moved_ids(&mut config.ui, &held, &landed);
            match config.save(&path) {
                Ok(()) => cleared += 1,
                Err(e) => {
                    tracing::warn!(error = %e, path = %path.display(), "profile file kept its custom themes");
                }
            }
        }
        cleared
    }
}

/// Move the custom themes that profile files still hold into
/// global.toml, then clear them from those files. Vosh runs this at
/// startup before it loads the active profile, and after the first run
/// no file holds a list, so it finds nothing to do. It leaves every file
/// alone while the `theme` scope category is per profile, since each
/// file then owns its list. Returns how many files it cleared.
pub(crate) fn migrate_custom_themes(set: &ProfileSet) -> Result<usize, ConfigError> {
    if !matches!(set.scope().theme, crate::profile_set::Scope::Global) {
        return Ok(0);
    }
    let mut held = HeldCustomThemes::find(set, None);
    if held.is_empty() {
        return Ok(0);
    }
    let path = set.global_path();
    let mut global = if path.exists() {
        GlobalConfig::load(&path)?
    } else {
        GlobalConfig::default()
    };
    let mut shared = global.custom_themes.take().unwrap_or_default();
    held.add_to(&mut shared);
    global.custom_themes = Some(shared.clone());
    // global.toml first, so a failure between the writes leaves every
    // theme on disk for the next launch to finish.
    global.save(&path)?;
    Ok(held.strip(&shared))
}

/// Fold the custom themes that the other profile files hold into the
/// live profile when the `theme` scope category turns global, save the
/// shared list to global.toml, and clear those files. Without this a
/// switch to one of those profiles lays the shared list over its own
/// and its themes are gone. `held` comes from `HeldCustomThemes::find`
/// with the active profile skipped, since the live profile holds its
/// list. Call with the new global `scope` and the persist lock held.
/// Returns true when the live list gained a theme.
pub(crate) fn share_custom_themes(
    mut held: HeldCustomThemes,
    scope: &ScopeConfig,
    global_path: &Path,
    live: &mut Profile,
) -> Result<bool, ConfigError> {
    if held.is_empty() {
        return Ok(false);
    }
    let mut shared = live.ui.custom_themes.clone();
    held.add_to(&mut shared);
    let gained = shared.len() > live.ui.custom_themes.len();
    let mut global = GlobalConfig::from_profile(live, scope);
    global.custom_themes = Some(shared.clone());
    global.save(global_path)?;
    held.strip(&shared);
    live.ui.custom_themes = shared;
    Ok(gained)
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
            let own_font = ui.font_family != defaults.font_family
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
/// A profile that never saved a file gets one when it has values to take.
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
            None => ProfileConfig::default(),
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
    use crate::profile_set::Scope;
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

/// The profile file and global.toml when Vosh could not read them at
/// launch. The live profile holds the defaults where their settings
/// belong, so a save would write those defaults over your settings, and
/// ten more saves would rotate the last good copy out of the backups.
/// [`write_with_backup`] refuses every path held here. A switch that
/// reads the files again, or a `#profile load` that reads the profile
/// file, lets them go. Held by path, so tests over their own folders
/// never meet.
static UNREAD_FILES: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

fn unread_files() -> std::sync::MutexGuard<'static, Vec<PathBuf>> {
    UNREAD_FILES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Refuse every write to `path` until [`release_unread`] lets it go.
pub(crate) fn hold_unread(path: &Path) {
    let mut files = unread_files();
    if !files.iter().any(|held| held == path) {
        files.push(path.to_path_buf());
    }
}

/// Let writes to `path` resume. The live profile holds what the file
/// says again, or no longer stands in for it.
pub(crate) fn release_unread(path: &Path) {
    unread_files().retain(|held| held != path);
}

/// True when Vosh could not read `path` at launch and still holds it.
pub(crate) fn is_unread(path: &Path) -> bool {
    unread_files().iter().any(|held| held == path)
}

/// Keep holding a file that a rename moved from `from` to `to`.
pub(crate) fn follow_unread(from: &Path, to: &Path) {
    for held in unread_files().iter_mut() {
        if held == from {
            *held = to.to_path_buf();
        }
    }
}

/// What Vosh tells you at launch when the active profile file does not
/// read.
pub(crate) fn unread_profile_notice(name: &str) -> String {
    format!(
        "Vosh could not read the {} profile file, so it will not save over it. Fix the file or \
         switch to another profile.",
        crate::profile_set::display_name(name)
    )
}

/// What Vosh tells you at launch when global.toml does not read.
pub(crate) const UNREAD_GLOBAL_NOTICE: &str = "Vosh could not read global.toml, which holds your \
     shared settings, so it will not save over it. Fix the file and restart Vosh.";

/// Load the active profile file and the shared part of global.toml into
/// `profile` at launch. A file that does not read stays as it is on
/// disk. Vosh holds it with [`hold_unread`], keeps the defaults in its
/// place for this session, and returns the sentence that tells you so.
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

/// Number of timestamped backups to retain alongside each profile /
/// global config file. Each call to a top-level `save()` rotates the
/// pre-write file off to a new backup before the new content lands,
/// so the user has a recovery window of the last N saves. 10 covers
/// enough launches that an upgrade-time bad save survives even if the
/// next few launches also touch the file.
const BACKUP_RETENTION: usize = 10;

/// Atomically write `contents` to `path`, snapshotting the current
/// on-disk file (if any) to a timestamped `.bak.<unix-ms>` sibling
/// first. After the write, prune older backups so at most
/// `BACKUP_RETENTION` remain.
///
/// Steps in order:
///   1. Ensure parent dir exists.
///   2. Write the new content to `<path>.tmp`.
///   3. Copy the existing file (if any) to `<path>.bak.<unix-ms>`.
///   4. Rename the temp file over `path`. The rename is atomic on
///      every platform we ship, so `path` holds either the old text or
///      the new, never nothing.
///   5. Prune backups: keep the `BACKUP_RETENTION` newest, delete
///      the rest.
///
/// Errors during pruning are swallowed — they should not block the
/// save from being reported as successful. A failure during steps 2 to
/// 4 is fatal, takes away the temp file, and leaves the original file
/// in place, since nothing moves it before the rename.
///
/// A file held by [`hold_unread`] is refused before any step, so no
/// save writes the defaults over settings Vosh could not read.
pub(crate) fn write_with_backup(path: &Path, contents: &str) -> std::io::Result<()> {
    if is_unread(path) {
        return Err(std::io::Error::other(format!(
            "Vosh could not read {} at launch, so it will not save over it",
            path.display()
        )));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = tmp_path_for(path);
    if let Err(e) = swap_in(path, &tmp, contents) {
        // A temp file the write left behind goes, and the original stays.
        if tmp.is_file() {
            let _ = std::fs::remove_file(&tmp);
        }
        return Err(e);
    }
    prune_backups(path, BACKUP_RETENTION);
    Ok(())
}

/// Steps 2 to 4 of [`write_with_backup`]. Nothing here moves or changes
/// `path` before the last step, the rename that swaps `tmp` in.
fn swap_in(path: &Path, tmp: &Path, contents: &str) -> std::io::Result<()> {
    std::fs::write(tmp, contents)?;
    if path.exists() {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let backup = backup_path_for(path, now_ms);
        if let Err(e) = std::fs::copy(path, &backup) {
            // A copy cut short is no backup.
            let _ = std::fs::remove_file(&backup);
            return Err(e);
        }
    }
    std::fs::rename(tmp, path)
}

fn tmp_path_for(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    name.push(".tmp");
    path.with_file_name(name)
}

fn backup_path_for(path: &Path, when_ms: u128) -> PathBuf {
    let mut name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    name.push(format!(".bak.{when_ms}"));
    path.with_file_name(name)
}

/// Delete every `<file>.bak.<digits>` sibling beyond the `keep` most
/// recent. Older backups vanish silently; nothing here is fatal.
fn prune_backups(path: &Path, keep: usize) {
    let Some(parent) = path.parent() else {
        return;
    };
    let Some(stem) = path.file_name().and_then(|s| s.to_str()) else {
        return;
    };
    let prefix = format!("{stem}.bak.");
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    let mut backups: Vec<(u128, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let suffix = name.strip_prefix(&prefix)?;
            let ts: u128 = suffix.parse().ok()?;
            Some((ts, e.path()))
        })
        .collect();
    // Newest first so the head of the list is what we keep.
    backups.sort_by_key(|(ts, _)| std::cmp::Reverse(*ts));
    for (_, path) in backups.into_iter().skip(keep) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vosh_trigger::{HighlightStyle, NamedColor, TriggerAction, TriggerPattern};

    #[test]
    fn write_with_backup_creates_initial_file_without_backup() {
        // First write to a fresh path produces just the file; no
        // backup yet because there was nothing to roll off.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_with_backup(&path, "initial = true\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "initial = true\n");
        let backups = list_backups(&path);
        assert!(backups.is_empty(), "no backups expected on first write");
    }

    #[test]
    fn write_with_backup_rolls_existing_file_off() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_with_backup(&path, "v1\n").unwrap();
        // A small sleep guarantees a distinct timestamp on the
        // second write; the millisecond resolution is normally
        // enough but back-to-back calls on a fast machine can land
        // in the same ms.
        std::thread::sleep(std::time::Duration::from_millis(2));
        write_with_backup(&path, "v2\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "v2\n");
        let backups = list_backups(&path);
        assert_eq!(backups.len(), 1, "previous version should be backed up");
        let backup_content = std::fs::read_to_string(&backups[0]).unwrap();
        assert_eq!(backup_content, "v1\n");
    }

    #[test]
    fn write_with_backup_keeps_at_most_retention_backups() {
        // Write N+5 times where N = BACKUP_RETENTION. Only the
        // BACKUP_RETENTION most-recent pre-write states should
        // remain on disk; the rest are pruned.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let count = BACKUP_RETENTION + 5;
        for i in 0..count {
            write_with_backup(&path, &format!("v{i}\n")).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let backups = list_backups(&path);
        assert_eq!(
            backups.len(),
            BACKUP_RETENTION,
            "expected exactly BACKUP_RETENTION backups; got {}",
            backups.len()
        );
    }

    #[test]
    fn write_with_backup_creates_parent_dir() {
        // Writing to a path whose parent does not yet exist must
        // create the directory tree; this mirrors how the live
        // `<app_data_dir>/profiles/` subdir is created on first
        // launch.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/under/here/config.toml");
        write_with_backup(&path, "ok\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "ok\n");
    }

    #[test]
    fn a_save_that_fails_leaves_the_old_file_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Bard.toml");
        write_with_backup(&path, "v1\n").unwrap();
        // Something holds the name of the temp file, the way a full disk
        // or a lock stops the write.
        std::fs::create_dir(tmp_path_for(&path)).unwrap();
        assert!(write_with_backup(&path, "v2\n").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "v1\n");
        assert!(tmp_path_for(&path).is_dir());

        // Once the write can land, it does, and the old text waits in a
        // backup.
        std::fs::remove_dir(tmp_path_for(&path)).unwrap();
        write_with_backup(&path, "v2\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "v2\n");
        let backups = list_backups(&path);
        assert_eq!(
            std::fs::read_to_string(backups.last().unwrap()).unwrap(),
            "v1\n"
        );
    }

    #[test]
    fn a_file_held_as_unread_is_never_written_or_moved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Healer.toml");
        std::fs::write(&path, "tracked = = [\n").unwrap();
        hold_unread(&path);
        assert!(write_with_backup(&path, "defaults = true\n").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "tracked = = [\n");
        assert!(list_backups(&path).is_empty());

        // A rename carries the hold to the new name.
        let renamed = dir.path().join("Cleric.toml");
        follow_unread(&path, &renamed);
        assert!(!is_unread(&path));
        assert!(write_with_backup(&renamed, "defaults = true\n").is_err());

        release_unread(&renamed);
        write_with_backup(&renamed, "fixed = true\n").unwrap();
        assert_eq!(std::fs::read_to_string(&renamed).unwrap(), "fixed = true\n");
    }

    /// Helper for the backup tests: list every `<file>.bak.<digits>`
    /// sibling of `path` in chronological order (oldest first).
    fn list_backups(path: &Path) -> Vec<std::path::PathBuf> {
        let Some(parent) = path.parent() else {
            return Vec::new();
        };
        let Some(stem) = path.file_name().and_then(|s| s.to_str()) else {
            return Vec::new();
        };
        let prefix = format!("{stem}.bak.");
        let mut out: Vec<(u128, std::path::PathBuf)> = std::fs::read_dir(parent)
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let suffix = name.strip_prefix(&prefix)?;
                let ts: u128 = suffix.parse().ok()?;
                Some((ts, e.path()))
            })
            .collect();
        out.sort_by_key(|(ts, _)| *ts);
        out.into_iter().map(|(_, p)| p).collect()
    }

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
            target: vosh_trigger::TriggerTarget::Line,
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
    fn normalize_tracked_affects_trims_and_drops_blank_rows() {
        let raw = vec![
            TrackedAffect {
                name: "  sanctuary ".into(),
                label: Some(" Sanc ".into()),
            },
            TrackedAffect {
                name: "   ".into(),
                label: Some("ghost".into()),
            },
            TrackedAffect {
                name: "haste".into(),
                label: Some("  ".into()),
            },
        ];
        assert_eq!(
            normalize_tracked_affects(raw),
            vec![
                TrackedAffect {
                    name: "sanctuary".into(),
                    label: Some("Sanc".into()),
                },
                TrackedAffect {
                    name: "haste".into(),
                    label: None,
                },
            ]
        );
    }

    #[test]
    fn normalize_tracked_affects_keeps_the_first_of_a_name_in_any_case() {
        let row = |name: &str, label: Option<&str>| TrackedAffect {
            name: name.into(),
            label: label.map(Into::into),
        };
        assert_eq!(
            normalize_tracked_affects(vec![
                row("Haste", Some("H")),
                row("Sanctuary", None),
                row(" haste ", None),
                row("HASTE", Some("again")),
                row("sanctuary", None),
            ]),
            vec![row("Haste", Some("H")), row("Sanctuary", None)]
        );
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
            target: vosh_trigger::TriggerTarget::Line,
        });
        let mut profile = Profile::default();
        let warnings = config.apply_to(&mut profile);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("rejected"));
    }

    #[test]
    fn every_picker_layout_survives_a_save() {
        // gauges / pips / strip regressed exactly this way: the save
        // path did not list them, so it rewrote each pick to the
        // ledger and the panel reset on the next load.
        for layout in VITALS_LAYOUTS {
            assert_eq!(coerce_vitals_layout(layout.to_string()), layout);
        }
        assert_eq!(coerce_vitals_layout("nonsense".to_string()), "ember");
    }

    fn dock(entries: &[(&str, &str, Option<&str>)]) -> Vec<DockEntryPersist> {
        entries
            .iter()
            .map(|&(id, zone, align)| DockEntryPersist {
                id: id.to_string(),
                zone: zone.to_string(),
                align: align.map(str::to_string),
            })
            .collect()
    }

    /// Pane types in reading order, for asserting tree shape.
    fn leaf_panes(node: &PaneNode) -> Vec<String> {
        if let Some(pane) = &node.pane {
            return vec![pane.clone()];
        }
        node.children.iter().flat_map(leaf_panes).collect()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    /// A layout with a nested row, a panel width and per-pane props,
    /// so round trips cover every field.
    fn custom_layout() -> PaneLayoutPersist {
        let mut chat = PaneNode::leaf("chat", 0.5);
        chat.props.insert("channel".into(), "tell".into());
        let row = PaneNode {
            weight: 0.4,
            ..PaneNode::split("split", "row", vec![PaneNode::leaf("group", 0.5), chat])
        };
        let mut layout = PaneLayoutPersist {
            version: PANE_LAYOUT_VERSION,
            panel_open: false,
            panel_width: Some(360),
            root: PaneNode::split("root", "column", vec![PaneNode::leaf("map", 0.6), row]),
        };
        layout.sanitize();
        layout
    }

    #[test]
    fn default_layout_is_map_over_affects() {
        let layout = PaneLayoutPersist::default_layout();
        assert!(layout.panel_open);
        assert_eq!(layout.panel_width, None);
        assert_eq!(layout.root.split.as_deref(), Some("column"));
        assert_eq!(leaf_panes(&layout.root), ["map", "affects"]);
        // The boards' split, 348 px over 315 px of the 663 px the two
        // panes share at 1280 by 800.
        assert!(close(layout.root.children[0].weight, 0.525));
        assert!(close(layout.root.children[1].weight, 0.475));
        // Already canonical, so a sanitize pass leaves it alone.
        let mut again = layout.clone();
        again.sanitize();
        assert_eq!(again, layout);
    }

    #[test]
    fn with_default_tree_keeps_the_panel_and_replaces_the_tree() {
        let reset = custom_layout().with_default_tree();
        assert_eq!(reset.root, PaneLayoutPersist::default_layout().root);
        assert_eq!(reset.panel_open, custom_layout().panel_open);
        assert_eq!(reset.panel_width, custom_layout().panel_width);
    }

    #[test]
    fn from_dock_empty_yields_default() {
        assert_eq!(
            PaneLayoutPersist::from_dock(&[]),
            PaneLayoutPersist::default_layout()
        );
    }

    #[test]
    fn from_dock_keeps_right_order_and_drops_vitals_roomstrip_combat_hidden() {
        let entries = dock(&[
            ("vitals", "right", Some("bottom")),
            ("affects", "right", Some("bottom")),
            ("chat", "bottom", None),
            ("map", "right", Some("top")),
            ("roomstrip", "top", None),
            ("combat", "right", Some("top")),
            ("group", "left", Some("top")),
            ("imm", "hidden", None),
        ]);
        let layout = PaneLayoutPersist::from_dock(&entries);
        assert!(layout.panel_open);
        assert_eq!(
            leaf_panes(&layout.root),
            ["map", "affects", "group", "chat"]
        );
        // Affects gets the largest share under the map so its rows show.
        let weights: Vec<f64> = layout.root.children.iter().map(|n| n.weight).collect();
        assert!(close(weights[0], 0.45));
        assert!(close(weights[1], 0.3));
        assert!(weights[2..].iter().all(|w| close(*w, 0.125)));
        // Ids are the pane types, so every read of the same dock layout
        // hands the frontend the same ids.
        assert_eq!(layout.root.children[0].id, "map");
    }

    #[test]
    fn from_dock_without_affects_splits_the_rest_under_the_map() {
        let layout = PaneLayoutPersist::from_dock(&dock(&[
            ("map", "right", Some("top")),
            ("group", "right", Some("top")),
            ("chat", "right", None),
            ("affects", "hidden", None),
        ]));
        assert_eq!(leaf_panes(&layout.root), ["map", "group", "chat"]);
        let weights: Vec<f64> = layout.root.children.iter().map(|n| n.weight).collect();
        assert!(close(weights[0], 0.45));
        assert!(weights[1..].iter().all(|w| close(*w, 0.275)));
    }

    #[test]
    fn from_dock_fills_ids_the_list_never_mentions() {
        // Only chat was ever saved. The rest take their old defaults:
        // map and group stack at the top of the right zone, affects at
        // the bottom, and chat (no align, so its bottom default) joins
        // the bottom stack ahead of affects because it comes first.
        let layout = PaneLayoutPersist::from_dock(&dock(&[("chat", "right", None)]));
        assert_eq!(
            leaf_panes(&layout.root),
            ["map", "group", "chat", "affects"]
        );
    }

    #[test]
    fn from_dock_rejects_a_map_in_a_strip() {
        // The map never allowed the top or bottom strip, so the saved
        // entry is skipped and the map falls back to the right zone.
        let layout = PaneLayoutPersist::from_dock(&dock(&[
            ("map", "top", None),
            ("affects", "hidden", None),
        ]));
        assert_eq!(leaf_panes(&layout.root), ["map", "group"]);
        assert!(close(layout.root.children[0].weight, 0.525));
        assert!(close(layout.root.children[1].weight, 0.475));
    }

    #[test]
    fn from_dock_all_hidden_closes_panel() {
        let ids = [
            "map",
            "group",
            "vitals",
            "roomstrip",
            "chat",
            "affects",
            "combat",
            "imm",
        ];
        let entries: Vec<_> = ids.iter().map(|id| (*id, "hidden", None)).collect();
        let layout = PaneLayoutPersist::from_dock(&dock(&entries));
        assert!(!layout.panel_open);
        assert!(layout.root.children.is_empty());
    }

    #[test]
    fn from_dock_vitals_alone_keeps_the_panel_open_with_no_panes() {
        let entries = dock(&[
            ("map", "hidden", None),
            ("group", "hidden", None),
            ("vitals", "left", Some("top")),
            ("affects", "hidden", None),
        ]);
        let layout = PaneLayoutPersist::from_dock(&entries);
        assert!(layout.panel_open);
        assert!(layout.root.children.is_empty());
        assert_eq!(layout.root.split.as_deref(), Some("column"));
    }

    #[test]
    fn sanitize_matches_the_shared_fixtures() {
        // The same cases run against sanitize in src/lib/paneLayout.ts,
        // so the two implementations cannot drift apart.
        let text = include_str!("../../fixtures/pane-layout/sanitize.json");
        let fixture: serde_json::Value = serde_json::from_str(text).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let mut got: PaneLayoutPersist = serde_json::from_value(case["input"].clone()).unwrap();
            got.sanitize();
            let expected: PaneLayoutPersist =
                serde_json::from_value(case["expected"].clone()).unwrap();
            assert_eq!(got, expected, "case `{name}`");
            let mut again = got.clone();
            again.sanitize();
            assert_eq!(
                again, got,
                "case `{name}` is not stable under a second pass"
            );
        }
    }

    #[test]
    fn panes_round_trip_through_toml() {
        let mut config = ProfileConfig::default();
        config.ui.panes = Some(custom_layout());
        let text = config.to_toml().unwrap();
        assert!(text.contains("[ui.panes]"), "{text}");
        let parsed = ProfileConfig::from_toml(&text).unwrap();
        assert_eq!(parsed.ui.panes, Some(custom_layout()));
    }

    #[test]
    fn profile_without_panes_loads_none() {
        let parsed = ProfileConfig::from_toml("[ui]\ntheme = \"nord\"\n").unwrap();
        assert!(parsed.ui.panes.is_none());
        assert_eq!(parsed.ui.pane_layout(), PaneLayoutPersist::default_layout());
        // Nothing is written for a profile that never touched its panes.
        assert!(!parsed.to_toml().unwrap().contains("panes"));
    }

    #[test]
    fn pane_layout_prefers_the_saved_tree_over_the_dock() {
        // Group was saved first, so it leads the right zone's top stack
        // ahead of the defaulted map, as it did in the old dock.
        let mut ui = UiConfig {
            dock_layout: dock(&[("group", "right", None)]),
            ..UiConfig::default()
        };
        assert_eq!(
            leaf_panes(&ui.pane_layout().root),
            ["group", "map", "affects"]
        );
        ui.panes = Some(custom_layout());
        assert_eq!(ui.pane_layout(), custom_layout());
        // The dock layout stays untouched for a rollback.
        assert_eq!(ui.dock_layout.len(), 1);
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

    #[test]
    fn profiles_keep_their_own_panes_across_a_switch() {
        use crate::profile_set::ProfileSet;

        // Mirrors persist_profile: per-profile file minus the global
        // fields, plus global.toml.
        fn persist(set: &ProfileSet, profile: &Profile) {
            let mut snapshot = ProfileConfig::from_profile(profile);
            strip_global_fields(&mut snapshot, set.scope());
            snapshot.save(&set.active_path()).unwrap();
            GlobalConfig::from_profile(profile, set.scope())
                .save(&set.global_path())
                .unwrap();
        }
        // Mirrors apply_profile_switch step 3.
        fn load(set: &ProfileSet) -> Profile {
            let mut profile = Profile::default();
            let path = set.active_path();
            if path.exists() {
                ProfileConfig::load(&path).unwrap().apply_to(&mut profile);
            }
            GlobalConfig::load(&set.global_path())
                .unwrap()
                .apply_to(&mut profile);
            profile
        }

        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        let first = set.active_name().to_string();
        let mut profile = Profile::default();
        profile.ui.panes = Some(custom_layout());
        persist(&set, &profile);

        set.create("alt").unwrap();
        set.switch("alt").unwrap();
        let mut alt = load(&set);
        assert!(alt.ui.panes.is_none(), "a new profile starts unarranged");
        alt.ui.panes = Some(PaneLayoutPersist::default_layout());
        persist(&set, &alt);

        set.switch(&first).unwrap();
        assert_eq!(load(&set).ui.panes, Some(custom_layout()));
        set.switch("alt").unwrap();
        assert_eq!(
            load(&set).ui.panes,
            Some(PaneLayoutPersist::default_layout())
        );
    }

    fn theme(id: &str, background: &str) -> CustomTheme {
        CustomTheme {
            id: id.into(),
            label: id.into(),
            description: String::new(),
            xterm: [("background".to_string(), background.to_string())]
                .into_iter()
                .collect(),
            chrome: BTreeMap::new(),
        }
    }

    /// A profile with every theme and font scope field off its default.
    fn styled_profile() -> Profile {
        let mut profile = Profile::default();
        profile.ui.theme = "night-ink".into();
        profile.ui.follow_system_appearance = true;
        profile.ui.light_theme = "classic-vivid".into();
        profile.ui.dark_theme = "night-ink".into();
        profile.ui.custom_themes = vec![theme("night-ink", "#000000")];
        profile.ui.font_size = 16;
        profile.ui.terminal_line_height = "loose".into();
        profile
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
        assert!(per_profile.ui.custom_themes.is_empty());

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
        use crate::profile_set::Scope as Kind;
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

    fn theme_ids(themes: &[CustomTheme]) -> Vec<&str> {
        themes.iter().map(|t| t.id.as_str()).collect()
    }

    fn background(theme: &CustomTheme) -> &str {
        theme.xterm.get("background").map_or("", String::as_str)
    }

    /// Write a profile file that holds its own custom themes, the way a
    /// build before custom themes joined the theme scope saved it.
    fn write_profile_themes(set: &ProfileSet, name: &str, themes: Vec<CustomTheme>) {
        let mut config = ProfileConfig::default();
        config.ui.custom_themes = themes;
        config.save(&set.profile_path(name)).unwrap();
    }

    /// Mirror `persist_profile` for the active profile.
    fn persist_live(set: &ProfileSet, profile: &Profile) {
        let mut snapshot = ProfileConfig::from_profile(profile);
        strip_global_fields(&mut snapshot, set.scope());
        snapshot.save(&set.active_path()).unwrap();
        GlobalConfig::from_profile(profile, set.scope())
            .save(&set.global_path())
            .unwrap();
    }

    /// Mirror a launch or a switch. The active profile file loads first,
    /// then global.toml over it.
    fn load_live(set: &ProfileSet) -> Profile {
        let mut profile = Profile::default();
        let path = set.active_path();
        if path.exists() {
            ProfileConfig::load(&path).unwrap().apply_to(&mut profile);
        }
        if let Some(global) = GlobalConfig::load_shared(&set.global_path(), set.scope()).unwrap() {
            global.apply_to(&mut profile);
        }
        profile
    }

    #[test]
    fn the_shared_custom_themes_replace_the_profile_list() {
        // A list left in a profile file never comes back through a
        // switch. The shared list is the whole list.
        let global = GlobalConfig {
            custom_themes: Some(vec![theme("shared", "#101010")]),
            ..GlobalConfig::default()
        };
        let mut profile = Profile::default();
        profile.ui.custom_themes = vec![theme("shared", "#ffffff"), theme("deleted", "#202020")];
        global.apply_to(&mut profile);
        assert_eq!(theme_ids(&profile.ui.custom_themes), ["shared"]);
        assert_eq!(background(&profile.ui.custom_themes[0]), "#101010");
    }

    #[test]
    fn startup_moves_older_profile_themes_into_global_toml() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.create("alt").unwrap();
        GlobalConfig {
            theme: Some("nord".into()),
            custom_themes: Some(vec![theme("shared", "#101010")]),
            ..GlobalConfig::default()
        }
        .save(&set.global_path())
        .unwrap();
        write_profile_themes(
            &set,
            "default",
            vec![theme("shared", "#ffffff"), theme("mine", "#202020")],
        );
        write_profile_themes(
            &set,
            "alt",
            vec![theme("mine", "#303030"), theme("alts", "#404040")],
        );

        assert_eq!(migrate_custom_themes(&set).unwrap(), 2);

        let global = GlobalConfig::load(&set.global_path()).unwrap();
        assert_eq!(global.theme.as_deref(), Some("nord"));
        let shared = global.custom_themes.unwrap();
        // A different theme under a taken id joins under a fresh id, and
        // its label names its profile when the label is taken too.
        assert_eq!(
            theme_ids(&shared),
            ["shared", "shared-2", "mine", "mine-2", "alts"]
        );
        let backgrounds: Vec<&str> = shared.iter().map(background).collect();
        assert_eq!(
            backgrounds,
            ["#101010", "#ffffff", "#202020", "#303030", "#404040"]
        );
        assert_eq!(shared[1].label, "shared (Default)");
        assert_eq!(shared[3].label, "mine (alt)");
        for name in ["default", "alt"] {
            let file = ProfileConfig::load(&set.profile_path(name)).unwrap();
            assert!(file.ui.custom_themes.is_empty(), "{name} kept its list");
        }
        // The next launch finds nothing left to move.
        assert_eq!(migrate_custom_themes(&set).unwrap(), 0);
    }

    #[test]
    fn a_deleted_custom_theme_stays_deleted_after_a_switch() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.create("alt").unwrap();
        // Both files predate the shared list.
        write_profile_themes(&set, "default", vec![theme("keep", "#101010")]);
        write_profile_themes(&set, "alt", vec![theme("gone", "#202020")]);
        migrate_custom_themes(&set).unwrap();

        let mut live = load_live(&set);
        assert_eq!(theme_ids(&live.ui.custom_themes), ["keep", "gone"]);
        // You delete a theme on the default profile.
        live.ui.custom_themes.retain(|t| t.id != "gone");
        persist_live(&set, &live);

        set.switch("alt").unwrap();
        assert_eq!(theme_ids(&load_live(&set).ui.custom_themes), ["keep"]);
        // The next launch does not bring it back either.
        assert_eq!(migrate_custom_themes(&set).unwrap(), 0);
        assert_eq!(theme_ids(&load_live(&set).ui.custom_themes), ["keep"]);
    }

    #[test]
    fn profile_scoped_custom_themes_stay_in_their_files() {
        use crate::profile_set::Scope as Kind;
        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.set_scope(ScopeConfig {
            theme: Kind::Profile,
            ..ScopeConfig::default()
        })
        .unwrap();
        write_profile_themes(&set, "default", vec![theme("mine", "#101010")]);

        assert_eq!(migrate_custom_themes(&set).unwrap(), 0);
        let file = ProfileConfig::load(&set.profile_path("default")).unwrap();
        assert_eq!(theme_ids(&file.ui.custom_themes), ["mine"]);
        assert!(!set.global_path().exists());
    }

    #[test]
    fn turning_the_theme_scope_global_keeps_every_profile_theme() {
        use crate::profile_set::Scope as Kind;
        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.create("alt").unwrap();
        set.set_scope(ScopeConfig {
            theme: Kind::Profile,
            ..ScopeConfig::default()
        })
        .unwrap();
        // Each profile saved its own list while the theme was per profile.
        let mut live = Profile::default();
        live.ui.custom_themes = vec![theme("mine", "#101010")];
        persist_live(&set, &live);
        write_profile_themes(
            &set,
            "alt",
            vec![theme("mine", "#ffffff"), theme("alts", "#202020")],
        );

        set.set_scope(ScopeConfig::default()).unwrap();
        let share = |set: &ProfileSet, live: &mut Profile| {
            let held = HeldCustomThemes::find(set, Some(set.active_name()));
            share_custom_themes(held, set.scope(), &set.global_path(), live).unwrap()
        };
        assert!(share(&set, &mut live));
        assert_eq!(
            theme_ids(&live.ui.custom_themes),
            ["mine", "mine-2", "alts"]
        );
        assert_eq!(background(&live.ui.custom_themes[0]), "#101010");
        assert_eq!(background(&live.ui.custom_themes[1]), "#ffffff");
        let global = GlobalConfig::load(&set.global_path()).unwrap();
        assert_eq!(
            theme_ids(&global.custom_themes.unwrap()),
            ["mine", "mine-2", "alts"]
        );
        let alt = ProfileConfig::load(&set.profile_path("alt")).unwrap();
        assert!(alt.ui.custom_themes.is_empty());

        // The persist that follows the scope change clears the active
        // file, and the other profile sees every theme.
        persist_live(&set, &live);
        set.switch("alt").unwrap();
        assert_eq!(
            theme_ids(&load_live(&set).ui.custom_themes),
            ["mine", "mine-2", "alts"]
        );
        // Nothing is left for a second pass.
        assert!(!share(&set, &mut live));
    }

    fn labeled(id: &str, label: &str, background: &str) -> CustomTheme {
        CustomTheme {
            label: label.into(),
            ..theme(id, background)
        }
    }

    /// A profile file that picked `id` as its theme and its dark theme
    /// while it held `themes`, the way the old Themes tab saved it.
    fn write_profile_pick(set: &ProfileSet, name: &str, id: &str, themes: Vec<CustomTheme>) {
        let mut config = ProfileConfig::default();
        config.ui.theme = id.into();
        config.ui.dark_theme = id.into();
        config.ui.custom_themes = themes;
        config.save(&set.profile_path(name)).unwrap();
    }

    #[test]
    fn startup_keeps_different_themes_that_two_profiles_saved_as_custom() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.create("Healer").unwrap();
        set.create("Bard").unwrap();
        let ember = labeled("custom", "Ember variant", "#101010");
        let healer = labeled("custom", "Ember variant", "#202020");
        let bard = labeled("custom", "Night blue", "#303030");
        write_profile_pick(&set, "default", "custom", vec![ember.clone()]);
        write_profile_pick(&set, "Healer", "custom", vec![healer]);
        write_profile_pick(&set, "Bard", "custom", vec![bard]);

        assert_eq!(migrate_custom_themes(&set).unwrap(), 3);

        let shared = GlobalConfig::load(&set.global_path())
            .unwrap()
            .custom_themes
            .unwrap();
        assert_eq!(theme_ids(&shared), ["custom", "custom-2", "custom-3"]);
        assert_eq!(shared[0], ember);
        assert_eq!(background(&shared[1]), "#202020");
        assert_eq!(shared[1].label, "Ember variant (Healer)");
        // A label nobody else holds stays as it is.
        assert_eq!(background(&shared[2]), "#303030");
        assert_eq!(shared[2].label, "Night blue");

        // Each file now points at the id its own theme landed under.
        for (name, id) in [
            ("default", "custom"),
            ("Healer", "custom-2"),
            ("Bard", "custom-3"),
        ] {
            let file = ProfileConfig::load(&set.profile_path(name)).unwrap();
            assert!(file.ui.custom_themes.is_empty(), "{name} kept its list");
            assert_eq!(file.ui.theme, id, "{name}");
            assert_eq!(file.ui.dark_theme, id, "{name}");
        }
    }

    #[test]
    fn startup_merges_identical_themes_into_one() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.create("Healer").unwrap();
        let ember = labeled("custom", "Ember variant", "#101010");
        write_profile_pick(&set, "default", "custom", vec![ember.clone()]);
        write_profile_pick(&set, "Healer", "custom", vec![ember.clone()]);

        assert_eq!(migrate_custom_themes(&set).unwrap(), 2);

        let shared = GlobalConfig::load(&set.global_path())
            .unwrap()
            .custom_themes
            .unwrap();
        assert_eq!(shared, [ember]);
        let healer = ProfileConfig::load(&set.profile_path("Healer")).unwrap();
        assert_eq!(healer.ui.theme, "custom");
    }

    #[test]
    fn a_move_that_runs_again_adds_no_second_copy() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.create("Healer").unwrap();
        let healer = labeled("custom", "Ember variant", "#202020");
        write_profile_pick(
            &set,
            "default",
            "custom",
            vec![labeled("custom", "Ember variant", "#101010")],
        );
        write_profile_pick(&set, "Healer", "custom", vec![healer.clone()]);
        migrate_custom_themes(&set).unwrap();
        let first = GlobalConfig::load(&set.global_path())
            .unwrap()
            .custom_themes
            .unwrap();

        // The Healer file failed to save last time and still holds its list.
        write_profile_pick(&set, "Healer", "custom", vec![healer]);
        assert_eq!(migrate_custom_themes(&set).unwrap(), 1);

        let again = GlobalConfig::load(&set.global_path())
            .unwrap()
            .custom_themes
            .unwrap();
        assert_eq!(again, first);
        let file = ProfileConfig::load(&set.profile_path("Healer")).unwrap();
        assert_eq!(file.ui.theme, "custom-2");
    }

    #[test]
    fn a_list_the_shared_themes_lack_is_not_cleared() {
        let held = vec![theme("custom", "#202020")];
        let shared = vec![theme("custom", "#101010")];
        assert!(!holds_every_theme(&shared, &held, &["custom".into()]));
        assert!(!holds_every_theme(&shared, &held, &[]));
        let mut merged = shared.clone();
        let landed = merge_custom_themes(&mut merged, &held, "Healer");
        assert!(holds_every_theme(&merged, &held, &landed));
    }

    #[test]
    fn an_imported_theme_picked_globally_survives_a_profile_switch() {
        use crate::profile_set::ProfileSet;

        fn persist(set: &ProfileSet, profile: &Profile) {
            let mut snapshot = ProfileConfig::from_profile(profile);
            strip_global_fields(&mut snapshot, set.scope());
            snapshot.save(&set.active_path()).unwrap();
            GlobalConfig::from_profile(profile, set.scope())
                .save(&set.global_path())
                .unwrap();
        }
        fn load(set: &ProfileSet) -> Profile {
            let mut profile = Profile::default();
            let path = set.active_path();
            if path.exists() {
                ProfileConfig::load(&path).unwrap().apply_to(&mut profile);
            }
            GlobalConfig::load(&set.global_path())
                .unwrap()
                .apply_to(&mut profile);
            profile
        }

        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        persist(&set, &styled_profile());

        set.create("alt").unwrap();
        set.switch("alt").unwrap();
        let alt = load(&set);
        assert_eq!(alt.ui.theme, "night-ink");
        assert_eq!(alt.ui.custom_themes.len(), 1);
        assert_eq!(alt.ui.custom_themes[0].id, "night-ink");
        assert!(alt.ui.follow_system_appearance);
        assert_eq!(alt.ui.terminal_line_height, "loose");
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
        assert!(live.ui.tracked_affects.is_empty());
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
        use crate::profile_set::Scope as Kind;
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
        assert!(live.ui.custom_themes.is_empty());
        assert!(!live.ui.follow_system_appearance);
        // The font is still shared, so it stays.
        assert_eq!(live.ui.font_size, 16);
        assert_eq!(live.ui.font_family, "Iosevka");
    }

    #[test]
    fn a_load_lays_only_the_shared_categories_over_the_profile() {
        use crate::profile_set::Scope as Kind;
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

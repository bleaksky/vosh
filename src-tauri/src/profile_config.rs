//! Per-profile TOML serialization. Phase 9.
//!
//! [`ProfileConfig`] is a serde-friendly snapshot of the parts of a
//! [`crate::profile::Profile`] that survive across app launches. The runtime
//! Profile holds extra state (compiled regex, Lua engine, tick deadlines)
//! that does not belong in the on-disk file.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;
use vosh_automation::alias::Alias;
use vosh_automation::trigger::Trigger;
use vosh_automation::vars::Scope;

// Callers outside this file still reach these here, until they point at
// crate::disk::atomic.
pub(crate) use crate::disk::atomic::{
    follow_unread, hold_unread, is_unread, release_unread, write_with_backup,
};
// Callers outside this file still reach these here, until they point at
// crate::disk::custom_themes.
pub(crate) use crate::disk::custom_themes::migrate_custom_themes;
// Callers outside this file still reach these here, until they point at
// crate::profile::panes.
pub(crate) use crate::profile::panes::{DockEntryPersist, PaneLayoutPersist};
// Callers outside this file still reach these here, until they point at
// crate::profile::shared.
pub(crate) use crate::profile::shared::{put_back, strip_global_fields, GlobalConfig, SharedLayer};
use crate::profile::{Macro, Profile, Timer};
use crate::profile_set::ProfileSet;
use crate::tick::TickConfig;

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
    /// The catalog groups each of your folders became in the shared
    /// catalog, which `#group` follows. See [`GroupFolders`].
    #[serde(default, skip_serializing_if = "GroupFolders::is_empty")]
    pub group_folders: GroupFolders,
    /// The `[prompt]` table: the switch, the design, earlier designs and
    /// how Vosh reads the game's prompt. None in a file an older build
    /// wrote, and left out of a file while it says nothing a default one
    /// does not. Set it with [`ProfileConfig::set_prompt`], which keeps
    /// the `[ui]` copy of the switch and the design in step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<vosh_prompt::PromptConfig>,
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

/// Trim the enabled preset ids, drop blank ones, and sort them without
/// repeats.
pub(crate) fn normalize_enabled_presets(list: Vec<String>) -> Vec<String> {
    let mut list: Vec<String> = list
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    list.sort();
    list.dedup();
    list
}

/// Trim a color pick and turn a blank one into None, so a picker clears
/// back to its default by sending an empty string.
pub(crate) fn normalize_optional_color(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
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
    /// The dock layout the side panels had before panes. Nothing edits
    /// it now. `pane_layout` turns it into a pane tree for a profile
    /// that has never saved one, and saves keep writing it through 1.0
    /// (D13).
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
    /// Blinking text: text the game or your prompt sets to blink shows
    /// and hides. None until you choose, which the page reads as on
    /// unless the system asks to reduce motion. Your choice always wins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blink_text: Option<bool>,
    /// Keep highlight colors readable. While on, a fixed color a trigger
    /// paints text in, a true color or a 256 color past the 16, that fades
    /// on the theme's terminal background draws at a lightness that reads
    /// (see `highlight_ground`). On by default, and a file written before
    /// this switch reads it on. Written only while off, so a profile that
    /// never turns it off saves the bytes it saved before.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub readable_highlights: bool,
    /// Collapse repeated lines. While on, a line the game sends that shows
    /// exactly as the line before it on screen, colors included, joins it,
    /// and the screen shows the two once with a count before them. The
    /// log keeps every line and triggers see each one. Off by default, and
    /// a file written before this switch reads it off. Written only while
    /// on, so a profile that never turns it on saves the bytes it saved
    /// before.
    #[serde(default, skip_serializing_if = "is_false")]
    pub collapse_repeats: bool,
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
    /// When true (the default), the echo of each command you send starts
    /// with a grey `›` and a space, so your commands stand apart from the
    /// game's lines. A profile from before the setting reads it on.
    #[serde(default = "default_input_echo_caret")]
    pub input_echo_caret: bool,
    /// Whether the old side panel zones filled the window height. The
    /// one window panel has no such zones, so nothing reads it. Every
    /// save writes back the value it loaded, so 0.7.2 keeps it on a
    /// downgrade (D12, D14).
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
    /// A copy of `[prompt] draw`, which holds the switch now. Every save
    /// writes it, so an older build that reads only this key still draws
    /// your prompt. A file with no `[prompt]` reads the switch from
    /// here, see [`ProfileConfig::prompt_config`].
    #[serde(default)]
    pub prompt_template_enabled: bool,
    /// A copy of `[prompt] template`, written and read the same way as
    /// [`UiConfig::prompt_template_enabled`].
    #[serde(default)]
    pub prompt_template: String,
    /// The old vitals bar look. The vitals under the panes read
    /// `vitals_density` and the rows after it instead, so nothing reads
    /// this. Every save writes back the table it loaded, so 0.7.2 keeps
    /// it on a downgrade (D12, D14).
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
    /// Hide the vitals under the panel's panes while your prompt is
    /// pinned above the command line, and give their room to the panes.
    /// On by default, since a pinned prompt usually shows them. A file
    /// written before this switch reads it on.
    #[serde(default = "default_true")]
    pub vitals_hide_when_pinned: bool,
    /// Where the old status bar drew the moons. The status line places
    /// them itself, so nothing reads this. Every save writes back the
    /// value it loaded, so 0.7.2 keeps it on a downgrade (D12, D14).
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
    /// Which way the status line tick counts. `up` (the default) shows
    /// the seconds since the last tick, `down` the seconds left until
    /// the next and waits at 0 while the game runs late, and
    /// `down_past_zero` counts on below zero until the tick lands. Per
    /// profile, like `chip_style`. Unknown values coerce back to `up`.
    #[serde(default = "default_tick_count")]
    pub tick_count: String,
    /// Which layout the Affects pane draws: `timers` (the default,
    /// Timers first), `countdown`, `chips` (Grouped chips), or
    /// `chips_drain` (Draining chips). Per profile, like the rest of the
    /// panel. Unknown values coerce back to `timers` on save, so a build
    /// from before Draining chips saves it as Timers first.
    #[serde(default = "default_affects_style")]
    pub affects_style: String,
    /// The mark beside each tracked affect in the timers and countdown
    /// layouts: `dot` (the default), `square`, `plus_minus`, or `none`.
    /// Unknown values coerce back to `dot` on save.
    #[serde(default = "default_affects_marker")]
    pub affects_marker: String,
    /// Tint the missing and running out rows in the timers and
    /// countdown layouts. Off by default. The chips layout always does.
    #[serde(default)]
    pub affects_tint: bool,
    /// At or under this many hours an affect you track turns yellow in
    /// the Affects pane and counts as running out. Whole hours, 0 to 99,
    /// and never under `affects_almost_gone_hours`. Written only once it
    /// differs from the default, 2, so a profile that never changed it
    /// writes nothing new. A hand edit that is not a number reads as the
    /// default and never stops the profile loading.
    #[serde(
        default = "default_affects_running_out_hours",
        deserialize_with = "deserialize_affects_running_out_hours",
        skip_serializing_if = "is_default_affects_running_out_hours"
    )]
    pub affects_running_out_hours: u32,
    /// At or under this many hours the hours of an affect turn bold red,
    /// as the game prints them in its own affects bar at 1, the default.
    /// Whole hours, 0 to 99, and never over `affects_running_out_hours`.
    /// Written and read like it.
    #[serde(
        default = "default_affects_almost_gone_hours",
        deserialize_with = "deserialize_affects_almost_gone_hours",
        skip_serializing_if = "is_default_affects_almost_gone_hours"
    )]
    pub affects_almost_gone_hours: u32,
    /// The chat pane's channel colors, picked from its own menu. Each
    /// key is a channel name in lowercase and each value one of the
    /// theme's 16 ANSI slots, like `brightBlue`. A channel left out takes
    /// the color the game prints it in. Only the pane menu writes it,
    /// through its own commands, so a whole config save from Settings
    /// never carries an old copy back.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub chat_colors: BTreeMap<String, String>,
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

fn default_vitals_layout() -> String {
    "ember".to_string()
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

/// The ways the status line chips draw. Anything else saves as the
/// default, the value alone.
pub(crate) const CHIP_STYLES: [&str; 3] = ["value_only", "caption_value", "icon_value"];

fn default_chip_style() -> String {
    "value_only".to_string()
}

/// Keep a known chip style and turn anything else into `value_only`, so
/// the page never draws a chip with no style.
pub(crate) fn coerce_chip_style(value: String) -> String {
    if CHIP_STYLES.contains(&value.as_str()) {
        value
    } else {
        default_chip_style()
    }
}

/// The ways the status line tick counts. Anything else saves as the
/// default, counting up.
pub(crate) const TICK_COUNTS: [&str; 3] = ["up", "down", "down_past_zero"];

fn default_tick_count() -> String {
    "up".to_string()
}

/// Keep a known tick count and turn anything else into `up`.
pub(crate) fn coerce_tick_count(value: String) -> String {
    if TICK_COUNTS.contains(&value.as_str()) {
        value
    } else {
        default_tick_count()
    }
}

/// The layouts the Affects pane draws. Anything else saves as the
/// default, Timers first.
pub(crate) const AFFECTS_STYLES: [&str; 4] = ["timers", "countdown", "chips", "chips_drain"];

fn default_affects_style() -> String {
    "timers".to_string()
}

/// Keep a known affects layout and turn anything else into `timers`.
pub(crate) fn coerce_affects_style(value: String) -> String {
    if AFFECTS_STYLES.contains(&value.as_str()) {
        value
    } else {
        default_affects_style()
    }
}

/// The marks the Affects pane draws beside a tracked affect. Anything
/// else saves as the default, the dot.
pub(crate) const AFFECTS_MARKERS: [&str; 4] = ["dot", "square", "plus_minus", "none"];

fn default_affects_marker() -> String {
    "dot".to_string()
}

/// Keep a known affects marker and turn anything else into `dot`.
pub(crate) fn coerce_affects_marker(value: String) -> String {
    if AFFECTS_MARKERS.contains(&value.as_str()) {
        value
    } else {
        default_affects_marker()
    }
}

/// The hours at which an affect you track turns yellow and counts as
/// running out, unless you set another.
pub(crate) const DEFAULT_AFFECTS_RUNNING_OUT_HOURS: u32 = 2;

/// The hours at which an affect's hours turn bold red, unless you set
/// another. The game's own affects bar turns red at 1.
pub(crate) const DEFAULT_AFFECTS_ALMOST_GONE_HOURS: u32 = 1;

/// The most hours either affects threshold takes.
pub(crate) const AFFECTS_HOURS_MAX: u32 = 99;

fn default_affects_running_out_hours() -> u32 {
    DEFAULT_AFFECTS_RUNNING_OUT_HOURS
}

fn default_affects_almost_gone_hours() -> u32 {
    DEFAULT_AFFECTS_ALMOST_GONE_HOURS
}

fn is_default_affects_running_out_hours(hours: &u32) -> bool {
    *hours == DEFAULT_AFFECTS_RUNNING_OUT_HOURS
}

fn is_default_affects_almost_gone_hours(hours: &u32) -> bool {
    *hours == DEFAULT_AFFECTS_ALMOST_GONE_HOURS
}

/// Keep both affects thresholds in whole hours from 0 to 99, with
/// almost gone never over running out. Running out wins, since the
/// Settings rows never let you set almost gone above it and only a hand
/// edit can.
pub(crate) fn coerce_affects_thresholds(running_out: u32, almost_gone: u32) -> (u32, u32) {
    let running_out = running_out.min(AFFECTS_HOURS_MAX);
    (running_out, almost_gone.min(running_out))
}

/// Whole hours from a number: rounded and held to 0 to 99. None when it
/// is not a finite number.
fn affects_hours_of(n: f64) -> Option<u32> {
    // The clamp keeps the cast in range.
    n.is_finite()
        .then(|| n.round().clamp(0.0, f64::from(AFFECTS_HOURS_MAX)) as u32)
}

/// Read an affects threshold leniently, so a hand edit never stops a
/// profile loading. A whole number or a decimal rounds and clamps to 0
/// to 99, a string that holds a number reads the same way, and anything
/// else reads as `fallback`.
fn lenient_affects_hours<'de, D>(deser: D, fallback: u32) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Whole(i64),
        Decimal(f64),
        Text(String),
        Other(serde::de::IgnoredAny),
    }
    // A whole number past 2^53 loses precision as a float, and clamps to
    // 99 all the same.
    #[allow(clippy::cast_precision_loss)]
    let hours = match Raw::deserialize(deser)? {
        Raw::Whole(n) => affects_hours_of(n as f64),
        Raw::Decimal(n) => affects_hours_of(n),
        Raw::Text(text) => text.trim().parse::<f64>().ok().and_then(affects_hours_of),
        Raw::Other(_) => None,
    };
    Ok(hours.unwrap_or(fallback))
}

pub(crate) fn deserialize_affects_running_out_hours<'de, D>(deser: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    lenient_affects_hours(deser, DEFAULT_AFFECTS_RUNNING_OUT_HOURS)
}

pub(crate) fn deserialize_affects_almost_gone_hours<'de, D>(deser: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    lenient_affects_hours(deser, DEFAULT_AFFECTS_ALMOST_GONE_HOURS)
}

fn default_echo_macros() -> bool {
    true
}

fn default_input_echo_caret() -> bool {
    true
}

fn default_paste_line_delay_ms() -> u32 {
    500
}

/// Hold the paste delay to at most 10 seconds a line, so a malformed
/// value cannot freeze the paste indicator.
pub(crate) fn coerce_paste_line_delay_ms(ms: u32) -> u32 {
    ms.min(10_000)
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
            blink_text: None,
            readable_highlights: true,
            collapse_repeats: false,
            terminal_base_ansi: None,
            custom_themes: Vec::new(),
            split_divider_color: None,
            input_echo_color: None,
            echo_macros: true,
            input_echo_caret: true,
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
            vitals_hide_when_pinned: true,
            moons_position: default_moons_position(),
            chip_style: default_chip_style(),
            tick_count: default_tick_count(),
            affects_style: default_affects_style(),
            affects_marker: default_affects_marker(),
            affects_tint: false,
            affects_running_out_hours: DEFAULT_AFFECTS_RUNNING_OUT_HOURS,
            affects_almost_gone_hours: DEFAULT_AFFECTS_ALMOST_GONE_HOURS,
            chat_colors: BTreeMap::new(),
        }
    }
}

fn default_theme() -> String {
    "obsidian-ember".to_string()
}

fn default_light_theme() -> String {
    "vellum".to_string()
}

/// Trim a light theme pick and turn a blank one into `vellum`.
pub(crate) fn coerce_light_theme(value: String) -> String {
    match value.trim() {
        "" => default_light_theme(),
        id => id.to_string(),
    }
}

/// Trim a dark theme pick. A blank one stays blank, which the page reads
/// as the current theme.
pub(crate) fn normalize_dark_theme(value: String) -> String {
    value.trim().to_string()
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
    "\"JetBrainsMono Bundled\", Menlo, Consolas, ui-monospace, monospace".to_string()
}

/// The default font list while Vosh bundled Berkeley Mono. Profile files
/// saved then hold it where you never picked a font, and it keeps
/// rendering as Berkeley Mono where you have it installed.
pub(crate) const RETIRED_DEFAULT_FONT_FAMILY: &str =
    "BerkeleyMono Nerd Font, JetBrains Mono, Fira Code, Menlo, Consolas, ui-monospace, monospace";

/// Whether `family` is the default font list, this one or the one before
/// Vosh stopped bundling Berkeley Mono.
pub(crate) fn is_default_font_family(family: &str) -> bool {
    family == default_font_family() || family == RETIRED_DEFAULT_FONT_FAMILY
}

fn default_font_size() -> u32 {
    14
}

/// Hold the terminal font size to 6 to 64 pixels.
pub(crate) fn coerce_font_size(size: u32) -> u32 {
    size.clamp(6, 64)
}

/// The caret shapes the command line draws. Anything else saves as the
/// default, the block.
pub(crate) const INPUT_CURSOR_STYLES: [&str; 7] = [
    "block",
    "block_outline",
    "half_block",
    "underline",
    "underline_thick",
    "pipe",
    "pipe_thick",
];

fn default_input_cursor_style() -> String {
    "block".to_string()
}

/// Keep a known caret shape and turn anything else into `block`, so the
/// input row always paints a caret. An unknown one comes from a hand
/// edit or a newer build.
pub(crate) fn coerce_input_cursor_style(value: String) -> String {
    if INPUT_CURSOR_STYLES.contains(&value.as_str()) {
        value
    } else {
        default_input_cursor_style()
    }
}

pub(crate) fn default_true() -> bool {
    true
}

/// Leave a switch that is on by default out of the file while it is on.
fn is_true(on: &bool) -> bool {
    *on
}

/// Leave a switch that is off by default out of the file while it is off.
fn is_false(on: &bool) -> bool {
    !*on
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
            group_folders: profile.group_folders.clone(),
            prompt: None,
        };
        config.set_prompt(profile.prompt.config().clone());
        config
    }

    /// What a profile that never saved a file stands for: the defaults,
    /// with Vosh's default design ready for when you turn drawing on. A
    /// switch to such a profile loads it, and its first save keeps it.
    pub(crate) fn fresh() -> Self {
        let mut config = Self::default();
        config.set_prompt(vosh_prompt::PromptConfig::fresh());
        config
    }

    /// The `[prompt]` table this file stands for: its own, or for a file
    /// with none, the switch and the design older builds kept in `[ui]`.
    pub(crate) fn prompt_config(&self) -> vosh_prompt::PromptConfig {
        match &self.prompt {
            Some(prompt) => prompt.clone(),
            None => vosh_prompt::PromptConfig::from_legacy(
                self.ui.prompt_template_enabled,
                &self.ui.prompt_template,
            ),
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
    /// is a default an earlier build shipped becomes today's default, see
    /// [`vosh_prompt::PromptConfig::upgrade_retired_default`].
    fn merge_legacy_prompt(&mut self) {
        let mut prompt = self.prompt_config();
        prompt.upgrade_retired_default();
        self.set_prompt(prompt);
    }

    /// Apply a snapshot onto a live profile, replacing the relevant pieces.
    /// Triggers with invalid regex are reported and skipped.
    pub(crate) fn apply_to(&self, profile: &mut Profile) -> Vec<String> {
        let mut warnings = Vec::new();

        // Aliases: replace the store entirely.
        let mut aliases = vosh_automation::alias::AliasStore::new();
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
        let mut vars = vosh_automation::vars::VariableStore::new();
        for (k, v) in &self.profile_vars {
            vars.set(Scope::Profile, k.clone(), v.clone());
        }
        for (k, v) in session_only {
            vars.set(Scope::Session, k, v);
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

        // Tick: take the persisted settings and keep the running count.
        // A switch mid session would otherwise drop the last tick and
        // freeze the status line until the game's next tick. A running
        // tick stays on (see `TickRuntime::adopt`).
        let reset_regex = crate::tick::compile_reset_pattern(self.tick.reset_pattern.as_deref())
            .unwrap_or_else(|e| {
                warnings.push(format!("tick reset pattern rejected: {e}"));
                None
            });
        profile.tick.adopt(
            TickConfig {
                interval_secs: self.tick.interval_secs.max(1),
                ..self.tick.clone()
            },
            reset_regex,
            tokio::time::Instant::now(),
        );

        // UI preferences carry across as a single clone (see the
        // matching note in `from_profile`).
        profile.ui = self.ui.clone();
        // The custom prompt takes the file's [prompt] table, or the
        // switch and the design an older file kept in [ui].
        profile.set_prompt_config(self.prompt_config());

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
        profile.group_folders.clone_from(&self.group_folders);

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
/// With no file yet, the profile is fresh and takes Vosh's default
/// design.
pub(crate) fn load_at_launch(set: &ProfileSet, profile: &mut Profile) -> Vec<String> {
    let mut notices = Vec::new();
    profile.display_name = Some(crate::profile_set::display_name(set.active_name()));
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
        profile.set_prompt_config(vosh_prompt::PromptConfig::fresh());
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
        let leftover = &warnings;
        assert!(leftover.is_empty(), "{leftover:?}");
        let snapshot = ProfileConfig::from_profile(&profile);
        assert_eq!(snapshot.aliases.len(), 1);
        assert_eq!(snapshot.aliases[0].name, "greet");
    }

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    /// A live profile in a session whose 30 second tick synced to the
    /// game's tick 3 seconds after it connected, with the world hour
    /// primed and the warning already printed this cycle.
    fn synced_profile(start: tokio::time::Instant) -> Profile {
        let mut profile = Profile::default();
        profile.tick.start_session(start);
        let _ = profile.tick.observe_world_hour("9");
        assert!(profile.tick.on_game_tick(start + secs(3)).is_some());
        profile.tick.warned_this_cycle = true;
        profile
    }

    #[test]
    fn a_profile_switch_mid_count_keeps_the_running_tick() {
        let t0 = tokio::time::Instant::now();
        let mut profile = synced_profile(t0);
        let next_fire = profile.tick.next_fire();

        let mut incoming = ProfileConfig::default();
        incoming.tick.auto_fire = Some("score".into());
        incoming.tick.sound = false;
        incoming.tick.reset_pattern = Some("^You feel".into());
        incoming.tick.warn_at_secs = Some(8);
        incoming.tick.warn_message = Some("Tick soon".into());
        let leftover = &incoming.apply_to(&mut profile);
        assert!(leftover.is_empty(), "{leftover:?}");

        let tick = &profile.tick;
        assert_eq!(tick.last_tick, Some(t0 + secs(3)));
        assert_eq!(tick.next_fire(), next_fire);
        assert!(tick.synced);
        assert!(tick.in_session);
        assert!(tick.warned_this_cycle);
        assert_eq!(tick.last_world_hour.as_deref(), Some("9"));
        // The new profile's settings.
        assert_eq!(tick.config.auto_fire.as_deref(), Some("score"));
        assert!(!tick.config.sound);
        assert_eq!(tick.config.warn_at_secs, Some(8));
        assert_eq!(tick.config.warn_message.as_deref(), Some("Tick soon"));
        assert!(tick.check_reset_match("You feel less tired."));
        // The same tick again is still the same tick, and the next one
        // fires with the new command.
        assert!(profile.tick.on_game_tick(t0 + secs(4)).is_none());
        let step = profile
            .tick
            .on_game_tick(t0 + secs(31))
            .expect("the next tick");
        assert_eq!(step.command.as_deref(), Some("score"));
    }

    #[test]
    fn a_profile_switch_to_another_interval_keeps_the_count() {
        let t0 = tokio::time::Instant::now();
        let mut profile = synced_profile(t0);
        let mut incoming = ProfileConfig::default();
        incoming.tick.interval_secs = 40;
        let _ = incoming.apply_to(&mut profile);
        assert_eq!(profile.tick.last_tick, Some(t0 + secs(3)));
        assert_eq!(profile.tick.next_fire(), Some(t0 + secs(43)));
        assert!(profile.tick.synced);
    }

    #[test]
    fn a_switch_to_a_profile_saved_with_the_tick_off_keeps_the_running_tick() {
        // Earlier builds saved the tick off whenever the game had
        // disconnected, so many files say off that you never turned off.
        let t0 = tokio::time::Instant::now();
        let mut profile = synced_profile(t0);
        let next_fire = profile.tick.next_fire();
        let mut incoming = ProfileConfig::default();
        incoming.tick.enabled = false;
        incoming.tick.auto_fire = Some("score".into());
        let _ = incoming.apply_to(&mut profile);
        assert!(profile.tick.config.enabled);
        assert_eq!(profile.tick.next_fire(), next_fire);
        assert!(profile.tick.synced);
        assert_eq!(profile.tick.config.auto_fire.as_deref(), Some("score"));
        let step = profile
            .tick
            .on_game_tick(t0 + secs(33))
            .expect("the next tick");
        assert!(step.payload.fired);
        // Saved again, the profile now keeps the tick on.
        assert!(ProfileConfig::from_profile(&profile).tick.enabled);
    }

    #[test]
    fn a_profile_saved_with_the_tick_off_loads_it_off_between_sessions() {
        let mut profile = Profile::default();
        let mut incoming = ProfileConfig::default();
        incoming.tick.enabled = false;
        let _ = incoming.apply_to(&mut profile);
        assert!(!profile.tick.config.enabled);
        assert_eq!(profile.tick.next_fire(), None);
    }

    #[test]
    fn a_tick_you_turned_off_stays_off_across_a_switch_to_one_saved_off() {
        let t0 = tokio::time::Instant::now();
        let mut profile = synced_profile(t0);
        profile.tick.disable();
        let mut off = ProfileConfig::default();
        off.tick.enabled = false;
        let _ = off.apply_to(&mut profile);
        assert!(!profile.tick.config.enabled);
        assert_eq!(profile.tick.next_fire(), None);
        assert!(profile.tick.on_game_tick(t0 + secs(20)).is_none());
    }

    #[test]
    fn a_profile_switch_that_turns_the_tick_on_arms_it() {
        let t0 = tokio::time::Instant::now();
        let mut profile = synced_profile(t0);
        // You turned the tick off with #tick disable or in Settings.
        profile.tick.disable();
        assert_eq!(profile.tick.next_fire(), None);

        let before = tokio::time::Instant::now();
        let _ = ProfileConfig::default().apply_to(&mut profile);
        let armed = profile.tick.last_tick.expect("the tick runs again");
        assert!(armed >= before);
        assert_eq!(profile.tick.next_fire(), Some(armed + secs(30)));
        assert!(!profile.tick.synced);
        // It counts on its own until the game's next tick syncs it.
        assert!(profile.tick.try_consume_fire(armed + secs(30)));
    }

    #[test]
    fn a_profile_load_between_sessions_leaves_the_tick_stopped() {
        let t0 = tokio::time::Instant::now();
        let mut profile = synced_profile(t0);
        profile.tick.end_session();
        let _ = ProfileConfig::default().apply_to(&mut profile);
        assert!(profile.tick.config.enabled);
        assert_eq!(profile.tick.next_fire(), None);
        // The next connection starts it.
        profile.tick.start_session(t0 + secs(100));
        assert_eq!(profile.tick.next_fire(), Some(t0 + secs(130)));
    }

    #[test]
    fn a_profile_saved_after_the_game_disconnects_keeps_the_tick_on() {
        let t0 = tokio::time::Instant::now();
        let mut profile = synced_profile(t0);
        profile.tick.end_session();
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
        let mut profile = synced_profile(t0);
        let next_fire = profile.tick.next_fire();
        let ran = crate::input::run_line(&mut profile, "#profile reset");
        assert!(ran.replaced);
        assert_eq!(profile.tick.next_fire(), next_fire);
        assert!(profile.tick.synced);
    }

    #[test]
    fn a_bad_reset_pattern_on_switch_warns_and_keeps_the_count() {
        let t0 = tokio::time::Instant::now();
        let mut profile = synced_profile(t0);
        let mut incoming = ProfileConfig::default();
        incoming.tick.reset_pattern = Some("[bad".into());
        let warnings = incoming.apply_to(&mut profile);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].starts_with("tick reset pattern rejected"));
        assert_eq!(profile.tick.config.reset_pattern.as_deref(), Some("[bad"));
        assert!(!profile.tick.check_reset_match("[bad"));
        assert_eq!(profile.tick.last_tick, Some(t0 + secs(3)));
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

    #[test]
    fn a_profile_from_before_mark_your_commands_reads_it_on() {
        let parsed = ProfileConfig::from_toml("[ui]\ntheme = \"nord\"\n").unwrap();
        assert!(parsed.ui.input_echo_caret);
        let off = ProfileConfig::from_toml("[ui]\ninput_echo_caret = false\n").unwrap();
        assert!(!off.ui.input_echo_caret);
        assert!(off.to_toml().unwrap().contains("input_echo_caret = false"));
    }
}

/// The `[prompt]` table in profile files: the legacy merge, the `[ui]`
/// copy, and the copy kept before the first table.
#[cfg(test)]
mod prompt_tests {
    use super::*;
    use crate::disk::atomic::BACKUP_RETENTION;
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
        live.set_prompt_config(migrated());
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
        live.set_prompt_config(migrated());
        let text = ProfileConfig::from_profile(&live).to_toml().unwrap();
        let back = ProfileConfig::from_toml(&text).unwrap();
        assert_eq!(back.prompt_config(), migrated());

        let mut next = Profile::default();
        let _ = back.apply_to(&mut next);
        assert_eq!(*next.prompt.config(), migrated());
        assert!(next.ui.prompt_template_enabled);
        assert_eq!(next.ui.prompt_template, TEMPLATE);
    }

    #[test]
    fn an_older_build_that_drops_the_table_still_draws_the_design() {
        let mut live = Profile::default();
        live.set_prompt_config(migrated());
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
    fn a_fresh_profile_file_keeps_the_default_design_with_drawing_off() {
        let fresh = ProfileConfig::fresh();
        assert_eq!(fresh.prompt_config(), PromptConfig::fresh());
        let text = fresh.to_toml().unwrap();
        let table: toml::Table = text.parse().unwrap();
        assert_eq!(table["prompt"]["draw"].as_bool(), Some(false));
        assert_eq!(
            table["prompt"]["template"].as_str(),
            Some(vosh_prompt::DEFAULT_DESIGN)
        );
        // The [ui] copy older builds read follows it.
        assert_eq!(
            table["ui"]["prompt_template"].as_str(),
            Some(vosh_prompt::DEFAULT_DESIGN)
        );

        let mut live = Profile::default();
        let _ = ProfileConfig::from_toml(&text).unwrap().apply_to(&mut live);
        assert_eq!(*live.prompt.config(), PromptConfig::fresh());
        assert!(!live.prompt.draws(), "it waits for you to turn it on");
        // Everything else is the defaults.
        let mut rest = ProfileConfig::fresh();
        rest.set_prompt(PromptConfig::default());
        assert_eq!(
            rest.to_toml().unwrap(),
            ProfileConfig::default().to_toml().unwrap()
        );
    }

    #[test]
    fn a_launch_with_no_profile_file_starts_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        assert!(!set.active_path().exists());
        let mut live = Profile::default();
        let leftover = &load_at_launch(&set, &mut live);
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(*live.prompt.config(), PromptConfig::fresh());
        assert_eq!(
            ProfileConfig::from_profile(&live).ui.prompt_template,
            vosh_prompt::DEFAULT_DESIGN
        );

        // A file of its own keeps what it says, a design or none.
        for design in ["%hp", ""] {
            let mut file = ProfileConfig::default();
            file.set_prompt(PromptConfig::from_legacy(false, design));
            file.save(&set.active_path()).unwrap();
            let mut live = Profile::default();
            let _ = load_at_launch(&set, &mut live);
            assert_eq!(live.prompt.config().template, design);
        }
    }

    #[test]
    fn a_file_with_an_old_default_design_loads_todays() {
        let dir = tempfile::tempdir().unwrap();
        let set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        for old in vosh_prompt::config::RETIRED_DEFAULTS {
            // As a fresh profile saved it, drawing off.
            let mut file = ProfileConfig::from_toml(&older_file()).unwrap();
            file.set_prompt(PromptConfig {
                show: vosh_prompt::PromptShow::Pinned,
                previous_templates: vec![TEMPLATE.into()],
                ..PromptConfig::from_legacy(false, old)
            });
            file.save(&set.active_path()).unwrap();
            let mut live = Profile::default();
            let leftover = &load_at_launch(&set, &mut live);
            assert!(leftover.is_empty(), "{leftover:?}");
            let prompt = live.prompt.config();
            assert_eq!(prompt.template, vosh_prompt::DEFAULT_DESIGN);
            assert_eq!(live.ui.prompt_template, vosh_prompt::DEFAULT_DESIGN);
            // Everything else in the table stays.
            assert!(!prompt.draw);
            assert_eq!(prompt.show, vosh_prompt::PromptShow::Pinned);
            assert_eq!(prompt.previous_templates, [TEMPLATE]);

            // A file older builds wrote, with the design only in [ui].
            let older = format!(
                "[ui]\nprompt_template_enabled = true\nprompt_template = {}\n",
                toml::Value::String(old.to_string())
            );
            let config = ProfileConfig::from_toml(&older).unwrap();
            assert_eq!(config.prompt_config().template, vosh_prompt::DEFAULT_DESIGN);
            assert_eq!(config.ui.prompt_template, vosh_prompt::DEFAULT_DESIGN);
            assert!(config.prompt_config().draw);
        }

        // A design of your own loads as you saved it.
        let mut file = ProfileConfig::default();
        file.set_prompt(PromptConfig::from_legacy(true, TEMPLATE));
        file.save(&set.active_path()).unwrap();
        let mut live = Profile::default();
        let _ = load_at_launch(&set, &mut live);
        assert_eq!(live.prompt.config().template, TEMPLATE);
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
        let _ = ProfileConfig::from_toml(&text).unwrap().apply_to(&mut live);
        assert_eq!(*live.prompt.config(), aabahran);
        assert!(
            live.prompt.forsaken(),
            "an Aabahran capture holds the rules on any host"
        );

        // A reset hands back the default table.
        let _ = ProfileConfig::default().apply_to(&mut live);
        assert!(live.prompt.config().is_default());
        assert!(!live.prompt.forsaken());
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

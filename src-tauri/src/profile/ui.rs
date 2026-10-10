//! The `[ui]` table of a profile file, which holds what you choose in
//! Settings, from the theme and the font to the vitals and the Affects
//! pane. This file keeps the default each key takes in a file that
//! leaves it out, and every rule, as a coerce or normalize function,
//! that turns a value Settings saves or a hand edit writes into one Vosh
//! knows.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Deserializer, Serialize};

use crate::profile::panes::{DockEntryPersist, PaneLayoutPersist};
use crate::profile::text_size::TextPx;

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
    /// What switches the theme by itself, the Switch themes row: `off`,
    /// `system` for `light_theme` and `dark_theme` by the OS appearance,
    /// or `game` for `day_theme` and `night_theme` by the game's dawn and
    /// dusk. Written only once it is not `off`, and
    /// `follow_system_appearance` stays true only for `system`, so 0.8.1
    /// reads `game` as off. A file without the key reads it from
    /// `follow_system_appearance` (see [`read_theme_follow`]). Part of the
    /// `theme` scope category. Unknown values coerce back to `off`.
    #[serde(
        default = "default_theme_follow",
        skip_serializing_if = "is_default_theme_follow"
    )]
    pub theme_follow: String,
    /// The theme shown by day while the theme follows the game. Empty
    /// until you pick one, and left out of the file while empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub day_theme: String,
    /// The theme shown by night while the theme follows the game. Empty
    /// until you pick one, and left out of the file while empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub night_theme: String,
    /// Opt in to background update checks. Off by default.
    #[serde(default)]
    pub auto_update: bool,
    /// CSS font-family stack used by the terminal, status bar, and input.
    /// Falls back to `default_font_family` when not set.
    #[serde(default = "default_font_family")]
    pub font_family: String,
    /// Terminal font size in pixels, on half steps (see [`TextPx`]).
    #[serde(default = "default_font_size")]
    pub font_size: TextPx,
    /// Terminal row spacing: `compact`, `default`, or `loose` (1.1,
    /// 1.2, and 1.35 times the glyph height). Part of the `font` scope
    /// category. Unknown values coerce back to `default` on save.
    #[serde(default = "default_terminal_line_height")]
    pub terminal_line_height: String,
    /// The font every pane in the right panel and the status line under
    /// the terminal draw their text in: empty for As designed, the
    /// default, where each text keeps the face the panes were designed
    /// in, `terminal` for the terminal font, `system` for the system
    /// font, or a CSS font list the way `font_family` holds one. Part of
    /// the `font` scope category. Written only once you pick a font, so
    /// a profile that never does saves the bytes it saved before. A build
    /// without it reads past the key.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub panel_font: String,
    /// The size in pixels every pane and the status line under the
    /// terminal draw at, on half steps, 12 by default, the size they were drawn at
    /// before you could pick one. 0 follows the terminal size
    /// (`PANEL_FONT_SIZE_TERMINAL`). Part of the `font` scope category.
    /// Written only once you pick another size, so a profile that never
    /// does saves the bytes it saved before. A build without it reads
    /// past the key.
    #[serde(
        default = "default_panel_font_size",
        skip_serializing_if = "is_default_panel_font_size"
    )]
    pub panel_font_size: TextPx,
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
    /// that has never saved one, and saves keep writing it through 1.0,
    /// so a rollback still finds it.
    #[serde(default)]
    pub dock_layout: Vec<DockEntryPersist>,
    /// The side panel's pane tree. Always per profile: it is
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
    /// Fit game colors. While on, play draws the game's colors in the
    /// slots the theme fits for them, so the colors that fade on its
    /// ground read, and Settings keeps the theme as published. On by
    /// default, and a file written before this switch reads it on.
    /// Written only while off, so a profile that never turns it off
    /// saves the bytes it saved before.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub fit_game_colors: bool,
    /// The color vision Fit game colors fits the game colors for:
    /// `typical`, `deuteranopia`, `protanopia` or `tritanopia`. Part of
    /// the `theme` scope category, since your vision is the same on
    /// every character. Typical by default, and a file written before
    /// this choice reads it Typical. Written only once you pick another,
    /// so a profile that never does saves the bytes it saved before. A
    /// build without it reads past the key.
    #[serde(
        default = "default_color_vision",
        skip_serializing_if = "is_default_color_vision"
    )]
    pub color_vision: String,
    /// Keep highlight colors readable. While on, a fixed color a trigger
    /// paints text in, a true color or a 256 color past the 16, that fades
    /// on the theme's terminal background draws at a lightness that reads
    /// (see `highlight_ground`). On by default, and a file written before
    /// this switch reads it on. Written only while off, so a profile that
    /// never turns it off saves the bytes it saved before.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub readable_highlights: bool,
    /// Read new game lines. While on, each line the screen shows, after
    /// gags, also goes to the page for a screen reader to announce. Off
    /// by default, and written only while on, so a profile that never
    /// turns it on saves the bytes it saved before.
    #[serde(default, skip_serializing_if = "is_false")]
    pub screen_reader: bool,
    /// Read in the background, under Read new game lines. While on, the
    /// lines are read while Vosh is not the window in front. Off by
    /// default, and written only while on.
    #[serde(default, skip_serializing_if = "is_false")]
    pub screen_reader_background: bool,
    /// Read your prompt, under Read new game lines. While on, the prompt
    /// is read as it changes too. Off by default, and written only while
    /// on.
    #[serde(default, skip_serializing_if = "is_false")]
    pub screen_reader_prompt: bool,
    /// Past this many lines in one pulse, the screen reader hears the
    /// count and the last line instead of each line. 4, 8, 16 or 32,
    /// and 8, the default, is not written. A hand edit of anything else
    /// reads as 8 and never stops the profile loading.
    #[serde(
        default = "default_screen_reader_burst",
        deserialize_with = "deserialize_screen_reader_burst",
        skip_serializing_if = "is_default_screen_reader_burst"
    )]
    pub screen_reader_burst: u32,
    /// Collapse repeated lines. While on, a line the game sends that shows
    /// exactly as the line before it on screen, colors included, joins it,
    /// and the screen shows the two once with a count before them. The
    /// log keeps every line and triggers see each one. Off by default, and
    /// a file written before this switch reads it off. Written only while
    /// on, so a profile that never turns it on saves the bytes it saved
    /// before.
    #[serde(default, skip_serializing_if = "is_false")]
    pub collapse_repeats: bool,
    /// In a fight, under Collapse repeated lines: the lines of a fight
    /// collapse, from the round Char.Combat names a target in to the round
    /// that ends the fight. On by default, and a file written before this
    /// choice reads it on. Off, every line of a fight shows, and attack
    /// lines show every line anywhere. Written only while off.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub collapse_fight_lines: bool,
    /// Attack lines, under Collapse repeated lines: the hits and misses
    /// the game prints collapse, in a fight or not. Off by default, so a
    /// count never hides how many hits landed, and a file written before
    /// this choice reads it off. In a fight off leaves them whole
    /// whatever this says. Written only while on.
    #[serde(default, skip_serializing_if = "is_false")]
    pub collapse_attack_lines: bool,
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
    /// CSS color string applied to the handle between the
    /// split-scrollback history pane and the live pane. Empty or
    /// missing means use the theme default (`--split-divider`, the
    /// theme's tertiary tone). Any valid CSS color is accepted; e.g.
    /// `#ff00ff`, `rgb(255, 0, 0)`.
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
    /// The switch the mark grew from, kept in step with
    /// `input_echo_mark` on every save (true unless the mark is `off`), so
    /// an older build still marks your commands or leaves them bare. A
    /// file without `input_echo_mark` reads the mark from it (see
    /// [`read_input_echo_mark`]).
    #[serde(default = "default_true")]
    pub input_echo_caret: bool,
    /// The mark the echo of each command you send starts with, so your
    /// commands stand apart from the game's lines. `off`, `chevron` for
    /// `›`, `gt` for `>`, or `own` for `input_echo_mark_text`. Written
    /// only once it is not `chevron`. Unknown values coerce to `chevron`.
    #[serde(
        default = "default_input_echo_mark",
        skip_serializing_if = "is_default_input_echo_mark"
    )]
    pub input_echo_mark: String,
    /// Your own mark, at most four characters. It stays while another
    /// mark is picked, so picking `own` again brings it back.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub input_echo_mark_text: String,
    /// CSS hex color of the mark. None means the theme's bright black,
    /// SGR 90.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_echo_mark_color: Option<String>,
    /// Draw the echo of each command you send faint. The mark keeps its
    /// own color. Off by default.
    #[serde(default, skip_serializing_if = "is_false")]
    pub input_echo_dim: bool,
    /// Start the line you type in with the same mark. On by default.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub input_line_mark: bool,
    /// Whether the old side panel zones filled the window height. The
    /// pane panel has no such zones, so nothing reads it. Every
    /// save writes back the value it loaded, so 0.7.2 keeps it on a
    /// downgrade.
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
    /// Offer the writing card in a notice when you open the game's line
    /// editor yourself on a text Vosh can name, `description edit` or
    /// `note edit` Default on.
    #[serde(default = "default_writing_offer")]
    pub writing_offer: bool,
    /// The writing card asks before it posts a note. Off, Post posts at
    /// once, and the card still asks when a report would record a room
    /// other than the one you began it in. Default on.
    #[serde(default = "default_writing_ask_post")]
    pub writing_ask_post: bool,
    /// Shape of the command-line caret: `block` (default),
    /// `block_outline`, `half_block`, `underline`, `underline_thick`,
    /// `pipe`, or `pipe_thick`. Every shape is painted inside the same
    /// anchor box, so switching never moves the input row. Unknown
    /// values coerce back to `block` on save.
    #[serde(default = "default_input_cursor_style")]
    pub input_cursor_style: String,
    /// The command-line caret blinks. On by default, and Reduce motion
    /// still holds it steady.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub input_caret_blink: bool,
    /// CSS hex color of the caret. None means the theme accent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_caret_color: Option<String>,
    /// CSS hex color of what you type. None means the theme text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_line_color: Option<String>,
    /// The command line's background: `theme` (the default), `tint` for
    /// a slight tint of the accent, or `own` for
    /// `input_line_background_color`. Written only once it is not
    /// `theme`. Unknown values coerce to `theme`.
    #[serde(
        default = "default_input_line_background",
        skip_serializing_if = "is_default_input_line_background"
    )]
    pub input_line_background: String,
    /// Your own background color. It stays while another background is
    /// picked, so picking `own` again brings it back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_line_background_color: Option<String>,
    /// Size in px of the text you type, on half steps. 0, the default,
    /// follows the terminal size. Anything else is held to 6 to 64.
    #[serde(
        default = "default_input_line_size",
        skip_serializing_if = "is_default_input_line_size"
    )]
    pub input_line_size: TextPx,
    /// Color the command line as you type, by what Vosh knows the first
    /// word to be. Off by default.
    #[serde(default, skip_serializing_if = "is_false")]
    pub input_type_colors: bool,
    /// CSS hex color of a line that starts with one of your aliases. None
    /// means the theme's cyan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_type_alias_color: Option<String>,
    /// CSS hex color of a line that starts with a Vosh `#` command. None
    /// means the theme's magenta.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_type_hash_color: Option<String>,
    /// CSS hex color of a chat line, the whole line. None means the
    /// theme's yellow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_type_chat_color: Option<String>,
    /// CSS hex color of a `#` command Vosh does not know. None means the
    /// theme's danger color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_type_unknown_color: Option<String>,
    /// A copy of `[prompt] draw`, which holds the switch now. Every save
    /// writes it, so an older build that reads only this key still draws
    /// your prompt. A file with no `[prompt]` reads the switch from
    /// here, see [`crate::profile::file::ProfileConfig::prompt_config`].
    #[serde(default)]
    pub prompt_template_enabled: bool,
    /// A copy of `[prompt] template`, written and read the same way as
    /// [`UiConfig::prompt_template_enabled`].
    #[serde(default)]
    pub prompt_template: String,
    /// The old vitals bar look. The vitals under the panes read
    /// `vitals_density` and the rows after it instead, so nothing reads
    /// this. Every save writes back the table it loaded, so 0.7.2 keeps
    /// it on a downgrade.
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
    /// The vitals style you picked from the gallery: `ledger`, `gauges`,
    /// `pips`, `bands`, `ladders`, `blocks`, `traces`, `dials`, `rings`,
    /// `vials`, `orbs`, `candles` or `text`. None for Rows and One line, which stay in
    /// `vitals_density`, so a build without styles reads your look. The
    /// keys from here to `vitals_text_previous` are written only once
    /// they differ from the default, so a profile that never picks saves
    /// the bytes it saved before, and a build without them reads past
    /// them. Unknown values coerce back to None on save.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vitals_style: Option<String>,
    /// Where your vitals show: `panel`, the default, under the panel's
    /// panes, or `status`, in the status line under the terminal.
    #[serde(
        default = "default_vitals_place",
        skip_serializing_if = "is_default_vitals_place"
    )]
    pub vitals_place: String,
    /// The order every style draws your vitals in, each of `hp`, `mana`
    /// and `move` once. Today's order by default.
    #[serde(
        default = "default_vitals_order",
        skip_serializing_if = "is_default_vitals_order"
    )]
    pub vitals_order: Vec<String>,
    /// The vitals you turned off, of `hp`, `mana` and `move`, and
    /// `opponent` while your opponent's row is off. None by default.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vitals_off: Vec<String>,
    /// Where your opponent's row sits in a fight: `top`, the default, or
    /// `bottom`.
    #[serde(
        default = "default_vitals_opponent",
        skip_serializing_if = "is_default_vitals_opponent"
    )]
    pub vitals_opponent: String,
    /// The color each vital takes, as one of the theme's 16 ANSI slots,
    /// 0 to 15. A vital left out takes Default. A hand edit that holds
    /// no slot drops that vital and never stops the profile loading.
    #[serde(
        default,
        deserialize_with = "deserialize_vitals_colors",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub vitals_colors: BTreeMap<String, u8>,
    /// The text the Text style writes your vitals with, in your prompt's
    /// codes. Empty by default.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub vitals_text: String,
    /// At most two texts you had before, newest first, as `[prompt]`
    /// keeps `previous_templates`, so a reset never loses one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vitals_text_previous: Vec<String>,
    /// Show each hit: the part a hit took stays pale for a moment on
    /// every style with a fill, then drains. Off by default and written
    /// only once you turn it on.
    #[serde(default, skip_serializing_if = "is_false")]
    pub vitals_hit: bool,
    /// Where the old status bar drew the moons. The status line places
    /// them itself, so nothing reads this. Every save writes back the
    /// value it loaded, so 0.7.2 keeps it on a downgrade.
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
    /// How the status bar draws. `meters` (the default) fills the bar
    /// with a zone for each vital and the tick, `compact` is the quiet
    /// line that `chip_style` labels, `strip` adds gauges on a raised
    /// ground, and `dashboard` puts a caption over each value on a
    /// taller bar. Written only once it differs from the default, and
    /// unknown values coerce back to `meters` on save.
    #[serde(
        default = "default_status_style",
        skip_serializing_if = "is_default_status_style"
    )]
    pub status_style: String,
    /// Which way the status line tick counts. `up` (the default) shows
    /// the seconds since the last tick, `down` the seconds left until
    /// the next and waits at 0 while the game runs late, and
    /// `down_past_zero` counts on below zero until the tick lands. Per
    /// profile, like `chip_style`. Unknown values coerce back to `up`.
    #[serde(default = "default_tick_count")]
    pub tick_count: String,
    /// How the status line shows the game time. `24h` (the default)
    /// reads like 18:00, and `12h` like 6:00 PM. Per profile, like
    /// `tick_count`. Written only once it differs from the default, so
    /// a profile that never changed it writes nothing new and an older
    /// build reads the file as it always has. Unknown values coerce back
    /// to `24h` on save.
    #[serde(
        default = "default_game_time",
        skip_serializing_if = "is_default_game_time"
    )]
    pub game_time: String,
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
    /// The share of the terminal column the snoop split takes, from 0.05
    /// to 0.95. You set it by dragging the line between the split and the
    /// terminal, and 0.4, the default, is not written, so a profile that
    /// never snoops saves the bytes it saved before. A hand edit that is
    /// not a number reads as the default.
    #[serde(
        default = "default_snoop_share",
        deserialize_with = "deserialize_snoop_share",
        skip_serializing_if = "is_default_snoop_share"
    )]
    pub snoop_share: f64,
    /// The snoop split folded to its strip. Off by default, and written
    /// only while on.
    #[serde(default, skip_serializing_if = "is_false")]
    pub snoop_folded: bool,
    /// Log sessions: the session log keeps every line this profile's
    /// sessions show. None until you choose, which logs every
    /// connection but one to this computer, see [`logs_connection`].
    /// Your choice always wins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_sessions: Option<bool>,
    /// Scrollback size: how many lines both renderers keep above the
    /// screen and the scrollback file keeps for the next launch. From
    /// 1,000 to 100,000, and 10,000, the default, is not written.
    #[serde(
        default = "default_scrollback_lines",
        deserialize_with = "deserialize_scrollback_lines",
        skip_serializing_if = "is_default_scrollback_lines"
    )]
    pub scrollback_lines: u32,
    /// Where you dragged the writing card, its left and top edges in CSS
    /// pixels from the main window's corner. None until you move it,
    /// which keeps the place over the terminal the card works out for
    /// itself. The page keeps the card on screen as the window resizes.
    /// A hand edit that is not a number reads as None.
    #[serde(
        default,
        deserialize_with = "deserialize_writing_card_edge",
        skip_serializing_if = "Option::is_none"
    )]
    pub writing_card_left: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_writing_card_edge",
        skip_serializing_if = "Option::is_none"
    )]
    pub writing_card_top: Option<f64>,
    /// The rows the writing card's text box shows, from dragging its foot.
    /// None until you drag it, which lets the box grow with the text.
    /// From 6 to 500, and anything else in a hand edit reads as None.
    #[serde(
        default,
        deserialize_with = "deserialize_writing_card_rows",
        skip_serializing_if = "Option::is_none"
    )]
    pub writing_card_rows: Option<u32>,
    /// The columns of text the writing card's box shows, from dragging the
    /// grip at its corner. None until you drag it, which keeps 80. From 75
    /// to 500, and anything else in a hand edit reads as None.
    #[serde(
        default,
        deserialize_with = "deserialize_writing_card_cols",
        skip_serializing_if = "Option::is_none"
    )]
    pub writing_card_cols: Option<u32>,
    /// The writing card opens in its pane in the panel. Off by default,
    /// and written only while on.
    #[serde(default, skip_serializing_if = "is_false")]
    pub writing_card_pinned: bool,
    /// Where the snoop window sat when you last moved or sized it, its
    /// outer left and top edges and its inner width and height in logical
    /// pixels. Only Rust writes these, as the window moves, and the page
    /// never reads them. None until the window moves, which opens it at
    /// 760 by 480 where the system puts it. A hand edit that is not a
    /// number, or a width under 480 or a height under 240, reads as None.
    #[serde(
        default,
        deserialize_with = "deserialize_writing_card_edge",
        skip_serializing_if = "Option::is_none"
    )]
    pub snoop_window_left: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_writing_card_edge",
        skip_serializing_if = "Option::is_none"
    )]
    pub snoop_window_top: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_snoop_window_width",
        skip_serializing_if = "Option::is_none"
    )]
    pub snoop_window_width: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_snoop_window_height",
        skip_serializing_if = "Option::is_none"
    )]
    pub snoop_window_height: Option<f64>,
    /// The chat pane's channel colors, picked from its own menu. Each
    /// key is a channel name in lowercase and each value one of the
    /// theme's 16 ANSI slots, like `brightBlue`. A channel left out takes
    /// the color the game prints it in. Only the pane menu writes it,
    /// through its own commands. `ui_get_config` leaves it out and
    /// `ui_set_fields` takes no field for it, so Settings never holds a
    /// copy.
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
    /// wants `JetBrains` `Mono` just for the bar to
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

impl VitalsConfig {
    /// The style that grew from these 0.7 vitals, which the gallery marks
    /// Yours in 0.7. A template that was on drew in place of every layout,
    /// so it gives `text`. Otherwise `gauges`, `pips`, `line` for strip
    /// and inline, and `rows` for stacked. Every profile saved `ember` by
    /// default, so it gives none.
    pub(crate) fn legacy_style(&self) -> Option<&'static str> {
        if self.template_enabled {
            return Some("text");
        }
        match self.layout.as_str() {
            "gauges" => Some("gauges"),
            "pips" => Some("pips"),
            "strip" | "inline" => Some("line"),
            "stacked" => Some("rows"),
            _ => None,
        }
    }

    /// The 0.7 template in today's codes, the text Text starts from, while
    /// it was on.
    pub(crate) fn legacy_text(&self) -> Option<String> {
        self.template_enabled
            .then(|| vosh_prompt::legacy::rewrite_07_vitals(&self.template, self.bar_width))
    }
}

impl UiConfig {
    /// The text the Text style draws. Yours, or while you have none the
    /// 0.7 template that was on, or Vosh's.
    pub(crate) fn vitals_text_drawn(&self) -> Cow<'_, str> {
        if !self.vitals_text.is_empty() {
            Cow::Borrowed(&self.vitals_text)
        } else if let Some(legacy) = self.vitals.legacy_text() {
            Cow::Owned(legacy)
        } else {
            Cow::Borrowed(vosh_prompt::DEFAULT_VITALS_TEXT)
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

/// The ways the status bar draws. Anything else saves as the default,
/// Meters.
pub(crate) const STATUS_STYLES: [&str; 4] = ["meters", "compact", "strip", "dashboard"];

fn default_status_style() -> String {
    "meters".to_string()
}

fn is_default_status_style(value: &str) -> bool {
    value == "meters"
}

/// Keep a known status bar style and turn anything else into `meters`.
pub(crate) fn coerce_status_style(value: String) -> String {
    if STATUS_STYLES.contains(&value.as_str()) {
        value
    } else {
        default_status_style()
    }
}

/// The clocks the status line reads the game time on. Anything else
/// saves as the default, the 24 hour clock.
pub(crate) const GAME_TIMES: [&str; 2] = ["24h", "12h"];

fn default_game_time() -> String {
    "24h".to_string()
}

fn is_default_game_time(value: &str) -> bool {
    value == "24h"
}

/// Keep a known clock and turn anything else into `24h`.
pub(crate) fn coerce_game_time(value: String) -> String {
    if GAME_TIMES.contains(&value.as_str()) {
        value
    } else {
        default_game_time()
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

/// Whether a connection to `host` writes the session log: your Log
/// sessions choice, or until you choose, every host but this computer,
/// such as a test server run beside Vosh, which would fill the log with
/// tests.
pub(crate) fn logs_connection(ui: &UiConfig, host: &str) -> bool {
    ui.log_sessions
        .unwrap_or_else(|| !vosh_log::is_local_host(host))
}

/// The lines of scrollback each terminal keeps until you pick another
/// Scrollback size, the 10,000 both renderers kept before it.
pub(crate) const DEFAULT_SCROLLBACK_LINES: u32 = 10_000;

fn default_scrollback_lines() -> u32 {
    DEFAULT_SCROLLBACK_LINES
}

fn is_default_scrollback_lines(lines: &u32) -> bool {
    *lines == DEFAULT_SCROLLBACK_LINES
}

/// Hold a scrollback size to 1,000 to 100,000 lines, the most the native
/// grid keeps.
pub(crate) fn coerce_scrollback_lines(lines: u32) -> u32 {
    lines.clamp(1_000, 100_000)
}

/// Read the scrollback size leniently, so a hand edit never stops a
/// profile loading. A number holds to its range, and anything else reads
/// as the default.
pub(crate) fn deserialize_scrollback_lines<'de, D>(deser: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Number(i64),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Raw::deserialize(deser)? {
        Raw::Number(n) => coerce_scrollback_lines(u32::try_from(n.max(0)).unwrap_or(u32::MAX)),
        Raw::Other(_) => DEFAULT_SCROLLBACK_LINES,
    })
}

/// The lines in one pulse the screen reader reads one by one until you
/// pick another burst.
pub(crate) const DEFAULT_SCREEN_READER_BURST: u32 = 8;

/// The bursts you can pick.
pub(crate) const SCREEN_READER_BURSTS: [u32; 4] = [4, 8, 16, 32];

fn default_screen_reader_burst() -> u32 {
    DEFAULT_SCREEN_READER_BURST
}

fn is_default_screen_reader_burst(burst: &u32) -> bool {
    *burst == DEFAULT_SCREEN_READER_BURST
}

/// Hold a burst to 4, 8, 16 or 32, and read anything else as 8.
pub(crate) fn coerce_screen_reader_burst(burst: u32) -> u32 {
    if SCREEN_READER_BURSTS.contains(&burst) {
        burst
    } else {
        DEFAULT_SCREEN_READER_BURST
    }
}

/// Read the burst leniently, so a hand edit never stops a profile
/// loading. A number holds to the four bursts, and anything else reads
/// as the default.
pub(crate) fn deserialize_screen_reader_burst<'de, D>(deser: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Number(i64),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Raw::deserialize(deser)? {
        Raw::Number(n) => coerce_screen_reader_burst(u32::try_from(n).unwrap_or(0)),
        Raw::Other(_) => DEFAULT_SCREEN_READER_BURST,
    })
}

/// The share of the terminal column a snoop split takes until you drag it.
pub(crate) const DEFAULT_SNOOP_SHARE: f64 = 0.4;

fn default_snoop_share() -> f64 {
    DEFAULT_SNOOP_SHARE
}

// The default is a constant, so the exact compare is the one meant.
#[allow(clippy::float_cmp)]
fn is_default_snoop_share(share: &f64) -> bool {
    *share == DEFAULT_SNOOP_SHARE
}

/// Hold the snoop split's share to 0.05 to 0.95, so neither the split
/// nor the terminal under it ever closes. Anything that is not a finite
/// number is the default.
pub(crate) fn coerce_snoop_share(share: f64) -> f64 {
    if share.is_finite() {
        share.clamp(0.05, 0.95)
    } else {
        DEFAULT_SNOOP_SHARE
    }
}

/// Read the snoop share leniently, so a hand edit never stops a profile
/// loading. A number holds to 0.05 to 0.95 and anything else reads as
/// the default.
pub(crate) fn deserialize_snoop_share<'de, D>(deser: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Number(f64),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Raw::deserialize(deser)? {
        Raw::Number(n) => coerce_snoop_share(n),
        Raw::Other(_) => DEFAULT_SNOOP_SHARE,
    })
}

/// The fewest and most rows the writing card's text box keeps.
pub(crate) const WRITING_CARD_ROWS_MIN: u32 = 6;
pub(crate) const WRITING_CARD_ROWS_MAX: u32 = 500;

/// The fewest and most columns of text the writing card's box keeps.
pub(crate) const WRITING_CARD_COLS_MIN: u32 = 75;
pub(crate) const WRITING_CARD_COLS_MAX: u32 = 500;

/// Hold a writing card edge to a finite number of pixels, or None.
pub(crate) fn coerce_writing_card_edge(edge: Option<f64>) -> Option<f64> {
    edge.filter(|e| e.is_finite())
        .map(|e| e.clamp(-100_000.0, 100_000.0))
}

/// Hold the writing card's rows to 6 to 500.
pub(crate) fn coerce_writing_card_rows(rows: Option<u32>) -> Option<u32> {
    rows.map(|r| r.clamp(WRITING_CARD_ROWS_MIN, WRITING_CARD_ROWS_MAX))
}

/// Hold the writing card's columns to 75 to 500.
pub(crate) fn coerce_writing_card_cols(cols: Option<u32>) -> Option<u32> {
    cols.map(|c| c.clamp(WRITING_CARD_COLS_MIN, WRITING_CARD_COLS_MAX))
}

/// Read a writing card edge leniently, so a hand edit never stops a
/// profile loading. Anything but a finite number reads as None.
pub(crate) fn deserialize_writing_card_edge<'de, D>(deser: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Number(f64),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Raw::deserialize(deser)? {
        Raw::Number(n) => coerce_writing_card_edge(Some(n)),
        Raw::Other(_) => None,
    })
}

/// Read the writing card's rows leniently. A whole number holds to 6 to
/// 500, and anything else reads as None.
pub(crate) fn deserialize_writing_card_rows<'de, D>(deser: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Number(u64),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Raw::deserialize(deser)? {
        Raw::Number(n) => coerce_writing_card_rows(Some(u32::try_from(n).unwrap_or(u32::MAX))),
        Raw::Other(_) => None,
    })
}

/// Read the writing card's columns leniently. A whole number holds to 75
/// to 500, and anything else reads as None.
pub(crate) fn deserialize_writing_card_cols<'de, D>(deser: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Number(u64),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Raw::deserialize(deser)? {
        Raw::Number(n) => coerce_writing_card_cols(Some(u32::try_from(n).unwrap_or(u32::MAX))),
        Raw::Other(_) => None,
    })
}

/// The smallest snoop window a saved place may ask for, in logical pixels.
pub(crate) const SNOOP_WINDOW_MIN_WIDTH: f64 = 480.0;
pub(crate) const SNOOP_WINDOW_MIN_HEIGHT: f64 = 240.0;

/// Hold a snoop window side to the writing card edge rule, and read one
/// under `min` as None.
pub(crate) fn coerce_snoop_window_side(side: Option<f64>, min: f64) -> Option<f64> {
    coerce_writing_card_edge(side).filter(|s| *s >= min)
}

/// Read the snoop window's saved width leniently. A number under 480
/// or anything but a number reads as None.
pub(crate) fn deserialize_snoop_window_width<'de, D>(deser: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    let edge = deserialize_writing_card_edge(deser)?;
    Ok(coerce_snoop_window_side(edge, SNOOP_WINDOW_MIN_WIDTH))
}

/// Read the snoop window's saved height leniently. A number under 240
/// or anything but a number reads as None.
pub(crate) fn deserialize_snoop_window_height<'de, D>(deser: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    let edge = deserialize_writing_card_edge(deser)?;
    Ok(coerce_snoop_window_side(edge, SNOOP_WINDOW_MIN_HEIGHT))
}

fn default_echo_macros() -> bool {
    true
}

fn default_writing_offer() -> bool {
    true
}

fn default_writing_ask_post() -> bool {
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
    /// The game color fit of `xterm`: the slots Fit game colors moves
    /// in play, body text and the 16 ANSI colors. Settings fits a theme
    /// once when you import it or change one of those colors, and keeps
    /// the fit here. Left out of the file while empty. Vosh 0.8.1 drops
    /// it on save, and the next build fits the theme again.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub fitted: std::collections::BTreeMap<String, String>,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            follow_system_appearance: false,
            light_theme: default_light_theme(),
            dark_theme: String::new(),
            theme_follow: default_theme_follow(),
            day_theme: String::new(),
            night_theme: String::new(),
            auto_update: false,
            font_family: default_font_family(),
            font_size: default_font_size(),
            terminal_line_height: default_terminal_line_height(),
            panel_font: String::new(),
            panel_font_size: default_panel_font_size(),
            tracked_affects: Vec::new(),
            enabled_presets: Vec::new(),
            dock_layout: Vec::new(),
            panes: None,
            keep_last_command: false,
            theme_terminal_colors: None,
            bright_bold: false,
            blink_text: None,
            fit_game_colors: true,
            color_vision: default_color_vision(),
            readable_highlights: true,
            screen_reader: false,
            screen_reader_background: false,
            screen_reader_prompt: false,
            screen_reader_burst: DEFAULT_SCREEN_READER_BURST,
            collapse_repeats: false,
            collapse_fight_lines: true,
            collapse_attack_lines: false,
            terminal_base_ansi: None,
            custom_themes: Vec::new(),
            split_divider_color: None,
            input_echo_color: None,
            echo_macros: true,
            input_echo_caret: true,
            input_echo_mark: default_input_echo_mark(),
            input_echo_mark_text: String::new(),
            input_echo_mark_color: None,
            input_echo_dim: false,
            input_line_mark: true,
            side_panels_fill_height: false,
            paste_line_delay_ms: default_paste_line_delay_ms(),
            spellcheck_prompt: false,
            writing_offer: true,
            writing_ask_post: true,
            input_cursor_style: default_input_cursor_style(),
            input_caret_blink: true,
            input_caret_color: None,
            input_line_color: None,
            input_line_background: default_input_line_background(),
            input_line_background_color: None,
            input_line_size: default_input_line_size(),
            input_type_colors: false,
            input_type_alias_color: None,
            input_type_hash_color: None,
            input_type_chat_color: None,
            input_type_unknown_color: None,
            prompt_template_enabled: false,
            prompt_template: String::new(),
            vitals: VitalsConfig::default(),
            vitals_density: default_vitals_density(),
            vitals_values: default_vitals_values(),
            vitals_meter: default_vitals_meter(),
            vitals_warn_thirds: false,
            vitals_hide_when_pinned: true,
            vitals_style: None,
            vitals_place: default_vitals_place(),
            vitals_order: default_vitals_order(),
            vitals_off: Vec::new(),
            vitals_opponent: default_vitals_opponent(),
            vitals_colors: BTreeMap::new(),
            vitals_text: String::new(),
            vitals_text_previous: Vec::new(),
            vitals_hit: false,
            moons_position: default_moons_position(),
            chip_style: default_chip_style(),
            status_style: default_status_style(),
            tick_count: default_tick_count(),
            game_time: default_game_time(),
            affects_style: default_affects_style(),
            affects_marker: default_affects_marker(),
            affects_tint: false,
            affects_running_out_hours: DEFAULT_AFFECTS_RUNNING_OUT_HOURS,
            affects_almost_gone_hours: DEFAULT_AFFECTS_ALMOST_GONE_HOURS,
            snoop_share: DEFAULT_SNOOP_SHARE,
            snoop_folded: false,
            log_sessions: None,
            scrollback_lines: DEFAULT_SCROLLBACK_LINES,
            writing_card_left: None,
            writing_card_top: None,
            writing_card_rows: None,
            writing_card_cols: None,
            writing_card_pinned: false,
            snoop_window_left: None,
            snoop_window_top: None,
            snoop_window_width: None,
            snoop_window_height: None,
            chat_colors: BTreeMap::new(),
        }
    }
}

/// The theme a file without the key reads, and the fallback. A new
/// install starts on Triad instead, which `NEW_INSTALL_THEME` in
/// profile/set.rs writes before the first launch.
fn default_theme() -> String {
    "obsidian-ember".to_string()
}

/// The light theme a file without the key reads. Vellum is retired, and
/// the frontend shows Rubric for it (`RETIRED_THEMES` in themes.ts),
/// while Vosh 0.8.1 still reads it as Vellum. A new install starts with
/// Rubric itself, from `NEW_INSTALL_LIGHT_THEME` in profile/set.rs.
fn default_light_theme() -> String {
    "vellum".to_string()
}

/// Trim a light theme pick and turn a blank one into `vellum`, which
/// shows Rubric.
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

/// The modes of the Switch themes row. Anything else saves as `off`.
pub(crate) const THEME_FOLLOWS: [&str; 3] = ["off", "system", "game"];

fn default_theme_follow() -> String {
    "off".to_string()
}

fn is_default_theme_follow(value: &str) -> bool {
    value == "off"
}

/// Keep a known Switch themes mode and turn anything else into `off`.
pub(crate) fn coerce_theme_follow(value: String) -> String {
    if THEME_FOLLOWS.contains(&value.as_str()) {
        value
    } else {
        default_theme_follow()
    }
}

/// The Switch themes mode a file reads as. `follow_system_appearance`
/// true is `system` whatever `theme_follow` says, so a file without the
/// key keeps the choice 0.8.1 saved, and an older Vosh that turns it on
/// in a file that says `game` wins. Else `game` stays, and anything else
/// is `off`, since `system` without the switch means an older Vosh turned
/// it off.
pub(crate) fn read_theme_follow(follow_system_appearance: bool, theme_follow: &str) -> String {
    match (follow_system_appearance, theme_follow) {
        (true, _) => "system".to_string(),
        (false, "game") => "game".to_string(),
        _ => default_theme_follow(),
    }
}

/// Set the Switch themes mode and keep `follow_system_appearance` true
/// only for `system`.
pub(crate) fn set_theme_follow(ui: &mut UiConfig, value: String) {
    ui.theme_follow = coerce_theme_follow(value);
    ui.follow_system_appearance = ui.theme_follow == "system";
}

/// Turn Follow system appearance on or off, which is the `system` mode
/// of Switch themes. Off leaves `game` as it is.
pub(crate) fn set_follow_system_appearance(ui: &mut UiConfig, on: bool) {
    let mode = match (on, ui.theme_follow.as_str()) {
        (true, _) => "system".to_string(),
        (false, "system") => default_theme_follow(),
        (false, kept) => kept.to_string(),
    };
    set_theme_follow(ui, mode);
}

/// The marks the echo of a command you send can start with. Anything
/// else saves as `chevron`.
pub(crate) const INPUT_ECHO_MARKS: [&str; 4] = ["off", "chevron", "gt", "own"];

/// The most characters your own mark keeps.
pub(crate) const INPUT_ECHO_MARK_TEXT_MAX: usize = 4;

fn default_input_echo_mark() -> String {
    "chevron".to_string()
}

fn is_default_input_echo_mark(value: &str) -> bool {
    value == "chevron"
}

/// Keep a known mark and turn anything else into `chevron`.
pub(crate) fn coerce_input_echo_mark(value: String) -> String {
    if INPUT_ECHO_MARKS.contains(&value.as_str()) {
        value
    } else {
        default_input_echo_mark()
    }
}

/// Your own mark as it saves. Control characters drop, the ends trim, and
/// it keeps at most [`INPUT_ECHO_MARK_TEXT_MAX`] characters.
pub(crate) fn coerce_input_echo_mark_text(value: String) -> String {
    let clean: String = value.chars().filter(|c| !c.is_control()).collect();
    let kept: String = clean
        .trim()
        .chars()
        .take(INPUT_ECHO_MARK_TEXT_MAX)
        .collect();
    kept.trim_end().to_string()
}

/// The mark a file reads as. `input_echo_caret` false is `off` whatever
/// `input_echo_mark` says, so a file without the key keeps the choice an
/// older build saved. True with `off` means an older build turned the mark
/// back on, which reads as `chevron`. Else the mark stays, coerced.
pub(crate) fn read_input_echo_mark(input_echo_caret: bool, input_echo_mark: &str) -> String {
    match (input_echo_caret, input_echo_mark) {
        (false, _) => "off".to_string(),
        (true, "off") => default_input_echo_mark(),
        (true, mark) => coerce_input_echo_mark(mark.to_string()),
    }
}

/// Set the mark and keep `input_echo_caret` true unless it is `off`.
pub(crate) fn set_input_echo_mark(ui: &mut UiConfig, value: String) {
    ui.input_echo_mark = coerce_input_echo_mark(value);
    ui.input_echo_caret = ui.input_echo_mark != "off";
}

/// Trim a day or night theme pick. A blank one stays blank, which the
/// page reads as the theme showing.
pub(crate) fn normalize_day_night_theme(value: String) -> String {
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

/// The color visions the game color fit knows (`COLOR_VISIONS` in
/// src/theme/gameFit.ts). Anything else saves as the default.
pub(crate) const COLOR_VISIONS: [&str; 4] = ["typical", "deuteranopia", "protanopia", "tritanopia"];

pub(crate) fn default_color_vision() -> String {
    "typical".to_string()
}

pub(crate) fn is_default_color_vision(value: &str) -> bool {
    value == "typical"
}

/// Keep a known color vision and turn anything else into `typical`.
pub(crate) fn coerce_color_vision(value: String) -> String {
    if COLOR_VISIONS.contains(&value.as_str()) {
        value
    } else {
        default_color_vision()
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

/// The vitals styles the gallery adds to Rows and One line. Anything
/// else saves as None, which draws `vitals_density`.
pub(crate) const VITALS_STYLES: [&str; 13] = [
    "ledger", "gauges", "pips", "bands", "ladders", "blocks", "traces", "dials", "rings", "vials",
    "orbs", "candles", "text",
];

/// Keep a known style and turn anything else into None.
pub(crate) fn coerce_vitals_style(value: Option<String>) -> Option<String> {
    value.filter(|style| VITALS_STYLES.contains(&style.as_str()))
}

/// The places your vitals show. Anything else saves as the default.
pub(crate) const VITALS_PLACES: [&str; 2] = ["panel", "status"];

fn default_vitals_place() -> String {
    "panel".to_string()
}

fn is_default_vitals_place(value: &str) -> bool {
    value == "panel"
}

/// Keep a known place and turn anything else into `panel`.
pub(crate) fn coerce_vitals_place(value: String) -> String {
    if VITALS_PLACES.contains(&value.as_str()) {
        value
    } else {
        default_vitals_place()
    }
}

/// Your vitals in today's order, which `vitals_order` defaults to.
pub(crate) const VITALS: [&str; 3] = ["hp", "mana", "move"];

fn default_vitals_order() -> Vec<String> {
    VITALS.map(String::from).to_vec()
}

fn is_default_vitals_order(order: &[String]) -> bool {
    order.iter().map(String::as_str).eq(VITALS)
}

/// Keep each known vital once, in the order given, and add any it is
/// missing after them in today's order.
pub(crate) fn coerce_vitals_order(order: Vec<String>) -> Vec<String> {
    let mut kept: Vec<String> = Vec::with_capacity(VITALS.len());
    for vital in order.into_iter().chain(default_vitals_order()) {
        if VITALS.contains(&vital.as_str()) && !kept.contains(&vital) {
            kept.push(vital);
        }
    }
    kept
}

/// What `vitals_off` can hold: each vital, and your opponent's row.
pub(crate) const VITALS_OFF: [&str; 4] = ["hp", "mana", "move", "opponent"];

/// Keep each known name once, in the order of [`VITALS_OFF`], so the
/// same set always saves the same way.
pub(crate) fn coerce_vitals_off(off: Vec<String>) -> Vec<String> {
    VITALS_OFF
        .iter()
        .filter(|name| off.iter().any(|o| o == *name))
        .map(|name| (*name).to_string())
        .collect()
}

/// The places your opponent's row takes. Anything else saves as the
/// default.
pub(crate) const VITALS_OPPONENT_PLACES: [&str; 2] = ["top", "bottom"];

fn default_vitals_opponent() -> String {
    "top".to_string()
}

fn is_default_vitals_opponent(value: &str) -> bool {
    value == "top"
}

/// Keep a known place for your opponent and turn anything else into
/// `top`.
pub(crate) fn coerce_vitals_opponent(value: String) -> String {
    if VITALS_OPPONENT_PLACES.contains(&value.as_str()) {
        value
    } else {
        default_vitals_opponent()
    }
}

/// The last ANSI slot a vital's color can take.
const VITALS_COLOR_SLOT_MAX: u8 = 15;

/// Keep the colors of known vitals that name a slot from 0 to 15, and
/// drop the rest, which then take Default.
pub(crate) fn coerce_vitals_colors(colors: BTreeMap<String, i64>) -> BTreeMap<String, u8> {
    colors
        .into_iter()
        .filter(|(vital, _)| VITALS.contains(&vital.as_str()))
        .filter_map(|(vital, slot)| {
            let slot = u8::try_from(slot)
                .ok()
                .filter(|s| *s <= VITALS_COLOR_SLOT_MAX)?;
            Some((vital, slot))
        })
        .collect()
}

/// Read the vitals colors leniently, so a hand edit never stops a
/// profile loading. A value that is not a whole number drops that vital,
/// as a slot past 15 does.
pub(crate) fn deserialize_vitals_colors<'de, D>(deser: D) -> Result<BTreeMap<String, u8>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Whole(i64),
        Other(serde::de::IgnoredAny),
    }
    let raw = BTreeMap::<String, Raw>::deserialize(deser)?;
    Ok(coerce_vitals_colors(
        raw.into_iter()
            .filter_map(|(vital, slot)| match slot {
                Raw::Whole(slot) => Some((vital, slot)),
                Raw::Other(_) => None,
            })
            .collect(),
    ))
}

/// How many earlier texts `vitals_text_previous` keeps.
pub(crate) const VITALS_TEXT_PREVIOUS: usize = 2;

/// Drop blank and repeated texts and keep the newest two.
pub(crate) fn normalize_vitals_text_previous(texts: Vec<String>) -> Vec<String> {
    let mut kept: Vec<String> = Vec::with_capacity(VITALS_TEXT_PREVIOUS);
    for text in texts {
        if kept.len() == VITALS_TEXT_PREVIOUS {
            break;
        }
        if !text.is_empty() && !kept.contains(&text) {
            kept.push(text);
        }
    }
    kept
}

/// Write a new vitals text and put the one it replaces first among the
/// earlier texts, so you can take it back. A blank text is never kept,
/// and the new text leaves the earlier ones, since it is no longer
/// earlier.
pub(crate) fn replace_vitals_text(ui: &mut UiConfig, text: String) {
    let replaced = std::mem::replace(&mut ui.vitals_text, text);
    let mut previous = std::mem::take(&mut ui.vitals_text_previous);
    previous.retain(|earlier| *earlier != ui.vitals_text);
    if replaced != ui.vitals_text {
        previous.insert(0, replaced);
    }
    ui.vitals_text_previous = normalize_vitals_text_previous(previous);
}

fn default_font_family() -> String {
    "\"JetBrainsMono Bundled\", Menlo, Consolas, ui-monospace, monospace".to_string()
}

/// The default font list before the current one.
/// Profile files saved then hold it where you never picked a font, and
/// it draws as the default list does now (`rendered_families` in
/// native/gpu/atlas.rs and `renderFontStack` in src/lib/fontLoader.ts).
pub(crate) const RETIRED_DEFAULT_FONT_FAMILY: &str =
    "BerkeleyMono Nerd Font, JetBrains Mono, Fira Code, Menlo, Consolas, ui-monospace, monospace";

/// Whether `family` is the default font list, this one or the one before
/// it.
pub(crate) fn is_default_font_family(family: &str) -> bool {
    family == default_font_family() || family == RETIRED_DEFAULT_FONT_FAMILY
}

fn default_font_size() -> TextPx {
    TextPx::whole(14)
}

/// The smallest text size Settings saves.
const MIN_TEXT_SIZE: TextPx = TextPx::whole(6);

/// The largest text size Settings saves.
const MAX_TEXT_SIZE: TextPx = TextPx::whole(64);

/// Hold the terminal font size to 6 to 64 pixels, keeping a half step.
pub(crate) fn coerce_font_size(size: TextPx) -> TextPx {
    size.clamp(MIN_TEXT_SIZE, MAX_TEXT_SIZE)
}

/// What the Panel font row saves for the terminal font. Empty is As
/// designed.
pub(crate) const PANEL_FONT_TERMINAL: &str = "terminal";

/// What the Panel font row saves for the system font.
pub(crate) const PANEL_FONT_SYSTEM: &str = "system";

/// Trim a panel font pick and spell the terminal font and the system
/// font one way each. Anything else is a font list, kept as written.
pub(crate) fn normalize_panel_font(value: String) -> String {
    let value = value.trim();
    [PANEL_FONT_TERMINAL, PANEL_FONT_SYSTEM]
        .into_iter()
        .find(|named| value.eq_ignore_ascii_case(named))
        .unwrap_or(value)
        .to_string()
}

/// What the panel Size row saves to follow the terminal size.
pub(crate) const PANEL_FONT_SIZE_TERMINAL: TextPx = TextPx::whole(0);

/// The panel size a profile starts at, the size the panes were drawn at.
pub(crate) const DEFAULT_PANEL_FONT_SIZE: TextPx = TextPx::whole(12);

fn default_panel_font_size() -> TextPx {
    DEFAULT_PANEL_FONT_SIZE
}

fn is_default_panel_font_size(size: &TextPx) -> bool {
    *size == DEFAULT_PANEL_FONT_SIZE
}

/// Hold a panel size to the terminal size's 6 to 64 pixels, keeping 0,
/// which follows the terminal size.
pub(crate) fn coerce_panel_font_size(size: TextPx) -> TextPx {
    if size == PANEL_FONT_SIZE_TERMINAL {
        size
    } else {
        coerce_font_size(size)
    }
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

/// The backgrounds the command line draws. Anything else saves as the
/// default, the theme's.
pub(crate) const INPUT_LINE_BACKGROUNDS: [&str; 3] = ["theme", "tint", "own"];

fn default_input_line_background() -> String {
    "theme".to_string()
}

fn is_default_input_line_background(value: &str) -> bool {
    value == "theme"
}

/// Keep a known background and turn anything else into `theme`.
pub(crate) fn coerce_input_line_background(value: String) -> String {
    if INPUT_LINE_BACKGROUNDS.contains(&value.as_str()) {
        value
    } else {
        default_input_line_background()
    }
}

/// What the command line Size row saves to follow the terminal size.
pub(crate) const INPUT_LINE_SIZE_TERMINAL: TextPx = TextPx::whole(0);

fn default_input_line_size() -> TextPx {
    INPUT_LINE_SIZE_TERMINAL
}

fn is_default_input_line_size(size: &TextPx) -> bool {
    *size == INPUT_LINE_SIZE_TERMINAL
}

/// Hold a command line size to the terminal size's 6 to 64 pixels,
/// keeping 0, which follows the terminal size.
pub(crate) fn coerce_input_line_size(size: TextPx) -> TextPx {
    if size == INPUT_LINE_SIZE_TERMINAL {
        size
    } else {
        coerce_font_size(size)
    }
}

pub(crate) fn default_true() -> bool {
    true
}

/// Leave a switch that is on by default out of the file while it is on.
pub(super) fn is_true(on: &bool) -> bool {
    *on
}

/// Leave a switch that is off by default out of the file while it is off.
fn is_false(on: &bool) -> bool {
    !*on
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::file::ProfileConfig;

    #[test]
    fn text_draws_your_0_7_template_while_you_have_none() {
        let full = include_str!("../../../fixtures/config/profile.full.toml");
        let mut ui = ProfileConfig::from_toml(full).unwrap().ui;
        assert_eq!(ui.vitals_text, "");
        assert_eq!(
            ui.vitals_text_drawn(),
            "%hp/%maxhp %mana/%maxmn %move/%maxmv"
        );
        ui.vitals_text = "%hp hp".into();
        assert_eq!(ui.vitals_text_drawn(), "%hp hp");
        let default = include_str!("../../../fixtures/config/profile.default.toml");
        let ui = ProfileConfig::from_toml(default).unwrap().ui;
        assert_eq!(ui.vitals_text_drawn(), vosh_prompt::DEFAULT_VITALS_TEXT);
    }

    #[test]
    fn update_checks_stay_off_until_you_turn_them_on() {
        assert!(!UiConfig::default().auto_update);
        let parsed = ProfileConfig::from_toml("[ui]\nfont_size = 14\n").unwrap();
        assert!(!parsed.ui.auto_update);
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
    fn the_old_mark_switch_reads_as_the_mark_and_saves_in_step() {
        let read = |text: &str| ProfileConfig::from_toml(text).unwrap().ui;
        // A profile from before the setting, or with the switch on, reads ›.
        for text in [
            "[ui]\ntheme = \"nord\"\n",
            "[ui]\ninput_echo_caret = true\n",
        ] {
            assert_eq!(read(text).input_echo_mark, "chevron", "{text}");
        }
        let off = read("[ui]\ninput_echo_caret = false\n");
        assert_eq!(off.input_echo_mark, "off");
        let text = through_text(&off);
        assert!(text.contains("input_echo_caret = false"), "{text}");
        assert!(text.contains("input_echo_mark = \"off\""), "{text}");
        // An older build that turns the switch off drops the mark, and one
        // that turns it back on reads as ›.
        assert_eq!(
            read("[ui]\ninput_echo_caret = false\ninput_echo_mark = \"gt\"\n").input_echo_mark,
            "off"
        );
        assert_eq!(
            read("[ui]\ninput_echo_caret = true\ninput_echo_mark = \"off\"\n").input_echo_mark,
            "chevron"
        );
        // Every mark but off saves the switch on, and only › is left out.
        for mark in INPUT_ECHO_MARKS {
            let mut ui = UiConfig::default();
            set_input_echo_mark(&mut ui, mark.to_string());
            let text = through_text(&ui);
            let on = mark != "off";
            assert!(
                text.contains(&format!("input_echo_caret = {on}")),
                "{mark}: {text}"
            );
            assert_eq!(
                text.contains("input_echo_mark ="),
                mark != "chevron",
                "{mark}: {text}"
            );
            assert_eq!(through_toml(&ui).input_echo_mark, mark);
        }
        let mut ui = UiConfig::default();
        set_input_echo_mark(&mut ui, "caret".into());
        assert_eq!(
            (ui.input_echo_mark.as_str(), ui.input_echo_caret),
            ("chevron", true)
        );
    }

    #[test]
    fn your_own_mark_keeps_four_characters_and_no_control_ones() {
        for (typed, kept) in [
            ("T>", "T>"),
            ("  ab  ", "ab"),
            ("\u{1b}[1m>>", "[1m>"),
            ("a\tb\nc", "abc"),
            ("abc def", "abc"),
            ("ᚠᚢᚦᚨᚱ", "ᚠᚢᚦᚨ"),
            ("\u{7}", ""),
        ] {
            assert_eq!(coerce_input_echo_mark_text(typed.into()), kept, "{typed:?}");
        }
        let ui = ProfileConfig::from_toml("[ui]\ninput_echo_mark_text = \" >>>>> \"\n")
            .unwrap()
            .ui;
        assert_eq!(ui.input_echo_mark_text, ">>>>");
    }

    #[test]
    fn the_mark_settings_save_only_once_they_change() {
        let text = through_text(&UiConfig::default());
        for key in [
            "input_echo_mark",
            "input_echo_mark_text",
            "input_echo_mark_color",
            "input_echo_dim",
            "input_line_mark",
        ] {
            assert!(!text.contains(&format!("{key} =")), "{key}: {text}");
        }
        let ui = UiConfig {
            input_echo_mark: "own".into(),
            input_echo_mark_text: "T>".into(),
            input_echo_mark_color: Some("#c6a46a".into()),
            input_echo_dim: true,
            input_line_mark: false,
            ..UiConfig::default()
        };
        let back = through_toml(&ui);
        assert_eq!(back.input_echo_mark, "own");
        assert_eq!(back.input_echo_mark_text, "T>");
        assert_eq!(back.input_echo_mark_color.as_deref(), Some("#c6a46a"));
        assert!(back.input_echo_dim);
        assert!(!back.input_line_mark);
    }

    #[test]
    fn the_command_line_look_saves_only_once_it_changes() {
        let text = through_text(&UiConfig::default());
        for key in [
            "input_caret_blink",
            "input_caret_color",
            "input_line_color",
            "input_line_background",
            "input_line_background_color",
            "input_line_size",
        ] {
            assert!(!text.contains(&format!("{key} =")), "{key}: {text}");
        }
        let back = through_toml(&UiConfig::default());
        assert!(back.input_caret_blink);
        assert_eq!(back.input_caret_color, None);
        assert_eq!(back.input_line_color, None);
        assert_eq!(back.input_line_background, "theme");
        assert_eq!(back.input_line_background_color, None);
        assert_eq!(back.input_line_size, TextPx::whole(0));
        let ui = UiConfig {
            input_caret_blink: false,
            input_caret_color: Some("#c6a46a".into()),
            input_line_color: Some("#d8dee9".into()),
            input_line_background: "tint".into(),
            input_line_background_color: Some("#1d1f21".into()),
            input_line_size: TextPx::whole(16),
            ..UiConfig::default()
        };
        let back = through_toml(&ui);
        assert!(!back.input_caret_blink);
        assert_eq!(back.input_caret_color.as_deref(), Some("#c6a46a"));
        assert_eq!(back.input_line_color.as_deref(), Some("#d8dee9"));
        assert_eq!(back.input_line_background, "tint");
        assert_eq!(back.input_line_background_color.as_deref(), Some("#1d1f21"));
        assert_eq!(back.input_line_size, TextPx::whole(16));
        for pick in ["theme", "tint", "own"] {
            let ui = UiConfig {
                input_line_background: pick.into(),
                ..UiConfig::default()
            };
            assert_eq!(through_toml(&ui).input_line_background, pick);
        }
    }

    #[test]
    fn coloring_as_you_type_saves_only_once_it_changes() {
        let keys = [
            "input_type_colors",
            "input_type_alias_color",
            "input_type_hash_color",
            "input_type_chat_color",
            "input_type_unknown_color",
        ];
        let text = through_text(&UiConfig::default());
        for key in keys {
            assert!(!text.contains(&format!("{key} =")), "{key}: {text}");
        }
        let back = through_toml(&UiConfig::default());
        assert!(!back.input_type_colors);
        assert_eq!(back.input_type_alias_color, None);
        assert_eq!(back.input_type_hash_color, None);
        assert_eq!(back.input_type_chat_color, None);
        assert_eq!(back.input_type_unknown_color, None);
        let ui = UiConfig {
            input_type_colors: true,
            input_type_alias_color: Some("#8abeb7".into()),
            input_type_hash_color: Some("#b294bb".into()),
            input_type_chat_color: Some("#f0c674".into()),
            input_type_unknown_color: Some("#cc6666".into()),
            ..UiConfig::default()
        };
        let text = through_text(&ui);
        for key in keys {
            assert!(text.contains(&format!("{key} =")), "{key}: {text}");
        }
        let back = through_toml(&ui);
        assert!(back.input_type_colors);
        assert_eq!(back.input_type_alias_color.as_deref(), Some("#8abeb7"));
        assert_eq!(back.input_type_hash_color.as_deref(), Some("#b294bb"));
        assert_eq!(back.input_type_chat_color.as_deref(), Some("#f0c674"));
        assert_eq!(back.input_type_unknown_color.as_deref(), Some("#cc6666"));
    }

    #[test]
    fn an_unknown_command_line_background_reads_as_the_theme() {
        for value in ["theme", "tint", "own"] {
            assert_eq!(coerce_input_line_background(value.into()), value);
        }
        for value in ["", "Tint", "glass"] {
            assert_eq!(coerce_input_line_background(value.into()), "theme");
        }
    }

    #[test]
    fn a_command_line_size_holds_to_the_terminal_sizes_and_keeps_same_as_terminal() {
        assert_eq!(
            coerce_input_line_size(INPUT_LINE_SIZE_TERMINAL),
            TextPx::whole(0)
        );
        assert_eq!(coerce_input_line_size(TextPx::whole(3)), TextPx::whole(6));
        assert_eq!(coerce_input_line_size(TextPx::whole(14)), TextPx::whole(14));
        assert_eq!(coerce_input_line_size(TextPx::whole(90)), TextPx::whole(64));
    }

    /// The profile file a save of `ui` writes.
    fn through_text(ui: &UiConfig) -> String {
        let config = ProfileConfig {
            ui: ui.clone(),
            ..ProfileConfig::default()
        };
        config.to_toml().unwrap()
    }

    /// Save `ui` to a profile file and read it back.
    fn through_toml(ui: &UiConfig) -> UiConfig {
        ProfileConfig::from_toml(&through_text(ui)).unwrap().ui
    }

    #[test]
    fn blinking_text_keeps_your_choice_and_none_until_you_make_one() {
        // None is no choice, which the page reads from the system's reduce
        // motion setting, so it never reaches the file as a value.
        let mut ui = UiConfig::default();
        assert_eq!(through_toml(&ui).blink_text, None);
        let written = ProfileConfig::default().to_toml().unwrap();
        assert!(!written.contains("blink_text"), "{written}");
        for choice in [true, false] {
            ui.blink_text = Some(choice);
            assert_eq!(through_toml(&ui).blink_text, Some(choice));
        }
    }

    #[test]
    fn follow_system_appearance_round_trips() {
        let ui = UiConfig {
            follow_system_appearance: true,
            ..UiConfig::default()
        };
        assert!(through_toml(&ui).follow_system_appearance);
    }

    #[test]
    fn light_theme_round_trips() {
        let ui = UiConfig {
            light_theme: "classic-vivid".into(),
            ..UiConfig::default()
        };
        assert_eq!(through_toml(&ui).light_theme, "classic-vivid");
    }

    #[test]
    fn dark_theme_round_trips() {
        let ui = UiConfig {
            dark_theme: "nord".into(),
            ..UiConfig::default()
        };
        assert_eq!(through_toml(&ui).dark_theme, "nord");
    }

    #[test]
    fn the_switch_themes_keys_round_trip_and_stay_out_of_the_file_at_their_defaults() {
        let written = ProfileConfig::default().to_toml().unwrap();
        for key in ["theme_follow", "day_theme", "night_theme"] {
            assert!(!written.contains(key), "{key}: {written}");
        }
        let mut ui = UiConfig::default();
        set_theme_follow(&mut ui, "game".into());
        ui.day_theme = "classic-vivid".into();
        ui.night_theme = "nord".into();
        let back = through_toml(&ui);
        assert_eq!(back.theme_follow, "game");
        assert_eq!(back.day_theme, "classic-vivid");
        assert_eq!(back.night_theme, "nord");
        set_theme_follow(&mut ui, "system".into());
        assert_eq!(through_toml(&ui).theme_follow, "system");
    }

    #[test]
    fn a_file_from_0_8_1_reads_its_switch_as_the_mode() {
        for (text, mode) in [
            ("[ui]\nfollow_system_appearance = true\n", "system"),
            ("[ui]\nfollow_system_appearance = false\n", "off"),
            ("[ui]\ntheme = \"nord\"\n", "off"),
            // An older Vosh turned the switch on in a file that says game.
            (
                "[ui]\nfollow_system_appearance = true\ntheme_follow = \"game\"\n",
                "system",
            ),
            // An older Vosh turned the switch off in a file that says system.
            (
                "[ui]\nfollow_system_appearance = false\ntheme_follow = \"system\"\n",
                "off",
            ),
            ("[ui]\ntheme_follow = \"dusk\"\n", "off"),
        ] {
            let ui = ProfileConfig::from_toml(text).unwrap().ui;
            assert_eq!(ui.theme_follow, mode, "{text}");
            assert_eq!(ui.follow_system_appearance, mode == "system", "{text}");
        }
    }

    #[test]
    fn game_writes_the_switch_off_so_0_8_1_reads_it_as_off() {
        let mut ui = UiConfig::default();
        set_follow_system_appearance(&mut ui, true);
        set_theme_follow(&mut ui, "game".into());
        assert!(!ui.follow_system_appearance);
        let text = ProfileConfig {
            ui,
            ..ProfileConfig::default()
        }
        .to_toml()
        .unwrap();
        assert!(text.contains("follow_system_appearance = false"), "{text}");
        assert!(text.contains("theme_follow = \"game\""), "{text}");
    }

    #[test]
    fn the_follow_system_switch_moves_between_system_and_off_and_leaves_game() {
        let mut ui = UiConfig::default();
        set_follow_system_appearance(&mut ui, true);
        assert_eq!(
            (ui.theme_follow.as_str(), ui.follow_system_appearance),
            ("system", true)
        );
        set_follow_system_appearance(&mut ui, false);
        assert_eq!(
            (ui.theme_follow.as_str(), ui.follow_system_appearance),
            ("off", false)
        );
        set_theme_follow(&mut ui, "game".into());
        set_follow_system_appearance(&mut ui, false);
        assert_eq!(ui.theme_follow, "game");
        assert_eq!(coerce_theme_follow("Game".into()), "off");
    }

    #[test]
    fn terminal_line_height_round_trips() {
        let mut ui = UiConfig::default();
        for id in ["compact", "default", "loose"] {
            ui.terminal_line_height = id.into();
            assert_eq!(through_toml(&ui).terminal_line_height, id);
        }
    }

    #[test]
    fn the_panel_font_round_trips_and_stays_out_of_the_file_until_you_pick_one() {
        // As designed, the default, writes nothing, so every file saved
        // before the row keeps its bytes and reads it back.
        let written = ProfileConfig::default().to_toml().unwrap();
        assert!(!written.contains("panel_font"), "{written}");
        let old = ProfileConfig::from_toml("[ui]\ntheme = \"nord\"\n").unwrap();
        assert_eq!(old.ui.panel_font, "");
        let mut ui = UiConfig::default();
        assert_eq!(through_toml(&ui).panel_font, "");
        for pick in ["terminal", "system", "\"Iosevka\", Menlo, monospace"] {
            ui.panel_font = pick.into();
            assert_eq!(through_toml(&ui).panel_font, pick);
        }
        let config = ProfileConfig {
            ui,
            ..ProfileConfig::default()
        };
        let text = config.to_toml().unwrap();
        assert!(
            text.contains(r#"panel_font = "\"Iosevka\", Menlo, monospace""#),
            "{text}"
        );
    }

    #[test]
    fn the_vitals_styles_keys_round_trip_and_stay_out_of_the_file_until_you_pick() {
        // Nothing picked writes none of the eight keys, so every file
        // saved before them keeps its bytes, and a file without them
        // reads the defaults.
        let written = ProfileConfig::default().to_toml().unwrap();
        for key in [
            "vitals_style",
            "vitals_place",
            "vitals_order",
            "vitals_off",
            "vitals_opponent",
            "vitals_colors",
            "vitals_text",
        ] {
            assert!(!written.contains(key), "{key}: {written}");
        }
        let old = ProfileConfig::from_toml("[ui]\nvitals_density = \"line\"\n").unwrap();
        let fresh = UiConfig::default();
        assert_eq!(old.ui.vitals_style, None);
        assert_eq!(old.ui.vitals_place, "panel");
        assert_eq!(old.ui.vitals_order, ["hp", "mana", "move"]);
        assert_eq!(old.ui.vitals_off, Vec::<String>::new());
        assert_eq!(old.ui.vitals_opponent, "top");
        assert!(old.ui.vitals_colors.is_empty());
        assert_eq!(old.ui.vitals_text, "");
        assert_eq!(old.ui.vitals_text_previous, Vec::<String>::new());
        assert_eq!(through_toml(&fresh).vitals_order, fresh.vitals_order);

        let ui = UiConfig {
            vitals_style: Some("pips".into()),
            vitals_place: "status".into(),
            vitals_order: vec!["move".into(), "hp".into(), "mana".into()],
            vitals_off: vec!["mana".into(), "opponent".into()],
            vitals_opponent: "bottom".into(),
            vitals_colors: BTreeMap::from([("hp".into(), 1), ("mana".into(), 12)]),
            vitals_text: "%hp/%maxhp %mn/%maxmn %mv/%maxmv".into(),
            vitals_text_previous: vec!["%hp(%pct_hp)h %mn(%pct_mn)m %mv(%pct_mv)v".into()],
            ..UiConfig::default()
        };
        let back = through_toml(&ui);
        assert_eq!(back.vitals_style, ui.vitals_style);
        assert_eq!(back.vitals_place, ui.vitals_place);
        assert_eq!(back.vitals_order, ui.vitals_order);
        assert_eq!(back.vitals_off, ui.vitals_off);
        assert_eq!(back.vitals_opponent, ui.vitals_opponent);
        assert_eq!(back.vitals_colors, ui.vitals_colors);
        assert_eq!(back.vitals_text, ui.vitals_text);
        assert_eq!(back.vitals_text_previous, ui.vitals_text_previous);
    }

    #[test]
    fn show_each_hit_stays_out_of_the_file_until_you_turn_it_on() {
        let written = ProfileConfig::default().to_toml().unwrap();
        assert!(!written.contains("vitals_hit"), "{written}");
        assert!(!ProfileConfig::from_toml("[ui]\n").unwrap().ui.vitals_hit);

        let ui = UiConfig {
            vitals_hit: true,
            vitals_style: Some("candles".into()),
            ..UiConfig::default()
        };
        let back = through_toml(&ui);
        assert!(back.vitals_hit);
        assert_eq!(back.vitals_style.as_deref(), Some("candles"));
    }

    #[test]
    fn junk_in_the_vitals_styles_keys_saves_as_something_vosh_draws() {
        let strings = |list: &[&str]| list.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        for style in VITALS_STYLES {
            assert_eq!(
                coerce_vitals_style(Some(style.into())).as_deref(),
                Some(style)
            );
        }
        // Rows and One line live in vitals_density, so they are no style.
        for junk in ["rows", "line", "Gauges", "Rings", ""] {
            assert_eq!(coerce_vitals_style(Some(junk.into())), None, "{junk}");
        }
        assert_eq!(coerce_vitals_style(None), None);

        assert_eq!(coerce_vitals_place("status".into()), "status");
        assert_eq!(coerce_vitals_place("footer".into()), "panel");
        assert_eq!(coerce_vitals_opponent("bottom".into()), "bottom");
        assert_eq!(coerce_vitals_opponent("middle".into()), "top");

        // Known vitals once each, in your order, then any missing.
        assert_eq!(
            coerce_vitals_order(strings(&["move", "move", "tp", "hp"])),
            ["move", "hp", "mana"]
        );
        assert_eq!(coerce_vitals_order(Vec::new()), ["hp", "mana", "move"]);

        assert_eq!(
            coerce_vitals_off(strings(&["opponent", "move", "move", "tp"])),
            ["move", "opponent"]
        );

        let colors = BTreeMap::from([
            ("hp".to_string(), 0),
            ("mana".to_string(), 15),
            ("move".to_string(), 16),
            ("opponent".to_string(), 3),
        ]);
        assert_eq!(
            coerce_vitals_colors(colors),
            BTreeMap::from([("hp".to_string(), 0), ("mana".to_string(), 15)])
        );
        assert!(coerce_vitals_colors(BTreeMap::from([("hp".to_string(), -1)])).is_empty());

        // A hand edit that holds no slot drops that vital and loads.
        let toml = "[ui.vitals_colors]\nhp = \"red\"\nmana = 99\nmove = 6\n";
        let ui = ProfileConfig::from_toml(toml).unwrap().ui;
        assert_eq!(ui.vitals_colors, BTreeMap::from([("move".to_string(), 6)]));

        assert_eq!(
            normalize_vitals_text_previous(strings(&["", "a", "a", "b", "c"])),
            ["a", "b"]
        );
    }

    #[test]
    fn a_new_vitals_text_keeps_the_one_it_replaces() {
        let mut ui = UiConfig::default();
        // Nothing to keep the first time.
        replace_vitals_text(&mut ui, "a".into());
        assert_eq!(ui.vitals_text_previous, Vec::<String>::new());
        replace_vitals_text(&mut ui, "b".into());
        assert_eq!(ui.vitals_text_previous, ["a"]);
        replace_vitals_text(&mut ui, "c".into());
        assert_eq!(ui.vitals_text_previous, ["b", "a"]);
        // Two kept, newest first.
        replace_vitals_text(&mut ui, "d".into());
        assert_eq!(ui.vitals_text_previous, ["c", "b"]);
        // The same text again changes nothing.
        replace_vitals_text(&mut ui, "d".into());
        assert_eq!(ui.vitals_text_previous, ["c", "b"]);
        // Taking an earlier one back moves it out, with no repeats.
        replace_vitals_text(&mut ui, "b".into());
        assert_eq!(ui.vitals_text, "b");
        assert_eq!(ui.vitals_text_previous, ["d", "c"]);
        // Clearing keeps what you had.
        replace_vitals_text(&mut ui, String::new());
        assert_eq!(ui.vitals_text_previous, ["b", "d"]);
    }

    #[test]
    fn a_panel_font_pick_trims_and_spells_the_named_fonts_one_way() {
        assert_eq!(normalize_panel_font(String::new()), "");
        assert_eq!(normalize_panel_font("  ".into()), "");
        assert_eq!(normalize_panel_font(" Terminal ".into()), "terminal");
        assert_eq!(normalize_panel_font("TERMINAL".into()), "terminal");
        assert_eq!(normalize_panel_font(" System ".into()), "system");
        assert_eq!(
            normalize_panel_font(" \"Iosevka\", Menlo, monospace ".into()),
            "\"Iosevka\", Menlo, monospace"
        );
        // A family named system or terminal, as the Font list saves one,
        // stays a font.
        assert_eq!(
            normalize_panel_font("\"system\", Menlo, monospace".into()),
            "\"system\", Menlo, monospace"
        );
        assert_eq!(
            normalize_panel_font("\"Terminal\", monospace".into()),
            "\"Terminal\", monospace"
        );
    }

    #[test]
    fn the_panel_size_round_trips_and_stays_out_of_the_file_until_you_pick_one() {
        // 12, the default, writes nothing, so every file saved before the
        // row keeps its bytes and reads it back at the size it drew.
        let written = ProfileConfig::default().to_toml().unwrap();
        assert!(!written.contains("panel_font_size"), "{written}");
        let old = ProfileConfig::from_toml("[ui]\nfont_size = 16\n").unwrap();
        assert_eq!(old.ui.panel_font_size, TextPx::whole(12));
        let mut ui = UiConfig::default();
        assert_eq!(through_toml(&ui).panel_font_size, TextPx::whole(12));
        for pick in [
            PANEL_FONT_SIZE_TERMINAL,
            TextPx::whole(11),
            TextPx::whole(14),
            TextPx::whole(18),
        ] {
            ui.panel_font_size = pick;
            assert_eq!(through_toml(&ui).panel_font_size, pick);
        }
        let config = ProfileConfig {
            ui,
            ..ProfileConfig::default()
        };
        let text = config.to_toml().unwrap();
        assert!(text.contains("panel_font_size = 18"), "{text}");
    }

    #[test]
    fn a_panel_size_holds_to_the_terminal_sizes_and_keeps_same_as_terminal() {
        assert_eq!(
            coerce_panel_font_size(PANEL_FONT_SIZE_TERMINAL),
            TextPx::whole(0)
        );
        assert_eq!(coerce_panel_font_size(TextPx::whole(3)), TextPx::whole(6));
        assert_eq!(coerce_panel_font_size(TextPx::whole(14)), TextPx::whole(14));
        assert_eq!(coerce_panel_font_size(TextPx::whole(90)), TextPx::whole(64));
    }

    fn half(px: f64) -> TextPx {
        TextPx::from_px(px)
    }

    #[test]
    fn every_size_keeps_a_half_step_and_holds_to_six_to_sixty_four() {
        for coerce in [
            coerce_font_size,
            coerce_panel_font_size,
            coerce_input_line_size,
        ] {
            assert_eq!(coerce(half(13.5)), half(13.5));
            assert_eq!(coerce(half(6.5)), half(6.5));
            assert_eq!(coerce(half(63.5)), half(63.5));
            assert_eq!(coerce(half(5.5)), TextPx::whole(6));
            assert_eq!(coerce(half(64.5)), TextPx::whole(64));
        }
        // 0 is too small for the terminal and follows it for the others.
        assert_eq!(coerce_font_size(TextPx::whole(0)), TextPx::whole(6));
        assert_eq!(coerce_panel_font_size(half(0.5)), TextPx::whole(6));
        assert_eq!(coerce_input_line_size(half(0.5)), TextPx::whole(6));
    }

    #[test]
    fn sizes_read_whole_half_and_odd_values_leniently() {
        let read = |text: &str| ProfileConfig::from_toml(text).unwrap().ui;
        let ui = read("[ui]\nfont_size = 13\npanel_font_size = 0\ninput_line_size = 16\n");
        assert_eq!(ui.font_size, TextPx::whole(13));
        assert_eq!(ui.panel_font_size, PANEL_FONT_SIZE_TERMINAL);
        assert_eq!(ui.input_line_size, TextPx::whole(16));
        let ui = read("[ui]\nfont_size = 13.5\npanel_font_size = 11.5\ninput_line_size = 15.5\n");
        assert_eq!(ui.font_size, half(13.5));
        assert_eq!(ui.panel_font_size, half(11.5));
        assert_eq!(ui.input_line_size, half(15.5));
        // A hand edit off the half steps reads as the nearest half.
        let ui = read("[ui]\nfont_size = 13.3\npanel_font_size = 0.2\ninput_line_size = 14.8\n");
        assert_eq!(ui.font_size, half(13.5));
        assert_eq!(ui.panel_font_size, PANEL_FONT_SIZE_TERMINAL);
        assert_eq!(ui.input_line_size, TextPx::whole(15));
        // Out of range reads as written, and the save clamps it as before.
        let ui = read("[ui]\nfont_size = 90.5\n");
        assert_eq!(ui.font_size, half(90.5));
        assert_eq!(coerce_font_size(ui.font_size), TextPx::whole(64));
    }

    #[test]
    fn a_whole_size_saves_as_an_integer_and_a_half_as_a_float() {
        let text = "[ui]\nfont_size = 13\npanel_font_size = 11\ninput_line_size = 16\n";
        let config = ProfileConfig::from_toml(text).unwrap();
        let saved = config.to_toml().unwrap();
        assert!(saved.contains("font_size = 13\n"), "{saved}");
        assert!(saved.contains("panel_font_size = 11\n"), "{saved}");
        assert!(saved.contains("input_line_size = 16\n"), "{saved}");
        let again = ProfileConfig::from_toml(&saved).unwrap().to_toml().unwrap();
        assert_eq!(saved, again);
        let ui = UiConfig {
            font_size: half(13.5),
            panel_font_size: half(12.5),
            input_line_size: half(14.5),
            ..UiConfig::default()
        };
        let saved = through_text(&ui);
        assert!(saved.contains("font_size = 13.5\n"), "{saved}");
        assert!(saved.contains("panel_font_size = 12.5\n"), "{saved}");
        assert!(saved.contains("input_line_size = 14.5\n"), "{saved}");
        let back = through_toml(&ui);
        assert_eq!(back.font_size, half(13.5));
        assert_eq!(back.panel_font_size, half(12.5));
        assert_eq!(back.input_line_size, half(14.5));
    }

    #[test]
    fn vitals_density_round_trips() {
        let mut ui = UiConfig::default();
        for id in ["rows", "line"] {
            ui.vitals_density = id.into();
            assert_eq!(through_toml(&ui).vitals_density, id);
        }
    }

    #[test]
    fn vitals_values_round_trips() {
        let mut ui = UiConfig::default();
        for id in ["current-max", "current", "percent"] {
            ui.vitals_values = id.into();
            assert_eq!(through_toml(&ui).vitals_values, id);
        }
    }

    #[test]
    fn vitals_meter_round_trips() {
        let mut ui = UiConfig::default();
        for id in ["line", "bar", "none"] {
            ui.vitals_meter = id.into();
            assert_eq!(through_toml(&ui).vitals_meter, id);
        }
    }

    #[test]
    fn vitals_warn_thirds_round_trips() {
        let ui = UiConfig {
            vitals_warn_thirds: true,
            ..UiConfig::default()
        };
        assert!(through_toml(&ui).vitals_warn_thirds);
    }

    #[test]
    fn fit_game_colors_round_trips() {
        let mut ui = UiConfig::default();
        assert!(through_toml(&ui).fit_game_colors);
        ui.fit_game_colors = false;
        assert!(!through_toml(&ui).fit_game_colors);
    }

    #[test]
    fn fit_game_colors_is_written_only_while_off() {
        let mut config = ProfileConfig::default();
        let on = config.to_toml().unwrap();
        assert!(!on.contains("fit_game_colors"), "{on}");
        config.ui.fit_game_colors = false;
        let off = config.to_toml().unwrap();
        assert!(off.contains("fit_game_colors = false"), "{off}");
        // A file from before the switch reads it on.
        let old = ProfileConfig::from_toml("[ui]\ntheme = \"vellum\"\n").unwrap();
        assert!(old.ui.fit_game_colors);
    }

    #[test]
    fn color_vision_round_trips_and_is_written_only_once_picked() {
        let mut config = ProfileConfig::default();
        assert_eq!(config.ui.color_vision, "typical");
        let typical = config.to_toml().unwrap();
        assert!(!typical.contains("color_vision"), "{typical}");
        config.ui.color_vision = "deuteranopia".into();
        let picked = config.to_toml().unwrap();
        assert!(
            picked.contains("color_vision = \"deuteranopia\""),
            "{picked}"
        );
        let back = ProfileConfig::from_toml(&picked).unwrap();
        assert_eq!(back.ui.color_vision, "deuteranopia");
        // A file from before the choice reads it Typical.
        let old = ProfileConfig::from_toml("[ui]\ntheme = \"vellum\"\n").unwrap();
        assert_eq!(old.ui.color_vision, "typical");
    }

    #[test]
    fn an_unknown_color_vision_coerces_to_typical() {
        for vision in COLOR_VISIONS {
            assert_eq!(coerce_color_vision(vision.into()), vision);
        }
        assert_eq!(coerce_color_vision("deutan".into()), "typical");
        assert_eq!(coerce_color_vision(String::new()), "typical");
    }

    #[test]
    fn a_custom_theme_keeps_its_fit_and_writes_none_while_empty() {
        let mut ui = UiConfig {
            custom_themes: vec![CustomTheme {
                id: "dusk".into(),
                label: "Dusk".into(),
                ..CustomTheme::default()
            }],
            ..UiConfig::default()
        };
        let config = ProfileConfig {
            ui: ui.clone(),
            ..ProfileConfig::default()
        };
        let empty = config.to_toml().unwrap();
        assert!(!empty.contains("fitted"), "{empty}");
        ui.custom_themes[0].fitted =
            std::collections::BTreeMap::from([("red".into(), "#cb7b74".into())]);
        assert_eq!(through_toml(&ui).custom_themes, ui.custom_themes);
        // A theme saved before the fit reads with none.
        let old =
            ProfileConfig::from_toml("[[ui.custom_themes]]\nid = \"dusk\"\nlabel = \"Dusk\"\n")
                .unwrap();
        assert!(old.ui.custom_themes[0].fitted.is_empty());
    }

    #[test]
    fn readable_highlights_round_trips() {
        let mut ui = UiConfig::default();
        assert!(through_toml(&ui).readable_highlights);
        ui.readable_highlights = false;
        assert!(!through_toml(&ui).readable_highlights);
    }

    #[test]
    fn the_screen_reader_switches_round_trip_and_are_written_only_while_on() {
        let mut config = ProfileConfig::default();
        let off = config.to_toml().unwrap();
        assert!(!off.contains("screen_reader"), "{off}");
        config.ui.screen_reader = true;
        config.ui.screen_reader_background = true;
        config.ui.screen_reader_prompt = true;
        let on = config.to_toml().unwrap();
        for key in [
            "screen_reader = true",
            "screen_reader_background = true",
            "screen_reader_prompt = true",
        ] {
            assert!(on.contains(key), "{on}");
        }
        let back = through_toml(&config.ui);
        assert!(back.screen_reader && back.screen_reader_background && back.screen_reader_prompt);
        // A file from before the switches reads them off.
        let old = ProfileConfig::from_toml("[ui]\ntheme = \"vellum\"\n").unwrap();
        assert!(!old.ui.screen_reader);
        assert!(!old.ui.screen_reader_background);
        assert!(!old.ui.screen_reader_prompt);
        assert_eq!(old.ui.screen_reader_burst, 8);
    }

    #[test]
    fn the_screen_reader_burst_is_written_only_off_8_and_a_hand_edit_reads_8() {
        let mut config = ProfileConfig::default();
        assert!(!config.to_toml().unwrap().contains("screen_reader_burst"));
        for burst in SCREEN_READER_BURSTS {
            config.ui.screen_reader_burst = burst;
            assert_eq!(through_toml(&config.ui).screen_reader_burst, burst);
        }
        config.ui.screen_reader_burst = 16;
        assert!(config
            .to_toml()
            .unwrap()
            .contains("screen_reader_burst = 16"));
        for value in ["7", "\"x\"", "-4", "0", "4294967300", "16.0"] {
            let ui = ProfileConfig::from_toml(&format!("[ui]\nscreen_reader_burst = {value}\n"))
                .unwrap()
                .ui;
            assert_eq!(ui.screen_reader_burst, 8, "{value}");
        }
        assert_eq!(coerce_screen_reader_burst(32), 32);
        assert_eq!(coerce_screen_reader_burst(7), 8);
    }

    #[test]
    fn readable_highlights_is_written_only_while_off() {
        let mut config = ProfileConfig::default();
        let on = config.to_toml().unwrap();
        assert!(!on.contains("readable_highlights"), "{on}");
        config.ui.readable_highlights = false;
        let off = config.to_toml().unwrap();
        assert!(off.contains("readable_highlights = false"), "{off}");
        // A file from before the switch reads it on.
        let old = ProfileConfig::from_toml("[ui]\ntheme = \"vellum\"\n").unwrap();
        assert!(old.ui.readable_highlights);
    }

    #[test]
    fn collapse_repeats_round_trips() {
        let mut ui = UiConfig::default();
        assert!(!through_toml(&ui).collapse_repeats);
        ui.collapse_repeats = true;
        assert!(through_toml(&ui).collapse_repeats);
    }

    #[test]
    fn collapse_repeats_is_written_only_while_on() {
        let mut config = ProfileConfig::default();
        let off = config.to_toml().unwrap();
        assert!(!off.contains("collapse_repeats"), "{off}");
        config.ui.collapse_repeats = true;
        let on = config.to_toml().unwrap();
        assert!(on.contains("collapse_repeats = true"), "{on}");
        // A file from before the switch reads it off.
        let old = ProfileConfig::from_toml("[ui]\ntheme = \"vellum\"\n").unwrap();
        assert!(!old.ui.collapse_repeats);
    }

    #[test]
    fn collapse_fight_and_attack_lines_round_trip() {
        let mut ui = UiConfig::default();
        assert!(through_toml(&ui).collapse_fight_lines);
        assert!(!through_toml(&ui).collapse_attack_lines);
        ui.collapse_fight_lines = false;
        ui.collapse_attack_lines = true;
        let back = through_toml(&ui);
        assert!(!back.collapse_fight_lines);
        assert!(back.collapse_attack_lines);
    }

    #[test]
    fn collapse_fight_and_attack_lines_are_written_only_off_their_defaults() {
        let mut config = ProfileConfig::default();
        let first = config.to_toml().unwrap();
        assert!(!first.contains("collapse_fight_lines"), "{first}");
        assert!(!first.contains("collapse_attack_lines"), "{first}");
        config.ui.collapse_fight_lines = false;
        config.ui.collapse_attack_lines = true;
        let changed = config.to_toml().unwrap();
        assert!(
            changed.contains("collapse_fight_lines = false"),
            "{changed}"
        );
        assert!(
            changed.contains("collapse_attack_lines = true"),
            "{changed}"
        );
        // A file from before the two choices reads their defaults.
        let old = ProfileConfig::from_toml("[ui]\ntheme = \"vellum\"\ncollapse_repeats = true\n")
            .unwrap();
        assert!(old.ui.collapse_repeats);
        assert!(old.ui.collapse_fight_lines);
        assert!(!old.ui.collapse_attack_lines);
    }

    #[test]
    fn the_snoop_split_is_written_only_off_its_defaults() {
        let mut config = ProfileConfig::default();
        let first = config.to_toml().unwrap();
        assert!(!first.contains("snoop_"), "{first}");
        config.ui.snoop_share = 0.25;
        config.ui.snoop_folded = true;
        let changed = config.to_toml().unwrap();
        assert!(changed.contains("snoop_share = 0.25"), "{changed}");
        assert!(changed.contains("snoop_folded = true"), "{changed}");
        let back = through_toml(&config.ui);
        assert!((back.snoop_share - 0.25).abs() < f64::EPSILON);
        assert!(back.snoop_folded);
        // A file from before the split reads its defaults.
        let old = ProfileConfig::from_toml("[ui]\ntheme = \"vellum\"\n").unwrap();
        assert!((old.ui.snoop_share - DEFAULT_SNOOP_SHARE).abs() < f64::EPSILON);
        assert!(!old.ui.snoop_folded);
    }

    #[test]
    fn the_writing_card_place_is_written_only_once_you_move_it() {
        let mut config = ProfileConfig::default();
        let first = config.to_toml().unwrap();
        assert!(!first.contains("writing_card"), "{first}");
        config.ui.writing_card_left = Some(140.0);
        config.ui.writing_card_top = Some(96.0);
        config.ui.writing_card_rows = Some(14);
        config.ui.writing_card_cols = Some(96);
        config.ui.writing_card_pinned = true;
        let changed = config.to_toml().unwrap();
        assert!(changed.contains("writing_card_left = 140.0"), "{changed}");
        assert!(changed.contains("writing_card_rows = 14"), "{changed}");
        assert!(changed.contains("writing_card_cols = 96"), "{changed}");
        assert!(changed.contains("writing_card_pinned = true"), "{changed}");
        let back = through_toml(&config.ui);
        assert_eq!(back.writing_card_left, Some(140.0));
        assert_eq!(back.writing_card_top, Some(96.0));
        assert_eq!(back.writing_card_rows, Some(14));
        assert_eq!(back.writing_card_cols, Some(96));
        assert!(back.writing_card_pinned);
        // A file from before the card moved reads its defaults.
        let old = ProfileConfig::from_toml("[ui]\ntheme = \"vellum\"\n").unwrap();
        assert_eq!(old.ui.writing_card_left, None);
        assert_eq!(old.ui.writing_card_rows, None);
        assert_eq!(old.ui.writing_card_cols, None);
        assert!(!old.ui.writing_card_pinned);
    }

    #[test]
    fn a_hand_edited_writing_card_place_never_stops_a_load() {
        let ui = |text: &str| ProfileConfig::from_toml(text).unwrap().ui;
        let odd = ui("[ui]\nwriting_card_left = \"left\"\nwriting_card_rows = 2\n");
        assert_eq!(odd.writing_card_left, None);
        assert_eq!(odd.writing_card_rows, Some(WRITING_CARD_ROWS_MIN));
        let big = ui("[ui]\nwriting_card_top = 12.6\nwriting_card_rows = 9000\n");
        assert_eq!(big.writing_card_top, Some(12.6));
        assert_eq!(big.writing_card_rows, Some(WRITING_CARD_ROWS_MAX));
        assert_eq!(ui("[ui]\nwriting_card_rows = -3\n").writing_card_rows, None);
        let narrow = ui("[ui]\nwriting_card_cols = 40\n").writing_card_cols;
        assert_eq!(narrow, Some(WRITING_CARD_COLS_MIN));
        let wide = ui("[ui]\nwriting_card_cols = 9000\n").writing_card_cols;
        assert_eq!(wide, Some(WRITING_CARD_COLS_MAX));
        assert_eq!(
            ui("[ui]\nwriting_card_cols = \"wide\"\n").writing_card_cols,
            None
        );
        assert_eq!(coerce_writing_card_edge(Some(f64::NAN)), None);
    }

    #[test]
    fn the_snoop_window_place_is_written_only_once_it_moves() {
        let mut config = ProfileConfig::default();
        let first = config.to_toml().unwrap();
        assert!(!first.contains("snoop_window"), "{first}");
        config.ui.snoop_window_left = Some(-1200.0);
        config.ui.snoop_window_top = Some(64.5);
        config.ui.snoop_window_width = Some(900.0);
        config.ui.snoop_window_height = Some(520.0);
        let changed = config.to_toml().unwrap();
        assert!(changed.contains("snoop_window_left = -1200.0"), "{changed}");
        assert!(changed.contains("snoop_window_width = 900.0"), "{changed}");
        let back = through_toml(&config.ui);
        assert_eq!(back.snoop_window_left, Some(-1200.0));
        assert_eq!(back.snoop_window_top, Some(64.5));
        assert_eq!(back.snoop_window_width, Some(900.0));
        assert_eq!(back.snoop_window_height, Some(520.0));
        let old = ProfileConfig::from_toml("[ui]\ntheme = \"vellum\"\n").unwrap();
        assert_eq!(old.ui.snoop_window_left, None);
        assert_eq!(old.ui.snoop_window_width, None);
    }

    #[test]
    fn a_hand_edited_snoop_window_place_never_stops_a_load() {
        let ui = |text: &str| ProfileConfig::from_toml(text).unwrap().ui;
        let odd = ui("[ui]\nsnoop_window_left = \"left\"\nsnoop_window_width = 300\n");
        assert_eq!(odd.snoop_window_left, None);
        assert_eq!(odd.snoop_window_width, None);
        let short = ui("[ui]\nsnoop_window_height = 200.0\nsnoop_window_width = 480\n");
        assert_eq!(short.snoop_window_height, None);
        assert_eq!(short.snoop_window_width, Some(480.0));
        let far = ui("[ui]\nsnoop_window_top = 1e9\nsnoop_window_height = \"tall\"\n");
        assert_eq!(far.snoop_window_top, Some(100_000.0));
        assert_eq!(far.snoop_window_height, None);
    }

    #[test]
    fn log_sessions_logs_every_world_but_this_computer_until_you_choose() {
        let mut ui = UiConfig::default();
        assert!(logs_connection(&ui, "play.theforsakenlands.com"));
        assert!(!logs_connection(&ui, "127.0.0.1"));
        assert!(!logs_connection(&ui, "LocalHost"));
        ui.log_sessions = Some(true);
        assert!(logs_connection(&ui, "localhost"));
        ui.log_sessions = Some(false);
        assert!(!logs_connection(&ui, "play.theforsakenlands.com"));
        let mut config = ProfileConfig::default();
        assert!(!config.to_toml().unwrap().contains("log_sessions"));
        config.ui.log_sessions = Some(false);
        assert!(config.to_toml().unwrap().contains("log_sessions = false"));
        assert_eq!(through_toml(&config.ui).log_sessions, Some(false));
    }

    #[test]
    fn scrollback_size_is_written_only_off_its_default_and_holds_to_its_range() {
        let mut config = ProfileConfig::default();
        assert!(!config.to_toml().unwrap().contains("scrollback_lines"));
        config.ui.scrollback_lines = 50_000;
        assert!(config
            .to_toml()
            .unwrap()
            .contains("scrollback_lines = 50000"));
        assert_eq!(through_toml(&config.ui).scrollback_lines, 50_000);
        let read = |value: &str| {
            ProfileConfig::from_toml(&format!("[ui]\nscrollback_lines = {value}\n"))
                .unwrap()
                .ui
                .scrollback_lines
        };
        assert_eq!(read("25000"), 25_000);
        assert_eq!(read("10"), 1_000);
        assert_eq!(read("-4"), 1_000);
        assert_eq!(read("9000000000"), 100_000);
        assert_eq!(read("\"lots\""), DEFAULT_SCROLLBACK_LINES);
    }

    #[test]
    fn a_hand_edited_snoop_share_holds_to_its_range() {
        let read = |value: &str| {
            ProfileConfig::from_toml(&format!("[ui]\nsnoop_share = {value}\n"))
                .unwrap()
                .ui
                .snoop_share
        };
        for (value, want) in [
            ("0.6", 0.6),
            ("2", 0.95),
            ("0", 0.05),
            ("-1.5", 0.05),
            ("\"wide\"", DEFAULT_SNOOP_SHARE),
            ("nan", DEFAULT_SNOOP_SHARE),
        ] {
            assert!((read(value) - want).abs() < f64::EPSILON, "{value}");
        }
        assert!((coerce_snoop_share(f64::INFINITY) - DEFAULT_SNOOP_SHARE).abs() < f64::EPSILON);
    }

    #[test]
    fn vitals_hide_when_pinned_round_trips() {
        let ui = UiConfig {
            vitals_hide_when_pinned: false,
            ..UiConfig::default()
        };
        assert!(!through_toml(&ui).vitals_hide_when_pinned);
    }

    #[test]
    fn tick_count_round_trips() {
        let mut ui = UiConfig::default();
        for id in ["up", "down", "down_past_zero"] {
            ui.tick_count = id.into();
            assert_eq!(through_toml(&ui).tick_count, id);
        }
    }

    #[test]
    fn a_profile_without_the_tick_count_counts_up() {
        let ui = ProfileConfig::from_toml("[ui]\ntheme = \"nord\"\n")
            .unwrap()
            .ui;
        assert_eq!(ui.tick_count, "up");
    }

    #[test]
    fn the_tick_count_stays_with_each_character() {
        // Like the tick and time style, the count is not one of the
        // settings you can keep the same for every character, so the
        // shared file never holds it and each profile file does.
        let ui = UiConfig {
            tick_count: "down".into(),
            ..UiConfig::default()
        };
        let toml = ProfileConfig {
            ui,
            ..ProfileConfig::default()
        }
        .to_toml()
        .unwrap();
        assert!(toml.contains("tick_count = \"down\""));
    }

    #[test]
    fn the_game_time_round_trips_and_stays_out_of_the_file_at_24_hours() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.game_time, "24h");
        for clock in ["24h", "12h"] {
            ui.game_time = clock.into();
            assert_eq!(through_toml(&ui).game_time, clock);
        }
        let file = |game_time: &str| {
            ProfileConfig {
                ui: UiConfig {
                    game_time: game_time.into(),
                    ..UiConfig::default()
                },
                ..ProfileConfig::default()
            }
            .to_toml()
            .unwrap()
        };
        assert!(file("12h").contains("game_time = \"12h\""));
        // At the default the key stays out, so a file reads as it did
        // before the row.
        assert!(!file("24h").contains("game_time"));
        assert_eq!(file("24h"), ProfileConfig::default().to_toml().unwrap());
    }

    #[test]
    fn the_status_style_round_trips_and_stays_out_of_the_file_at_meters() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.status_style, "meters");
        for style in super::STATUS_STYLES {
            ui.status_style = style.into();
            assert_eq!(through_toml(&ui).status_style, style);
        }
        let file = |status_style: &str| {
            ProfileConfig {
                ui: UiConfig {
                    status_style: status_style.into(),
                    ..UiConfig::default()
                },
                ..ProfileConfig::default()
            }
            .to_toml()
            .unwrap()
        };
        assert!(file("strip").contains("status_style = \"strip\""));
        assert_eq!(file("meters"), ProfileConfig::default().to_toml().unwrap());
        assert_eq!(super::coerce_status_style("dashboard".into()), "dashboard");
        assert_eq!(super::coerce_status_style("loud".into()), "meters");
    }

    #[test]
    fn a_profile_without_the_game_time_reads_24_hours() {
        let ui = ProfileConfig::from_toml("[ui]\ntheme = \"nord\"\n")
            .unwrap()
            .ui;
        assert_eq!(ui.game_time, "24h");
    }

    #[test]
    fn an_unknown_game_time_saves_as_24_hours() {
        assert_eq!(super::coerce_game_time("12h".into()), "12h");
        assert_eq!(super::coerce_game_time("24h".into()), "24h");
        assert_eq!(super::coerce_game_time("noon".into()), "24h");
        assert_eq!(super::coerce_game_time(String::new()), "24h");
    }

    #[test]
    fn affects_style_round_trips() {
        let mut ui = UiConfig::default();
        for id in ["timers", "countdown", "chips", "chips_drain"] {
            ui.affects_style = id.into();
            assert_eq!(through_toml(&ui).affects_style, id);
        }
    }

    #[test]
    fn affects_marker_round_trips() {
        let mut ui = UiConfig::default();
        for id in ["dot", "square", "plus_minus", "none"] {
            ui.affects_marker = id.into();
            assert_eq!(through_toml(&ui).affects_marker, id);
        }
    }

    #[test]
    fn affects_tint_round_trips() {
        let ui = UiConfig {
            affects_tint: true,
            ..UiConfig::default()
        };
        assert!(through_toml(&ui).affects_tint);
    }

    #[test]
    fn a_profile_without_the_affects_display_loads_the_defaults() {
        let ui = ProfileConfig::from_toml("[ui]\ntheme = \"nord\"\n")
            .unwrap()
            .ui;
        assert_eq!(ui.affects_style, "timers");
        assert_eq!(ui.affects_marker, "dot");
        assert!(!ui.affects_tint);
    }

    #[test]
    fn the_affects_display_stays_with_each_character() {
        let ui = UiConfig {
            affects_style: "chips".into(),
            affects_marker: "square".into(),
            affects_tint: true,
            ..UiConfig::default()
        };
        let toml = ProfileConfig {
            ui,
            ..ProfileConfig::default()
        }
        .to_toml()
        .unwrap();
        assert!(toml.contains("affects_style = \"chips\""));
        assert!(toml.contains("affects_marker = \"square\""));
        assert!(toml.contains("affects_tint = true"));
    }

    #[test]
    fn affects_thresholds_round_trip() {
        let mut ui = UiConfig::default();
        for (running_out, almost_gone) in [(2, 1), (5, 2), (0, 0), (3, 3), (99, 0), (99, 99)] {
            ui.affects_running_out_hours = running_out;
            ui.affects_almost_gone_hours = almost_gone;
            let read = through_toml(&ui);
            assert_eq!(read.affects_running_out_hours, running_out);
            assert_eq!(read.affects_almost_gone_hours, almost_gone);
        }
    }

    #[test]
    fn a_profile_without_the_affects_thresholds_reads_two_and_one_and_writes_nothing_new() {
        let file = "[ui]\ntheme = \"nord\"\naffects_style = \"chips\"\n";
        let config = ProfileConfig::from_toml(file).unwrap();
        assert_eq!(config.ui.affects_running_out_hours, 2);
        assert_eq!(config.ui.affects_almost_gone_hours, 1);
        let toml = config.to_toml().unwrap();
        assert!(!toml.contains("affects_running_out_hours"));
        assert!(!toml.contains("affects_almost_gone_hours"));
    }

    #[test]
    fn the_affects_thresholds_stay_with_each_character_once_you_change_them() {
        let ui = UiConfig {
            affects_running_out_hours: 5,
            affects_almost_gone_hours: 2,
            ..UiConfig::default()
        };
        let toml = ProfileConfig {
            ui,
            ..ProfileConfig::default()
        }
        .to_toml()
        .unwrap();
        assert!(toml.contains("affects_running_out_hours = 5"));
        assert!(toml.contains("affects_almost_gone_hours = 2"));
        // Changing one writes that one alone.
        let ui = UiConfig {
            affects_running_out_hours: 4,
            ..UiConfig::default()
        };
        let toml = ProfileConfig {
            ui,
            ..ProfileConfig::default()
        }
        .to_toml()
        .unwrap();
        assert!(toml.contains("affects_running_out_hours = 4"));
        assert!(!toml.contains("affects_almost_gone_hours"));
    }

    #[test]
    fn a_hand_edited_affects_threshold_never_stops_a_profile_loading() {
        let read = |lines: &str| {
            let ui = ProfileConfig::from_toml(&format!("[ui]\n{lines}\n"))
                .unwrap()
                .ui;
            (ui.affects_running_out_hours, ui.affects_almost_gone_hours)
        };
        assert_eq!(
            read("affects_running_out_hours = 6\naffects_almost_gone_hours = 3"),
            (6, 3)
        );
        // Out of range clamps to 0 to 99.
        assert_eq!(
            read("affects_running_out_hours = 400\naffects_almost_gone_hours = -2"),
            (99, 0)
        );
        // A decimal rounds, and a string that holds a number reads as one.
        assert_eq!(
            read("affects_running_out_hours = 3.6\naffects_almost_gone_hours = \" 2 \""),
            (4, 2)
        );
        // Anything else reads as the default.
        assert_eq!(
            read("affects_running_out_hours = \"soon\"\naffects_almost_gone_hours = true"),
            (2, 1)
        );
        assert_eq!(
            read("affects_running_out_hours = [3]\naffects_almost_gone_hours = { at = 1 }"),
            (2, 1)
        );
        // Almost gone over running out reads as running out.
        assert_eq!(
            read("affects_running_out_hours = 1\naffects_almost_gone_hours = 4"),
            (1, 1)
        );
    }

    #[test]
    fn chat_colors_stay_with_each_character() {
        let mut ui = UiConfig::default();
        assert!(ui.chat_colors.is_empty());
        // A profile with no recolor writes no table.
        let plain = ProfileConfig {
            ui: ui.clone(),
            ..ProfileConfig::default()
        }
        .to_toml()
        .unwrap();
        assert!(!plain.contains("chat_colors"));

        ui.chat_colors.insert("say".into(), "brightBlue".into());
        assert_eq!(
            through_toml(&ui).chat_colors.get("say").map(String::as_str),
            Some("brightBlue")
        );
    }

    #[test]
    fn a_profile_without_the_vitals_options_loads_the_defaults() {
        let ui = ProfileConfig::from_toml("[ui]\ntheme = \"nord\"\n")
            .unwrap()
            .ui;
        assert_eq!(ui.vitals_values, "current-max");
        assert_eq!(ui.vitals_meter, "line");
        assert!(!ui.vitals_warn_thirds);
        assert!(ui.vitals_hide_when_pinned);
    }

    #[test]
    fn a_profile_without_the_vitals_density_loads_rows() {
        let ui = ProfileConfig::from_toml("[ui]\ntheme = \"nord\"\n")
            .unwrap()
            .ui;
        assert_eq!(ui.vitals_density, "rows");
    }

    #[test]
    fn a_profile_without_the_appearance_fields_loads_the_defaults() {
        let ui = ProfileConfig::from_toml("[ui]\ntheme = \"nord\"\n")
            .unwrap()
            .ui;
        assert_eq!(ui.theme, "nord");
        assert!(!ui.follow_system_appearance);
        assert_eq!(ui.light_theme, "vellum");
        assert_eq!(ui.dark_theme, "");
        assert_eq!(ui.terminal_line_height, "default");
        assert_eq!(ui.panel_font, "");
        assert_eq!(ui.panel_font_size, TextPx::whole(12));
    }
}

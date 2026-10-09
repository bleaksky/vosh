// The UI config as every window reads it and the calls that save some
// of its fields alone. uiConfigEvents.ts hears one field at a time.

import { invoke } from '@tauri-apps/api/core';
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';
import { normalizePanelFont } from '../panel/panelFont';
import { normalizePanelSize } from '../panel/panelSize';
import { toColorVision, type ColorVision } from '../theme/gameFit';
import {
  DEFAULT_LIGHT_THEME_ID,
  DEFAULT_THEME_ID,
  freeBuiltinThemeIds,
  seedDarkTheme,
} from '../theme/themes';
import {
  normalizeAffectsMarker,
  normalizeAffectsStyle,
  normalizeAffectsThresholds,
  normalizeTrackedAffects,
  type AffectsMarker,
  type AffectsStyle,
  type TrackedAffect,
} from './affects';
import { CHAT_COLORS_CHANGED, UI_CONFIG_REPLACED, WRITING_ASK_POST_CHANGED } from './events';
import { THEME_PREFS_FIELDS, type CustomTheme, type ThemeChoice } from './theme';
import {
  normalizeVitalsColors,
  normalizeVitalsDensity,
  normalizeVitalsMeter,
  normalizeVitalsOff,
  normalizeVitalsOpponent,
  normalizeVitalsOrder,
  normalizeVitalsPlace,
  normalizeVitalsStyle,
  normalizeVitalsTextPrevious,
  normalizeVitalsValues,
  VITALS_STYLES,
  type SavedVitalsStyle,
  type Vital,
  type VitalOff,
  type VitalsColors,
  type VitalsDensity,
  type VitalsMeter,
  type VitalsOpponent,
  type VitalsPlace,
  type VitalsStyle,
  type VitalsValues,
} from './uiConfigVitals';
import {
  coerceEchoMarkText,
  normalizeInputCursorStyle,
  normalizeInputEchoMark,
  normalizeInputLineBackground,
  normalizeInputLineSize,
  optionalColor,
  type InputCursorStyle,
  type InputEchoMark,
  type InputLineBackground,
} from './uiConfigInput';

export interface SystemFontEntry {
  family: string;
  monospace: boolean;
}

export async function listSystemFonts(): Promise<SystemFontEntry[]> {
  try {
    const entries = await invoke<SystemFontEntry[]>('fonts_list');
    return Array.isArray(entries) ? entries : [];
  } catch {
    return [];
  }
}

/** Terminal row spacing. Each id maps to the multiple of the glyph
 *  height that xterm takes as `lineHeight`, and the native grid follows
 *  through the cell size xterm reports. */
export const TERMINAL_LINE_HEIGHTS = {
  compact: 1.1,
  default: 1.2,
  loose: 1.35,
} as const;

export type TerminalLineHeight = keyof typeof TERMINAL_LINE_HEIGHTS;

/** Coerce an unknown line height id back to the default. */
export function normalizeTerminalLineHeight(value: unknown): TerminalLineHeight {
  return value === 'compact' || value === 'loose' ? value : 'default';
}

/** What switches the theme by itself, the Switch themes row: nothing,
 *  the OS appearance, or the game's dawn and dusk. */
export const THEME_FOLLOWS = ['off', 'system', 'game'] as const;

export type ThemeFollow = (typeof THEME_FOLLOWS)[number];

/** Coerce an unknown mode back to off. */
export function normalizeThemeFollow(value: unknown): ThemeFollow {
  return THEME_FOLLOWS.find((mode) => mode === value) ?? 'off';
}

export interface UiConfig {
  /** The theme you picked. Vosh shows it while follow_system_appearance
   *  is off. */
  theme: ThemeChoice;
  /** Show light_theme or dark_theme to match the OS appearance. */
  follow_system_appearance: boolean;
  /** The theme shown while following the system and the OS is light. */
  light_theme: string;
  /** The theme shown while following the system and the OS is dark. */
  dark_theme: string;
  /** What switches the theme by itself, one of THEME_FOLLOWS. Rust keeps
   *  follow_system_appearance true only for `system`. */
  theme_follow: ThemeFollow;
  /** The theme shown by day while following the game. Empty until you
   *  pick one. */
  day_theme: string;
  /** The theme shown by night while following the game. Empty until you
   *  pick one. */
  night_theme: string;
  auto_update: boolean;
  font_family: string;
  font_size: number;
  /** Terminal row spacing, one of TERMINAL_LINE_HEIGHTS. */
  terminal_line_height: TerminalLineHeight;
  /** The face of the panes and the status line: empty for As designed,
   *  `terminal` for the terminal font, `system` for the system font, or
   *  a font list (panelFont.ts). */
  panel_font: string;
  /** The size of the panes and the status line in px, or 0 for your
   *  terminal size (panelSize.ts). */
  panel_font_size: number;
  tracked_affects: TrackedAffect[];
  enabled_presets: string[];
  keep_last_command: boolean;
  /** Tri-state: true / false are explicit user choices; null means
   *  the default, which is on. Resolve with resolveThemeTerminalColors
   *  before use. */
  theme_terminal_colors: boolean | null;
  bright_bold: boolean;
  /** Blinking text. Tri-state: true and false are your choice, null
   *  means none, which reads as on unless your system reduces motion.
   *  Resolve with resolveBlinkText before use. */
  blink_text: boolean | null;
  /** Fit game colors. While on, play draws the game's colors in the
   *  slots the theme fits for them (themes.ts playPalette), and Settings
   *  keeps the theme as published. On unless you turn it off. */
  fit_game_colors: boolean;
  /** The color vision the game colors and the window's status colors
   *  swap for. Typical plays every theme as it ships, and another vision
   *  swaps the colors it runs together for ones it tells apart, with Fit
   *  game colors on or off (themes.ts visionFitOf). Typical unless you
   *  pick another. */
  color_vision: ColorVision;
  /** Keep highlight colors readable. While on, the session draws a true
   *  color a trigger paints text in at a lightness that reads on the
   *  theme's terminal background. On unless you turn it off. */
  readable_highlights: boolean;
  /** Read new game lines. While on, the session sends each line it shows,
   *  after gags, for a screen reader to announce. Off unless you turn it
   *  on. */
  screen_reader: boolean;
  /** Read in the background, under Read new game lines. Off unless you
   *  turn it on. */
  screen_reader_background: boolean;
  /** Read your prompt, under Read new game lines. Off unless you turn it
   *  on. */
  screen_reader_prompt: boolean;
  /** Past this many lines in one pulse, the reader hears the count and
   *  the last line. 4, 8, 16 or 32, and 8 unless you pick another. */
  screen_reader_burst: ScreenReaderBurst;
  /** Collapse repeated lines. While on, the session shows a line the
   *  game sends that reads exactly as the line before it on screen,
   *  colors included, once with a count before it. Off unless you turn
   *  it on. */
  collapse_repeats: boolean;
  /** In a fight, under Collapse repeated lines. While on, lines that
   *  come while Char.Combat names a target collapse too. On unless you
   *  turn it off. Off, attack lines show every line as well. */
  collapse_fight_lines: boolean;
  /** Attack lines, under Collapse repeated lines. While on, the hits and
   *  misses the game prints collapse too, in a fight or not, as long as
   *  collapse_fight_lines is on. Off unless you turn it on. */
  collapse_attack_lines: boolean;
  /** Custom base terminal palette: 16 CSS colors in ANSI 0-15 order,
   *  used whenever the tint toggle resolves off. Null = canonical
   *  xterm chart. */
  terminal_base_ansi: string[] | null;
  custom_themes: CustomTheme[];
  /** Override color for the split-scrollback divider. Empty/undefined
   *  means use the theme default (--split-divider, the tertiary tone). */
  split_divider_color: string | null;
  /** Override color for locally-echoed sent input. Empty/undefined
   *  means no recoloring (default terminal foreground). */
  input_echo_color: string | null;
  /** When true (default), commands sent by keyboard macros echo
   *  locally like typed commands, so under lag the keybind visibly
   *  registered before the world responds. */
  echo_macros: boolean;
  /** The mark each command you send echoes after, one of
   *  INPUT_ECHO_MARKS. `›` by default. */
  input_echo_mark: InputEchoMark;
  /** Your own mark, at most four characters, kept while another mark is
   *  picked. Rust coerces it. */
  input_echo_mark_text: string;
  /** Hex color of the mark. Null means the theme's bright black. */
  input_echo_mark_color: string | null;
  /** Draw the echo of each command faint, the mark unchanged. Off by
   *  default. */
  input_echo_dim: boolean;
  /** Start the line you type in with the same mark. On by default. */
  input_line_mark: boolean;
  /** Milliseconds to wait between lines when sending a multi-line
   *  paste. 0 = no pacing; non-zero spreads sends out so the MUD
   *  flood filter does not kick. Clamped server-side to [0, 10000]. */
  paste_line_delay_ms: number;
  /** When true, the prompt input enables the webview's native spell
   *  check, but only when the current line starts with a chat verb
   *  (say / tell / chat / gossip / ooc / clan / immtalk / reply /
   *  ' / "). Plain commands stay un-checked so MUD verbs like
   *  `kill` / `oload` / alias names do not light up red. Default
   *  off; opt-in for roleplayers. */
  spellcheck_prompt: boolean;
  /** Offer the writing card in a notice when you open the game's line
   *  editor yourself. Default on. */
  writing_offer: boolean;
  /** The writing card asks before it posts a note. Off, Post posts at
   *  once. Default on. */
  writing_ask_post: boolean;
  /** Shape of the command-line caret. Defaults to the ember block. */
  input_cursor_style: InputCursorStyle;
  /** The caret blinks. On by default, and Reduce motion still holds it
   *  steady. */
  input_caret_blink: boolean;
  /** Hex color of the caret. Null means the theme accent. */
  input_caret_color: string | null;
  /** Hex color of what you type. Null means the theme text. */
  input_line_color: string | null;
  /** The command line's background, one of INPUT_LINE_BACKGROUNDS. */
  input_line_background: InputLineBackground;
  /** Your own background color, kept while another background is
   *  picked. */
  input_line_background_color: string | null;
  /** Size in px of what you type. 0 follows your terminal size. */
  input_line_size: number;
  /** Color the command line as you type, by what Vosh knows the first
   *  word to be. Off by default. */
  input_type_colors: boolean;
  /** Hex color of a line that starts with an alias. Null means the
   *  theme's cyan. */
  input_type_alias_color: string | null;
  /** Hex color of a line that starts with a Vosh # command. Null means
   *  the theme's magenta. */
  input_type_hash_color: string | null;
  /** Hex color of a chat line. Null means the theme's yellow. */
  input_type_chat_color: string | null;
  /** Hex color of a # command Vosh does not know. Null means the theme's
   *  danger color. */
  input_type_unknown_color: string | null;
  /** How the vitals under the panel's panes lay out, one of
   *  VITALS_DENSITIES. */
  vitals_density: VitalsDensity;
  /** What each vital's value shows, one of VITALS_VALUES. */
  vitals_values: VitalsValues;
  /** The meter under each vital, one of VITALS_METERS. */
  vitals_meter: VitalsMeter;
  /** Warn before you run low, by the Group pane's thirds. */
  vitals_warn_thirds: boolean;
  /** Hide the panel's vitals while your prompt is pinned, so the panes
   *  take their room. On unless you turn it off. */
  vitals_hide_when_pinned: boolean;
  /** The style you picked from the gallery, or null for Rows and One
   *  line, which vitals_density holds. */
  vitals_style: SavedVitalsStyle | null;
  /** Where your vitals show, one of VITALS_PLACES. */
  vitals_place: VitalsPlace;
  /** The order every style draws your vitals in. */
  vitals_order: Vital[];
  /** The vitals you turned off, and `opponent` for your opponent's row. */
  vitals_off: VitalOff[];
  /** Where your opponent's row sits, one of VITALS_OPPONENT_PLACES. */
  vitals_opponent: VitalsOpponent;
  /** Each vital's ANSI slot. A vital left out takes Default. */
  vitals_colors: VitalsColors;
  /** The text the Text style writes your vitals with. */
  vitals_text: string;
  /** At most two earlier texts, newest first. Saving vitals_text puts
   *  the one it replaces here. */
  vitals_text_previous: string[];
  /** Show each hit, on every style with a fill. */
  vitals_hit: boolean;
  /** The style your 0.7 vitals grew into, which the gallery marks
   *  Yours in 0.7, or null when they give no clue. Read only, nothing
   *  saves it. */
  vitals_legacy_style: VitalsStyle | null;
  /** Your 0.7 template in today's codes while it was on, which the
   *  vitals text card offers among its Presets, or null. Read only. */
  vitals_legacy_text: string | null;
  /** How the status line draws the tick, the game time, and the moons.
   *  The value alone, a caption before each value, or an icon before
   *  each. The moons are icons already, so only Caption changes them. */
  chip_style: ChipStyle;
  /** Which way the status line tick counts, one of TICK_COUNTS. */
  tick_count: TickCount;
  /** The clock the status line reads the game time on, one of
   *  GAME_TIMES. */
  game_time: GameTime;
  /** The Affects pane's layout, one of AFFECTS_STYLES. */
  affects_style: AffectsStyle;
  /** The mark beside each tracked affect, one of AFFECTS_MARKERS. */
  affects_marker: AffectsMarker;
  /** Tint what to recast in the timers and countdown layouts. */
  affects_tint: boolean;
  /** At or under this many hours an affect you track turns yellow and
   *  counts as running out. Whole hours from 0 to 99. */
  affects_running_out_hours: number;
  /** At or under this many hours an affect's hours turn bold red. Whole
   *  hours from 0 to 99, never over affects_running_out_hours. */
  affects_almost_gone_hours: number;
  /** The share of the terminal column the snoop split takes, from 0.05
   *  to 0.95. You set it by dragging the line under the split. */
  snoop_share: number;
  /** The snoop split folded to its strip. */
  snoop_folded: boolean;
  /** Log sessions. Null until you choose, which logs every world but
   *  this computer. */
  log_sessions: boolean | null;
  /** Scrollback size: the lines each terminal keeps above the screen,
   *  and the scrollback file for the next launch, 1,000 to 100,000. */
  scrollback_lines: number;
  /** Where you dragged the writing card, its left and top edges in CSS
   *  pixels from the window's corner. Null until you move it, which
   *  keeps the place over the terminal the card works out itself. */
  writing_card_left: number | null;
  writing_card_top: number | null;
  /** The rows the writing card's text box shows, 6 to 500. Null until
   *  you drag its foot, which lets the box grow with the text. */
  writing_card_rows: number | null;
  /** The columns of text the writing card's box shows, 75 to 500. Null
   *  until you drag its corner, which keeps 80. */
  writing_card_cols: number | null;
  /** The writing card opens in its pane in the panel. */
  writing_card_pinned: boolean;
}

export type ChipStyle = 'value_only' | 'caption_value' | 'icon_value';

/** Read a stored or broadcast chip style. Anything unknown is the
 *  value alone, the default. */
export function normalizeChipStyle(raw: unknown): ChipStyle {
  return raw === 'caption_value' || raw === 'icon_value' ? raw : 'value_only';
}

/** The ways the status line tick counts. `up`, the default, shows the
 *  seconds since the last tick. `down` shows the seconds left until the
 *  next and waits at 0 while the game runs late. `down_past_zero`
 *  counts on below zero until the tick lands. */
export const TICK_COUNTS = ['up', 'down', 'down_past_zero'] as const;
export type TickCount = (typeof TICK_COUNTS)[number];

/** Read a stored or broadcast tick count. Anything unknown counts up. */
export function normalizeTickCount(raw: unknown): TickCount {
  return TICK_COUNTS.find((count) => count === raw) ?? 'up';
}

/** The clocks the status line reads the game time on. `24h`, the
 *  default, reads like 18:00, and `12h` like 6:00 PM. */
export const GAME_TIMES = ['24h', '12h'] as const;
export type GameTime = (typeof GAME_TIMES)[number];

/** Read a stored or broadcast clock. Anything unknown is the 24 hour
 *  clock. */
export function normalizeGameTime(raw: unknown): GameTime {
  return GAME_TIMES.find((clock) => clock === raw) ?? '24h';
}

// Dedupe the mount-time burst: MainWindow, Input, and the tracked affects
// store all call getUiConfig on first render. Sharing one
// in-flight promise turns that into a single IPC round-trip. The cache
// clears once resolved, so a later call (after a config change) still
// re-fetches fresh — no staleness. A read that names its profile, as
// Settings does, goes alone.
let uiConfigInFlight: Promise<UiConfig> | null = null;
export function getUiConfig(profile?: string | null): Promise<UiConfig> {
  if (profile != null) return fetchUiConfig(profile);
  if (!uiConfigInFlight) {
    uiConfigInFlight = fetchUiConfig().finally(() => {
      uiConfigInFlight = null;
    });
  }
  return uiConfigInFlight;
}

/** The raw shape `ui_get_config` returns, before normalizeUiConfig
 *  fills gaps and coerces unknown values. */
export interface RawUiConfig {
  theme: string;
  follow_system_appearance?: boolean;
  light_theme?: string;
  dark_theme?: string;
  theme_follow?: string;
  day_theme?: string;
  night_theme?: string;
  auto_update: boolean;
  font_family: string;
  font_size: number;
  terminal_line_height?: string;
  panel_font?: string;
  panel_font_size?: number;
  tracked_affects: unknown[];
  enabled_presets: string[];
  keep_last_command?: boolean;
  theme_terminal_colors?: boolean;
  bright_bold?: boolean;
  blink_text?: boolean | null;
  fit_game_colors?: boolean;
  color_vision?: string;
  readable_highlights?: boolean;
  screen_reader?: boolean;
  screen_reader_background?: boolean;
  screen_reader_prompt?: boolean;
  screen_reader_burst?: number;
  collapse_repeats?: boolean;
  collapse_fight_lines?: boolean;
  collapse_attack_lines?: boolean;
  terminal_base_ansi?: unknown;
  custom_themes?: CustomTheme[];
  split_divider_color?: string | null;
  input_echo_color?: string | null;
  echo_macros?: boolean;
  input_echo_mark?: string;
  input_echo_mark_text?: string;
  input_echo_mark_color?: string | null;
  input_echo_dim?: boolean;
  input_line_mark?: boolean;
  paste_line_delay_ms?: number;
  spellcheck_prompt?: boolean;
  writing_offer?: boolean;
  writing_ask_post?: boolean;
  input_cursor_style?: string;
  input_caret_blink?: boolean;
  input_caret_color?: string | null;
  input_line_color?: string | null;
  input_line_background?: string;
  input_line_background_color?: string | null;
  input_line_size?: number;
  input_type_colors?: boolean;
  input_type_alias_color?: string | null;
  input_type_hash_color?: string | null;
  input_type_chat_color?: string | null;
  input_type_unknown_color?: string | null;
  vitals_density?: string;
  vitals_values?: string;
  vitals_meter?: string;
  vitals_warn_thirds?: boolean;
  vitals_hide_when_pinned?: boolean;
  vitals_style?: string | null;
  vitals_place?: string;
  vitals_order?: unknown;
  vitals_off?: unknown;
  vitals_opponent?: string;
  vitals_colors?: unknown;
  vitals_text?: string;
  vitals_text_previous?: unknown;
  vitals_hit?: boolean;
  vitals_legacy_style?: string | null;
  vitals_legacy_text?: unknown;
  chip_style?: string;
  tick_count?: string;
  game_time?: string;
  affects_style?: string;
  affects_marker?: string;
  affects_tint?: boolean;
  affects_running_out_hours?: number;
  affects_almost_gone_hours?: number;
  snoop_share?: number;
  snoop_folded?: boolean;
  log_sessions?: boolean | null;
  scrollback_lines?: number;
  writing_card_left?: number | null;
  writing_card_top?: number | null;
  writing_card_rows?: number | null;
  writing_card_cols?: number | null;
  writing_card_pinned?: boolean;
}

/** A profile's UI config, the selected session's profile's when it
 *  names none. */
export async function fetchUiConfig(profile?: string | null): Promise<UiConfig> {
  const raw = await invoke<RawUiConfig>('ui_get_config', { profile });
  const freed = freeBuiltinThemeIds(raw);
  const config = normalizeUiConfig(freed);
  // A custom theme moved off a built-in id is saved under its new id at
  // once. Later reads then find no collision, so a choice that names
  // the built-in keeps meaning the built-in. Every window moves it the
  // same way, so the save sends no events.
  if (freed !== raw) {
    const moved: UiFields = {
      custom_themes: config.custom_themes,
      ...Object.fromEntries(THEME_PREFS_FIELDS.map((field) => [field, config[field]])),
    };
    try {
      await setUiFields(moved, profile);
    } catch (e) {
      console.error('[themes] saving the moved custom themes failed', e);
    }
  }
  return config;
}

/** Fill gaps and coerce unknown values in a raw config so every window
 *  reads the same shape. A gap takes the value Rust sends for a profile
 *  that sets nothing, and fixtures/ui-config/defaults.json holds both
 *  sides to those values. Custom themes leave the ids built-in themes
 *  have (freeBuiltinThemeIds). */
export function normalizeUiConfig(raw: RawUiConfig): UiConfig {
  const cfg = freeBuiltinThemeIds(raw);
  const theme =
    typeof cfg.theme === 'string' && cfg.theme.length > 0 ? cfg.theme : DEFAULT_THEME_ID;
  const customThemes = Array.isArray(cfg.custom_themes) ? cfg.custom_themes : [];
  const thresholds = normalizeAffectsThresholds(
    cfg.affects_running_out_hours,
    cfg.affects_almost_gone_hours,
  );
  return {
    theme,
    follow_system_appearance: cfg.follow_system_appearance === true,
    light_theme:
      typeof cfg.light_theme === 'string' && cfg.light_theme.length > 0
        ? cfg.light_theme
        : DEFAULT_LIGHT_THEME_ID,
    dark_theme:
      typeof cfg.dark_theme === 'string' && cfg.dark_theme.length > 0
        ? cfg.dark_theme
        : seedDarkTheme(theme, customThemes),
    theme_follow: normalizeThemeFollow(cfg.theme_follow),
    day_theme: typeof cfg.day_theme === 'string' ? cfg.day_theme : '',
    night_theme: typeof cfg.night_theme === 'string' ? cfg.night_theme : '',
    auto_update: cfg.auto_update,
    font_family: cfg.font_family,
    font_size: cfg.font_size,
    terminal_line_height: normalizeTerminalLineHeight(cfg.terminal_line_height),
    panel_font: normalizePanelFont(cfg.panel_font),
    panel_font_size: normalizePanelSize(cfg.panel_font_size),
    tracked_affects: Array.isArray(cfg.tracked_affects)
      ? normalizeTrackedAffects(cfg.tracked_affects)
      : [],
    enabled_presets: Array.isArray(cfg.enabled_presets) ? cfg.enabled_presets : [],
    keep_last_command: Boolean(cfg.keep_last_command),
    theme_terminal_colors:
      typeof cfg.theme_terminal_colors === 'boolean' ? cfg.theme_terminal_colors : null,
    bright_bold: Boolean(cfg.bright_bold),
    blink_text: typeof cfg.blink_text === 'boolean' ? cfg.blink_text : null,
    fit_game_colors: cfg.fit_game_colors !== false,
    color_vision: toColorVision(cfg.color_vision),
    readable_highlights: cfg.readable_highlights !== false,
    screen_reader: cfg.screen_reader === true,
    screen_reader_background: cfg.screen_reader_background === true,
    screen_reader_prompt: cfg.screen_reader_prompt === true,
    screen_reader_burst: normalizeScreenReaderBurst(cfg.screen_reader_burst),
    collapse_repeats: cfg.collapse_repeats === true,
    collapse_fight_lines: cfg.collapse_fight_lines !== false,
    collapse_attack_lines: cfg.collapse_attack_lines === true,
    terminal_base_ansi:
      Array.isArray(cfg.terminal_base_ansi) &&
      cfg.terminal_base_ansi.length === 16 &&
      cfg.terminal_base_ansi.every((c) => typeof c === 'string' && c.length > 0)
        ? (cfg.terminal_base_ansi as string[])
        : null,
    custom_themes: customThemes,
    split_divider_color:
      typeof cfg.split_divider_color === 'string' && cfg.split_divider_color.length > 0
        ? cfg.split_divider_color
        : null,
    input_echo_color:
      typeof cfg.input_echo_color === 'string' && cfg.input_echo_color.length > 0
        ? cfg.input_echo_color
        : null,
    echo_macros: cfg.echo_macros !== false,
    input_echo_mark: normalizeInputEchoMark(cfg.input_echo_mark),
    input_echo_mark_text:
      typeof cfg.input_echo_mark_text === 'string'
        ? coerceEchoMarkText(cfg.input_echo_mark_text)
        : '',
    input_echo_mark_color:
      typeof cfg.input_echo_mark_color === 'string' && cfg.input_echo_mark_color.length > 0
        ? cfg.input_echo_mark_color
        : null,
    input_echo_dim: cfg.input_echo_dim === true,
    input_line_mark: cfg.input_line_mark !== false,
    paste_line_delay_ms:
      typeof cfg.paste_line_delay_ms === 'number' && cfg.paste_line_delay_ms >= 0
        ? Math.min(10_000, Math.floor(cfg.paste_line_delay_ms))
        : 500,
    spellcheck_prompt: Boolean(cfg.spellcheck_prompt),
    writing_offer: cfg.writing_offer !== false,
    writing_ask_post: cfg.writing_ask_post !== false,
    input_cursor_style: normalizeInputCursorStyle(cfg.input_cursor_style),
    input_caret_blink: cfg.input_caret_blink !== false,
    input_caret_color: optionalColor(cfg.input_caret_color),
    input_line_color: optionalColor(cfg.input_line_color),
    input_line_background: normalizeInputLineBackground(cfg.input_line_background),
    input_line_background_color: optionalColor(cfg.input_line_background_color),
    input_line_size: normalizeInputLineSize(cfg.input_line_size),
    input_type_colors: cfg.input_type_colors === true,
    input_type_alias_color: optionalColor(cfg.input_type_alias_color),
    input_type_hash_color: optionalColor(cfg.input_type_hash_color),
    input_type_chat_color: optionalColor(cfg.input_type_chat_color),
    input_type_unknown_color: optionalColor(cfg.input_type_unknown_color),
    vitals_density: normalizeVitalsDensity(cfg.vitals_density),
    vitals_values: normalizeVitalsValues(cfg.vitals_values),
    vitals_meter: normalizeVitalsMeter(cfg.vitals_meter),
    vitals_warn_thirds: cfg.vitals_warn_thirds === true,
    vitals_hide_when_pinned: cfg.vitals_hide_when_pinned !== false,
    vitals_style: normalizeVitalsStyle(cfg.vitals_style),
    vitals_place: normalizeVitalsPlace(cfg.vitals_place),
    vitals_order: normalizeVitalsOrder(cfg.vitals_order),
    vitals_off: normalizeVitalsOff(cfg.vitals_off),
    vitals_opponent: normalizeVitalsOpponent(cfg.vitals_opponent),
    vitals_colors: normalizeVitalsColors(cfg.vitals_colors),
    vitals_text: typeof cfg.vitals_text === 'string' ? cfg.vitals_text : '',
    vitals_text_previous: normalizeVitalsTextPrevious(cfg.vitals_text_previous),
    vitals_hit: cfg.vitals_hit === true,
    vitals_legacy_style: VITALS_STYLES.find((style) => style === cfg.vitals_legacy_style) ?? null,
    vitals_legacy_text:
      typeof cfg.vitals_legacy_text === 'string' && cfg.vitals_legacy_text !== ''
        ? cfg.vitals_legacy_text
        : null,
    chip_style: normalizeChipStyle(cfg.chip_style),
    tick_count: normalizeTickCount(cfg.tick_count),
    game_time: normalizeGameTime(cfg.game_time),
    affects_style: normalizeAffectsStyle(cfg.affects_style),
    affects_marker: normalizeAffectsMarker(cfg.affects_marker),
    affects_tint: cfg.affects_tint === true,
    affects_running_out_hours: thresholds.running_out,
    affects_almost_gone_hours: thresholds.almost_gone,
    snoop_share: normalizeSnoopShare(cfg.snoop_share),
    snoop_folded: cfg.snoop_folded === true,
    log_sessions: typeof cfg.log_sessions === 'boolean' ? cfg.log_sessions : null,
    scrollback_lines: normalizeScrollbackLines(cfg.scrollback_lines),
    writing_card_left: normalizeWritingCardEdge(cfg.writing_card_left),
    writing_card_top: normalizeWritingCardEdge(cfg.writing_card_top),
    writing_card_rows: normalizeWritingCardRows(cfg.writing_card_rows),
    writing_card_cols: normalizeWritingCardCols(cfg.writing_card_cols),
    writing_card_pinned: cfg.writing_card_pinned === true,
  };
}

/** Read a stored writing card edge. Anything but a finite number is
 *  null, the place the card works out itself. */
export function normalizeWritingCardEdge(raw: unknown): number | null {
  return typeof raw === 'number' && Number.isFinite(raw)
    ? Math.min(100_000, Math.max(-100_000, raw))
    : null;
}

/** Read the writing card's stored rows, held to 6 to 500 as Rust holds
 *  them. Anything but a number is null, a box that grows with the text. */
export function normalizeWritingCardRows(raw: unknown): number | null {
  return typeof raw === 'number' && Number.isFinite(raw)
    ? Math.min(500, Math.max(6, Math.round(raw)))
    : null;
}

/** Read the writing card's stored columns, a whole number from 75 to
 *  500, or null for the box's own 80. */
export function normalizeWritingCardCols(raw: unknown): number | null {
  return typeof raw === 'number' && Number.isFinite(raw)
    ? Math.min(500, Math.max(75, Math.round(raw)))
    : null;
}

/** The bursts you can pick for the screen reader. */
export const SCREEN_READER_BURSTS = [4, 8, 16, 32] as const;
export type ScreenReaderBurst = (typeof SCREEN_READER_BURSTS)[number];

/** The burst the screen reader takes until you pick another. */
export const DEFAULT_SCREEN_READER_BURST: ScreenReaderBurst = 8;

/** Read a stored burst as Rust reads it. Anything but 4, 8, 16 or 32 is
 *  8. */
export function normalizeScreenReaderBurst(raw: unknown): ScreenReaderBurst {
  return SCREEN_READER_BURSTS.find((burst) => burst === raw) ?? DEFAULT_SCREEN_READER_BURST;
}

/** The lines a terminal keeps until you pick another Scrollback size. */
export const DEFAULT_SCROLLBACK_LINES = 10_000;

/** Read a stored scrollback size, held to 1,000 to 100,000 as Rust holds
 *  it. Anything that is not a number is the default. */
export function normalizeScrollbackLines(raw: unknown): number {
  return typeof raw === 'number' && Number.isFinite(raw)
    ? Math.min(100_000, Math.max(1_000, Math.round(raw)))
    : DEFAULT_SCROLLBACK_LINES;
}

/** The share of the terminal column a snoop split takes until you drag
 *  it. */
export const DEFAULT_SNOOP_SHARE = 0.4;

/** Read a stored snoop share, held to 0.05 to 0.95 as Rust holds it.
 *  Anything that is not a finite number is the default. */
export function normalizeSnoopShare(raw: unknown): number {
  return typeof raw === 'number' && Number.isFinite(raw)
    ? Math.min(0.95, Math.max(0.05, raw))
    : DEFAULT_SNOOP_SHARE;
}

/** Hear that the backend replaced the live profile's whole UI config,
 *  on a profile switch, a #profile load or reset, or an import. It
 *  comes after the events that carry the panes, the tracked affects,
 *  the tick settings, and the chip style. */
export async function subscribeUiConfigReplaced(cb: () => void): Promise<UnlistenFn> {
  return listen<unknown>(UI_CONFIG_REPLACED, () => cb());
}

/** Save the theme choice alone, from a window that keeps no copy of
 *  the other fields, like the palette. Pass the slots too when the pick
 *  came from pickTheme, which fills the light or dark one while the
 *  theme follows the system and the day or night one while it follows
 *  the game. */
export async function setUiTheme(
  theme: string,
  slots?: Pick<UiConfig, 'light_theme' | 'dark_theme' | 'day_theme' | 'night_theme'>,
): Promise<void> {
  await invoke('ui_set_theme', {
    theme,
    lightTheme: slots?.light_theme ?? null,
    darkTheme: slots?.dark_theme ?? null,
    dayTheme: slots?.day_theme ?? null,
    nightTheme: slots?.night_theme ?? null,
  });
}

/** The fields setUiFields can save. The tracked affects save through
 *  trackedAffectsSet, and the 0.7 style and text are read only. */
export type UiFields = Partial<
  Omit<UiConfig, 'tracked_affects' | 'vitals_legacy_style' | 'vitals_legacy_text'>
>;

/** Save only the fields `fields` names, so two windows that each change
 *  a field keep both changes. It writes the profile `profile` names while
 *  a session plays it, or else the selected session's, and tells no
 *  other window. */
export async function setUiFields(fields: UiFields, profile?: string | null): Promise<void> {
  await invoke('ui_set_fields', {
    fields: Object.entries(fields)
      .filter(([, value]) => value !== undefined)
      .map(([field, value]) => ({ field, value })),
    profile: profile ?? null,
  });
}

/** The chat pane's channel colors for the live profile, as the backend
 *  holds them. chatColors.ts normalizeChatColors reads the table. */
export async function getChatColorsTable(): Promise<unknown> {
  return invoke<unknown>('ui_get_chat_colors');
}

/** Recolor one chat channel from the pane menu, or give it back its
 *  default with null. The backend saves it alone, so no window writes a
 *  stale copy of the rest of the config, and tells every window. */
export async function setChatColor(channel: string, color: string | null): Promise<void> {
  await invoke('ui_set_chat_color', { channel, color });
}

/** Give every chat channel its default color again. */
export async function resetChatColors(): Promise<void> {
  await invoke('ui_reset_chat_colors');
}

/** Hear the chat colors after a pick, a reset, or a profile switch. */
export async function subscribeChatColorsChanged(
  cb: (table: unknown) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(CHAT_COLORS_CHANGED, (event) => {
    cb(event.payload);
  });
}

/** Turn Ask before you post off from the writing card, and tell every
 *  window, since setUiFields tells none. */
export async function stopAskingToPost(): Promise<void> {
  await setUiFields({ writing_ask_post: false });
  await emit(WRITING_ASK_POST_CHANGED, false);
}

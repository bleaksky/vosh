// The UI config as every window reads it, the events that carry each
// field, and the calls that save some of its fields alone.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
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
import {
  BASE_ANSI_CHANGED,
  BLINK_TEXT_CHANGED,
  BRIGHT_BOLD_CHANGED,
  CHAT_COLORS_CHANGED,
  CHIP_STYLE_CHANGED,
  COLOR_VISION_CHANGED,
  ECHO_MACROS_CHANGED,
  FIT_GAME_COLORS_CHANGED,
  FONT_CHANGED,
  GAME_TIME_CHANGED,
  INPUT_CURSOR_STYLE_CHANGED,
  INPUT_ECHO_CARET_CHANGED,
  INPUT_ECHO_COLOR_CHANGED,
  KEEP_LAST_CHANGED,
  PASTE_LINE_DELAY_CHANGED,
  READABLE_HIGHLIGHTS_CHANGED,
  SPELLCHECK_PROMPT_CHANGED,
  SPLIT_DIVIDER_CHANGED,
  TERMINAL_LINE_HEIGHT_CHANGED,
  THEME_TERMINAL_COLORS_CHANGED,
  TICK_COUNT_CHANGED,
  UI_CONFIG_REPLACED,
  VITALS_OPTIONS_CHANGED,
} from './events';
import { THEME_PREFS_FIELDS, type CustomTheme, type ThemeChoice } from './theme';

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

/** Caret shapes the command line can paint. Each one renders inside the
 *  same anchor box as the default block, so switching shapes never
 *  reflows the input row. */
export const INPUT_CURSOR_STYLES = [
  'block',
  'block_outline',
  'half_block',
  'underline',
  'underline_thick',
  'pipe',
  'pipe_thick',
] as const;

export type InputCursorStyle = (typeof INPUT_CURSOR_STYLES)[number];

/** Coerce an unknown caret shape back to the default block. */
export function normalizeInputCursorStyle(value: unknown): InputCursorStyle {
  return INPUT_CURSOR_STYLES.includes(value as InputCursorStyle)
    ? (value as InputCursorStyle)
    : 'block';
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

/** How the vitals under the panel's panes lay out. `rows` gives each
 *  vital its own row. `line` sets Health, Mana, and Moves side by side
 *  on one row. */
export const VITALS_DENSITIES = ['rows', 'line'] as const;

export type VitalsDensity = (typeof VITALS_DENSITIES)[number];

/** Coerce an unknown vitals density back to rows. */
export function normalizeVitalsDensity(value: unknown): VitalsDensity {
  return value === 'line' ? 'line' : 'rows';
}

/** What each vital's value shows. `current-max` reads `186 / 1020`,
 *  `current` reads `186`, and `percent` reads `18%`. */
export const VITALS_VALUES = ['current-max', 'current', 'percent'] as const;

export type VitalsValues = (typeof VITALS_VALUES)[number];

/** Coerce an unknown value form back to current and max. */
export function normalizeVitalsValues(value: unknown): VitalsValues {
  return value === 'current' || value === 'percent' ? value : 'current-max';
}

/** The meter under each vital. `line` is the 2 px meter, `bar` the
 *  4 px one, and `none` drops the meters and tightens the rows. */
export const VITALS_METERS = ['line', 'bar', 'none'] as const;

export type VitalsMeter = (typeof VITALS_METERS)[number];

/** Coerce an unknown meter back to the line. */
export function normalizeVitalsMeter(value: unknown): VitalsMeter {
  return value === 'bar' || value === 'none' ? value : 'line';
}

/** The six styles of the gallery, in its order. Rows and One line are
 *  the two densities, and vitals_style holds the other four. */
export const VITALS_STYLES = ['rows', 'line', 'ledger', 'gauges', 'pips', 'text'] as const;

export type VitalsStyle = (typeof VITALS_STYLES)[number];

/** The styles vitals_style saves. Rows and One line stay in
 *  vitals_density, so a build without styles still reads your look. */
const SAVED_VITALS_STYLES = ['ledger', 'gauges', 'pips', 'text'] as const;

export type SavedVitalsStyle = (typeof SAVED_VITALS_STYLES)[number];

/** Coerce an unknown saved style back to null, which draws the
 *  density. */
export function normalizeVitalsStyle(value: unknown): SavedVitalsStyle | null {
  return SAVED_VITALS_STYLES.find((style) => style === value) ?? null;
}

/** The style your vitals draw in, the one you picked or else your
 *  density, so a player who never picks sees today's look (Vitals
 *  Styles Q12). */
export function shownStyle(config: Pick<UiConfig, 'vitals_style' | 'vitals_density'>): VitalsStyle {
  return config.vitals_style ?? config.vitals_density;
}

/** Where your vitals show, under the panel's panes or in the status
 *  line. */
export const VITALS_PLACES = ['panel', 'status'] as const;

export type VitalsPlace = (typeof VITALS_PLACES)[number];

/** Coerce an unknown place back to the panel. */
export function normalizeVitalsPlace(value: unknown): VitalsPlace {
  return value === 'status' ? 'status' : 'panel';
}

/** Your vitals in today's order. */
export const VITALS = ['hp', 'mana', 'move'] as const;

export type Vital = (typeof VITALS)[number];

/** Keep each known vital once, in the order given, and add any missing
 *  after them in today's order. */
export function normalizeVitalsOrder(value: unknown): Vital[] {
  const given = Array.isArray(value) ? (value as unknown[]) : [];
  const kept: Vital[] = [];
  for (const name of [...given, ...VITALS]) {
    const vital = VITALS.find((v) => v === name);
    if (vital && !kept.includes(vital)) kept.push(vital);
  }
  return kept;
}

/** What vitals_off can hold, each vital and your opponent's row. */
export const VITALS_OFF = [...VITALS, 'opponent'] as const;

export type VitalOff = (typeof VITALS_OFF)[number];

/** Keep each known name once, in the order of VITALS_OFF. */
export function normalizeVitalsOff(value: unknown): VitalOff[] {
  const given = Array.isArray(value) ? (value as unknown[]) : [];
  return VITALS_OFF.filter((name) => given.includes(name));
}

/** Where your opponent's row sits in a fight. */
export const VITALS_OPPONENT_PLACES = ['top', 'bottom'] as const;

export type VitalsOpponent = (typeof VITALS_OPPONENT_PLACES)[number];

/** Coerce an unknown opponent place back to the top. */
export function normalizeVitalsOpponent(value: unknown): VitalsOpponent {
  return value === 'bottom' ? 'bottom' : 'top';
}

/** Each vital's color as an ANSI slot from 0 to 15. A vital left out
 *  takes Default. */
export type VitalsColors = Partial<Record<Vital, number>>;

/** Keep the colors of known vitals that name a slot from 0 to 15. */
export function normalizeVitalsColors(value: unknown): VitalsColors {
  const given = value && typeof value === 'object' ? (value as Record<string, unknown>) : {};
  const colors: VitalsColors = {};
  for (const vital of VITALS) {
    const slot = given[vital];
    if (typeof slot === 'number' && Number.isInteger(slot) && slot >= 0 && slot <= 15) {
      colors[vital] = slot;
    }
  }
  return colors;
}

/** How many earlier vitals texts Vosh keeps. */
const VITALS_TEXT_PREVIOUS = 2;

/** Drop blank and repeated texts and keep the newest two. */
export function normalizeVitalsTextPrevious(value: unknown): string[] {
  const given = Array.isArray(value) ? (value as unknown[]) : [];
  const kept: string[] = [];
  for (const text of given) {
    if (kept.length === VITALS_TEXT_PREVIOUS) break;
    if (typeof text === 'string' && text.length > 0 && !kept.includes(text)) kept.push(text);
  }
  return kept;
}

/** Every vitals choice the footer, the status line and the menu draw
 *  from, as one event payload, so a pick moves them together. The
 *  status line reads the values and the warning, never the meter. Your
 *  vitals text comes on its own event, rendered (src/ipc/vitals.ts). */
export interface VitalsOptions {
  /** The style shown, your pick or else your density. */
  style: VitalsStyle;
  place: VitalsPlace;
  order: Vital[];
  off: VitalOff[];
  opponent: VitalsOpponent;
  colors: VitalsColors;
  values: VitalsValues;
  meter: VitalsMeter;
  /** Warn under two thirds and turn danger under one third, like the
   *  Group pane. Off keeps danger under 20 percent. */
  warn_thirds: boolean;
  /** Hide the panel's vitals while your prompt is pinned. */
  hide_when_pinned: boolean;
}

/** What Reset to default under Customize vitals puts back. Every vital
 *  on in today's order with Default colors, your opponent on top,
 *  Current and max, Line, and the warning off. Your style, where your
 *  vitals show and Hide vitals while your prompt is pinned stay as they
 *  are. */
export const DEFAULT_VITALS_CUSTOM: Pick<
  UiConfig,
  | 'vitals_order'
  | 'vitals_off'
  | 'vitals_colors'
  | 'vitals_opponent'
  | 'vitals_values'
  | 'vitals_meter'
  | 'vitals_warn_thirds'
> = {
  vitals_order: [...VITALS],
  vitals_off: [],
  vitals_colors: {},
  vitals_opponent: 'top',
  vitals_values: 'current-max',
  vitals_meter: 'line',
  vitals_warn_thirds: false,
};

/** The fields of the config VitalsOptions reads. */
type VitalsFields =
  | 'vitals_style'
  | 'vitals_density'
  | 'vitals_place'
  | 'vitals_order'
  | 'vitals_off'
  | 'vitals_opponent'
  | 'vitals_colors'
  | 'vitals_values'
  | 'vitals_meter'
  | 'vitals_warn_thirds'
  | 'vitals_hide_when_pinned';

/** The vitals options a config holds. */
export function vitalsOptionsOf(config: Pick<UiConfig, VitalsFields>): VitalsOptions {
  return {
    style: shownStyle(config),
    place: config.vitals_place,
    order: config.vitals_order,
    off: config.vitals_off,
    opponent: config.vitals_opponent,
    colors: config.vitals_colors,
    values: config.vitals_values,
    meter: config.vitals_meter,
    warn_thirds: config.vitals_warn_thirds,
    hide_when_pinned: config.vitals_hide_when_pinned,
  };
}

export const DEFAULT_VITALS_OPTIONS: VitalsOptions = vitalsOptionsOf({
  ...DEFAULT_VITALS_CUSTOM,
  vitals_style: null,
  vitals_density: 'rows',
  vitals_place: 'panel',
  vitals_hide_when_pinned: true,
});

/** Read vitals options off the bus, filling anything missing or
 *  unknown with the defaults. */
export function normalizeVitalsOptions(raw: unknown): VitalsOptions {
  const o = raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : {};
  return {
    style: VITALS_STYLES.find((style) => style === o.style) ?? 'rows',
    place: normalizeVitalsPlace(o.place),
    order: normalizeVitalsOrder(o.order),
    off: normalizeVitalsOff(o.off),
    opponent: normalizeVitalsOpponent(o.opponent),
    colors: normalizeVitalsColors(o.colors),
    values: normalizeVitalsValues(o.values),
    meter: normalizeVitalsMeter(o.meter),
    warn_thirds: o.warn_thirds === true,
    hide_when_pinned: o.hide_when_pinned !== false,
  };
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
  /** When true (default), each command you send echoes after a grey
   *  `›` and a space, Mark your commands under Input in Settings. */
  input_echo_caret: boolean;
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
  /** Shape of the command-line caret. Defaults to the ember block. */
  input_cursor_style: InputCursorStyle;
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
  collapse_repeats?: boolean;
  collapse_fight_lines?: boolean;
  collapse_attack_lines?: boolean;
  terminal_base_ansi?: unknown;
  custom_themes?: CustomTheme[];
  split_divider_color?: string | null;
  input_echo_color?: string | null;
  echo_macros?: boolean;
  input_echo_caret?: boolean;
  paste_line_delay_ms?: number;
  spellcheck_prompt?: boolean;
  input_cursor_style?: string;
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
  chip_style?: string;
  tick_count?: string;
  game_time?: string;
  affects_style?: string;
  affects_marker?: string;
  affects_tint?: boolean;
  affects_running_out_hours?: number;
  affects_almost_gone_hours?: number;
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
    input_echo_caret: cfg.input_echo_caret !== false,
    paste_line_delay_ms:
      typeof cfg.paste_line_delay_ms === 'number' && cfg.paste_line_delay_ms >= 0
        ? Math.min(10_000, Math.floor(cfg.paste_line_delay_ms))
        : 500,
    spellcheck_prompt: Boolean(cfg.spellcheck_prompt),
    input_cursor_style: normalizeInputCursorStyle(cfg.input_cursor_style),
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
    chip_style: normalizeChipStyle(cfg.chip_style),
    tick_count: normalizeTickCount(cfg.tick_count),
    game_time: normalizeGameTime(cfg.game_time),
    affects_style: normalizeAffectsStyle(cfg.affects_style),
    affects_marker: normalizeAffectsMarker(cfg.affects_marker),
    affects_tint: cfg.affects_tint === true,
    affects_running_out_hours: thresholds.running_out,
    affects_almost_gone_hours: thresholds.almost_gone,
  };
}

/** What FONT_CHANGED carries. */
export interface FontChange {
  family: string;
  size: number;
  /** The Panel font as saved (panelFont.ts). */
  panel: string;
  /** The panel size as saved, 0 for the terminal size (panelSize.ts). */
  panelSize: number;
}

/** Hear that the backend replaced the live profile's whole UI config,
 *  on a profile switch, a #profile load or reset, or an import. It
 *  comes after the events that carry the panes, the tracked affects,
 *  the tick settings, and the chip style. */
export async function subscribeUiConfigReplaced(cb: () => void): Promise<UnlistenFn> {
  return listen<unknown>(UI_CONFIG_REPLACED, () => cb());
}

/** Save the theme choice alone, from a window that keeps no copy of
 *  the other fields, like the palette. Pass the light and dark pair too
 *  when the pick came from pickTheme, which fills one of them while
 *  follow system appearance is on. */
export async function setUiTheme(
  theme: string,
  pair?: { light_theme: string; dark_theme: string },
): Promise<void> {
  await invoke('ui_set_theme', {
    theme,
    lightTheme: pair?.light_theme ?? null,
    darkTheme: pair?.dark_theme ?? null,
  });
}

/** The fields setUiFields can save. The tracked affects save through
 *  trackedAffectsSet. */
export type UiFields = Partial<Omit<UiConfig, 'tracked_affects'>>;

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

/** Hear a new chip style saved from Settings. The Settings save emits
 *  it to every window, so the main window's status line follows at
 *  once. */
export async function subscribeChipStyleChanged(
  cb: (value: ChipStyle) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(CHIP_STYLE_CHANGED, (event) => {
    cb(normalizeChipStyle(event.payload));
  });
}

/** Hear a new tick count saved from Settings. The Settings save emits
 *  it to every window, so the main window's status line follows at
 *  once. */
export async function subscribeTickCountChanged(
  cb: (value: TickCount) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(TICK_COUNT_CHANGED, (event) => {
    cb(normalizeTickCount(event.payload));
  });
}

/** Hear a new game time clock saved from Settings, or the one a
 *  profile switch brings. The Settings save emits it to every window,
 *  so the main window's status line follows at once. */
export async function subscribeGameTimeChanged(cb: (value: GameTime) => void): Promise<UnlistenFn> {
  return listen<unknown>(GAME_TIME_CHANGED, (event) => {
    cb(normalizeGameTime(event.payload));
  });
}

/** Hear a new terminal line height saved from Settings. */
export async function subscribeTerminalLineHeightChanged(
  cb: (value: TerminalLineHeight) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(TERMINAL_LINE_HEIGHT_CHANGED, (event) => {
    cb(normalizeTerminalLineHeight(event.payload));
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

/** Hear new vitals options, your style and every choice under Layout,
 *  Vitals, saved from Settings or the menu, or the ones a profile
 *  switch brings. */
export async function subscribeVitalsOptionsChanged(
  cb: (value: VitalsOptions) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(VITALS_OPTIONS_CHANGED, (event) => {
    cb(normalizeVitalsOptions(event.payload));
  });
}

/** Hear the Blinking text choice change, null for none. */
export async function subscribeBlinkTextChanged(
  cb: (value: boolean | null) => void,
): Promise<UnlistenFn> {
  return listen<boolean | null>(BLINK_TEXT_CHANGED, (event) => {
    cb(typeof event.payload === 'boolean' ? event.payload : null);
  });
}

export async function subscribeBrightBoldChanged(
  cb: (value: boolean) => void,
): Promise<UnlistenFn> {
  return listen<boolean>(BRIGHT_BOLD_CHANGED, (event) => {
    cb(Boolean(event.payload));
  });
}

/** Hear Fit game colors change, saved in Settings or brought by
 *  another profile. */
export async function subscribeFitGameColorsChanged(
  cb: (value: boolean) => void,
): Promise<UnlistenFn> {
  return listen<boolean>(FIT_GAME_COLORS_CHANGED, (event) => {
    cb(event.payload !== false);
  });
}

/** Hear the color vision change, saved in Settings or brought by
 *  another profile. */
export async function subscribeColorVisionChanged(
  cb: (value: ColorVision) => void,
): Promise<UnlistenFn> {
  return listen<string>(COLOR_VISION_CHANGED, (event) => {
    cb(toColorVision(event.payload));
  });
}

/** Hear Keep highlight colors readable change, saved in Settings or
 *  brought by another profile. */
export async function subscribeReadableHighlightsChanged(
  cb: (value: boolean) => void,
): Promise<UnlistenFn> {
  return listen<boolean>(READABLE_HIGHLIGHTS_CHANGED, (event) => {
    cb(event.payload !== false);
  });
}

export async function subscribeSplitDividerChanged(
  cb: (color: string | null) => void,
): Promise<UnlistenFn> {
  return listen<string | null>(SPLIT_DIVIDER_CHANGED, (event) => {
    cb(typeof event.payload === 'string' && event.payload.length > 0 ? event.payload : null);
  });
}

export async function subscribeBaseAnsiChanged(
  cb: (colors: string[] | null) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(BASE_ANSI_CHANGED, (event) => {
    const p = event.payload;
    cb(
      Array.isArray(p) && p.length === 16 && p.every((c) => typeof c === 'string')
        ? (p as string[])
        : null,
    );
  });
}

/** Hear the terminal font and size and the panel font and size saved in
 *  Settings. */
export function subscribeFontChanged(cb: (change: FontChange) => void): Promise<UnlistenFn> {
  return listen<FontChange>(FONT_CHANGED, (event) => cb(event.payload));
}

/** Hear Keep last command change. */
export function subscribeKeepLastChanged(cb: (on: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>(KEEP_LAST_CHANGED, (event) => cb(event.payload));
}

/** Hear Use the theme's colors for MUD text change. */
export function subscribeThemeTerminalColorsChanged(
  cb: (on: boolean) => void,
): Promise<UnlistenFn> {
  return listen<boolean>(THEME_TERMINAL_COLORS_CHANGED, (event) => cb(event.payload));
}

/** Hear Sent command color change, null for the default. */
export function subscribeInputEchoColorChanged(
  cb: (color: string | null) => void,
): Promise<UnlistenFn> {
  return listen<string | null>(INPUT_ECHO_COLOR_CHANGED, (event) => cb(event.payload));
}

/** Hear Show the commands your macros send change. */
export function subscribeEchoMacrosChanged(cb: (on: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>(ECHO_MACROS_CHANGED, (event) => cb(event.payload));
}

/** Hear Mark your commands change. */
export function subscribeInputEchoCaretChanged(cb: (on: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>(INPUT_ECHO_CARET_CHANGED, (event) => cb(event.payload));
}

/** Hear Wait between pasted lines change, in ms. */
export function subscribePasteLineDelayChanged(cb: (ms: number) => void): Promise<UnlistenFn> {
  return listen<number>(PASTE_LINE_DELAY_CHANGED, (event) => cb(event.payload));
}

/** Hear Check spelling when you chat change. */
export function subscribeSpellcheckPromptChanged(cb: (on: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>(SPELLCHECK_PROMPT_CHANGED, (event) => cb(event.payload));
}

/** Hear Caret shape change. */
export function subscribeInputCursorStyleChanged(cb: (style: string) => void): Promise<UnlistenFn> {
  return listen<string>(INPUT_CURSOR_STYLE_CHANGED, (event) => cb(event.payload));
}

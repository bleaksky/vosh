// Saving the whole UI config, telling every other window what changed,
// and following a config the backend replaced.

import { invoke } from '@tauri-apps/api/core';
import { emit, type UnlistenFn } from '@tauri-apps/api/event';
import { noteThemeEcho, resolveActiveTheme, systemPrefersDark, themePrefsOf } from '../theme/theme';
import { resolveThemeTerminalColors } from '../theme/themes';
import { affectsDisplayOf, normalizeAffectsDisplay, type AffectsDisplay } from './affects';
import {
  AFFECTS_DISPLAY_CHANGED,
  BASE_ANSI_CHANGED,
  BLINK_TEXT_CHANGED,
  BRIGHT_BOLD_CHANGED,
  CHIP_STYLE_CHANGED,
  CUSTOM_THEMES_CHANGED,
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
  THEME_CHANGED,
  THEME_PREFS_CHANGED,
  THEME_TERMINAL_COLORS_CHANGED,
  TICK_COUNT_CHANGED,
  TRACKED_AFFECTS_CHANGED,
  VITALS_DENSITY_CHANGED,
  VITALS_OPTIONS_CHANGED,
} from './events';
import {
  fetchUiConfig,
  subscribeUiConfigReplaced,
  uiConfigPayload,
  vitalsOptionsOf,
  type FontChange,
  type UiConfig,
} from './uiConfig';

function fontChangeOf(config: UiConfig): FontChange {
  return {
    family: config.font_family,
    size: config.font_size,
    panel: config.panel_font,
    panelSize: config.panel_font_size,
  };
}

async function emitChanged<T>(
  event: string,
  value: T,
  prevValue: T | undefined,
  equal: (a: T, b: T) => boolean = Object.is,
): Promise<void> {
  if (prevValue !== undefined && equal(value, prevValue)) return;
  try {
    await emit(event, value);
  } catch {
    // Tauri bus unavailable (dev preview, window not yet ready); the
    // same-window in-process state is already updated by the caller.
  }
}

// Cheap deep-equality for the structured fields. Each one, like
// custom_themes, is a small bounded object, so JSON round-trip is
// faster (and more predictable) than a hand-rolled walker.
function deepEqual<T>(a: T, b: T): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

// Fire the cross-window `vosh://*-changed` event fan-out for every
// field in `config` that differs from `before`, or for every field when
// there is no `before`. The Settings save passes what its fields held
// before your edits, so a font size nudge sends only the font, and the
// main window's followReplacedUiConfig sends every field after a
// replace, so every window's per-field subscriber sees the new value.
export async function broadcastUiConfigChanges(config: UiConfig, before?: UiConfig): Promise<void> {
  // Custom themes go out first so any other window's theme registry is
  // current by the time `theme-changed` points at a custom theme id.
  await emitChanged(CUSTOM_THEMES_CHANGED, config.custom_themes, before?.custom_themes, deepEqual);
  // The four theme fields go out whole so Settings and the palette keep
  // current copies. theme-changed carries the id they resolve to, which
  // is `theme` unless follow is on. Both come back to this window too,
  // so note them first as its own.
  const prefs = themePrefsOf(config);
  const prevPrefs = before ? themePrefsOf(before) : undefined;
  if (!prevPrefs || !deepEqual(prefs, prevPrefs)) noteThemeEcho(prefs);
  await emitChanged(THEME_PREFS_CHANGED, prefs, prevPrefs, deepEqual);
  const systemDark = systemPrefersDark();
  const shown = resolveActiveTheme(config, systemDark);
  const prevShown = before ? resolveActiveTheme(before, systemDark) : undefined;
  if (shown !== prevShown) noteThemeEcho(shown);
  await emitChanged(THEME_CHANGED, shown, prevShown);
  await emitChanged<FontChange>(
    FONT_CHANGED,
    fontChangeOf(config),
    before ? fontChangeOf(before) : undefined,
    (a, b) =>
      a.family === b.family &&
      a.size === b.size &&
      a.panel === b.panel &&
      a.panelSize === b.panelSize,
  );
  await emitChanged(
    TERMINAL_LINE_HEIGHT_CHANGED,
    config.terminal_line_height,
    before?.terminal_line_height,
  );
  await emitChanged(KEEP_LAST_CHANGED, config.keep_last_command, before?.keep_last_command);
  // The event carries the RESOLVED boolean so listeners never see the
  // tri-state. Resolving both sides of the diff means a theme switch
  // with the setting on auto also fires this event when the effective
  // value flips.
  await emitChanged(
    THEME_TERMINAL_COLORS_CHANGED,
    resolveThemeTerminalColors(config.theme_terminal_colors),
    before ? resolveThemeTerminalColors(before.theme_terminal_colors) : undefined,
  );
  await emitChanged(BRIGHT_BOLD_CHANGED, config.bright_bold, before?.bright_bold);
  // Your choice as you made it. Each window reads its own system's
  // reduce motion setting to resolve none.
  await emitChanged(BLINK_TEXT_CHANGED, config.blink_text, before?.blink_text);
  await emitChanged(FIT_GAME_COLORS_CHANGED, config.fit_game_colors, before?.fit_game_colors);
  await emitChanged(
    READABLE_HIGHLIGHTS_CHANGED,
    config.readable_highlights,
    before?.readable_highlights,
  );
  await emitChanged(
    BASE_ANSI_CHANGED,
    config.terminal_base_ansi,
    before?.terminal_base_ansi,
    deepEqual,
  );
  await emitChanged(SPLIT_DIVIDER_CHANGED, config.split_divider_color, before?.split_divider_color);
  await emitChanged(INPUT_ECHO_COLOR_CHANGED, config.input_echo_color, before?.input_echo_color);
  await emitChanged(ECHO_MACROS_CHANGED, config.echo_macros, before?.echo_macros);
  await emitChanged(INPUT_ECHO_CARET_CHANGED, config.input_echo_caret, before?.input_echo_caret);
  await emitChanged(
    PASTE_LINE_DELAY_CHANGED,
    config.paste_line_delay_ms,
    before?.paste_line_delay_ms,
  );
  await emitChanged(SPELLCHECK_PROMPT_CHANGED, config.spellcheck_prompt, before?.spellcheck_prompt);
  await emitChanged(
    INPUT_CURSOR_STYLE_CHANGED,
    config.input_cursor_style,
    before?.input_cursor_style,
  );
  await emitChanged(VITALS_DENSITY_CHANGED, config.vitals_density, before?.vitals_density);
  await emitChanged(
    VITALS_OPTIONS_CHANGED,
    vitalsOptionsOf(config),
    before ? vitalsOptionsOf(before) : undefined,
    deepEqual,
  );
  await emitChanged(CHIP_STYLE_CHANGED, config.chip_style, before?.chip_style);
  await emitChanged(TICK_COUNT_CHANGED, config.tick_count, before?.tick_count);
  await emitChanged(GAME_TIME_CHANGED, config.game_time, before?.game_time);
  const display = affectsDisplayOf(config);
  const prevDisplay = before ? affectsDisplayOf(before) : undefined;
  if (!prevDisplay || !deepEqual(display, prevDisplay)) noteAffectsDisplayEcho(display);
  await emitChanged(AFFECTS_DISPLAY_CHANGED, display, prevDisplay, deepEqual);
  await emitChanged(
    TRACKED_AFFECTS_CHANGED,
    config.tracked_affects,
    before?.tracked_affects,
    deepEqual,
  );
}

/** The reads followReplacedUiConfig runs in this window. A save the
 *  backend turned away runs them too. */
const replaceFollowers = new Set<() => void>();

/** How followReplacedUiConfig hands a window the replaced config. */
export interface FollowReplacedOptions {
  /** After `apply`, send every field to every window. The main window
   *  does this, since Input, the vitals, the prompt, and the other
   *  per-field listeners follow those events, and the backend sends
   *  only a few of them. */
  broadcast?: boolean;
}

/** Keep a window on the live profile's UI config. The backend replaces
 *  it on a profile switch, a #profile load or reset, or an import.
 *  Every replace reads the config again, never sharing a read that
 *  started before it, and hands it to `apply`. Only the newest read
 *  applies. A whole config save the backend turned away reads it again
 *  here too. The main window passes `broadcast` and sends every field
 *  to the other windows. */
export async function followReplacedUiConfig(
  apply: (config: UiConfig) => void,
  onError: (error: unknown) => void,
  options: FollowReplacedOptions = {},
): Promise<UnlistenFn> {
  let latestRead = 0;
  const reread = () => {
    const mine = ++latestRead;
    fetchUiConfig()
      .then(async (config) => {
        if (mine !== latestRead) return;
        apply(config);
        if (options.broadcast) await broadcastUiConfigChanges(config);
      })
      .catch(onError);
  };
  replaceFollowers.add(reread);
  const unlisten = await subscribeUiConfigReplaced(reread);
  return () => {
    replaceFollowers.delete(reread);
    unlisten();
  };
}

// Every window hears its own affects display broadcast too. One this
// window sent in the last second is its own echo, and adopting it could
// undo a newer pick made while that save was in flight.
const AFFECTS_DISPLAY_ECHO_MS = 1000;
let affectsDisplayEchoes: { key: string; at: number }[] = [];

function noteAffectsDisplayEcho(display: AffectsDisplay): void {
  const now = Date.now();
  affectsDisplayEchoes = affectsDisplayEchoes.filter((e) => now - e.at < AFFECTS_DISPLAY_ECHO_MS);
  affectsDisplayEchoes.push({ key: JSON.stringify(normalizeAffectsDisplay(display)), at: now });
}

/** Whether an affects display heard on the bus is this window's own
 *  broadcast coming back. */
export function isOwnAffectsDisplayEcho(display: AffectsDisplay): boolean {
  const now = Date.now();
  const key = JSON.stringify(normalizeAffectsDisplay(display));
  return affectsDisplayEchoes.some((e) => e.key === key && now - e.at < AFFECTS_DISPLAY_ECHO_MS);
}

/** Save the whole config and send every field to every window. Resolves
 *  false when the backend turned the save away, because it replaced
 *  the live config after this copy was read (a profile switch, a
 *  #profile load or reset, or an import). Nothing is sent then, and
 *  this window reads the config again. */
export async function setUiConfig(config: UiConfig): Promise<boolean> {
  const applied = await invoke<boolean | undefined>('ui_set_config', {
    config: uiConfigPayload(config),
  });
  if (applied === false) {
    // The old profile's values stay off the new one. Take the new copy,
    // even if the replace notice never reached this window.
    for (const reread of replaceFollowers) reread();
    return false;
  }
  await broadcastUiConfigChanges(config);
  return true;
}

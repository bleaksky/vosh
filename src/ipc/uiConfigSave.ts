// Saving the whole UI config, telling every other window what changed,
// and following a config the backend replaced.

import { invoke } from '@tauri-apps/api/core';
import { emit, type UnlistenFn } from '@tauri-apps/api/event';
import {
  noteThemeEcho,
  resolveActiveTheme,
  systemPrefersDark,
  THEME_PREFS_EVENT,
  themePrefsOf,
  type ThemePrefs,
} from '../lib/theme';
import {
  AFFECTS_DISPLAY_EVENT,
  affectsDisplayFields,
  affectsDisplayOf,
  normalizeAffectsDisplay,
  TRACKED_AFFECTS_EVENT,
  type AffectsDisplay,
} from './affects';
import {
  BLINK_TEXT_EVENT,
  fetchUiConfig,
  FIT_GAME_COLORS_EVENT,
  FONT_CHANGED_EVENT,
  READABLE_HIGHLIGHTS_EVENT,
  resolveThemeTerminalColors,
  subscribeUiConfigReplaced,
  TERMINAL_LINE_HEIGHT_EVENT,
  uiConfigPayload,
  VITALS_DENSITY_EVENT,
  VITALS_OPTIONS_EVENT,
  vitalsOptionsOf,
  type FontChange,
  type UiConfig,
} from './uiConfig';

// Phase 7 perf fix: snapshot of the last UiConfig we successfully
// wrote, used to skip cross-window emits for fields the user did
// NOT change. Previously every setUiConfig call fanned out 10-11
// emits regardless of which slider moved — a font-size nudge fired
// custom-themes (500B-5KB), vitals, moons, etc. The diff cache cuts
// that to "emit only for fields that actually moved" without
// changing wire-protocol or subscriber surface.
let lastSentConfig: UiConfig | null = null;

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
// field in `config` that differs from the last-broadcast snapshot.
// `setUiConfig` calls this after writing to disk, and the main window's
// followReplacedUiConfig sends every field through it after a replace,
// so every window's per-field subscriber sees the updated value.
export async function broadcastUiConfigChanges(config: UiConfig): Promise<void> {
  const prev = lastSentConfig;
  lastSentConfig = config;
  await emitChanged(
    'vosh://custom-themes-changed',
    config.custom_themes,
    prev?.custom_themes,
    deepEqual,
  );
  // The four theme fields go out whole so Settings and the palette keep
  // current copies. theme-changed carries the id they resolve to, which
  // is `theme` unless follow is on. Both come back to this window too,
  // so note them first as its own.
  const prefs = themePrefsOf(config);
  const prevPrefs = prev ? themePrefsOf(prev) : undefined;
  if (!prevPrefs || !deepEqual(prefs, prevPrefs)) noteThemeEcho(prefs);
  await emitChanged(THEME_PREFS_EVENT, prefs, prevPrefs, deepEqual);
  const systemDark = systemPrefersDark();
  const shown = resolveActiveTheme(config, systemDark);
  const prevShown = prev ? resolveActiveTheme(prev, systemDark) : undefined;
  if (shown !== prevShown) noteThemeEcho(shown);
  await emitChanged('vosh://theme-changed', shown, prevShown);
  await emitChanged<FontChange>(
    FONT_CHANGED_EVENT,
    fontChangeOf(config),
    prev ? fontChangeOf(prev) : undefined,
    (a, b) =>
      a.family === b.family &&
      a.size === b.size &&
      a.panel === b.panel &&
      a.panelSize === b.panelSize,
  );
  await emitChanged(
    TERMINAL_LINE_HEIGHT_EVENT,
    config.terminal_line_height,
    prev?.terminal_line_height,
  );
  await emitChanged('vosh://keep-last-changed', config.keep_last_command, prev?.keep_last_command);
  // The event carries the RESOLVED boolean so listeners never see the
  // tri-state. Resolving both sides of the diff means a theme switch
  // with the setting on auto also fires this event when the effective
  // value flips.
  await emitChanged(
    'vosh://theme-terminal-colors-changed',
    resolveThemeTerminalColors(config.theme, config.theme_terminal_colors),
    prev ? resolveThemeTerminalColors(prev.theme, prev.theme_terminal_colors) : undefined,
  );
  await emitChanged('vosh://bright-bold-changed', config.bright_bold, prev?.bright_bold);
  // Your choice as you made it. Each window reads its own system's
  // reduce motion setting to resolve none.
  await emitChanged(BLINK_TEXT_EVENT, config.blink_text, prev?.blink_text);
  await emitChanged(FIT_GAME_COLORS_EVENT, config.fit_game_colors, prev?.fit_game_colors);
  await emitChanged(
    READABLE_HIGHLIGHTS_EVENT,
    config.readable_highlights,
    prev?.readable_highlights,
  );
  await emitChanged(
    'vosh://base-ansi-changed',
    config.terminal_base_ansi,
    prev?.terminal_base_ansi,
    deepEqual,
  );
  await emitChanged(
    'vosh://split-divider-changed',
    config.split_divider_color,
    prev?.split_divider_color,
  );
  await emitChanged(
    'vosh://input-echo-color-changed',
    config.input_echo_color,
    prev?.input_echo_color,
  );
  await emitChanged('vosh://echo-macros-changed', config.echo_macros, prev?.echo_macros);
  await emitChanged(
    'vosh://input-echo-caret-changed',
    config.input_echo_caret,
    prev?.input_echo_caret,
  );
  await emitChanged(
    'vosh://paste-line-delay-changed',
    config.paste_line_delay_ms,
    prev?.paste_line_delay_ms,
  );
  await emitChanged(
    'vosh://spellcheck-prompt-changed',
    config.spellcheck_prompt,
    prev?.spellcheck_prompt,
  );
  await emitChanged(
    'vosh://input-cursor-style-changed',
    config.input_cursor_style,
    prev?.input_cursor_style,
  );
  await emitChanged(VITALS_DENSITY_EVENT, config.vitals_density, prev?.vitals_density);
  await emitChanged(
    VITALS_OPTIONS_EVENT,
    vitalsOptionsOf(config),
    prev ? vitalsOptionsOf(prev) : undefined,
    deepEqual,
  );
  await emitChanged('vosh://chip-style-changed', config.chip_style, prev?.chip_style);
  await emitChanged('vosh://tick-count-changed', config.tick_count, prev?.tick_count);
  await emitChanged('vosh://game-time-changed', config.game_time, prev?.game_time);
  const display = affectsDisplayOf(config);
  const prevDisplay = prev ? affectsDisplayOf(prev) : undefined;
  if (!prevDisplay || !deepEqual(display, prevDisplay)) noteAffectsDisplayEcho(display);
  await emitChanged(AFFECTS_DISPLAY_EVENT, display, prevDisplay, deepEqual);
  await emitChanged(
    TRACKED_AFFECTS_EVENT,
    config.tracked_affects,
    prev?.tracked_affects,
    deepEqual,
  );
}

// Adopt `config` as this window's last-broadcast snapshot without
// emitting anything. A window that reads its config again after a
// replace calls this, since the main window sends every window the new
// values, so its next save diffs against the new profile rather than
// the old.
export function primeUiConfigBroadcast(config: UiConfig): void {
  lastSentConfig = config;
}

/** The reads followReplacedUiConfig runs in this window. A save the
 *  backend turned away runs them too. */
const replaceFollowers = new Set<() => void>();

/** How followReplacedUiConfig hands a window the replaced config. */
export interface FollowReplacedOptions {
  /** After `apply`, send every field to every window. The main window
   *  does this, since Input, the vitals, the prompt, and the other
   *  per-field listeners follow those events, and the backend sends
   *  only a few of them. A diff against this window's last broadcast
   *  could skip a field, since saves from Settings never move it. */
  broadcast?: boolean;
}

/** Keep a window on the live profile's UI config. The backend replaces
 *  it on a profile switch, a #profile load or reset, or an import.
 *  Every replace reads the config again, never sharing a read that
 *  started before it, and hands it to `apply`. Only the newest read
 *  applies.
 *
 *  A window that saves the whole UiConfig (Settings) sends every field
 *  with each save, so a copy from before the replace would write the
 *  old profile's values back. The backend turns such a save away, and
 *  that reads the config again here too. The read becomes this
 *  window's last broadcast, so the next save sends only what you
 *  change, since the main window has sent the rest. The main window
 *  passes `broadcast` and sends them. */
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
        if (!options.broadcast) {
          primeUiConfigBroadcast(config);
          apply(config);
          return;
        }
        apply(config);
        lastSentConfig = null;
        await broadcastUiConfigChanges(config);
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

// Adopt a theme another window already applied and broadcast, and
// nothing else, so this window's next save does not emit it again
// while its own unsaved edits still diff as changes.
export function primeUiConfigTheme(theme: string): void {
  if (lastSentConfig) lastSentConfig = { ...lastSentConfig, theme };
}

// Adopt the four theme fields another window saved (the palette's
// Choose theme), the same way.
export function primeUiConfigThemePrefs(prefs: ThemePrefs): void {
  if (lastSentConfig) lastSentConfig = { ...lastSentConfig, ...themePrefsOf(prefs) };
}

// Adopt an affects display the pane menu saved, the same way, so this
// window's next save does not send it on as its own change.
export function primeUiConfigAffectsDisplay(display: AffectsDisplay): void {
  if (!lastSentConfig) return;
  lastSentConfig = { ...lastSentConfig, ...affectsDisplayFields(display) };
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

/** Save the whole config and tell every window what changed. Resolves
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
  // Theme + custom-themes go out first so any other window's theme
  // registry is current by the time `theme-changed` points at a
  // custom theme id. `broadcastUiConfigChanges` preserves that
  // ordering.
  await broadcastUiConfigChanges(config);
  return true;
}

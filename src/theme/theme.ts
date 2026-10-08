// Theme runtime. Applies the active theme to the document root via the
// derived chrome token vars (theme/chrome) plus `data-theme` and
// `data-appearance` attributes, and broadcasts a window event so the
// Terminal can refresh its xterm palette. Each paint of the theme the
// saved fields resolve to also lands in the paint cache (theme/themePaint),
// so the next window to open paints it before React renders.
//
// The active theme comes from seven saved fields. Switch themes Off
// shows `theme`. With the system, follow_system_appearance on, it is
// `dark_theme` or `light_theme`, whichever matches the OS appearance,
// and a prefers-color-scheme listener swaps them when the OS flips.
// While macOS Increase contrast is on, prefers-contrast more, it shows
// the high contrast pair instead, the dark one or the light one as the
// OS appearance says (Q24).
// With the game, theme_follow `game`, it is `day_theme` or
// `night_theme`, whichever matches the selected session's daylight
// (stores/session/daylightStore), and the store swaps them at the
// game's dawn and dusk. Before the game says, it is the side last shown,
// which the paint cache keeps through a drop and a relaunch (Alerts
// Q15).
//
// Every window paints the chrome for your color vision, from UiConfig
// color_vision, which swaps the status colors (theme/chrome). A window
// takes it from its config through applyThemePrefs, and from a change
// through setColorVision, which paints the theme again.

import type { UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { Daylight } from '../ipc/tick';
import {
  emitThemeChanged,
  emitThemePrefsChanged,
  subscribeThemeChanged,
  subscribeThemePrefsChanged,
  type THEME_PREFS_FIELDS,
} from '../ipc/theme';
import { getUiConfig, THEME_FOLLOWS, type ThemeFollow, type UiConfig } from '../ipc/uiConfig';
import { windowBackdropSet } from '../ipc/windows';
import { getDaylight, subscribeDaylight } from '../stores/session/daylightStore';
import { createStore } from '../stores/store';
import { tokensToCssVars, type Appearance } from './chrome';
import { parseHex, toHex, toRgba } from './color';
import { toColorVision, type ColorVision } from './gameFit';
import {
  bootPaintPhase,
  bootPaintSide,
  osPrefersDark,
  osPrefersMoreContrast,
  pageStorage,
  paintRoot,
  samePaintSide,
  writeThemePaint,
  type ThemePaint,
  type ThemePaintSide,
} from './themePaint';
import {
  customToAppTheme,
  DEFAULT_THEME_ID,
  findTheme,
  onCustomThemesChanged,
  RETIRED_THEMES,
  setCustomThemes,
  themeTokens,
  type AppTheme,
} from './themes';

const DARK_QUERY = '(prefers-color-scheme: dark)';
const CONTRAST_QUERY = '(prefers-contrast: more)';

let cleanupContrastListener: (() => void) | null = null;
let cleanupSchemeListener: (() => void) | null = null;
let cleanupDaylightListener: (() => void) | null = null;
// The theme on screen. Before the first apply that is the one the
// startup paint put there (src/prepaint.ts runs before this module), so
// the terminal mounts on its palette instead of the default theme's.
// A custom theme is not loaded yet at mount, and the terminal falls
// back to the default theme until the config arrives.
let currentThemeId: string = bootPaintSide()?.id ?? DEFAULT_THEME_ID;
let windowAppearance: Appearance | 'system' | null = null;
let themePrefs: ThemePrefs | null = null;
let followingSystem = false;
let broadcastFlips = false;
/** The game's day or night this window last knew, from the startup
 *  paint until the selected session's game says. */
let lastDaylight: Daylight | null = bootPaintPhase();
/** What this window last painted on the root. */
let lastPaint: ThemePaintSide | null = null;
/** The theme id lastPaint shows, or null for a fallback still waiting
 *  on the catalog. */
let lastStandsFor: string | null = null;
/** The theme id applyTheme was last asked for. A catalog refresh that
 *  answers after a later pick leaves the later pick on screen. */
let lastChoice: string | null = null;
/** The color vision this window paints for. */
const visionStore = createStore<ColorVision>('typical');

/** The saved fields that decide which theme Vosh shows. */
export type ThemePrefs = Pick<UiConfig, (typeof THEME_PREFS_FIELDS)[number]>;

/** Just the seven theme fields, so a whole UiConfig can be passed in. */
export function themePrefsOf(ui: ThemePrefs): ThemePrefs {
  return {
    theme: ui.theme,
    follow_system_appearance: ui.follow_system_appearance,
    light_theme: ui.light_theme,
    dark_theme: ui.dark_theme,
    theme_follow: ui.theme_follow,
    day_theme: ui.day_theme,
    night_theme: ui.night_theme,
  };
}

/** What switches the theme by itself. The game while theme_follow says
 *  so, the system while follow_system_appearance is on, which Rust keeps
 *  true only for the system, else nothing. */
export function themeFollowOf(ui: ThemePrefs): ThemeFollow {
  if (ui.theme_follow === 'game') return 'game';
  return ui.follow_system_appearance ? 'system' : 'off';
}

/** The slot a pick fills and the theme shows, or null for `theme`.
 *  With the system the slot that matches the OS, with the game the one
 *  that matches the daylight, and `theme` before the game says. */
function shownSlot(
  ui: ThemePrefs,
  systemDark: boolean,
  daylight: Daylight | null,
): 'light_theme' | 'dark_theme' | 'day_theme' | 'night_theme' | null {
  switch (themeFollowOf(ui)) {
    case 'off':
      return null;
    case 'system':
      return systemDark ? 'dark_theme' : 'light_theme';
    case 'game':
      return daylight === null ? null : daylight === 'day' ? 'day_theme' : 'night_theme';
  }
}

/** The theme id to show. Pure. `theme` while Switch themes is off, the
 *  pair entry that matches the OS while it follows the system, and the
 *  one that matches `daylight` while it follows the game. A blank entry,
 *  or a game that has not said, shows `theme`. While it follows the
 *  system and the OS asks for more contrast, the high contrast theme
 *  that matches the OS shows instead of either entry. */
export function resolveActiveTheme(
  ui: ThemePrefs,
  systemDark: boolean,
  daylight: Daylight | null,
  moreContrast = false,
): string {
  if (moreContrast && themeFollowOf(ui) === 'system') {
    return systemDark ? 'high-contrast' : 'high-contrast-light';
  }
  const slot = shownSlot(ui, systemDark, daylight);
  const id = slot === null ? '' : ui[slot];
  return id.length > 0 ? id : ui.theme;
}

/** Whether a theme reads as light or dark, from its derived chrome. */
export function themeAppearance(id: string): Appearance {
  return themeTokens(findTheme(id)).appearance;
}

/** The fields after you pick a theme in the gallery or the palette.
 *  While Switch themes is off the pick becomes `theme`. While it follows
 *  the system the pick fills the light or dark slot that matches its own
 *  appearance, and shows only when that matches the OS. While it follows
 *  the game the pick fills the day or night slot showing now, as
 *  `daylight` says, and shows at once. Either way `theme` keeps the
 *  manual pick for when Switch themes goes off again, and before the
 *  game says it takes the pick, since it is what shows. */
export function pickTheme<T extends ThemePrefs>(
  ui: T,
  id: string,
  daylight: Daylight | null = daylightShown(),
): T {
  switch (themeFollowOf(ui)) {
    case 'off':
      return { ...ui, theme: id };
    case 'system':
      return themeAppearance(id) === 'light'
        ? { ...ui, light_theme: id }
        : { ...ui, dark_theme: id };
    case 'game':
      return { ...ui, [shownSlot(ui, false, daylight) ?? 'theme']: id };
  }
}

/** Whether the OS asks for dark. False outside a browser. */
export function systemPrefersDark(): boolean {
  return osPrefersDark();
}

/** Whether the OS asks for more contrast, macOS Increase contrast.
 *  False outside a browser. */
export function systemPrefersMoreContrast(): boolean {
  return osPrefersMoreContrast();
}

/** The selected session's day or night, or the one last shown before
 *  its game says. Null when this window never knew one. */
export function daylightShown(): Daylight | null {
  const now = getDaylight();
  if (now !== null) lastDaylight = now;
  return lastDaylight;
}

/** The theme id the saved fields resolve to right now. */
export function activeThemeFor(ui: ThemePrefs): string {
  return resolveActiveTheme(ui, systemPrefersDark(), daylightShown(), systemPrefersMoreContrast());
}

// Match the native window appearance to the theme: on macOS the window
// rim and the inactive traffic lights, elsewhere the title bar where
// the system draws one. It runs in whichever window applies the theme,
// so the main and Settings windows each follow. Outside Tauri, or when
// the call fails, the window keeps the system appearance.
//
// While following the system the window follows the OS instead, and
// while following the game it takes the appearance of the theme the
// daylight shows, as for your own pick. On
// macOS the call sets the appearance for the whole app, and a forced
// appearance also pins prefers-color-scheme in every webview, so the
// listener below would never hear the OS flip.
function syncWindowAppearance(appearance: Appearance) {
  const want = followingSystem ? 'system' : appearance;
  if (want === windowAppearance) return;
  windowAppearance = want;
  const retry = () => {
    windowAppearance = null;
  };
  try {
    getCurrentWindow()
      .setTheme(want === 'system' ? null : want)
      .catch(retry);
  } catch {
    retry();
  }
}

// Swap the pair when the OS appearance flips, and swap in the high
// contrast pair when Increase contrast goes on. Installed while follow
// is on, removed when it goes off.
function followSystemScheme(on: boolean) {
  followingSystem = on;
  if (!on) {
    cleanupSchemeListener?.();
    cleanupSchemeListener = null;
    return;
  }
  if (cleanupSchemeListener) return;
  let dark: MediaQueryList;
  let contrast: MediaQueryList;
  try {
    dark = window.matchMedia(DARK_QUERY);
    contrast = window.matchMedia(CONTRAST_QUERY);
  } catch {
    return;
  }
  const update = () => {
    if (!themePrefs || themeFollowOf(themePrefs) !== 'system') return;
    const id = resolveActiveTheme(themePrefs, dark.matches, null, contrast.matches);
    if (id === currentThemeId) return;
    if (broadcastFlips) void applyAndBroadcastTheme(id);
    else applyTheme(id);
  };
  dark.addEventListener('change', update);
  contrast.addEventListener('change', update);
  cleanupSchemeListener = () => {
    dark.removeEventListener('change', update);
    contrast.removeEventListener('change', update);
  };
}

// Swap the day and night themes when the selected session's game turns,
// or when the selection moves to a session whose game shows the other.
// Installed while the theme follows the game, removed when it stops. A
// turn that keeps the same theme still leaves the new daylight in the
// cache, so a relaunch opens on the side the game shows.
function followGameDaylight(on: boolean) {
  if (!on) {
    cleanupDaylightListener?.();
    cleanupDaylightListener = null;
    return;
  }
  if (cleanupDaylightListener) return;
  cleanupDaylightListener = subscribeDaylight(() => {
    if (!themePrefs || themeFollowOf(themePrefs) !== 'game') return;
    const id = activeThemeFor(themePrefs);
    if (id !== currentThemeId) {
      if (broadcastFlips) void applyAndBroadcastTheme(id);
      else applyTheme(id);
    } else if (lastPaint && lastStandsFor !== null) {
      rememberPaint(lastPaint, lastStandsFor);
    }
  });
}

export interface ThemePrefsOptions {
  /** Send the resolved id to every window now. */
  broadcast?: boolean;
  /** Send the resolved id to every window each time the OS flips or the
   *  game turns. The main window owns this, so the Terminal repaints its
   *  palette. The setting sticks until a later call passes it again. */
  broadcastFlips?: boolean;
}

/** Remember the theme fields, show the theme they resolve to, and follow
 *  the OS or the game while Switch themes says so. Returns the id it
 *  applied. A whole UiConfig also brings the color vision the window
 *  paints for. */
export function applyThemePrefs(
  prefs: ThemePrefs & { color_vision?: unknown },
  options: ThemePrefsOptions = {},
): string {
  if (prefs.color_vision !== undefined) visionStore.set(toColorVision(prefs.color_vision));
  themePrefs = themePrefsOf(prefs);
  if (options.broadcastFlips !== undefined) broadcastFlips = options.broadcastFlips;
  const follow = themeFollowOf(themePrefs);
  followSystemScheme(follow === 'system');
  followGameDaylight(follow === 'game');
  const id = activeThemeFor(themePrefs);
  if (options.broadcast) void applyAndBroadcastTheme(id);
  else applyTheme(id);
  return id;
}

/** The theme fields this window last applied, or null before the
 *  first applyThemePrefs. */
export function getThemePrefs(): ThemePrefs | null {
  return themePrefs;
}

/** Paint for `vision` from now on, and paint the theme on screen again
 *  when it changes. The main window also fits the game colors for it
 *  (theme/fitGameColors). */
export function setColorVision(vision: ColorVision): void {
  if (vision === visionStore.get()) return;
  visionStore.set(vision);
  if (lastChoice !== null) applyTheme(lastChoice);
}

/** The color vision this window paints for. */
export function getColorVision(): ColorVision {
  return visionStore.get();
}

/** Hear each change of the color vision this window paints for. */
export function subscribeColorVision(cb: () => void): () => void {
  return visionStore.subscribe(cb);
}

/** What a theme paints on the root for `vision`, by default the one
 *  this window paints for: its attributes, its chrome tokens, and two
 *  more values the page reads. */
export function themePaintSide(
  theme: AppTheme,
  vision: ColorVision = visionStore.get(),
): ThemePaintSide {
  const tokens = themeTokens(theme, vision);
  const vars = tokensToCssVars(tokens);
  // The soft accent goes on as plain rgba over the color-mix() default
  // in styles/tokens.css. The map canvas reads it through
  // getComputedStyle, and a canvas fill cannot parse color-mix() in
  // every webview.
  const accent = parseHex(tokens.accent);
  if (accent) vars['--accent-soft'] = toRgba(accent, 0.13);
  // Expose the xterm background as a CSS var so the split history
  // overlay can paint an opaque undercoat that matches the renderer's
  // own background — covers xterm's sub-frame render gap during scroll.
  if (theme.xterm.background) vars['--xterm-bg'] = theme.xterm.background;
  return { id: theme.id, appearance: tokens.appearance, vars };
}

// Paint a theme. `standsFor` is the theme id the paint shows: the
// theme's own id, or an id the catalog does not have, once the catalog
// confirms the fallback is all there is. Null marks a fallback that is
// still waiting on the catalog, which the cache skips.
function applyToRoot(theme: AppTheme, standsFor: string | null = theme.id) {
  const side = themePaintSide(theme);
  paintRoot(document.documentElement, side);
  syncWindowAppearance(side.appearance);
  currentThemeId = theme.id;
  lastPaint = side;
  lastStandsFor = standsFor;
  if (standsFor !== null) rememberPaint(side, standsFor);
}

// The legacy `system` choice tracks the OS contrast preference: the
// high contrast theme when you asked for more contrast, else the
// default theme.
const LEGACY_SYSTEM = 'system';

function contrastTheme(more: boolean): string {
  return more ? 'high-contrast' : DEFAULT_THEME_ID;
}

/** The theme id a choice shows, with the legacy `system` resolved. */
function shownId(choice: string): string {
  if (choice !== LEGACY_SYSTEM) return choice;
  try {
    return contrastTheme(window.matchMedia(CONTRAST_QUERY).matches);
  } catch {
    return DEFAULT_THEME_ID;
  }
}

// Leave the paint for the next window to open, when it shows the theme
// the saved fields resolve to. A theme id broadcast ahead of its fields,
// a custom theme this window has not loaded yet, and a paint before the
// fields arrive each show something else for a moment, and caching that
// would open the next window on it. A saved id the catalog does not
// have shows the fallback for good, and the fallback is what gets
// cached.
function rememberPaint(shown: ThemePaintSide, standsFor: string) {
  const prefs = themePrefs;
  if (prefs && cachePaint(prefs, shown, standsFor)) {
    reportBackdrop(shown, themeFollowOf(prefs) === 'system');
  }
}

// Write the cache for a paint of `standsFor`. Returns whether that is
// the theme the fields resolve to, so the cache holds it. While the
// theme follows the game and no daylight was ever known, the theme shows
// `theme`, and the cache holds it as your pick.
function cachePaint(prefs: ThemePrefs, shown: ThemePaintSide, standsFor: string): boolean {
  const systemDark = systemPrefersDark();
  const more = systemPrefersMoreContrast();
  const daylight = daylightShown();
  if (standsFor !== shownId(resolveActiveTheme(prefs, systemDark, daylight, more))) return false;
  const side = (dark: boolean, phase: Daylight | null, contrast = false) =>
    themePaintSide(findTheme(shownId(resolveActiveTheme(prefs, dark, phase, contrast))));
  const follow = themeFollowOf(prefs);
  let paint: ThemePaint;
  if (follow === 'system') {
    // All four sides, the one on screen as painted.
    const pairSide = (dark: boolean, contrast: boolean) =>
      dark === systemDark && contrast === more ? shown : side(dark, null, contrast);
    paint = {
      v: 1,
      follow: true,
      light: pairSide(false, false),
      dark: pairSide(true, false),
      more: { light: pairSide(false, true), dark: pairSide(true, true) },
    };
  } else if (follow === 'game' && daylight !== null) {
    paint = {
      v: 1,
      follow: 'game',
      day: daylight === 'day' ? shown : side(systemDark, 'day'),
      night: daylight === 'night' ? shown : side(systemDark, 'night'),
      phase: daylight,
    };
  } else {
    paint = { v: 1, follow: false, manual: shown };
  }
  writeThemePaint(paint, pageStorage());
  return true;
}

// A custom theme edit saves without a repaint unless the theme is on
// screen. While the theme follows the system, the side not on screen
// can be that custom theme, and the cache has to pick up its new
// colors for a launch after the OS flips while Vosh is closed. When the
// theme on screen changed as well, the repaint that follows the edit
// caches both sides, and until then the cache holds what is on screen.
// The backdrop follows the theme on screen, which this leaves alone.
onCustomThemesChanged(() => {
  const prefs = themePrefs;
  const shown = lastPaint;
  const standsFor = lastStandsFor;
  if (!prefs || !shown || standsFor === null) return;
  if (!samePaintSide(themePaintSide(findTheme(standsFor)), shown)) return;
  cachePaint(prefs, shown, standsFor);
});

// Tell the backend what a new window opens on: the theme's ground, and
// the light or dark native appearance while the theme is your pick.
// While it follows the system the window follows the system too, since
// a pinned appearance would also pin prefers-color-scheme. A ground
// that is not one solid color goes as null, and the window keeps its
// own clear color, but the appearance still goes, so a new window never
// opens on the previous theme's.
function reportBackdrop(shown: ThemePaintSide, follow: boolean) {
  windowBackdropSet({
    background: solidHex(shown.vars['--bg'] ?? ''),
    appearance: follow ? null : shown.appearance,
  }).catch(() => {
    // Tauri unavailable. A new window opens on the defaults.
  });
}

// A CSS color as `#rrggbb`, or null when it is not one solid color. A
// custom theme's Background takes any color CSS can draw, so anything
// but hex goes through a canvas, which reads every syntax the webview
// does. A color the canvas cannot read leaves the clear fill in place.
function solidHex(css: string): string | null {
  const hex = parseHex(css);
  if (hex) return toHex(hex);
  try {
    const canvas = document.createElement('canvas');
    canvas.width = 1;
    canvas.height = 1;
    const ctx = canvas.getContext('2d');
    if (!ctx) return null;
    ctx.fillStyle = 'rgba(0, 0, 0, 0)';
    ctx.fillStyle = css;
    ctx.fillRect(0, 0, 1, 1);
    const [r, g, b, alpha] = ctx.getImageData(0, 0, 1, 1).data;
    return alpha === 255 ? toHex({ r, g, b }) : null;
  } catch {
    return null;
  }
}

/** Whether the root shows what the startup paint put there, so the
 *  window's first frame already held the active theme and no repaint
 *  is waiting. False when the startup paint found no cache. */
export function paintMatchesBoot(): boolean {
  const boot = bootPaintSide();
  return boot !== null && lastPaint !== null && samePaintSide(boot, lastPaint);
}

export function applyTheme(choice: string) {
  lastChoice = choice;
  if (cleanupContrastListener) {
    cleanupContrastListener();
    cleanupContrastListener = null;
  }

  if (choice === LEGACY_SYSTEM) {
    const mq = window.matchMedia(CONTRAST_QUERY);
    const update = () => {
      applyToRoot(findTheme(contrastTheme(mq.matches)));
    };
    update();
    mq.addEventListener('change', update);
    cleanupContrastListener = () => mq.removeEventListener('change', update);
    return;
  }

  const found = findTheme(choice);
  if (found.id === choice || found.id === RETIRED_THEMES.get(choice) || !choice) {
    // A blank choice shows the first theme, and a retired id its
    // successor, and that is all either shows.
    applyToRoot(found, choice);
    return;
  }
  // Requested theme wasn't found in the live registry — likely a
  // freshly-saved custom theme this window hasn't synced yet.
  // Re-fetch the catalog from the backend, register, retry. Falls
  // back to the matched-but-defaulted result if the refresh fails.
  applyToRoot(found, null);
  void refreshAndReapply(choice);
}

async function refreshAndReapply(choice: string): Promise<void> {
  try {
    const cfg = await getUiConfig();
    setCustomThemes((cfg.custom_themes ?? []).map(customToAppTheme));
    // A pick made while the catalog loaded is on screen now. Keep it.
    if (lastChoice !== choice) return;
    // The catalog is current, so the theme it finds for the id is the
    // one the id shows: the theme itself, or the fallback for an id
    // that is gone.
    applyToRoot(findTheme(choice), choice);
  } catch {
    // Backend unavailable or config malformed; keep the fallback.
  }
}

/// Apply + broadcast so other windows (settings ↔ main) stay in sync.
export async function applyAndBroadcastTheme(choice: string): Promise<void> {
  applyTheme(choice);
  try {
    await emitThemeChanged(choice);
  } catch {
    // Tauri unavailable; local apply is the persistent fallback.
  }
}

export async function subscribeThemeChanges(
  callback: (themeId: string) => void,
): Promise<UnlistenFn> {
  return subscribeThemeChanged((id) => {
    if (typeof id !== 'string') return;
    // No same-id guard. applyTheme is idempotent, and on startup the
    // local applyTheme runs before the broadcast lands, so the guard
    // would skip the only chance the Terminal has to pick up its
    // initial xterm palette.
    applyTheme(id);
    callback(id);
  });
}

export function getCurrentThemeId(): string {
  return currentThemeId;
}

/** Tell every window the theme fields changed. */
export async function broadcastThemePrefs(prefs: ThemePrefs): Promise<void> {
  try {
    await emitThemePrefsChanged(themePrefsOf(prefs));
  } catch {
    // Tauri unavailable; the local copy is already current.
  }
}

function isThemePrefs(value: unknown): value is ThemePrefs {
  if (!value || typeof value !== 'object') return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v.theme === 'string' &&
    typeof v.follow_system_appearance === 'boolean' &&
    typeof v.light_theme === 'string' &&
    typeof v.dark_theme === 'string' &&
    THEME_FOLLOWS.some((mode) => mode === v.theme_follow) &&
    typeof v.day_theme === 'string' &&
    typeof v.night_theme === 'string'
  );
}

/** Hear the theme fields another window saved. */
export async function subscribeThemePrefs(
  callback: (prefs: ThemePrefs) => void,
): Promise<UnlistenFn> {
  return subscribeThemePrefsChanged((payload) => {
    if (isThemePrefs(payload)) callback(themePrefsOf(payload));
  });
}

// Your custom themes as the UI config holds them, and the events that
// carry a theme pick, the theme fields and your custom themes to every
// window.

import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { ThemePrefs } from '../theme/theme';
import { CUSTOM_THEMES_CHANGED, THEME_CHANGED, THEME_PREFS_CHANGED } from './events';

// ThemeChoice is now a free-form string keyed against THEMES in
// src/theme/themes.ts plus the legacy `system` sentinel for tracking the
// OS contrast preference. The settings UI populates options from the
// theme registry.
export type ThemeChoice = string;

// User-authored theme. Mirrors AppTheme on the theme/themes side
// but with the two palette objects exposed as bare records so
// adding a slot only needs to touch themes.ts + the editor UI.
export interface CustomTheme {
  id: string;
  label: string;
  description: string;
  xterm: Record<string, string>;
  chrome: Record<string, string>;
  /** The game color fit of the palette, kept once Settings has fitted
   *  it (theme/gameFit). None on a theme saved before the fit or by Vosh
   *  0.8.1, which drops it. */
  fitted?: Record<string, string>;
}

export async function subscribeCustomThemesChanged(
  cb: (themes: CustomTheme[]) => void,
): Promise<UnlistenFn> {
  return listen<CustomTheme[]>(CUSTOM_THEMES_CHANGED, (event) => {
    cb(Array.isArray(event.payload) ? event.payload : []);
  });
}

/** Tell every window, this one included, your custom themes after an
 *  edit, before the save sends them. */
export function emitCustomThemesChanged(themes: CustomTheme[]): Promise<void> {
  return emit(CUSTOM_THEMES_CHANGED, themes);
}

/** Tell every window, this one included, the theme id to show. */
export function emitThemeChanged(id: string): Promise<void> {
  return emit(THEME_CHANGED, id);
}

/** Hear the theme id a window picked or saved. Pages hear it through
 *  subscribeThemeChanges in theme/theme.ts, which applies the theme first. */
export function subscribeThemeChanged(cb: (id: string) => void): Promise<UnlistenFn> {
  return listen<string>(THEME_CHANGED, (event) => cb(event.payload));
}

/** The seven fields that decide which theme a window shows, as
 *  THEME_PREFS_CHANGED carries them. ThemePrefs in theme/theme.ts picks
 *  these from the UI config. */
export const THEME_PREFS_FIELDS = [
  'theme',
  'follow_system_appearance',
  'light_theme',
  'dark_theme',
  'theme_follow',
  'day_theme',
  'night_theme',
] as const;

/** Tell every window, this one included, the seven theme fields. */
export function emitThemePrefsChanged(prefs: ThemePrefs): Promise<void> {
  return emit(THEME_PREFS_CHANGED, prefs);
}

/** Hear the theme fields another window saved. Pages hear them through
 *  subscribeThemePrefs in theme/theme.ts, which checks their shape first. */
export function subscribeThemePrefsChanged(cb: (payload: unknown) => void): Promise<UnlistenFn> {
  return listen<unknown>(THEME_PREFS_CHANGED, (event) => cb(event.payload));
}

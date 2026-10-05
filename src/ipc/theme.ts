// The theme fields of the UI config and the themes a profile starts
// with, your custom themes, and the event that carries them.

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { uniqueThemeId } from '../lib/themeImport';
import {
  BUILTIN_THEMES,
  customToAppTheme,
  DEFAULT_THEME_ID,
  themeShownBy,
  themeTokens,
} from '../lib/themes';
import { CUSTOM_THEMES_CHANGED } from './events';
import type { RawUiConfig } from './uiConfig';

/** Resolve the tri-state tint setting: an explicit user choice wins;
 *  unset is on for every theme. The chrome derives its status colors
 *  from the theme's ANSI slots, so output painted in the same slots
 *  keeps the MUD's red and the chrome's red in agreement. */
export function resolveThemeTerminalColors(_theme: string, stored: boolean | null): boolean {
  return stored ?? true;
}

// ThemeChoice is now a free-form string keyed against THEMES in
// src/lib/themes.ts plus the legacy `system` sentinel for tracking the
// OS contrast preference. The settings UI populates options from the
// theme registry.
export type ThemeChoice = string;

// User-authored theme. Mirrors AppTheme on the lib/themes side
// but with the two palette objects exposed as bare records so
// adding a slot only needs to touch themes.ts + the editor UI.
export interface CustomTheme {
  id: string;
  label: string;
  description: string;
  xterm: Record<string, string>;
  chrome: Record<string, string>;
  /** The game color fit of the palette, kept once Settings has fitted
   *  it (lib/gameFit). None on a theme saved before the fit or by Vosh
   *  0.8.1, which drops it. */
  fitted?: Record<string, string>;
}

/** The light theme a profile that never chose one is saved with, as
 *  Rust saves it (default_light_theme in profile/ui.rs). Vosh retired
 *  Vellum for Rubric (Themes review Q14), so the id shows Rubric
 *  (RETIRED_THEMES), and Vosh 0.8.1 still reads it as Vellum. A new
 *  install starts with Rubric itself, which NEW_INSTALL_LIGHT_THEME in
 *  profile/set.rs writes (Q4). */
export const DEFAULT_LIGHT_THEME_ID = 'vellum';

/** The dark theme a profile that never saved one starts with: its
 *  current theme when the theme it shows is dark, else Obsidian Ember. */
export function seedDarkTheme(theme: string, customThemes: CustomTheme[]): string {
  const found = themeShownBy([...BUILTIN_THEMES, ...customThemes.map(customToAppTheme)], theme);
  return found && themeTokens(found).appearance === 'dark' ? theme : DEFAULT_THEME_ID;
}

/** Move every custom theme whose id a built-in theme now has to a free
 *  id of its own, and point the theme choices that named it there. A
 *  theme you imported before Vosh shipped one under the same id (a
 *  Solarized Light file reads as solarized-light) would otherwise hide
 *  behind the built-in. findTheme returns the built-in, the gallery
 *  shows two tiles under one id, and your edits never reach the screen.
 *  Until a built-in took the id, a choice that named it meant the custom
 *  theme, the first one when two shared it. Returns `cfg` itself when no
 *  id collides. */
export function freeBuiltinThemeIds(cfg: RawUiConfig): RawUiConfig {
  const customs = Array.isArray(cfg.custom_themes) ? cfg.custom_themes : [];
  const builtinIds = new Set(BUILTIN_THEMES.map((t) => t.id));
  if (!customs.some((t) => builtinIds.has(t.id))) return cfg;
  const taken = new Set([...builtinIds, ...customs.map((t) => t.id)]);
  const moved = new Map<string, string>();
  const custom_themes = customs.map((t) => {
    if (!builtinIds.has(t.id)) return t;
    const id = uniqueThemeId(t.id, taken);
    taken.add(id);
    if (!moved.has(t.id)) moved.set(t.id, id);
    return { ...t, id };
  });
  const out: RawUiConfig = { ...cfg, custom_themes };
  for (const key of ['theme', 'light_theme', 'dark_theme'] as const) {
    const id = cfg[key];
    const to = typeof id === 'string' ? moved.get(id) : undefined;
    if (to !== undefined) out[key] = to;
  }
  return out;
}

export async function subscribeCustomThemesChanged(
  cb: (themes: CustomTheme[]) => void,
): Promise<UnlistenFn> {
  return listen<CustomTheme[]>(CUSTOM_THEMES_CHANGED, (event) => {
    cb(Array.isArray(event.payload) ? event.payload : []);
  });
}

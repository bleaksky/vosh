// The theme a window paints before React renders.
//
// A window learns its theme from the UI config, which arrives after an
// await, and the stylesheet defaults in styles/tokens.css are the dark
// Obsidian Ember tokens. A window opening on a light theme would paint
// dark first and flip to light a beat later. So every theme paint also
// leaves what it painted in localStorage, which every Vosh window
// shares, and a window starting up paints from it before React renders
// (src/prepaint.ts, the first import in src/main.tsx).
//
// The cache holds finished values, not theme ids, so painting from it
// needs no theme catalog, and a custom theme paints as early as a built
// in one. While the theme follows the system appearance it holds both
// sides, and the window picks the side the OS shows now, so an OS flip
// while Vosh was closed still opens on the right side. While it follows
// the game it holds both sides and the day or night last shown, so a
// drop or a relaunch opens on what you saw until World.Time comes
// again.
//
// This module stays free of the theme catalog and the Tauri API so the
// startup paint costs next to nothing. theme/theme.ts writes the cache.

import type { Daylight } from '../ipc/tick';

export const THEME_PAINT_KEY = 'vosh.cache.themePaint';

const DARK_QUERY = '(prefers-color-scheme: dark)';

/** What one theme paints on the document root. */
export interface ThemePaintSide {
  /** The theme id, written to `data-theme`. */
  id: string;
  /** Written to `data-appearance`. */
  appearance: 'light' | 'dark';
  /** Custom properties and their values, the chrome tokens and the
   *  terminal ground among them. */
  vars: Record<string, string>;
}

/** The cached paint. `manual` while the theme is your pick, the light
 *  and dark pair while it follows the system appearance, and the day
 *  and night pair with the phase last shown while it follows the game. */
export type ThemePaint =
  | { v: 1; follow: false; manual: ThemePaintSide }
  | { v: 1; follow: true; light: ThemePaintSide; dark: ThemePaintSide }
  | { v: 1; follow: 'game'; day: ThemePaintSide; night: ThemePaintSide; phase: Daylight };

/** The slice of Storage the cache uses. */
export interface PaintStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

/** The slice of the document root a paint writes. */
export interface PaintRoot {
  setAttribute(name: string, value: string): void;
  style: { setProperty(name: string, value: string): void };
}

/** The side to paint: the manual pick, the side the OS shows, or the
 *  side of the day or night last shown. */
export function pickPaintSide(paint: ThemePaint, systemDark: boolean): ThemePaintSide {
  if (paint.follow === 'game') return paint[paint.phase];
  if (!paint.follow) return paint.manual;
  return systemDark ? paint.dark : paint.light;
}

/** Write a side's attributes and custom properties on the root. */
export function paintRoot(root: PaintRoot, side: ThemePaintSide): void {
  root.setAttribute('data-theme', side.id);
  root.setAttribute('data-appearance', side.appearance);
  for (const [name, value] of Object.entries(side.vars)) {
    root.style.setProperty(name, value);
  }
}

const VAR_NAME = /^--[a-z0-9-]+$/i;

function asSide(value: unknown): ThemePaintSide | null {
  if (!value || typeof value !== 'object') return null;
  const v = value as Record<string, unknown>;
  if (typeof v.id !== 'string' || v.id === '') return null;
  if (v.appearance !== 'light' && v.appearance !== 'dark') return null;
  const vars = v.vars;
  if (!vars || typeof vars !== 'object' || Array.isArray(vars)) return null;
  const out: Record<string, string> = {};
  for (const [name, css] of Object.entries(vars as Record<string, unknown>)) {
    if (!VAR_NAME.test(name) || typeof css !== 'string') return null;
    out[name] = css;
  }
  if (Object.keys(out).length === 0) return null;
  return { id: v.id, appearance: v.appearance, vars: out };
}

/** Read a cached paint, or null for anything that is not one. */
export function parseThemePaint(raw: string | null): ThemePaint | null {
  if (!raw) return null;
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!value || typeof value !== 'object') return null;
  const v = value as Record<string, unknown>;
  if (v.v !== 1) return null;
  if (v.follow === false) {
    const manual = asSide(v.manual);
    return manual ? { v: 1, follow: false, manual } : null;
  }
  if (v.follow === true) {
    const light = asSide(v.light);
    const dark = asSide(v.dark);
    return light && dark ? { v: 1, follow: true, light, dark } : null;
  }
  if (v.follow === 'game' && (v.phase === 'day' || v.phase === 'night')) {
    const day = asSide(v.day);
    const night = asSide(v.night);
    return day && night ? { v: 1, follow: 'game', day, night, phase: v.phase } : null;
  }
  return null;
}

/** The cached paint in `storage`, or null. Never throws. */
export function readThemePaint(storage: PaintStorage | null): ThemePaint | null {
  if (!storage) return null;
  try {
    return parseThemePaint(storage.getItem(THEME_PAINT_KEY));
  } catch {
    return null;
  }
}

/** Leave a paint in `storage` for the next window, unless it is there
 *  already. Returns whether storage holds it. Never throws. */
export function writeThemePaint(paint: ThemePaint, storage: PaintStorage | null): boolean {
  if (!storage) return false;
  try {
    const entry = JSON.stringify(paint);
    if (storage.getItem(THEME_PAINT_KEY) !== entry) storage.setItem(THEME_PAINT_KEY, entry);
    return true;
  } catch {
    return false;
  }
}

/** The page's localStorage, or null where there is none or it is
 *  blocked. Reading the property itself can throw. */
export function pageStorage(): PaintStorage | null {
  try {
    return typeof window === 'undefined' ? null : (window.localStorage ?? null);
  } catch {
    return null;
  }
}

/** Whether the OS asks for dark. False outside a browser. */
export function osPrefersDark(): boolean {
  try {
    return typeof window !== 'undefined' && window.matchMedia(DARK_QUERY).matches;
  } catch {
    return false;
  }
}

export interface PrepaintEnv {
  storage: () => PaintStorage | null;
  systemDark: () => boolean;
  root: () => PaintRoot;
}

const pageEnv: PrepaintEnv = {
  storage: pageStorage,
  systemDark: osPrefersDark,
  root: () => document.documentElement,
};

let bootSide: ThemePaintSide | null = null;
let bootPhase: Daylight | null = null;

/** Paint the cached theme on the root. Synchronous, so it lands before
 *  React renders and before any await. Returns the side it painted, or
 *  null when the cache was missing or bad and the stylesheet defaults
 *  stand. Never throws. */
export function prepaintTheme(env: PrepaintEnv = pageEnv): ThemePaintSide | null {
  bootSide = null;
  bootPhase = null;
  try {
    const paint = readThemePaint(env.storage());
    if (!paint) return null;
    const side = pickPaintSide(paint, env.systemDark());
    paintRoot(env.root(), side);
    bootSide = side;
    if (paint.follow === 'game') bootPhase = paint.phase;
    return side;
  } catch {
    return null;
  }
}

/** The side prepaintTheme painted at startup, or null. */
export function bootPaintSide(): ThemePaintSide | null {
  return bootSide;
}

/** The day or night the startup paint showed while the theme followed
 *  the game, or null. */
export function bootPaintPhase(): Daylight | null {
  return bootPhase;
}

/** Whether two sides paint the same thing. */
export function samePaintSide(a: ThemePaintSide, b: ThemePaintSide): boolean {
  if (a.id !== b.id || a.appearance !== b.appearance) return false;
  const aKeys = Object.keys(a.vars);
  if (aKeys.length !== Object.keys(b.vars).length) return false;
  return aKeys.every((key) => a.vars[key] === b.vars[key]);
}

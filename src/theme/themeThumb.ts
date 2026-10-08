// Gallery thumbnails for the Appearance tab. Each tile paints a tiny
// window in five colors derived from the theme (theme/chrome), so an
// imported theme gets a true thumbnail without anyone picking swatches.
// The tile recipe: the ground in bg, a 22 px panel strip in panel
// behind a 1 px sep line, a 6 px accent dot, and three text bars in
// text at full, 0.6, and 0.35 opacity.

import type { Appearance } from './chrome';
import { themeTokens, type AppTheme } from './themes';

export interface ThemeThumb {
  bg: string;
  panel: string;
  sep: string;
  accent: string;
  text: string;
  appearance: Appearance;
  /** The 1 px inset ring around the tile, white on dark themes and
   *  black on light ones. */
  ring: string;
}

const RING: Record<Appearance, string> = {
  dark: 'rgba(255,255,255,0.10)',
  light: 'rgba(0,0,0,0.14)',
};

/** The thumbnail colors for one theme. Pure. */
export function themeThumb(theme: AppTheme): ThemeThumb {
  const t = themeTokens(theme);
  return {
    bg: t.bg,
    panel: t.panel,
    sep: t.sep,
    accent: t.accent,
    text: t.text,
    appearance: t.appearance,
    ring: RING[t.appearance],
  };
}

/** The themes the gallery shows first: Vosh's signature pair, then six
 *  more in a set order, less Vellum, which Rubric replaced. */
export const GALLERY_LEAD_IDS = [
  'triad',
  'rubric',
  'nord',
  'obsidian-ember',
  'gruvbox',
  'rose-pine',
  'tokyo-night',
] as const;

/** Every theme in gallery order: the lead (GALLERY_LEAD_IDS) first, then
 *  the other built ins by label, then your custom themes as you added
 *  them. */
export function galleryThemes(builtins: AppTheme[], custom: AppTheme[]): AppTheme[] {
  const lead = GALLERY_LEAD_IDS.map((id) => builtins.find((t) => t.id === id)).filter(
    (t): t is AppTheme => t !== undefined,
  );
  const leadIds = new Set<string>(GALLERY_LEAD_IDS);
  const rest = builtins
    .filter((t) => !leadIds.has(t.id))
    .sort((a, b) => a.label.localeCompare(b.label));
  return [...lead, ...rest, ...custom];
}

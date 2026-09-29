// The Appearance page's choices and edits, kept pure so they can be
// tested without a window: what the Font, Size, Light theme, and Dark
// theme selects offer, and how custom themes and the base palette
// change. The page (src/components/settings/pages/AppearancePage.tsx)
// applies and saves the results.

import type { Appearance } from './chrome';
import { ANSI_SLOTS, CANONICAL_ANSI_16, type AnsiSlot } from './baseAnsi';
import { DEFAULT_LIGHT_THEME_ID, type CustomTheme, type SystemFontEntry } from './session';
import type { ThemePrefs } from './theme';
import { themeIdFromLabel, uniqueThemeId } from './themeImport';
import { DEFAULT_THEME_ID, themeTokens, type AppTheme } from './themes';

/** One option of a settings select. */
export interface Choice {
  value: string;
  label: string;
}

// ── Font ─────────────────────────────────────────────────────────────

/** The fonts Vosh ships, which every install has. */
export const BUNDLED_FONTS: readonly Choice[] = [
  { label: 'Berkeley Mono', value: '"BerkeleyMono Bundled", Menlo, monospace' },
  { label: 'JetBrains Mono', value: '"JetBrainsMono Bundled", Menlo, monospace' },
];

/** The first family in a CSS font list, without its quotes. */
export function primaryFontFamily(stack: string): string {
  const first = stack.split(',')[0] ?? '';
  return first
    .trim()
    .replace(/^["']|["']$/g, '')
    .trim();
}

// Two names for the same face compare equal: `JetBrainsMono Bundled`
// is the bundled copy of `JetBrains Mono`.
function fontKey(family: string): string {
  return family
    .toLowerCase()
    .replace(/\s+bundled$/, '')
    .replace(/[\s_-]+/g, '');
}

/** The font list Vosh saves for an installed family. */
export function systemFontStack(family: string): string {
  return `"${family}", Menlo, monospace`;
}

/** The name the Font select shows for a font list. */
export function fontLabel(stack: string): string {
  const family = primaryFontFamily(stack).replace(/\s+Bundled$/i, '');
  const bundled = BUNDLED_FONTS.find(
    (f) => fontKey(primaryFontFamily(f.value)) === fontKey(family),
  );
  if (bundled) return bundled.label;
  return family || stack.trim();
}

/** What the Font select offers: the bundled fonts, then the installed
 *  monospace fonts by name. The option for your current font carries
 *  your exact font list, so showing it never rewrites it. A current
 *  font the list does not have comes first. */
export function fontChoices(current: string, installed: readonly SystemFontEntry[]): Choice[] {
  const choices: Choice[] = BUNDLED_FONTS.map((f) => ({ ...f }));
  const seen = new Set(choices.map((c) => fontKey(primaryFontFamily(c.value))));
  const families = installed
    .filter((f) => f.monospace)
    .map((f) => f.family.trim())
    .filter((family) => family !== '')
    .sort((a, b) => a.localeCompare(b));
  for (const family of families) {
    const key = fontKey(family);
    if (seen.has(key)) continue;
    seen.add(key);
    choices.push({ label: family, value: systemFontStack(family) });
  }
  if (current.trim() === '') return choices;
  const key = fontKey(primaryFontFamily(current));
  const index = choices.findIndex((c) => fontKey(primaryFontFamily(c.value)) === key);
  if (index >= 0) {
    choices[index] = { ...choices[index], value: current };
    return choices;
  }
  return [{ label: fontLabel(current), value: current }, ...choices];
}

// ── Size ─────────────────────────────────────────────────────────────

/** The sizes the approved board offers, in points. */
export const TEXT_SIZES: readonly number[] = [11, 12, 13, 14, 15, 16, 18];

/** What the Size select offers: the board's sizes plus your current
 *  size when it is not one of them, smallest first. */
export function sizeChoices(current: number): Choice[] {
  const sizes = new Set<number>(TEXT_SIZES);
  if (Number.isFinite(current) && current > 0) sizes.add(current);
  return [...sizes].sort((a, b) => a - b).map((n) => ({ value: String(n), label: `${n} pt` }));
}

// ── Light and dark themes ────────────────────────────────────────────

/** What the Light theme or Dark theme select offers: every theme of
 *  that appearance in gallery order. The current pick stays listed
 *  first when it is not one of them, so the select shows it. */
export function pairChoices(
  themes: readonly AppTheme[],
  appearance: Appearance,
  current: string,
): Choice[] {
  const choices = themes
    .filter((t) => themeTokens(t).appearance === appearance)
    .map((t) => ({ value: t.id, label: t.label }));
  if (current !== '' && !choices.some((c) => c.value === current)) {
    const found = themes.find((t) => t.id === current);
    choices.unshift({ value: current, label: found?.label ?? current });
  }
  return choices;
}

// ── Custom themes ────────────────────────────────────────────────────

type ThemeFields = ThemePrefs & { custom_themes: CustomTheme[] };

/** A new custom theme that starts as a copy of `base`, with an id
 *  clear of `takenIds`. */
export function copyTheme(base: AppTheme, takenIds: Iterable<string>): CustomTheme {
  const label = `${base.label} copy`;
  return {
    id: uniqueThemeId(themeIdFromLabel(label), takenIds),
    label,
    description: `A copy of ${base.label}.`,
    xterm: { ...(base.xterm as unknown as Record<string, string>) },
    chrome: { ...(base.chrome ?? {}) } as Record<string, string>,
  };
}

/** Replace one custom theme's fields. */
export function editCustomTheme(
  list: readonly CustomTheme[],
  id: string,
  patch: Partial<CustomTheme>,
): CustomTheme[] {
  return list.map((t) => (t.id === id ? { ...t, ...patch } : t));
}

/** The fields after you delete a custom theme. Any pick that named it
 *  falls back to the stock theme for its place: Obsidian Ember for the
 *  manual pick and the dark theme, Vellum for the light theme. */
export function removeCustomTheme<T extends ThemeFields>(ui: T, id: string): T {
  return {
    ...ui,
    custom_themes: ui.custom_themes.filter((t) => t.id !== id),
    theme: ui.theme === id ? DEFAULT_THEME_ID : ui.theme,
    light_theme: ui.light_theme === id ? DEFAULT_LIGHT_THEME_ID : ui.light_theme,
    dark_theme: ui.dark_theme === id ? DEFAULT_THEME_ID : ui.dark_theme,
  };
}

/** The visible names of the 16 ANSI slots. */
export const ANSI_SLOT_LABELS: Readonly<Record<AnsiSlot, string>> = {
  black: 'Black',
  red: 'Red',
  green: 'Green',
  yellow: 'Yellow',
  blue: 'Blue',
  magenta: 'Magenta',
  cyan: 'Cyan',
  white: 'White',
  brightBlack: 'Bright black',
  brightRed: 'Bright red',
  brightGreen: 'Bright green',
  brightYellow: 'Bright yellow',
  brightBlue: 'Bright blue',
  brightMagenta: 'Bright magenta',
  brightCyan: 'Bright cyan',
  brightWhite: 'Bright white',
};

/** A group of slots the custom theme editor shows. Chrome slots pin a
 *  window token the theme would otherwise derive. Terminal slots are
 *  the theme's own palette. */
export interface ThemeSlotGroup {
  heading: string;
  source: 'chrome' | 'xterm';
  slots: readonly { key: string; label: string }[];
}

const ansiSlots = (keys: readonly AnsiSlot[]) =>
  keys.map((key) => ({ key, label: ANSI_SLOT_LABELS[key] }));

/** The custom theme editor's groups, in the order it shows them. */
export const THEME_SLOT_GROUPS: readonly ThemeSlotGroup[] = [
  {
    heading: 'Accent and status',
    source: 'chrome',
    slots: [
      { key: 'accent', label: 'Accent' },
      { key: 'danger', label: 'Danger' },
      { key: 'warn', label: 'Warning' },
      { key: 'success', label: 'Success' },
    ],
  },
  {
    heading: 'Terminal',
    source: 'xterm',
    slots: [
      { key: 'background', label: 'Background' },
      { key: 'foreground', label: 'Text' },
      { key: 'cursor', label: 'Cursor' },
      { key: 'cursorAccent', label: 'Text under the cursor' },
      { key: 'selectionBackground', label: 'Selection' },
      { key: 'selectionForeground', label: 'Selected text' },
    ],
  },
  {
    heading: 'Normal colors',
    source: 'xterm',
    slots: ansiSlots(ANSI_SLOTS.slice(0, 8)),
  },
  {
    heading: 'Bright colors',
    source: 'xterm',
    slots: ansiSlots(ANSI_SLOTS.slice(8)),
  },
];

// ── Colors ───────────────────────────────────────────────────────────

/** A `#rrggbb` a native color input accepts, read from any hex or
 *  rgb() text. Anything else gives `fallback`. */
export function colorInputValue(value: string, fallback = '#888888'): string {
  const text = value.trim();
  if (/^#[0-9a-f]{6}$/i.test(text)) return text.toLowerCase();
  const short = /^#([0-9a-f])([0-9a-f])([0-9a-f])$/i.exec(text);
  if (short)
    return `#${short[1]}${short[1]}${short[2]}${short[2]}${short[3]}${short[3]}`.toLowerCase();
  const long = /^#([0-9a-f]{6})[0-9a-f]{2}$/i.exec(text);
  if (long) return `#${long[1]}`.toLowerCase();
  const rgb = /^rgba?\(\s*(-?[\d.]+)\s*,\s*(-?[\d.]+)\s*,\s*(-?[\d.]+)/i.exec(text);
  if (rgb) {
    const hex = (n: string) =>
      Math.max(0, Math.min(255, Math.round(Number(n))))
        .toString(16)
        .padStart(2, '0');
    return `#${hex(rgb[1])}${hex(rgb[2])}${hex(rgb[3])}`;
  }
  return fallback;
}

/** The 16 base palette colors in ANSI order, the stock chart when you
 *  have not changed any. */
export function basePalette(saved: readonly string[] | null): string[] {
  return saved && saved.length === ANSI_SLOTS.length
    ? [...saved]
    : ANSI_SLOTS.map((slot) => CANONICAL_ANSI_16[slot]);
}

/** The base palette after you change one slot. The first change saves
 *  all 16, starting from the stock chart. */
export function withBaseColor(
  saved: readonly string[] | null,
  index: number,
  color: string,
): string[] {
  const next = basePalette(saved);
  if (index >= 0 && index < next.length) next[index] = color;
  return next;
}

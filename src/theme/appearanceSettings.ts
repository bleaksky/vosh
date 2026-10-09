// The Appearance page's choices and edits, kept pure so they can be
// tested without a window: what the Font and Size selects of Terminal
// text and Panel text, and the Light theme and Dark theme selects,
// offer, what the Color vision row says your vision swaps, and how
// custom themes and the base palette change. The page
// (src/settings/appearance/AppearancePage.tsx) applies and saves the
// results.

import type { Appearance } from './chrome';
import { ANSI_SLOTS, CANONICAL_ANSI_16, type AnsiSlot } from './baseAnsi';
import {
  normalizePanelFont,
  PANEL_FONT_DESIGNED,
  PANEL_FONT_SYSTEM,
  PANEL_FONT_TERMINAL,
} from '../panel/panelFont';
import { renderFontStack } from '../lib/fontLoader';
import type { CustomTheme } from '../ipc/theme';
import type { SystemFontEntry } from '../ipc/uiConfig';
import type { ThemePrefs } from './theme';
import { themeIdFromLabel, uniqueThemeId } from './themeImport';
import { fitKey, type ColorVision } from './gameFit';
import {
  customToAppTheme,
  DEFAULT_LIGHT_THEME_ID,
  DEFAULT_THEME_ID,
  themeTokens,
  type AppTheme,
  type ThemeLicense,
  type XtermPalette,
} from './themes';

/** One option of a settings select. */
export interface Choice {
  value: string;
  label: string;
}

// ── Font ─────────────────────────────────────────────────────────────

/** The fonts Vosh ships, which every install has. */
export const BUNDLED_FONTS: readonly Choice[] = [
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

// The first family of a font list as the page draws it, so a list saved
// with a retired name reads as the font it draws in.
function drawnFamily(stack: string): string {
  return primaryFontFamily(renderFontStack(stack));
}

/** The name the Font select shows for a font list. */
export function fontLabel(stack: string): string {
  const family = drawnFamily(stack).replace(/\s+Bundled$/i, '');
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
  const key = fontKey(drawnFamily(current));
  const index = choices.findIndex((c) => fontKey(primaryFontFamily(c.value)) === key);
  if (index >= 0) {
    choices[index] = { ...choices[index], value: current };
    return choices;
  }
  return [{ label: fontLabel(current), value: current }, ...choices];
}

// ── Panel font ───────────────────────────────────────────────────────

/** What the Panel text Font select offers: As designed, the terminal
 *  font, the system font, then the fonts the Font select offers. A font
 *  you picked keeps your exact font list, as Font does. */
export function panelFontChoices(current: string, installed: readonly SystemFontEntry[]): Choice[] {
  const pick = normalizePanelFont(current);
  const named = ![PANEL_FONT_DESIGNED, PANEL_FONT_TERMINAL, PANEL_FONT_SYSTEM].includes(pick);
  return [
    { value: PANEL_FONT_DESIGNED, label: 'As designed' },
    { value: PANEL_FONT_TERMINAL, label: 'Same as terminal' },
    { value: PANEL_FONT_SYSTEM, label: 'System font' },
    ...fontChoices(named ? pick : '', installed),
  ];
}

// ── Size ─────────────────────────────────────────────────────────────

/** The sizes the Size select offers, in points. */
export const TEXT_SIZES: readonly number[] = [11, 12, 13, 14, 15, 16, 18];

/** What the Size select offers: those sizes plus your current
 *  size when it is not one of them, smallest first. */
export function sizeChoices(current: number): Choice[] {
  const sizes = new Set<number>(TEXT_SIZES);
  if (Number.isFinite(current) && current > 0) sizes.add(current);
  return [...sizes].sort((a, b) => a - b).map((n) => ({ value: String(n), label: `${n} pt` }));
}

/** What a Size select that can follow the terminal offers: Same as
 *  terminal, then the sizes Size offers, your size among them. Panel
 *  text and the command line both save 0 to follow the terminal. */
export function sizeChoicesWithTerminal(current: number): Choice[] {
  return [{ value: '0', label: 'Same as terminal' }, ...sizeChoices(current)];
}

// ── Light and dark themes ────────────────────────────────────────────

/** What the Light theme or Dark theme select offers: every theme of
 *  that appearance in gallery order. Null offers every theme, light or
 *  dark, as the Day theme and Night theme selects do. The current pick
 *  stays listed first when it is not one of them, so the select shows
 *  it. */
export function pairChoices(
  themes: readonly AppTheme[],
  appearance: Appearance | null,
  current: string,
): Choice[] {
  const choices = themes
    .filter((t) => appearance === null || themeTokens(t).appearance === appearance)
    .map((t) => ({ value: t.id, label: t.label }));
  if (current !== '' && !choices.some((c) => c.value === current)) {
    const found = themes.find((t) => t.id === current);
    choices.unshift({ value: current, label: found?.label ?? current });
  }
  return choices;
}

/** The theme an arrow key moves to in the gallery. `step` 1 is the
 *  next theme in gallery order and -1 the one before, wrapping at both
 *  ends. With `appearance` set only themes of that appearance count.
 *  While Switch themes follows the system the page passes the OS
 *  appearance, so each step shows the theme it lands on and fills only
 *  the slot the OS uses now, never the other one. Returns `from` when
 *  no other theme qualifies. */
export function stepGalleryTheme(
  themes: readonly AppTheme[],
  from: string,
  step: 1 | -1,
  appearance?: Appearance,
): string {
  const count = themes.length;
  const found = themes.findIndex((t) => t.id === from);
  const at = found >= 0 ? found : step > 0 ? -1 : count;
  for (let i = 1; i <= count; i += 1) {
    const theme = themes[(((at + step * i) % count) + count) % count];
    if (appearance === undefined || themeTokens(theme).appearance === appearance) return theme.id;
  }
  return from;
}

// ── Theme caption ────────────────────────────────────────────────────

const LICENSE_TERMS: Record<ThemeLicense, string> = {
  MIT: 'under the MIT license',
  'GPL-3.0': 'under the GPL version 3',
  'GPL-3.0-or-later': 'under the GPL version 3 or later',
  'Public domain': 'in the public domain',
  'None published': 'with no license published',
};

/** The caption under the theme gallery for the theme on screen: its
 *  description, then for a built in theme one sentence that names
 *  where its colors come from, who made them, and their license. A
 *  custom theme shows its description alone, and nothing when that is
 *  blank. */
export function themeCaption(theme: AppTheme): string {
  const { source, author, license } = theme;
  const parts = [theme.description.trim()];
  if (source !== undefined && author !== undefined && license !== undefined) {
    const terms = LICENSE_TERMS[license];
    const from = author === source ? source : `${source} by ${author}`;
    parts.push(
      source === 'Vosh'
        ? `${author} made it for Vosh, ${terms}.`
        : `Its colors come from ${from}, ${terms}.`,
    );
  }
  return parts.filter((part) => part !== '').join(' ');
}

// ── Color vision ─────────────────────────────────────────────────────

/** The quiet line under the Color vision row, which says what your
 *  vision swaps. Every theme swaps the same families as far as its own
 *  colors leave room, so the line names none. Empty under Typical. While
 *  the theme's colors are off for MUD text (`themeTerminalColors`), game
 *  text keeps your base palette, so only the window changes. */
export function colorVisionNote(vision: ColorVision, themeTerminalColors = true): string {
  if (vision === 'typical') return '';
  const game = themeTerminalColors
    ? vision === 'tritanopia'
      ? 'In the game text blues turn purple and magentas turn pink.'
      : 'In the game text greens turn blue, reds lean toward orange and blues toward violet, as far as your theme leaves room.'
    : "Game text keeps your base palette while the theme's colors are off for MUD text.";
  const window =
    vision === 'tritanopia'
      ? 'The window keeps danger, warn and success where you tell them apart, and makes them lighter or darker where they sit near. An accent Vosh picks moves clear of them.'
      : 'In the window success turns blue and danger leans toward orange.';
  return `${game} ${window}`;
}

// ── Custom themes ────────────────────────────────────────────────────

type ThemeFields = ThemePrefs & { custom_themes: CustomTheme[] };

/** A new custom theme that starts as a copy of `base`, with an id
 *  clear of `takenIds`. It keeps the fit of `base`, since its colors
 *  are the same. */
export function copyTheme(base: AppTheme, takenIds: Iterable<string>): CustomTheme {
  const label = `${base.label} copy`;
  return {
    id: uniqueThemeId(themeIdFromLabel(label), takenIds),
    label,
    description: `A copy of ${base.label}.`,
    xterm: { ...(base.xterm as unknown as Record<string, string>) },
    chrome: { ...(base.chrome ?? {}) } as Record<string, string>,
    ...(base.fitted && { fitted: { ...base.fitted } as Record<string, string> }),
  };
}

/** The colors a game color fit of the custom theme reads (fitKey). */
export function customFitKey(theme: CustomTheme): string {
  return fitKey(customToAppTheme(theme).xterm);
}

/** Replace one custom theme's fields. A change to a color the game
 *  color fit reads drops the fit the theme kept, since it was fitted to
 *  the old colors. */
export function editCustomTheme(
  list: readonly CustomTheme[],
  id: string,
  patch: Partial<CustomTheme>,
): CustomTheme[] {
  return list.map((t) => {
    if (t.id !== id) return t;
    const next = { ...t, ...patch };
    if (patch.xterm !== undefined && customFitKey(next) !== customFitKey(t)) delete next.fitted;
    return next;
  });
}

/** `list` with `fitted` kept on theme `id`, or null when that theme is
 *  gone or its colors changed since `palette` was fitted. */
export function keepFit(
  list: readonly CustomTheme[],
  id: string,
  palette: XtermPalette,
  fitted: Partial<XtermPalette>,
): CustomTheme[] | null {
  const theme = list.find((t) => t.id === id);
  if (!theme || customFitKey(theme) !== fitKey(palette)) return null;
  return editCustomTheme(list, id, { fitted: { ...fitted } as Record<string, string> });
}

/** The fields after you delete a custom theme. Any pick that named it
 *  falls back to the stock theme for its place: Obsidian Ember for the
 *  manual pick and the dark theme, DEFAULT_LIGHT_THEME_ID for the light
 *  theme, which shows Rubric. A day or night theme that named it goes
 *  empty, so that slot shows the manual pick. */
export function removeCustomTheme<T extends ThemeFields>(ui: T, id: string): T {
  return {
    ...ui,
    custom_themes: ui.custom_themes.filter((t) => t.id !== id),
    theme: ui.theme === id ? DEFAULT_THEME_ID : ui.theme,
    light_theme: ui.light_theme === id ? DEFAULT_LIGHT_THEME_ID : ui.light_theme,
    dark_theme: ui.dark_theme === id ? DEFAULT_THEME_ID : ui.dark_theme,
    day_theme: ui.day_theme === id ? '' : ui.day_theme,
    night_theme: ui.night_theme === id ? '' : ui.night_theme,
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

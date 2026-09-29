import { describe, expect, it } from 'vitest';
import {
  CHROME_COLOR_KEYS,
  ON_ACCENT_CONTRAST,
  SECONDARY_CONTRAST,
  STATUS_CONTRAST,
  TERTIARY_CONTRAST,
  type Appearance,
  type ChromeColorKey,
} from './chrome';
import { composite, contrast, deltaE2000, parseHex, type Rgb } from './color';
import {
  BUILTIN_THEMES,
  customToAppTheme,
  findTheme,
  migrateCustomChrome,
  themeTokens,
} from './themes';

const hex = (h: string): Rgb => {
  const c = parseHex(h);
  if (!c) throw new Error(`not hex ${h}`);
  return c;
};

// rgba(r, g, b, a) composited over an opaque ground, so a translucent
// token compares by the color it actually paints.
const paint = (color: string, ground: string): Rgb => {
  const m = /^rgba\((\d+),\s*(\d+),\s*(\d+),\s*([\d.]+)\)$/.exec(color);
  if (!m) return hex(color);
  return composite({ r: +m[1], g: +m[2], b: +m[3] }, hex(ground), +m[4]);
};

interface CanvasSheet {
  id: string;
  appearance: Appearance;
  tokens: Record<ChromeColorKey, string>;
}

// The approved One Window canvas tokens (SPEC section 4, the Palette
// and Main artboards for onAccent). The canvas token sheet differs
// only in Ember's title, #8e8e8e, which the second Ember entry covers.
const NORD: CanvasSheet = {
  id: 'nord',
  appearance: 'dark',
  tokens: {
    bg: '#2e3440',
    panel: '#2e3440',
    sep: '#434c5e',
    divider: '#383e4a',
    selrow: '#3b4252',
    hover: '#373d49',
    inputband: '#363c48',
    text: '#e5e9f0',
    secondary: '#c0c7d3',
    tertiary: '#7b8294',
    title: '#a1a4a9',
    raised: '#2e3440',
    accent: '#88c0d0',
    onAccent: '#1b1f27',
    danger: '#bf616a',
    warn: '#ebcb8b',
    success: '#a3be8c',
    selection: 'rgba(136, 192, 208, 0.22)',
  },
};

const EMBER: CanvasSheet = {
  id: 'obsidian-ember',
  appearance: 'dark',
  tokens: {
    bg: '#050403',
    panel: '#0c0a08',
    sep: '#1d1b19',
    divider: '#181614',
    selrow: '#1d1b19',
    hover: '#171513',
    inputband: '#0f0e0d',
    text: '#c0bdbb',
    secondary: '#918e8c',
    tertiary: '#62605e',
    title: '#8e8c8b',
    raised: '#100f0d',
    accent: '#ef8f2f',
    onAccent: '#140b02',
    danger: '#ea8f80',
    warn: '#ecc985',
    success: '#8fdaa8',
    selection: 'rgba(239, 143, 47, 0.20)',
  },
};

const VELLUM: CanvasSheet = {
  id: 'vellum',
  appearance: 'light',
  tokens: {
    bg: '#f7f4ee',
    panel: '#f0ede7',
    sep: '#dad8d2',
    divider: '#dfdcd7',
    selrow: '#ffffff',
    hover: '#e6e4de',
    inputband: '#eeebe6',
    text: '#2a2622',
    secondary: '#5c5853',
    tertiary: '#898681',
    title: '#7c7a77',
    raised: '#ffffff',
    accent: '#3f6690',
    onAccent: '#ffffff',
    danger: '#a8453a',
    warn: '#94661a',
    success: '#4f7a3a',
    selection: 'rgba(63, 102, 144, 0.18)',
  },
};

const SHEETS: CanvasSheet[] = [
  NORD,
  EMBER,
  VELLUM,
  { ...EMBER, tokens: { ...EMBER.tokens, title: '#8e8e8e' } },
];

describe('built-in themes reproduce the approved canvas', () => {
  SHEETS.forEach((sheet, n) => {
    it(`${sheet.id} (sheet ${n + 1}) within delta E 2`, () => {
      const theme = findTheme(sheet.id);
      expect(theme.id).toBe(sheet.id);
      const derived = themeTokens(theme);
      expect(derived.appearance).toBe(sheet.appearance);
      for (const key of CHROME_COLOR_KEYS) {
        const got = paint(derived[key], sheet.tokens.bg);
        const want = paint(sheet.tokens[key], sheet.tokens.bg);
        expect(
          deltaE2000(got, want),
          `${key} ${derived[key]} vs ${sheet.tokens[key]}`,
        ).toBeLessThan(2);
      }
    });
  });
});

describe('contrast floors', () => {
  for (const theme of BUILTIN_THEMES) {
    it(`${theme.id} keeps every floor`, () => {
      const t = themeTokens(theme);
      const panel = hex(t.panel);
      const bg = hex(t.bg);
      const on = (key: ChromeColorKey, ground: Rgb) => contrast(hex(t[key]), ground);
      expect(on('text', panel), 'text').toBeGreaterThanOrEqual(SECONDARY_CONTRAST);
      expect(on('secondary', panel), 'secondary').toBeGreaterThanOrEqual(SECONDARY_CONTRAST);
      expect(on('tertiary', panel), 'tertiary').toBeGreaterThanOrEqual(TERTIARY_CONTRAST);
      expect(on('danger', panel), 'danger').toBeGreaterThanOrEqual(STATUS_CONTRAST);
      expect(on('warn', panel), 'warn').toBeGreaterThanOrEqual(STATUS_CONTRAST);
      expect(on('success', panel), 'success').toBeGreaterThanOrEqual(STATUS_CONTRAST);
      expect(on('title', bg), 'title').toBeGreaterThanOrEqual(3);
      expect(on('accent', bg), 'accent').toBeGreaterThanOrEqual(3);
      expect(on('onAccent', hex(t.accent)), 'onAccent').toBeGreaterThanOrEqual(ON_ACCENT_CONTRAST);
    });
  }

  it('keeps the text tiers in order', () => {
    for (const theme of BUILTIN_THEMES) {
      const t = themeTokens(theme);
      const panel = hex(t.panel);
      const text = contrast(hex(t.text), panel);
      const secondary = contrast(hex(t.secondary), panel);
      const tertiary = contrast(hex(t.tertiary), panel);
      expect(text, theme.id).toBeGreaterThanOrEqual(secondary);
      expect(secondary, theme.id).toBeGreaterThan(tertiary);
    }
  });

  it('keeps the accent each theme shipped before chrome was derived', () => {
    const accents: Record<string, string> = {
      'obsidian-ember': '#ef8f2f',
      'kanso-zen': '#b0c8d4',
      'tokyo-night': '#7aa2f7',
      nord: '#88c0d0',
      gruvbox: '#fabd2f',
      catppuccin: '#f5c2e7',
      dracula: '#bd93f9',
      monokai: '#f92672',
      'one-dark': '#61afef',
      'one-half-dark': '#61afef',
      'tango-dark': '#729fcf',
      'classic-vivid': '#ffaa00',
      'high-contrast': '#ffff00',
    };
    for (const [id, accent] of Object.entries(accents)) {
      expect(themeTokens(findTheme(id)).accent, id).toBe(accent);
    }
  });
});

describe('custom theme chrome', () => {
  // A custom theme forked from Obsidian Ember before chrome was derived.
  const legacy = {
    surfaceDeep: '#0e0c0b',
    surface: '#0a0908',
    surfacePane: '#100f0d',
    surfaceLift: '#181514',
    surfaceEmphasis: '#201d1c',
    textStrong: '#f2efee',
    text: '#e0dddb',
    textMuted: '#c0bdbb',
    textFaint: '#9b9795',
    textDim: '#726e6b',
    borderSoft: 'rgba(80, 76, 74, 0.24)',
    border: 'rgba(80, 76, 74, 0.24)',
    borderStrong: 'rgba(80, 76, 74, 0.42)',
    borderHover: 'rgba(80, 76, 74, 0.42)',
    accent: '#ff3399',
    accentSoft: 'rgba(255, 51, 153, 0.13)',
    warn: '#e5c057',
    danger: '#e3645e',
    info: '#6ec3eb',
    success: '#76cf8a',
  };

  it('keeps only the accent from a legacy palette', () => {
    expect(migrateCustomChrome(legacy)).toEqual({ accent: '#ff3399' });
  });

  it('passes token overrides through and drops unknown or empty keys', () => {
    expect(
      migrateCustomChrome({ accent: '#123456', danger: '#aa0000', bogus: '#fff', warn: '' }),
    ).toEqual({ accent: '#123456', danger: '#aa0000' });
    expect(migrateCustomChrome({ appearance: 'light', panel: '#eeeeee' })).toEqual({
      appearance: 'light',
      panel: '#eeeeee',
    });
    expect(migrateCustomChrome(undefined)).toEqual({});
  });

  it('derives a legacy custom theme from its terminal slots', () => {
    const ember = findTheme('obsidian-ember');
    const custom = customToAppTheme({
      id: 'custom',
      label: 'Ember (custom)',
      description: '',
      xterm: { ...ember.xterm },
      chrome: legacy,
    });
    const t = themeTokens(custom);
    expect(t.accent).toBe('#ff3399');
    expect(t.panel).toBe('#0c0a08');
    expect(t.text).toBe('#c0bdbb');
    expect(t.warn).toBe('#ecc985');
  });
});

import { afterEach, describe, expect, it } from 'vitest';
import {
  ACCENT_APART,
  CHROME_COLOR_KEYS,
  ON_ACCENT_CONTRAST,
  SECONDARY_CONTRAST,
  STATUS_CONTRAST,
  STATUS_MOVE_MIN,
  STATUS_PART,
  STATUS_SWAP,
  STATUS_TEXT_CONTRAST,
  TERTIARY_CONTRAST,
  WASH_STEP,
  type Appearance,
  type ChromeColorKey,
} from './chrome';
import {
  composite,
  contrast,
  deltaE2000,
  deltaEOk,
  oklchToRgbInGamut,
  parseHex,
  rgbToOklab,
  rgbToOklch,
  toHex,
  WHITE,
  type Rgb,
} from './color';
import type { CustomTheme } from '../ipc/theme';
import type { AnsiSlot } from './baseAnsi';
import {
  CHANNEL_FLOOR,
  CHANNEL_LEAST,
  CHANNEL_PAIRS,
  checks,
  CHROMA_KEEP,
  CUE_FLOOR,
  CUE_PAIRS,
  CUE_SLOTS,
  familyOf,
  GAME_SLOTS,
  holdsCheck,
  HUE_CHROMA_KEEP,
  KEPT_PAIRS,
  KEPT_SLACK,
  L_REACH,
  LEAD_SLOTS,
  MISS_SLACK,
  MOVE_MIN,
  PART_MIN,
  PARTED_PAIRS,
  seenApart,
  SWAP_TARGETS,
  TEXT_SLOTS,
  VISION_GUARD,
  VISION_SLACK,
  type ColorVision,
} from './gameFit';
import {
  BUILTIN_THEMES,
  customThemeLabel,
  customToAppTheme,
  findTheme,
  migrateCustomChrome,
  playPalette,
  RETIRED_THEMES,
  seedDarkTheme,
  setCustomThemes,
  themeShownBy,
  themeTokens,
  typicalStart,
  visionFitOf,
  type AppTheme,
  type XtermPalette,
} from './themes';
import credits from '../../public/theme-credits.txt?raw';

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

// A lightness step in OKLab L times 100. Below Obsidian Ember's ground
// OKLab L runs too steep to measure by, so there a step counts as the
// one that gives the same contrast on Ember's ground.
const EMBER_GROUND = hex('#050403');
const lightness = (c: Rgb) => rgbToOklab(c).L * 100;
function stepDL(a: Rgb, b: Rgb): number {
  const base = lightness(EMBER_GROUND);
  if (Math.min(lightness(a), lightness(b)) >= base) return Math.abs(lightness(a) - lightness(b));
  const ratio = contrast(a, b);
  let lo = 0;
  let hi = 1;
  for (let i = 0; i < 30; i += 1) {
    const alpha = (lo + hi) / 2;
    if (contrast(composite(WHITE, EMBER_GROUND, alpha), EMBER_GROUND) < ratio) lo = alpha;
    else hi = alpha;
  }
  return lightness(composite(WHITE, EMBER_GROUND, hi)) - base;
}

interface TokenSheet {
  id: string;
  appearance: Appearance;
  tokens: Record<ChromeColorKey, string>;
}

// The token sheets under the one ground rule (Themes review Q7, board
// 11). The One Window canvas sheets predate it, so the panel now sits
// on the ground, the lines step in lightness, and the title takes the
// secondary tone. Ember's is the sheet the board draws, and Nord's is
// the rule's with its pins. The light sheet is Rubric's from the
// shortlist, since Rubric took Vellum's place (Q14) and Vellum's sheet
// went with it. The selection is each scheme's own, opaque, with its
// own text (Q9), where the canvas drew the accent with alpha. The
// control washes (Q10) on Ember are the ones the stylesheets fixed, and
// on Nord and Rubric they are the rule's, with Rubric's field its
// raised paper.
const NORD: TokenSheet = {
  id: 'nord',
  appearance: 'dark',
  tokens: {
    bg: '#2e3440',
    panel: '#2e3440',
    sep: '#434c5e',
    divider: '#3e444f',
    selrow: '#3b4252',
    hover: '#393f4a',
    inputband: '#353b46',
    text: '#e5e9f0',
    secondary: '#c0c7d3',
    tertiary: '#7b8294',
    title: '#c0c7d3',
    raised: '#2e3440',
    accent: '#88c0d0',
    onAccent: '#1a1f2a',
    danger: '#bf616a',
    dangerText: '#dc8a92',
    warn: '#ebcb8b',
    warnText: '#ebcb8b',
    success: '#a3be8c',
    selection: '#4c566a',
    selectionText: '#eceff4',
    field: 'rgba(255, 255, 255, 0.102)',
    track: 'rgba(255, 255, 255, 0.249)',
    menuHi: 'rgba(255, 255, 255, 0.108)',
    keyRing: 'rgba(255, 255, 255, 0.219)',
    edge: 'rgba(255, 255, 255, 0.19)',
  },
};

const EMBER: TokenSheet = {
  id: 'obsidian-ember',
  appearance: 'dark',
  tokens: {
    bg: '#050403',
    panel: '#050403',
    sep: '#1b1a19',
    divider: '#100f0e',
    selrow: '#121110',
    hover: '#0b0b0a',
    inputband: '#080807',
    text: '#c0bdbb',
    secondary: '#8e8b89',
    tertiary: '#63615f',
    title: '#8e8b89',
    raised: '#100f0e',
    accent: '#ef8f2f',
    onAccent: '#140b02',
    danger: '#ea8f80',
    dangerText: '#ea8f80',
    warn: '#ecc985',
    warnText: '#ecc985',
    success: '#8fdaa8',
    selection: '#201d1c',
    selectionText: '#f2efee',
    field: 'rgba(255, 255, 255, 0.06)',
    track: 'rgba(255, 255, 255, 0.16)',
    menuHi: 'rgba(255, 255, 255, 0.08)',
    keyRing: 'rgba(255, 255, 255, 0.14)',
    edge: 'rgba(255, 255, 255, 0.12)',
  },
};

const RUBRIC: TokenSheet = {
  id: 'rubric',
  appearance: 'light',
  tokens: {
    bg: '#f0e5cf',
    panel: '#f0e5cf',
    sep: '#cbc1af',
    divider: '#dcd1bd',
    selrow: '#f5efe4',
    hover: '#e2d8c3',
    inputband: '#e7ddc7',
    text: '#151d2a',
    secondary: '#525558',
    tertiary: '#83817d',
    title: '#525558',
    raised: '#f5efe4',
    accent: '#3656b1',
    onAccent: '#ffffff',
    danger: '#e15400',
    dangerText: '#b24100',
    warn: '#5d4000',
    warnText: '#5d4000',
    success: '#007873',
    selection: '#cbc8c9',
    selectionText: '#151d2a',
    field: '#f5efe4',
    track: 'rgba(0, 0, 0, 0.146)',
    menuHi: 'rgba(0, 0, 0, 0.052)',
    keyRing: 'rgba(0, 0, 0, 0.124)',
    edge: 'rgba(0, 0, 0, 0.146)',
  },
};

// The canvas drew Ember's title in #8e8e8e on a second sheet. The title
// is the secondary tone now, so that sheet is gone.
const SHEETS: TokenSheet[] = [NORD, EMBER, RUBRIC];

describe('built-in themes reproduce the one ground boards', () => {
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
      expect(on('dangerText', panel), 'dangerText').toBeGreaterThanOrEqual(STATUS_TEXT_CONTRAST);
      expect(on('warn', panel), 'warn').toBeGreaterThanOrEqual(STATUS_CONTRAST);
      expect(on('warnText', panel), 'warnText').toBeGreaterThanOrEqual(STATUS_TEXT_CONTRAST);
      expect(on('success', panel), 'success').toBeGreaterThanOrEqual(STATUS_CONTRAST);
      // Menus, the palette, and dialogs draw the same tiers on raised.
      const raised = hex(t.raised);
      expect(on('tertiary', raised), 'tertiary on raised').toBeGreaterThanOrEqual(
        TERTIARY_CONTRAST,
      );
      expect(on('danger', raised), 'danger on raised').toBeGreaterThanOrEqual(STATUS_CONTRAST);
      expect(on('dangerText', raised), 'dangerText on raised').toBeGreaterThanOrEqual(
        STATUS_TEXT_CONTRAST,
      );
      expect(on('warn', raised), 'warn on raised').toBeGreaterThanOrEqual(STATUS_CONTRAST);
      expect(on('success', raised), 'success on raised').toBeGreaterThanOrEqual(STATUS_CONTRAST);
      expect(on('title', bg), 'title').toBeGreaterThanOrEqual(3);
      expect(on('accent', bg), 'accent').toBeGreaterThanOrEqual(3);
      expect(on('onAccent', hex(t.accent)), 'onAccent').toBeGreaterThanOrEqual(ON_ACCENT_CONTRAST);
      // The window floors of the one ground rule. The 1 px line stands
      // dL 8 to 12 off the ground, and 4 or more off a menu, where the
      // menu separators draw in it. A theme that pins its line (the High
      // Contrast pair, at 3:1) keeps it as drawn.
      if (theme.chrome?.sep === undefined) {
        const sep = stepDL(hex(t.sep), bg);
        expect(sep, 'sep off the ground').toBeGreaterThanOrEqual(8);
        expect(sep, 'sep off the ground').toBeLessThanOrEqual(12);
        expect(stepDL(hex(t.sep), raised), 'sep off raised').toBeGreaterThanOrEqual(4);
      }
      // An accent the rule picks stands apart from every status color.
      // A pinned one stays as the theme drew it.
      if (theme.chrome?.accent === undefined) {
        for (const key of ['danger', 'warn', 'success'] as const) {
          expect(deltaEOk(hex(t.accent), hex(t[key])), `accent from ${key}`).toBeGreaterThanOrEqual(
            ACCENT_APART,
          );
        }
      }
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

  // A pin keeps the accent a theme always drew. Tokyo Night, One Half
  // Dark, Tango Dark and Green Screen pin none, so the rule
  // picks a hue clear of every status color (your answer on October 4).
  it('keeps each pinned accent and lets the rule pick the rest', () => {
    const accents: Record<string, string> = {
      'obsidian-ember': '#ef8f2f',
      'kanso-zen': '#b0c8d4',
      'tokyo-night': '#bb9af7',
      nord: '#88c0d0',
      gruvbox: '#fabd2f',
      catppuccin: '#f5c2e7',
      dracula: '#bd93f9',
      monokai: '#f92672',
      'one-half-dark': '#c678dd',
      'tango-dark': '#4e9a06',
      'classic-vivid': '#ffaa00',
      'high-contrast': '#5cc8ff',
      'green-screen': '#ff55ff',
      'harbor-dark': '#2f81f7',
      'iceberg-dark': '#a093c7',
    };
    for (const [id, accent] of Object.entries(accents)) {
      expect(themeTokens(findTheme(id)).accent, id).toBe(accent);
    }
  });
});

describe('control washes', () => {
  // The surface each wash sits on and steps: the field, the track and
  // the keycap ring the panel, the menu highlight raised, the edge the
  // ground.
  const SURFACE = {
    field: 'panel',
    track: 'panel',
    keyRing: 'panel',
    menuHi: 'raised',
    edge: 'bg',
  } as const;
  type Wash = keyof typeof SURFACE;
  const WASHES = Object.keys(SURFACE) as Wash[];

  for (const theme of BUILTIN_THEMES) {
    it(`${theme.id} stands each wash its step off its surface`, () => {
      const t = themeTokens(theme);
      const steps: Partial<Record<Wash, number>> = WASH_STEP[t.appearance];
      for (const key of WASHES) {
        const step = steps[key];
        // A light field is the raised paper, below, and a pinned wash
        // stays as the theme drew it.
        if (step === undefined || theme.chrome?.[key] !== undefined) continue;
        // Below Obsidian Ember's ground a wash keeps the alpha it takes
        // on Ember's, so on Modus Vivendi's pure black it steps as it
        // does there.
        const surface = t[SURFACE[key]];
        const ground = lightness(hex(surface)) < lightness(EMBER_GROUND) ? '#050403' : surface;
        // A three place alpha and whole channels round a step by up to
        // about 0.4.
        expect(Math.abs(stepDL(paint(t[key], ground), hex(ground)) - step), key).toBeLessThan(0.5);
      }
    });
  }

  it('fields a light theme on its raised paper, never on white', () => {
    const light = BUILTIN_THEMES.map((t) => themeTokens(t)).filter((t) => t.appearance === 'light');
    expect(light.length).toBeGreaterThan(0);
    for (const t of light) {
      expect(t.field).toBe(t.raised);
      expect(t.field).not.toBe('#ffffff');
    }
  });

  it('paints Obsidian Ember within dE 1 of the washes the stylesheets fixed', () => {
    const t = themeTokens(findTheme('obsidian-ember'));
    // The edge is the ring inside a floating surface, white 0.12, which
    // board 11 draws. The window edges took 0.10 and 0.18 and move to it.
    const fixed: Record<Wash, number> = {
      field: 0.06,
      track: 0.16,
      keyRing: 0.14,
      menuHi: 0.08,
      edge: 0.12,
    };
    for (const key of WASHES) {
      const ground = t[SURFACE[key]];
      const before = composite(WHITE, hex(ground), fixed[key]);
      expect(deltaEOk(paint(t[key], ground), before), key).toBeLessThan(1);
    }
  });
});

describe('Triad and Rubric', () => {
  it('follow Obsidian Ember, which stays first as the fallback', () => {
    expect(BUILTIN_THEMES.slice(0, 3).map((t) => t.id)).toEqual([
      'obsidian-ember',
      'triad',
      'rubric',
    ]);
    expect(findTheme('gone').id).toBe('obsidian-ember');
    expect(themeTokens(findTheme('triad')).appearance).toBe('dark');
    expect(themeTokens(findTheme('rubric')).appearance).toBe('light');
  });

  it('pass all 46 game checks as they stand, so they keep no fit', () => {
    for (const id of ['triad', 'rubric']) {
      const theme = findTheme(id);
      const all = checks(theme.xterm);
      expect(all, id).toHaveLength(46);
      expect(
        all.filter((c) => !c.ok),
        id,
      ).toEqual([]);
      expect(theme.fitted, id).toBeUndefined();
    }
  });

  it('take Nercuros cyan and lapis as the accent', () => {
    expect(themeTokens(findTheme('triad')).accent).toBe('#44d4e2');
    expect(themeTokens(findTheme('rubric')).accent).toBe('#3656b1');
  });
});

describe('retired themes', () => {
  afterEach(() => setCustomThemes([]));

  it('give each retired id a successor Vosh ships (Q13, Q14 and Q16)', () => {
    expect([...RETIRED_THEMES]).toEqual([
      ['one-dark', 'one-half-dark'],
      ['vellum', 'rubric'],
      ['everforest-light', 'melange-light'],
    ]);
    const ids = BUILTIN_THEMES.map((t) => t.id);
    for (const [retired, successor] of RETIRED_THEMES) {
      expect(ids, retired).not.toContain(retired);
      expect(findTheme(retired).id, retired).toBe(successor);
    }
  });

  it('paint a saved vellum in the tokens of Rubric', () => {
    expect(themeTokens(findTheme('vellum'))).toEqual(themeTokens(findTheme('rubric')));
  });

  it('let a custom theme with a retired id win over its successor', () => {
    const mine = customToAppTheme({
      id: 'vellum',
      label: 'My Vellum',
      description: '',
      xterm: { background: '#f7f4ee', foreground: '#2a2622' },
      chrome: {},
    });
    setCustomThemes([mine]);
    expect(findTheme('vellum').label).toBe('My Vellum');
    expect(themeShownBy([...BUILTIN_THEMES, mine], 'vellum')).toBe(mine);
    expect(findTheme('one-dark').id).toBe('one-half-dark');
  });

  it('find nothing for an id no theme has and none retired', () => {
    expect(themeShownBy(BUILTIN_THEMES, 'gone')).toBeUndefined();
    expect(findTheme('gone').id).toBe('obsidian-ember');
  });
});

describe('Everforest and Green Screen', () => {
  const NEW_THEMES = ['everforest-dark', 'green-screen'];
  // Black and bright black stay near the ground on purpose in many
  // palettes, Everforest's own mapping included. Every other slot draws
  // game text.
  const WORD_SLOTS = [
    'foreground',
    'red',
    'green',
    'yellow',
    'blue',
    'magenta',
    'cyan',
    'white',
    'brightRed',
    'brightGreen',
    'brightYellow',
    'brightBlue',
    'brightMagenta',
    'brightCyan',
    'brightWhite',
  ] as const;
  const hue = (h: string) => rgbToOklch(hex(h)).h;

  it('are built in and sort into the light and dark lists by their ground', () => {
    const ids = BUILTIN_THEMES.map((t) => t.id);
    for (const id of NEW_THEMES) expect(ids, id).toContain(id);
    expect(themeTokens(findTheme('everforest-dark')).appearance).toBe('dark');
    expect(themeTokens(findTheme('green-screen')).appearance).toBe('dark');
  });

  // The colors CGA publishes under 3:1 on its own ground. The terminal
  // draws them as published. The chat pane lifts them where it draws
  // them on the panel (chatColors.test.ts).
  const PUBLISHED_FAINT: Record<string, readonly (typeof WORD_SLOTS)[number][]> = {
    'green-screen': ['red', 'blue'],
  };

  it('draw every other game color at 3:1 or better on the terminal ground', () => {
    for (const id of NEW_THEMES) {
      const x = findTheme(id).xterm;
      const ground = hex(x.background);
      const faint: readonly string[] = PUBLISHED_FAINT[id] ?? [];
      for (const slot of WORD_SLOTS) {
        const ratio = contrast(hex(x[slot]), ground);
        if (faint.includes(slot)) expect(ratio, `${id} ${slot}`).toBeLessThan(3);
        else expect(ratio, `${id} ${slot}`).toBeGreaterThanOrEqual(3);
      }
    }
  });

  it('keep the published Everforest and CGA colors', () => {
    const everforest: Partial<XtermPalette> = {
      red: '#e67e80',
      green: '#a7c080',
      yellow: '#dbbc7f',
      blue: '#7fbbb3',
      magenta: '#d699b6',
      cyan: '#83c092',
    };
    // Everforest repeats the six colors in the bright slots.
    const brights = Object.fromEntries(
      Object.entries(everforest).map(([slot, value]) => [
        `bright${slot[0].toUpperCase()}${slot.slice(1)}`,
        value,
      ]),
    );
    expect(findTheme('everforest-dark').xterm).toMatchObject({ ...everforest, ...brights });
    expect(findTheme('green-screen').xterm).toMatchObject({
      black: '#000000',
      red: '#aa0000',
      green: '#00aa00',
      yellow: '#aa5500',
      blue: '#0000aa',
      magenta: '#aa00aa',
      cyan: '#00aaaa',
      white: '#aaaaaa',
      brightBlack: '#555555',
      brightRed: '#ff5555',
      brightGreen: '#55ff55',
      brightYellow: '#ffff55',
      brightBlue: '#5555ff',
      brightMagenta: '#ff55ff',
      brightCyan: '#55ffff',
      brightWhite: '#ffffff',
    });
  });

  it('store the Everforest selection as its bg_visual', () => {
    expect(findTheme('everforest-dark').xterm.selectionBackground).toBe('#543a48');
  });

  it('take Everforest green as the accent', () => {
    expect(themeTokens(findTheme('everforest-dark')).accent).toBe('#a7c080');
  });

  it('give Green Screen soft phosphor text on a green black ground', () => {
    const x = findTheme('green-screen').xterm;
    const ground = rgbToOklch(hex(x.background));
    expect(ground.L).toBeLessThan(0.2);
    expect(ground.C).toBeGreaterThan(0);
    expect(ground.C).toBeLessThan(0.02);
    expect(Math.abs(ground.h - 145)).toBeLessThan(20);
    // Green, and well short of the 14:1 and up glare of #00ff00.
    const text = contrast(hex(x.foreground), hex(x.background));
    expect(text).toBeGreaterThanOrEqual(10);
    expect(text).toBeLessThan(12);
    expect(Math.abs(hue(x.foreground) - hue('#00ff00'))).toBeLessThan(10);
    // The cursor and the selection glow green. The cursor sits too near
    // the success tone to be the accent, so the rule takes the magenta.
    const t = themeTokens(findTheme('green-screen'));
    expect(t.accent).toBe('#ff55ff');
    for (const green of [x.cursor, x.selectionBackground]) {
      expect(Math.abs(hue(green) - hue('#00ff00')), green).toBeLessThan(10);
    }
  });
});

describe('Solarized', () => {
  // Ethan Schoonover's published values.
  const SOL = {
    base03: '#002b36',
    base02: '#073642',
    base01: '#586e75',
    base00: '#657b83',
    base0: '#839496',
    base1: '#93a1a1',
    base2: '#eee8d5',
    base3: '#fdf6e3',
    yellow: '#b58900',
    orange: '#cb4b16',
    red: '#dc322f',
    magenta: '#d33682',
    violet: '#6c71c4',
    blue: '#268bd2',
    cyan: '#2aa198',
    green: '#859900',
  };
  const dark = findTheme('solarized-dark');
  const light = findTheme('solarized-light');
  const BOTH = [
    { theme: dark, appearance: 'dark', bg: SOL.base03, fg: SOL.base0, strong: 1 },
    { theme: light, appearance: 'light', bg: SOL.base3, fg: SOL.base00, strong: -1 },
  ] as const;
  const HUED = [
    ['green', 'brightGreen'],
    ['yellow', 'brightYellow'],
    ['blue', 'brightBlue'],
    ['cyan', 'brightCyan'],
  ] as const;

  it('ships a dark and a light theme', () => {
    for (const { theme, appearance } of BOTH) {
      expect(theme.id).toBe(`solarized-${appearance}`);
      expect(themeTokens(theme).appearance).toBe(appearance);
    }
  });

  it('keeps the published ground, text, accent, and normal colors', () => {
    for (const { theme, bg, fg } of BOTH) {
      expect(theme.xterm).toMatchObject({
        background: bg,
        foreground: fg,
        black: SOL.base02,
        red: SOL.red,
        green: SOL.green,
        yellow: SOL.yellow,
        blue: SOL.blue,
        magenta: SOL.magenta,
        cyan: SOL.cyan,
        brightRed: SOL.orange,
        brightMagenta: SOL.violet,
      });
      expect(themeTokens(theme).accent, theme.id).toBe(SOL.blue);
    }
    expect(dark.xterm.white).toBe(SOL.base2);
  });

  it('gives the hued brights a hue instead of a grey base tone', () => {
    const greys = new Set([SOL.base01, SOL.base00, SOL.base0, SOL.base1]);
    for (const { theme, strong } of BOTH) {
      for (const [normal, bright] of HUED) {
        const label = `${theme.id} ${bright}`;
        expect(greys.has(theme.xterm[bright]), label).toBe(false);
        const n = rgbToOklch(hex(theme.xterm[normal]));
        const b = rgbToOklch(hex(theme.xterm[bright]));
        // The normal hue, still a color, a step toward the strong end.
        expect(Math.abs(b.h - n.h), label).toBeLessThan(5);
        expect(b.C, label).toBeGreaterThan(0.08);
        expect(Math.sign(b.L - n.L), label).toBe(strong);
      }
    }
  });

  it('draws danger in Solarized red, not the orange in bright red', () => {
    const red = rgbToOklch(hex(SOL.red));
    const orange = rgbToOklch(hex(SOL.orange));
    for (const { theme } of BOTH) {
      const tokens = themeTokens(theme);
      for (const key of ['danger', 'dangerText'] as const) {
        const h = rgbToOklch(hex(tokens[key])).h;
        const label = `${theme.id} ${key}`;
        expect(Math.abs(h - red.h), label).toBeLessThan(3);
        expect(Math.abs(h - red.h), label).toBeLessThan(Math.abs(h - orange.h));
      }
    }
  });

  it('keeps the dark theme out of Fit game colors and says why (Q20)', () => {
    expect(dark.fitGameColors).toBe(false);
    expect(dark.fitted).toBeUndefined();
    expect(dark.description).toContain('low contrast by design');
    expect(light.fitGameColors).toBeUndefined();
    expect(BUILTIN_THEMES.filter((t) => t.fitGameColors === false).map((t) => t.id)).toEqual([
      'solarized-dark',
      'high-contrast',
    ]);
  });

  it('keeps every slot game text reads off its ground', () => {
    const slots = [
      'white',
      'brightBlack',
      'brightGreen',
      'brightYellow',
      'brightBlue',
      'brightCyan',
      'brightWhite',
    ] as const;
    for (const { theme } of BOTH) {
      const bg = hex(theme.xterm.background);
      for (const slot of slots) {
        expect(contrast(hex(theme.xterm[slot]), bg), `${theme.id} ${slot}`).toBeGreaterThan(2);
      }
    }
  });
});

// The fits are computed ahead by theme/gameFit, and gameFit.test.ts fits
// each again with VOSH_FIT_THEMES=1. These pin what the decisions say of them.
describe('fitted game colors', () => {
  const inPlay = (id: string) => playPalette(findTheme(id), true);
  const misses = (id: string) => checks(inPlay(id)).filter((c) => !c.ok);
  const value = (id: string, check: string) =>
    checks(inPlay(id)).find((c) => c.id === check)?.value;

  it('draws play in the fitted slots while Fit game colors is on (Q2)', () => {
    const kanso = findTheme('kanso-zen');
    expect(inPlay('kanso-zen')).toEqual({ ...kanso.xterm, ...kanso.fitted });
    expect(inPlay('kanso-zen').foreground).toBe('#c9cdcb');
    expect(inPlay('kanso-zen').brightBlack).toBe('#92979d');
    expect(inPlay('kanso-zen').brightWhite).toBe('#f0f5f2');
    expect(inPlay('kanso-zen').background).toBe(kanso.xterm.background);
    // Off, play draws the theme as published.
    for (const theme of BUILTIN_THEMES)
      expect(playPalette(theme, false), theme.id).toBe(theme.xterm);
    // Solarized Dark keeps out, so play draws it as published (Q20).
    const dark = findTheme('solarized-dark');
    expect(playPalette(dark, true)).toBe(dark.xterm);
  });

  it('draws a custom theme in the fit it kept, and as published without one', () => {
    const base = { id: 'dusk', label: 'Dusk', description: '', chrome: {} };
    const xterm = { background: '#1a1b26', foreground: '#c0caf5' };
    const kept = customToAppTheme({ ...base, xterm, fitted: { red: '#cb7b74' } });
    expect(kept.fitted).toEqual({ red: '#cb7b74' });
    expect(playPalette(kept, true)).toEqual({ ...kept.xterm, red: '#cb7b74' });
    expect(playPalette(kept, false)).toBe(kept.xterm);
    const none = customToAppTheme({ ...base, xterm });
    expect(none.fitted).toBeUndefined();
    expect(playPalette(none, true)).toBe(none.xterm);
  });

  it('stores only the slots the fit moved, from body text and the 16 colors', () => {
    for (const theme of BUILTIN_THEMES) {
      for (const [slot, hex] of Object.entries(theme.fitted ?? {})) {
        expect(GAME_SLOTS, `${theme.id} ${slot}`).toContain(slot);
        expect(hex, `${theme.id} ${slot}`).toMatch(/^#[0-9a-f]{6}$/);
        expect(hex, `${theme.id} ${slot}`).not.toBe(theme.xterm[slot as keyof XtermPalette]);
      }
    }
  });

  it('lifts Kanso Zen to 44 of 46 in play and keeps its palette (Q15)', () => {
    const kanso = findTheme('kanso-zen');
    expect(Object.keys(kanso.fitted ?? {})).toHaveLength(16);
    expect(kanso.xterm.brightBlack).toBe('#5c6066');
    expect(misses('kanso-zen')).toHaveLength(2);
  });

  it('accepts what Classic Vivid and Green Screen still miss (Q17)', () => {
    expect(misses('classic-vivid')).toHaveLength(6);
    expect(value('classic-vivid', 'T3 blue Lc')).toBe(39);
    expect(value('classic-vivid', 'T3 red Lc')).toBe(40.7);
    expect(value('green-screen', 'T3 blue Lc')).toBe(45.1);
    expect(misses('green-screen').map((c) => `${c.id} ${c.value}`)).toEqual(
      expect.arrayContaining(['T2 cyan Lc 53.8', 'T2 brightBlue Lc 58.8']),
    );
  });

  it('lifts Tango Dark blue and magenta and leaves red at Lc 36.2 (Q18)', () => {
    expect(misses('tango-dark')).toHaveLength(4);
    expect(value('tango-dark', 'T3 blue Lc')).toBeGreaterThanOrEqual(45);
    expect(value('tango-dark', 'T3 magenta Lc')).toBeGreaterThanOrEqual(45);
    expect(value('tango-dark', 'T3 red Lc')).toBe(36.2);
  });

  // High Contrast was the sixth until Board 14 took it out of the fit,
  // so its body text stays white.
  it('sets bright white dL 8 above body text in the five of Q19 still fitted', () => {
    for (const id of ['monokai', 'rose-pine', 'everforest-dark', 'tokyo-night', 'gruvbox']) {
      expect(value(id, 'T6 fg/brightWhite dL'), id).toBeGreaterThanOrEqual(8);
    }
    expect(findTheme('monokai').fitted?.foreground).toBe('#e4e4df');
    expect(playPalette(findTheme('high-contrast'), true).foreground).toBe('#ffffff');
  });

  it('leaves the new schemes short where the review said (Q1)', () => {
    const missed = (id: string) => misses(id).map((c) => `${c.id} ${c.value}`);
    expect(missed('srcery')).toEqual(['T3 red Lc 40.3']);
    expect(missed('nightfly')).toEqual(['T3 red Lc 38.1', 'T6 yellow pair dE 7.9']);
    expect(missed('melange-dark')).toEqual(['T2 yellow Lc 58.5', 'T3 red Lc 41.5']);
    expect(missed('melange-light')).toEqual([]);
    expect(missed('modus-vivendi')).toEqual(['T3 red Lc 43.5']);
    // Melange Light passes 35 as published.
    expect(checks(findTheme('melange-light').xterm).filter((c) => !c.ok)).toHaveLength(11);
  });

  // The palettes the Themes review read into shortlist.json and the fits
  // its survey computed (metrics/fit-survey.json, github-dark-default and
  // iceberg-dark), which you picked on October 5.
  it('ships Harbor Dark and Iceberg Dark as the review drew them', () => {
    const harbor = findTheme('harbor-dark');
    const iceberg = findTheme('iceberg-dark');
    expect(BUILTIN_THEMES.slice(-3).map((t) => t.id)).toEqual([
      'modus-vivendi',
      'harbor-dark',
      'iceberg-dark',
    ]);
    expect([harbor.label, harbor.source, harbor.author]).toEqual([
      'Harbor Dark',
      'GitHub Dark Default',
      'GitHub',
    ]);
    expect([iceberg.label, iceberg.source, iceberg.author]).toEqual([
      'Iceberg Dark',
      'Iceberg',
      'cocopon',
    ]);
    expect(Object.values(harbor.xterm).join(' ')).toBe(
      '#0d1117 #e6edf3 #2f81f7 #0d1117 #343941 #e6edf3 #484f58 #ff7b72 #3fb950 #d29922 ' +
        '#58a6ff #bc8cff #39c5cf #b1bac4 #6e7681 #ffa198 #56d364 #e3b341 #79c0ff #d2a8ff ' +
        '#56d4dd #ffffff',
    );
    expect(Object.values(iceberg.xterm).join(' ')).toBe(
      '#161821 #c6c8d1 #c6c8d1 #161821 #272c42 #c6c8d1 #1e2132 #e27878 #b4be82 #e2a478 ' +
        '#84a0c6 #a093c7 #89b8c2 #c6c8d1 #6b7089 #e98989 #c0ca8e #e9b189 #91acd1 #ada0d3 ' +
        '#95c4ce #d2d4de',
    );
    // The review counts 26 and 15 of 46 as published.
    expect(checks(harbor.xterm).filter((c) => c.ok)).toHaveLength(26);
    expect(checks(iceberg.xterm).filter((c) => c.ok)).toHaveLength(15);
    // The survey moves 11 and 15 slots and leaves these short.
    expect(Object.keys(harbor.fitted ?? {})).toHaveLength(11);
    expect(Object.keys(iceberg.fitted ?? {})).toHaveLength(15);
    const missed = (id: string) => misses(id).map((c) => `${c.id} ${c.value}`);
    expect(missed('harbor-dark')).toEqual(['T3 red Lc 40.8']);
    expect(missed('iceberg-dark')).toEqual(['T2 yellow Lc 58.1', 'T3 red Lc 37.8']);
  });
});

describe('color vision swaps', () => {
  const OTHER = ['deuteranopia', 'protanopia', 'tritanopia'] as const;
  type Other = (typeof OTHER)[number];
  type Slot = (typeof GAME_SLOTS)[number];
  // FNV-1a over a JSON string, to pin a large value in a few characters.
  const digest = (text: string) => {
    let h = 0x811c9dc5;
    for (let i = 0; i < text.length; i++) {
      h ^= text.charCodeAt(i);
      h = Math.imul(h, 0x01000193) >>> 0;
    }
    return h.toString(16).padStart(8, '0');
  };
  // Board 14 rebuilt High Contrast, so the digests below hold every
  // other theme to the commit they name.
  const REBUILT = ['high-contrast'];
  const kept = (themes: readonly AppTheme[]) => themes.filter((t) => !REBUILT.includes(t.id));
  const sees = (p: XtermPalette, a: Slot, b: Slot, vision: ColorVision) =>
    seenApart(hex(p[a]), hex(p[b]), vision);
  const apart = (p: XtermPalette, a: Slot, b: Slot) => deltaEOk(hex(p[a]), hex(p[b]));
  const lch = (c: string) => rgbToOklch(hex(c));
  // How far a check falls outside its target, as the swap measures it.
  const gap = (need: string, value: number) => {
    if (need.startsWith('>=')) return Math.max(0, +need.slice(2) - value);
    const [a, b] = need.split('..').map(Number);
    return Math.max(0, a - value, value - b);
  };
  // How far hue `h` sits from `target`, in degrees either way.
  const hueOff = (h: number, target: number) => ((h - target + 540) % 360) - 180;

  // Every built in theme under each other vision, from each start it
  // plays: the Typical fit while Fit game colors is on, and the
  // published colors while it is off. A theme whose Typical fit is
  // empty, and Solarized Dark, play the published start alone.
  interface Case {
    at: string;
    theme: AppTheme;
    vision: Other;
    fit: boolean;
    start: XtermPalette;
    own: XtermPalette;
  }
  const CASES: Case[] = BUILTIN_THEMES.flatMap((theme) =>
    OTHER.flatMap((vision) => {
      const fitted = Object.keys(typicalStart(theme, true)).length > 0;
      return (fitted ? [true, false] : [false]).map((fit) => ({
        at: `${vision} ${theme.id} ${fit ? 'fitted' : 'published'}`,
        theme,
        vision,
        fit,
        start: playPalette(theme, fit),
        own: playPalette(theme, fit, vision),
      }));
    }),
  );

  // The 24 themes one-window (c5a6ebd0) shipped, less the rebuilt ones,
  // their Typical fits and their play palettes with Fit game colors on,
  // digested from that commit's themes.ts. Typical plays them byte for
  // byte as it did.
  it('plays Typical byte for byte as before color vision', () => {
    const before = kept(BUILTIN_THEMES).filter(
      (t) => !['harbor-dark', 'iceberg-dark'].includes(t.id),
    );
    expect(before).toHaveLength(23);
    expect(digest(JSON.stringify(before.map((t) => [t.id, t.fitted ?? null])))).toBe('4a76b97c');
    expect(digest(JSON.stringify(before.map((t) => [t.id, playPalette(t, true)])))).toBe(
      '8fc72860',
    );
    for (const theme of BUILTIN_THEMES) {
      for (const fit of [true, false]) {
        expect(playPalette(theme, fit, 'typical'), theme.id).toEqual(playPalette(theme, fit));
      }
      expect(visionFitOf(theme, 'typical'), theme.id).toBe(theme.fitted);
    }
  });

  // The 26 themes one-window (a206426c) ships, less the rebuilt ones,
  // their Typical fits, their play palettes with Fit game colors on and
  // their window tokens, digested from that commit. Color vision changes none of them.
  it('fits and paints Typical byte for byte as at a206426c', () => {
    const shipped = kept(BUILTIN_THEMES);
    expect(shipped).toHaveLength(25);
    expect(digest(JSON.stringify(shipped.map((t) => [t.id, t.fitted ?? null])))).toBe('6dc6293a');
    expect(digest(JSON.stringify(shipped.map((t) => [t.id, playPalette(t, true)])))).toBe(
      '3fcb0e5b',
    );
    expect(digest(JSON.stringify(shipped.map((t) => [t.id, themeTokens(t)])))).toBe('f4935d5a');
    for (const theme of BUILTIN_THEMES) {
      expect(themeTokens(theme, 'typical'), theme.id).toEqual(themeTokens(theme));
    }
  });

  it('stores only the slots each swap moved, in hex', () => {
    for (const c of CASES) {
      for (const [slot, color] of Object.entries(visionFitOf(c.theme, c.vision, c.fit) ?? {})) {
        expect(GAME_SLOTS, `${c.at} ${slot}`).toContain(slot);
        expect(color, `${c.at} ${slot}`).toMatch(/^#[0-9a-f]{6}$/);
        expect(color, `${c.at} ${slot}`).not.toBe(c.theme.xterm[slot as Slot]);
      }
    }
  });

  // The swap starts from what Typical plays: the Typical fit with Fit
  // game colors on, and the published colors with it off or on Solarized
  // Dark. It moves only the twelve cue colors, so body text, white, bold
  // white, black and bold black play as the start has them.
  it('starts from the palette Typical plays and moves only the cue colors', () => {
    for (const c of CASES) {
      expect(c.start, c.at).toEqual({ ...c.theme.xterm, ...typicalStart(c.theme, c.fit) });
      for (const slot of GAME_SLOTS.filter((k) => !CUE_SLOTS.includes(k as AnsiSlot))) {
        expect(c.own[slot], `${c.at} ${slot}`).toBe(c.start[slot]);
      }
      expect(
        CUE_SLOTS.some((k) => c.own[k] !== c.start[k]),
        c.at,
      ).toBe(true);
    }
    for (const theme of BUILTIN_THEMES) {
      if (Object.keys(typicalStart(theme, true)).length > 0) continue;
      for (const vision of OTHER) {
        expect(playPalette(theme, true, vision), `${vision} ${theme.id}`).toEqual(
          playPalette(theme, false, vision),
        );
      }
    }
    // Solarized Dark keeps out of Fit game colors under Typical, and
    // swaps from its published colors under every other vision.
    const dark = findTheme('solarized-dark');
    expect(playPalette(dark, true)).toBe(dark.xterm);
    for (const vision of OTHER) {
      expect(playPalette(dark, true, vision)).not.toEqual(dark.xterm);
    }
  });

  // Whether `slot` of `p` sits in the window its family turns to, give
  // or take the rounding to a hex color, which bends the hue of a pale
  // color more. A family the vision does not turn sits in none.
  const inWindow = (vision: Other, p: XtermPalette, slot: AnsiSlot) => {
    const target = SWAP_TARGETS[vision][familyOf(slot)];
    if (!target) return false;
    const now = lch(p[slot]);
    const slack = 1 + 0.2 / Math.max(now.C, 0.01);
    return Math.abs(hueOff(now.h, target.hue)) <= target.reach + slack;
  };
  // How far the swap moves `slot` of `p`, as a typical eye sees it,
  // where the slot turned into its window. A turned family that keeps
  // its own hue counts as no move.
  const turnedMove = (c: Case, p: XtermPalette, slot: AnsiSlot) =>
    inWindow(c.vision, p, slot) ? deltaEOk(hex(c.start[slot]), hex(p[slot])) : 0;
  // The chroma `slot` of `p` keeps at least: a turned color CHROMA_KEEP
  // of its start chroma or of its target's, and a color that keeps its
  // hue HUE_CHROMA_KEEP of its start chroma.
  const chromaFloor = (c: Case, p: XtermPalette, slot: AnsiSlot) => {
    const target = SWAP_TARGETS[c.vision][familyOf(slot)];
    const from = lch(c.start[slot]).C;
    if (target && inWindow(c.vision, p, slot)) return CHROMA_KEEP * Math.min(from, target.chroma);
    return from > 0.04 ? HUE_CHROMA_KEEP * from : 0;
  };

  // The colors of a family the vision turns that keep their own hue,
  // because no hue in their window holds the firm floors: on Catppuccin
  // and Tokyo Night the theme's own text, yells and cabal fill the
  // blues, so tells keep their green, and on many fitted themes newbie
  // chat in bold green sits so near white that as a blue it would run
  // into cabal, clan or body text.
  const KEEP_HUE: Record<string, string> = {
    'deuteranopia obsidian-ember fitted': 'brightGreen',
    'protanopia obsidian-ember fitted': 'brightGreen',
    'protanopia obsidian-ember published': 'brightGreen',
    'tritanopia obsidian-ember fitted': 'brightBlue',
    'deuteranopia triad published': 'brightGreen',
    'protanopia triad published': 'brightGreen',
    'deuteranopia tokyo-night fitted': 'red green brightGreen',
    'deuteranopia tokyo-night published': 'green brightGreen',
    'protanopia tokyo-night fitted': 'green brightGreen',
    'protanopia tokyo-night published': 'green brightGreen',
    'deuteranopia nord fitted': 'blue',
    'deuteranopia rose-pine fitted': 'red',
    'deuteranopia rose-pine published': 'red',
    'protanopia rose-pine fitted': 'red brightBlue',
    'protanopia rose-pine published': 'red blue',
    'tritanopia gruvbox fitted': 'brightBlue',
    'deuteranopia catppuccin fitted': 'brightGreen',
    'deuteranopia catppuccin published': 'green brightGreen',
    'protanopia catppuccin fitted': 'green brightGreen',
    'protanopia catppuccin published': 'green brightGreen',
    'protanopia dracula fitted': 'brightGreen',
    'deuteranopia monokai fitted': 'blue',
    'protanopia monokai fitted': 'red brightGreen brightBlue',
    'protanopia monokai published': 'red blue',
    'deuteranopia one-half-dark fitted': 'brightGreen',
    'protanopia one-half-dark fitted': 'red brightGreen brightBlue',
    'tritanopia one-half-dark fitted': 'magenta',
    'protanopia solarized-dark published': 'red',
    'deuteranopia tango-dark fitted': 'blue brightGreen',
    'protanopia tango-dark fitted': 'blue brightGreen',
    'deuteranopia classic-vivid fitted': 'brightGreen',
    'protanopia classic-vivid fitted': 'brightGreen',
    'deuteranopia high-contrast published': 'brightGreen',
    'protanopia high-contrast published': 'brightGreen brightBlue',
    'deuteranopia everforest-dark fitted': 'red',
    'tritanopia everforest-dark fitted': 'brightBlue',
    'deuteranopia green-screen fitted': 'blue',
    'protanopia green-screen fitted': 'blue',
    'tritanopia srcery fitted': 'brightBlue',
    'deuteranopia nightfly published': 'brightGreen',
    'protanopia nightfly fitted': 'brightGreen',
    'protanopia nightfly published': 'brightGreen',
    'deuteranopia melange-dark fitted': 'red',
    'deuteranopia melange-dark published': 'red',
    'protanopia melange-light published': 'red',
    'deuteranopia modus-vivendi published': 'brightBlue',
    'protanopia modus-vivendi fitted': 'brightGreen',
    'deuteranopia harbor-dark fitted': 'red brightGreen',
    'protanopia harbor-dark fitted': 'blue brightGreen',
    'protanopia harbor-dark published': 'brightBlue',
    'deuteranopia iceberg-dark fitted': 'brightGreen',
    'deuteranopia iceberg-dark published': 'brightBlue',
    'protanopia iceberg-dark fitted': 'brightGreen brightBlue',
    'protanopia iceberg-dark published': 'brightGreen',
  };

  it('turns each family into its window, or keeps its hue where KEEP_HUE names it', () => {
    const kept: Record<string, string> = {};
    for (const c of CASES) {
      const own: string[] = [];
      for (const slot of CUE_SLOTS) {
        if (inWindow(c.vision, c.own, slot)) continue;
        if (SWAP_TARGETS[c.vision][familyOf(slot)]) own.push(slot);
        if (c.own[slot] !== c.start[slot] && lch(c.start[slot]).C >= 0.04) {
          const off = Math.abs(hueOff(lch(c.own[slot]).h, lch(c.start[slot]).h));
          expect(off, `${c.at} ${slot}`).toBeLessThan(3);
        }
      }
      if (own.length > 0) kept[c.at] = own.join(' ');
    }
    expect(kept).toEqual(KEEP_HUE);
  });

  // No swap gives up a check its start passes or falls more than
  // MISS_SLACK further short of one it misses, leaving out the T7 pairs
  // another vision sees through. No color moves more than L_REACH in
  // lightness, red keeps its side of yellow and bold yellow, and every
  // color keeps its chroma floor (chromaFloor).
  it('holds every check, the reach in lightness, red against yellow and the chroma', () => {
    for (const c of CASES) {
      const held = checks(c.start);
      checks(c.own).forEach((check, i) => {
        if (!holdsCheck(check.id, c.vision)) return;
        const at = `${c.at} ${check.id} ${held[i].value} to ${check.value}`;
        if (held[i].ok) expect(check.ok, at).toBe(true);
        else {
          expect(gap(check.need, check.value), at).toBeLessThanOrEqual(
            gap(held[i].need, held[i].value) + MISS_SLACK,
          );
        }
      });
      for (const slot of CUE_SLOTS) {
        const at = `${c.at} ${slot}`;
        const moved = Math.abs(lch(c.own[slot]).L - lch(c.start[slot]).L);
        expect(moved, at).toBeLessThanOrEqual(L_REACH + 1e-9);
        expect(lch(c.own[slot]).C, at).toBeGreaterThanOrEqual(chromaFloor(c, c.own, slot) - 1e-9);
      }
      for (const y of ['yellow', 'brightYellow'] as const) {
        const was = lch(c.start.red).L - lch(c.start[y]).L;
        if (Math.abs(was) < 0.02) continue;
        const now = lch(c.own.red).L - lch(c.own[y]).L;
        expect(Math.sign(now), `${c.at} red/${y}`).toBe(Math.sign(was));
      }
    }
  });

  // Every two channels, newbie chat and immortal talk among them, stand
  // CHANNEL_LEAST apart on every theme, or as far as at the start if that
  // is less, as the vision sees them and as a typical eye does.
  it('keeps every two channels CHANNEL_LEAST apart on every theme', () => {
    for (const c of CASES) {
      for (const [a, b] of CHANNEL_PAIRS) {
        const at = `${c.at} ${a}/${b}`;
        const seen = Math.min(sees(c.start, a, b, c.vision), CHANNEL_LEAST);
        expect(sees(c.own, a, b, c.vision), at).toBeGreaterThanOrEqual(seen - 1e-9);
        const typical = Math.min(apart(c.start, a, b), CHANNEL_LEAST);
        expect(apart(c.own, a, b), `${at} typical`).toBeGreaterThanOrEqual(typical - 1e-9);
      }
    }
  });

  // The floors and targets of the swap below the firm ones, firmest
  // first (gameFit swapFor): each color clear of body text, white and
  // bold white, the kept pairs, a turned color's move, the lead color's
  // move, the channel and cue floors and the parted pairs. Each reads
  // `p`, the palette in play, against the start. The channels at
  // CHANNEL_LEAST stand with the checks, firmest of all.
  interface Rule {
    id: string;
    tier: number;
    slots: readonly Slot[];
    value: (p: XtermPalette) => number;
    need: number;
  }
  const rulesOf = (c: Case): Rule[] => {
    const { start, vision } = c;
    const out: Rule[] = [];
    const pair = (
      id: string,
      tier: number,
      [a, b]: readonly [Slot, Slot],
      most: number,
      less = 0,
    ) => {
      out.push({
        id: `${id} ${a}/${b}`,
        tier,
        slots: [a, b],
        value: (p) => sees(p, a, b, vision),
        need: Math.min(sees(start, a, b, vision), most) - less,
      });
      out.push({
        id: `${id} ${a}/${b} typical`,
        tier,
        slots: [a, b],
        value: (p) => apart(p, a, b),
        need: Math.min(apart(start, a, b), most) - less,
      });
    };
    for (const p of CHANNEL_PAIRS) pair('least', 0, p, CHANNEL_LEAST);
    for (const k of CUE_SLOTS) {
      for (const t of TEXT_SLOTS) pair('text', 1, [k, t], VISION_GUARD, VISION_SLACK);
    }
    for (const [a, b] of KEPT_PAIRS[vision]) {
      out.push({
        id: `kept ${a}/${b}`,
        tier: 1,
        slots: [a, b],
        value: (p) => sees(p, a, b, vision),
        need: sees(start, a, b, vision) - KEPT_SLACK,
      });
    }
    const plain = CUE_SLOTS.slice(0, 6).filter((k) => SWAP_TARGETS[vision][familyOf(k)]);
    out.push({
      id: 'show',
      tier: 2,
      slots: plain,
      value: (p) => Math.max(...plain.map((k) => turnedMove(c, p, k))),
      need: MOVE_MIN.lead,
    });
    LEAD_SLOTS[vision].forEach((k, i) => {
      out.push({
        id: `move ${k}`,
        tier: 3,
        slots: [k],
        value: (p) => turnedMove(c, p, k),
        need: i === 0 ? MOVE_MIN.lead : MOVE_MIN.bold,
      });
    });
    for (const p of CHANNEL_PAIRS) pair('channel', 5, p, CHANNEL_FLOOR);
    for (const p of CUE_PAIRS) pair('cue', 5, p, CUE_FLOOR);
    for (const [a, b] of PARTED_PAIRS[vision]) {
      out.push({
        id: `part ${a}/${b}`,
        tier: 6,
        slots: [a, b],
        value: (p) => sees(p, a, b, vision),
        need: Math.min(PART_MIN[vision], apart(start, a, b)) - VISION_SLACK,
      });
    }
    return out;
  };
  // What stops each step that would bring a short rule nearer its
  // target: a firmer rule, or one as firm, that the step breaks, the
  // window a turned hue may not leave, or a check, the reach in
  // lightness or the chroma the swap holds. Where nothing stops it the
  // search missed the step.
  const blockersOf = (c: Case, rules: Rule[], short: Rule[]) => {
    const out = new Set<string>();
    const held = checks(c.start);
    const holds = (p: XtermPalette) => {
      const why: string[] = [];
      checks(p).forEach((check, i) => {
        if (!holdsCheck(check.id, c.vision)) return;
        const miss = held[i].ok
          ? !check.ok
          : gap(check.need, check.value) > gap(held[i].need, held[i].value) + MISS_SLACK;
        if (miss) why.push(check.id);
      });
      for (const slot of CUE_SLOTS) {
        if (Math.abs(lch(p[slot]).L - lch(c.start[slot]).L) > L_REACH) why.push(`${slot} reach`);
        if (lch(p[slot]).C < chromaFloor(c, p, slot)) why.push(`${slot} chroma`);
      }
      return why;
    };
    for (const r of short) {
      const now = r.value(c.own);
      for (const k of r.slots) {
        if (!CUE_SLOTS.includes(k as AnsiSlot)) continue;
        // Steps the swap could take: lightness, and for a turned color its
        // hue, or for a color of a turned family that keeps its hue, the
        // turn to its target.
        const target = SWAP_TARGETS[c.vision][familyOf(k as AnsiSlot)];
        const from = lch(c.start[k]);
        const turned = inWindow(c.vision, c.own, k as AnsiSlot);
        const o = {
          L: lch(c.own[k]).L,
          C: target && turned ? Math.max(from.C, target.chroma) : from.C,
          h: target && turned ? lch(c.own[k]).h : from.h,
        };
        const steps = [-0.02, 0.02].map((d) => ({ ...o, L: Math.max(0, Math.min(1, o.L + d)) }));
        if (target && turned) steps.push({ ...o, h: o.h - 2.5 }, { ...o, h: o.h + 2.5 });
        if (target && !turned) {
          for (const side of [-1, 0, 1]) {
            const C = Math.max(from.C, target.chroma);
            steps.push({ L: o.L, C, h: target.hue + side * target.reach });
          }
        }
        for (const step of steps) {
          const p = { ...c.own, [k]: toHex(oklchToRgbInGamut(step)) };
          if (r.value(p) <= now + 0.05) continue;
          const why = holds(p);
          if (target && turned && Math.abs(hueOff(step.h, target.hue)) > target.reach + 0.5) {
            why.push(`${k} hue window`);
          }
          for (const q of rules) {
            if (q === r || q.tier > r.tier) continue;
            const after = q.value(p);
            if (after < q.need - 1e-9 && after < q.value(c.own) - 1e-9) {
              why.push(q.id.replace(/ typical$/, ''));
            }
          }
          if (why.length === 0) why.push('missed');
          why.forEach((w) => out.add(w));
        }
      }
    }
    return [...out].sort();
  };

  // Each rule each swap leaves short, how far it gets of how far it
  // needs, and what stops it going further.
  const SWAP_SHORT: Record<string, string> = {
    'deuteranopia obsidian-ember fitted':
      'move brightGreen 0.0 of 6.0. brightGreen chroma, text brightGreen/brightWhite',
    'deuteranopia obsidian-ember published':
      'channel green/brightBlue 4.3 of 6.0, channel brightGreen/brightBlue 4.7 of 6.0. T2 green Lc, brightGreen chroma, channel brightBlue/brightMagenta, channel brightGreen/brightBlue, channel brightGreen/brightCyan, channel green/brightBlue, least brightGreen/brightBlue, least brightGreen/brightCyan, least green/brightBlue',
    'protanopia obsidian-ember fitted':
      'move brightGreen 0.0 of 6.0. brightGreen chroma, least brightGreen/brightCyan, text brightGreen/brightWhite',
    'protanopia obsidian-ember published':
      'move brightGreen 0.0 of 6.0, channel green/brightMagenta 5.9 of 6.0. T2 brightMagenta Lc, brightGreen chroma, channel green/brightBlue, least brightGreen/brightBlue',
    'tritanopia obsidian-ember fitted':
      'move brightBlue 0.0 of 6.0. T6 blue pair dE, least brightBlue/brightMagenta, text brightBlue/foreground, text brightBlue/white',
    'tritanopia obsidian-ember published':
      'channel yellow/brightMagenta 5.1 of 5.1, channel brightYellow/brightBlue 5.3 of 6.0. T6 blue bright step dL, T6 blue pair dE, T6 magenta pair dE, T7 red/yellow dL, channel brightRed/brightMagenta, channel yellow/brightBlue, kept red/yellow, text brightBlue/foreground, text brightYellow/brightWhite',
    'deuteranopia triad published':
      'move brightGreen 0.0 of 6.0, channel brightBlue/brightMagenta 4.6 of 4.8. T6 magenta pair dE, brightGreen chroma, channel brightMagenta/brightCyan, channel cyan/brightMagenta, channel green/brightBlue, least cyan/brightMagenta, least green/brightBlue, text brightGreen/brightWhite, text brightGreen/foreground',
    'protanopia triad published':
      'move brightGreen 0.0 of 6.0, channel green/brightBlue 4.7 of 6.0, channel brightBlue/brightMagenta 4.0 of 4.3, part green/brightYellow 17.6 of 18.0. T6 blue pair dE, T6 magenta pair dE, brightGreen chroma, channel brightBlue/brightMagenta, channel green/brightBlue, channel green/brightCyan, channel green/brightMagenta, green hue window, least brightBlue/brightMagenta, least brightGreen/brightCyan, least green/brightBlue, move green, show, text brightGreen/brightWhite, text brightGreen/foreground, text green/foreground',
    'tritanopia triad published':
      'channel brightRed/brightMagenta 5.9 of 6.0. T2 brightRed Lc, T6 red pair dE, T7 red/brightRed dL, channel brightBlue/brightMagenta, channel yellow/brightMagenta',
    'protanopia rubric published':
      'channel brightMagenta/brightCyan 4.1 of 6.0. channel brightGreen/brightCyan, channel green/brightMagenta',
    'deuteranopia tokyo-night fitted':
      'show 8.5 of 12.0, move green 0.0 of 12.0, move brightGreen 0.0 of 6.0, channel brightBlue/brightMagenta typical 5.1 of 6.0, part brightRed/brightGreen 14.6 of 18.0, part green/brightYellow 4.6 of 9.6. T3 blue Lc, T6 green bright step dL, T6 green pair dE, T6 yellow pair dE, T7 deutan red/yellow, blue hue window, brightBlue hue window, brightGreen chroma, channel brightMagenta/brightCyan, channel brightRed/brightYellow, channel cyan/brightBlue, channel green/brightGreen, channel yellow/brightRed, green chroma, kept red/brightYellow, kept red/yellow, least brightMagenta/brightCyan, least cyan/brightBlue, text brightGreen/brightWhite, text brightMagenta/brightWhite, text green/brightWhite',
    'deuteranopia tokyo-night published':
      'move green 0.0 of 12.0, move brightGreen 0.0 of 6.0. brightGreen chroma, green chroma',
    'protanopia tokyo-night fitted':
      'move green 0.0 of 12.0, move brightGreen 0.0 of 6.0, part green/brightYellow 7.2 of 9.6. T6 green pair dE, T6 yellow pair dE, brightGreen chroma, brightYellow chroma, channel brightGreen/brightYellow, channel green/brightCyan, green chroma, text brightGreen/brightWhite, text brightYellow/brightWhite, text green/brightWhite',
    'protanopia tokyo-night published':
      'show 9.1 of 12.0, move green 0.0 of 12.0, move brightGreen 0.0 of 6.0, channel brightBlue/brightMagenta typical 5.9 of 6.0. T6 blue bright step dL, blue hue window, brightBlue hue window, brightGreen chroma, channel brightMagenta/brightCyan, channel cyan/brightMagenta, green chroma, least brightGreen/brightCyan, least cyan/brightGreen, least green/brightCyan, least green/cyan, text brightGreen/brightWhite, text brightGreen/foreground, text brightMagenta/brightWhite, text brightMagenta/foreground, text brightMagenta/white, text green/brightWhite, text green/foreground',
    'protanopia nord fitted':
      'channel green/blue 4.9 of 6.0. T3 blue Lc, channel green/brightBlue, channel green/brightMagenta, missed',
    'tritanopia nord fitted':
      'channel yellow/brightBlue 5.2 of 6.0, channel brightRed/brightMagenta 6.0 of 6.0. T2 brightRed Lc, T6 blue pair dE, T6 yellow pair dE, brightBlue hue window, brightMagenta hue window, channel brightBlue/brightMagenta, channel yellow/brightMagenta, least yellow/brightMagenta, move brightBlue',
    'deuteranopia rose-pine fitted':
      'move green 5.6 of 12.0, move brightGreen 3.6 of 6.0, channel green/brightBlue 5.0 of 6.0, part red/green 14.7 of 18.0, part brightRed/brightGreen 15.7 of 18.0. T2 brightRed Lc, T2 green Lc, T3 red Lc, T6 green pair dE, T6 red pair dE, T7 red/brightRed dL, brightGreen chroma, brightGreen hue window, channel brightBlue/brightMagenta, channel brightGreen/brightBlue, channel brightGreen/brightMagenta, channel green/blue, channel green/brightBlue, channel yellow/brightRed, green hue window, kept red/yellow, least brightGreen/brightBlue, least brightGreen/brightMagenta, least green/brightBlue, text brightGreen/foreground, text brightGreen/white',
    'deuteranopia rose-pine published':
      'part red/green 16.3 of 18.0. T6 red bright step dL, green hue window, kept red/brightYellow, kept red/yellow, move green, show',
    'protanopia rose-pine fitted':
      'show 9.8 of 12.0, move green 5.8 of 12.0, move brightGreen 2.5 of 6.0, channel green/blue 4.2 of 6.0. T2 green Lc, T6 green pair dE, T6 red pair dE, brightGreen chroma, green hue window, kept blue/magenta, kept red/brightYellow, kept red/yellow, least brightGreen/brightMagenta, least green/blue, move green, show, text blue/foreground, text blue/white, text brightGreen/foreground, text brightGreen/white',
    'protanopia rose-pine published':
      'part red/green 10.8 of 18.0. T3 red Lc, T6 green bright step dL, channel green/brightMagenta, green reach, kept red/brightYellow, kept red/yellow',
    'tritanopia rose-pine fitted':
      'channel cyan/brightBlue 5.2 of 6.0, channel cyan/brightMagenta 5.0 of 6.0, channel brightYellow/brightMagenta 4.6 of 5.4. T2 brightBlue Lc, T6 blue pair dE, T6 cyan pair dE, T6 magenta pair dE, brightBlue hue window, brightMagenta hue window, channel brightMagenta/brightCyan, channel brightYellow/brightCyan, channel brightYellow/brightMagenta, channel cyan/brightBlue, channel cyan/brightMagenta, channel yellow/brightMagenta, kept cyan/green, least brightMagenta/brightCyan, least brightYellow/brightCyan, least brightYellow/brightMagenta, least cyan/brightMagenta, least yellow/brightMagenta, text brightYellow/brightWhite',
    'protanopia gruvbox fitted':
      'channel brightGreen/brightBlue 5.3 of 6.0. brightBlue hue window, brightGreen chroma, brightGreen hue window, channel green/brightBlue, least green/brightBlue',
    'tritanopia gruvbox fitted':
      'move brightBlue 0.0 of 6.0. T2 brightBlue Lc, T6 blue pair dE, least brightBlue/brightMagenta, text brightBlue/white',
    'tritanopia gruvbox published':
      'part green/blue 10.6 of 13.0. T3 blue Lc, T6 green pair dE, blue hue window, move blue, show, text green/white',
    'deuteranopia catppuccin fitted':
      'move brightGreen 0.0 of 6.0, part brightRed/brightGreen 11.7 of 18.0. T2 brightRed Lc, T6 red pair dE, brightGreen chroma, brightRed hue window, channel brightGreen/brightBlue, channel brightGreen/brightMagenta, channel brightGreen/brightYellow, channel cyan/brightGreen, channel yellow/brightGreen, least brightGreen/brightBlue, least brightGreen/brightMagenta, least yellow/brightGreen, text brightGreen/brightWhite, text brightGreen/foreground, text brightGreen/white',
    'deuteranopia catppuccin published':
      'move green 0.0 of 12.0, move brightGreen 0.0 of 6.0, part brightRed/brightGreen 16.0 of 18.0. T2 brightRed Lc, brightGreen chroma, brightRed hue window, channel brightGreen/brightCyan, channel brightGreen/brightMagenta, channel brightGreen/brightYellow, channel cyan/brightGreen, channel yellow/brightGreen, green chroma, least brightGreen/brightMagenta, text brightGreen/foreground, text green/foreground, text green/white',
    'protanopia catppuccin fitted':
      'move green 0.0 of 12.0, move brightGreen 0.0 of 6.0, channel brightGreen/brightYellow 5.5 of 6.0, part green/yellow 10.7 of 14.1, part green/brightYellow 17.3 of 18.0. T2 green Lc, T6 yellow bright step dL, T6 yellow pair dE, brightGreen chroma, brightYellow chroma, channel brightGreen/brightMagenta, channel brightYellow/brightCyan, channel cyan/brightGreen, channel green/brightBlue, channel green/brightMagenta, channel green/brightRed, channel yellow/brightGreen, channel yellow/brightYellow, least brightYellow/brightCyan, least green/brightBlue, least yellow/brightGreen, part red/green, text brightGreen/brightWhite, text brightGreen/foreground, text brightGreen/white, text green/white',
    'protanopia catppuccin published':
      'move green 0.0 of 12.0, move brightGreen 0.0 of 6.0. brightGreen chroma, least green/brightMagenta, text brightGreen/foreground, text brightGreen/white, text green/foreground, text green/white',
    'deuteranopia dracula published':
      'channel cyan/brightGreen 6.0 of 6.0. T2 cyan Lc, channel brightGreen/brightBlue, least brightGreen/brightBlue',
    'protanopia dracula fitted':
      'move brightGreen 0.0 of 6.0. brightGreen chroma, least cyan/brightGreen, text brightGreen/foreground, text brightGreen/white',
    'tritanopia dracula fitted':
      'show 9.6 of 12.0, move blue 9.6 of 12.0, channel brightRed/brightBlue 5.2 of 6.0. T6 blue pair dE, channel brightRed/brightMagenta, channel yellow/brightBlue, least brightRed/brightMagenta, text brightBlue/foreground, text brightBlue/white',
    'deuteranopia monokai fitted':
      'channel green/brightMagenta 4.2 of 6.0, channel brightGreen/brightBlue 4.0 of 6.0, channel brightBlue/brightMagenta 4.0 of 6.0, channel brightBlue/brightMagenta typical 4.8 of 6.0, channel green/blue typical 5.7 of 6.0, part brightRed/brightGreen 16.1 of 18.0. T2 brightMagenta Lc, T2 brightRed Lc, T2 green Lc, T6 blue bright step dL, T6 blue pair dE, T6 magenta pair dE, T6 red pair dE, T7 red/brightRed dL, brightBlue chroma, brightGreen chroma, channel brightBlue/brightMagenta, channel brightGreen/brightBlue, channel brightGreen/brightCyan, channel brightGreen/brightMagenta, channel cyan/blue, channel green/blue, channel green/brightBlue, channel green/brightMagenta, channel yellow/brightRed, green reach, kept blue/magenta, least brightBlue/brightMagenta, least brightGreen/brightBlue, least green/blue, least green/brightMagenta, missed, text brightGreen/foreground, text brightGreen/white',
    'protanopia monokai fitted':
      'move brightGreen 0.0 of 6.0, channel green/blue 4.9 of 6.0. T2 green Lc, T6 blue bright step dL, blue chroma, blue hue window, brightGreen chroma, green hue window, green reach, least brightGreen/brightCyan, text brightGreen/brightWhite, text brightGreen/foreground, text brightGreen/white',
    'protanopia monokai published':
      'channel green/blue 4.3 of 6.0. T6 blue bright step dL, T6 green bright step dL, green chroma, green hue window, kept blue/magenta, text green/brightWhite, text green/foreground, text green/white',
    'tritanopia monokai fitted':
      'channel yellow/brightRed 5.7 of 6.0. T2 brightRed Lc, T6 red pair dE, T7 red/brightRed dL, channel yellow/brightMagenta',
    'deuteranopia one-half-dark fitted':
      'move brightGreen 0.0 of 6.0, channel yellow/brightRed 5.3 of 5.5, part brightRed/brightGreen 13.2 of 18.0. T7 deutan red/yellow, brightGreen chroma, brightRed chroma, brightRed hue window, channel yellow/brightRed, kept red/yellow, least yellow/brightRed, text brightGreen/brightWhite, text brightGreen/foreground, text brightGreen/white, text brightRed/foreground, text brightRed/white',
    'protanopia one-half-dark fitted':
      'move brightGreen 0.0 of 6.0, channel green/brightBlue 5.1 of 6.0, channel green/brightMagenta 4.0 of 6.0. T2 brightMagenta Lc, T2 green Lc, T6 magenta pair dE, brightGreen chroma, channel brightBlue/brightCyan, channel green/blue, channel green/brightBlue, channel green/brightMagenta, channel green/cyan, least green/brightBlue, least green/brightMagenta, text brightBlue/foreground, text brightBlue/white, text brightGreen/brightWhite, text brightGreen/foreground, text brightGreen/white',
    'tritanopia one-half-dark fitted':
      'channel yellow/brightRed 4.9 of 6.0, channel yellow/brightBlue 5.4 of 6.0. T2 yellow Lc, T7 red/yellow dL, brightBlue hue window, brightRed chroma, channel brightRed/brightBlue, channel brightRed/brightYellow, kept red/yellow, least brightRed/brightYellow, text brightBlue/foreground, text brightBlue/white, text brightRed/foreground, text brightRed/white',
    'deuteranopia tango-dark fitted':
      'move brightGreen 0.0 of 6.0. brightGreen chroma, text brightGreen/brightWhite, text brightGreen/foreground, text brightGreen/white',
    'protanopia tango-dark fitted':
      'move brightGreen 0.0 of 6.0. brightGreen chroma, least brightGreen/brightCyan, text brightGreen/brightWhite, text brightGreen/foreground, text brightGreen/white',
    'tritanopia tango-dark fitted':
      'channel yellow/brightMagenta 4.3 of 5.0. T2 yellow Lc, channel brightBlue/brightMagenta, kept red/yellow',
    'deuteranopia classic-vivid fitted':
      'move brightGreen 0.0 of 6.0, part brightRed/brightGreen 14.6 of 18.0. T2 brightRed Lc, T6 red pair dE, T7 red/brightRed dL, brightGreen chroma, channel brightGreen/brightCyan, channel yellow/brightGreen, least brightGreen/brightCyan, least yellow/brightGreen, text brightGreen/foreground',
    'protanopia classic-vivid fitted':
      'move brightGreen 0.0 of 6.0, part green/yellow 17.4 of 18.0. T2 green Lc, T6 yellow bright step dL, brightGreen chroma, channel green/brightBlue, channel green/brightMagenta, channel yellow/brightCyan, text brightGreen/foreground, text yellow/brightWhite, yellow chroma',
    'deuteranopia high-contrast published':
      'move brightGreen 0.0 of 6.0, part brightRed/brightGreen 17.2 of 18.0. T2 brightRed Lc, T6 red pair dE, T7 red/brightRed dL, brightGreen chroma, brightRed hue window, channel brightGreen/brightCyan, channel red/brightRed, least brightGreen/brightCyan, text brightGreen/brightWhite, text brightGreen/foreground, text brightGreen/white',
    'protanopia high-contrast published':
      'move brightGreen 0.0 of 6.0. brightGreen chroma, least cyan/brightGreen, text brightGreen/brightWhite, text brightGreen/foreground, text brightGreen/white',
    'protanopia everforest-dark fitted':
      'channel yellow/brightRed 4.5 of 6.0. T2 brightRed Lc, T2 yellow Lc, T6 yellow pair dE, brightRed hue window, channel cyan/brightRed, channel yellow/cyan, kept red/yellow, least cyan/brightRed, text brightRed/white, text yellow/white',
    'tritanopia everforest-dark fitted':
      'move brightBlue 0.0 of 6.0. brightBlue chroma, text brightBlue/brightWhite, text brightBlue/foreground',
    'deuteranopia green-screen fitted':
      'channel green/brightBlue 4.0 of 6.0, channel brightGreen/brightMagenta 4.3 of 6.0. T2 green Lc, brightGreen chroma, channel brightBlue/brightMagenta, channel brightGreen/brightCyan, least brightBlue/brightMagenta, least brightGreen/brightCyan',
    'protanopia green-screen fitted':
      'part green/yellow 14.7 of 18.0. T6 green pair dE, T6 yellow bright step dL, channel green/brightBlue, channel yellow/brightCyan, text yellow/brightWhite, yellow chroma',
    'tritanopia srcery fitted':
      'move brightBlue 0.0 of 6.0. T2 brightBlue Lc, T6 blue pair dE, text brightBlue/white',
    'tritanopia srcery published':
      'channel brightYellow/brightBlue 5.8 of 6.0. T6 yellow bright step dL, T6 yellow pair dE, brightBlue hue window, channel yellow/brightYellow, kept red/brightYellow, text brightBlue/white',
    'deuteranopia nightfly fitted':
      'channel brightBlue/brightMagenta typical 4.2 of 6.0. T6 magenta pair dE, brightBlue hue window, channel brightGreen/brightBlue, channel green/brightMagenta, least brightGreen/brightBlue, text brightBlue/foreground',
    'deuteranopia nightfly published':
      'move brightGreen 0.0 of 6.0, part brightRed/brightGreen 16.1 of 18.0. T2 brightGreen Lc, T6 green bright step dL, T6 green pair dE, channel brightGreen/brightBlue, channel brightGreen/brightMagenta, least brightGreen/brightBlue',
    'protanopia nightfly fitted':
      'move brightGreen 0.0 of 6.0, channel green/brightBlue 4.5 of 6.0, channel green/brightMagenta 4.0 of 6.0, part green/brightYellow 17.7 of 18.0. T2 brightMagenta Lc, T6 magenta pair dE, brightBlue chroma, brightGreen chroma, channel brightYellow/brightCyan, channel green/brightBlue, channel green/brightMagenta, least brightYellow/brightCyan, least green/brightBlue, least green/brightMagenta, text brightBlue/brightWhite, text brightBlue/foreground, text brightGreen/brightWhite, text brightGreen/foreground, text green/foreground',
    'protanopia nightfly published':
      'move brightGreen 0.0 of 6.0. T2 brightGreen Lc, T6 green pair dE, least brightGreen/brightBlue, least green/brightGreen',
    'deuteranopia melange-dark fitted':
      'channel brightGreen/brightBlue 4.5 of 6.0, channel brightGreen/brightCyan 5.9 of 6.0, channel brightGreen/brightCyan typical 6.0 of 6.0, part red/green 17.1 of 18.0, part brightRed/brightGreen 14.4 of 18.0. T3 red Lc, T6 green pair dE, T7 deutan red/yellow, channel brightGreen/brightBlue, channel brightGreen/brightCyan, channel green/brightBlue, channel yellow/brightRed, green hue window, kept red/yellow, least brightGreen/brightBlue, least brightGreen/brightCyan, least green/brightBlue, least yellow/brightRed, text brightCyan/brightWhite, text brightCyan/foreground',
    'protanopia melange-dark fitted':
      'channel green/brightBlue 5.6 of 6.0, channel brightGreen/brightCyan 4.7 of 6.0, channel brightGreen/brightCyan typical 5.2 of 6.0. T2 brightBlue Lc, T6 blue pair dE, T6 green pair dE, brightBlue hue window, brightGreen hue window, channel green/brightGreen, text brightCyan/brightWhite',
    'tritanopia melange-dark fitted': 'show 11.5 of 12.0, move blue 11.5 of 12.0. text blue/white',
    'tritanopia melange-dark published':
      'show 11.4 of 12.0, move blue 11.4 of 12.0, channel yellow/brightMagenta 6.0 of 6.0, cue red/blue 2.8 of 3.0. T2 yellow Lc, T3 red Lc, T7 red/yellow dL, channel brightYellow/brightMagenta, kept red/yellow, move blue, show, text blue/white',
    'protanopia melange-light published':
      'part red/green 14.2 of 14.6. T7 protan red/brightYellow, T7 protan red/yellow, channel green/brightMagenta, channel red/cyan, green hue window, green reach, kept red/brightYellow, kept red/yellow, least red/cyan',
    'deuteranopia modus-vivendi fitted':
      'channel green/brightBlue 5.0 of 6.0, channel brightGreen/brightMagenta 5.2 of 6.0. T2 green Lc, brightBlue hue window, brightGreen chroma, channel brightBlue/brightMagenta, channel green/blue, least brightBlue/brightMagenta, text brightGreen/foreground',
    'protanopia modus-vivendi fitted':
      'move brightGreen 0.0 of 6.0, channel green/brightMagenta 4.6 of 6.0. brightGreen chroma, brightMagenta chroma, channel green/brightBlue, green hue window, text brightGreen/brightWhite, text brightGreen/foreground, text brightMagenta/foreground',
    'deuteranopia harbor-dark fitted':
      'move brightGreen 0.0 of 6.0, channel brightBlue/brightCyan 5.9 of 6.0, part brightRed/brightGreen 16.6 of 18.0. T2 brightRed Lc, brightGreen chroma, channel brightBlue/brightMagenta, channel yellow/brightRed, least brightBlue/brightMagenta, text brightCyan/foreground, text brightGreen/brightWhite, text brightGreen/foreground',
    'protanopia harbor-dark fitted':
      'move brightGreen 0.0 of 6.0. brightGreen chroma, least brightGreen/brightCyan, text brightGreen/brightWhite, text brightGreen/foreground',
    'protanopia harbor-dark published':
      'channel green/brightMagenta 4.2 of 6.0, channel brightGreen/brightMagenta 5.3 of 6.0, channel brightBlue/brightCyan 4.8 of 6.0. T2 brightMagenta Lc, T2 green Lc, T6 cyan bright step dL, T6 cyan pair dE, brightBlue chroma, channel brightGreen/brightBlue, channel brightGreen/brightCyan, channel brightGreen/brightMagenta, channel cyan/brightCyan, channel green/brightMagenta, green hue window, least brightGreen/brightMagenta, least cyan/brightCyan, least green/brightMagenta, move green, text brightBlue/foreground, text brightCyan/white, text brightGreen/white',
    'deuteranopia iceberg-dark fitted':
      'move brightGreen 0.0 of 6.0, channel green/brightMagenta 5.9 of 6.0, part brightRed/brightGreen 17.9 of 18.0. T2 green Lc, brightGreen chroma, brightRed hue window, channel brightBlue/brightMagenta, channel yellow/brightRed, least brightBlue/brightMagenta, text brightGreen/brightWhite',
    'protanopia iceberg-dark fitted':
      'move brightGreen 0.0 of 6.0, channel green/brightBlue 5.5 of 6.0, channel green/brightMagenta 4.8 of 6.0. T2 green Lc, brightGreen chroma, channel brightBlue/brightCyan, channel brightBlue/brightMagenta, green hue window, text brightBlue/foreground, text brightBlue/white, text brightGreen/brightWhite, text brightMagenta/foreground',
    'protanopia iceberg-dark published':
      'move brightGreen 0.0 of 6.0. brightGreen chroma, least brightGreen/brightCyan, text brightGreen/brightWhite',
    'tritanopia iceberg-dark published':
      'part green/blue 13.0 of 13.0. T3 blue Lc, blue hue window, move blue, text green/brightWhite, text green/foreground, text green/white',
  };

  it('keeps every floor and target, or names the rule short and what stops it', () => {
    const report: Record<string, string> = {};
    for (const c of CASES) {
      const rules = rulesOf(c);
      const short = rules.filter((r) => r.value(c.own) < r.need - 1e-9);
      if (short.length === 0) continue;
      // The channels, the text and the kept pairs hold on every theme.
      for (const r of short) expect(r.tier, `${c.at} ${r.id}`).toBeGreaterThan(1);
      const items = short.map(
        (r) => `${r.id} ${r.value(c.own).toFixed(1)} of ${r.need.toFixed(1)}`,
      );
      report[c.at] = `${items.join(', ')}. ${blockersOf(c, rules, short).join(', ')}`;
    }
    expect(report).toEqual(SWAP_SHORT);
  });

  // The swap shows on every theme. The lead color, green or for a
  // tritanope blue, or where it cannot another plain color the vision
  // turns, moves at least MOVE_MIN.lead as a typical eye sees it.
  // SHOW_SHORT names the cases where none can, with the move that shows
  // most: where the lead already sits near its target, such as Rose
  // Pine's pine green, which is a blue, and Dracula's and Melange Dark's
  // blue, which lean purple, and Tokyo Night, whose yells and text are
  // blue already.
  const SHOW_SHORT: Record<string, string> = {
    'deuteranopia tokyo-night fitted': 'blue 8.5',
    'protanopia tokyo-night published': 'blue 9.1',
    'protanopia rose-pine fitted': 'blue 9.8',
    'tritanopia dracula fitted': 'blue 9.6',
    'tritanopia melange-dark fitted': 'blue 11.5',
    'tritanopia melange-dark published': 'blue 11.4',
  };

  it('moves a turned color far enough to see on every theme', () => {
    const report: Record<string, string> = {};
    for (const c of CASES) {
      const plain = CUE_SLOTS.slice(0, 6).filter((k) => SWAP_TARGETS[c.vision][familyOf(k)]);
      const moves = plain.map((k) => [k, turnedMove(c, c.own, k)] as const);
      const [slot, most] = moves.reduce((win, m) => (m[1] > win[1] ? m : win));
      if (most < MOVE_MIN.lead) report[c.at] = `${slot} ${most.toFixed(1)}`;
    }
    expect(report).toEqual(SHOW_SHORT);
  });

  // Kanso Zen's soft palette changed nothing you could see under the
  // old fit. The swap moves its lead color at least MOVE_MIN.lead under
  // every vision, with Fit game colors on and off, and turns its window's
  // success blue under deuteranopia and protanopia.
  it('changes Kanso Zen plainly', () => {
    const kanso = findTheme('kanso-zen');
    for (const vision of OTHER) {
      const [lead] = LEAD_SLOTS[vision];
      for (const fit of [true, false]) {
        const typical = playPalette(kanso, fit);
        const own = playPalette(kanso, fit, vision);
        expect(
          deltaEOk(hex(typical[lead]), hex(own[lead])),
          `${vision} ${fit}`,
        ).toBeGreaterThanOrEqual(MOVE_MIN.lead);
      }
    }
    for (const vision of ['deuteranopia', 'protanopia'] as const) {
      const t = themeTokens(kanso);
      const v = themeTokens(kanso, vision);
      expect(deltaEOk(hex(t.success), hex(v.success)), vision).toBeGreaterThanOrEqual(
        STATUS_MOVE_MIN,
      );
      const h = lch(v.success).h;
      expect(h, vision).toBeGreaterThanOrEqual(205);
      expect(h, vision).toBeLessThanOrEqual(255);
    }
    expect(themeTokens(kanso, 'tritanopia').success).toBe(themeTokens(kanso).success);
  });
});

describe('window status colors for a color vision', () => {
  const OTHER: ColorVision[] = ['deuteranopia', 'protanopia', 'tritanopia'];
  const KEYS = ['danger', 'warn', 'success'] as const;
  type Key = (typeof KEYS)[number];
  const sees = (a: string, b: string, vision: ColorVision) => seenApart(hex(a), hex(b), vision);
  const lch = (c: string) => rgbToOklch(hex(c));
  const hueOff = (h: number, target: number) => ((h - target + 540) % 360) - 180;

  // A tritanope tells red, yellow and green apart by hue, so the window
  // keeps its status colors wherever a tritanope sees danger STATUS_PART
  // from warn and from success, or as far as a typical eye sees them if
  // that is less, give or take VISION_SLACK. TRITAN_MOVED names the
  // themes where danger stands nearer, with the distance a tritanope
  // sees before and after. There the window moves the three in lightness
  // at their own hues.
  const TRITAN_MOVED: Record<string, string> = {
    'tokyo-night': 'danger/warn 12.1 to 16.4',
    'rose-pine': 'danger/warn 16.9 to 20.3',
    'solarized-light': 'danger/warn 15.9 to 19.0',
    'high-contrast': 'danger/warn 13.3 to 14.6',
    'melange-light': 'danger/warn 8.6 to 26.3',
    'harbor-dark': 'danger/warn 3.8 to 12.7',
  };

  it('keeps the status colors under tritanopia where a tritanope tells them apart', () => {
    const moved: Record<string, string> = {};
    for (const theme of BUILTIN_THEMES) {
      const t = themeTokens(theme);
      const v = themeTokens(theme, 'tritanopia');
      const items: string[] = [];
      for (const key of ['warn', 'success'] as const) {
        const need = Math.min(STATUS_PART, deltaEOk(hex(t.danger), hex(t[key]))) - VISION_SLACK;
        const was = sees(t.danger, t[key], 'tritanopia');
        if (was >= need) continue;
        const now = sees(v.danger, v[key], 'tritanopia');
        items.push(`danger/${key} ${was.toFixed(1)} to ${now.toFixed(1)}`);
      }
      for (const key of KEYS) {
        const at = `${theme.id} ${key}`;
        if (items.length === 0) expect(v[key], at).toBe(t[key]);
        else expect(Math.abs(hueOff(lch(v[key]).h, lch(t[key]).h)), at).toBeLessThan(3);
      }
      if (items.length > 0) moved[theme.id] = items.join(', ');
    }
    expect(moved).toEqual(TRITAN_MOVED);
  });

  it('turns success blue and danger toward vermilion for deuteranopia and protanopia', () => {
    const red = {
      danger: { hue: 45, reach: 15, chroma: 0.13 },
      success: { hue: 230, reach: 25, chroma: 0.11 },
    };
    expect(STATUS_SWAP).toEqual({
      typical: {},
      deuteranopia: red,
      protanopia: red,
      tritanopia: {},
    });
    expect([STATUS_MOVE_MIN, STATUS_PART]).toEqual([10, 20]);
  });

  // The floors and targets of the window's swap (chrome statusSeenBy),
  // firmest first: the 3:1 floor, the text tiers, each turned color's
  // window and chroma, a pinned accent, danger from warn and warn from
  // success, success's move, and danger from success.
  interface Rule {
    id: string;
    tier: number;
    keys: readonly Key[];
    value: (s: Record<Key, string>) => number;
    need: number;
  }
  const rulesOf = (theme: AppTheme, vision: ColorVision): Rule[] => {
    const t = themeTokens(theme);
    const v = themeTokens(theme, vision);
    const out: Rule[] = [];
    for (const key of KEYS) {
      for (const ground of [v.panel, v.raised]) {
        out.push({
          id: `${key} 3:1 floor`,
          tier: 0,
          keys: [key],
          value: (s) => contrast(hex(s[key]), hex(ground)),
          need: Math.min(STATUS_CONTRAST, contrast(hex(t[key]), hex(ground))),
        });
      }
      for (const tier of ['text', 'secondary'] as const) {
        out.push({
          id: `${key}/${tier}`,
          tier: 0,
          keys: [key],
          value: (s) => sees(s[key], t[tier], vision),
          need: Math.min(sees(t[key], t[tier], vision), VISION_GUARD),
        });
        out.push({
          id: `${key}/${tier} typical`,
          tier: 0,
          keys: [key],
          value: (s) => deltaEOk(hex(s[key]), hex(t[tier])),
          need: Math.min(deltaEOk(hex(t[key]), hex(t[tier])), VISION_GUARD),
        });
      }
      const target = STATUS_SWAP[vision][key];
      if (target) {
        out.push({
          id: `${key} hue window`,
          tier: 0,
          keys: [key],
          value: (s) => target.reach + 1.5 - Math.abs(hueOff(lch(s[key]).h, target.hue)),
          need: 0,
        });
        out.push({
          id: `${key} chroma`,
          tier: 0,
          keys: [key],
          value: (s) => lch(s[key]).C,
          need: CHROMA_KEEP * Math.min(lch(t[key]).C, target.chroma) - 0.002,
        });
      } else {
        out.push({
          id: `${key} hue`,
          tier: 0,
          keys: [key],
          value: (s) => 3 - Math.abs(hueOff(lch(s[key]).h, lch(t[key]).h)),
          need: 0,
        });
        const from = lch(t[key]).C;
        out.push({
          id: `${key} chroma`,
          tier: 0,
          keys: [key],
          value: (s) => lch(s[key]).C,
          need: from > 0.04 ? HUE_CHROMA_KEEP * from - 0.002 : 0,
        });
      }
      if (theme.chrome?.accent !== undefined) {
        out.push({
          id: `accent/${key}`,
          tier: 1,
          keys: [key],
          value: (s) => sees(t.accent, s[key], vision),
          need: Math.min(
            ACCENT_APART,
            deltaEOk(hex(t.accent), hex(t[key])),
            sees(t.accent, t[key], vision),
          ),
        });
      }
    }
    out.push({
      id: 'danger/warn',
      tier: 2,
      keys: ['danger', 'warn'],
      value: (s) => sees(s.danger, s.warn, vision),
      need: sees(t.danger, t.warn, vision) - KEPT_SLACK,
    });
    out.push({
      id: 'warn/success',
      tier: 2,
      keys: ['warn', 'success'],
      value: (s) => sees(s.warn, s.success, vision),
      need: Math.min(sees(t.warn, t.success, vision), VISION_GUARD),
    });
    if (STATUS_SWAP[vision].success) {
      out.push({
        id: 'success move',
        tier: 3,
        keys: ['success'],
        value: (s) => deltaEOk(hex(t.success), hex(s.success)),
        need: STATUS_MOVE_MIN,
      });
    }
    out.push({
      id: 'danger/success',
      tier: 5,
      keys: ['danger', 'success'],
      value: (s) => sees(s.danger, s.success, vision),
      need: Math.min(STATUS_PART, deltaEOk(hex(t.danger), hex(t.success))) - VISION_SLACK,
    });
    if (Object.keys(STATUS_SWAP[vision]).length === 0) {
      out.push({
        id: 'danger/warn part',
        tier: 5,
        keys: ['danger', 'warn'],
        value: (s) => sees(s.danger, s.warn, vision),
        need: Math.min(STATUS_PART, deltaEOk(hex(t.danger), hex(t.warn))) - VISION_SLACK,
      });
    }
    return out;
  };

  // Each rule the window leaves short for each vision, how far it gets of
  // how far it needs, and what stops it going further: a firmer rule, or
  // one as firm, that a step in lightness or hue breaks. Where nothing
  // stops it the search missed the step. High Contrast holds its status
  // colors at 7:1 on every ground, and a blue that reads 7:1 on its black
  // sits near its sky blue accent.
  const WINDOW_SHORT: Record<string, string> = {
    'deuteranopia high-contrast': 'accent/success 8.2 of 12.0. success/secondary',
    'protanopia high-contrast': 'accent/success 5.0 of 12.0. missed, success hue window',
    'protanopia rose-pine': 'success move 6.6 of 10.0. success hue window, success/secondary',
    'tritanopia high-contrast': 'danger/warn part 14.6 of 15.6. missed, warn/secondary',
  };

  it('keeps every floor and target of the window, or names the rule short and what stops it', () => {
    const report: Record<string, string> = {};
    for (const vision of OTHER) {
      for (const theme of BUILTIN_THEMES) {
        const v = themeTokens(theme, vision);
        const own = { danger: v.danger, warn: v.warn, success: v.success };
        const rules = rulesOf(theme, vision);
        const short = rules.filter((r) => r.value(own) < r.need - 1e-9);
        if (short.length === 0) continue;
        const why = new Set<string>();
        for (const r of short) {
          for (const key of r.keys) {
            const o = lch(own[key]);
            const steps = [-0.02, 0.02].map((d) => ({
              ...o,
              L: Math.max(0, Math.min(1, o.L + d)),
            }));
            if (STATUS_SWAP[vision][key])
              steps.push({ ...o, h: o.h - 2.5 }, { ...o, h: o.h + 2.5 });
            for (const step of steps) {
              const s = { ...own, [key]: toHex(oklchToRgbInGamut(step)) };
              if (r.value(s) <= r.value(own) + 0.05) continue;
              const broke = rules
                .filter((q) => q !== r && q.tier <= r.tier)
                .filter((q) => q.value(s) < q.need - 1e-9 && q.value(s) < q.value(own) - 1e-9)
                .map((q) => q.id.replace(/ typical$/, ''));
              (broke.length > 0 ? broke : ['missed']).forEach((w) => why.add(w));
            }
          }
        }
        const items = short.map(
          (r) => `${r.id} ${r.value(own).toFixed(1)} of ${r.need.toFixed(1)}`,
        );
        report[`${vision} ${theme.id}`] = `${items.join(', ')}. ${[...why].sort().join(', ')}`;
      }
    }
    expect(report).toEqual(WINDOW_SHORT);
  });

  it('keeps the words drawn in danger and warn readable', () => {
    for (const vision of OTHER) {
      for (const theme of BUILTIN_THEMES) {
        const v = themeTokens(theme, vision);
        for (const key of ['dangerText', 'warnText'] as const) {
          for (const ground of [v.panel, v.raised]) {
            expect(
              contrast(hex(v[key]), hex(ground)),
              `${vision} ${theme.id} ${key}`,
            ).toBeGreaterThanOrEqual(STATUS_TEXT_CONTRAST);
          }
        }
      }
    }
  });

  // An accent the rule picks stands ACCENT_APART from every status color
  // as the vision sees them, wherever a hue of the theme stands that far.
  // On One Half Dark for a tritanope none does, since the window keeps
  // its Typical status colors, and the rule takes the hue that stands
  // farthest, its Typical pick. A pinned accent stays as the theme drew it, and no status
  // color comes nearer to it than the Typical one stands, up to
  // ACCENT_APART, except where High Contrast holds success at 7:1 and
  // the blue it turns to sits near its sky blue accent.
  const ACCENT_SHORT: Record<string, string> = {
    'deuteranopia high-contrast accent from success': '8.2',
    'protanopia high-contrast accent from success': '5.0',
    'tritanopia one-half-dark accent from danger': '11.1',
  };

  it('keeps the accent apart from every status color as the vision sees them', () => {
    const report: Record<string, string> = {};
    for (const vision of OTHER) {
      for (const theme of BUILTIN_THEMES) {
        const t = themeTokens(theme);
        const v = themeTokens(theme, vision);
        for (const key of KEYS) {
          const at = `${vision} ${theme.id} accent from ${key}`;
          if (theme.chrome?.accent === undefined) {
            const away = sees(v.accent, v[key], vision);
            if (away < ACCENT_APART) report[at] = away.toFixed(1);
          } else {
            expect(v.accent, at).toBe(t.accent);
            const floor = Math.min(
              ACCENT_APART,
              deltaEOk(hex(t.accent), hex(t[key])),
              sees(t.accent, t[key], vision),
            );
            const away = sees(v.accent, v[key], vision);
            if (away < floor - 1e-9) report[at] = away.toFixed(1);
          }
        }
      }
    }
    expect(report).toEqual(ACCENT_SHORT);
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
    warnText: '#e5c057',
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
    // The panel sits on the terminal ground under the one ground rule.
    expect(t.panel).toBe('#050403');
    expect(t.text).toBe('#c0bdbb');
    expect(t.warn).toBe('#ecc985');
  });
});

describe('customThemeLabel', () => {
  const base = { description: '', xterm: {}, chrome: {} };

  it('shows a custom theme with a blank name under its id', () => {
    expect(customToAppTheme({ ...base, id: 'nord-copy', label: '' }).label).toBe('nord-copy');
    expect(customToAppTheme({ ...base, id: 'paper', label: '   ' }).label).toBe('paper');
  });

  it('keeps a name you typed, without the spaces around it', () => {
    expect(customThemeLabel({ id: 'dusk', label: ' Dusk ' })).toBe('Dusk');
    expect(customToAppTheme({ ...base, id: 'dusk', label: 'Dusk' }).label).toBe('Dusk');
  });
});

describe('theme credits', () => {
  it('names the source, the author and the license of every built in theme', () => {
    for (const theme of BUILTIN_THEMES) {
      expect(theme.source?.trim(), theme.id).toBeTruthy();
      expect(theme.author?.trim(), theme.id).toBeTruthy();
      expect(theme.license, theme.id).toBeTruthy();
    }
  });

  it('records Dracula as the source of Dracula at Night, on a darker ground', () => {
    const night = findTheme('dracula');
    expect(night.source).toBe('Dracula');
    expect(night.license).toBe('MIT');
    // Dracula's own ground is #282a36.
    expect(night.xterm.background).toBe('#1a1c23');
    expect(night.description).toContain('#282a36');
  });

  it('leaves a custom theme without a credit', () => {
    const custom = customToAppTheme({
      id: 'dusk',
      label: 'Dusk',
      description: 'Mine.',
      xterm: {},
      chrome: {},
    });
    expect(custom.source).toBeUndefined();
    expect(custom.author).toBeUndefined();
    expect(custom.license).toBeUndefined();
  });
});

describe('public/theme-credits.txt', () => {
  // Each section opens under a rule of equals signs. Its first line
  // names the themes it covers, its Source line the work and its author.
  const sections = credits
    .split(/^=+$/m)
    .slice(1)
    .map((text) => {
      const [title = '', ...rest] = text.trim().split('\n');
      return { themes: title.split(/, | and /), body: rest.join('\n') };
    });
  const sectionFor = (label: string) => sections.find((s) => s.themes.includes(label));
  const flat = (text: string) => text.replace(/\s+/g, ' ');
  const PERMISSION =
    'Permission is hereby granted, free of charge, to any person obtaining a copy of this ' +
    'software and associated documentation files (the "Software"), to deal in the Software ' +
    'without restriction';

  it('credits every built in theme and its author', () => {
    for (const theme of BUILTIN_THEMES) {
      const section = sectionFor(theme.label);
      expect(section, theme.id).toBeDefined();
      const source = /^Source (.*)$/m.exec(section?.body ?? '')?.[1] ?? '';
      expect(source.toLowerCase(), theme.id).toContain(String(theme.author).toLowerCase());
    }
  });

  it('keeps the copyright line and the permission of every MIT theme', () => {
    const mit = BUILTIN_THEMES.filter((t) => t.license === 'MIT');
    expect(mit.length).toBeGreaterThan(0);
    for (const theme of mit) {
      const body = sectionFor(theme.label)?.body ?? '';
      expect(body, theme.id).toMatch(/^Copyright \(c\) \d{4}\S* \S/m);
      expect(flat(body), theme.id).toContain(PERMISSION);
    }
  });

  it('keeps the MIT notice of the base16 scheme Monokai takes its colors from', () => {
    const body = sectionFor('Monokai')?.body ?? '';
    expect(body).toContain('Copyright (c) 2022 Tinted Theming');
    expect(flat(body)).toContain(PERMISSION);
  });

  it('keeps the Apache License of the port Tokyo Night takes its slots from', () => {
    const body = flat(sectionFor('Tokyo Night')?.body ?? '');
    expect(body).toContain('Apache License Version 2.0, January 2004');
    expect(body).toContain('TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION');
    expect(body).toContain('END OF TERMS AND CONDITIONS');
  });

  it('keeps the copyright line of Modus Vivendi', () => {
    expect(findTheme('modus-vivendi').license).toBe('GPL-3.0-or-later');
    expect(sectionFor('Modus Vivendi')?.body).toContain(
      'Copyright (C) 2019-2026 Free Software Foundation, Inc.',
    );
  });
});

const custom = (id: string, background: string): CustomTheme => ({
  id,
  label: id,
  description: '',
  xterm: { background, foreground: background === '#000000' ? '#ffffff' : '#000000' },
  chrome: {},
});

describe('seedDarkTheme', () => {
  it('takes the current theme when it is dark', () => {
    expect(seedDarkTheme('nord', [])).toBe('nord');
    expect(seedDarkTheme('obsidian-ember', [])).toBe('obsidian-ember');
  });

  it('falls back to Obsidian Ember for a light or unknown theme', () => {
    expect(seedDarkTheme('rubric', [])).toBe('obsidian-ember');
    expect(seedDarkTheme('system', [])).toBe('obsidian-ember');
    expect(seedDarkTheme('gone', [])).toBe('obsidian-ember');
  });

  it('reads custom themes by their own background', () => {
    const themes = [custom('night-ink', '#000000'), custom('paper', '#ffffff')];
    expect(seedDarkTheme('night-ink', themes)).toBe('night-ink');
    expect(seedDarkTheme('paper', themes)).toBe('obsidian-ember');
  });

  it('reads a retired id by the theme that took its place', () => {
    // A saved One Dark shows One Half Dark, so it seeds a dark theme of
    // One Half Dark under the id you saved.
    expect(seedDarkTheme('one-dark', [])).toBe('one-dark');
    expect(findTheme(seedDarkTheme('one-dark', [])).id).toBe('one-half-dark');
    expect(seedDarkTheme('vellum', [])).toBe('obsidian-ember');
    expect(seedDarkTheme('everforest-light', [])).toBe('obsidian-ember');
    // A custom theme with a retired id wins over the successor.
    expect(seedDarkTheme('one-dark', [custom('one-dark', '#ffffff')])).toBe('obsidian-ember');
  });
});

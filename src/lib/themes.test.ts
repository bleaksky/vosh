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
  KEPT_PAIRS,
  KEPT_SLACK,
  L_REACH,
  largestMove,
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
      // menu separators draw in it.
      const sep = stepDL(hex(t.sep), bg);
      expect(sep, 'sep off the ground').toBeGreaterThanOrEqual(8);
      expect(sep, 'sep off the ground').toBeLessThanOrEqual(12);
      expect(stepDL(hex(t.sep), raised), 'sep off raised').toBeGreaterThanOrEqual(4);
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
  // Dark, Tango Dark, High Contrast and Green Screen pin none, so the rule
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
      'high-contrast': '#ff55ff',
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
        // A light field is the raised paper, below.
        if (step === undefined) continue;
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

// The fits are computed ahead by lib/gameFit, and gameFit.test.ts fits
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

  it('sets bright white dL 8 above body text in the six of Q19', () => {
    for (const id of [
      'monokai',
      'rose-pine',
      'everforest-dark',
      'tokyo-night',
      'gruvbox',
      'high-contrast',
    ]) {
      expect(value(id, 'T6 fg/brightWhite dL'), id).toBeGreaterThanOrEqual(8);
    }
    expect(findTheme('monokai').fitted?.foreground).toBe('#e4e4df');
    expect(findTheme('high-contrast').fitted?.foreground).toBe('#e4e4e4');
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

  // The 24 themes one-window (c5a6ebd0) shipped, their Typical fits and
  // their play palettes with Fit game colors on, digested from that
  // commit's themes.ts. Typical plays them byte for byte as it did.
  it('plays Typical byte for byte as before color vision', () => {
    const before = BUILTIN_THEMES.filter((t) => !['harbor-dark', 'iceberg-dark'].includes(t.id));
    expect(before).toHaveLength(24);
    expect(digest(JSON.stringify(before.map((t) => [t.id, t.fitted ?? null])))).toBe('84775ab7');
    expect(digest(JSON.stringify(before.map((t) => [t.id, playPalette(t, true)])))).toBe(
      '188ff686',
    );
    for (const theme of BUILTIN_THEMES) {
      for (const fit of [true, false]) {
        expect(playPalette(theme, fit, 'typical'), theme.id).toEqual(playPalette(theme, fit));
      }
      expect(visionFitOf(theme, 'typical'), theme.id).toBe(theme.fitted);
    }
  });

  // The 26 themes one-window (a206426c) ships, their Typical fits, their
  // play palettes with Fit game colors on and their window tokens,
  // digested from that commit. Color vision changes none of them.
  it('fits and paints Typical byte for byte as at a206426c', () => {
    expect(BUILTIN_THEMES).toHaveLength(26);
    expect(digest(JSON.stringify(BUILTIN_THEMES.map((t) => [t.id, t.fitted ?? null])))).toBe(
      '3d4595e1',
    );
    expect(digest(JSON.stringify(BUILTIN_THEMES.map((t) => [t.id, playPalette(t, true)])))).toBe(
      '9c975f41',
    );
    expect(digest(JSON.stringify(BUILTIN_THEMES.map((t) => [t.id, themeTokens(t)])))).toBe(
      '8b44151f',
    );
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

  // Each family the vision turns lands inside its window, give or take
  // the rounding to a hex color, which bends the hue of a pale color
  // more. Every other cue color keeps its hue within 3 degrees.
  it('turns each family into its window and keeps the hue of every other', () => {
    for (const c of CASES) {
      for (const slot of CUE_SLOTS) {
        const at = `${c.at} ${slot}`;
        const target = SWAP_TARGETS[c.vision][familyOf(slot)];
        const now = lch(c.own[slot]);
        if (target) {
          const slack = 1 + 0.2 / Math.max(now.C, 0.01);
          expect(Math.abs(hueOff(now.h, target.hue)), at).toBeLessThanOrEqual(target.reach + slack);
        } else if (c.own[slot] !== c.start[slot] && lch(c.start[slot]).C >= 0.04) {
          expect(Math.abs(hueOff(now.h, lch(c.start[slot]).h)), at).toBeLessThan(3);
        }
      }
    }
  });

  // No swap gives up a check its start passes or falls more than
  // MISS_SLACK further short of one it misses, leaving out the T7 pairs
  // another vision sees through. No color moves more than L_REACH in
  // lightness, red keeps its side of yellow and bold yellow, and a
  // turned color keeps CHROMA_KEEP of its chroma, or of its target's.
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
        const target = SWAP_TARGETS[c.vision][familyOf(slot)];
        if (target) {
          const keep = CHROMA_KEEP * Math.min(lch(c.start[slot]).C, target.chroma);
          expect(lch(c.own[slot]).C, at).toBeGreaterThanOrEqual(keep - 1e-9);
        }
      }
      for (const y of ['yellow', 'brightYellow'] as const) {
        const was = lch(c.start.red).L - lch(c.start[y]).L;
        if (Math.abs(was) < 0.02) continue;
        const now = lch(c.own.red).L - lch(c.own[y]).L;
        expect(Math.sign(now), `${c.at} red/${y}`).toBe(Math.sign(was));
      }
    }
  });

  // The floors and targets of the swap below the checks, firmest first
  // (gameFit swapFor): each color clear of body text, white and bold
  // white, the kept pairs, the channels at CHANNEL_LEAST, the lead
  // color's move, the channel and cue floors and the parted pairs.
  // Each reads `p`, the palette in play, against `start`.
  interface Rule {
    id: string;
    tier: number;
    slots: readonly Slot[];
    value: (p: XtermPalette) => number;
    need: number;
  }
  const rulesOf = ({ start, vision }: Case): Rule[] => {
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
    for (const p of CHANNEL_PAIRS) pair('least', 2, p, CHANNEL_LEAST);
    LEAD_SLOTS[vision].forEach((k, i) => {
      out.push({
        id: `move ${k}`,
        tier: 3,
        slots: [k],
        value: (p) => deltaEOk(hex(start[k]), hex(p[k])),
        need: i === 0 ? MOVE_MIN.lead : MOVE_MIN.bold,
      });
    });
    for (const p of CHANNEL_PAIRS) pair('channel', 4, p, CHANNEL_FLOOR);
    for (const p of CUE_PAIRS) pair('cue', 4, p, CUE_FLOOR);
    for (const [a, b] of PARTED_PAIRS[vision]) {
      out.push({
        id: `part ${a}/${b}`,
        tier: 5,
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
        const target = SWAP_TARGETS[c.vision][familyOf(slot)];
        const keep = target ? CHROMA_KEEP * Math.min(lch(c.start[slot]).C, target.chroma) : 0;
        if (lch(p[slot]).C < keep) why.push(`${slot} chroma`);
      }
      return why;
    };
    for (const r of short) {
      const now = r.value(c.own);
      for (const k of r.slots) {
        if (!CUE_SLOTS.includes(k as AnsiSlot)) continue;
        // Steps the swap could take: lightness at the color's hue and the
        // chroma it aims for, and for a turned color its hue.
        const target = SWAP_TARGETS[c.vision][familyOf(k as AnsiSlot)];
        const from = lch(c.start[k]);
        const o = {
          L: lch(c.own[k]).L,
          C: target ? Math.max(from.C, target.chroma) : from.C,
          h: target ? lch(c.own[k]).h : from.h,
        };
        const steps = [-0.02, 0.02].map((d) => ({ ...o, L: Math.max(0, Math.min(1, o.L + d)) }));
        if (target) steps.push({ ...o, h: o.h - 2.5 }, { ...o, h: o.h + 2.5 });
        for (const step of steps) {
          const p = { ...c.own, [k]: toHex(oklchToRgbInGamut(step)) };
          if (r.value(p) <= now + 0.05) continue;
          const why = holds(p);
          if (target && Math.abs(hueOff(step.h, target.hue)) > target.reach + 0.5) {
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
    'protanopia obsidian-ember published':
      'channel brightBlue/brightCyan 5.5 of 6.0. channel green/brightBlue, cue brightGreen/brightBlue, least green/brightBlue, text brightCyan/brightWhite',
    'protanopia triad published':
      'channel green/brightMagenta 4.7 of 6.0. T2 green Lc, channel brightBlue/brightMagenta, green hue window, least brightBlue/brightMagenta',
    'deuteranopia tokyo-night fitted':
      'text green/white 7.4 of 8.0, text brightGreen/foreground 2.1 of 8.0, text brightGreen/white 7.0 of 8.0, channel green/cyan 5.0 of 6.0, channel green/cyan typical 5.0 of 6.0. T2 green Lc, T6 cyan pair dE, T6 green pair dE, channel green/blue, cue green/magenta, text brightGreen/brightWhite, text brightGreen/foreground, text brightGreen/white, text cyan/foreground',
    'deuteranopia tokyo-night published':
      'text green/white 7.3 of 8.0, text brightGreen/white 7.3 of 8.0, least green/brightMagenta 2.8 of 4.0, channel green/brightBlue 4.6 of 6.0, channel green/brightMagenta 2.8 of 6.0, cue brightGreen/brightMagenta 2.8 of 3.0. T2 brightBlue Lc, T2 brightGreen Lc, T2 green Lc, T6 green bright step dL, brightBlue hue window, channel green/blue, channel green/brightBlue, channel green/brightMagenta, cue brightGreen/brightBlue, cue green/magenta, least green/brightBlue, least green/brightMagenta, text brightMagenta/brightWhite, text brightMagenta/foreground, text green/white',
    'protanopia tokyo-night fitted':
      'text green/foreground 6.5 of 8.0, text green/white 5.3 of 8.0, text brightGreen/foreground 2.8 of 8.0, text brightGreen/brightWhite 5.5 of 8.0, channel green/cyan 5.0 of 6.0, channel green/cyan typical 5.1 of 6.0, channel green/brightBlue 5.6 of 6.0. T2 green Lc, T6 cyan pair dE, T6 green pair dE, brightGreen hue window, channel brightBlue/brightCyan, cue brightGreen/brightBlue, text brightBlue/brightWhite, text brightGreen/brightWhite, text brightGreen/foreground, text brightGreen/white, text cyan/brightWhite, text cyan/foreground, text green/white',
    'protanopia tokyo-night published':
      'text green/foreground 7.2 of 8.0, text green/white 5.5 of 8.0, text green/brightWhite 7.2 of 8.0, text brightGreen/foreground 7.2 of 8.0, text brightGreen/white 5.5 of 8.0, text brightGreen/brightWhite 7.2 of 8.0, channel green/brightMagenta 4.8 of 6.0. T2 brightGreen Lc, T2 brightMagenta Lc, T2 green Lc, T6 green bright step dL, channel brightBlue/brightMagenta, text brightGreen/brightWhite, text brightGreen/foreground, text brightGreen/white, text green/brightWhite, text green/foreground, text green/white',
    'deuteranopia rose-pine fitted':
      'move green 5.6 of 12.0, move brightGreen 3.6 of 6.0. T6 green pair dE, brightGreen chroma, brightGreen hue window, green hue window, text brightGreen/foreground, text brightGreen/white',
    'protanopia rose-pine fitted':
      'move green 6.6 of 12.0, move brightGreen 3.4 of 6.0, channel green/brightBlue 4.8 of 6.0, part green/brightYellow 16.4 of 18.0. T2 brightBlue Lc, T6 blue pair dE, T6 green pair dE, brightBlue hue window, brightBlue reach, brightGreen chroma, channel green/brightBlue, channel green/brightMagenta, green hue window, least green/brightBlue, move green, text brightGreen/brightWhite, text brightGreen/foreground, text brightGreen/white, text brightYellow/brightWhite, text green/white',
    'tritanopia gruvbox fitted':
      'channel brightBlue/brightMagenta 4.5 of 6.0. T2 brightMagenta Lc, T6 magenta pair dE, brightMagenta hue window, text brightBlue/foreground, text brightBlue/white',
    'tritanopia gruvbox published':
      'part green/blue 10.6 of 13.0. T3 blue Lc, T6 green pair dE, blue hue window, text green/white',
    'deuteranopia catppuccin fitted':
      'text brightGreen/foreground 3.9 of 8.0, text brightGreen/foreground typical 7.6 of 8.0, text brightGreen/white 4.9 of 8.0. T6 green pair dE, brightGreen chroma, brightGreen hue window, text brightGreen/foreground, text brightGreen/white',
    'deuteranopia catppuccin published':
      'text green/brightWhite 7.7 of 8.0, text brightGreen/brightWhite 7.7 of 8.0, least green/brightBlue 2.0 of 4.0, channel green/brightBlue 2.0 of 6.0, cue brightGreen/brightBlue 2.0 of 3.0. T2 brightBlue Lc, T6 green bright step dL, text brightBlue/white, text brightGreen/white, text green/white',
    'protanopia catppuccin fitted':
      'text green/white 6.5 of 8.0, text brightGreen/foreground 1.9 of 8.0, text brightGreen/foreground typical 6.2 of 8.0, text brightGreen/brightWhite 6.5 of 8.0. T2 green Lc, T6 green pair dE, brightGreen chroma, brightGreen hue window, green hue window, text brightGreen/brightWhite, text brightGreen/foreground, text brightGreen/white',
    'protanopia catppuccin published':
      'text green/white 5.2 of 8.0, text brightGreen/white 5.2 of 8.0, channel green/brightBlue 5.6 of 6.0. T2 brightBlue Lc, T6 green bright step dL, brightBlue hue window, text brightGreen/brightWhite, text brightGreen/white, text green/brightWhite, text green/foreground, text green/white',
    'tritanopia dracula fitted': 'move blue 9.4 of 12.0. T6 blue pair dE, blue hue window',
    'deuteranopia monokai fitted':
      'kept blue/magenta 8.3 of 11.1, least green/blue 3.1 of 4.0, channel green/blue 3.1 of 6.0, channel green/brightMagenta 4.0 of 6.0, channel brightBlue/brightMagenta 4.8 of 6.0, channel brightBlue/brightMagenta typical 5.1 of 6.0, cue brightGreen/brightBlue 2.0 of 3.0. T2 green Lc, T3 magenta Lc, T6 blue pair dE, T6 magenta pair dE, blue hue window, brightBlue chroma, brightGreen chroma, channel brightBlue/brightMagenta, channel green/blue, channel green/brightMagenta, cue brightGreen/brightBlue, kept blue/magenta, least brightBlue/brightMagenta, least green/blue, least green/brightMagenta, text brightGreen/foreground, text brightGreen/white',
    'protanopia monokai fitted':
      'text brightYellow/foreground 4.5 of 4.6, kept blue/magenta 8.9 of 16.2, least green/brightMagenta 3.2 of 4.0, channel green/blue 5.1 of 6.0, channel green/brightBlue 4.0 of 6.0, channel green/brightMagenta 3.2 of 6.0, cue brightGreen/brightBlue 2.0 of 3.0. T2 brightMagenta Lc, T2 green Lc, T3 magenta Lc, T6 blue pair dE, T6 green pair dE, T6 magenta pair dE, T6 yellow pair dE, blue hue window, brightBlue chroma, brightGreen chroma, channel brightBlue/brightMagenta, channel green/blue, channel green/brightBlue, channel green/brightMagenta, cue brightGreen/brightBlue, green reach, kept blue/magenta, kept red/brightYellow, least green/blue, least green/brightBlue, least green/brightMagenta, text brightGreen/foreground, text brightGreen/white',
    'protanopia monokai published':
      'kept blue/magenta 18.7 of 21.2. T3 magenta Lc, T6 blue bright step dL, blue chroma, blue hue window',
    'protanopia one-half-dark fitted':
      'channel green/cyan 5.9 of 6.0, channel green/brightBlue 4.6 of 6.0, channel green/brightMagenta 4.0 of 6.0, cue brightGreen/brightBlue 1.2 of 3.0. T2 brightMagenta Lc, T2 cyan Lc, T2 green Lc, T6 green bright step dL, T6 green pair dE, T6 magenta pair dE, brightBlue chroma, brightGreen hue window, brightGreen reach, channel brightBlue/brightCyan, channel green/blue, channel green/brightBlue, channel green/brightMagenta, channel green/cyan, green reach, least green/brightBlue, least green/brightMagenta, text brightBlue/foreground, text brightBlue/white',
    'protanopia tango-dark fitted':
      'channel green/brightBlue 4.5 of 6.0, channel green/brightMagenta 4.5 of 6.0. T2 brightMagenta Lc, T2 green Lc, T6 green bright step dL, T6 magenta pair dE, brightBlue chroma, channel brightBlue/brightCyan, channel green/brightBlue, channel green/brightMagenta, least green/brightBlue, least green/brightMagenta, text brightBlue/foreground, text brightBlue/white',
    'deuteranopia classic-vivid fitted':
      'channel brightBlue/brightMagenta 4.1 of 6.0, cue brightGreen/brightMagenta 1.4 of 3.0. T6 green pair dE, brightGreen chroma, channel brightBlue/brightMagenta, channel green/brightBlue, cue brightGreen/brightBlue, least brightBlue/brightMagenta, least green/brightBlue, text brightGreen/foreground, text brightMagenta/foreground',
    'protanopia classic-vivid fitted':
      'text brightGreen/foreground 7.4 of 8.0, part green/yellow 17.7 of 18.0. T2 green Lc, T6 yellow bright step dL, brightGreen chroma, brightGreen hue window, channel green/brightBlue, channel green/brightMagenta, text yellow/brightWhite',
    'protanopia high-contrast fitted':
      'cue brightGreen/brightBlue 2.8 of 3.0. T6 green pair dE, brightBlue chroma, brightGreen hue window, text brightBlue/foreground, text brightBlue/white',
    'tritanopia everforest-dark fitted':
      'text brightBlue/white 6.6 of 8.0, channel brightBlue/brightMagenta 5.5 of 6.0. T2 brightBlue Lc, T6 blue pair dE, brightBlue hue window, cue brightYellow/brightMagenta, text brightMagenta/brightWhite, text brightMagenta/foreground',
    'tritanopia srcery fitted':
      'text brightBlue/white 7.5 of 8.0. brightBlue hue window, text brightBlue/foreground',
    'deuteranopia nightfly fitted':
      'channel brightBlue/brightMagenta typical 5.1 of 6.0. T6 magenta pair dE, brightBlue hue window, channel green/brightMagenta, cue brightGreen/brightBlue, text brightBlue/foreground',
    'protanopia nightfly fitted':
      'channel green/brightBlue 4.5 of 6.0, channel green/brightMagenta 4.0 of 6.0. T2 brightMagenta Lc, T6 green pair dE, T6 magenta pair dE, brightBlue chroma, channel green/brightBlue, channel green/brightMagenta, cue brightGreen/brightBlue, least green/brightBlue, least green/brightMagenta, text brightBlue/brightWhite, text brightBlue/foreground, text green/foreground',
    'protanopia nightfly published':
      'text brightGreen/brightWhite 6.7 of 8.0. T6 green pair dE, brightGreen chroma, text brightGreen/foreground',
    'tritanopia melange-dark fitted': 'move blue 11.5 of 12.0. blue hue window, text blue/white',
    'tritanopia melange-dark published':
      'move blue 11.4 of 12.0, cue red/blue 2.8 of 3.0. T3 red Lc, blue hue window, move blue, text blue/white',
    'protanopia modus-vivendi fitted':
      'channel green/brightBlue 5.3 of 6.0. T2 brightBlue Lc, T6 blue pair dE, T6 green pair dE, brightBlue hue window, channel green/brightMagenta, least green/brightMagenta',
    'protanopia harbor-dark fitted':
      'channel green/brightBlue 4.7 of 6.0, channel brightBlue/brightMagenta typical 5.3 of 6.0, cue brightGreen/brightMagenta 1.5 of 3.0. T2 green Lc, T6 green pair dE, brightBlue chroma, brightBlue hue window, channel brightBlue/brightMagenta, channel brightCyan/brightMagenta, channel green/brightBlue, channel green/brightMagenta, cue brightGreen/brightBlue, cue brightGreen/brightMagenta, least brightBlue/brightMagenta, least green/brightBlue, text brightGreen/foreground, text brightMagenta/foreground',
    'deuteranopia iceberg-dark fitted':
      'text brightGreen/foreground 6.5 of 8.0, text brightGreen/white 6.5 of 8.0, channel green/brightMagenta 5.9 of 6.0, channel brightBlue/brightMagenta typical 4.0 of 5.0, cue brightGreen/brightBlue 2.7 of 3.0. T2 green Lc, T6 green pair dE, T6 magenta pair dE, brightBlue hue window, brightGreen hue window, channel brightBlue/brightCyan, channel brightBlue/brightMagenta, channel green/brightBlue, channel green/brightMagenta, cue brightGreen/brightBlue, least brightBlue/brightMagenta, text brightBlue/foreground, text brightGreen/foreground, text brightGreen/white',
    'deuteranopia iceberg-dark published':
      'channel brightBlue/brightMagenta typical 4.6 of 4.8. T2 brightBlue Lc, T6 blue bright step dL, T6 blue pair dE, brightBlue hue window, channel brightCyan/brightMagenta, channel green/brightMagenta',
    'protanopia iceberg-dark fitted':
      'text brightGreen/foreground 5.5 of 8.0, text brightGreen/white 6.5 of 8.0, text brightGreen/brightWhite 6.9 of 7.4, channel green/brightMagenta 4.2 of 6.0, channel brightBlue/brightMagenta typical 4.2 of 5.0. T2 green Lc, T6 green pair dE, T6 magenta pair dE, brightBlue hue window, channel brightBlue/brightCyan, channel brightBlue/brightMagenta, channel green/brightMagenta, cue brightGreen/brightBlue, green hue window, least brightBlue/brightMagenta, least green/brightMagenta, text brightBlue/brightWhite, text brightGreen/brightWhite, text brightGreen/foreground, text brightGreen/white, text brightMagenta/foreground',
    'protanopia iceberg-dark published':
      'channel brightBlue/brightMagenta typical 4.7 of 4.8. T2 brightBlue Lc, brightBlue hue window, channel green/brightMagenta',
    'tritanopia iceberg-dark published':
      'part green/blue 12.1 of 13.0. T3 blue Lc, T6 green bright step dL, T6 green pair dE, blue hue window, cue blue/magenta, move blue, text green/brightWhite, text green/foreground, text green/white',
  };

  it('keeps every floor and target, or names the rule short and what stops it', () => {
    const report: Record<string, string> = {};
    for (const c of CASES) {
      const rules = rulesOf(c);
      const short = rules.filter((r) => r.value(c.own) < r.need - 1e-9);
      if (short.length === 0) continue;
      const items = short.map(
        (r) => `${r.id} ${r.value(c.own).toFixed(1)} of ${r.need.toFixed(1)}`,
      );
      report[c.at] = `${items.join(', ')}. ${blockersOf(c, rules, short).join(', ')}`;
    }
    expect(report).toEqual(SWAP_SHORT);
  });

  // No channel comes nearer another than CHANNEL_LEAST, or than it stood
  // at the start if that is less, except where SWAP_SHORT names it.
  it('keeps the channels apart wherever SWAP_SHORT names no exception', () => {
    for (const c of CASES) {
      for (const [a, b] of CHANNEL_PAIRS) {
        const at = `${c.at} ${a}/${b}`;
        const floor = Math.min(sees(c.start, a, b, c.vision), CHANNEL_LEAST);
        if (SWAP_SHORT[c.at]?.includes(`least ${a}/${b} `)) continue;
        expect(sees(c.own, a, b, c.vision), at).toBeGreaterThanOrEqual(floor - 1e-9);
      }
    }
  });

  // The swap shows on every theme. The lead color, green or for a
  // tritanope blue, moves at least MOVE_MIN.lead and its bold twin
  // MOVE_MIN.bold as a typical eye sees them, and so some cue color
  // moves MOVE_MIN.lead. SWAP_SHORT names the few where the lead color
  // already sits near its target, such as Rose Pine's pine green, which
  // is a blue, and Dracula's blue, which is a purple. There the lead
  // moves less, and some cue color still moves MOVE_MIN.bold.
  it('moves the lead color far enough to see on every theme', () => {
    for (const c of CASES) {
      const [lead, bold] = LEAD_SLOTS[c.vision];
      const most = largestMove(c.start, c.own, CUE_SLOTS);
      if (SWAP_SHORT[c.at]?.includes('move ')) {
        expect(most, c.at).toBeGreaterThanOrEqual(MOVE_MIN.bold);
        continue;
      }
      expect(most, c.at).toBeGreaterThanOrEqual(MOVE_MIN.lead);
      const move = (k: Slot) => deltaEOk(hex(c.start[k]), hex(c.own[k]));
      expect(move(lead), c.at).toBeGreaterThanOrEqual(MOVE_MIN.lead);
      expect(move(bold), c.at).toBeGreaterThanOrEqual(MOVE_MIN.bold);
    }
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

  // A tritanope tells red, yellow and green apart, so the window keeps
  // its status colors. TRITAN_NEAR names the themes where a tritanope
  // sees danger nearer warn than CHANNEL_FLOOR, or nearer success than
  // STATUS_PART, or as far as a typical eye sees them if less, minus
  // VISION_SLACK.
  const TRITAN_NEAR: Record<string, string> = { 'harbor-dark': 'danger/warn 3.8' };

  it('keeps the status colors under tritanopia', () => {
    const near: Record<string, string> = {};
    for (const theme of BUILTIN_THEMES) {
      const t = themeTokens(theme);
      const v = themeTokens(theme, 'tritanopia');
      for (const key of KEYS) expect(v[key], `${theme.id} ${key}`).toBe(t[key]);
      const items: string[] = [];
      const warn = sees(t.danger, t.warn, 'tritanopia');
      if (warn < CHANNEL_FLOOR) items.push(`danger/warn ${warn.toFixed(1)}`);
      const success = sees(t.danger, t.success, 'tritanopia');
      const part = Math.min(STATUS_PART, deltaEOk(hex(t.danger), hex(t.success))) - VISION_SLACK;
      if (success < part) items.push(`danger/success ${success.toFixed(1)} of ${part.toFixed(1)}`);
      if (items.length > 0) near[theme.id] = items.join(', ');
    }
    expect(near).toEqual(TRITAN_NEAR);
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
    return out;
  };

  // Each rule the window leaves short for deuteranopia and protanopia,
  // how far it gets of how far it needs, and what stops it going
  // further: a firmer rule, or one as firm, that a step in lightness or
  // hue breaks. Where nothing stops it the search missed the step.
  const WINDOW_SHORT: Record<string, string> = {
    'protanopia rose-pine': 'success move 6.6 of 10.0. success hue window, success/secondary',
  };

  it('keeps every floor and target of the window, or names the rule short and what stops it', () => {
    const report: Record<string, string> = {};
    for (const vision of ['deuteranopia', 'protanopia'] as const) {
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
  // On two themes for a tritanope none does, since the window keeps its
  // Typical status colors, and the rule takes the hue that stands
  // farthest. A pinned accent stays as the theme drew it, and no status
  // color comes nearer to it than the Typical one stands, up to
  // ACCENT_APART.
  const ACCENT_SHORT: Record<string, string> = {
    'tritanopia one-half-dark accent from danger': '11.1',
    'tritanopia tokyo-night accent from success': '11.2',
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
            expect(sees(v.accent, v[key], vision), at).toBeGreaterThanOrEqual(floor - 1e-9);
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

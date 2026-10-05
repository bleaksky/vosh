import { afterEach, describe, expect, it } from 'vitest';
import {
  ACCENT_APART,
  CHROME_COLOR_KEYS,
  ON_ACCENT_CONTRAST,
  SECONDARY_CONTRAST,
  STATUS_CONTRAST,
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
  checks,
  COLOR_VISIONS,
  GAME_SLOTS,
  GUARD_SHARE,
  GUARDED_PAIRS,
  holdsCheck,
  holdsVision,
  HUE_TURN_FAR,
  seenApart,
  textGuards,
  turnHue,
  turnRoom,
  turnSlots,
  VISION_GUARD,
  visionChecks,
  visionPairs,
  visionSlots,
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

// How far `to` sits up OKLCH hue from `from`, in degrees either way.
const turned = (from: string, to: string) => {
  const d = rgbToOklch(hex(to)).h - rgbToOklch(hex(from)).h;
  return d > 180 ? d - 360 : d < -180 ? d + 360 : d;
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

describe('color vision fits', () => {
  const OTHER: ColorVision[] = ['deuteranopia', 'protanopia', 'tritanopia'];
  const fitting = BUILTIN_THEMES.filter((t) => t.fitGameColors !== false);
  // FNV-1a over a JSON string, to pin a large value in a few characters.
  const digest = (text: string) => {
    let h = 0x811c9dc5;
    for (let i = 0; i < text.length; i++) {
      h ^= text.charCodeAt(i);
      h = Math.imul(h, 0x01000193) >>> 0;
    }
    return h.toString(16).padStart(8, '0');
  };
  type Slot = (typeof GAME_SLOTS)[number];
  const sees = (p: XtermPalette, a: Slot, b: Slot, vision: ColorVision) =>
    seenApart(hex(p[a]), hex(p[b]), vision);
  // How far a check falls outside its target, as the fit measures it.
  const gap = (need: string, value: number) => {
    if (need.startsWith('>=')) return Math.max(0, +need.slice(2) - value);
    const [a, b] = need.split('..').map(Number);
    return Math.max(0, a - value, value - b);
  };
  // The floor a fit for `vision` holds each guarded pair to: a pair the
  // vision keeps apart where the Typical fit has it or at its target,
  // any other where the Typical fit has it or at three quarters of that,
  // and never under VISION_GUARD.
  const guardFloor = (typical: XtermPalette, a: AnsiSlot, b: AnsiSlot, vision: ColorVision) => {
    const seen = sees(typical, a, b, vision);
    const kept = visionPairs(vision).some(([x, y]) => x === a && y === b);
    const cap = kept
      ? deltaEOk(hex(typical[a]), hex(typical[b]))
      : Math.max(VISION_GUARD, GUARD_SHARE * seen);
    return Math.min(seen, cap);
  };
  // The floor a fit for `vision` holds each color it moves to from body
  // text, white and bold white: where the Typical fit has it, or
  // VISION_GUARD if that is less.
  const textFloor = (typical: XtermPalette, a: Slot, b: Slot, vision: ColorVision) =>
    Math.min(sees(typical, a, b, vision), VISION_GUARD);

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
      expect(playPalette(theme, true, 'typical'), theme.id).toEqual(playPalette(theme, true));
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

  it('stores only the slots each fit moved, in hex', () => {
    for (const theme of fitting) {
      for (const vision of OTHER) {
        for (const [slot, color] of Object.entries(visionFitOf(theme, vision) ?? {})) {
          const at = `${theme.id} ${vision} ${slot}`;
          expect(GAME_SLOTS, at).toContain(slot);
          expect(color, at).toMatch(/^#[0-9a-f]{6}$/);
          expect(color, at).not.toBe(theme.xterm[slot as keyof XtermPalette]);
        }
      }
    }
  });

  it('keeps Solarized Dark as published for every vision (Q20)', () => {
    const dark = findTheme('solarized-dark');
    for (const vision of COLOR_VISIONS) expect(playPalette(dark, true, vision)).toBe(dark.xterm);
  });

  it('plays the published palette for every vision while Fit game colors is off', () => {
    for (const theme of BUILTIN_THEMES) {
      for (const vision of COLOR_VISIONS) {
        expect(playPalette(theme, false, vision), theme.id).toBe(theme.xterm);
      }
    }
  });

  // A vision's fit holds to the Typical fit. It passes every check the
  // Typical fit passes, and on each check the Typical fit misses it falls
  // no further short, so no color reads fainter on its ground than the
  // Typical fit has it. It leaves only the T7 pairs another vision sees
  // through.
  it('gives up no check the Typical fit passes and falls no further short of one it misses', () => {
    for (const vision of OTHER) {
      for (const theme of fitting) {
        const held = checks(playPalette(theme, true));
        checks(playPalette(theme, true, vision)).forEach((c, i) => {
          if (!holdsCheck(c.id, vision)) return;
          const at = `${vision} ${theme.id} ${c.id} ${held[i].value} to ${c.value}`;
          if (held[i].ok) expect(c.ok, at).toBe(true);
          else
            expect(gap(c.need, c.value), at).toBeLessThanOrEqual(gap(held[i].need, held[i].value));
        });
      }
    }
  });

  it('moves only the colors of the pairs a vision keeps apart and their twins', () => {
    for (const vision of OTHER) {
      const moves = visionSlots(vision);
      for (const theme of fitting) {
        const typical = playPalette(theme, true);
        const own = playPalette(theme, true, vision);
        for (const slot of GAME_SLOTS.filter((k) => !moves.includes(k))) {
          expect(own[slot], `${vision} ${theme.id} ${slot}`).toBe(typical[slot]);
        }
      }
    }
  });

  it('brings no cue pair nearer, as the vision sees it, than its guard', () => {
    for (const vision of OTHER) {
      for (const theme of fitting) {
        const typical = playPalette(theme, true);
        const own = playPalette(theme, true, vision);
        for (const [a, b] of GUARDED_PAIRS) {
          const floor = guardFloor(typical, a, b, vision);
          const at = `${vision} ${theme.id} ${a}/${b}`;
          expect(sees(own, a, b, vision), at).toBeGreaterThanOrEqual(floor - 1e-9);
        }
      }
    }
  });

  // A tell in green never reads as body text. Each color a vision moves
  // stays as far from body text, white and bold white, as the vision
  // sees them, as the Typical fit has it, or VISION_GUARD if that is
  // less.
  it('keeps every color it moves clear of body text, white and bold white', () => {
    for (const vision of OTHER) {
      for (const theme of fitting) {
        const typical = playPalette(theme, true);
        const own = playPalette(theme, true, vision);
        for (const [a, b] of textGuards(vision)) {
          const at = `${vision} ${theme.id} ${a}/${b}`;
          const floor = textFloor(typical, a, b, vision);
          expect(sees(own, a, b, vision), at).toBeGreaterThanOrEqual(floor - 1e-9);
        }
      }
    }
  });

  // Red, green and for a tritanope cyan turn up OKLCH hue, at most
  // HUE_TURN_FAR and never past their family's limit. Every other color
  // keeps its hue, give or take the rounding of a step in lightness.
  it('turns no hue past its bound or its family limit', () => {
    for (const vision of OTHER) {
      for (const theme of fitting) {
        const typical = playPalette(theme, true);
        const own = playPalette(theme, true, vision);
        for (const slot of visionSlots(vision)) {
          if (own[slot] === typical[slot] || rgbToOklch(hex(typical[slot])).C < 0.05) continue;
          const at = `${vision} ${theme.id} ${slot}`;
          const family = turnSlots(vision).find((f) => f === slot);
          const dh = turned(typical[slot], own[slot]);
          if (!family) {
            expect(Math.abs(dh), at).toBeLessThan(3);
            continue;
          }
          const room = turnRoom(family, rgbToOklch(hex(theme.xterm[slot])).h, HUE_TURN_FAR);
          expect(dh, at).toBeGreaterThan(-3);
          expect(dh, at).toBeLessThanOrEqual(room + 1.5);
        }
      }
    }
  });

  // What stops each step that would part a pair further: a check the fit
  // holds that the step gives up or falls further short of, a cue pair
  // it brings under its guard, the hue limit where a turn has no room
  // left, or trade where nothing stops the step and the fit spent the
  // room on another pair.
  const gameBlockers = (theme: AppTheme, vision: ColorVision, a: AnsiSlot, b: AnsiSlot) => {
    const typical = playPalette(theme, true);
    const own = playPalette(theme, true, vision);
    const held = checks(typical);
    const now = sees(own, a, b, vision);
    const out = new Set<string>();
    for (const k of [a, b]) {
      const lch = rgbToOklch(hex(own[k]));
      const family = turnSlots(vision).find((f) => f === k);
      const steps: [string, string][] = [-0.02, 0.02].map((d) => [
        'L',
        toHex(oklchToRgbInGamut({ ...lch, L: Math.max(0, Math.min(1, lch.L + d)) })),
      ]);
      if (family) steps.push(['turn', toHex(turnHue(hex(own[k]), 4))]);
      for (const [step, color] of steps) {
        const p = { ...own, [k]: color };
        if (sees(p, a, b, vision) <= now + 0.05) continue;
        const why: string[] = [];
        checks(p).forEach((c, i) => {
          if (!holdsCheck(c.id, vision)) return;
          if (held[i].ok ? !c.ok : gap(c.need, c.value) > gap(held[i].need, held[i].value)) {
            why.push(c.id);
          }
        });
        for (const [x, y] of GUARDED_PAIRS) {
          if (sees(p, x, y, vision) < guardFloor(typical, x, y, vision)) why.push(`${x}/${y}`);
        }
        for (const [x, y] of textGuards(vision)) {
          if (sees(p, x, y, vision) < textFloor(typical, x, y, vision)) why.push(`${x}/${y}`);
        }
        const c0 = rgbToOklch(hex(theme.xterm[k])).C;
        if (c0 > 0.04 && rgbToOklch(hex(color)).C < 0.6 * c0) why.push(`${k} chroma`);
        if (step === 'turn' && family) {
          const room = turnRoom(family, rgbToOklch(hex(theme.xterm[k])).h, HUE_TURN_FAR);
          if (turned(theme.xterm[k], own[k]) + 4 > room + 1) why.push(`${k} hue limit`);
        }
        if (why.length === 0) why.push('trade');
        why.forEach((w) => out.add(w));
      }
    }
    return [...out].sort();
  };

  // Each pair each vision's fit leaves short of its target, how far it
  // gets of how far it needs, and what stops it going further.
  const GAME_SHORT: Record<string, string> = {
    'deuteranopia obsidian-ember':
      'red/green 19.3 of 28.6, red/brightYellow 21.9 of 23.9, red/yellow 12 of 15.9. T3 red Lc, T6 green pair dE, T6 yellow pair dE, T7 deutan yellow/green, brightYellow/brightWhite, green/brightWhite, green/brightYellow, green/foreground, green/white, green/yellow',
    'deuteranopia triad':
      'red/green 12.1 of 30.3, red/brightYellow 25.1 of 30.5, red/yellow 15.6 of 22.6. T3 red Lc, T6 yellow pair dE, brightYellow/brightWhite, brightYellow/foreground, cyan/green, green hue limit, green/white, red hue limit',
    'deuteranopia rubric':
      'red/green 22.9 of 29.1. T3 red Lc, cyan/green, green hue limit, red hue limit',
    'deuteranopia kanso-zen':
      'red/green 21.5 of 25.1, red/brightYellow 25.5 of 27.2, red/yellow 12.2 of 15.4. T3 red Lc, T6 green pair dE, T7 deutan yellow/green, brightYellow chroma, brightYellow/brightWhite, green/brightWhite, green/brightYellow, green/foreground, green/yellow, red hue limit, yellow/foreground',
    'deuteranopia tokyo-night':
      'red/green 21.1 of 33.2, red/brightYellow 24 of 25.1, red/yellow 12 of 18.1. T3 red Lc, T6 green pair dE, T7 deutan yellow/green, brightYellow chroma, brightYellow/brightWhite, green/yellow',
    'deuteranopia nord':
      'red/green 12.9 of 19.7, red/brightYellow 32.2 of 34.4, red/yellow 23.4 of 26. T3 red Lc, T6 yellow pair dE, T7 deutan yellow/green, brightYellow chroma, brightYellow/brightWhite, brightYellow/foreground, brightYellow/white, cyan/green, green/yellow, red/yellow',
    'deuteranopia rose-pine':
      'red/green 10.2 of 23, red/brightYellow 22.6 of 26.7, red/yellow 15.9 of 20.7. T3 red Lc, T6 green pair dE, T6 yellow pair dE, brightYellow chroma, brightYellow/brightWhite, brightYellow/foreground, brightYellow/white, green hue limit, red/yellow',
    'deuteranopia gruvbox':
      'red/green 21.7 of 31.9, red/brightYellow 24.7 of 28.6, red/yellow 12.1 of 19.6. T3 red Lc, T6 green pair dE, T7 deutan yellow/green, brightYellow/foreground, green/brightYellow, green/foreground, green/yellow',
    'deuteranopia catppuccin':
      'red/green 11.7 of 24.3, red/brightYellow 29.1 of 32.1, red/yellow 23.4 of 26.9. T3 red Lc, T6 green pair dE, T6 yellow pair dE, T7 deutan yellow/green, brightYellow chroma, brightYellow/brightWhite, cyan/green, green/yellow, yellow/brightWhite',
    'deuteranopia dracula':
      'red/green 13.5 of 39.1, red/brightYellow 32.1 of 39.8, red/yellow 25.4 of 35.8. T3 red Lc, T6 green pair dE, T6 yellow pair dE, T7 deutan yellow/green, brightYellow chroma, brightYellow/brightWhite, brightYellow/foreground, green/yellow, red/yellow',
    'deuteranopia monokai':
      'red/green 22.5 of 39.6, red/brightYellow 19.7 of 26.6, red/yellow 12.5 of 20.9. T3 red Lc, T6 green bright step dL, T6 green pair dE, T6 yellow pair dE, T7 deutan yellow/green, brightYellow chroma, brightYellow/brightWhite, brightYellow/foreground, brightYellow/white, green/brightYellow, green/yellow',
    'deuteranopia one-half-dark':
      'red/green 21.2 of 30.8, red/brightYellow 21.5 of 25.4, red/yellow 12.1 of 17.9. T3 red Lc, T6 green bright step dL, T6 green pair dE, T6 yellow pair dE, T7 deutan yellow/green, brightYellow/brightWhite, brightYellow/foreground, brightYellow/white, green/brightWhite, green/yellow',
    'deuteranopia solarized-light':
      'red/green 32.3 of 35.1. T3 red Lc, T6 green pair dE, green/brightYellow',
    'deuteranopia tango-dark':
      'red/green 20.8 of 38.9, red/brightYellow 27.4 of 34.7, red/yellow 12 of 23.9. T3 red Lc, T6 green bright step dL, T6 green pair dE, T7 deutan red/yellow, T7 deutan yellow/green, brightYellow chroma, brightYellow/brightWhite, green/brightYellow, green/yellow, red/yellow',
    'deuteranopia classic-vivid':
      'red/green 11.7 of 35.9, red/brightYellow 30 of 40.5, red/yellow 20.7 of 24.4. T3 red Lc, T6 green pair dE, T7 deutan yellow/green, brightYellow chroma, brightYellow/brightWhite, green/yellow, red/brightYellow, yellow chroma',
    'deuteranopia high-contrast':
      'red/green 14.1 of 40.8, red/brightYellow 28.9 of 37.8, red/yellow 25.9 of 37.4. T3 red Lc, T6 green pair dE, T6 yellow bright step dL, T6 yellow pair dE, brightYellow chroma, brightYellow/brightWhite, brightYellow/foreground, green/brightYellow, green/yellow',
    'deuteranopia everforest-dark':
      'red/green 21.5 of 28.2, red/brightYellow 21.8 of 24.8, red/yellow 12 of 16.8. T3 red Lc, T6 green pair dE, T6 yellow pair dE, T7 deutan yellow/green, brightYellow/brightWhite, green/brightWhite, green/yellow, yellow/foreground, yellow/white',
    'deuteranopia green-screen':
      'red/green 10 of 35.8, red/brightYellow 27 of 37.2, red/yellow 18.6 of 22.8. T3 red Lc, T6 green pair dE, T7 deutan red/green, T7 deutan yellow/green, brightYellow chroma, brightYellow/brightWhite, green/foreground, green/yellow, red/green, yellow chroma',
    'deuteranopia srcery':
      'red/green 19.8 of 36.1, red/brightYellow 21.4 of 28.9, red/yellow 12.7 of 21.7. T3 red Lc, T6 yellow pair dE, brightYellow/brightWhite, brightYellow/foreground, green hue limit, green/brightWhite, green/foreground, trade',
    'deuteranopia nightfly':
      'red/green 12.2 of 30, red/brightYellow 29.1 of 34.5, red/yellow 22.3 of 30.3. T3 red Lc, T6 yellow pair dE, T7 deutan yellow/green, brightYellow chroma, brightYellow/brightWhite, green/yellow, red hue limit',
    'deuteranopia melange-dark':
      'red/green 17.6 of 21.6. T3 red Lc, T6 green pair dE, cyan/green, green/brightYellow, green/foreground',
    'deuteranopia melange-light':
      'red/green 29.4 of 30.8. T3 red Lc, green/brightWhite, green/foreground',
    'deuteranopia modus-vivendi':
      'red/green 19.1 of 38.8, red/brightYellow 19.5 of 26, red/yellow 12.2 of 25. T3 red Lc, T6 green bright step dL, T6 green pair dE, T6 yellow pair dE, T7 deutan yellow/green, green/brightYellow, green/yellow',
    'deuteranopia harbor-dark':
      'red/green 19.5 of 36.2, red/brightYellow 25.3 of 25.7, red/yellow 12 of 17.7. T3 red Lc, T6 green pair dE, T7 deutan yellow/green, brightYellow chroma, brightYellow/brightWhite, brightYellow/foreground, green/brightYellow, green/yellow',
    'deuteranopia iceberg-dark':
      'red/green 22.9 of 27.6, red/brightYellow 22.5 of 23.5, red/yellow 12.7 of 14.3. T3 red Lc, T6 green pair dE, T6 yellow pair dE, T7 deutan yellow/green, brightYellow/brightWhite, brightYellow/foreground, green/brightWhite, green/yellow',
    'protanopia obsidian-ember':
      'red/green 28.3 of 28.6. T3 red Lc, T6 green pair dE, green/brightWhite, green/foreground',
    'protanopia triad':
      'red/green 20 of 30.3. T3 red Lc, T7 protan yellow/green, cyan/green, green hue limit, green/foreground, green/white, green/yellow, red hue limit',
    'protanopia rubric':
      'red/green 12.3 of 29.1, red/brightYellow 32 of 37.5, red/yellow 21.8 of 26.9. T3 red Lc, T6 yellow pair dE, brightYellow/foreground, cyan/green, green hue limit, green/white, red hue limit',
    'protanopia nord': 'red/green 17.6 of 19.7. T3 red Lc, T7 protan yellow/green, green/yellow',
    'protanopia rose-pine':
      'red/green 16.9 of 23. T3 red Lc, T6 green pair dE, cyan/green, green/white, red/yellow',
    'protanopia catppuccin':
      'red/green 19.4 of 24.3. T3 red Lc, T6 green pair dE, T7 protan yellow/green, green/yellow',
    'protanopia dracula': 'red/green 27.7 of 39.1. T3 red Lc, T7 protan yellow/green, green/yellow',
    'protanopia monokai':
      'red/green 35.6 of 39.6, red/yellow 20.2 of 20.9. T3 red Lc, T6 green bright step dL, T6 green pair dE, T6 yellow pair dE',
    'protanopia one-half-dark':
      'red/green 30.7 of 30.8, red/yellow 17.8 of 17.9. T3 red Lc, T6 green bright step dL, T6 green pair dE, T6 yellow pair dE, green/foreground, green/white',
    'protanopia solarized-light':
      'red/green 21.8 of 35.1, red/yellow 12.9 of 21.9. T3 red Lc, T6 green pair dE, T7 protan yellow/green, green/yellow',
    'protanopia classic-vivid':
      'red/green 26.1 of 35.9, red/brightYellow 40.4 of 40.5. T3 red Lc, T7 protan yellow/green, brightYellow chroma, brightYellow/brightWhite, green/yellow',
    'protanopia high-contrast':
      'red/green 28.7 of 40.8. T3 red Lc, T6 green pair dE, T7 protan yellow/green, cyan/green, green/brightYellow, green/yellow',
    'protanopia green-screen':
      'red/green 25.7 of 35.8. T3 red Lc, T6 green pair dE, T7 protan yellow/green, green/brightYellow, green/foreground, green/yellow',
    'protanopia srcery':
      'red/green 32.9 of 36.1. T3 red Lc, T6 green pair dE, green/brightWhite, green/foreground',
    'protanopia nightfly': 'red/green 24.2 of 30. T3 red Lc, T7 protan yellow/green, green/yellow',
    'protanopia melange-dark':
      'red/yellow 12.4 of 12.9. T3 red Lc, T6 yellow pair dE, yellow/white',
    'protanopia melange-light': 'red/green 23.9 of 30.8. T3 red Lc, green/foreground',
    'protanopia modus-vivendi':
      'red/green 34.7 of 38.8, red/yellow 22.1 of 25. T3 red Lc, T6 green bright step dL, T6 green pair dE, T6 yellow pair dE',
    'protanopia harbor-dark':
      'red/green 33.4 of 36.2. T3 red Lc, T6 green pair dE, green/foreground',
    'tritanopia obsidian-ember':
      'red/brightYellow 23.1 of 23.9, red/yellow 14.3 of 15.9, cyan/green 8.6 of 11.9. T3 red Lc, T6 green pair dE, T6 yellow pair dE, brightYellow/brightWhite, brightYellow/foreground, trade, yellow/foreground',
    'tritanopia kanso-zen':
      'red/brightYellow 26.6 of 27.2, cyan/green 14.5 of 14.7. T2 cyan Lc, T3 red Lc, T6 green pair dE, brightYellow chroma, brightYellow/brightWhite, cyan/blue, green/brightWhite, green/brightYellow',
    'tritanopia tokyo-night':
      'red/green 32 of 33.2, red/brightYellow 23 of 25.1, red/yellow 15.1 of 18.1, cyan/green 14.9 of 22.5. T2 cyan Lc, T3 red Lc, T6 green pair dE, T6 yellow pair dE, brightYellow/brightWhite, cyan/green, green/brightWhite, green/brightYellow',
    'tritanopia nord':
      'red/brightYellow 33.8 of 34.4, red/yellow 24.7 of 26. T3 red Lc, T6 yellow pair dE, brightYellow chroma, brightYellow/brightWhite, yellow/foreground, yellow/white',
    'tritanopia rose-pine':
      'red/brightYellow 26.6 of 26.7, red/yellow 17.3 of 20.7. T3 red Lc, T6 yellow pair dE, brightYellow chroma, brightYellow/brightWhite, brightYellow/foreground, brightYellow/white, trade',
    'tritanopia gruvbox':
      'red/brightYellow 25.1 of 28.6, red/yellow 17.5 of 19.6, cyan/blue 11.3 of 11.7. T3 blue Lc, T3 red Lc, T6 cyan pair dE, T6 yellow pair dE, brightYellow/foreground, trade, yellow/white',
    'tritanopia catppuccin':
      'red/brightYellow 31.9 of 32.1, red/yellow 25.7 of 26.9, cyan/green 11.2 of 11.8. T2 green Lc, T3 red Lc, T6 cyan pair dE, T6 yellow pair dE, brightYellow chroma, brightYellow/brightWhite, trade, yellow/brightWhite',
    'tritanopia dracula': 'cyan/green 10.4 of 21.8. T2 green Lc, T6 cyan pair dE',
    'tritanopia monokai':
      'red/yellow 18.2 of 20.9, cyan/green 14.7 of 19.7. T2 cyan Lc, T3 red Lc, T6 green bright step dL, T6 green pair dE, T6 yellow pair dE, green chroma',
    'tritanopia one-half-dark':
      'red/brightYellow 24.2 of 25.4, red/yellow 16.2 of 17.9, cyan/green 17.4 of 18.9, cyan/blue 7.3 of 8.9. T2 cyan Lc, T3 blue Lc, T3 red Lc, T6 cyan pair dE, T6 green bright step dL, T6 green pair dE, T6 yellow pair dE, brightYellow/brightWhite, brightYellow/foreground, brightYellow/white, cyan/green, green/brightWhite, green/brightYellow',
    'tritanopia solarized-light':
      'red/green 32.4 of 35.1, red/yellow 21.1 of 21.9, cyan/blue 11.3 of 12.2. T3 blue Lc, T3 red Lc, T6 cyan pair dE, T6 green pair dE, cyan/green, cyan/white, green/brightYellow, green/foreground, green/white, green/yellow',
    'tritanopia tango-dark':
      'cyan/green 12.5 of 19.1, cyan/blue 8.7 of 13. T2 cyan Lc, T3 blue Lc, T6 cyan pair dE, T6 green bright step dL, T6 green pair dE, cyan/blue, cyan/green',
    'tritanopia classic-vivid':
      'red/brightYellow 37.5 of 40.5, cyan/green 17.4 of 19.5, cyan/blue 6.2 of 19.4. T2 cyan Lc, T3 blue Lc, T3 red Lc, T6 green bright step dL, T6 green pair dE, brightYellow chroma, brightYellow/brightWhite, cyan/blue, trade',
    'tritanopia high-contrast':
      'red/yellow 36.5 of 37.4, cyan/green 11.5 of 22.3. T3 red Lc, T6 cyan pair dE, T6 yellow bright step dL, T6 yellow pair dE, red/green, yellow/brightWhite',
    'tritanopia everforest-dark':
      'red/brightYellow 24 of 24.8, red/yellow 15.7 of 16.8. T3 red Lc, T6 yellow pair dE, brightYellow/brightWhite, brightYellow/foreground, green/brightYellow, yellow/foreground, yellow/white',
    'tritanopia green-screen':
      'red/brightYellow 36.5 of 37.2, cyan/green 14.2 of 19.3, cyan/blue 5.9 of 17.4. T2 cyan Lc, T3 blue Lc, T3 red Lc, T6 green bright step dL, T6 green pair dE, brightYellow chroma, brightYellow/brightWhite, cyan/blue, trade',
    'tritanopia srcery':
      'red/yellow 19.8 of 21.7, cyan/green 9.8 of 15, cyan/blue 11.5 of 13.4. T3 blue Lc, T3 red Lc, T6 cyan pair dE, T6 green bright step dL, T6 green pair dE, T6 yellow pair dE, T7 tritan cyan/green, cyan/green, trade, yellow/white',
    'tritanopia nightfly':
      'cyan/green 11.6 of 12.8, cyan/blue 15.7 of 17.7. T2 green Lc, T3 blue Lc, T6 cyan pair dE',
    'tritanopia melange-dark':
      'red/brightYellow 18.5 of 21.8, red/yellow 10.8 of 12.9, cyan/green 8.4 of 9.1, cyan/blue 8.6 of 9.9. T2 cyan Lc, T3 blue Lc, T3 red Lc, T6 cyan pair dE, T6 green pair dE, T6 yellow pair dE, T7 tritan cyan/green, brightYellow/foreground, cyan/blue, cyan/green, cyan/white, green/foreground, yellow/white',
    'tritanopia modus-vivendi':
      'red/brightYellow 25.9 of 26, red/yellow 22.4 of 25, cyan/green 9.5 of 18.2, cyan/blue 11 of 14.1. T3 blue Lc, T3 red Lc, T6 cyan pair dE, T6 green bright step dL, T6 green pair dE, T6 yellow pair dE, T7 tritan cyan/green, brightYellow/foreground, cyan/green, trade',
    'tritanopia harbor-dark':
      'red/brightYellow 24.1 of 25.7, red/yellow 15.4 of 17.7, cyan/green 10.1 of 18.7, cyan/blue 11.7 of 12.9. T3 blue Lc, T3 red Lc, T6 cyan pair dE, T6 green bright step dL, T6 green pair dE, T6 yellow pair dE, brightYellow/foreground, cyan/green, green hue limit, trade',
    'tritanopia iceberg-dark':
      'red/brightYellow 22.3 of 23.5, red/yellow 13.5 of 14.3, cyan/green 13.1 of 14.4, cyan/blue 8.6 of 9.1. T2 cyan Lc, T3 blue Lc, T3 red Lc, T6 cyan pair dE, T6 green pair dE, T6 yellow pair dE, brightYellow/foreground, brightYellow/white, cyan/blue, cyan/foreground, cyan/green, cyan/white, green/brightWhite, green/brightYellow',
  };

  it('parts every pair as far as a typical eye sees it in the Typical fit, or names the pair it cannot reach and why', () => {
    const report: Record<string, string> = {};
    for (const vision of OTHER) {
      for (const theme of fitting) {
        const typical = playPalette(theme, true);
        const own = playPalette(theme, true, vision);
        const missed = visionChecks(own, typical, vision).filter((c) => !c.ok);
        if (missed.length === 0) continue;
        const why = new Set<string>();
        for (const c of missed) {
          const [a, b] = c.id.split('/') as [AnsiSlot, AnsiSlot];
          gameBlockers(theme, vision, a, b).forEach((w) => why.add(w));
        }
        const pairs = missed.map((c) => `${c.id} ${c.value} of ${c.need}`).join(', ');
        report[`${vision} ${theme.id}`] = `${pairs}. ${[...why].sort().join(', ')}`;
      }
    }
    expect(report).toEqual(GAME_SHORT);
  });

  // Where the Typical fit misses a target, the fit for the vision moves
  // something, except where GAME_SHORT shows every step that would part
  // the pair further giving up a floor or a guard the Typical fit holds.
  // Their window changes all the same.
  it('changes every theme for every vision its Typical fit misses, and none it holds', () => {
    const kept: string[] = [];
    for (const vision of OTHER) {
      for (const theme of fitting) {
        const typical = playPalette(theme, true);
        const holds = holdsVision(typical, typical, vision);
        const same = visionFitOf(theme, vision) === theme.fitted;
        if (holds) expect(same, `${vision} ${theme.id}`).toBe(true);
        else if (same) kept.push(`${vision} ${theme.id}`);
      }
    }
    expect(kept).toEqual([
      'deuteranopia monokai',
      'deuteranopia one-half-dark',
      'deuteranopia everforest-dark',
      'protanopia obsidian-ember',
      'protanopia nord',
      'protanopia catppuccin',
      'protanopia dracula',
      'protanopia monokai',
      'protanopia one-half-dark',
      'protanopia classic-vivid',
      'protanopia green-screen',
      'protanopia nightfly',
      'protanopia melange-dark',
    ]);
    for (const at of kept) {
      expect(GAME_SHORT[at], at).not.toContain('trade');
      const [vision, id] = at.split(' ') as [ColorVision, string];
      const theme = findTheme(id);
      const window = CHROME_COLOR_KEYS.some(
        (key) => themeTokens(theme, vision)[key] !== themeTokens(theme)[key],
      );
      expect(window, at).toBe(true);
    }
  });

  it('changes Kanso Zen under all three visions, in the game text or the window', () => {
    const kanso = findTheme('kanso-zen');
    for (const vision of OTHER) {
      const text = playPalette(kanso, true, vision) !== playPalette(kanso, true);
      const window = CHROME_COLOR_KEYS.some(
        (key) => themeTokens(kanso, vision)[key] !== themeTokens(kanso)[key],
      );
      expect(text || window, vision).toBe(true);
    }
    // Deuteranopia parts danger from success in the window, which a
    // deuteranope sees 2.9 apart under Typical.
    const typical = themeTokens(kanso);
    const deut = themeTokens(kanso, 'deuteranopia');
    expect(seenApart(hex(typical.danger), hex(typical.success), 'deuteranopia')).toBeCloseTo(
      2.9,
      1,
    );
    expect(seenApart(hex(deut.danger), hex(deut.success), 'deuteranopia')).toBeGreaterThan(20.2);
  });
});

describe('window status colors for a color vision', () => {
  const OTHER: ColorVision[] = ['deuteranopia', 'protanopia', 'tritanopia'];
  const sees = (a: string, b: string, vision: ColorVision) => seenApart(hex(a), hex(b), vision);
  const FAMILY = { danger: 'red', success: 'green' } as const;

  it('keeps every floor, turns danger and success no further than their bound, and warn not at all', () => {
    for (const vision of OTHER) {
      for (const theme of BUILTIN_THEMES) {
        const t = themeTokens(theme);
        const v = themeTokens(theme, vision);
        for (const key of ['danger', 'warn', 'success'] as const) {
          const at = `${vision} ${theme.id} ${key}`;
          for (const ground of [v.panel, v.raised]) {
            const floor = Math.min(STATUS_CONTRAST, contrast(hex(t[key]), hex(ground)));
            expect(contrast(hex(v[key]), hex(ground)), at).toBeGreaterThanOrEqual(floor - 1e-9);
          }
          if (v[key] === t[key] || rgbToOklch(hex(t[key])).C < 0.05) continue;
          const dh = turned(t[key], v[key]);
          if (key === 'warn') {
            expect(Math.abs(dh), at).toBeLessThan(3);
            continue;
          }
          const room = turnRoom(FAMILY[key], rgbToOklch(hex(t[key])).h, HUE_TURN_FAR);
          expect(dh, at).toBeGreaterThan(-3);
          expect(dh, at).toBeLessThanOrEqual(room + 1.5);
        }
        for (const key of ['dangerText', 'warnText'] as const) {
          for (const ground of [v.panel, v.raised]) {
            const at = `${vision} ${theme.id} ${key}`;
            expect(contrast(hex(v[key]), hex(ground)), at).toBeGreaterThanOrEqual(
              STATUS_TEXT_CONTRAST,
            );
          }
        }
      }
    }
  });

  // Danger and success, and danger and warn, never come nearer than the
  // Typical colors stand, unless past their target, and warn and success
  // never under VISION_GUARD, or nearer than they stood if less.
  it('brings no pair of status colors nearer than its guard', () => {
    for (const vision of OTHER) {
      for (const theme of BUILTIN_THEMES) {
        const t = themeTokens(theme);
        const v = themeTokens(theme, vision);
        const at = `${vision} ${theme.id}`;
        for (const other of ['success', 'warn'] as const) {
          const floor = Math.min(
            sees(t.danger, t[other], vision),
            deltaEOk(hex(t.danger), hex(t[other])),
          );
          expect(sees(v.danger, v[other], vision), `${at} danger/${other}`).toBeGreaterThanOrEqual(
            floor - 1e-9,
          );
        }
        const apart = Math.min(sees(t.warn, t.success, vision), VISION_GUARD);
        expect(sees(v.warn, v.success, vision), `${at} warn/success`).toBeGreaterThanOrEqual(
          apart - 1e-9,
        );
      }
    }
  });

  // The accent the rule picks stands ACCENT_APART from every status color
  // as the vision sees them. A pinned accent stays as the theme drew it,
  // and no status color comes nearer to it than the Typical one stands,
  // up to ACCENT_APART.
  it('keeps the accent apart from every status color as the vision sees them', () => {
    for (const vision of OTHER) {
      for (const theme of BUILTIN_THEMES) {
        const t = themeTokens(theme);
        const v = themeTokens(theme, vision);
        for (const key of ['danger', 'warn', 'success'] as const) {
          const at = `${vision} ${theme.id} accent from ${key}`;
          if (theme.chrome?.accent === undefined) {
            expect(sees(v.accent, v[key], vision), at).toBeGreaterThanOrEqual(ACCENT_APART);
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
  });

  // What stops each step that would part danger from `other` further:
  // the 3:1 floor on the panel or raised, a guard on another pair, the
  // accent, the hue limit, lost chroma, or trade where nothing stops it
  // and the window spent the room on the other pair.
  const windowBlockers = (theme: AppTheme, vision: ColorVision, other: 'success' | 'warn') => {
    const t = themeTokens(theme);
    const v = themeTokens(theme, vision);
    // The status colors keep clear of a pinned accent, and of the one the
    // rule picks for Typical.
    const pinned = theme.chrome?.accent !== undefined;
    const now = sees(v.danger, v[other], vision);
    const rest = other === 'success' ? 'warn' : 'success';
    const restFloor = Math.min(
      sees(t.danger, t[rest], vision),
      deltaEOk(hex(t.danger), hex(t[rest])),
    );
    const out = new Set<string>();
    for (const key of ['danger', other] as const) {
      const lch = rgbToOklch(hex(v[key]));
      const family = key === 'warn' ? null : FAMILY[key];
      const steps: [string, string][] = [-0.02, 0.02].map((d) => [
        'L',
        toHex(oklchToRgbInGamut({ ...lch, L: Math.max(0, Math.min(1, lch.L + d)) })),
      ]);
      if (family) steps.push(['turn', toHex(turnHue(hex(v[key]), 4))]);
      for (const [step, color] of steps) {
        const s = { danger: v.danger, warn: v.warn, success: v.success, [key]: color };
        if (sees(s.danger, s[other], vision) <= now + 0.05) continue;
        const why: string[] = [];
        for (const ground of [v.panel, v.raised]) {
          const floor = Math.min(STATUS_CONTRAST, contrast(hex(t[key]), hex(ground)));
          if (contrast(hex(color), hex(ground)) < floor) why.push(`${key} 3:1 floor`);
        }
        if (key === 'danger' && sees(s.danger, s[rest], vision) < restFloor) {
          why.push(`danger/${rest}`);
        }
        const apart = Math.min(sees(t.warn, t.success, vision), VISION_GUARD);
        if (sees(s.warn, s.success, vision) < apart) why.push('warn/success');
        const c0 = rgbToOklch(hex(t[key])).C;
        if (c0 > 0.04 && rgbToOklch(hex(color)).C < 0.6 * c0) why.push(`${key} chroma`);
        const accentFloor = Math.min(
          ACCENT_APART,
          deltaEOk(hex(t.accent), hex(t[key])),
          pinned ? sees(t.accent, t[key], vision) : Infinity,
        );
        if (sees(t.accent, color, vision) < accentFloor) why.push('accent');
        if (step === 'turn' && family) {
          const room = turnRoom(family, rgbToOklch(hex(t[key])).h, HUE_TURN_FAR);
          if (turned(t[key], v[key]) + 4 > room + 1) why.push(`${key} hue limit`);
        }
        if (why.length === 0) why.push('trade');
        why.forEach((w) => out.add(w));
      }
    }
    return [...out].sort();
  };

  // Each pair the window leaves short of its target for each vision, how
  // far it gets of how far it needs, and what stops it going further.
  const WINDOW_SHORT: Record<string, string> = {
    'deuteranopia dracula':
      'danger/success 34.0 of 37.8. danger 3:1 floor, danger hue limit, success chroma, trade',
    'deuteranopia monokai':
      'danger/success 30.4 of 43.4, danger/warn 27.7 of 29.5. danger 3:1 floor, success chroma, warn chroma, warn/success',
    'deuteranopia tango-dark':
      'danger/success 23.5 of 37.5, danger/warn 24.4 of 34.2. accent, danger 3:1 floor, danger hue limit, success chroma, success hue limit, warn chroma, warn/success',
    'deuteranopia classic-vivid':
      'danger/success 36.5 of 52.0, danger/warn 42.1 of 45.7. danger 3:1 floor, danger hue limit, success chroma, warn chroma, warn/success',
    'deuteranopia green-screen':
      'danger/success 37.5 of 44.2. danger 3:1 floor, danger hue limit, success chroma',
    'protanopia rubric': 'danger/warn 24.3 of 26.9. danger chroma, warn/success',
    'tritanopia classic-vivid':
      'danger/success 51.1 of 52.0, danger/warn 45.2 of 45.7. danger 3:1 floor, danger hue limit, success hue limit, warn chroma',
  };

  it('parts danger from success and warn as far as a typical eye sees them, or names the pair it cannot reach and why', () => {
    const report: Record<string, string> = {};
    for (const vision of OTHER) {
      for (const theme of BUILTIN_THEMES) {
        const t = themeTokens(theme);
        const v = themeTokens(theme, vision);
        const missed: string[] = [];
        const why = new Set<string>();
        for (const other of ['success', 'warn'] as const) {
          const need = deltaEOk(hex(t.danger), hex(t[other]));
          const value = sees(v.danger, v[other], vision);
          if (value >= need) continue;
          missed.push(`danger/${other} ${value.toFixed(1)} of ${need.toFixed(1)}`);
          windowBlockers(theme, vision, other).forEach((w) => why.add(w));
        }
        if (missed.length > 0) {
          report[`${vision} ${theme.id}`] = `${missed.join(', ')}. ${[...why].sort().join(', ')}`;
        }
      }
    }
    expect(report).toEqual(WINDOW_SHORT);
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

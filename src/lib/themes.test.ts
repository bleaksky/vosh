import { describe, expect, it } from 'vitest';
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
  parseHex,
  rgbToOklab,
  rgbToOklch,
  WHITE,
  type Rgb,
} from './color';
import { checks, GAME_SLOTS } from './gameFit';
import {
  BUILTIN_THEMES,
  customThemeLabel,
  customToAppTheme,
  findTheme,
  migrateCustomChrome,
  playPalette,
  themeTokens,
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
// on the ground, the lines step in lightness, the title takes the
// secondary tone, and Vellum floats on its paper instead of white.
// Ember's is the sheet the board draws, and Nord's and Vellum's are the
// rule's with their pins. The selection is each scheme's own, opaque,
// with its own text (Q9), where the canvas drew the accent with alpha.
// The control washes (Q10) on Ember and Vellum are the ones the
// stylesheets fixed per appearance, but for Vellum's field, now its
// raised paper and no longer white, and on Nord they are the rule's.
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
    edge: 'rgba(255, 255, 255, 0.219)',
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
    edge: 'rgba(255, 255, 255, 0.14)',
  },
};

const VELLUM: TokenSheet = {
  id: 'vellum',
  appearance: 'light',
  tokens: {
    bg: '#f7f4ee',
    panel: '#f7f4ee',
    sep: '#d2d0cb',
    divider: '#e3e0db',
    selrow: '#fffdfa',
    hover: '#e9e6e1',
    inputband: '#eeebe6',
    text: '#2a2622',
    secondary: '#5f5c57',
    tertiary: '#8c8984',
    title: '#5f5c57',
    raised: '#fffdfa',
    accent: '#3f6690',
    onAccent: '#ffffff',
    danger: '#a8453a',
    dangerText: '#a8453a',
    warn: '#94661a',
    warnText: '#94661a',
    success: '#4f7a3a',
    selection: '#a4b4c4',
    selectionText: '#2a2622',
    field: '#fffdfa',
    track: 'rgba(0, 0, 0, 0.14)',
    menuHi: 'rgba(0, 0, 0, 0.05)',
    keyRing: 'rgba(0, 0, 0, 0.12)',
    edge: 'rgba(0, 0, 0, 0.14)',
  },
};

// The canvas drew Ember's title in #8e8e8e on a second sheet. The title
// is the secondary tone now, so that sheet is gone.
const SHEETS: TokenSheet[] = [NORD, EMBER, VELLUM];

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
    const light = BUILTIN_THEMES.map(themeTokens).filter((t) => t.appearance === 'light');
    expect(light.length).toBeGreaterThan(0);
    for (const t of light) {
      expect(t.field).toBe(t.raised);
      expect(t.field).not.toBe('#ffffff');
    }
  });

  it('paints Obsidian Ember within dE 1 of the washes the stylesheets fixed', () => {
    const t = themeTokens(findTheme('obsidian-ember'));
    const fixed: Record<Wash, number> = {
      field: 0.06,
      track: 0.16,
      keyRing: 0.14,
      menuHi: 0.08,
      edge: 0.14,
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

describe('Everforest and Green Screen', () => {
  const NEW_THEMES = ['everforest-dark', 'everforest-light', 'green-screen'];
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
    expect(themeTokens(findTheme('everforest-light')).appearance).toBe('light');
    expect(themeTokens(findTheme('green-screen')).appearance).toBe('dark');
  });

  // The colors Everforest and CGA publish under 3:1 on their own ground.
  // The terminal draws them as published. The chat pane lifts them where
  // it draws them on the panel (chatColors.test.ts).
  const PUBLISHED_FAINT: Record<string, readonly (typeof WORD_SLOTS)[number][]> = {
    'everforest-light': [
      'green',
      'yellow',
      'magenta',
      'cyan',
      'brightGreen',
      'brightYellow',
      'brightMagenta',
      'brightCyan',
    ],
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
      red: '#f85552',
      green: '#8da101',
      yellow: '#dfa000',
      blue: '#3a94c5',
      magenta: '#df69ba',
      cyan: '#35a77c',
    };
    // Everforest repeats the six colors in the bright slots.
    const brights = Object.fromEntries(
      Object.entries(everforest).map(([slot, value]) => [
        `bright${slot[0].toUpperCase()}${slot.slice(1)}`,
        value,
      ]),
    );
    expect(findTheme('everforest-light').xterm).toMatchObject({ ...everforest, ...brights });
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
    expect(findTheme('everforest-light').xterm.selectionBackground).toBe('#eaedc8');
  });

  it('take Everforest green as the accent', () => {
    expect(themeTokens(findTheme('everforest-dark')).accent).toBe('#a7c080');
    // The published green lifted to 3:1. The pin keeps the shade the
    // chrome derived for success while menus floated on white, a step
    // darker than the one ground rule's success on the paper.
    const light = themeTokens(findTheme('everforest-light'));
    expect(light.accent).toBe('#809300');
    expect(Math.abs(hue(light.accent) - hue('#8da101'))).toBeLessThan(2);
    expect(contrast(hex(light.accent), hex(light.bg))).toBeGreaterThanOrEqual(3);
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
    // The green cursor becomes the accent and tints the selection.
    const t = themeTokens(findTheme('green-screen'));
    expect(t.accent).toBe(x.cursor);
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

// The fits are the Themes review's own (fit-survey.json), computed
// ahead by lib/gameFit. These pin what the decisions say of them.
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

  it('keeps the copyright line of Modus Vivendi', () => {
    expect(findTheme('modus-vivendi').license).toBe('GPL-3.0-or-later');
    expect(sectionFor('Modus Vivendi')?.body).toContain(
      'Copyright (C) 2019-2026 Free Software Foundation, Inc.',
    );
  });
});

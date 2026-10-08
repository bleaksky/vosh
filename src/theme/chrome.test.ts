import { describe, expect, it } from 'vitest';
import {
  ACCENT_APART,
  CHROME_COLOR_KEYS,
  deriveChrome,
  ON_ACCENT_CONTRAST,
  SECONDARY_CONTRAST,
  STATUS_CONTRAST,
  STATUS_PART,
  STATUS_TEXT_CONTRAST,
  TERTIARY_CONTRAST,
  tokensToCssVars,
  tokenVarName,
} from './chrome';
import { contrast, deltaEOk, parseHex, rgbToOklch, type Rgb } from './color';
import { seenApart, VISION_SLACK } from './gameFit';
import { DEFAULT_THEME_ID, findTheme, themeTokens } from './themes';
import tokensCss from '../styles/tokens.css?raw';

const hex = (h: string): Rgb => {
  const c = parseHex(h);
  if (!c) throw new Error(`not hex ${h}`);
  return c;
};

describe('derivation rules', () => {
  const ember = findTheme('obsidian-ember').xterm;
  const paper = { ...ember, background: '#f7f4ee', foreground: '#2a2622', cursor: '#3f6690' };

  it('reads appearance from the background lightness', () => {
    expect(deriveChrome(ember).appearance).toBe('dark');
    expect(deriveChrome(paper).appearance).toBe('light');
  });

  it('puts the panel on the terminal ground', () => {
    // One ground: the window is one surface with the terminal.
    expect(deriveChrome(ember).panel).toBe('#050403');
    expect(deriveChrome(paper).panel).toBe('#f7f4ee');
  });

  it('floats and selects a light theme on its paper, never on white', () => {
    const t = deriveChrome(paper);
    expect(t.raised).toBe('#fffdfa');
    expect(t.selrow).toBe(t.raised);
    expect(t.field).toBe(t.raised);
    const lift = rgbToOklch(hex(t.raised)).L - rgbToOklch(hex(paper.background)).L;
    expect(lift).toBeGreaterThan(0.02);
    expect(lift).toBeLessThan(0.035);
    // A white paper floats on a step under white.
    expect(deriveChrome({ ...paper, background: '#ffffff' }).raised).toBe('#fdfdfd');
  });

  it('steps the lines and fills off the ground in lightness', () => {
    const t = deriveChrome(ember);
    expect([t.sep, t.divider, t.hover, t.selrow, t.raised, t.inputband]).toEqual([
      '#1b1a19',
      '#100f0e',
      '#0b0b0a',
      '#121110',
      '#100f0e',
      '#080807',
    ]);
  });

  it('keeps the steps Obsidian Ember takes on a ground darker than its own', () => {
    // On pure black a step of 11 in OKLab L lands on #040404, so each
    // step keeps the alpha it takes on Ember's #050403 and a menu still
    // stands off the ground.
    const t = deriveChrome({ ...ember, background: '#000000' });
    expect([t.sep, t.divider, t.hover, t.selrow, t.raised, t.inputband]).toEqual([
      '#161616',
      '#0b0b0b',
      '#070707',
      '#0d0d0d',
      '#0b0b0b',
      '#040404',
    ]);
    // The control washes on the ground keep the alpha they take there.
    const e = deriveChrome(ember);
    expect([t.field, t.track, t.keyRing, t.edge]).toEqual([e.field, e.track, e.keyRing, e.edge]);
  });

  it('lays the control washes in white on dark and black on light', () => {
    expect(deriveChrome(ember).track).toBe('rgba(255, 255, 255, 0.158)');
    expect(deriveChrome(paper).track).toBe('rgba(0, 0, 0, 0.141)');
    // A pin wins as it stands.
    expect(deriveChrome(ember, { edge: '#333333' }).edge).toBe('#333333');
  });

  it('solves the text tiers against the panel', () => {
    for (const x of [ember, paper]) {
      const t = deriveChrome(x);
      const panel = hex(t.panel);
      expect(contrast(hex(t.secondary), panel)).toBeGreaterThanOrEqual(SECONDARY_CONTRAST);
      expect(contrast(hex(t.tertiary), panel)).toBeGreaterThanOrEqual(TERTIARY_CONTRAST);
      expect(contrast(hex(t.tertiary), panel)).toBeLessThan(SECONDARY_CONTRAST);
    }
  });

  it('lifts a foreground too dim for the secondary floor', () => {
    const dim = { ...ember, foreground: '#5a5856' };
    const t = deriveChrome(dim);
    expect(contrast(hex(t.text), hex(t.panel))).toBeGreaterThanOrEqual(SECONDARY_CONTRAST);
  });

  it('takes a colored cursor as the accent when it stands 12 dE from the status colors', () => {
    const t = deriveChrome(paper);
    expect(t.accent).toBe('#3f6690');
    for (const key of ['danger', 'warn', 'success'] as const) {
      expect(deltaEOk(hex(t.accent), hex(t[key])), key).toBeGreaterThanOrEqual(ACCENT_APART);
    }
  });

  it('else takes the scheme hue with the most chroma that stands as far', () => {
    // Ember's orange cursor sits 8.1 dE from its danger red, so the rule
    // passes it over for Ember's magenta. A pin keeps the orange.
    expect(deriveChrome(ember).accent).toBe('#b48ec9');
    expect(deriveChrome(ember, { accent: '#ef8f2f' }).accent).toBe('#ef8f2f');
    // A cursor without color of its own gives way the same.
    expect(deriveChrome({ ...ember, cursor: ember.foreground }).accent).toBe('#b48ec9');
    expect(deriveChrome({ ...ember, cursor: '#888888' }).accent).toBe('#b48ec9');
  });

  it('else falls back to bright blue', () => {
    // Every hue sits within 12 dE of the success green, bright blue too,
    // so none stands apart and bright blue wins as it is.
    const green = '#8fdaa8';
    const t = deriveChrome({
      ...ember,
      cursor: green,
      green,
      brightGreen: green,
      yellow: green,
      brightYellow: green,
      blue: green,
      brightBlue: '#7fcf9f',
      magenta: green,
      brightMagenta: green,
      cyan: green,
      brightCyan: green,
    });
    expect(t.success).toBe(green);
    expect(deltaEOk(hex('#7fcf9f'), hex(green))).toBeLessThan(ACCENT_APART);
    expect(t.accent).toBe('#7fcf9f');
  });

  it('inks the accent in white or a dark that reads at 4.5:1', () => {
    expect(deriveChrome(paper).onAccent).toBe('#ffffff');
    const pink = deriveChrome({ ...ember, cursor: '#f92672' });
    expect(contrast(hex(pink.onAccent), hex(pink.accent))).toBeGreaterThanOrEqual(
      ON_ACCENT_CONTRAST,
    );
  });

  it('lifts a status color that falls under 3:1 on the panel', () => {
    const dim = { ...ember, brightRed: '#401010' };
    const t = deriveChrome(dim);
    expect(contrast(hex(t.danger), hex(t.panel))).toBeGreaterThanOrEqual(STATUS_CONTRAST);
    expect(t.danger).not.toBe('#401010');
  });

  it('gives danger words their own tier at 4.5:1', () => {
    // Nord's red clears 3:1 as a dot but reads at about 3:1 as words.
    const nord = findTheme('nord');
    const { dangerText: _pinned, ...unpinned } = nord.chrome ?? {};
    const t = deriveChrome(nord.xterm, unpinned);
    expect(t.danger).toBe('#bf616a');
    expect(contrast(hex(t.danger), hex(t.panel))).toBeLessThan(STATUS_TEXT_CONTRAST);
    expect(contrast(hex(t.dangerText), hex(t.panel))).toBeGreaterThanOrEqual(STATUS_TEXT_CONTRAST);
    // A red that already reads as words keeps one color for both.
    expect(deriveChrome(ember).dangerText).toBe(deriveChrome(ember).danger);
    // Nord pins the tier to the lighter red its danger words take.
    expect(deriveChrome(nord.xterm, nord.chrome).dangerText).toBe('#dc8a92');
  });

  it('reads the normal ANSI slots on a light ground', () => {
    const t = deriveChrome({ ...paper, red: '#a8453a', brightRed: '#c2574a' });
    expect(t.danger).toBe('#a8453a');
  });

  it('lets an override pin a token and feed the ones built on it', () => {
    // Ember's selection set to its ground, so the selection takes the
    // accent.
    const t = deriveChrome(
      { ...ember, selectionBackground: ember.background },
      { panel: '#202020', accent: 'rgba(255, 0, 0, 0.5)' },
    );
    expect(t.panel).toBe('#202020');
    // The divider steps 6 in OKLab L off the pinned panel.
    expect(t.divider).toBe('#2f2f2f');
    // A non hex override passes through and the derivation keeps its
    // own accent, Ember's magenta, for anything built on it.
    expect(t.accent).toBe('rgba(255, 0, 0, 0.5)');
    expect(t.selection).toBe('#362b3a');
    // A pinned selection keeps the text the rule picks for it.
    const pinned = deriveChrome(ember, { selection: '#123456' });
    expect(pinned.selection).toBe('#123456');
    expect(pinned.selectionText).toBe('#f2efee');
  });

  it('draws the scheme selection opaque with its own text when the pair reads', () => {
    const t = deriveChrome(ember);
    expect(t.selection).toBe('#201d1c');
    expect(t.selectionText).toBe('#f2efee');
    // Without a selection text of its own, the foreground reads on it.
    expect(deriveChrome({ ...ember, selectionForeground: '' }).selectionText).toBe('#c0bdbb');
    // Near black a step counts by contrast, so #0c0c0c reads as a fill on
    // #000000 and #0a0a0a, 11.7 off it in OKLab L, does not.
    const black = { ...ember, background: '#000000' };
    expect(deriveChrome({ ...black, selectionBackground: '#0c0c0c' }).selection).toBe('#0c0c0c');
    expect(deriveChrome({ ...black, selectionBackground: '#0a0a0a' }).selection).not.toBe(
      '#0a0a0a',
    );
  });

  it('else draws the accent over the ground with the text tier on it', () => {
    // A selection under a step of 6 off the ground reads as no fill. The
    // accent, Ember's magenta, goes over the ground at 0.28.
    const flat = deriveChrome({ ...ember, selectionBackground: '#0b0a09' });
    expect(flat.selection).toBe('#362b3a');
    expect(flat.selectionText).toBe(flat.text);
    // The same for selection text under 4.5:1 on the scheme's selection.
    const dim = deriveChrome({ ...ember, selectionForeground: '#5a5856' });
    expect(dim.selection).toBe('#362b3a');
    expect(dim.selectionText).toBe('#c0bdbb');
    // A light theme puts its accent over the paper at 0.20.
    const light = deriveChrome({ ...paper, selectionBackground: paper.background });
    expect(light.selection).toBe('#d2d8db');
    expect(light.selectionText).toBe('#2a2622');
  });

  it('writes one var per token', () => {
    const vars = tokensToCssVars(deriveChrome(ember));
    expect(Object.keys(vars)).toHaveLength(CHROME_COLOR_KEYS.length);
    expect(vars['--bg']).toBe('#050403');
    expect(vars['--on-accent']).toBe('#050403');
    // The input band steps 2.5 in OKLab L off the ground.
    expect(vars['--inputband']).toBe('#080807');
  });
});

describe('status colors for a color vision', () => {
  const kanso = findTheme('kanso-zen').xterm;

  // Kanso Zen's danger and success look 2.9 apart to a deuteranope. The
  // window turns success blue and danger toward vermilion until they
  // stand STATUS_PART apart, and every token that only reads the ground
  // stays as it is. A tritanope keeps the Typical status colors.
  it('swaps danger and success for a deuteranope and leaves the ground tokens', () => {
    const typical = deriveChrome(kanso);
    const deutan = deriveChrome(kanso, {}, 'deuteranopia');
    expect(deriveChrome(kanso, {}, 'typical')).toEqual(typical);
    const apart = (t: typeof typical) => seenApart(hex(t.danger), hex(t.success), 'deuteranopia');
    expect(apart(typical)).toBeCloseTo(2.9, 1);
    expect(apart(deutan)).toBeGreaterThanOrEqual(
      Math.min(STATUS_PART, deltaEOk(hex(typical.danger), hex(typical.success))) - VISION_SLACK,
    );
    const hue = (c: string) => rgbToOklch(hex(c)).h;
    expect(hue(deutan.success)).toBeGreaterThanOrEqual(205);
    expect(hue(deutan.success)).toBeLessThanOrEqual(255);
    expect(hue(deutan.danger)).toBeGreaterThanOrEqual(30);
    expect(hue(deutan.danger)).toBeLessThanOrEqual(60);
    expect(deutan.warn).toBe(typical.warn);
    const tritan = deriveChrome(kanso, {}, 'tritanopia');
    for (const key of ['danger', 'warn', 'success'] as const) {
      expect(tritan[key], key).toBe(typical[key]);
    }
    for (const key of ['bg', 'panel', 'sep', 'text', 'secondary', 'tertiary', 'raised'] as const) {
      expect(deutan[key], key).toBe(typical[key]);
    }
    expect(contrast(hex(deutan.danger), hex(deutan.panel))).toBeGreaterThanOrEqual(STATUS_CONTRAST);
    expect(contrast(hex(deutan.dangerText), hex(deutan.panel))).toBeGreaterThanOrEqual(
      STATUS_TEXT_CONTRAST,
    );
  });

  it('keeps a status color pinned in a form other than hex as pinned', () => {
    const pins = { danger: 'rgb(228, 104, 118)', success: 'var(--x)' };
    for (const vision of ['deuteranopia', 'protanopia', 'tritanopia'] as const) {
      const t = deriveChrome(kanso, pins, vision);
      expect(t.danger, vision).toBe('rgb(228, 104, 118)');
      expect(t.success, vision).toBe('var(--x)');
    }
  });
});

// A window paints from styles/tokens.css until theme.ts writes the
// derived tokens on :root, so a stylesheet default that drifts from the
// derivation flashes the wrong color at startup in the default theme.
describe('the stylesheet defaults', () => {
  /** The custom properties the first :root block of tokens.css sets. */
  function rootVars(css: string): Map<string, string> {
    const block = /:root\s*\{([^}]*)\}/.exec(css)?.[1] ?? '';
    const vars = new Map<string, string>();
    for (const m of block.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) vars.set(m[1], m[2].trim());
    return vars;
  }

  const theme = findTheme(DEFAULT_THEME_ID);
  const derived = tokensToCssVars(themeTokens(theme));
  const css = rootVars(tokensCss);

  it('paint every color token as the default theme derives it', () => {
    expect(theme.id).toBe(DEFAULT_THEME_ID);
    for (const key of CHROME_COLOR_KEYS) {
      const name = tokenVarName(key);
      expect(css.get(name)?.toLowerCase(), name).toBe(derived[name].toLowerCase());
    }
  });

  it('paint the terminal ground the default theme gives xterm', () => {
    expect(css.get('--bg')?.toLowerCase()).toBe(theme.xterm.background?.toLowerCase());
    expect(tokensCss).toMatch(/--xterm-bg:\s*var\(--bg\);/);
  });
});

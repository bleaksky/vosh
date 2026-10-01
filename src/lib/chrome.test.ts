import { describe, expect, it } from 'vitest';
import {
  CHROME_COLOR_KEYS,
  deriveChrome,
  ON_ACCENT_CONTRAST,
  SECONDARY_CONTRAST,
  STATUS_CONTRAST,
  STATUS_TEXT_CONTRAST,
  TERTIARY_CONTRAST,
  tokensToCssVars,
  tokenVarName,
} from './chrome';
import { contrast, parseHex, type Rgb } from './color';
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

  it('shifts the panel off the terminal ground', () => {
    expect(deriveChrome(ember).panel).toBe('#0c0a08');
    expect(deriveChrome(paper).panel).toBe('#f0ede7');
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

  it('takes a colored cursor as the accent, else bright blue', () => {
    expect(deriveChrome(ember).accent).toBe('#ef8f2f');
    const neutral = { ...ember, cursor: ember.foreground };
    expect(deriveChrome(neutral).accent).toBe(ember.brightBlue);
    const gray = { ...ember, cursor: '#888888' };
    expect(deriveChrome(gray).accent).toBe(ember.brightBlue);
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
    // Nord pins the tier to the red the approved boards draw words in.
    expect(deriveChrome(nord.xterm, nord.chrome).dangerText).toBe('#dc8a92');
  });

  it('reads the normal ANSI slots on a light ground', () => {
    const t = deriveChrome({ ...paper, red: '#a8453a', brightRed: '#c2574a' });
    expect(t.danger).toBe('#a8453a');
  });

  it('lets an override pin a token and feed the ones built on it', () => {
    const t = deriveChrome(ember, { panel: '#202020', accent: 'rgba(255, 0, 0, 0.5)' });
    expect(t.panel).toBe('#202020');
    expect(t.divider).toBe('#2b2b2b');
    // A non hex override passes through and the derivation keeps the
    // cursor accent for anything built on it.
    expect(t.accent).toBe('rgba(255, 0, 0, 0.5)');
    expect(t.selection).toBe('rgba(239, 143, 47, 0.22)');
  });

  it('writes one var per token', () => {
    const vars = tokensToCssVars(deriveChrome(ember));
    expect(Object.keys(vars)).toHaveLength(CHROME_COLOR_KEYS.length);
    expect(vars['--bg']).toBe('#050403');
    expect(vars['--on-accent']).toBe('#050403');
    expect(vars['--inputband']).toBe('#0f0e0d');
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

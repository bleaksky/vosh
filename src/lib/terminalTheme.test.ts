import { describe, expect, it } from 'vitest';
import { ansi16Of, nativeThemeOf, xtermThemeFor } from './terminalTheme';
import { BUILTIN_THEMES, findTheme, themeTokens } from './themes';

describe('the xterm theme', () => {
  it('selects in the opaque token pair, with the theme colors for MUD text on or off', () => {
    for (const id of ['obsidian-ember', 'kanso-zen', 'solarized-dark', 'vellum']) {
      const theme = findTheme(id);
      const tokens = themeTokens(theme);
      for (const tinted of [true, false]) {
        for (const fit of [true, false]) {
          const x = xtermThemeFor(theme, tinted, fit);
          expect(x.selectionBackground, id).toBe(tokens.selection);
          expect(x.selectionForeground, id).toBe(tokens.selectionText);
          expect(x.selectionBackground, id).toMatch(/^#[0-9a-f]{6}$/);
        }
      }
    }
    // Ember selects in its own selection with its own text, where xterm
    // drew the selection at 40 percent before.
    const ember = xtermThemeFor(findTheme('obsidian-ember'), true, true);
    expect(ember.selectionBackground).toBe('#201d1c');
    expect(ember.selectionForeground).toBe('#f2efee');
  });

  it('draws the fitted slots and body text while Fit game colors is on', () => {
    const kanso = findTheme('kanso-zen');
    const fitted = xtermThemeFor(kanso, true, true);
    expect(fitted.foreground).toBe('#c9cdcb');
    expect(fitted.brightBlack).toBe('#92979d');
    expect(fitted.brightWhite).toBe('#f0f5f2');
    const published = xtermThemeFor(kanso, true, false);
    expect(published.foreground).toBe(kanso.xterm.foreground);
    expect(published.brightBlack).toBe(kanso.xterm.brightBlack);
    // The base palette replaces the 16 colors while the theme's colors
    // are off, and the fitted body text stays.
    const base = xtermThemeFor(kanso, false, true);
    expect(base.brightBlack).not.toBe('#92979d');
    expect(base.foreground).toBe('#c9cdcb');
  });

  it('hands the native grid the 16 colors and body text xterm draws', () => {
    for (const theme of BUILTIN_THEMES) {
      for (const tinted of [true, false]) {
        for (const fit of [true, false]) {
          const x = xtermThemeFor(theme, tinted, fit);
          const native = nativeThemeOf(theme, tinted, fit);
          const at = `${theme.id} tinted ${tinted} fit ${fit}`;
          expect(native.ansi, at).toEqual(ansi16Of(x));
          expect(native.foreground, at).toBe(x.foreground);
          expect(native.background, at).toBe(x.background);
          expect(native.selection, at).toBe(x.selectionBackground);
        }
      }
    }
  });
});

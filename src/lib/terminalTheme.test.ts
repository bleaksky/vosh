import { describe, expect, it } from 'vitest';
import { xtermThemeFor } from './terminalTheme';
import { findTheme, themeTokens } from './themes';

describe('the xterm theme', () => {
  it('selects in the opaque token pair, with the theme colors for MUD text on or off', () => {
    for (const id of ['obsidian-ember', 'kanso-zen', 'solarized-dark', 'vellum']) {
      const theme = findTheme(id);
      const tokens = themeTokens(theme);
      for (const tinted of [true, false]) {
        const x = xtermThemeFor(theme, tinted);
        expect(x.selectionBackground, id).toBe(tokens.selection);
        expect(x.selectionForeground, id).toBe(tokens.selectionText);
        expect(x.selectionBackground, id).toMatch(/^#[0-9a-f]{6}$/);
      }
    }
    // Ember selects in its own selection with its own text, where xterm
    // drew the selection at 40 percent before.
    const ember = xtermThemeFor(findTheme('obsidian-ember'), true);
    expect(ember.selectionBackground).toBe('#201d1c');
    expect(ember.selectionForeground).toBe('#f2efee');
  });
});

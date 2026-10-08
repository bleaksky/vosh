import { describe, expect, it } from 'vitest';
import { galleryThemes, themeThumb } from './themeThumb';
import { BUILTIN_THEMES, customToAppTheme, findTheme } from './themes';

describe('themeThumb', () => {
  // The thumbnail paints these exact colors.
  it('matches the board for Nord', () => {
    expect(themeThumb(findTheme('nord'))).toEqual({
      bg: '#2e3440',
      panel: '#2e3440',
      sep: '#434c5e',
      accent: '#88c0d0',
      text: '#e5e9f0',
      appearance: 'dark',
      ring: 'rgba(255,255,255,0.10)',
    });
  });

  it('matches the board for Obsidian Ember', () => {
    expect(themeThumb(findTheme('obsidian-ember'))).toEqual({
      bg: '#050403',
      // The panel sits on the ground under the one ground rule, and the
      // line steps 11 in OKLab L off it.
      panel: '#050403',
      sep: '#1b1a19',
      accent: '#ef8f2f',
      text: '#c0bdbb',
      appearance: 'dark',
      ring: 'rgba(255,255,255,0.10)',
    });
  });

  it('matches the shortlist for Rubric', () => {
    // Rubric took Vellum's place, and these are its shortlist tokens.
    expect(themeThumb(findTheme('rubric'))).toEqual({
      bg: '#f0e5cf',
      // The panel sits on the paper under the one ground rule, and the
      // line steps 11 in OKLab L off it.
      panel: '#f0e5cf',
      sep: '#cbc1af',
      accent: '#3656b1',
      text: '#151d2a',
      appearance: 'light',
      ring: 'rgba(0,0,0,0.14)',
    });
  });

  it('derives a thumbnail for a custom theme from its palette alone', () => {
    const custom = customToAppTheme({
      id: 'paper',
      label: 'Paper',
      description: '',
      xterm: { background: '#ffffff', foreground: '#222222', cursor: '#d7005f' },
      chrome: {},
    });
    const thumb = themeThumb(custom);
    expect(thumb).toMatchObject({ bg: '#ffffff', accent: '#d7005f', appearance: 'light' });
    // The panel sits on the ground under the one ground rule, so the
    // strip shows by its line.
    expect(thumb.panel).toBe(thumb.bg);
    expect(thumb.sep).not.toBe(thumb.bg);
  });
});

describe('galleryThemes', () => {
  it('leads with the signature pair and the board order, then the rest by label, then the high contrast pair, then custom themes', () => {
    const custom = customToAppTheme({
      id: 'aardvark',
      label: 'Aardvark',
      description: '',
      xterm: {},
      chrome: {},
    });
    const ids = galleryThemes(BUILTIN_THEMES, [custom]).map((t) => t.id);
    expect(ids.slice(0, 7)).toEqual([
      'triad',
      'rubric',
      'nord',
      'obsidian-ember',
      'gruvbox',
      'rose-pine',
      'tokyo-night',
    ]);
    expect(ids).toHaveLength(BUILTIN_THEMES.length + 1);
    expect(ids.slice(-3)).toEqual(['high-contrast', 'high-contrast-light', 'aardvark']);
    const rest = galleryThemes(BUILTIN_THEMES, [])
      .slice(7, -2)
      .map((t) => t.label);
    expect(rest).toEqual([...rest].sort((a, b) => a.localeCompare(b)));
    expect(rest[0]).toBe('Catppuccin');
    const everforest = rest.indexOf('Everforest Dark');
    expect(rest.slice(everforest, everforest + 2)).toEqual(['Everforest Dark', 'Green Screen']);
  });
});

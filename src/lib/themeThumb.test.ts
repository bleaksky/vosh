import { describe, expect, it } from 'vitest';
import { galleryThemes, themeThumb } from './themeThumb';
import { BUILTIN_THEMES, customToAppTheme, findTheme } from './themes';

describe('themeThumb', () => {
  // The approved Appearance board paints these exact colors.
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
      panel: '#0c0a08',
      sep: '#1d1b19',
      accent: '#ef8f2f',
      text: '#c0bdbb',
      appearance: 'dark',
      ring: 'rgba(255,255,255,0.10)',
    });
  });

  it('matches the board for Vellum', () => {
    expect(themeThumb(findTheme('vellum'))).toEqual({
      bg: '#f7f4ee',
      panel: '#f0ede7',
      sep: '#dad8d2',
      accent: '#3f6690',
      text: '#2a2622',
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
    expect(thumb.panel).not.toBe(thumb.bg);
  });
});

describe('galleryThemes', () => {
  it('leads with the board order, then the rest by label, then custom themes', () => {
    const custom = customToAppTheme({
      id: 'aardvark',
      label: 'Aardvark',
      description: '',
      xterm: {},
      chrome: {},
    });
    const ids = galleryThemes(BUILTIN_THEMES, [custom]).map((t) => t.id);
    expect(ids.slice(0, 6)).toEqual([
      'nord',
      'obsidian-ember',
      'vellum',
      'gruvbox',
      'rose-pine',
      'tokyo-night',
    ]);
    expect(ids).toHaveLength(BUILTIN_THEMES.length + 1);
    expect(ids[ids.length - 1]).toBe('aardvark');
    const rest = galleryThemes(BUILTIN_THEMES, [])
      .slice(6)
      .map((t) => t.label);
    expect(rest).toEqual([...rest].sort((a, b) => a.localeCompare(b)));
    expect(rest[0]).toBe('Catppuccin');
    const everforest = rest.indexOf('Everforest Dark');
    expect(rest.slice(everforest, everforest + 2)).toEqual(['Everforest Dark', 'Everforest Light']);
  });
});

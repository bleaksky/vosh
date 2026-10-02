import { describe, expect, it } from 'vitest';
import {
  ANSI_SLOT_LABELS,
  basePalette,
  BUNDLED_FONTS,
  colorInputValue,
  copyTheme,
  editCustomTheme,
  fontChoices,
  fontLabel,
  pairChoices,
  primaryFontFamily,
  removeCustomTheme,
  sizeChoices,
  stepGalleryTheme,
  THEME_SLOT_GROUPS,
  withBaseColor,
} from './appearanceSettings';
import { ANSI_SLOTS, CANONICAL_ANSI_16 } from './baseAnsi';
import { CHROME_COLOR_KEYS } from './chrome';
import type { CustomTheme } from './session';
import { pickTheme, resolveActiveTheme } from './theme';
import { galleryThemes } from './themeThumb';
import { BUILTIN_THEMES, customToAppTheme, findTheme } from './themes';

const installed = [
  { family: 'Menlo', monospace: true },
  { family: 'Fira Code', monospace: true },
  { family: 'Helvetica', monospace: false },
  { family: 'JetBrains Mono', monospace: true },
  { family: 'Courier New', monospace: true },
  { family: ' ', monospace: true },
];

describe('primaryFontFamily', () => {
  it('reads the first family without quotes', () => {
    expect(primaryFontFamily('"BerkeleyMono Bundled", Menlo, monospace')).toBe(
      'BerkeleyMono Bundled',
    );
    expect(primaryFontFamily("  'SF Mono' , monospace")).toBe('SF Mono');
    expect(primaryFontFamily('Menlo')).toBe('Menlo');
    expect(primaryFontFamily('')).toBe('');
  });
});

describe('fontLabel', () => {
  it('names the bundled fonts the way the board does', () => {
    expect(fontLabel('"JetBrainsMono Bundled", Menlo, monospace')).toBe('JetBrains Mono');
    expect(fontLabel('"BerkeleyMono Bundled", "JetBrainsMono Bundled", monospace')).toBe(
      'Berkeley Mono',
    );
  });

  it('names any other list by its first family', () => {
    expect(fontLabel('"Iosevka Term", monospace')).toBe('Iosevka Term');
    expect(fontLabel('Menlo, monospace')).toBe('Menlo');
  });
});

describe('fontChoices', () => {
  it('lists the bundled fonts, then installed monospace fonts by name', () => {
    const choices = fontChoices(BUNDLED_FONTS[0].value, installed);
    expect(choices.map((c) => c.label)).toEqual([
      'JetBrains Mono',
      'Courier New',
      'Fira Code',
      'Menlo',
    ]);
    expect(choices.find((c) => c.label === 'Fira Code')?.value).toBe(
      '"Fira Code", Menlo, monospace',
    );
  });

  it('gives the current font its exact list so showing it saves nothing', () => {
    const current = 'Menlo, monospace';
    const choices = fontChoices(current, installed);
    expect(choices.filter((c) => c.value === current)).toEqual([
      { label: 'Menlo', value: current },
    ]);
    const stack = '"JetBrainsMono Bundled", "Fira Code", monospace';
    expect(fontChoices(stack, installed).find((c) => c.label === 'JetBrains Mono')?.value).toBe(
      stack,
    );
  });

  it('keeps a current font the list does not have, first', () => {
    const current = '"Helvetica", Menlo, monospace';
    const choices = fontChoices(current, installed);
    expect(choices[0]).toEqual({ label: 'Helvetica', value: current });
    expect(choices).toHaveLength(5);
  });

  it('keeps a Berkeley Mono list saved while Vosh bundled it, by name', () => {
    const saved = '"BerkeleyMono Bundled", Menlo, monospace';
    expect(fontChoices(saved, installed)[0]).toEqual({ label: 'Berkeley Mono', value: saved });
    // Where you have it installed, its entry carries your list.
    const withBerkeley = [...installed, { family: 'Berkeley Mono', monospace: true }];
    const choices = fontChoices(saved, withBerkeley);
    expect(choices.filter((c) => c.label === 'Berkeley Mono')).toEqual([
      { label: 'Berkeley Mono', value: saved },
    ]);
  });

  it('works before the installed list loads', () => {
    expect(fontChoices('', []).map((c) => c.label)).toEqual(['JetBrains Mono']);
  });
});

describe('sizeChoices', () => {
  it('offers the board sizes in points', () => {
    expect(sizeChoices(13).map((c) => c.label)).toEqual([
      '11 pt',
      '12 pt',
      '13 pt',
      '14 pt',
      '15 pt',
      '16 pt',
      '18 pt',
    ]);
  });

  it('keeps a current size outside the list, in order', () => {
    expect(sizeChoices(24).map((c) => c.value)).toEqual([
      '11',
      '12',
      '13',
      '14',
      '15',
      '16',
      '18',
      '24',
    ]);
    expect(sizeChoices(17).map((c) => c.value)).toContain('17');
    expect(sizeChoices(9)[0]).toEqual({ value: '9', label: '9 pt' });
  });
});

describe('pairChoices', () => {
  const themes = galleryThemes(BUILTIN_THEMES, []);

  it('lists the dark themes in gallery order', () => {
    const dark = pairChoices(themes, 'dark', 'nord').map((c) => c.label);
    expect(dark.slice(0, 5)).toEqual([
      'Nord',
      'Obsidian Ember',
      'Gruvbox',
      'Rosé Pine',
      'Tokyo Night',
    ]);
    expect(dark).toContain('Everforest Dark');
    expect(dark).toContain('Green Screen');
    expect(dark).not.toContain('Vellum');
    expect(dark).toContain('Solarized Dark');
    expect(dark).not.toContain('Everforest Light');
    expect(dark).not.toContain('Solarized Light');
  });

  it('lists the light themes', () => {
    expect(pairChoices(themes, 'light', 'vellum').map((c) => c.value)).toEqual([
      'vellum',
      'everforest-light',
      'solarized-light',
    ]);
  });

  it('keeps a pick of the other appearance, first', () => {
    const light = pairChoices(themes, 'light', 'nord');
    expect(light[0]).toEqual({ value: 'nord', label: 'Nord' });
    expect(light.map((c) => c.value)).toContain('vellum');
  });

  it('keeps an id it cannot find', () => {
    expect(pairChoices(themes, 'dark', 'gone')[0]).toEqual({ value: 'gone', label: 'gone' });
  });

  it('lists a custom theme with a blank name under its id', () => {
    const withBlank = galleryThemes(BUILTIN_THEMES, [customToAppTheme(custom('nord-copy', ''))]);
    const dark = pairChoices(withBlank, 'dark', 'nord');
    expect(dark).toContainEqual({ value: 'nord-copy', label: 'nord-copy' });
    expect(dark.every((c) => c.label.trim() !== '')).toBe(true);
  });
});

describe('stepGalleryTheme', () => {
  const themes = galleryThemes(BUILTIN_THEMES, []);
  const last = themes[themes.length - 1].id;

  it('steps through every theme while follow is off', () => {
    expect(stepGalleryTheme(themes, 'obsidian-ember', 1)).toBe('vellum');
    expect(stepGalleryTheme(themes, 'vellum', -1)).toBe('obsidian-ember');
  });

  it('wraps at both ends', () => {
    expect(stepGalleryTheme(themes, last, 1)).toBe('nord');
    expect(stepGalleryTheme(themes, 'nord', -1)).toBe(last);
    expect(stepGalleryTheme(themes, last, 1, 'dark')).toBe('nord');
  });

  it('passes over the themes of the other appearance while follow is on', () => {
    expect(stepGalleryTheme(themes, 'obsidian-ember', 1, 'dark')).toBe('gruvbox');
    expect(stepGalleryTheme(themes, 'gruvbox', -1, 'dark')).toBe('obsidian-ember');
  });

  it('steps from a theme of the other appearance you clicked', () => {
    expect(stepGalleryTheme(themes, 'vellum', 1, 'dark')).toBe('gruvbox');
    expect(stepGalleryTheme(themes, 'vellum', -1, 'dark')).toBe('obsidian-ember');
  });

  it('steps between the light themes while follow is on', () => {
    expect(stepGalleryTheme(themes, 'vellum', 1, 'light')).toBe('everforest-light');
    expect(stepGalleryTheme(themes, 'everforest-light', 1, 'light')).toBe('solarized-light');
    expect(stepGalleryTheme(themes, 'solarized-light', 1, 'light')).toBe('vellum');
    expect(stepGalleryTheme(themes, 'vellum', -1, 'light')).toBe('solarized-light');
    expect(stepGalleryTheme(themes, 'everforest-light', -1, 'light')).toBe('vellum');
  });

  it('stays put when no other theme has that appearance', () => {
    const oneLight = ['nord', 'vellum', 'gruvbox'].map((id) => findTheme(id));
    expect(stepGalleryTheme(oneLight, 'vellum', 1, 'light')).toBe('vellum');
  });

  it('shows every step and leaves the light theme alone on a dark system', () => {
    let ui = {
      theme: 'nord',
      follow_system_appearance: true,
      light_theme: 'vellum',
      dark_theme: 'nord',
    };
    let id = 'nord';
    for (let i = 0; i < themes.length; i += 1) {
      id = stepGalleryTheme(themes, id, 1, 'dark');
      ui = pickTheme(ui, id);
      // The radio the arrow lands on is the one the gallery checks.
      expect(resolveActiveTheme(ui, true)).toBe(id);
    }
    expect(ui.light_theme).toBe('vellum');
  });
});

const custom = (id: string, label = id): CustomTheme => ({
  id,
  label,
  description: '',
  xterm: {},
  chrome: {},
});

describe('copyTheme', () => {
  it('copies the colors under a free id', () => {
    const nord = findTheme('nord');
    const copy = copyTheme(nord, ['nord', 'nord-copy']);
    expect(copy.id).toBe('nord-copy-2');
    expect(copy.label).toBe('Nord copy');
    expect(copy.description).toBe('A copy of Nord.');
    expect(copy.xterm.background).toBe(nord.xterm.background);
    expect(copy.chrome).toEqual(nord.chrome);
    expect(customToAppTheme(copy).xterm).toEqual(nord.xterm);
  });

  it('does not share the base objects', () => {
    const nord = findTheme('nord');
    const copy = copyTheme(nord, []);
    copy.xterm.background = '#000000';
    expect(nord.xterm.background).not.toBe('#000000');
  });
});

describe('editCustomTheme', () => {
  it('patches only the named theme', () => {
    const list = [custom('a'), custom('b')];
    const next = editCustomTheme(list, 'b', { label: 'Bee' });
    expect(next.map((t) => t.label)).toEqual(['a', 'Bee']);
    expect(list[1].label).toBe('b');
  });
});

describe('removeCustomTheme', () => {
  const ui = {
    theme: 'mine',
    follow_system_appearance: true,
    light_theme: 'mine',
    dark_theme: 'mine',
    custom_themes: [custom('mine'), custom('other')],
  };

  it('drops the theme and resets every pick that named it', () => {
    expect(removeCustomTheme(ui, 'mine')).toEqual({
      theme: 'obsidian-ember',
      follow_system_appearance: true,
      light_theme: 'vellum',
      dark_theme: 'obsidian-ember',
      custom_themes: [custom('other')],
    });
  });

  it('leaves other picks alone', () => {
    const next = removeCustomTheme({ ...ui, theme: 'nord', dark_theme: 'nord' }, 'mine');
    expect(next.theme).toBe('nord');
    expect(next.dark_theme).toBe('nord');
    expect(next.light_theme).toBe('vellum');
  });
});

describe('colorInputValue', () => {
  it('reads hex and rgb text', () => {
    expect(colorInputValue('#88C0D0')).toBe('#88c0d0');
    expect(colorInputValue('#abc')).toBe('#aabbcc');
    expect(colorInputValue('#88c0d080')).toBe('#88c0d0');
    expect(colorInputValue('rgba(136, 192, 208, 0.22)')).toBe('#88c0d0');
    expect(colorInputValue('rgb(300,0,-1)')).toBe('#ff0000');
  });

  it('falls back for anything else', () => {
    expect(colorInputValue('')).toBe('#888888');
    expect(colorInputValue('teal', '#000000')).toBe('#000000');
  });
});

describe('basePalette', () => {
  it('starts from the stock chart', () => {
    const stock = basePalette(null);
    expect(stock).toHaveLength(16);
    expect(stock[1]).toBe(CANONICAL_ANSI_16.red);
  });

  it('saves all 16 on the first change', () => {
    const next = withBaseColor(null, 1, '#ff5555');
    expect(next).toHaveLength(16);
    expect(next[1]).toBe('#ff5555');
    expect(next[0]).toBe(CANONICAL_ANSI_16.black);
  });

  it('changes one slot of a saved palette', () => {
    const saved = basePalette(null).map(() => '#111111');
    const next = withBaseColor(saved, 15, '#eeeeee');
    expect(next[15]).toBe('#eeeeee');
    expect(next[14]).toBe('#111111');
    expect(saved[15]).toBe('#111111');
  });
});

describe('THEME_SLOT_GROUPS', () => {
  it('covers every palette slot once and only chrome tokens a theme can pin', () => {
    const xterm = THEME_SLOT_GROUPS.filter((g) => g.source === 'xterm').flatMap((g) =>
      g.slots.map((s) => s.key),
    );
    expect(new Set(xterm).size).toBe(xterm.length);
    expect(Object.keys(findTheme('nord').xterm).sort()).toEqual([...xterm].sort());
    const chrome = THEME_SLOT_GROUPS.filter((g) => g.source === 'chrome').flatMap((g) =>
      g.slots.map((s) => s.key),
    );
    for (const key of chrome) expect(CHROME_COLOR_KEYS).toContain(key);
  });

  it('names the ANSI slots in sentence case', () => {
    expect(ANSI_SLOTS.map((s) => ANSI_SLOT_LABELS[s]).slice(7, 9)).toEqual([
      'White',
      'Bright black',
    ]);
  });
});

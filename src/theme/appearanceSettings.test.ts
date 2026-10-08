import { describe, expect, it } from 'vitest';
import {
  ANSI_SLOT_LABELS,
  basePalette,
  BUNDLED_FONTS,
  colorVisionNote,
  copyTheme,
  editCustomTheme,
  fontChoices,
  fontLabel,
  keepFit,
  pairChoices,
  panelFontChoices,
  panelSizeChoices,
  primaryFontFamily,
  removeCustomTheme,
  sizeChoices,
  stepGalleryTheme,
  THEME_SLOT_GROUPS,
  themeCaption,
  withBaseColor,
} from './appearanceSettings';
import { ANSI_SLOTS, CANONICAL_ANSI_16 } from './baseAnsi';
import { CHROME_COLOR_KEYS } from './chrome';
import type { CustomTheme } from '../ipc/theme';
import { COLOR_VISIONS } from './gameFit';
import { pickTheme, resolveActiveTheme, type ThemePrefs } from './theme';
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

describe('panelFontChoices', () => {
  it('offers As designed, the terminal font and the system font, then the Font list', () => {
    const choices = panelFontChoices('', installed);
    expect(choices.slice(0, 3)).toEqual([
      { value: '', label: 'As designed' },
      { value: 'terminal', label: 'Same as terminal' },
      { value: 'system', label: 'System font' },
    ]);
    expect(choices.slice(3)).toEqual(fontChoices('', installed));
    expect(panelFontChoices('terminal', installed)).toEqual(choices);
    expect(panelFontChoices('system', installed)).toEqual(choices);
  });

  it('keeps a font you picked with its exact list, first when the list lacks it', () => {
    const picked = 'Menlo, monospace';
    expect(panelFontChoices(picked, installed).filter((c) => c.value === picked)).toEqual([
      { label: 'Menlo', value: picked },
    ]);
    const gone = '"PT Mono", Menlo, monospace';
    expect(panelFontChoices(gone, installed)[3]).toEqual({ label: 'PT Mono', value: gone });
  });
});

describe('panelSizeChoices', () => {
  it('offers the terminal size, then the sizes Size offers', () => {
    expect(panelSizeChoices(12)).toEqual([
      { value: '0', label: 'Same as terminal' },
      ...sizeChoices(12),
    ]);
    expect(panelSizeChoices(0)).toEqual(panelSizeChoices(12));
    expect(panelSizeChoices(12).map((c) => c.value)).toEqual([
      '0',
      '11',
      '12',
      '13',
      '14',
      '15',
      '16',
      '18',
    ]);
  });

  it('lists a size of your own among them', () => {
    expect(panelSizeChoices(20).at(-1)).toEqual({ value: '20', label: '20 pt' });
    expect(panelSizeChoices(9)[1]).toEqual({ value: '9', label: '9 pt' });
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
    expect(dark.slice(0, 6)).toEqual([
      'Triad',
      'Nord',
      'Obsidian Ember',
      'Gruvbox',
      'Rosé Pine',
      'Tokyo Night',
    ]);
    expect(dark).toContain('Everforest Dark');
    expect(dark).toContain('Green Screen');
    expect(dark).not.toContain('Rubric');
    expect(dark).toContain('Solarized Dark');
    expect(dark).not.toContain('Melange Light');
    expect(dark).not.toContain('Solarized Light');
  });

  it('lists the light themes', () => {
    expect(pairChoices(themes, 'light', 'rubric').map((c) => c.value)).toEqual([
      'rubric',
      'melange-light',
      'solarized-light',
    ]);
  });

  it('lists every theme, light or dark, in gallery order for Day and Night', () => {
    const every = pairChoices(themes, null, 'gruvbox').map((c) => c.value);
    expect(every).toEqual(themes.map((t) => t.id));
    expect(every).toContain('rubric');
    expect(every).toContain('obsidian-ember');
  });

  it('keeps a pick of the other appearance, first', () => {
    const light = pairChoices(themes, 'light', 'nord');
    expect(light[0]).toEqual({ value: 'nord', label: 'Nord' });
    expect(light.map((c) => c.value)).toContain('rubric');
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
    expect(stepGalleryTheme(themes, 'rubric', 1)).toBe('nord');
    expect(stepGalleryTheme(themes, 'nord', -1)).toBe('rubric');
  });

  it('wraps at both ends', () => {
    expect(stepGalleryTheme(themes, last, 1)).toBe('triad');
    expect(stepGalleryTheme(themes, 'triad', -1)).toBe(last);
    expect(stepGalleryTheme(themes, last, 1, 'dark')).toBe('triad');
  });

  it('passes over the themes of the other appearance while follow is on', () => {
    expect(stepGalleryTheme(themes, 'obsidian-ember', 1, 'dark')).toBe('gruvbox');
    expect(stepGalleryTheme(themes, 'gruvbox', -1, 'dark')).toBe('obsidian-ember');
  });

  it('steps from a theme of the other appearance you clicked', () => {
    expect(stepGalleryTheme(themes, 'rubric', 1, 'dark')).toBe('nord');
    expect(stepGalleryTheme(themes, 'rubric', -1, 'dark')).toBe('triad');
  });

  it('steps between the light themes while follow is on', () => {
    expect(stepGalleryTheme(themes, 'rubric', 1, 'light')).toBe('melange-light');
    expect(stepGalleryTheme(themes, 'melange-light', 1, 'light')).toBe('solarized-light');
    expect(stepGalleryTheme(themes, 'solarized-light', 1, 'light')).toBe('rubric');
    expect(stepGalleryTheme(themes, 'rubric', -1, 'light')).toBe('solarized-light');
    expect(stepGalleryTheme(themes, 'melange-light', -1, 'light')).toBe('rubric');
  });

  it('stays put when no other theme has that appearance', () => {
    const oneLight = ['nord', 'rubric', 'gruvbox'].map((id) => findTheme(id));
    expect(stepGalleryTheme(oneLight, 'rubric', 1, 'light')).toBe('rubric');
  });

  it('shows every step and leaves the light theme alone on a dark system', () => {
    let ui: ThemePrefs = {
      theme: 'nord',
      follow_system_appearance: true,
      light_theme: 'rubric',
      dark_theme: 'nord',
      theme_follow: 'system',
      day_theme: '',
      night_theme: '',
    };
    let id = 'nord';
    for (let i = 0; i < themes.length; i += 1) {
      id = stepGalleryTheme(themes, id, 1, 'dark');
      ui = pickTheme(ui, id);
      // The radio the arrow lands on is the one the gallery checks.
      expect(resolveActiveTheme(ui, true, null)).toBe(id);
    }
    expect(ui.light_theme).toBe('rubric');
  });
});

const custom = (id: string, label = id): CustomTheme => ({
  id,
  label,
  description: '',
  xterm: {},
  chrome: {},
});

describe('themeCaption', () => {
  it('follows the description with the source, the author and the license', () => {
    expect(themeCaption(findTheme('kanso-zen'))).toBe(
      'Calm Japanese dark. Cool blue accent, with sage, gold and red for status. ' +
        'Its colors come from kanso.nvim by Webhooked, under the MIT license.',
    );
    expect(themeCaption(findTheme('solarized-light'))).toBe(
      'Warm cream ground, slate text, blue accent. Bright colors keep their hue. ' +
        'Its colors come from Solarized by Ethan Schoonover, under the MIT license.',
    );
  });

  it('says James Wright made a theme of its own for Vosh', () => {
    expect(themeCaption(findTheme('obsidian-ember'))).toBe(
      'Warm near black ground, pastel colors and a single ember accent. ' +
        'James Wright made it for Vosh, under the GPL version 3.',
    );
  });

  it('names a source once when its author has the same name', () => {
    expect(themeCaption(findTheme('catppuccin'))).toMatch(
      / Its colors come from Catppuccin, under the MIT license\.$/,
    );
    expect(themeCaption(findTheme('tango-dark'))).toMatch(
      / Its colors come from the Tango Desktop Project, in the public domain\.$/,
    );
  });

  it('says when the author publishes no license', () => {
    expect(themeCaption(findTheme('monokai'))).toMatch(
      / Its colors come from Monokai by Wimer Hazenberg, with no license published\.$/,
    );
  });

  it('says when the license takes any later version', () => {
    expect(themeCaption(findTheme('modus-vivendi'))).toMatch(
      / Its colors come from the Modus themes by Protesilaos Stavrou, under the GPL version 3 or later\.$/,
    );
  });

  it('shows a custom theme by its description alone, and nothing for a blank one', () => {
    const mine = customToAppTheme({ ...custom('dusk', 'Dusk'), description: ' Low light. ' });
    expect(themeCaption(mine)).toBe('Low light.');
    expect(themeCaption(customToAppTheme(custom('blank')))).toBe('');
  });

  it('keeps every built in caption free of colons, semicolons and dashes', () => {
    for (const theme of BUILTIN_THEMES) {
      const caption = themeCaption(theme);
      expect(caption, theme.id).not.toMatch(/[:;\u2010-\u2015-]/);
      expect(caption, theme.id).toMatch(/\.$/);
    }
  });
});

describe('colorVisionNote', () => {
  it('says nothing for Typical', () => {
    expect(colorVisionNote('typical')).toBe('');
    expect(colorVisionNote('typical', false)).toBe('');
  });

  // Every theme swaps the same families, so the line names no theme and
  // says what turns into what, as far as the theme leaves room.
  it('says what each vision swaps, in the game text and the window', () => {
    const redGreen =
      'In the game text greens turn blue, reds lean toward orange and blues toward violet, as far as your theme leaves room. In the window success turns blue and danger leans toward orange.';
    expect(colorVisionNote('deuteranopia')).toBe(redGreen);
    expect(colorVisionNote('protanopia')).toBe(redGreen);
    expect(colorVisionNote('tritanopia')).toBe(
      'In the game text blues turn purple and magentas turn pink. The window keeps danger, warn and success where you tell them apart, and makes them lighter or darker where they sit near. An accent Vosh picks moves clear of them.',
    );
  });

  // With the theme's colors off the window still swaps, and under
  // tritanopia it can still move a status color or the accent, so the
  // line never says nothing changes.
  it('says the game text keeps your base palette while the theme colors are off', () => {
    expect(colorVisionNote('deuteranopia', false)).toBe(
      "Game text keeps your base palette while the theme's colors are off for MUD text. In the window success turns blue and danger leans toward orange.",
    );
    expect(colorVisionNote('protanopia', false)).toBe(colorVisionNote('deuteranopia', false));
    expect(colorVisionNote('tritanopia', false)).toBe(
      "Game text keeps your base palette while the theme's colors are off for MUD text. The window keeps danger, warn and success where you tell them apart, and makes them lighter or darker where they sit near. An accent Vosh picks moves clear of them.",
    );
    for (const vision of COLOR_VISIONS) {
      expect(colorVisionNote(vision, false)).not.toContain('nothing changes');
    }
  });

  it('leaves out the old notes on what a theme keeps or cannot part', () => {
    for (const vision of COLOR_VISIONS) {
      for (const on of [true, false]) {
        const note = colorVisionNote(vision, on);
        for (const old of ['already keeps', 'cannot part', 'Fit game colors', 'published']) {
          expect(note, `${vision} ${on}`).not.toContain(old);
        }
        for (const theme of BUILTIN_THEMES) expect(note).not.toContain(theme.label);
        expect(note).not.toMatch(/[;:\u2010-\u2015*]/);
      }
    }
  });
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

  it('keeps the fit of the theme it copies, which has the same colors', () => {
    const kanso = findTheme('kanso-zen');
    expect(copyTheme(kanso, []).fitted).toEqual(kanso.fitted);
    // Solarized Dark keeps out of the fit, so it has none to keep.
    expect(copyTheme(findTheme('solarized-dark'), [])).not.toHaveProperty('fitted');
  });
});

describe('editCustomTheme', () => {
  it('patches only the named theme', () => {
    const list = [custom('a'), custom('b')];
    const next = editCustomTheme(list, 'b', { label: 'Bee' });
    expect(next.map((t) => t.label)).toEqual(['a', 'Bee']);
    expect(list[1].label).toBe('b');
  });

  it('drops the fit when a color the fit reads changes, and only then', () => {
    const fitted = { red: '#cb7b74' };
    const list = [{ ...custom('dusk'), xterm: { background: '#1a1b26' }, fitted }];
    const red = editCustomTheme(list, 'dusk', { xterm: { background: '#1a1b26', red: '#ff0000' } });
    expect(red[0]).not.toHaveProperty('fitted');
    const ground = editCustomTheme(list, 'dusk', { xterm: { background: '#000000' } });
    expect(ground[0]).not.toHaveProperty('fitted');
    // The cursor, a name and a pinned chrome color leave the fit alone.
    const cursor = editCustomTheme(list, 'dusk', {
      xterm: { background: '#1a1b26', cursor: '#ff0000' },
    });
    expect(cursor[0].fitted).toBe(fitted);
    expect(editCustomTheme(list, 'dusk', { label: 'Dusk' })[0].fitted).toBe(fitted);
    expect(editCustomTheme(list, 'dusk', { chrome: { accent: '#ff0000' } })[0].fitted).toBe(fitted);
  });
});

describe('keepFit', () => {
  const dusk = { ...custom('dusk'), xterm: { background: '#1a1b26' } };
  const palette = customToAppTheme(dusk).xterm;

  it('keeps the fit with the theme it was fitted for', () => {
    const list = keepFit([custom('a'), dusk], 'dusk', palette, { red: '#cb7b74' });
    expect(list?.map((t) => t.fitted)).toEqual([undefined, { red: '#cb7b74' }]);
  });

  it('drops a fit for a theme that is gone or has new colors since', () => {
    expect(keepFit([custom('a')], 'dusk', palette, { red: '#cb7b74' })).toBeNull();
    const changed = { ...dusk, xterm: { background: '#000000' } };
    expect(keepFit([changed], 'dusk', palette, { red: '#cb7b74' })).toBeNull();
  });
});

describe('removeCustomTheme', () => {
  const ui = {
    theme: 'mine',
    follow_system_appearance: true,
    light_theme: 'mine',
    dark_theme: 'mine',
    theme_follow: 'system' as const,
    day_theme: 'mine',
    night_theme: 'nord',
    custom_themes: [custom('mine'), custom('other')],
  };

  it('drops the theme and resets every pick that named it', () => {
    // The light pick falls back to the light default, which shows Rubric,
    // and the day pick to none, which shows the manual pick.
    expect(removeCustomTheme(ui, 'mine')).toEqual({
      theme: 'obsidian-ember',
      follow_system_appearance: true,
      light_theme: 'vellum',
      dark_theme: 'obsidian-ember',
      theme_follow: 'system',
      day_theme: '',
      night_theme: 'nord',
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

import { describe, expect, it } from 'vitest';
import ghosttyFile from '../../fixtures/themes/catppuccin-mocha?raw';
import itermFile from '../../fixtures/themes/Dracula.itermcolors?raw';
import kittyFile from '../../fixtures/themes/tokyonight_night.conf?raw';
import alacrittyTomlFile from '../../fixtures/themes/gruvbox_dark.toml?raw';
import alacrittyYamlFile from '../../fixtures/themes/solarized_light.yml?raw';
import {
  detectThemeFormat,
  MISSING_COLORS_MESSAGE,
  normalizeColor,
  parseThemeFile,
  themeLabelFromFileName,
  ThemeFileError,
  UNKNOWN_FORMAT_MESSAGE,
  uniqueThemeId,
} from './themeImport';
import { BUILTIN_THEMES, customToAppTheme, themeTokens } from './themes';

const XTERM_KEYS = [
  'background',
  'foreground',
  'cursor',
  'cursorAccent',
  'selectionBackground',
  'selectionForeground',
  'black',
  'red',
  'green',
  'yellow',
  'blue',
  'magenta',
  'cyan',
  'white',
  'brightBlack',
  'brightRed',
  'brightGreen',
  'brightYellow',
  'brightBlue',
  'brightMagenta',
  'brightCyan',
  'brightWhite',
];

const FIXTURES = [
  { name: 'catppuccin-mocha', text: ghosttyFile, format: 'ghostty' },
  { name: 'Dracula.itermcolors', text: itermFile, format: 'iterm2' },
  { name: 'tokyonight_night.conf', text: kittyFile, format: 'kitty' },
  { name: 'gruvbox_dark.toml', text: alacrittyTomlFile, format: 'alacritty-toml' },
  { name: 'solarized_light.yml', text: alacrittyYamlFile, format: 'alacritty-yaml' },
] as const;

describe('detectThemeFormat', () => {
  it('names each fixture by its contents', () => {
    for (const f of FIXTURES) expect(detectThemeFormat(f.name, f.text)).toBe(f.format);
  });

  it('reads the contents before the extension', () => {
    // A Ghostty theme saved with a .conf name is still Ghostty.
    expect(detectThemeFormat('mocha.conf', ghosttyFile)).toBe('ghostty');
    expect(detectThemeFormat('theme.txt', kittyFile)).toBe('kitty');
  });

  it('falls back to the extension and gives up on anything else', () => {
    expect(detectThemeFormat('empty.itermcolors', '')).toBe('iterm2');
    expect(detectThemeFormat('notes.md', '# Shopping\n\n- eggs\n')).toBeNull();
  });
});

describe('parseThemeFile fixtures', () => {
  it('fills every xterm slot with a hex color and leaves the chrome to derive', () => {
    for (const f of FIXTURES) {
      const theme = parseThemeFile(f.name, f.text);
      expect(Object.keys(theme.xterm).sort()).toEqual([...XTERM_KEYS].sort());
      for (const value of Object.values(theme.xterm)) expect(value).toMatch(/^#[0-9a-f]{6}$/);
      expect(theme.chrome).toEqual({});
    }
  });

  it('reads a Ghostty theme', () => {
    const theme = parseThemeFile('catppuccin-mocha', ghosttyFile);
    expect(theme).toMatchObject({
      id: 'catppuccin-mocha',
      label: 'Catppuccin Mocha',
      description: 'Imported from Ghostty.',
    });
    expect(theme.xterm).toMatchObject({
      background: '#1e1e2e',
      foreground: '#cdd6f4',
      cursor: '#f5e0dc',
      cursorAccent: '#11111b',
      selectionBackground: '#353749',
      selectionForeground: '#cdd6f4',
      black: '#45475a',
      white: '#bac2de',
      brightBlack: '#585b70',
      brightWhite: '#a6adc8',
    });
  });

  it('reads an iTerm2 color preset', () => {
    const theme = parseThemeFile('Dracula.itermcolors', itermFile);
    expect(theme).toMatchObject({
      id: 'dracula',
      label: 'Dracula',
      description: 'Imported from iTerm2.',
    });
    expect(theme.xterm).toMatchObject({
      background: '#282a36',
      foreground: '#f8f8f2',
      cursor: '#f8f8f2',
      cursorAccent: '#282a36',
      selectionBackground: '#44475a',
      selectionForeground: '#f8f8f2',
      black: '#21222c',
      red: '#ff5555',
      brightBlue: '#d6acff',
      brightWhite: '#ffffff',
    });
  });

  it('reads a Kitty theme and takes its own name', () => {
    const theme = parseThemeFile('tokyonight_night.conf', kittyFile);
    expect(theme).toMatchObject({
      id: 'tokyo-night',
      label: 'Tokyo Night',
      description: 'Imported from Kitty.',
    });
    expect(theme.xterm).toMatchObject({
      background: '#1a1b26',
      foreground: '#c0caf5',
      cursorAccent: '#1a1b26',
      selectionBackground: '#283457',
      black: '#15161e',
      brightRed: '#ff899d',
      brightWhite: '#c0caf5',
    });
  });

  it('reads an Alacritty TOML theme and skips cell cursor colors', () => {
    const theme = parseThemeFile('gruvbox_dark.toml', alacrittyTomlFile);
    expect(theme).toMatchObject({
      id: 'gruvbox-dark',
      label: 'Gruvbox Dark',
      description: 'Imported from Alacritty.',
    });
    expect(theme.xterm).toMatchObject({
      background: '#282828',
      foreground: '#ebdbb2',
      // CellForeground and CellBackground are not colors.
      cursor: '#ebdbb2',
      cursorAccent: '#282828',
      selectionBackground: '#928374',
      yellow: '#d79921',
      brightYellow: '#fabd2f',
    });
  });

  it('reads a legacy Alacritty YAML theme', () => {
    const theme = parseThemeFile('solarized_light.yml', alacrittyYamlFile);
    expect(theme).toMatchObject({ id: 'solarized-light', label: 'Solarized Light' });
    expect(theme.xterm).toMatchObject({
      background: '#fdf6e3',
      foreground: '#586e75',
      cursor: '#586e75',
      cursorAccent: '#fdf6e3',
      black: '#073642',
      blue: '#268bd2',
      brightBlack: '#002b36',
      brightWhite: '#fdf6e3',
    });
  });

  it('derives a light or dark window from each background', () => {
    const appearance = (name: string, text: string) =>
      themeTokens(customToAppTheme(parseThemeFile(name, text))).appearance;
    expect(appearance('solarized_light.yml', alacrittyYamlFile)).toBe('light');
    expect(appearance('Dracula.itermcolors', itermFile)).toBe('dark');
    expect(appearance('catppuccin-mocha', ghosttyFile)).toBe('dark');
  });
});

describe('parseThemeFile edge cases', () => {
  it('reads Ghostty colors without a hash, with comments and other settings', () => {
    const palette = Array.from(
      { length: 16 },
      (_, i) => `palette = ${i} = ${'0123456789abcdef'[i].repeat(6)}`,
    );
    const text = [
      '# my config',
      'font-family = "Iosevka Term"',
      'background = 101010',
      'foreground = "#EEEEEE"',
      ...palette,
    ].join('\n');
    const theme = parseThemeFile('config', text);
    expect(theme.xterm.background).toBe('#101010');
    expect(theme.xterm.foreground).toBe('#eeeeee');
    expect(theme.xterm.brightWhite).toBe('#ffffff');
    expect(theme.label).toBe('Config');
  });

  it('takes the dark variant from a newer iTerm2 export', () => {
    const text = itermFile.replace(
      /<key>(Ansi \d+ Color|Background Color|Foreground Color)</g,
      '<key>$1 (Dark)<',
    );
    const theme = parseThemeFile('Dracula.itermcolors', text);
    expect(theme.xterm.background).toBe('#282a36');
    expect(theme.xterm.red).toBe('#ff5555');
  });

  it('refuses a file missing a color', () => {
    const text = kittyFile.replace(/^color7 .*$/m, '');
    expect(() => parseThemeFile('tokyonight_night.conf', text)).toThrow(MISSING_COLORS_MESSAGE);
    expect(() => parseThemeFile('bare.conf', 'foreground #ffffff\n')).toThrow(ThemeFileError);
  });

  it('refuses a file that is not a theme', () => {
    expect(() => parseThemeFile('notes.md', 'just some words\n')).toThrow(UNKNOWN_FORMAT_MESSAGE);
  });

  it('keeps the id clear of the themes you already have', () => {
    const taken = BUILTIN_THEMES.map((t) => t.id);
    // Dracula, Tokyo Night, and Solarized Light ship built in.
    expect(parseThemeFile('Dracula.itermcolors', itermFile, taken).id).toBe('dracula-2');
    expect(parseThemeFile('tokyonight_night.conf', kittyFile, taken).id).toBe('tokyo-night-2');
    expect(parseThemeFile('solarized_light.yml', alacrittyYamlFile, taken).id).toBe(
      'solarized-light-2',
    );
    expect(parseThemeFile('catppuccin-mocha', ghosttyFile, taken).id).toBe('catppuccin-mocha');
    expect(uniqueThemeId('nord', ['nord', 'nord-2'])).toBe('nord-3');
    expect(uniqueThemeId('fresh', taken)).toBe('fresh');
  });
});

describe('normalizeColor', () => {
  it('reads the common spellings', () => {
    expect(normalizeColor('#ABC')).toBe('#aabbcc');
    expect(normalizeColor('0x1d1f21')).toBe('#1d1f21');
    expect(normalizeColor("'#282828'")).toBe('#282828');
    expect(normalizeColor('#28282880')).toBe('#282828');
    expect(normalizeColor('rgb:ff/80/00')).toBe('#ff8000');
    expect(normalizeColor('rgb:ffff/0000/8080')).toBe('#ff0080');
  });

  it('rejects values that are not colors', () => {
    expect(normalizeColor('CellForeground')).toBeNull();
    expect(normalizeColor('none')).toBeNull();
    expect(normalizeColor('#12345')).toBeNull();
  });
});

describe('themeLabelFromFileName', () => {
  it('turns a file name into a label', () => {
    expect(themeLabelFromFileName('/Users/you/themes/rose-pine_moon.toml')).toBe('Rose Pine Moon');
    expect(themeLabelFromFileName('Tomorrow Night.itermcolors')).toBe('Tomorrow Night');
    expect(themeLabelFromFileName('.conf')).toBe('Imported theme');
  });
});

// Theme palettes. A theme is a terminal palette (the 16 ANSI slots plus
// surfaces) and nothing else by default: theme/chrome derives the window
// chrome from it, so adding a scheme is one xterm block. A theme may pin
// individual chrome tokens where the derivation misses a look the theme
// is known for (Nord's frost accent, for example).

import type { CustomTheme } from '../ipc/theme';
import type { RawUiConfig } from '../ipc/uiConfig';
import { CHROME_COLOR_KEYS, deriveChrome, type ChromeOverrides, type ChromeTokens } from './chrome';
import { fitKey, GAME_SLOTS, type ColorVision } from './gameFit';
import { uniqueThemeId } from './themeImport';

export interface XtermPalette {
  background: string;
  foreground: string;
  cursor: string;
  cursorAccent: string;
  selectionBackground: string;
  selectionForeground: string;
  black: string;
  red: string;
  green: string;
  yellow: string;
  blue: string;
  magenta: string;
  cyan: string;
  white: string;
  brightBlack: string;
  brightRed: string;
  brightGreen: string;
  brightYellow: string;
  brightBlue: string;
  brightMagenta: string;
  brightCyan: string;
  brightWhite: string;
}

/** The license a theme's colors carry. public/theme-credits.txt keeps
 *  the notice each one asks to travel with the colors. */
export type ThemeLicense =
  | 'MIT'
  | 'GPL-3.0'
  | 'GPL-3.0-or-later'
  | 'Public domain'
  | 'None published';

export interface AppTheme {
  id: string;
  label: string;
  description: string;
  /// Where the colors come from, named as a sentence names it (Nord, the
  /// CGA palette), who made them, and the license they carry. Every
  /// built in theme names all three, and public/theme-credits.txt keeps
  /// the notices. A custom theme has none.
  source?: string;
  author?: string;
  license?: ThemeLicense;
  xterm: XtermPalette;
  /// The colors Fit game colors draws in play, the slots the game color
  /// fit (theme/gameFit) moves off the published palette, from body text
  /// and the 16 ANSI colors. A built in theme stores them computed
  /// ahead, since a fit takes about two seconds, and gameFit.test.ts
  /// fits each again with VOSH_FIT_THEMES=1. A custom theme keeps the
  /// fit Settings made when you imported or changed it.
  fitted?: Partial<XtermPalette>;
  /// False keeps the published palette in play with Fit game colors on.
  /// A color vision other than Typical still swaps it.
  fitGameColors?: false;
  /// Chrome tokens this theme pins instead of deriving.
  chrome?: ChromeOverrides;
}

// ── Kanso Zen ───────────────────────────────────────────────────────
// Mirrors the user's Ghostty config exactly, so Settings and exports
// match the terminal outside Vosh. In play Fit game colors retunes the
// game colors (Themes review Q15).
const kansoZen: AppTheme = {
  id: 'kanso-zen',
  label: 'Kanso Zen',
  description: 'Calm Japanese dark. Cool blue accent, with sage, gold and red for status.',
  source: 'kanso.nvim',
  author: 'Webhooked',
  license: 'MIT',
  xterm: {
    background: '#090e13',
    foreground: '#c5c9c7',
    cursor: '#c5c9c7',
    cursorAccent: '#090e13',
    selectionBackground: '#22262d',
    selectionForeground: '#c5c9c7',
    black: '#585858',
    red: '#c4746e',
    green: '#8a9a7b',
    yellow: '#c4b28a',
    blue: '#8ba4b0',
    magenta: '#a292a3',
    cyan: '#8ea4a2',
    white: '#a4a7a4',
    brightBlack: '#5c6066',
    brightRed: '#e46876',
    brightGreen: '#87a987',
    brightYellow: '#e6c384',
    brightBlue: '#7fb4ca',
    brightMagenta: '#938aa9',
    brightCyan: '#7aa89f',
    brightWhite: '#c5c9c7',
  },
  // The palette stays as the Ghostty config has it, and in play the fit
  // retunes 16 slots to pass 44 of 46 (Q15).
  fitted: {
    foreground: '#c9cdcb',
    black: '#656565',
    red: '#d17f79',
    green: '#d4e5c4',
    yellow: '#d0be95',
    blue: '#879fab',
    cyan: '#a0b6b4',
    white: '#b7bab7',
    brightBlack: '#92979d',
    brightRed: '#ff919a',
    brightGreen: '#e9ffe9',
    brightYellow: '#ffe8bf',
    brightBlue: '#8cc2d8',
    brightMagenta: '#bab1d1',
    brightCyan: '#acdbd1',
    brightWhite: '#f0f5f2',
  },
  // Kanso's brand cool blue lives outside its terminal palette.
  chrome: { accent: '#b0c8d4' },
};

// ── Obsidian Ember ──────────────────────────────────────────────────
// The Ember redesign palette. A warm near-black ground, pastel ANSI,
// and an ember cursor the theme pins as its single accent. The orange
// sits 8.1 dE from the danger red, under the 12 the chrome rule asks of
// an accent it picks itself, so without the pin the rule would take
// Ember's magenta. The orange is what the name promises.
const obsidianEmber: AppTheme = {
  id: 'obsidian-ember',
  label: 'Obsidian Ember',
  description: 'Warm near black ground, pastel colors and a single ember accent.',
  source: 'Vosh',
  author: 'James Wright',
  license: 'GPL-3.0',
  xterm: {
    background: '#050403',
    foreground: '#c0bdbb',
    cursor: '#ef8f2f',
    cursorAccent: '#050403',
    selectionBackground: '#201d1c',
    selectionForeground: '#f2efee',
    black: '#4a4642',
    red: '#d97a6e',
    green: '#79c795',
    yellow: '#d8b56a',
    blue: '#82a8e0',
    magenta: '#b48ec9',
    cyan: '#7ec8d4',
    white: '#b8b2ac',
    brightBlack: '#5f5a55',
    brightRed: '#ea8f80',
    brightGreen: '#8fdaa8',
    brightYellow: '#ecc985',
    brightBlue: '#9bbdf0',
    brightMagenta: '#cba6dd',
    brightCyan: '#97dde8',
    brightWhite: '#ece7e1',
  },
  fitted: {
    foreground: '#cecbc9',
    red: '#d07166',
    green: '#95e4b0',
    yellow: '#d2af64',
    blue: '#799ed6',
    magenta: '#b08ac5',
    cyan: '#79c3cf',
    brightBlack: '#99948f',
    brightRed: '#f59989',
    brightGreen: '#c7ffd8',
    brightYellow: '#f2cf8a',
    brightBlue: '#9bbef1',
    brightMagenta: '#d0aae2',
    brightCyan: '#9de3ee',
  },
  // The ember accent, and the ember ink the approved canvas sets on
  // accent buttons.
  chrome: { accent: '#ef8f2f', onAccent: '#140b02' },
};

// ── Triad ───────────────────────────────────────────────────────────
// Vosh's signature dark, under the sky it is named for. Each moon sits
// on the slot moon.c paints it in: Lysenties silver in bright white,
// Nercuros cyan in bright cyan, Dyphrities blood red in red, a scarlet
// that stays apart from green for deuteranopes. The ground is a violet
// night and body text a neutral stone, so both moons stand clear of it.
// The cursor is Nercuros cyan, which the theme pins as its accent. It
// passes all 46 game checks as it stands, so it keeps no fit.
const triad: AppTheme = {
  id: 'triad',
  label: 'Triad',
  description:
    'The three moons of Aabahran over a violet night. Each moon keeps the color the game ' +
    'paints it in, and Nercuros cyan is the accent.',
  source: 'Vosh',
  author: 'James Wright',
  license: 'GPL-3.0',
  xterm: {
    background: '#150c22',
    foreground: '#dbdbda',
    cursor: '#44d4e2',
    cursorAccent: '#150c22',
    selectionBackground: '#224458',
    selectionForeground: '#dbdbda',
    black: '#41464d',
    red: '#fe6457',
    green: '#45c6a8',
    yellow: '#eeca71',
    blue: '#78a0d5',
    magenta: '#bb8eba',
    cyan: '#a7c2c4',
    white: '#b5babe',
    brightBlack: '#98a0ab',
    brightRed: '#ff9b8e',
    brightGreen: '#aafddd',
    brightYellow: '#ffeead',
    brightBlue: '#94c2fb',
    brightMagenta: '#dbade1',
    brightCyan: '#84e6ff',
    brightWhite: '#f4f8fb',
  },
  chrome: { accent: '#44d4e2' },
};

// ── Rubric ──────────────────────────────────────────────────────────
// Vosh's signature light, a manuscript page. Body text is iron gall
// ink, blue black, and room names the same ink faded to sepia. Hurt is
// vermilion, alarms kermes, tells verdigris, the cabal lapis and says
// umber. The bright slots are darker than the normal ones, so bold text
// grows heavier on the paper instead of fading. The accent is the lapis
// of the initials, so red in the window always means trouble. It passes
// all 46 game checks as it stands, so it keeps no fit.
const rubric: AppTheme = {
  id: 'rubric',
  label: 'Rubric',
  description:
    'Ink on parchment. Blue black text, red kept for trouble, and the lapis of the ' +
    'initials as the accent.',
  source: 'Vosh',
  author: 'James Wright',
  license: 'GPL-3.0',
  xterm: {
    background: '#f0e5cf',
    foreground: '#151d2a',
    cursor: '#3656b1',
    cursorAccent: '#f0e5cf',
    selectionBackground: '#cbc8c9',
    selectionForeground: '#151d2a',
    black: '#2b2f38',
    red: '#e15400',
    green: '#007873',
    yellow: '#5d4000',
    blue: '#4e73c2',
    magenta: '#b05684',
    cyan: '#2c587b',
    white: '#5e6770',
    brightBlack: '#948170',
    brightRed: '#970004',
    brightGreen: '#004e47',
    brightYellow: '#3b2200',
    brightBlue: '#334eb1',
    brightMagenta: '#8f3075',
    brightCyan: '#003f56',
    brightWhite: '#050911',
  },
  chrome: { accent: '#3656b1' },
};

// ── Tokyo Night ─────────────────────────────────────────────────────
// Saturated blues, muted purples, signature deep navy. The window rule
// takes its magenta as the accent, clear of every status color.
const tokyoNight: AppTheme = {
  id: 'tokyo-night',
  label: 'Tokyo Night',
  description: 'Night variant. Cool blues, deep navy, frosted accents.',
  source: 'Tokyo Night',
  author: 'Enkia',
  license: 'MIT',
  xterm: {
    background: '#1a1b26',
    foreground: '#c0caf5',
    cursor: '#c0caf5',
    cursorAccent: '#1a1b26',
    selectionBackground: '#28344a',
    selectionForeground: '#c0caf5',
    black: '#15161e',
    red: '#f7768e',
    green: '#9ece6a',
    yellow: '#e0af68',
    blue: '#7aa2f7',
    magenta: '#bb9af7',
    cyan: '#7dcfff',
    white: '#a9b1d6',
    brightBlack: '#414868',
    brightRed: '#f7768e',
    brightGreen: '#9ece6a',
    brightYellow: '#e0af68',
    brightBlue: '#7aa2f7',
    brightMagenta: '#bb9af7',
    brightCyan: '#7dcfff',
    brightWhite: '#c0caf5',
  },
  fitted: {
    foreground: '#bdc7f2',
    black: '#2c2e36',
    red: '#e86982',
    green: '#bbed87',
    cyan: '#70c2f2',
    white: '#aab3d8',
    brightBlack: '#8e97bb',
    brightRed: '#ff94a5',
    brightGreen: '#e0ffc4',
    brightYellow: '#ffd08d',
    brightBlue: '#a3c2ff',
    brightMagenta: '#d4bfff',
    brightCyan: '#a8deff',
    brightWhite: '#dbe2ff',
  },
};

// ── Nord ────────────────────────────────────────────────────────────
// Arctic, north-bluish dark. Frost accent (`#88c0d0`).
const nord: AppTheme = {
  id: 'nord',
  label: 'Nord',
  description: 'Arctic palette. Polar nights base, frost accents.',
  source: 'Nord',
  author: 'Sven Greb',
  license: 'MIT',
  xterm: {
    background: '#2e3440',
    foreground: '#d8dee9',
    cursor: '#d8dee9',
    cursorAccent: '#2e3440',
    selectionBackground: '#4c566a',
    selectionForeground: '#eceff4',
    black: '#3b4252',
    red: '#bf616a',
    green: '#a3be8c',
    yellow: '#ebcb8b',
    blue: '#81a1c1',
    magenta: '#b48ead',
    cyan: '#88c0d0',
    white: '#d8dee9',
    brightBlack: '#4c566a',
    brightRed: '#bf616a',
    brightGreen: '#a3be8c',
    brightYellow: '#ebcb8b',
    brightBlue: '#81a1c1',
    brightMagenta: '#b48ead',
    brightCyan: '#8fbcbb',
    brightWhite: '#eceff4',
  },
  fitted: {
    black: '#3c4353',
    red: '#c3656e',
    green: '#96b07f',
    yellow: '#e9c989',
    blue: '#83a4c4',
    magenta: '#bb95b4',
    cyan: '#8cc5d5',
    brightBlack: '#95a1b7',
    brightRed: '#ff9ea5',
    brightGreen: '#ceeab6',
    brightYellow: '#ffeac1',
    brightBlue: '#a3c4e5',
    brightMagenta: '#dcb4d4',
    brightCyan: '#b6e4e3',
    brightWhite: '#feffff',
  },
  // otty's Nord, measured from otty's own theme file. The panel and
  // floating surfaces stay on the terminal ground, and the text tiers
  // follow nord5 rather than the terminal's nord4 foreground. Danger
  // words use the lighter red the approved boards draw them in.
  chrome: {
    panel: '#2e3440',
    raised: '#2e3440',
    sep: '#434c5e',
    selrow: '#3b4252',
    text: '#e5e9f0',
    secondary: '#c0c7d3',
    tertiary: '#7b8294',
    accent: '#88c0d0',
    dangerText: '#dc8a92',
  },
};

// ── Gruvbox Dark ────────────────────────────────────────────────────
// Warm retro tones. Earthy yellows and reds.
const gruvbox: AppTheme = {
  id: 'gruvbox',
  label: 'Gruvbox',
  description: 'Warm, retro, earthy. Yellow accent on warm dark.',
  source: 'gruvbox',
  author: 'Pavel Pertsev',
  license: 'MIT',
  xterm: {
    background: '#282828',
    foreground: '#ebdbb2',
    cursor: '#ebdbb2',
    cursorAccent: '#282828',
    selectionBackground: '#504945',
    selectionForeground: '#ebdbb2',
    black: '#282828',
    red: '#cc241d',
    green: '#98971a',
    yellow: '#d79921',
    blue: '#458588',
    magenta: '#b16286',
    cyan: '#689d6a',
    white: '#a89984',
    brightBlack: '#928374',
    brightRed: '#fb4934',
    brightGreen: '#b8bb26',
    brightYellow: '#fabd2f',
    brightBlue: '#83a598',
    brightMagenta: '#d3869b',
    brightCyan: '#8ec07c',
    brightWhite: '#ebdbb2',
  },
  fitted: {
    foreground: '#e4d4ac',
    black: '#383838',
    red: '#e03c30',
    green: '#cdce5d',
    blue: '#66a6a9',
    magenta: '#d582a7',
    cyan: '#8ec590',
    white: '#c5b59f',
    brightBlack: '#a8998a',
    brightRed: '#ff9583',
    brightGreen: '#ecf068',
    brightBlue: '#9dc0b2',
    brightMagenta: '#f4a4b9',
    brightCyan: '#b2e6a0',
    brightWhite: '#ffefc5',
  },
  chrome: { accent: '#fabd2f' },
};

// ── Catppuccin Mocha ────────────────────────────────────────────────
// Soft pastels, warm dark base. Pink accent.
const catppuccin: AppTheme = {
  id: 'catppuccin',
  label: 'Catppuccin',
  description: 'Mocha variant. Soft pastels on a warm dark base.',
  source: 'Catppuccin',
  author: 'Catppuccin',
  license: 'MIT',
  xterm: {
    background: '#1e1e2e',
    foreground: '#cdd6f4',
    cursor: '#f5e0dc',
    cursorAccent: '#1e1e2e',
    selectionBackground: '#45475a',
    selectionForeground: '#cdd6f4',
    black: '#45475a',
    red: '#f38ba8',
    green: '#a6e3a1',
    yellow: '#f9e2af',
    blue: '#89b4fa',
    magenta: '#f5c2e7',
    cyan: '#94e2d5',
    white: '#bac2de',
    brightBlack: '#585b70',
    brightRed: '#f38ba8',
    brightGreen: '#a6e3a1',
    brightYellow: '#f9e2af',
    brightBlue: '#89b4fa',
    brightMagenta: '#f5c2e7',
    brightCyan: '#94e2d5',
    brightWhite: '#a6adc8',
  },
  fitted: {
    red: '#d5708d',
    green: '#89c484',
    yellow: '#f1dba8',
    magenta: '#d4a2c7',
    brightBlack: '#9498af',
    brightRed: '#fe95b2',
    brightGreen: '#adeba8',
    brightYellow: '#fff1d2',
    brightBlue: '#b5d2ff',
    brightCyan: '#c4fff4',
    brightWhite: '#edf1ff',
  },
  chrome: { accent: '#f5c2e7' },
};

// ── Classic Vivid ───────────────────────────────────────────────────
// Saturated CGA / VGA "high intensity" palette — the look CMUD,
// zMUD, and the original Windows console shipped. Pure primaries
// for the bright variants; standard half-intensity for the base
// eight. Pick this if Kanso Zen feels too muted / pastel.
const classicVivid: AppTheme = {
  id: 'classic-vivid',
  label: 'Classic Vivid',
  description: 'Saturated CGA/VGA primaries. Bright reds, greens, blues.',
  source: 'the CGA palette',
  author: 'IBM',
  license: 'Public domain',
  xterm: {
    background: '#0a0a0a',
    foreground: '#cccccc',
    cursor: '#ffffff',
    cursorAccent: '#0a0a0a',
    selectionBackground: '#444444',
    selectionForeground: '#ffffff',
    black: '#000000',
    red: '#aa0000',
    green: '#00aa00',
    yellow: '#aa5500',
    blue: '#0000aa',
    magenta: '#aa00aa',
    cyan: '#00aaaa',
    white: '#aaaaaa',
    brightBlack: '#555555',
    brightRed: '#ff0000',
    brightGreen: '#00ff00',
    brightYellow: '#ffff00',
    // Pure #0000ff is unreadable on dark; use the high-intensity
    // bluish-purple the legacy VGA "high-intensity blue" rendered as.
    brightBlue: '#5555ff',
    brightMagenta: '#ff00ff',
    brightCyan: '#00ffff',
    brightWhite: '#ffffff',
  },
  // The fit lifts blue from Lc 0 to 39 in play and leaves 6 checks
  // short, red at Lc 40.7 among them (Q17).
  fitted: {
    black: '#232323',
    red: '#ef5746',
    green: '#4cd546',
    yellow: '#ffc6a2',
    blue: '#4b82ff',
    magenta: '#e756e4',
    cyan: '#0badac',
    white: '#b2b2b2',
    brightBlack: '#959595',
    brightRed: '#ff8574',
    brightBlue: '#8a9bff',
    brightMagenta: '#ff84fc',
  },
  // Vivid amber accent, distinct from every ANSI status color and in
  // keeping with a CGA era highlight.
  chrome: { accent: '#ffaa00' },
};

// ── Dracula at Night ────────────────────────────────────────────────
// Dracula's text and its sixteen colors as its terminal ports ship
// them, with the purple #bd93f9 as the accent, on a ground darker than
// Dracula's for late sessions. The ground is #1a1c23 where Dracula has
// #282a36, and black and the selection step down with it, #15161c for
// #21222c and #363948 for #44475a.
const dracula: AppTheme = {
  id: 'dracula',
  label: 'Dracula at Night',
  description: 'Dracula on a darker ground, #1a1c23 where Dracula has #282a36.',
  source: 'Dracula',
  author: 'Zeno Rocha',
  license: 'MIT',
  xterm: {
    background: '#1a1c23',
    foreground: '#f8f8f2',
    cursor: '#f8f8f2',
    cursorAccent: '#1a1c23',
    selectionBackground: '#363948',
    selectionForeground: '#f8f8f2',
    black: '#15161c',
    red: '#ff5555',
    green: '#50fa7b',
    yellow: '#f1fa8c',
    blue: '#bd93f9',
    magenta: '#ff79c6',
    cyan: '#8be9fd',
    white: '#f8f8f2',
    brightBlack: '#6272a4',
    brightRed: '#ff6e6e',
    brightGreen: '#69ff94',
    brightYellow: '#ffffa5',
    brightBlue: '#d6acff',
    brightMagenta: '#ff92df',
    brightCyan: '#a4ffff',
    brightWhite: '#ffffff',
  },
  fitted: {
    foreground: '#e4e4df',
    black: '#2c2e34',
    red: '#f2494b',
    green: '#07d558',
    yellow: '#e2eb7d',
    blue: '#b68cf2',
    magenta: '#f671be',
    cyan: '#81dff3',
    white: '#deded9',
    brightBlack: '#8597cb',
    brightRed: '#ff9692',
    brightYellow: '#feffc9',
    brightBlue: '#d7aeff',
    brightMagenta: '#ff9ee2',
    brightCyan: '#aeffff',
  },
  chrome: { accent: '#bd93f9' },
};

// ── Monokai ─────────────────────────────────────────────────────────
// Classic Sublime Text palette. Warm brown-tinted surfaces with the
// signature magenta accent (#f92672) against vivid green / orange /
// cyan semantics.
const monokai: AppTheme = {
  id: 'monokai',
  label: 'Monokai',
  description: 'Warm dark with the signature magenta accent.',
  source: 'Monokai',
  author: 'Wimer Hazenberg',
  license: 'None published',
  xterm: {
    background: '#272822',
    foreground: '#f8f8f2',
    cursor: '#f8f8f2',
    cursorAccent: '#272822',
    selectionBackground: '#49483e',
    selectionForeground: '#f8f8f2',
    black: '#272822',
    red: '#f92672',
    green: '#a6e22e',
    yellow: '#f4bf75',
    blue: '#66d9ef',
    magenta: '#ae81ff',
    cyan: '#a1efe4',
    white: '#f8f8f2',
    brightBlack: '#75715e',
    brightRed: '#f92672',
    brightGreen: '#a6e22e',
    brightYellow: '#f4bf75',
    brightBlue: '#66d9ef',
    brightMagenta: '#ae81ff',
    brightCyan: '#a1efe4',
    brightWhite: '#f9f8f5',
  },
  // Bright white has no room above body text, so the fit lowers body
  // text to #e4e4df in play (Q19).
  fitted: {
    foreground: '#e4e4df',
    black: '#363831',
    red: '#ff648c',
    green: '#bbf94d',
    yellow: '#edb96f',
    blue: '#52c7dd',
    magenta: '#af84ff',
    cyan: '#89d6cc',
    white: '#deded9',
    brightBlack: '#a09c87',
    brightRed: '#ff99ad',
    brightGreen: '#d8ffa4',
    brightYellow: '#ffdbac',
    brightBlue: '#76e8fe',
    brightMagenta: '#c5aaff',
    brightCyan: '#a9f8ec',
    brightWhite: '#fffffd',
  },
  chrome: { accent: '#f92672' },
};

// ── One Half Dark ───────────────────────────────────────────────────
// One Half Dark as its Sublime Text and iTerm2 ports ship it. Atom's
// One Dark colors with a brighter foreground (#dcdfe4), which lifts body
// text from Lc 56 to Lc 83. It took the place of One Dark (Themes review
// Q13), so a saved One Dark shows it (RETIRED_THEMES).
const oneHalfDark: AppTheme = {
  id: 'one-half-dark',
  label: 'One Half Dark',
  description: 'Cool slate in the style of Atom. Soft pastels, a bright foreground, purple accent.',
  source: 'One Half',
  author: 'Son A. Pham',
  license: 'MIT',
  xterm: {
    background: '#282c34',
    foreground: '#dcdfe4',
    cursor: '#a3b3cc',
    cursorAccent: '#282c34',
    selectionBackground: '#474e5d',
    selectionForeground: '#dcdfe4',
    black: '#282c34',
    red: '#e06c75',
    green: '#98c379',
    yellow: '#e5c07b',
    blue: '#61afef',
    magenta: '#c678dd',
    cyan: '#56b6c2',
    white: '#dcdfe4',
    brightBlack: '#5d677a',
    brightRed: '#e06c75',
    brightGreen: '#98c379',
    brightYellow: '#e5c07b',
    brightBlue: '#61afef',
    brightMagenta: '#c678dd',
    brightCyan: '#56b6c2',
    brightWhite: '#ffffff',
  },
  fitted: {
    black: '#373c44',
    red: '#e9747d',
    green: '#c6f3a6',
    yellow: '#e0bc77',
    blue: '#5faded',
    magenta: '#cd7ee4',
    cyan: '#66c5d1',
    white: '#dbdee3',
    brightBlack: '#939eb2',
    brightRed: '#ff9da1',
    brightGreen: '#e3ffd1',
    brightYellow: '#ffdd9e',
    brightBlue: '#90cbff',
    brightMagenta: '#e9a2ff',
    brightCyan: '#87e6f2',
  },
};

// ── Tango Dark ──────────────────────────────────────────────────────
// GNOME Terminal classic. Saturated primaries (#cc0000 red, #4e9a06
// green, #3465a4 blue) with the bright variants jumped up a stop.
// Warm dark background gives the chrome a slight green tint.
const tangoDark: AppTheme = {
  id: 'tango-dark',
  label: 'Tango Dark',
  description: 'GNOME Terminal classic. Saturated primaries on a warm dark.',
  source: 'the Tango Desktop Project',
  author: 'the Tango Desktop Project',
  license: 'Public domain',
  xterm: {
    background: '#2e3436',
    foreground: '#d3d7cf',
    cursor: '#d3d7cf',
    cursorAccent: '#2e3436',
    selectionBackground: '#555753',
    selectionForeground: '#eeeeec',
    black: '#2e3436',
    red: '#cc0000',
    green: '#4e9a06',
    yellow: '#c4a000',
    blue: '#3465a4',
    magenta: '#75507b',
    cyan: '#06989a',
    white: '#d3d7cf',
    brightBlack: '#555753',
    brightRed: '#ef2929',
    brightGreen: '#8ae234',
    brightYellow: '#fce94f',
    brightBlue: '#729fcf',
    brightMagenta: '#ad7fa8',
    brightCyan: '#34e2e2',
    brightWhite: '#eeeeec',
  },
  // The fit lifts blue and magenta past Lc 45 in play. Red stays at
  // Lc 36.2, with 4 checks short in all (Q18).
  fitted: {
    foreground: '#d4d8d0',
    black: '#3d4345',
    red: '#fe4a3b',
    green: '#a3f476',
    yellow: '#dcb834',
    blue: '#70a3e7',
    magenta: '#bc94c2',
    cyan: '#58ccce',
    white: '#d6dad2',
    brightBlack: '#adafab',
    brightRed: '#ff8f82',
    brightGreen: '#caffa6',
    brightBlue: '#98c7f8',
    brightMagenta: '#e4b4df',
    brightCyan: '#4cf2f1',
    brightWhite: '#fbfbf9',
  },
};

// ── High Contrast ───────────────────────────────────────────────────
// White on an off black ground, rebuilt so every text color reads 7:1
// or better (Board 14). The chrome is pinned, not derived, so the text
// tones, the status words and the accent hold their ratios on every
// ground, and the lines and field edges read 3:1. Black plays as
// #9a9a9a so the game's black text still shows. Fit game colors keeps
// out, since the palette already reads and the fit would lower body
// text.
const highContrast: AppTheme = {
  id: 'high-contrast',
  label: 'High Contrast',
  description: 'White on black, every text color 7:1 or better.',
  source: 'Vosh',
  author: 'James Wright',
  license: 'GPL-3.0',
  xterm: {
    background: '#0a0a0a',
    foreground: '#ffffff',
    cursor: '#5cc8ff',
    cursorAccent: '#000000',
    selectionBackground: '#0b4f8a',
    selectionForeground: '#ffffff',
    black: '#9a9a9a',
    red: '#ff7a7a',
    green: '#5cf25c',
    yellow: '#ffe94d',
    blue: '#7fb2ff',
    magenta: '#ff8aff',
    cyan: '#5cf2f2',
    white: '#d9d9d9',
    brightBlack: '#b8b8b8',
    brightRed: '#ffa3a3',
    brightGreen: '#99ff99',
    brightYellow: '#ffff99',
    brightBlue: '#a8cbff',
    brightMagenta: '#ffb3ff',
    brightCyan: '#a3ffff',
    brightWhite: '#ffffff',
  },
  fitGameColors: false,
  chrome: {
    raised: '#121212',
    text: '#ffffff',
    secondary: '#d6d6d6',
    tertiary: '#b0b0b0',
    title: '#d6d6d6',
    accent: '#5cc8ff',
    onAccent: '#000000',
    danger: '#ff9a90',
    dangerText: '#ff9a90',
    warn: '#ffd75f',
    warnText: '#ffd75f',
    success: '#7cf29a',
    sep: '#8a8a8a',
    edge: '#8a8a8a',
    keyRing: '#8a8a8a',
    selection: '#0b4f8a',
    selectionText: '#ffffff',
  },
};

// ── Rosé Pine ───────────────────────────────────────────────────────
// The main Rosé Pine variant as its own terminal ports ship it. The
// cursor is a neutral highlight, so the chrome takes iris as its
// accent.
const rosePine: AppTheme = {
  id: 'rose-pine',
  label: 'Rosé Pine',
  description: 'Muted rose, gold, and iris on a deep violet base.',
  source: 'Rosé Pine',
  author: 'Rosé Pine',
  license: 'MIT',
  xterm: {
    background: '#191724',
    foreground: '#e0def4',
    cursor: '#524f67',
    cursorAccent: '#e0def4',
    selectionBackground: '#403d52',
    selectionForeground: '#e0def4',
    black: '#26233a',
    red: '#eb6f92',
    green: '#31748f',
    yellow: '#f6c177',
    blue: '#9ccfd8',
    magenta: '#c4a7e7',
    cyan: '#ebbcba',
    white: '#e0def4',
    brightBlack: '#6e6a86',
    brightRed: '#eb6f92',
    brightGreen: '#31748f',
    brightYellow: '#f6c177',
    brightBlue: '#9ccfd8',
    brightMagenta: '#c4a7e7',
    brightCyan: '#ebbcba',
    brightWhite: '#e0def4',
  },
  fitted: {
    black: '#2b2940',
    green: '#79bcd9',
    yellow: '#f1bd73',
    white: '#dedcf2',
    brightBlack: '#9894b1',
    brightRed: '#ff98b2',
    brightGreen: '#9addfb',
    brightYellow: '#ffdfb4',
    brightBlue: '#bcf0f9',
    brightMagenta: '#e1caff',
    brightCyan: '#ffe1e0',
    brightWhite: '#ffffff',
  },
  chrome: { accent: '#c4a7e7' },
};

// ── Everforest ──────────────────────────────────────────────────────
// Everforest Dark at its medium background. The palette comes from
// autoload/everforest.vim and the ANSI mapping from the Terminal section
// of colors/everforest.vim, which repeats the eight colors for the
// bright slots and maps black and white as below.
//
//   bg0 #2d353b  bg3 #475258  fg #d3c6aa  bg_visual #543a48
//   red #e67e80  green #a7c080  yellow #dbbc7f  blue #7fbbb3
//   purple #d699b6  aqua #83c092
//   black bg3, white fg
//
// The cursor is fg on bg0, Everforest's default reversed cursor, and the
// selection is bg_visual. Everforest's green is the accent, the color
// its status line and ports lead with.
const everforestDark: AppTheme = {
  id: 'everforest-dark',
  label: 'Everforest Dark',
  description: 'Soft forest greens and warm earth tones on a gray green dark.',
  source: 'Everforest',
  author: 'sainnhe',
  license: 'MIT',
  xterm: {
    background: '#2d353b',
    foreground: '#d3c6aa',
    cursor: '#d3c6aa',
    cursorAccent: '#2d353b',
    selectionBackground: '#543a48',
    selectionForeground: '#d3c6aa',
    black: '#475258',
    red: '#e67e80',
    green: '#a7c080',
    yellow: '#dbbc7f',
    blue: '#7fbbb3',
    magenta: '#d699b6',
    cyan: '#83c092',
    white: '#d3c6aa',
    brightBlack: '#475258',
    brightRed: '#e67e80',
    brightGreen: '#a7c080',
    brightYellow: '#dbbc7f',
    brightBlue: '#7fbbb3',
    brightMagenta: '#d699b6',
    brightCyan: '#83c092',
    brightWhite: '#d3c6aa',
  },
  fitted: {
    foreground: '#e1d4b8',
    red: '#d06b6e',
    green: '#c4de9d',
    yellow: '#ccae71',
    cyan: '#8cca9b',
    brightBlack: '#96a3a9',
    brightRed: '#ffa3a3',
    brightGreen: '#e6ffc0',
    brightYellow: '#edce90',
    brightBlue: '#9fdcd3',
    brightMagenta: '#f8b9d6',
    brightCyan: '#acebbb',
    brightWhite: '#fcefd2',
  },
  chrome: { accent: '#a7c080' },
};

// ── Green Screen ────────────────────────────────────────────────────
// The old school MUD look. Green text on a monochrome terminal, with
// the game's own colors drawn as a CGA telnet client drew them. The
// ground is near black with the faintest green cast. Default text is
// a softened phosphor green near 11:1, the reading level of the house
// dark themes, where pure #00ff00 would glare at 14:1. The cursor is
// the same phosphor at full glow, and the selection is a deeper
// phosphor green. The cursor sits too near the success tone, so the
// window rule takes the magenta as the accent.
//
// The sixteen slots are the CGA palette (#aa0000, #00aa00, #aa5500,
// #0000aa, #aa00aa, #00aaaa, #aaaaaa, #555555, then the 55 and ff
// brights) as published. Dark red and dark blue sit at 2.5:1 and 1.5:1
// on the ground, as they did on a CGA screen, and the terminal draws
// them that way. The chat pane lifts the game colors it draws on the
// panel (chatColors.ts).
const greenScreen: AppTheme = {
  id: 'green-screen',
  label: 'Green Screen',
  description: 'Old school terminal. Phosphor green text and classic CGA colors on black.',
  source: 'the CGA palette',
  author: 'IBM',
  license: 'Public domain',
  xterm: {
    background: '#0a0e0b',
    foreground: '#84d48a',
    cursor: '#79f887',
    cursorAccent: '#0a0e0b',
    selectionBackground: '#358540',
    selectionForeground: '#79f887',
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
  },
  // The fit lifts blue to Lc 45.1 in play. Cyan at Lc 53.8 and bright
  // blue at 58.8 stay short (Q17).
  fitted: {
    foreground: '#8dde93',
    black: '#242424',
    red: '#fa6150',
    green: '#57de50',
    yellow: '#ffcaa9',
    blue: '#6091ff',
    magenta: '#e757e5',
    cyan: '#25b7b6',
    white: '#b2b2b2',
    brightBlack: '#969696',
    brightRed: '#ff938c',
    brightGreen: '#7eff7a',
    brightBlue: '#9aaaff',
    brightMagenta: '#ff84fd',
  },
};

// ── Solarized ───────────────────────────────────────────────────────
// The ground, the text, the cursor, the normal colors, bright red
// (orange), and bright magenta (violet) are Solarized's published values
// as its Xresources map them, save white on the light theme (below).
// The canonical mapping fills bright green, yellow, blue, and cyan with
// the grey base tones base01, base00, base0, and base1. MUD text leans
// on those four. Aabahran prints say in
// bright yellow, newbie in bright green, cabal in bright blue, and clan
// in bright cyan, so on the canonical mapping say turns grey, newbie
// drops to the comment tone, and cabal matches plain text. Instead,
// each of the four brights here is its
// accent moved 10 in CIELAB L*, the scale Solarized is built on, with
// its hue held. The step goes toward the theme's strong end, lighter on
// the dark ground and darker on the light one, the way Solarized moves
// emphasis from base1 to base01 when it flips modes. A lighter step on
// the cream ground would fall near 2:1. The darker step trims chroma
// only as far as sRGB needs, 13 percent at most.
//
// Bright black and bright white follow the same mode flip. The canonical
// bright black is base03, the dark ground itself, so dim text vanishes,
// and the canonical bright white is base3, the light ground. Here dim
// text takes the comment tone of each mode (base01 dark, base1 light)
// and bright white the far end (base3 dark, base03 light). The light
// theme also gives white base01, the light mode emphasis tone, since the
// canonical base2 sits at 1.1:1 on base3.
//
// The selection takes the tones Solarized's own Visual mode uses, base01
// dark and base1 light. Neither cursor carries color, so the chrome pins
// Solarized blue as its accent.
//
// The dark theme also pins danger. A dark theme's chrome reads danger
// from bright red, which Solarized fills with orange, so low HP and
// error words would turn orange beside the yellow warn tone. The pin is
// Solarized red lifted with its hue held until it clears 3:1 on the
// panel and the raised surface, the value the chrome derives from red
// itself. The light theme reads plain red and needs no pin.
//
// The dark theme stays out of Fit game colors (Themes review Q20). Its
// body text reads at Lc 39.5, far under the Lc 75 the game asks, and
// that is what Solarized is. Fitted, it would stop looking like the
// scheme you picked.
const solarizedDark: AppTheme = {
  id: 'solarized-dark',
  label: 'Solarized Dark',
  description:
    'Deep teal ground, muted grey text, blue accent. Bright colors keep their hue. ' +
    'Body text is low contrast by design, so Fit game colors leaves this theme as it ships.',
  source: 'Solarized',
  author: 'Ethan Schoonover',
  license: 'MIT',
  xterm: {
    background: '#002b36',
    foreground: '#839496',
    cursor: '#93a1a1',
    cursorAccent: '#002b36',
    selectionBackground: '#586e75',
    selectionForeground: '#eee8d5',
    black: '#073642',
    red: '#dc322f',
    green: '#859900',
    yellow: '#b58900',
    blue: '#268bd2',
    magenta: '#d33682',
    cyan: '#2aa198',
    white: '#eee8d5',
    brightBlack: '#586e75',
    brightRed: '#cb4b16',
    brightGreen: '#a1b42b',
    brightYellow: '#d3a32a',
    brightBlue: '#4fa5ef',
    brightMagenta: '#6c71c4',
    brightCyan: '#4dbcb3',
    brightWhite: '#fdf6e3',
  },
  fitGameColors: false,
  chrome: { accent: '#268bd2', danger: '#e8403a' },
};

const solarizedLight: AppTheme = {
  id: 'solarized-light',
  label: 'Solarized Light',
  description: 'Warm cream ground, slate text, blue accent. Bright colors keep their hue.',
  source: 'Solarized',
  author: 'Ethan Schoonover',
  license: 'MIT',
  xterm: {
    background: '#fdf6e3',
    foreground: '#657b83',
    cursor: '#586e75',
    cursorAccent: '#fdf6e3',
    selectionBackground: '#93a1a1',
    selectionForeground: '#073642',
    black: '#073642',
    red: '#dc322f',
    green: '#859900',
    yellow: '#b58900',
    blue: '#268bd2',
    magenta: '#d33682',
    cyan: '#2aa198',
    white: '#586e75',
    brightBlack: '#93a1a1',
    brightRed: '#cb4b16',
    brightGreen: '#6d7e00',
    brightYellow: '#957000',
    brightBlue: '#0371b0',
    brightMagenta: '#6c71c4',
    brightCyan: '#00867e',
    brightWhite: '#002b36',
  },
  fitted: {
    foreground: '#42575f',
    red: '#ff766a',
    green: '#4d5900',
    yellow: '#946f00',
    blue: '#278cd3',
    magenta: '#db3e88',
    cyan: '#0a9189',
    brightGreen: '#343d00',
    brightYellow: '#715400',
    brightBlue: '#006eac',
    brightMagenta: '#6a6ec1',
    brightCyan: '#00706a',
  },
  chrome: { accent: '#268bd2' },
};

// ── Srcery ──────────────────────────────────────────────────────────
// The Ghostty theme of srcery-terminal, as published. Its yellow cursor
// is its warn color, so the theme pins its bright cyan as the accent.
// The selection is the scheme's own cream under black text.
const srcery: AppTheme = {
  id: 'srcery',
  label: 'Srcery',
  description:
    'Cream text on a warm black, with bright colors made for the terminal first. Bright cyan accent.',
  source: 'Srcery',
  author: 'Daniel Berg',
  license: 'MIT',
  xterm: {
    background: '#121110',
    foreground: '#fce8c3',
    cursor: '#fed06e',
    cursorAccent: '#121110',
    selectionBackground: '#fce8c3',
    selectionForeground: '#121110',
    black: '#121110',
    red: '#ef2f27',
    green: '#519f50',
    yellow: '#fbb829',
    blue: '#2c78bf',
    magenta: '#e02c6d',
    cyan: '#0aaeb3',
    white: '#c5b088',
    brightBlack: '#917e6b',
    brightRed: '#f75341',
    brightGreen: '#98bc37',
    brightYellow: '#fed06e',
    brightBlue: '#68a8e4',
    brightMagenta: '#ff5c8f',
    brightCyan: '#2be4d0',
    brightWhite: '#fce8c3',
  },
  // In play the fit moves 15 slots and passes 45 of 46. Red stays short
  // at Lc 40.3 (Q1).
  fitted: {
    foreground: '#e8d5b0',
    black: '#282625',
    red: '#fe4135',
    green: '#92e28f',
    yellow: '#ecaa04',
    blue: '#4f9ae3',
    magenta: '#ff5589',
    cyan: '#36c3c7',
    brightBlack: '#a6937f',
    brightRed: '#ff9483',
    brightGreen: '#d1f878',
    brightBlue: '#79baf7',
    brightMagenta: '#ff90ac',
    brightCyan: '#32e8d3',
    brightWhite: '#fff0d3',
  },
  chrome: { accent: '#2be4d0' },
};

// ── Nightfly ────────────────────────────────────────────────────────
// The Ghostty theme of vim-nightfly-colors, as published, Night Owl's
// navy with its terminal colors reworked. Its cursor is gray, so the
// theme pins its bright magenta, a violet, as the accent.
const nightfly: AppTheme = {
  id: 'nightfly',
  label: 'Nightfly',
  description:
    'Deep navy night after Night Owl, with soft text, distinct bright colors and a violet accent.',
  source: 'nightfly',
  author: 'bluz71',
  license: 'MIT',
  xterm: {
    background: '#011627',
    foreground: '#bdc1c6',
    cursor: '#9ca1aa',
    cursorAccent: '#011627',
    selectionBackground: '#b2ceee',
    selectionForeground: '#080808',
    black: '#1d3b53',
    red: '#fc514e',
    green: '#a1cd5e',
    yellow: '#e3d18a',
    blue: '#82aaff',
    magenta: '#c792ea',
    cyan: '#7fdbca',
    white: '#a1aab8',
    brightBlack: '#7c8f8f',
    brightRed: '#ff5874',
    brightGreen: '#21c7a8',
    brightYellow: '#ecc48d',
    brightBlue: '#82aaff',
    brightMagenta: '#ae81ff',
    brightCyan: '#7fdbca',
    brightWhite: '#d6deeb',
  },
  // In play the fit moves 16 slots by small steps and passes 44 of 46.
  // Red stays short at Lc 38.1 and the yellow pair at dE 7.9 (Q1).
  fitted: {
    foreground: '#c9cdd2',
    red: '#f24746',
    green: '#96c152',
    yellow: '#e9d68f',
    blue: '#7aa1f6',
    magenta: '#be89e1',
    cyan: '#77d3c2',
    white: '#aab4c2',
    brightBlack: '#879b9b',
    brightRed: '#ff939d',
    brightGreen: '#62f5d4',
    brightYellow: '#ffe7c6',
    brightBlue: '#a2c1ff',
    brightMagenta: '#c9b1ff',
    brightCyan: '#98f5e3',
    brightWhite: '#dfe8f5',
  },
  chrome: { accent: '#ae81ff' },
};

// ── Melange ─────────────────────────────────────────────────────────
// The Ghostty themes of melange-nvim, as published. Neither cursor
// carries a hue, cream on the dark one and brown on the light one, so
// each pins its bright magenta as the accent, mauve on the dark ground
// and plum on the light one.
const melangeDark: AppTheme = {
  id: 'melange-dark',
  label: 'Melange Dark',
  description:
    'Warm coffee and clay. Muted earth colors on a dark brown ground, with a mauve accent.',
  source: 'Melange',
  author: 'Sergio Alejandro Vargas',
  license: 'MIT',
  xterm: {
    background: '#292522',
    foreground: '#ece1d7',
    cursor: '#ece1d7',
    cursorAccent: '#292522',
    selectionBackground: '#403a36',
    selectionForeground: '#ece1d7',
    black: '#34302c',
    red: '#bd8183',
    green: '#78997a',
    yellow: '#e49b5d',
    blue: '#7f91b2',
    magenta: '#b380b0',
    cyan: '#7b9695',
    white: '#c1a78e',
    brightBlack: '#867462',
    brightRed: '#d47766',
    brightGreen: '#85b695',
    brightYellow: '#ebc06d',
    brightBlue: '#a3a9ce',
    brightMagenta: '#cf9bc2',
    brightCyan: '#89b3b6',
    brightWhite: '#ece1d7',
  },
  // In play the fit moves 16 slots and passes 44 of 46. Yellow stays
  // short at Lc 58.5 and red at Lc 41.5 (Q1).
  fitted: {
    black: '#393531',
    red: '#c08485',
    green: '#b7dab8',
    yellow: '#eda365',
    blue: '#899cbd',
    magenta: '#bd8aba',
    cyan: '#a0bcba',
    white: '#ccb299',
    brightBlack: '#aa9885',
    brightRed: '#fe9d8a',
    brightGreen: '#cbffdb',
    brightYellow: '#f7cc79',
    brightBlue: '#b1b8dd',
    brightMagenta: '#deaad1',
    brightCyan: '#b3dfe2',
    brightWhite: '#fffefc',
  },
  chrome: { accent: '#cf9bc2' },
};

const melangeLight: AppTheme = {
  id: 'melange-light',
  label: 'Melange Light',
  description:
    'The light twin of Melange Dark. Warm brown ink on soft gray paper, with a plum accent.',
  source: 'Melange',
  author: 'Sergio Alejandro Vargas',
  license: 'MIT',
  xterm: {
    background: '#f1f1f1',
    foreground: '#54433a',
    cursor: '#54433a',
    cursorAccent: '#f1f1f1',
    selectionBackground: '#d9d3ce',
    selectionForeground: '#54433a',
    black: '#e9e1db',
    red: '#c77b8b',
    green: '#6e9b72',
    yellow: '#bc5c00',
    blue: '#7892bd',
    magenta: '#be79bb',
    cyan: '#739797',
    white: '#7d6658',
    brightBlack: '#a98a78',
    brightRed: '#bf0021',
    brightGreen: '#3a684a',
    brightYellow: '#a06d00',
    brightBlue: '#465aa4',
    brightMagenta: '#904180',
    brightCyan: '#3d6568',
    brightWhite: '#54433a',
  },
  // It passes 35 of 46 as published. In play the fit moves 9 slots,
  // most of the change in the greens, and passes all 46.
  fitted: {
    black: '#dfd7d2',
    red: '#c97c8c',
    green: '#2c5631',
    yellow: '#b85a00',
    cyan: '#608383',
    brightRed: '#c00222',
    brightGreen: '#003218',
    brightYellow: '#835900',
    brightWhite: '#3e2e26',
  },
  chrome: { accent: '#904180' },
};

// ── Modus Vivendi ───────────────────────────────────────────────────
// The dark theme of the Modus themes, as modus-themes.el resolves its
// palette, built to WCAG AAA contrast and licensed GPL like Vosh. Its
// cursor is white, so the theme pins its blue as the accent.
const modusVivendi: AppTheme = {
  id: 'modus-vivendi',
  label: 'Modus Vivendi',
  description: 'White text on pure black, built for the highest contrast. Clear blue accent.',
  source: 'the Modus themes',
  author: 'Protesilaos Stavrou',
  license: 'GPL-3.0-or-later',
  xterm: {
    background: '#000000',
    foreground: '#ffffff',
    cursor: '#ffffff',
    cursorAccent: '#000000',
    selectionBackground: '#5a5a5a',
    selectionForeground: '#ffffff',
    black: '#000000',
    red: '#ff5f59',
    green: '#44bc44',
    yellow: '#d0bc00',
    blue: '#2fafff',
    magenta: '#feacd0',
    cyan: '#00d3d0',
    white: '#a6a6a6',
    brightBlack: '#595959',
    brightRed: '#ff6b55',
    brightGreen: '#00c06f',
    brightYellow: '#fec43f',
    brightBlue: '#79a8ff',
    brightMagenta: '#b6a0ff',
    brightCyan: '#6ae4b9',
    brightWhite: '#ffffff',
  },
  // In play the fit lowers body text to #e4e4e4, so bold white reads
  // above it, and lifts black to the dimmed ground Modus uses itself. It
  // moves 15 slots and passes 45 of 46, with red short at Lc 43.5 (Q1).
  fitted: {
    foreground: '#e4e4e4',
    black: '#1e1e1e',
    red: '#f85954',
    green: '#81f67e',
    blue: '#25a8f8',
    magenta: '#e797bb',
    cyan: '#00cecb',
    white: '#b1b1b1',
    brightBlack: '#959595',
    brightRed: '#ff9380',
    brightGreen: '#aaffc8',
    brightYellow: '#ffcd62',
    brightBlue: '#90b7ff',
    brightMagenta: '#c3b3ff',
    brightCyan: '#6fe9bd',
  },
  chrome: { accent: '#2fafff' },
};

// ── Harbor Dark ─────────────────────────────────────────────────────
// The GitHub Dark Default palette of GitHub's theme for VS Code, as
// published, under a name of its own, since GitHub is a trademark
// (Themes review Q6). GitHub publishes no terminal selection, so the
// selection is its list selection, #6e768166 laid over the ground. The
// blue cursor is the accent the review picked, and the theme pins it.
const harborDark: AppTheme = {
  id: 'harbor-dark',
  label: 'Harbor Dark',
  description: 'Bright text on a blue black ground, with clear colors and a blue accent.',
  source: 'GitHub Dark Default',
  author: 'GitHub',
  license: 'MIT',
  xterm: {
    background: '#0d1117',
    foreground: '#e6edf3',
    cursor: '#2f81f7',
    cursorAccent: '#0d1117',
    selectionBackground: '#343941',
    selectionForeground: '#e6edf3',
    black: '#484f58',
    red: '#ff7b72',
    green: '#3fb950',
    yellow: '#d29922',
    blue: '#58a6ff',
    magenta: '#bc8cff',
    cyan: '#39c5cf',
    white: '#b1bac4',
    brightBlack: '#6e7681',
    brightRed: '#ffa198',
    brightGreen: '#56d364',
    brightYellow: '#e3b341',
    brightBlue: '#79c0ff',
    brightMagenta: '#d2a8ff',
    brightCyan: '#56d4dd',
    brightWhite: '#ffffff',
  },
  // In play the fit moves 11 slots and passes 45 of 46. Red stays short
  // at Lc 40.8.
  fitted: {
    foreground: '#dee5eb',
    red: '#e4635c',
    green: '#7af185',
    yellow: '#e1a837',
    blue: '#56a4fd',
    brightBlack: '#8e97a2',
    brightGreen: '#b9ffbc',
    brightYellow: '#fccc5d',
    brightBlue: '#82c4ff',
    brightMagenta: '#d6b0ff',
    brightCyan: '#6ae6ef',
  },
  chrome: { accent: '#2f81f7' },
};

// ── Iceberg Dark ────────────────────────────────────────────────────
// The dark terminal colors of iceberg.vim, as published. Its cursor is
// the gray of its text, so the theme pins its magenta, a soft violet,
// as the accent the review picked.
const icebergDark: AppTheme = {
  id: 'iceberg-dark',
  label: 'Iceberg Dark',
  description: 'A blue gray night with muted pastel colors and a soft violet accent.',
  source: 'Iceberg',
  author: 'cocopon',
  license: 'MIT',
  xterm: {
    background: '#161821',
    foreground: '#c6c8d1',
    cursor: '#c6c8d1',
    cursorAccent: '#161821',
    selectionBackground: '#272c42',
    selectionForeground: '#c6c8d1',
    black: '#1e2132',
    red: '#e27878',
    green: '#b4be82',
    yellow: '#e2a478',
    blue: '#84a0c6',
    magenta: '#a093c7',
    cyan: '#89b8c2',
    white: '#c6c8d1',
    brightBlack: '#6b7089',
    brightRed: '#e98989',
    brightGreen: '#c0ca8e',
    brightYellow: '#e9b189',
    brightBlue: '#91acd1',
    brightMagenta: '#ada0d3',
    brightCyan: '#95c4ce',
    brightWhite: '#d2d4de',
  },
  // In play the fit moves 15 slots and passes 44 of 46. Yellow stays
  // short at Lc 58.1 and red at Lc 37.8.
  fitted: {
    foreground: '#cbcdd6',
    black: '#272b3c',
    red: '#d16869',
    green: '#cfda9c',
    yellow: '#e0a276',
    blue: '#809bc1',
    cyan: '#8cbbc5',
    brightBlack: '#9196b0',
    brightRed: '#f99897',
    brightGreen: '#f1fbbd',
    brightYellow: '#fdc49b',
    brightBlue: '#9fbbe0',
    brightMagenta: '#c0b3e7',
    brightCyan: '#acdce6',
    brightWhite: '#e6e9f3',
  },
  chrome: { accent: '#a093c7' },
};

export const BUILTIN_THEMES: AppTheme[] = [
  obsidianEmber,
  triad,
  rubric,
  kansoZen,
  tokyoNight,
  nord,
  rosePine,
  gruvbox,
  catppuccin,
  dracula,
  monokai,
  oneHalfDark,
  solarizedDark,
  solarizedLight,
  tangoDark,
  classicVivid,
  highContrast,
  everforestDark,
  greenScreen,
  srcery,
  nightfly,
  melangeDark,
  melangeLight,
  modusVivendi,
  harborDark,
  icebergDark,
];

// ── Color vision swaps ──────────────────────────────────────────────
// The swap of each built in theme for each color vision other than
// Typical, worked out ahead like `fitted` (theme/gameFit swapFor). Each
// vision holds two rows, the swap of the palette Typical plays with Fit
// game colors on (`fitted`), and of the published palette, which plays
// with it off (`published`). A theme whose Typical fit is empty, and
// Solarized Dark, which keeps out of the fit, hold the published row
// alone. Each string holds the colors of GAME_SLOTS in order, body text
// then the 16 ANSI colors, with a dot for a slot left as published.
// gameFit.test.ts works each out again with VOSH_FIT_THEMES=1.
type OtherVision = Exclude<ColorVision, 'typical'>;
interface VisionRows {
  fitted?: string;
  published: string;
}
const VISION_FITS: Readonly<Record<string, Readonly<Record<OtherVision, VisionRows>>>> = {
  'obsidian-ember': {
    deuteranopia: {
      fitted:
        '#cecbc9 . #d07448 #7ab6f5 #d5b267 #8a8fd9 #b08ac5 #75bfcb . #99948f #feac65 #deffe7 #fcd893 #b8beff #d0aae2 #9de3ee .',
      published:
        '. . #da7c51 #80b4f6 #e1be73 #8c8ed9 #b28cc7 #73bdc9 . . #ed915d #9cd1ff #f6d28e #b1b6ff #cca7de #9fe5f0 .',
    },
    protanopia: {
      fitted:
        '#cecbc9 . #d07448 #a3cbff #d6b267 #8e93de #af89c4 #79c3cf . #99948f #fd947f #c7ffd8 #f6d38d #aeb2ff #d0aae2 #a4eaf5 .',
      published:
        '. . #d97c50 #89bdff #deba6f #9596e2 #ad87c2 . . . #f18978 #96e1af #f2cf8b #c9ccff #c9a4db #9fe5f0 .',
    },
    tritanopia: {
      fitted:
        '#cecbc9 . #cf7065 #95e4b0 #d2af64 #da92d3 #cc819e #79c3cf . #99948f #f59989 #c7ffd8 #f2cf8a #9bbef1 #f4abd4 #9de3ee .',
      published: '. . . . #d6b368 #e69ee0 #d68aa9 . . . . . #f4d18d #e6b4ff #f4a2b4 . .',
    },
  },
  triad: {
    deuteranopia: {
      published: '. . #e87500 #76b6fc . #948dd9 . #a1bcbe . . #f49f5d #abfede . #b8bbff . . .',
    },
    protanopia: {
      published: '. . #f76a1c #5ed3fe . #9d91dd . . . . #ff9e72 . . #b4baff . #89e7ff .',
    },
    tritanopia: { published: '. . . . . #cc85c5 #ce7f91 . . . #fc988b . . #dda9f7 #f3a6c8 . .' },
  },
  rubric: {
    deuteranopia: {
      published:
        '. . #dc5b00 #134a83 #5c3f00 #7f6fc6 #c56997 #335f82 . . #8a2800 #002d5f . #5c50ba #a6468a #003e54 .',
    },
    protanopia: {
      published:
        '. . #dc5b00 #1b4d87 #5b3e00 #8576cd #b85d8b #346084 . . #993d00 #002d5f . #6754bf #84256b #003e54 .',
    },
    tritanopia: {
      published: '. . . . #593d00 #b26bb2 #b2557d . . . #810003 . . #7d399a #972c60 . .',
    },
  },
  'kanso-zen': {
    deuteranopia: {
      fitted:
        '#c9cdcb #656565 #d67f46 #83bcfc #d0be95 #918ed9 . #a0b6b4 #b7bab7 #92979d #fd9a6d #dfeeff #ffe8bf #d2bbff #bab1d1 #acdbd1 #f0f5f2',
      published: '. . #cc7044 #7fcaff . #9f93df . . . . #e87d38 #78d9ff . #a3a4f1 . . .',
    },
    protanopia: {
      fitted:
        '#c9cdcb #656565 #d97d4e #70d2fd #d0be95 #978cd7 . #a0b6b4 #b7bab7 #92979d #ff9480 #dfeeff #ffe8bf #a9affc #bab1d1 #acdbd1 #f0f5f2',
      published:
        '. . #cf6b57 #6a9cdd . #b8adfb . . . . #e77841 #4eadda #f3d090 #cfbaff #9188a7 . .',
    },
    tritanopia: {
      fitted:
        '#c9cdcb #656565 #d17f79 #d4e5c4 #d0be95 #e39bdc #d28bb3 #a0b6b4 #b7bab7 #92979d #ff919a #fafffa #ffe9c2 #ffc3ff #f7add6 #acdbd1 #f0f5f2',
      published: '. . #c3736d . . #e89fe1 #d084a1 . . . . . #ffe8c0 #fab1f4 #e393a9 . .',
    },
  },
  'tokyo-night': {
    deuteranopia: {
      fitted:
        '#bdc7f2 #2c2e36 #e86982 #c9fc95 . #a988e1 #b897f4 #6dbfef #aab3d8 #8e97bb #ffae9d #eaffd9 #ffd08d #b2b9ff #d4bfff #a8deff #dbe2ff',
      published:
        '. . #ef8631 #c9fb95 #e9b770 #9c8de9 . . . . #f38045 #c8fa94 #e9b770 #9699f5 #be9dfb . .',
    },
    protanopia: {
      fitted:
        '#bdc7f2 #2c2e36 #e17920 #c9fc95 #f1bf78 #9c8de9 . #70c2f2 #aab3d8 #8e97bb #ff9887 #eaffd9 #ffe2bb #afb2ff #d4bfff #a8deff #dbe2ff',
      published:
        '. . #f0745b #b4e580 #e2b16a #be9df8 #aa89e5 . . . #f58148 #b1e27d #e2b16a #9ea4ff #c6a8ff . .',
    },
    tritanopia: {
      fitted:
        '#bdc7f2 #2c2e36 #e86982 #bbed87 . #ca7dc4 #f089b3 #70c2f2 #aab3d8 #8e97bb #ff94a5 #eaffd9 #ffd8a1 #d1a4f3 #feb4de #a8deff #dbe2ff',
      published:
        '. . #f6758d #bbed87 #ffd292 #e498e3 #f38bb5 . . . #f6758d #bbed87 #ffd292 #d798eb #ffa1b8 . .',
    },
  },
  nord: {
    deuteranopia: {
      fitted:
        '. #3c4353 #c3683c #75b0ef #e9c989 #83a4c4 #bb95b4 #8cc5d5 . #95a1b7 #f8a460 #8ddeff #ffeac1 #c2b1ff #dcb4d4 #b6e4e3 #feffff',
      published: '. . #bf6439 #a0caff . #9c93df . . . . #c0653a #75d7ff . #9994e0 #b38dac . .',
    },
    protanopia: {
      fitted:
        '. #3c4353 #c3683c #55b0df #e9c989 #ac95e1 #bb95b4 #8cc5d5 . #95a1b7 #ff9e87 #94d4ff #ffeac1 #cdb5ff #dcb4d4 #b6e4e3 #feffff',
      published:
        '. . #c06539 #95c3ff #eccc8c #9e93df . #87bfcf . . #c0653a #95c3ff #eccc8c #a498e5 . #8ebbba .',
    },
    tritanopia: {
      fitted:
        '. #3c4353 #c3656e #95af7e #e9c989 #d58ecf #db8a9d #8cc5d5 . #95a1b7 #fe9da4 #ceeab6 #ffeac1 #deb3ff #f7add6 #b6e4e3 #feffff',
      published: '. . #bd5f68 . . #e69edf #cd819f . . . #bd5f68 . . #d79fe9 #cd829f . .',
    },
  },
  'rose-pine': {
    deuteranopia: {
      fitted:
        '. #2b2940 #ea6e91 #83b6f9 #f1bd73 #9b90dc . . #dedcf2 #9894b1 #ff9686 #b4d5ff #ffdfb4 #c0b6ff #e1caff #ffe1e0 #ffffff',
      published:
        '. . #ee7295 #5b8dcc #f7c278 #c4bbff #bfa2e2 . . . #eb7942 #3f90c3 #f7c278 #d3bcff . . .',
    },
    protanopia: {
      fitted:
        '. #2b2940 #ea6e91 #87bafd #eeba70 #ccc1ff #bda0e0 #e9bab8 #dedcf2 #9894b1 #ff9686 #afdaff #ffdfb4 #bcf0f9 #dfc8fd #ffe1e0 #ffffff',
      published: '. . #ea6e91 #6598d8 . . . . . . #eb7942 #6295d4 . #bdc0ff #c5a8e8 . .',
    },
    tritanopia: {
      fitted:
        '. #2b2940 . #79bcd9 #eebb71 #bf85cd #e799b7 #e7b8b6 #dedcf2 #9894b1 #ff98b2 #9addfb #ffdfb4 #d5a7f7 #ffc4e4 #ffe1e0 #ffffff',
      published: '. . #ea6e91 . . #c691dd #e99bb9 . . . #ea6e91 . . #cfa1f1 #ee9caf #f1c2c0 .',
    },
  },
  gruvbox: {
    deuteranopia: {
      fitted:
        '#e4d4ac #383838 #cf5500 #88bdff . #9195e1 #d582a7 #8ec590 #c5b59f #a8998a #fa9770 #95e1ff #ffc237 #eae0ff #f4a4b9 #b2e6a0 #ffefc5',
      published:
        '. . #b64a00 #4ca0e9 #d89a22 #7c6fb8 . . . . #eb6200 #95e1ff . #b5aaf7 . #8fc17d .',
    },
    protanopia: {
      fitted:
        '#e4d4ac #383838 #df3e1e #85baff . #9d92de #d582a7 #8ec590 #c5b59f #a8998a #fe9483 #99e2ff . #d9c6ff #f4a4b9 #b2e6a0 #ffefc5',
      published: '. . #cb2701 #59beff . #7c70b9 . . . . #eb6200 #8ddfff . #9e93df #df91a6 . .',
    },
    tritanopia: {
      fitted:
        '#e4d4ac #383838 #e03c30 #d2d362 . #c685c8 #da8196 #8ec590 #c5b59f #a8998a #ff9583 #faff77 #fdbf33 #a1c4b6 #f9a6ba #b8eca6 #ffefc5',
      published: '. . . #9b9a20 . #a5619f #b86479 . . . . . . #d9abfb #d3859e . .',
    },
  },
  catppuccin: {
    deuteranopia: {
      fitted:
        '. . #d5784c #7db9f8 #f4deaa #8f92de #d4a2c7 . . #9498af #f4a25c #b0eeab #fff1d2 #cbc9ff . #c4fff4 #edf1ff',
      published:
        '. . #d5863e #aae7a5 #fff1d1 #a498e6 . . . . #e89750 #b4f2af #fff1d1 #a9affe #fcc8ee . .',
    },
    protanopia: {
      fitted:
        '. . #cb7a35 #89c484 #f3ddaa #a196e3 #d4a2c7 . . #9498af #ff9988 #b0eeab #fff1d2 #c0b7ff . #c4fff4 #edf1ff',
      published:
        '. . #d98347 #98d493 #ffedc5 #a89deb . . . . #f39367 #adeaa8 #ffedc5 #abaaf9 . . .',
    },
    tritanopia: {
      fitted:
        '. . #d5708d #89c484 #f1dba8 #c781c1 #e799b7 . . #9498af #fe95b2 #adeba8 #fff1d2 #d2a5f4 #ffc3e4 #c4fff4 #edf1ff',
      published: '. . . . . #c781c1 #ffbfd7 . . . . . . #d39eea #ffc3e0 . .',
    },
  },
  dracula: {
    deuteranopia: {
      fitted:
        '#e4e4df #2c2e34 #e55f00 #00c0ff #e2eb7d #8b8ef5 #ed69b6 #66c4d8 #deded9 #8597cb #f3a15b #95e1ff #feffc9 #cbaeff #f696da #aeffff .',
      published:
        '. . #eb6a00 #7fb7ff . #8b8ef5 #ff98d0 #62c1d4 . . #fa7834 #3acfff . #cdc3ff #ffb5e7 . .',
    },
    protanopia: {
      fitted:
        '#e4e4df #2c2e34 #ed4f00 #00caff #e2eb7d #918cf4 #f36ebb #81dff3 #deded9 #8597cb #fc9c70 #6cff95 #feffc9 #c1a6fa #fc9bdf #aeffff .',
      published:
        '. . #fd5932 #76d9ff . #ab9aff . #62c1d4 . . #fa7834 #95e1ff . #c7adff #fe91de . .',
    },
    tritanopia: {
      fitted:
        '#e4e4df #2c2e34 #f2494b #07d558 #e2eb7d #ed93e5 #f569a7 #81dff3 #deded9 #8597cb #ffb7b3 . #feffc9 #fbbdff #ff97cf #aeffff .',
      published: '. . . . . #ffadf7 #ff7db5 . . . . . . #ffc9fd #ff99c2 . .',
    },
  },
  monokai: {
    deuteranopia: {
      fitted:
        '#e4e4df #363831 #fe7121 #00c5ff #f6c278 #55cae0 #af84ff #85d2c8 #deded9 #a09c87 #ff9d82 #96e1ff #ffe4c1 #c6c0ff #c7adff #a9f8ec #fffffd',
      published:
        '. . #e15d00 #0cc5ff . #d2c3ff . #7ac7bd . . #e35e00 #25c4ff . #d2c3ff #ac7ffd #7ac7bd .',
    },
    protanopia: {
      fitted:
        '#e4e4df #363831 #ff648c #82baff #f1bd73 #c0c5ff #af84ff #83d0c6 #deded9 #a09c87 #ff9d8d #d9ffa6 #ffdfb5 #7eeaff #d7c6ff #adfcf0 #fffffd',
      published:
        '. . #f92772 #99e2ff . #64d7ed . #78c5ba . . #e25e00 #92dfff . #c9b2ff . #7bc8be .',
    },
    tritanopia: {
      fitted:
        '#e4e4df #363831 #ff648c #bbf94d #ebb76d #c68bd3 #ff7ab5 #89d6cc #deded9 #a09c87 #ff99ad #d8ffa4 #ffe0b8 #dcaefe #ffbfe2 #a9f8ec #fffffd',
      published: '. . . #a6e22d #ffd499 #e19bdf #f167a4 . . . . . #ffd499 #d3a5f5 #ee68ae . .',
    },
  },
  'one-half-dark': {
    deuteranopia: {
      fitted:
        '. #373c44 #df7f38 #7bbefb #e0bc77 #9395e9 #cd7ee4 #65c4d0 #dbdee3 #939eb2 #ffb5a7 #e9ffdc #ffdd9f #cdc0ff #ebadff #92f2fe .',
      published: '. . #db7442 #91d1ff . #a19cf1 . . . . #dd7543 #91d1ff . #a59ff4 . . .',
    },
    protanopia: {
      fitted:
        '. #373c44 #e9747d #74c2fa #e0bc77 #b3a1f7 #cd7ee4 #66c5d1 #dbdee3 #939eb2 #ff9f8e #e3ffd1 #ffdd9e #9ed1ff #e9a2ff #89e8f4 .',
      published:
        '. . #da7240 #78cbff #e7c27d #a3a4f9 #c577dc #55b5c1 . . #dd7543 #78cbff #e7c27d #b09ff4 #c476db #55b5c1 .',
    },
    tritanopia: {
      fitted:
        '. #373c44 #e7727b #c6f3a6 #dcb873 #c487cf #dc8cf4 #66c5d1 #dbdee3 #939eb2 #ffbdbf #e3ffd1 #ffdd9e #dbaefe #ff98c7 #87e6f2 .',
      published:
        '. . . #9cc77d #e6c17c #cc85c5 #f178b6 . . . . #9ac57b #e6c17c #c595e3 #ff8bc9 . .',
    },
  },
  'solarized-dark': {
    deuteranopia: {
      published:
        '. . #c85200 #88bfff #be9218 #8877d5 . . . . #d9523f #90e0ff #ddac37 #9196f5 . . .',
    },
    protanopia: {
      published: '. . #dc332f #0ebefd . #8879d7 #d23581 . . . #c65100 #76d9ff . #ae8dea . . .',
    },
    tritanopia: {
      published:
        '. . . #849800 #b48800 #dd8ddb #d23684 . . . . #a1b42c #eab947 #e4b5ff #e9849f . .',
    },
  },
  'solarized-light': {
    deuteranopia: {
      fitted:
        '#42575f . #f97d3f #3767a4 #936e00 #8574d3 #e84b93 #0a9189 . . #c65100 #00376d #705300 #424191 #7479cd #004c48 .',
      published:
        '. . #c85200 #0099e0 . #7160bc . . . . #b04700 #005b75 #936e00 #393484 #6367b9 . .',
    },
    protanopia: {
      fitted:
        '#42575f . #f97d3f #003f70 #8f6b00 #847ad9 #db3e88 #0a9189 . . #b94b00 #00132f #6b4f00 #5e4ea0 #6a6ec1 #00706a .',
      published: '. . #d62e18 #0099e0 . #6a57b2 #ff69aa . . . #c14f00 #005b97 . #463384 . . .',
    },
    tritanopia: {
      fitted:
        '#42575f . #ff766a #4d5900 #967105 #ab68ba #db3f8b #0a9189 . . . #343d00 #715400 #632162 #9c3f59 #00706a .',
      published:
        '. . . #869a03 #b78b06 #d784d0 #e54994 . . . #a63600 . #654b00 #8e4e96 #b65775 . .',
    },
  },
  'tango-dark': {
    deuteranopia: {
      fitted:
        '#d4d8d0 #3d4345 #ed6300 #00c9ff #e5c140 #70a3e7 #bc94c2 #5cd0d2 #d6dad2 #adafab #fb906e #cfffaf . #bcc2ff #e9b9e4 #57faf9 #fbfbf9',
      published: '. . #b63f00 #008dd3 . #5f59a3 . . . . #d55800 #82c9ff . #9599e5 . . .',
    },
    protanopia: {
      fitted:
        '#d4d8d0 #3d4345 #fa4d07 #83d1ff #dcb834 #70a3e7 #bc94c2 #58ccce #d6dad2 #adafab #fc936c #caffa6 . #beb2ff #e2b2dd #53f7f6 #fbfbf9',
      published: '. . #ca0e00 #2987f0 . #625aa4 . . . . #d55800 #96c4ff . #979de8 . . .',
    },
    tritanopia: {
      fitted:
        '#d4d8d0 #3d4345 #fe4a3b #a3f476 #dcb834 #dd98dc #d88aa2 #58ccce #d6dad2 #adafab #fe8e81 #caffa6 . #fdb9ff #f7a9cb #4cf2f1 #fbfbf9',
      published: '. . . . #dcb834 #9a5896 #8b4663 #04989a . . . . . #bc8fdc #c27691 . .',
    },
  },
  'classic-vivid': {
    deuteranopia: {
      fitted:
        '. #232323 #ed5b2c #00caff #ffd4b9 #7876fc #e756e4 #0badac #b2b2b2 #959595 #fe8572 #84ff7c . #a393fa #ff99fc . .',
      published:
        '. . #9f2600 #0899ff #ab5601 #3d008d . . . . #e75500 #6dd2ff . #7d44f4 #ff50fd #27ffff .',
    },
    protanopia: {
      fitted:
        '. #232323 #ef583d #00bdfc #ffdcc6 #8b7bff #e756e4 #0badac #b2b2b2 #959595 #fb8b57 #00fe00 . #b49aff #ff84fc . .',
      published: '. . #a80a00 #0097ec . #3d008d . . . . #e35200 #00c1ff . #7b46f5 . . .',
    },
    tritanopia: {
      fitted:
        '. #232323 #ee5645 #4cd546 #ffc6a2 #c05ed6 #ff61a9 #0badac #b2b2b2 #959595 #ff8574 . . #d089e3 #ff9ac4 . .',
      published: '. . . . . #610075 #ba0076 . . . . . . #a22fd9 #ff4ac1 . .',
    },
  },
  'high-contrast': {
    deuteranopia: {
      published: '. . #ec763b #1abcff . #8d8de2 . #2acfcf . . #f09f58 #beffbd . #bcc2ff . . .',
    },
    protanopia: {
      published: '. . #f07a3f #78b4ff . #988be0 #f984f9 . . . #ffa278 . . #afcfff #ffb8ff . .',
    },
    tritanopia: {
      published: '. . #f57172 . . #c381c7 #ff8cc1 . . . #f69a9b . . #cda0ef #ffb8df . .',
    },
  },
  'everforest-dark': {
    deuteranopia: {
      fitted:
        '#e1d4b8 . #cf6a6d #7cc2fd #cbad70 #969ae6 #d598b5 #8cca9b . #96a3a9 #ffab66 #b1e8ff #f2d394 #c9c2ff #f7b8d5 #b0efbf #fcefd2',
      published: '. . #e18357 #7ccaff . #a498e5 . . . . #e18357 #78c7fd . #deceff . . .',
    },
    protanopia: {
      fitted:
        '#e1d4b8 . #cd7045 #8bbfff #ceb073 #a297e3 #d194b1 #8fcd9e . #96a3a9 #ffa494 #b0e8ff #f1d294 #d1cdff #f2b4d1 #afeebe #fcefd2',
      published:
        '. . #e38459 #71c1f7 #f0d193 #a89dea . . . . #e38559 #70bff5 #f0d193 #e1d2ff . . .',
    },
    tritanopia: {
      fitted:
        '#e1d4b8 . #d06b6e #c4de9d #ccae71 #c08eda #e193b1 #8cca9b . #96a3a9 #ffa3a3 #e6ffc0 #edce90 #b8f6ec #ffb9da #acebbb #fcefd2',
      published:
        '. . . #cee8a6 #eacb8e #c98ed7 #e293ae . . . . #cee8a6 #eacb8e #d499e3 #f19fb1 . .',
    },
  },
  'green-screen': {
    deuteranopia: {
      fitted:
        '#8dde93 #242424 #e87700 #03bdff #ffcaa9 #6091ff #e757e5 #25b7b6 #b2b2b2 #969696 #f4a25a #97dcff . #bca7fe #ff95fc . .',
      published:
        '. . #983400 #0099f8 #b05a0c #3d008d . . . . #f66600 #72d8ff . #7b46f5 . #71ffff .',
    },
    protanopia: {
      fitted:
        '#8dde93 #242424 #fa6248 #3bc7ff #ffdeca #6091ff #e757e5 #25b7b6 #b2b2b2 #969696 #fb9a6d #99ddff . #b0a7fe #ff84fd . .',
      published: '. . #a80a00 #00acfc . #3d008d . . . . #f66700 #3acfff . #7b46f5 . . .',
    },
    tritanopia: {
      fitted:
        '#8dde93 #242424 #fa6150 #57de50 #ffceb0 #c974dc #ff6cae #25b7b6 #b2b2b2 #969696 #ff938c #7eff7a . #d1a0f2 #ff9cd5 . .',
      published: '. . . . . #610075 #ba0076 . . . . . . #a22fd9 #ff6dc8 . .',
    },
  },
  srcery: {
    deuteranopia: {
      fitted:
        '#e8d5b0 #282625 #ea6100 #5fb9ff #ecaa04 #8a8de8 #ff5589 #36c3c7 . #a6937f #fc9871 #8fdfff . #caaffc #ff90ac #5cffea #fff0d3',
      published: '. . #d55800 #5697e8 . #6e69c0 . . . . #ed6300 #99e2ff . #afa4f2 . . .',
    },
    protanopia: {
      fitted:
        '#e8d5b0 #282625 #fe432a #46c8ff #ecaa04 #8f8ce6 #ff5589 #36c3c7 . #a6937f #ff9483 #96e1ff . #bea6f3 #ff90ac #32e8d3 #fff0d3',
      published: '. . #ee3201 #2493d6 . #7767bf . . . . #ed6300 #68d6ff . #b4a9f7 . #a6fff1 .',
    },
    tritanopia: {
      fitted:
        '#e8d5b0 #282625 #fe4135 #92e28f #ecaa04 #c87bc1 #f958a3 #37c4c8 . #a6937f #ff9483 #d1f878 . #76b7f4 #ff90ac #4df9e3 #fff0d3',
      published: '. . . . #f9b625 #995ba7 #da3087 . . . . . #fbce6b #e1bbff #f95ea5 . .',
    },
  },
  nightfly: {
    deuteranopia: {
      fitted:
        '#c9cdd2 . #e45f00 #34bdff #e9d68f #8a8ee9 #b984db #77d3c2 #aab4c2 #879b9b #f2a05a #99e2ff #ffe7c6 #b8beff #c6aefc #98f5e3 #dfe8f5',
      published:
        '. . #f16400 #74cbff . #a091ec . #82decd . . #ff8f02 #24c8a9 #f8cf98 #9ca2fd #ac7ffd #82decd .',
    },
    protanopia: {
      fitted:
        '#c9cdd2 . #f04e0f #5bc3ff #e9d68f #a095f1 #b681d9 #77d3c2 #aab4c2 #879b9b #fc9b6f #62f5d4 #ffe7c6 #d6c7ff #c1a9f6 #a0feeb #dfe8f5',
      published:
        '. . #e27e00 #6dc6ff #f6e49c #998ce7 #c993ec #a3ffed . . #ff664a #22c7a8 #fed59e #c09ffa #ac7ffd #a3ffed .',
    },
    tritanopia: {
      fitted:
        '#c9cdd2 . #f24746 #96c152 #e9d68f #c97cc2 #e47da7 #77d3c2 #aab4c2 #879b9b #ff939d #62f5d4 #ffe7c6 #d3a2f0 #ffb4de #98f5e3 #dfe8f5',
      published: '. . . . . #cb7ec4 #f185a4 . . . . . . #d093e4 #fd78c3 . .',
    },
  },
  'melange-dark': {
    deuteranopia: {
      fitted:
        '. #393531 #be8283 #87bafd #eda365 #9593df #bd8aba #a0bcba #ccb299 #aa9885 #ffa790 #b1d9ff #fed27f #cbbbff #deaad1 #bde9ec #fffefc',
      published:
        '. . . #6da4e4 #e99f61 #c1a8f4 . . . . #d5784c #60c6ed #f0c572 #d6c4ff . #93bdc0 .',
    },
    protanopia: {
      fitted:
        '. #393531 #d87a4f #6acbf6 #f7ad6e #9d91dd #bd8aba #a0bcba #ccb299 #aa9885 #ff9c87 #bddaff #ffd78c #c7acf9 #deaad1 #bbe7ea #fffefc',
      published:
        '. . #d17b40 #84b9fb #efa667 #9185d0 . . . . #d87262 #73d9ff #f7cc79 #b79ce8 #d5a0c7 #8fb9bc .',
    },
    tritanopia: {
      fitted:
        '. #393531 #be8283 #b7dab8 #eda365 #cc85c5 #d9899c #a0bcba #ccb299 #aa9885 #fe9d8a #cbffdb #ffd482 #deb3ff #faaed5 #b3dfe2 #fffefc',
      published:
        '. . #bb7f81 . #e39a5c #c27cbb #d08193 . . . #d37665 . #f2c774 #dbadfe #efa3c6 . .',
    },
  },
  'melange-light': {
    deuteranopia: {
      fitted:
        '. #dfd7d2 #e47d6c #3073ab #b85a00 #9185d0 . #608383 . . #9e2c00 #002950 #835900 #49418d #803271 . #3e2e26',
      published: '. . #e47d6d #4082bb . #9a8eda . . . . #b62600 #004569 . #564e9b . . .',
    },
    protanopia: {
      fitted:
        '. #dfd7d2 #de8155 #003963 #aa5300 #9186d1 . #608383 . . #9c1b00 #001d41 #765000 #6557a5 . . #3e2e26',
      published: '. . . #3264a0 . #9186d1 . . . . #c71b02 #003562 . #473e8a . . .',
    },
    tritanopia: {
      fitted:
        '. #dfd7d2 #d18393 #2c5631 #bd5f0c #a763a1 #c76c91 #608383 . . #c00222 #003218 #875d08 #7c3b78 #751642 . #3e2e26',
      published: '. . #d28595 . #bd5d03 #a763a1 #c86e93 . . . . . . #581955 #983b65 . .',
    },
  },
  'modus-vivendi': {
    deuteranopia: {
      fitted:
        '#e4e4e4 #1e1e1e #f26500 #01bcff #deca28 #878af7 #e797bb #00c4c1 #b1b1b1 #959595 #fb976c #99e2ff #ffda8e #bfa4f1 #c9bbff #6fe9bd .',
      published:
        '. . #fa6718 #83cfff #d6c216 #a592ff . #00cac7 . . #fb7429 #8bdeff #ffcb5c #78a7fe #b59ffe #7ff8cc .',
    },
    protanopia: {
      fitted:
        '#e4e4e4 #1e1e1e #f65e32 #93c3ff #d4c011 #9b87f5 #e797bb #00cecb #b1b1b1 #959595 #fb986a #aaffc8 #ffd172 #c0a6f4 #d6ccff #6fe9bd .',
      published:
        '. . #fd5f4c #00b7e7 . #9e8bf8 #ce80a3 #13d7d4 . . #fb7328 #90ddff . #b397f7 #cbbeff #76efc3 .',
    },
    tritanopia: {
      fitted:
        '#e4e4e4 #1e1e1e #f85954 #81f67e #cdba00 #dd88e5 #cf7f9f #00cecb #b1b1b1 #959595 #ff9380 #aaffc8 #fdcb60 #e1bbff #f2a3ce #6fe9bd .',
      published: '. . . . . #df8cea #cd7fa2 . . . . . #ffc748 #ecc4ff #ff91ad . .',
    },
  },
  'harbor-dark': {
    deuteranopia: {
      fitted:
        '#dee5eb . #e4635c #71b6ff #e1a837 #898bf4 #b484f6 #36c3cd . #8e97a2 #ff977f #caffcb #fccc5d #bec3ff #d2acfb #73eef7 .',
      published:
        '. . #ef793e #00b4e2 #fdc256 #8a8cf5 #be90ff #38c4ce . . #fc9b6f #99e0ff #ffdd95 #a8adfe . . .',
    },
    protanopia: {
      fitted:
        '#dee5eb . #e06c2f #a0cfff #f0b648 #56a4fd . . . #8e97a2 #ff9c88 #b8febb #ffdb8d #acaaf8 #d6b0ff #6be7f0 .',
      published:
        '. . #ee783d #5aa3ff #f5bb4e #cbb4ff #b080f2 . . . #fc9874 #91c1ff #ffd780 #acd7ff #cea4fb . .',
    },
    tritanopia: {
      fitted:
        '#dee5eb . #e4635c #7af185 #e1a837 #c479d5 #f777ae . . #8e97a2 #f69990 #b9ffbc #ffd883 #dba0ea #ffb5ca #6ae6ef .',
      published:
        '. . #ef6d65 . #d0971e #ca82e0 #f378b8 . . . #ffc3bd . #ffd987 #dca1ec #fb93ac . .',
    },
  },
  'iceberg-dark': {
    deuteranopia: {
      fitted:
        '#cbcdd6 #272b3c #cc6f3f #81b6f8 #e0a276 #938fdb . #8cbbc5 . #9196b0 #ffab66 #f8ffd8 #ffc69d #c3c5ff #c0b3e7 #afe0ea #e6e9f3',
      published:
        '. . #de7f53 #7cc8ff #ebad80 #8c91dc . . . . #ed8b66 #73d9ff #fec59c #8faacf . . .',
    },
    protanopia: {
      fitted:
        '#cbcdd6 #272b3c #cb6d40 #81b6f8 #e0a276 #998ed9 #a99cd1 #8cbbc5 . #9196b0 #ffab66 #f1fbbd #fdc49b #a6c3e8 #cbbef3 #acdce6 #e6e9f3',
      published:
        '. . #dc804c #7fc1fe #ebad80 #a797e4 #9d90c4 . . . #ed8a6b #dee9ab #f3bb92 #bca1ed #ac9fd1 #c4f4ff .',
    },
    tritanopia: {
      fitted:
        '#cbcdd6 #272b3c #d16869 #cfda9c #dfa175 #cc85c5 #cf8092 #8cbbc5 . #9196b0 #f99897 #f1fbbd #ffd0af #dba8f6 #f8aed7 #acdce6 #e6e9f3',
      published: '. . . #bac488 . #cb84c4 #f3a6ca . . . . #e2edaf . #c396e4 #ffbfe2 . .',
    },
  },
};

const BUILTIN_BY_ID = new Map(BUILTIN_THEMES.map((t) => [t.id, t]));
const VISION_FIT_CACHE = new Map<string, Partial<XtermPalette>>();

/** The slots Typical plays over the published palette of `theme`: its
 *  fit while Fit game colors is on (`fit`), unless the theme keeps out,
 *  else none. A color vision other than Typical swaps from them. */
export function typicalStart(theme: AppTheme, fit: boolean): Partial<XtermPalette> {
  if (!fit || theme.fitGameColors === false) return {};
  return theme.fitted ?? {};
}

/** The slots `theme` plays in for `vision`, laid over its published
 *  palette. Typical plays the theme's own fit (`fitted`). Another vision
 *  plays its swap from the palette Typical plays (typicalStart), with
 *  Fit game colors on or off (`fit`): the swap a built in theme stores
 *  (VISION_FITS), and for a custom theme the swap this window holds
 *  (holdVisionFit), or the Typical slots until one lands. */
export function visionFitOf(
  theme: AppTheme,
  vision: ColorVision,
  fit = true,
): Partial<XtermPalette> | undefined {
  if (vision === 'typical') return theme.fitted;
  const start = typicalStart(theme, fit);
  // A custom theme never holds a built in id, and the palette check
  // keeps a copy with other colors on its own swap.
  if (BUILTIN_BY_ID.get(theme.id)?.xterm !== theme.xterm) {
    return customVisionFit(theme, vision, start);
  }
  const rows = VISION_FITS[theme.id]?.[vision];
  if (rows === undefined) return start;
  const fitted = Object.keys(start).length > 0;
  const row = fitted ? (rows.fitted ?? rows.published) : rows.published;
  const key = `${vision} ${fitted ? 'fitted' : 'published'} ${theme.id}`;
  let swapped = VISION_FIT_CACHE.get(key);
  if (!swapped) {
    const slots: Partial<XtermPalette> = {};
    row.split(' ').forEach((hex, i) => {
      if (hex !== '.') slots[GAME_SLOTS[i]] = hex;
    });
    swapped = slots;
    VISION_FIT_CACHE.set(key, swapped);
  }
  return swapped;
}

/** The chrome tokens a theme paints the window with, for a player with
 *  `vision`. A color vision other than Typical swaps the status colors
 *  and what derives from them (chrome deriveChrome). */
export function themeTokens(theme: AppTheme, vision: ColorVision = 'typical'): ChromeTokens {
  return deriveChrome(theme.xterm, theme.chrome, vision);
}

/** The palette the game draws in while you play. Under Typical, with
 *  Fit game colors on (`fit`), it is the published palette with the
 *  theme's fit laid over it, unless the theme keeps out, and else the
 *  published palette. A color vision other than Typical lays its swap
 *  over the published palette whether Fit game colors is on or off
 *  (visionFitOf). The window tokens, Settings and log exports read the
 *  published palette, theme.xterm. */
export function playPalette(
  theme: AppTheme,
  fit: boolean,
  vision: ColorVision = 'typical',
): XtermPalette {
  if (vision === 'typical' && (!fit || theme.fitGameColors === false)) return theme.xterm;
  const fitted = visionFitOf(theme, vision, fit);
  if (!fitted || Object.keys(fitted).length === 0) return theme.xterm;
  return { ...theme.xterm, ...fitted };
}

// User-authored themes, set by the Settings UI on load. Merged into
// THEMES via a Proxy so callers that iterate THEMES (the picker
// dropdown, findTheme) see custom entries without changes.
let CUSTOM_THEMES: AppTheme[] = [];

const customThemeListeners = new Set<() => void>();

/** Hear every setCustomThemes, after the new list is in place. Returns
 *  the call that stops listening. */
export function onCustomThemesChanged(listener: () => void): () => void {
  customThemeListeners.add(listener);
  return () => {
    customThemeListeners.delete(listener);
  };
}

// Swaps for a color vision the main window made this launch for custom
// themes in play, by the vision, the colors they swap and the slots
// Typical plays over them. No file holds them. theme/customThemeFits makes
// them when play asks for one this window holds none of
// (onMissingVisionFit).
const HELD_VISION_FITS = new Map<string, Partial<XtermPalette>>();
type AskSwap = (theme: AppTheme, vision: ColorVision, start: Partial<XtermPalette>) => void;
let askVisionFit: AskSwap | undefined;

const swapKey = (palette: XtermPalette, vision: ColorVision, start: Partial<XtermPalette>) =>
  `${vision} ${fitKey(palette)} from ${fitKey({ ...palette, ...start })}`;

function customVisionFit(
  theme: AppTheme,
  vision: ColorVision,
  start: Partial<XtermPalette>,
): Partial<XtermPalette> {
  const held = HELD_VISION_FITS.get(swapKey(theme.xterm, vision, start));
  if (held) return held;
  askVisionFit?.(theme, vision, start);
  return start;
}

/** Hand play a way to swap a custom theme for a vision this window holds
 *  no swap of, from `start`, the slots Typical plays. The main window
 *  sets it (theme/customThemeFits). Elsewhere play keeps the Typical
 *  slots. */
export function onMissingVisionFit(ask: AskSwap): void {
  askVisionFit = ask;
}

/** Hold `swapped` in memory as the swap for `vision`, from `start`, of
 *  the custom themes with the colors of `palette`, and tell every
 *  listener. Each such theme comes back as a new object, so a view that
 *  keeps the theme it drew draws again. */
export function holdVisionFit(
  palette: XtermPalette,
  vision: ColorVision,
  start: Partial<XtermPalette>,
  swapped: Partial<XtermPalette>,
): void {
  const key = fitKey(palette);
  HELD_VISION_FITS.set(swapKey(palette, vision, start), swapped);
  setCustomThemes(CUSTOM_THEMES.map((t) => (fitKey(t.xterm) === key ? { ...t } : t)));
}

// Fits the main window made this launch for custom themes in play that
// keep none, by the colors they fit (gameFit fitKey). theme/customThemeFits
// makes them, and no file holds them. Each list set later, a broadcast
// from Settings included, lays a held fit on the theme with its colors
// that keeps none, so the fit stays until Settings keeps one.
const HELD_FITS = new Map<string, Partial<XtermPalette>>();

function withHeldFit(theme: AppTheme): AppTheme {
  if (theme.fitted) return theme;
  const fitted = HELD_FITS.get(fitKey(theme.xterm));
  return fitted ? { ...theme, fitted } : theme;
}

/** Replace the registered custom themes. The settings save path
 *  calls this whenever the user-authored list changes; subsequent
 *  iterations of THEMES include the new entries. A theme that keeps no
 *  fit takes the one this window holds for its colors (holdFit). */
export function setCustomThemes(themes: AppTheme[]): void {
  CUSTOM_THEMES = themes.map(withHeldFit);
  for (const listener of customThemeListeners) {
    try {
      listener();
    } catch {
      // The new list is in place whatever a listener does with it.
    }
  }
}

/** The custom themes registered now. */
export function customThemes(): readonly AppTheme[] {
  return CUSTOM_THEMES;
}

/** Hold `fitted` in memory for the custom themes with the colors of
 *  `palette` that keep no fit, and lay it on the ones registered now. */
export function holdFit(palette: XtermPalette, fitted: Partial<XtermPalette>): void {
  HELD_FITS.set(fitKey(palette), fitted);
  setCustomThemes(CUSTOM_THEMES);
}

// Slots of the hand-authored chrome palette custom themes carried
// before chrome was derived. A chrome map holding any of them is the
// legacy shape.
const LEGACY_CHROME_KEYS = new Set([
  'surfaceDeep',
  'surface',
  'surfacePane',
  'surfaceLift',
  'surfaceEmphasis',
  'textStrong',
  'textMuted',
  'textFaint',
  'textDim',
  'borderSoft',
  'border',
  'borderStrong',
  'borderHover',
  'accentSoft',
  'info',
]);

const OVERRIDE_KEYS = new Set<string>(CHROME_COLOR_KEYS);

/** Read a custom theme's on-disk chrome map as token overrides.
 *
 *  Legacy maps hold a full 20 slot palette forked from a built-in, so
 *  every slot would pin a token and nothing would derive. Only the
 *  accent carries over from them, since it is the one slot people
 *  chose on purpose. Status colors now follow the ANSI slots. The
 *  current shape holds token names and passes through, minus unknown
 *  keys and empty values. The result is what the editor writes back,
 *  so a legacy map converts on its first edit and the file on disk
 *  stays readable by older builds until then. */
export function migrateCustomChrome(chrome: Record<string, string> | undefined): ChromeOverrides {
  const src = chrome ?? {};
  const legacy = Object.keys(src).some((k) => LEGACY_CHROME_KEYS.has(k));
  const out: Record<string, string> = {};
  for (const [key, value] of Object.entries(src)) {
    if (typeof value !== 'string' || value.trim() === '') continue;
    if (legacy ? key === 'accent' : OVERRIDE_KEYS.has(key)) out[key] = value;
  }
  if (!legacy && (src.appearance === 'dark' || src.appearance === 'light')) {
    out.appearance = src.appearance;
  }
  return out as ChromeOverrides;
}

/** The name a custom theme shows under. You can clear the Name field,
 *  and a blank name would leave a gallery radio and a select option
 *  with nothing to read, so the theme then shows under its id. */
export function customThemeLabel(custom: { id: string; label: string }): string {
  const label = custom.label.trim();
  return label === '' ? custom.id : label;
}

/** Convert a CustomTheme record (the on-disk shape with bare maps)
 *  into a full AppTheme. The xterm map overlays a Kanso Zen base so
 *  missing slots fall back to a sensible default rather than rendering
 *  as undefined, and the chrome map becomes token overrides. The fit
 *  Settings kept carries over. The label is never blank
 *  (customThemeLabel). */
export function customToAppTheme(custom: {
  id: string;
  label: string;
  description: string;
  xterm: Record<string, string>;
  chrome: Record<string, string>;
  fitted?: Record<string, string>;
}): AppTheme {
  return {
    id: custom.id,
    label: customThemeLabel(custom),
    description: custom.description,
    xterm: { ...kansoZen.xterm, ...(custom.xterm as Partial<XtermPalette>) },
    ...(custom.fitted && { fitted: { ...(custom.fitted as Partial<XtermPalette>) } }),
    chrome: migrateCustomChrome(custom.chrome),
  };
}

/** Live, read-through list of every theme available to the user
 *  (built-in + custom). Iteration order is built-ins first, custom
 *  appended. Re-evaluates on every iteration so changes via
 *  setCustomThemes are reflected without subscribing. */
export const THEMES: AppTheme[] = new Proxy([] as AppTheme[], {
  get(_target, prop, receiver) {
    const merged = [...BUILTIN_THEMES, ...CUSTOM_THEMES];
    const value = Reflect.get(merged, prop, receiver);
    return typeof value === 'function' ? value.bind(merged) : value;
  },
  has(_target, prop) {
    const merged = [...BUILTIN_THEMES, ...CUSTOM_THEMES];
    return Reflect.has(merged, prop);
  },
  ownKeys() {
    const merged = [...BUILTIN_THEMES, ...CUSTOM_THEMES];
    return Reflect.ownKeys(merged);
  },
  getOwnPropertyDescriptor(_target, prop) {
    const merged = [...BUILTIN_THEMES, ...CUSTOM_THEMES];
    return Reflect.getOwnPropertyDescriptor(merged, prop);
  },
});

/** The theme a config without a theme key reads, as Rust reads it
 *  (default_theme in profile/ui.rs), and the theme Vosh falls back to.
 *  It is not the theme a new install starts on. A new install starts on
 *  Triad, which NEW_INSTALL_THEME in profile/set.rs writes to the first
 *  config (Themes review Q3). */
export const DEFAULT_THEME_ID = 'obsidian-ember';

/** Themes Vosh no longer ships, each by the id of the theme that took
 *  its place (Themes review Q13, Q14 and Q16). A saved pick keeps the
 *  retired id until you pick another theme, so an older build that still
 *  ships the theme reads it as it was, and this build shows the
 *  successor. */
export const RETIRED_THEMES: ReadonlyMap<string, string> = new Map([
  ['one-dark', 'one-half-dark'],
  ['vellum', 'rubric'],
  ['everforest-light', 'melange-light'],
]);

/** The theme in `themes` that `id` shows: the one with that id, a custom
 *  theme included, else the successor of a retired id. */
export function themeShownBy(themes: readonly AppTheme[], id: string): AppTheme | undefined {
  return themes.find((t) => t.id === id) ?? themes.find((t) => t.id === RETIRED_THEMES.get(id));
}

/** The theme `id` shows (themeShownBy), else Obsidian Ember. */
export function findTheme(id: string | undefined): AppTheme {
  const all = [...BUILTIN_THEMES, ...CUSTOM_THEMES];
  return (id !== undefined && themeShownBy(all, id)) || all[0];
}

/** Resolve the tri-state tint setting: an explicit user choice wins;
 *  unset is on for every theme. The chrome derives its status colors
 *  from the theme's ANSI slots, so output painted in the same slots
 *  keeps the MUD's red and the chrome's red in agreement. */
export function resolveThemeTerminalColors(stored: boolean | null): boolean {
  return stored ?? true;
}

/** The light theme a profile that never chose one is saved with, as
 *  Rust saves it (default_light_theme in profile/ui.rs). Vosh retired
 *  Vellum for Rubric (Themes review Q14), so the id shows Rubric
 *  (RETIRED_THEMES), and Vosh 0.8.1 still reads it as Vellum. A new
 *  install starts with Rubric itself, which NEW_INSTALL_LIGHT_THEME in
 *  profile/set.rs writes (Q4). */
export const DEFAULT_LIGHT_THEME_ID = 'vellum';

/** The dark theme a profile that never saved one starts with: its
 *  current theme when the theme it shows is dark, else Obsidian Ember. */
export function seedDarkTheme(theme: string, customThemes: CustomTheme[]): string {
  const found = themeShownBy([...BUILTIN_THEMES, ...customThemes.map(customToAppTheme)], theme);
  return found && themeTokens(found).appearance === 'dark' ? theme : DEFAULT_THEME_ID;
}

/** Move every custom theme whose id a built-in theme now has to a free
 *  id of its own, and point the theme choices that named it there. A
 *  theme you imported before Vosh shipped one under the same id (a
 *  Solarized Light file reads as solarized-light) would otherwise hide
 *  behind the built-in. findTheme returns the built-in, the gallery
 *  shows two tiles under one id, and your edits never reach the screen.
 *  Until a built-in took the id, a choice that named it meant the custom
 *  theme, the first one when two shared it. Returns `cfg` itself when no
 *  id collides. */
export function freeBuiltinThemeIds(cfg: RawUiConfig): RawUiConfig {
  const customs = Array.isArray(cfg.custom_themes) ? cfg.custom_themes : [];
  const builtinIds = new Set(BUILTIN_THEMES.map((t) => t.id));
  if (!customs.some((t) => builtinIds.has(t.id))) return cfg;
  const taken = new Set([...builtinIds, ...customs.map((t) => t.id)]);
  const moved = new Map<string, string>();
  const custom_themes = customs.map((t) => {
    if (!builtinIds.has(t.id)) return t;
    const id = uniqueThemeId(t.id, taken);
    taken.add(id);
    if (!moved.has(t.id)) moved.set(t.id, id);
    return { ...t, id };
  });
  const out: RawUiConfig = { ...cfg, custom_themes };
  for (const key of ['theme', 'light_theme', 'dark_theme', 'day_theme', 'night_theme'] as const) {
    const id = cfg[key];
    const to = typeof id === 'string' ? moved.get(id) : undefined;
    if (to !== undefined) out[key] = to;
  }
  return out;
}

// Theme palettes. A theme is a terminal palette (the 16 ANSI slots plus
// surfaces) and nothing else by default: lib/chrome derives the window
// chrome from it, so adding a scheme is one xterm block. A theme may pin
// individual chrome tokens where the derivation misses a look the theme
// is known for (Nord's frost accent, for example).

import { CHROME_COLOR_KEYS, deriveChrome, type ChromeOverrides, type ChromeTokens } from './chrome';
import { fitKey } from './gameFit';

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
  /// fit (lib/gameFit) moves off the published palette, from body text
  /// and the 16 ANSI colors. A built in theme stores them computed
  /// ahead, since a fit takes about two seconds, and gameFit.test.ts
  /// fits each again with VOSH_FIT_THEMES=1. A custom theme keeps the
  /// fit Settings made when you imported or changed it.
  fitted?: Partial<XtermPalette>;
  /// False keeps the published palette in play with Fit game colors on.
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
// Re-thought from the original WCAG-AA stab: an off-black ground (so
// it isn't a flat black void) and pure white text. Its yellow cursor
// sits too near the warn tone, so the window rule takes its magenta as
// the accent.
const highContrast: AppTheme = {
  id: 'high-contrast',
  label: 'High Contrast',
  description: 'Maximum readability. White text on near black, magenta accent.',
  source: 'Vosh',
  author: 'James Wright',
  license: 'GPL-3.0',
  xterm: {
    // Slight off-black instead of pure #000000. xterm.js can't be
    // told to override the 256-color cube; ANSI 256 codes like 022
    // (rgb 0,95,0) emit at their standard cube position, which is
    // invisible on pure black. A small lift means dark cube entries
    // are still readable while contrast stays high.
    background: '#0d0d0d',
    foreground: '#ffffff',
    cursor: '#ffff00',
    cursorAccent: '#000000',
    selectionBackground: '#666600',
    selectionForeground: '#ffffff',
    black: '#000000',
    red: '#ff5555',
    green: '#55ff55',
    yellow: '#ffff55',
    blue: '#55aaff',
    magenta: '#ff55ff',
    cyan: '#55ffff',
    white: '#cccccc',
    brightBlack: '#888888',
    brightRed: '#ff8888',
    brightGreen: '#88ff88',
    brightYellow: '#ffff88',
    brightBlue: '#88bbff',
    brightMagenta: '#ff88ff',
    brightCyan: '#88ffff',
    brightWhite: '#ffffff',
  },
  // Bright white has no room above body text, so the fit lowers body
  // text to #e4e4e4 in play (Q19).
  fitted: {
    foreground: '#e4e4e4',
    black: '#242424',
    red: '#fb5252',
    green: '#1fdc29',
    yellow: '#f4f447',
    cyan: '#44f3f3',
    brightBlack: '#969696',
    brightRed: '#ff9291',
    brightYellow: '#feffb5',
    brightBlue: '#97c3ff',
    brightMagenta: '#ff8dff',
    brightCyan: '#b9fffe',
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
];

/** The chrome tokens a theme paints the window with. */
export function themeTokens(theme: AppTheme): ChromeTokens {
  return deriveChrome(theme.xterm, theme.chrome);
}

/** The palette the game draws in while you play. With Fit game colors on
 *  (`fit`) it is the published palette with the theme's fitted slots laid
 *  over it, unless the theme keeps out. Off, or for a theme with nothing
 *  fitted, it is the published palette. The window tokens, Settings and
 *  log exports read the published palette, theme.xterm. */
export function playPalette(theme: AppTheme, fit: boolean): XtermPalette {
  if (!fit || theme.fitGameColors === false || !theme.fitted) return theme.xterm;
  return { ...theme.xterm, ...theme.fitted };
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

// Fits the main window made this launch for custom themes in play that
// keep none, by the colors they fit (gameFit fitKey). lib/customThemeFits
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

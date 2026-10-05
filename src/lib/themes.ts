// Theme palettes. A theme is a terminal palette (the 16 ANSI slots plus
// surfaces) and nothing else by default: lib/chrome derives the window
// chrome from it, so adding a scheme is one xterm block. A theme may pin
// individual chrome tokens where the derivation misses a look the theme
// is known for (Nord's frost accent, for example).

import { CHROME_COLOR_KEYS, deriveChrome, type ChromeOverrides, type ChromeTokens } from './chrome';
import { fitKey, GAME_SLOTS, type ColorVision } from './gameFit';

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
// Typical, worked out ahead like `fitted` (lib/gameFit swapFor). Each
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
        '#cecbc9 . #d26e5c #7ab6f5 #d2af64 #8a8fd9 #b08ac5 #79c3cf . #99948f #f39e5d #87dbff #f2cf8a #b8beff #d0aae2 #a6ecf7 .',
      published:
        '. . #da7c51 #80b4f6 #e1be73 #8c8ed9 . #89d3df . . #ee8f63 #80c7ff #f2cf8b #c3c3ff #cca7de #9fe5f0 .',
    },
    protanopia: {
      fitted:
        '#cecbc9 . #d0714d #85cdff #d2af64 #8e93de #af89c4 #73bdc9 . #99948f #f9996d #b9eaff #f2cf8a #aeb2ff #d0aae2 #9de3ee .',
      published:
        '. . #db7b57 #77c1fa #d9b66b #9c98e4 #ad87c2 . . . #ee8f63 #9fc9ff . #d1ceff #c9a4db #a0e6f1 .',
    },
    tritanopia: {
      fitted:
        '#cecbc9 . #cf7065 #95e4b0 #d2af64 #da92d3 #cc819e #79c3cf . #99948f #f59989 #c7ffd8 #f2cf8a #feb6fb #e9a1c9 #9de3ee .',
      published: '. . . . . #f2a9eb #ce86ab . . . . . . #ffbeff #e79dc3 . .',
    },
  },
  triad: {
    deuteranopia: {
      published:
        '. . #f96e1b #76b6fc . #948dd9 . . . . #f5a15e #7cdbff . #b6bcff #e1b3e7 #9febff .',
    },
    protanopia: {
      published:
        '. . #f76a1c #7ab5fd . #a08dd9 . #9cb7b9 . . #ff9e71 #9bd8ff . #c8c7ff #e0b2e6 #9febff .',
    },
    tritanopia: { published: '. . . . . #df97d8 #d185a3 . . . #fc988b . . #feb8ff #fcaabd . .' },
  },
  rubric: {
    deuteranopia: {
      published: '. . #dc5b00 #174983 . #7f7cd4 . #346084 . . #862d00 #002e5f . #654cb7 . . .',
    },
    protanopia: {
      published:
        '. . #dc5b00 #184983 #573c00 #8273ca #b45987 #305c7f . . #993d00 #002d5f . #684fba #802167 #003346 .',
    },
    tritanopia: { published: '. . . . . #b167ac #b2557d . . . . . . #783597 #962d65 . .' },
  },
  'kanso-zen': {
    deuteranopia: {
      fitted:
        '#c9cdcb #656565 #d67f46 #79baf7 #d0be95 #8b90db . #a0b6b4 #b7bab7 #92979d #fc9b6a #dfeeff #ffe8bf #c9bdff #bab1d1 #afded4 #f0f5f2',
      published: '. . #cd7146 #75ceff . #9e93df . . . . #e77841 #76d9ff #e7c485 #9fa5f2 . . .',
    },
    protanopia: {
      fitted:
        '#c9cdcb #656565 #da7d51 #70d2fd #d0be95 #978cd7 . #a0b6b4 #b7bab7 #92979d #ff9c6f #cff1ff #ffe8bf #a9affc #bab1d1 #acdbd1 #f0f5f2',
      published: '. . #cd7146 #74a7e8 . #a189d3 . . . . #e77841 #7bb1f1 . #d2bbff . . .',
    },
    tritanopia: {
      fitted:
        '#c9cdcb #656565 #d17f79 #d4e5c4 #d0be95 #e098d9 #ce88b0 #a0b6b4 #b7bab7 #92979d #ff919a #e9ffe9 #ffe8bf #fcc2ff #f5a6c4 #acdbd1 #f0f5f2',
      published: '. . #c3736d . #eddbb1 #e09ce0 #c882aa . . . . . #ffefd3 #f0a7e9 #c07693 . .',
    },
  },
  'tokyo-night': {
    deuteranopia: {
      fitted:
        '#bdc7f2 #2c2e36 #e6733a #43bfff #e7b66f #8b90ea #b393ef #76c8f8 #aab3d8 #8e97bb #f7a161 #78d9ff #ffd9a4 #c0b6ff #fdfcff #b3e2ff #dbe2ff',
      published:
        '. . #ea773e #44bfff . #8b90ea #b393ef #c2e7ff . . #f58148 #44bfff #e1b069 #9699f5 #c4a5ff #c2e7ff .',
    },
    protanopia: {
      fitted:
        '#bdc7f2 #2c2e36 #e6733a #6eb9ff #eab871 #9c8de9 . #78cafa #aab3d8 #8e97bb #fd9c70 #71d8ff #ffdba9 #cdc3ff #fdfcff #b7e3ff #dbe2ff',
      published:
        '. . #f0745b #6eb9ff #e2b16a #9c8de9 . #c2e7ff . . #f58148 #6eb9ff #e2b16a #9699f5 #be9dfb #c2e7ff .',
    },
    tritanopia: {
      fitted:
        '#bdc7f2 #2c2e36 #e86982 #bbed87 . #e498e3 #ef88b3 #70c2f2 #aab3d8 #8e97bb #ff94a5 #e0ffc4 #ffd08d #ffc0ff #ffb1c3 #a8deff #dbe2ff',
      published:
        '. . #f6758d #bbed87 #dfae67 #f8a9f4 #ef89b6 . . . . #bbed87 #dfae67 #edaafc #ef89b8 . .',
    },
  },
  nord: {
    deuteranopia: {
      fitted:
        '. #3c4353 #c3683c #85bbfd #e9c989 #a995e1 #bb95b4 #91cada . #95a1b7 #f9a761 #90e0ff #ffeac1 #d0b9ff #dcb4d4 #c0eeed #feffff',
      published:
        '. . #bf6439 #9cc7ff . #9c93df . #87bfce . . #c0653a #7ecaff . #9994e0 #b38dac . .',
    },
    protanopia: {
      fitted:
        '. #3c4353 #c86551 #62b3e8 #e9c989 #ac94df #bb95b4 #8cc5d5 . #95a1b7 #ffa26e #99cfff #ffeac1 #c4beff #deb6d6 #b6e4e3 #feffff',
      published: '. . #bf6439 #88bbfe . #9c93df . #87bfce . . #c0653a #6fbef4 . #9894e0 . . .',
    },
    tritanopia: {
      fitted:
        '. #3c4353 #c3656e #96b07f #e9c989 #e39bdc #d68aa8 #8cc5d5 . #95a1b7 #fd9da4 #ceeab6 #ffeac1 #ffbefd #ffacc0 #b6e4e3 #feffff',
      published: '. . . . . #eda4e6 #cd819f . . . . . . #e0a4ee #cd819f . .',
    },
  },
  'rose-pine': {
    deuteranopia: {
      fitted:
        '. #2b2940 #e07d1c #83b6f9 #f5c177 #9b90dc . . #dedcf2 #9894b1 #f5a35c #b4d5ff #ffe3be #ceb5ff #e1caff #ffe1e0 #ffffff',
      published:
        '. . #e97840 #5b9bd6 #fcc77c #c4bbff #bfa2e2 . . . #eb7942 #4b9bcf #fcc77c #d3bcff . . .',
    },
    protanopia: {
      fitted:
        '. #2b2940 #ec7354 #95c3ff #f8c479 #a58bd6 #c7aaea . #dedcf2 #9894b1 #ffa074 #abe7ff #ffe6c5 #c5aaf7 #e4d0ff #ffe1e0 #ffffff',
      published:
        '. . #eb734f #6398d7 #fcc77c #c4bbff #b79ad9 . . . #e97a3c #4b9bcf #fcc77c #bdc0ff . . .',
    },
    tritanopia: {
      fitted:
        '. #2b2940 . #79bcd9 #f1bd73 #bc82c9 #e194b2 . #dedcf2 #9894b1 #ff98b2 #9addfb #ffdfb4 #d1a4f3 #ffb8ca #ffe1e0 #ffffff',
      published: '. . . . . #ca8fd7 #e99bb9 . . . . . . #cfa1f1 #ee9caf #f1c2c0 .',
    },
  },
  gruvbox: {
    deuteranopia: {
      fitted:
        '#e4d4ac #383838 #cf5500 #88bdff . #9195e1 #d582a7 #8ec590 #c5b59f #a8998a #f59b60 #95e1ff . #e9deff #f4a4b9 #b2e6a0 #ffefc5',
      published: '. . #b64a00 #579fec . #7c70b9 . . . . #eb6200 #00c5f8 . #c2b9ff . . .',
    },
    protanopia: {
      fitted:
        '#e4d4ac #383838 #dc4300 #85baff #d99b24 #9d92de #d582a7 #8ec590 #c5b59f #a8998a #fb9a6d #99e2ff #fcbf32 #d9c6ff #f4a4b9 #b2e6a0 #ffefc5',
      published:
        '. . #c33300 #3499dd #d99b24 #7c70b9 . . . . #eb6200 #61d0ff #fcbf32 #bbb0fe . . .',
    },
    tritanopia: {
      fitted:
        '#e4d4ac #383838 #e03c30 #cdce5d . #bf87d0 #d482a8 #8ec590 #c5b59f #a8998a #fd9381 #ecf068 . #e9a6eb #f5a3b5 #b2e6a0 #ffefc5',
      published: '. . . #9b9a20 #da9c26 #a5619f #bf6b88 . . . . . . #daa7f6 #dc8da5 . .',
    },
  },
  catppuccin: {
    deuteranopia: {
      fitted:
        '. . #d4774c #81b9fa #f1dba8 #8f92de #d4a2c7 . . #9498af #f4a25c #88deff #fff1d2 #d0b7ff . #c4fff4 #edf1ff',
      published: '. . #f19264 #88bcff . #8f92de . . . . #f39467 #88bcff . #bca5f2 . . .',
    },
    protanopia: {
      fitted:
        '. . #d2764a #81b8f9 #f1dba8 #9b90dc #d4a2c7 . . #9498af #fe9e71 #99e2ff #fff1d2 #cccfff . #c4fff4 #edf1ff',
      published:
        '. . #ef8f63 #7ec1fd . #a697e5 #ffe7f8 . . . #f39467 #7ec1fd . #bea4f2 #ffe7f8 . .',
    },
    tritanopia: {
      fitted:
        '. . #d5708d #89c484 #f1dba8 #c781c1 #e799b7 . . #9498af #fe95b2 #adeba8 #fff1d2 #d3a4f3 #ffc7de #c4fff4 #edf1ff',
      published: '. . . . . #c781c1 #ffbfd7 . . . . . . #d89ce5 #ffc3e3 . .',
    },
  },
  dracula: {
    deuteranopia: {
      fitted:
        '#e4e4df #2c2e34 #e55f00 #00c0ff #e2eb7d #8b8ef5 #f36ebb #81dff3 #deded9 #8597cb #f3a15b #90e0ff #feffc9 #c3b5ff #fc9bdf #aeffff .',
      published: '. . #f66600 #1fc0ff . #8b8ef5 . . . . #fa7834 #13cbff . #d5bfff #ff93df . .',
    },
    protanopia: {
      fitted:
        '#e4e4df #2c2e34 #ed4f00 #00caff #e2eb7d #918cf4 #f36ebb #81dff3 #deded9 #8597cb #fc9b6e #a2cfff #feffc9 #c1a6fa #fc9bdf #aeffff .',
      published:
        '. . #fc5c1c #82dcff #f3fc8e #ab9aff . #62c1d4 . . #fa7834 #97e0ff . #c0b0ff . . .',
    },
    tritanopia: {
      fitted:
        '#e4e4df #2c2e34 #f2494b #07d558 #e2eb7d #ec92e5 #fb6ead #81dff3 #deded9 #8597cb #ff9692 . #feffc9 #ffbbfc #ff9ecd #aeffff .',
      published: '. . . . . #ffadf7 #ff7db5 . . . . . . #ffc9fd #ff99c2 . .',
    },
  },
  monokai: {
    deuteranopia: {
      fitted:
        '#e4e4df #363831 #fc7311 #1fc9ff #f1bd73 #ba9feb #af84ff #89d6cc #deded9 #a09c87 #f8a55f #96e1ff #ffdfb5 #d1c4ff #cab1ff #a9f8ec #fffffd',
      published:
        '. . #e15d00 #0cc5ff . #d2c3ff . #7ac7bd . . #e35e00 #00caff . #d2c3ff #ac7ffd #7ac7bd .',
    },
    protanopia: {
      fitted:
        '#e4e4df #363831 #ff6c56 #36c3ff #f6c278 #a2a8f4 #af84ff #89d6cc #deded9 #a09c87 #fda16b #9dd2ff #ffe3bf #d1c4ff #c5aaff #a9f8ec #fffffd',
      published:
        '. . #fc3100 #95e1ff #f8c379 #c1c5ff #ac7ffd #7dcabf . . #e25e00 #8ddfff #f8c379 #d1b9ff . #7dcabf .',
    },
    tritanopia: {
      fitted:
        '#e4e4df #363831 #ff648c #bbf94d #edb96f #cc91da #f173bf #89d6cc #deded9 #a09c87 #ffb5c2 #d8ffa4 #ffdbac #e7b3ff #fd9ab2 #a9f8ec #fffffd',
      published: '. . . . . #e19bdf #f167a4 . . . . . . #e1b0ff #f167a4 . .',
    },
  },
  'one-half-dark': {
    deuteranopia: {
      fitted:
        '. #373c44 #df7f38 #7bbefb #e0bc77 #9395e9 #cd7ee4 #74d3df #dbdee3 #939eb2 #f9a760 #92e0ff #ffdd9e #bfc3ff #ecaeff #a3f5ff .',
      published:
        '. . #dc7442 #84bafc #e6c17c #d2bdff . #55b5c1 . . #dd7543 #6dbdf3 #e6c17c #d2bdff #c476db #55b5c1 .',
    },
    protanopia: {
      fitted:
        '. #373c44 #e57852 #70c2f8 #e2be79 #a5a3f8 #cd7ee4 #65c4d0 #dbdee3 #939eb2 #fca566 #a2caff #ffdfa5 #d4c8ff #e9a2ff #87e6f2 .',
      published:
        '. . #da7240 #7bceff #e7c27d #a3a4f9 #c577dc . . . #dd7543 #7dc9ff #e7c27d #a0a5f9 #c577dc . .',
    },
    tritanopia: {
      fitted:
        '. #373c44 #e7727b #c6f3a6 #e0bc77 #c88ad2 #ec77be #66c5d1 #dbdee3 #939eb2 #ffb7b9 #e3ffd1 #ffdd9e #dbadfd #ff9cb3 #87e6f2 .',
      published: '. . #de6a74 #9bc67c . #cc85c5 #ec76b8 . . . #ff949a . . #ce92db #ef71a5 . .',
    },
  },
  'solarized-dark': {
    deuteranopia: {
      published: '. . #c85200 #62a7ff #b58901 #7a7ad8 . . . . #b66100 #58c4ff . #d0b8ff . . .',
    },
    protanopia: {
      published:
        '. . #db3421 #1ca5ee #b58901 #8879d7 #d23581 . . . #c65100 #81daff . #ccbbff . . .',
    },
    tritanopia: {
      published:
        '. . . #849800 #b58901 #dd8ddb #d23583 . . . . #a1b42c #d4a42b #eda8fd #cc6e95 . .',
    },
  },
  'solarized-light': {
    deuteranopia: {
      fitted:
        '#42575f . #f6802f #32639f #946f00 #8574d3 #e3468f #0a9189 . . #c65100 #00376d #715400 #443888 #7075c8 #004c48 .',
      published:
        '. . #c85200 #0099e0 #b58901 #715fbb . . . . #b66100 #0065a0 #936e00 #503e90 #6367b9 . .',
    },
    protanopia: {
      fitted:
        '#42575f . #f97d3f #447cb8 #8f6b00 #9c8ced #db3e88 #0a9189 . . #c65100 #00386c #6c5000 #7053a5 #424391 #00706a .',
      published:
        '. . #cf3f00 #007ac0 #b78b06 #988cec . #29a198 . . #c15500 #005180 #99740b #473485 #5154a4 . .',
    },
    tritanopia: {
      fitted:
        '#42575f . #ff766a #4d5900 #946f00 #aa67b9 #e34792 #0a9189 . . . #343d00 #715400 #874d96 #b0557c #00706a .',
      published: '. . . #859901 #b68a03 #934d9a #e64a95 . . . . . #967103 #763b83 #b3587f . .',
    },
  },
  'tango-dark': {
    deuteranopia: {
      fitted:
        '#d4d8d0 #3d4345 #ed6300 #35c8ff #dcb834 #9699e8 #bc94c2 #5cd0d2 #d6dad2 #adafab #f8985c #8bdeff . #bcc2ff #e9b9e4 #58fbfa #fbfbf9',
      published: '. . #b63f00 #008dd3 . #5f59a3 . #07989a . . #d55800 #00cbff . #9d9ae6 . . .',
    },
    protanopia: {
      fitted:
        '#d4d8d0 #3d4345 #fa4d07 #00c9ff #dcb834 #a296e6 #bc94c2 #58ccce #d6dad2 #adafab #fb9566 #9bc7ff . #dbcaff #e2b2dd #4cf2f1 #fbfbf9',
      published: '. . #ca0e00 #0089e6 . #625aa4 . . . . #d55800 #00caff . #aca1ee . . .',
    },
    tritanopia: {
      fitted:
        '#d4d8d0 #3d4345 #fe4a3b #a3f476 #e2be3c #dd98dc #d88aa2 #58ccce #d6dad2 #adafab #ff8f82 #caffa6 . #feb8ff #fca9bb #4cf2f1 #fbfbf9',
      published: '. . . . . #9a5896 #8b4663 . . . . . . #ce93db #c17694 . .',
    },
  },
  'classic-vivid': {
    deuteranopia: {
      fitted:
        '. #232323 #ed5b2c #1abcff #ffc6a2 #7876fc #e756e4 #0badac #b2b2b2 #959595 #fb8b57 #94d5ff . #b1b6ff #ff9cfb #71fffe .',
      published:
        '. . #983300 #0099ef #b05a0c #3d008d . . . . #e35200 #48d1ff . #7b46f5 #ff56fd . .',
    },
    protanopia: {
      fitted:
        '. #232323 #ef583d #00bdfc #ffe0cd #8b7bff #e756e4 #0badac #b2b2b2 #959595 #fb8b57 #99e2ff . #b49aff #ff84fc . .',
      published: '. . #a80a00 #0097ec . #3d008d . . . . #e35200 #00c1ff . #7b46f5 . . .',
    },
    tritanopia: {
      fitted:
        '. #232323 #ee5645 #4cd546 #ffc6a2 #c05ed6 #ff61a9 #0badac #b2b2b2 #959595 #ff8574 . . #d687dd #ffb0d9 . .',
      published: '. . #a80000 . . #610075 #bc006f . . . . . . #af25ca #ff49bf . .',
    },
  },
  'high-contrast': {
    deuteranopia: {
      fitted:
        '#e4e4e4 #242424 #eb6900 #03bdff #f4f447 #888bf2 #f145f1 #44f3f3 . #969696 #f69e5e #7fdaff #feffb5 #c4aaf6 #fb89fb #b9fffe .',
      published:
        '. . #f36500 #03bdff . #888bf2 #f145f1 . . . #fb905e #78b6ff . #d1b9ff #ff8aff . .',
    },
    protanopia: {
      fitted:
        '#e4e4e4 #242424 #f85336 #00bff3 #f4f447 #9398ff #f449f5 #44f3f3 . #969696 #fb9a6b #99c6ff #feffb5 #dac8ff #f887f8 #b9fffe .',
      published:
        '. . #fd5838 #46c8ff #fdffad #9999ff #f64bf6 . . . #fb905e #8dc2ff . #bca5f3 #ffdafe . .',
    },
    tritanopia: {
      fitted:
        '#e4e4e4 #242424 #fb5252 #1edc29 #f4f447 #ce84df #ff61b6 #44f3f3 . #969696 #ffacaa . #feffb5 #ffb6f8 #ff90af #b9fffe .',
      published: '. . . . . #ce84df #ff63b1 . . . #fe8787 . . #ffb8fd #ff99ba . .',
    },
  },
  'everforest-dark': {
    deuteranopia: {
      fitted:
        '#e1d4b8 . #cb7043 #65caf2 #ccae71 #979ae6 . #8cca9b . #96a3a9 #fda865 #b1e8ff #edce90 #cbd0ff #f8b9d6 #acebbb #fcefd2',
      published: '. . #e18357 #6fbef4 . #d2c3ff . . . . #e38559 #6fbef4 . #d2c3ff . . .',
    },
    protanopia: {
      fitted:
        '#e1d4b8 . #ce6c52 #8ec1ff #ccae71 #a499e6 #d194b1 #8cca9b . #96a3a9 #ffa769 #b1e8ff #edce90 #d6d6ff #f2b4d1 #acebbb #fcefd2',
      published:
        '. . #e38459 #7dbaf9 #f0d193 #a498e5 . . . . #e38559 #6fbef4 #f0d193 #ddccff . . .',
    },
    tritanopia: {
      fitted:
        '#e1d4b8 . #d06b6e #c4de9d #ccae71 #c18dd8 #e193b1 #8cca9b . #96a3a9 #ffa3a3 #e6ffc0 #f4d597 #efa6e7 #ffc1e0 #acebbb #fcefd2',
      published: '. . . . #eaca8d #c98ed7 #e293ae . . . . . #eaca8d #d499e3 #f19fb1 . .',
    },
  },
  'green-screen': {
    deuteranopia: {
      fitted:
        '#8dde93 #242424 #f66a14 #00bdfd #ffcaa9 #8b88ff #ffa4fb #25b7b6 #b2b2b2 #969696 #f3a159 #99e1ff #feff89 #b8b9ff #ffd5fd #2de6e6 .',
      published: '. . #983400 #0099f8 #b05a0c #3d008d . . . . #f66600 #43d0ff . #7b46f5 . . .',
    },
    protanopia: {
      fitted:
        '#8dde93 #242424 #fa6248 #72b6ff #ffdbc4 #9289ff #e757e5 #25b7b6 #b2b2b2 #969696 #fb996d #90e0ff . #c7c3ff #ff84fd . .',
      published: '. . #a80a00 #00acfc . #3d008d . . . . #f66600 #3acfff . #7b46f5 . . .',
    },
    tritanopia: {
      fitted:
        '#8dde93 #242424 #fa6150 #57de50 #ffcaa9 #c974dc #ff6cae #25b7b6 #b2b2b2 #969696 #ff938c #7eff7a . #ecc2ff #ff9dc6 . .',
      published: '. . #a80000 . . #610075 #bc006f . . . . . . #af25ca #ff67be . .',
    },
  },
  srcery: {
    deuteranopia: {
      fitted:
        '#e8d5b0 #282625 #ea6100 #51baff #ecaa04 #8a8de8 #ff5589 #4ed5d9 . #a6937f #fc9870 #95e1ff . #c6b6ff #ff90ac #54fee8 #fff0d3',
      published: '. . #d75900 #479ae5 . #6e69c0 . #1cb4b9 . . #ed6300 #61d5ff . #b0a5f2 . . .',
    },
    protanopia: {
      fitted:
        '#e8d5b0 #282625 #f84600 #36cafd #ecaa04 #8f8ce6 #ff5589 #36c3c7 . #a6937f #ff9483 #95e1ff . #bea6f3 #ff90ac #32e8d3 #fff0d3',
      published: '. . #ef3114 #2493d6 . #7767bf . . . . #ed6300 #65c8ff . #b4a9f7 . . .',
    },
    tritanopia: {
      fitted:
        '#e8d5b0 #282625 #fe4135 #92e28f #ecaa04 #c87bc1 #f957a3 #36c3c7 . #a6937f #ff9483 #d1f878 . #f1a8eb #ff90ac #32e8d3 #fff0d3',
      published: '. . . . . #995ba7 #da3087 . . . . . . #f9aff2 #f95ea5 . .',
    },
  },
  nightfly: {
    deuteranopia: {
      fitted:
        '#c9cdd2 . #e45f00 #34bdff #e9d68f #8a8ee9 #b984db #77d3c2 #aab4c2 #879b9b #f3a05b #8bdeff #ffe7c6 #bcc2ff #c6aefc #98f5e3 #dfe8f5',
      published:
        '. . #f06400 #00c0f2 . #8b8ee8 #bf8ae2 . . . #ff6d0e #84d7ff . #c6b3ff #ac7ffd . .',
    },
    protanopia: {
      fitted:
        '#c9cdd2 . #ea5300 #5bc3ff #ead790 #a095f1 #b681d9 #77d3c2 #aab4c2 #879b9b #fc9b6f #97e1ff #fffdfa #d6c7ff #c1a9f6 #98f5e3 #dfe8f5',
      published:
        '. . #f45e00 #05c9fe #e5d38c #998ce7 #c993ec . . . #f17100 #a1cfff #edc58e #a3a9ff #ac7ffd . .',
    },
    tritanopia: {
      fitted:
        '#c9cdd2 . #f24746 #95c051 #e9d68f #f0a0e9 #e47da7 #77d3c2 #aab4c2 #879b9b #ff939d #62f5d4 #fffdfa #ffc9ff #fda6c8 #98f5e3 #dfe8f5',
      published: '. . . . . #cb7ec4 #ed86b0 . . . . . . #d591df #ef67aa . .',
    },
  },
  'melange-dark': {
    deuteranopia: {
      fitted:
        '. #393531 #d6794d #81bcfc #f6ab6d #9194e0 #bd8aba #a0bcba #ccb299 #aa9885 #faa861 #b0e8ff #ffd482 #d0c2ff #deaad1 #b3dfe2 #fffefc',
      published:
        '. . #d57356 #69a8e5 #eda365 #b6b0fe . . . . #db814c #74c3f9 #f1c673 #d9c6ff #d39fc6 #97c1c4 .',
    },
    protanopia: {
      fitted:
        '. #393531 #d97759 #84bafc #eda365 #9d91dd #bd8aba #a0bcba #ccb299 #aa9885 #ff9f73 #8ddfff #f7cc79 #d8c7ff #deaad1 #b3dfe2 #fffefc',
      published:
        '. . #d57356 #6fa2e3 . #aeb4ff . . . . #d17d3e #84bafc . #d9c6ff #dda8d0 #96c1c4 .',
    },
    tritanopia: {
      fitted:
        '. #393531 #be8283 #b7dab8 #eda365 #cc85c5 #d9899c #a0bcba #ccb299 #aa9885 #fe9d8a #cbffdb #f7cc79 #deb3ff #f9a9c4 #b3dfe2 #fffefc',
      published: '. . #bb7f81 . . #c27cbb #d08196 . . . . . . #ddaefd #e798b3 . .',
    },
  },
  'melange-light': {
    deuteranopia: {
      fitted:
        '. #dfd7d2 #e47d6c #114b83 #b85a00 #9185d0 . #608383 . . #b62d00 #002950 #835900 #6458a7 #a45393 . #3e2e26',
      published: '. . #e47d6d #3f86bd . #9c92de . . . . #aa3d00 #006495 . #4e3e89 . . .',
    },
    protanopia: {
      fitted:
        '. #dfd7d2 #d78542 #376faa #aa5300 #9186d1 . #608383 . . #902f00 #002047 #765000 #4d3d88 . . #3e2e26',
      published:
        '. . #db7e52 #3581b6 #a95200 #6659a0 #c882c4 . . . #bc1300 #006495 #7e5500 #4b3a85 #8d3e7d . .',
    },
    tritanopia: {
      fitted:
        '. #dfd7d2 #d18393 #2c5631 #b85a00 #a763a1 #c76c91 #608383 . . #c00222 #003218 #835900 #7b3a76 #a84973 . #3e2e26',
      published: '. . #d28595 . . #a763a1 #c86e93 . . . . . . #581955 #9b3d67 . .',
    },
  },
  'modus-vivendi': {
    deuteranopia: {
      fitted:
        '#e4e4e4 #1e1e1e #f65946 #01bcff . #878af7 #e797bb #00cecb #b1b1b1 #959595 #f99b62 #73d8ff #ffcd62 #c8acfb #d3c8ff #6fe9bd .',
      published:
        '. . #ef6f00 #5fb5ff #d1bd04 #878af6 . . . . #fb7328 #2abbff . #d1b9ff #e0d9ff #66e0b5 .',
    },
    protanopia: {
      fitted:
        '#e4e4e4 #1e1e1e #f26500 #34c4ff #d9c51e #9886f4 #e797bb #00cecb #b1b1b1 #959595 #fb986a #94e0ff #ffd681 #bea4f2 #d6ccff #6fe9bd .',
      published:
        '. . #fd5f4c #58a9ff . #aa80ec . . . . #fb7328 #01afff . #c9c2ff #e2dcff #66e0b5 .',
    },
    tritanopia: {
      fitted:
        '#e4e4e4 #1e1e1e #f85954 #81f67e . #bf7ade #d383a2 #00cecb #b1b1b1 #959595 #ff9380 #aaffc8 #ffcd62 #dfb4ff #f6a2c3 #6fe9bd .',
      published: '. . . . . #d580dd #ffbccb . . . . . . #e0a9ff #fa91bc . .',
    },
  },
  'harbor-dark': {
    deuteranopia: {
      fitted:
        '#dee5eb . #e36844 #45bbff #e6ac3d #898bf4 #b484f6 . . #8e97a2 #f3a05b #96dbff #ffd168 #bcc2ff #d0aaf9 #73eef7 .',
      published:
        '. . #ff8471 #01a8ff . #aeaeff #be90ff #36c3cd . . #ffa789 #00bff8 #e9b948 #d4c4ff . #51d0d9 .',
    },
    protanopia: {
      fitted:
        '#dee5eb . #e36746 #33bcff #e6ac3d #9b88f2 #be90ff #44cdd7 . #8e97a2 #fd9c70 #85d7ff #ffd168 #bcc2ff #e1c6ff #73eef7 .',
      published:
        '. . #ed6f50 #00aedd . #9b86f0 #bd8dff #3cc7d1 . . #fe9e71 #85dbff . #aeb4ff #dabaff . .',
    },
    tritanopia: {
      fitted:
        '#dee5eb . #e4635c #7af185 #e1a837 #cb80dc #e66eb1 . . #8e97a2 #ffb5ad #b9ffbc #ffd578 #fdb9ff #f597b2 #6ae6ef .',
      published: '. . #f16e66 . . #cb82df #f278ba . . . #ffb1a9 . #e0b03d #f2b8ff #fb93ac . .',
    },
  },
  'iceberg-dark': {
    deuteranopia: {
      fitted:
        '#cbcdd6 #272b3c #cc6f3f #81b6f8 #e0a276 #938fdb . #8cbbc5 . #9196b0 #f4a15d #81dcff #fdc49b #bac0ff #c0b3e7 #acdce6 #e6e9f3',
      published:
        '. . #e17969 #76ccff #e3a579 #9398e3 #9d90c4 #88b7c1 . . #eb8c60 #76d9ff . #9ca2ee #b2a5d8 #98c7d1 .',
    },
    protanopia: {
      fitted:
        '#cbcdd6 #272b3c #cb6d40 #81b6f8 #e0a276 #938fda #a79ace #8cbbc5 . #9196b0 #fb9b6f #8adbff #fdc49b #d9c6ff #c8bbef #b2e2ec #e6e9f3',
      published:
        '. . #e17a65 #7abefa #e3a579 #8c91dc #a598cc #88b7c1 . . #eb8c60 #92c4ff . #9ea2ef #b3a6d9 #c4f4ff .',
    },
    tritanopia: {
      fitted:
        '#cbcdd6 #272b3c #d16869 #cfda9c #dfa175 #df98da #cc819e #8cbbc5 . #9196b0 #f99897 #f1fbbd #fdc49b #fdb9ff #e9a1c9 #acdce6 #e6e9f3',
      published:
        '. . . #b7c185 . #cc85c5 #cc819e . . . #e78787 . #ecb38b #c795e2 #dd95bd #93c2cc .',
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
// Typical plays over them. No file holds them. lib/customThemeFits makes
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
 *  sets it (lib/customThemeFits). Elsewhere play keeps the Typical
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

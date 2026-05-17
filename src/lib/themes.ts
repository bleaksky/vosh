// Theme palettes — drives both the chrome (sidebar, panes, room info,
// statusbar, etc. via CSS variables) and the xterm.js terminal palette
// (16 ANSI slots). Each theme is a single source of truth so adding a
// new scheme is mechanical: define one entry and the whole UI flips.

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

export interface ChromePalette {
  /// Settings sidebar / off-floor backgrounds — deepest surface tone.
  surfaceDeep: string;
  /// Primary app background — terminal host, side panel, etc.
  surface: string;
  /// Pane headers and similar slightly darker accents over surface.
  surfacePane: string;
  /// Lifted surface on hover.
  surfaceLift: string;
  /// More emphasized surface (active card, etc.).
  surfaceEmphasis: string;

  /// Brightest text — section titles, names.
  textStrong: string;
  /// Body text default.
  text: string;
  /// Muted secondary labels.
  textMuted: string;
  /// Faint tertiary text (meta, descriptions).
  textFaint: string;
  /// Dimmest readable text (eyebrows, dividers).
  textDim: string;

  /// Soft hairline dividers.
  borderSoft: string;
  /// Standard borders.
  border: string;
  /// Emphasized borders (around fields, buttons).
  borderStrong: string;
  /// Hover state for borders.
  borderHover: string;

  /// Primary accent — pink in Kanso Zen, blue in Nord, etc.
  accent: string;
  /// Translucent accent for subtle backgrounds (linear-gradient targets).
  accentSoft: string;
  /// Warning / amber tone.
  warn: string;
  /// Danger / red tone.
  danger: string;
  /// Info / blue tone.
  info: string;
  /// Success / green tone.
  success: string;
}

export interface AppTheme {
  id: string;
  label: string;
  description: string;
  xterm: XtermPalette;
  chrome: ChromePalette;
}

// ── Kanso Zen ───────────────────────────────────────────────────────
// Default. Mirrors the user's Ghostty config exactly so the in-app
// terminal renders identically to the one outside it.
const kansoZen: AppTheme = {
  id: 'kanso-zen',
  label: 'Kanso Zen',
  description: 'Calm Japanese-inspired dark. Pink accent, muted greys.',
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
  chrome: {
    surfaceDeep: '#06090d',
    surface: '#090e13',
    surfacePane: '#0d0c0c',
    surfaceLift: '#14141a',
    surfaceEmphasis: '#1d1d24',
    textStrong: '#f0f3f1',
    text: '#c5c9c7',
    textMuted: '#a4a7a4',
    textFaint: '#6e7681',
    textDim: '#4a4e57',
    borderSoft: '#161b22',
    border: '#1a1f26',
    borderStrong: '#393b44',
    borderHover: '#2a313b',
    accent: '#ff3399',
    accentSoft: 'rgba(255, 51, 153, 0.09)',
    warn: '#e6c384',
    danger: '#e46876',
    info: '#7fb4ca',
    success: '#87a987',
  },
};

// ── Tokyo Night Storm ───────────────────────────────────────────────
// Saturated blues, muted purples, signature deep navy. Accent on the
// frost blue (`#7aa2f7`).
const tokyoNight: AppTheme = {
  id: 'tokyo-night',
  label: 'Tokyo Night',
  description: 'Storm variant. Cool blues, deep navy, frosted accents.',
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
  chrome: {
    surfaceDeep: '#16161e',
    surface: '#1a1b26',
    surfacePane: '#13141c',
    surfaceLift: '#24283b',
    surfaceEmphasis: '#2f334d',
    textStrong: '#c0caf5',
    text: '#a9b1d6',
    textMuted: '#737aa2',
    textFaint: '#565f89',
    textDim: '#3b4261',
    borderSoft: '#1f2335',
    border: '#292e42',
    borderStrong: '#3b4261',
    borderHover: '#545c7e',
    accent: '#7aa2f7',
    accentSoft: 'rgba(122, 162, 247, 0.12)',
    warn: '#e0af68',
    danger: '#f7768e',
    info: '#7dcfff',
    success: '#9ece6a',
  },
};

// ── Nord ────────────────────────────────────────────────────────────
// Arctic, north-bluish dark. Frost accent (`#88c0d0`).
const nord: AppTheme = {
  id: 'nord',
  label: 'Nord',
  description: 'Arctic palette. Polar nights base, frost accents.',
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
  chrome: {
    surfaceDeep: '#242933',
    surface: '#2e3440',
    surfacePane: '#272c36',
    surfaceLift: '#3b4252',
    surfaceEmphasis: '#434c5e',
    textStrong: '#eceff4',
    text: '#d8dee9',
    textMuted: '#8a92a4',
    textFaint: '#6e7889',
    textDim: '#4c566a',
    borderSoft: '#2b303b',
    border: '#353b48',
    borderStrong: '#434c5e',
    borderHover: '#4c566a',
    accent: '#88c0d0',
    accentSoft: 'rgba(136, 192, 208, 0.12)',
    warn: '#ebcb8b',
    danger: '#bf616a',
    info: '#81a1c1',
    success: '#a3be8c',
  },
};

// ── Gruvbox Dark ────────────────────────────────────────────────────
// Warm retro tones. Earthy yellows and reds.
const gruvbox: AppTheme = {
  id: 'gruvbox',
  label: 'Gruvbox',
  description: 'Warm, retro, earthy. Yellow accent on warm dark.',
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
  chrome: {
    surfaceDeep: '#1d2021',
    surface: '#282828',
    surfacePane: '#222222',
    surfaceLift: '#3c3836',
    surfaceEmphasis: '#504945',
    textStrong: '#fbf1c7',
    text: '#ebdbb2',
    textMuted: '#bdae93',
    textFaint: '#928374',
    textDim: '#665c54',
    borderSoft: '#32302f',
    border: '#3c3836',
    borderStrong: '#504945',
    borderHover: '#665c54',
    accent: '#fabd2f',
    accentSoft: 'rgba(250, 189, 47, 0.12)',
    warn: '#fe8019',
    danger: '#fb4934',
    info: '#83a598',
    success: '#b8bb26',
  },
};

// ── Catppuccin Mocha ────────────────────────────────────────────────
// Soft pastels, warm dark base. Pink accent.
const catppuccin: AppTheme = {
  id: 'catppuccin',
  label: 'Catppuccin',
  description: 'Mocha variant. Soft pastels on a warm dark base.',
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
  chrome: {
    surfaceDeep: '#181825',
    surface: '#1e1e2e',
    surfacePane: '#11111b',
    surfaceLift: '#313244',
    surfaceEmphasis: '#45475a',
    textStrong: '#cdd6f4',
    text: '#bac2de',
    textMuted: '#a6adc8',
    textFaint: '#7f849c',
    textDim: '#585b70',
    borderSoft: '#1e1e2e',
    border: '#313244',
    borderStrong: '#45475a',
    borderHover: '#585b70',
    accent: '#f5c2e7',
    accentSoft: 'rgba(245, 194, 231, 0.12)',
    warn: '#f9e2af',
    danger: '#f38ba8',
    info: '#89b4fa',
    success: '#a6e3a1',
  },
};

// ── High Contrast ───────────────────────────────────────────────────
// Re-thought from the original WCAG-AA stab: deeper blacks for chrome
// (so it isn't a flat black void), pure white text, yellow + bright
// cyan accents for emphasis. Borders are bright white so structure
// reads at a glance.
const highContrast: AppTheme = {
  id: 'high-contrast',
  label: 'High Contrast',
  description: 'Maximum readability. Pure black surfaces, white borders.',
  xterm: {
    background: '#000000',
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
  chrome: {
    surfaceDeep: '#000000',
    surface: '#000000',
    surfacePane: '#0a0a0a',
    surfaceLift: '#1a1a1a',
    surfaceEmphasis: '#2a2a2a',
    textStrong: '#ffffff',
    text: '#ffffff',
    textMuted: '#ffff00',
    textFaint: '#cccccc',
    textDim: '#888888',
    borderSoft: '#555555',
    border: '#888888',
    borderStrong: '#ffffff',
    borderHover: '#ffff00',
    accent: '#ffff00',
    accentSoft: 'rgba(255, 255, 0, 0.2)',
    warn: '#ffff00',
    danger: '#ff5555',
    info: '#55ffff',
    success: '#55ff55',
  },
};

export const THEMES: AppTheme[] = [
  kansoZen,
  tokyoNight,
  nord,
  gruvbox,
  catppuccin,
  highContrast,
];

export const DEFAULT_THEME_ID = 'kanso-zen';

export function findTheme(id: string | undefined): AppTheme {
  return THEMES.find((t) => t.id === id) ?? THEMES[0];
}

// Map a ChromePalette to CSS custom property pairs that styles.css
// reads. Centralized so the variable names stay in one place.
export function chromeToCssVars(chrome: ChromePalette): Record<string, string> {
  return {
    '--c-surface-deep': chrome.surfaceDeep,
    '--c-surface': chrome.surface,
    '--c-surface-pane': chrome.surfacePane,
    '--c-surface-lift': chrome.surfaceLift,
    '--c-surface-emphasis': chrome.surfaceEmphasis,
    '--c-text-strong': chrome.textStrong,
    '--c-text': chrome.text,
    '--c-text-muted': chrome.textMuted,
    '--c-text-faint': chrome.textFaint,
    '--c-text-dim': chrome.textDim,
    '--c-border-soft': chrome.borderSoft,
    '--c-border': chrome.border,
    '--c-border-strong': chrome.borderStrong,
    '--c-border-hover': chrome.borderHover,
    '--c-accent': chrome.accent,
    '--c-accent-soft': chrome.accentSoft,
    '--c-warn': chrome.warn,
    '--c-danger': chrome.danger,
    '--c-info': chrome.info,
    '--c-success': chrome.success,
  };
}

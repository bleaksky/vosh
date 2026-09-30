// The xterm theme the terminal draws with, from the chrome theme and the
// "Use the theme's colors for MUD text" setting. The pinned prompt band
// resolves its colors from the same palette, so a prompt on the band
// looks as it would in the text.

import type { ITheme } from '@xterm/xterm';
import { baseAnsiRecord } from './baseAnsi';
import { hexToRgba } from './mapPalette';
import type { AppTheme } from './themes';

// Canonical xterm-256 palette for ANSI codes 0-15. Used when the
// terminal renders in "independent palette" mode (the default) so
// the server's 16-color and 256-color output looks the same
// regardless of which chrome theme the user picked. The 6x6x6
// cube (codes 16-231) and 24-step grayscale ramp (232-255) are
// already theme-independent inside xterm.js; this fixes the 0-15
// slice that the theme used to tint.
export function xtermThemeFor(theme: AppTheme, themeTerminalColors: boolean): ITheme {
  // Tinted mode lets the chrome theme color server output; otherwise
  // the BASE palette applies — the canonical xterm-256 chart unless
  // the user replaced slots in the themes tab (lib/baseAnsi). Either
  // way the chrome theme owns the surfaces (background, foreground,
  // cursor, selection).
  const base: ITheme = themeTerminalColors
    ? { ...theme.xterm }
    : { ...theme.xterm, ...baseAnsiRecord() };
  // Make the selection translucent. Some themes ship a solid (and light)
  // selectionBackground; the search addon selects every active match, so
  // a washed-out selection means an unreadable search hit. Blending over
  // the dark terminal surface keeps the cell dark enough that the line's
  // own text stays legible, while still marking the selection.
  if (theme.xterm.selectionBackground) {
    base.selectionBackground = hexToRgba(theme.xterm.selectionBackground, 0.4);
  }
  return base;
}

/** The 16 ANSI colors of a resolved theme, in ANSI order. */
export function ansi16Of(resolved: ITheme): string[] {
  return [
    resolved.black,
    resolved.red,
    resolved.green,
    resolved.yellow,
    resolved.blue,
    resolved.magenta,
    resolved.cyan,
    resolved.white,
    resolved.brightBlack,
    resolved.brightRed,
    resolved.brightGreen,
    resolved.brightYellow,
    resolved.brightBlue,
    resolved.brightMagenta,
    resolved.brightCyan,
    resolved.brightWhite,
  ].map((c) => c ?? '#000000');
}

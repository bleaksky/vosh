// The xterm theme the terminal draws with, from the chrome theme, the
// "Use the theme's colors for MUD text" setting, Fit game colors and the
// color vision it fits for, which the selection follows too. The
// pinned prompt band and the native renderer resolve their colors from
// the same palette, so a prompt on the band looks as it would in the
// text, on either renderer.

import type { ITheme } from '@xterm/xterm';
import { baseAnsiRecord } from './baseAnsi';
import type { ColorVision } from './gameFit';
import { playPalette, themeTokens, type AppTheme } from './themes';

// Canonical xterm-256 palette for ANSI codes 0-15. Used when the
// terminal renders in "independent palette" mode (the default) so
// the server's 16-color and 256-color output looks the same
// regardless of which chrome theme the user picked. The 6x6x6
// cube (codes 16-231) and 24-step grayscale ramp (232-255) are
// already theme-independent inside xterm.js; this fixes the 0-15
// slice that the theme used to tint.
export function xtermThemeFor(
  theme: AppTheme,
  themeTerminalColors: boolean,
  fit: boolean,
  vision: ColorVision = 'typical',
): ITheme {
  // The play palette, fitted for your color vision while Fit game colors
  // is on. Tinted mode
  // lets it color server output; otherwise the BASE palette applies, the
  // canonical xterm-256 chart unless the user replaced slots in the
  // themes tab (lib/baseAnsi). Either way the theme owns the surfaces
  // (background, foreground, cursor, selection). The selection is the
  // window's token pair, an opaque fill with its own text, so xterm
  // draws what the native renderer and the window draw.
  const play = playPalette(theme, fit, vision);
  const base: ITheme = themeTerminalColors ? { ...play } : { ...play, ...baseAnsiRecord() };
  const tokens = themeTokens(theme, vision);
  base.selectionBackground = tokens.selection;
  base.selectionForeground = tokens.selectionText;
  return base;
}

/** What native_surface_set_theme takes, from the theme xterm draws
 *  with, so the native grid draws the same ground, text and 16 colors. */
export function nativeThemeOf(
  theme: AppTheme,
  themeTerminalColors: boolean,
  fit: boolean,
  vision: ColorVision = 'typical',
) {
  const resolved = xtermThemeFor(theme, themeTerminalColors, fit, vision);
  return {
    background: resolved.background ?? '#101218',
    foreground: resolved.foreground ?? '#cccccc',
    // The selection token, the opaque fill xterm draws.
    selection: themeTokens(theme, vision).selection,
    ansi: ansi16Of(resolved),
  };
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

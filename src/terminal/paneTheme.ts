import { nativeSurfaceSetTheme } from '../ipc/nativeSurface';
import { getColorVision, getFitGameColors } from '../theme/fitGameColors';
import { findTheme } from '../theme/themes';
import { setHighlightGround } from './highlightGround';
import { nativeThemeOf, xtermThemeFor } from './terminalTheme';
import { nativeSurfaceEnabled } from './terminalRenderer';
import { washFields, type WashFields } from './xterm/xtermWash';

// Report the active theme's terminal background to the session, which
// keeps trigger colors readable on it, on either renderer. Then report the
// surface colors and resolved ANSI palette to the native renderer so its
// background/foreground/selection and the 16-color palette match xterm
// (including the themeTerminalColors tint, Fit game colors and the
// color vision it fits for),
// live-updating on theme or toggle change.
export function reportTheme(themeId: string, themeTerminalColors: boolean): void {
  const theme = findTheme(themeId);
  setHighlightGround(theme.xterm.background);
  if (!nativeSurfaceEnabled()) return;
  const native = nativeThemeOf(theme, themeTerminalColors, getFitGameColors(), getColorVision());
  void nativeSurfaceSetTheme({
    background: native.background,
    foreground: native.foreground,
    selection: native.selection,
    ansi: native.ansi,
  }).catch(() => {});
}

// The terminal's palette lives in src/terminal/terminalTheme.ts, which the
// pinned prompt band reads too.

/** `color`, a #rrggbb ground, fully clear. */
function clearGround(color: string | undefined): string {
  const m = /^#?([0-9a-f]{6})/i.exec(color ?? '');
  return m ? `#${m[1]}00` : 'rgba(0, 0, 0, 0)';
}

/** The theme xterm draws with: the terminal palette, its ground clear
 *  while the pane lifts your prompts (`clear`), since the bands draw under
 *  xterm's text and the terminal area's ground shows through. */
export function themeFor(themeId: string, tinted: boolean, clear: boolean) {
  const theme = xtermThemeFor(findTheme(themeId), tinted, getFitGameColors(), getColorVision());
  if (clear) theme.background = clearGround(theme.background);
  return theme;
}

/** The fields xterm paints washed rows in, from the palette and the
 *  ground the native renderer draws, the ground before it goes clear. */
export function washFieldsFor(themeId: string, tinted: boolean): WashFields {
  const native = nativeThemeOf(findTheme(themeId), tinted, getFitGameColors(), getColorVision());
  return washFields(native.ansi, native.background);
}

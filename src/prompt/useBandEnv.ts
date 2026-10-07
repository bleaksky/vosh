import { useEffect, useMemo, useState } from 'react';
import { subscribeBaseAnsi } from '../theme/baseAnsi';
import type { BandEnv } from '../terminal/bandCells';
import { useColorVision, useFitGameColors } from '../theme/fitGameColors';
import { ansi16Of, xtermThemeFor } from '../terminal/terminalTheme';
import { getCurrentThemeId } from '../theme/theme';
import { findTheme, onCustomThemesChanged, themeTokens } from '../theme/themes';

// The colors terminal text outside the renderers draws with. The pinned
// band and the prompt card read them, so a prompt looks there as it does
// in the text.

/** The colors the terminal draws with now: the theme's, fitted for your
 *  color vision while Fit game colors is on, or the base palette while
 *  "Use the theme's colors for MUD text" is off. It follows a theme
 *  change (every apply writes data-theme on the root), a new custom
 *  theme list, which brings a custom theme its fit, an edit to the base
 *  palette, Fit game colors and the color vision. `fitGameColors` stands
 *  in for this window's Fit game colors, for Settings, which plays the
 *  published palette everywhere but the vitals gallery. */
export function useBandEnv(
  themeTerminalColors: boolean,
  brightBold: boolean,
  renderer: BandEnv['renderer'],
  fitGameColors?: boolean,
): BandEnv {
  const windowFit = useFitGameColors();
  const fit = fitGameColors ?? windowFit;
  const vision = useColorVision();
  const [tick, setTick] = useState(0);
  useEffect(() => {
    const bump = () => setTick((n) => n + 1);
    const observer = new MutationObserver(bump);
    observer.observe(document.documentElement, {
      attributeFilter: ['data-theme', 'data-appearance'],
    });
    const stopBase = subscribeBaseAnsi(bump);
    const stopList = onCustomThemesChanged(bump);
    return () => {
      observer.disconnect();
      stopBase();
      stopList();
    };
  }, []);
  return useMemo(() => {
    const theme = findTheme(getCurrentThemeId());
    const resolved = xtermThemeFor(theme, themeTerminalColors, fit, vision);
    const { selection, selectionText } = themeTokens(theme, vision);
    return {
      palette: ansi16Of(resolved),
      fg: resolved.foreground ?? '#cccccc',
      bg: resolved.background ?? '#101218',
      selection,
      selectionText,
      renderer,
      brightBold,
    };
    // tick marks a theme or palette change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tick, themeTerminalColors, fit, vision, brightBold, renderer]);
}

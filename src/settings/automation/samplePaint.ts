import { createContext, useMemo } from 'react';
import type { SamplePaint } from '../../automation/presetSample';
import type { UiConfig } from '../../ipc/uiConfig';
import { nativeThemeOf } from '../../terminal/terminalTheme';
import { resolveThemeTerminalColors } from '../../theme/themes';
import { useActiveTheme } from '../../theme/useActiveTheme';

/** What the samples on the page paint with, and the terminal's text
 *  color. Null draws them in the page's text color alone. */
export type SampleLook = SamplePaint & { foreground: string };

export const SamplePaintContext = createContext<SampleLook | null>(null);

/** The terminal's sixteen and ground, as your theme and settings give
 *  them to the main window, and the ground a fixed color lifts against
 *  while Keep highlight colors readable is on. */
export function useSamplePaint(config: UiConfig): SampleLook {
  const theme = useActiveTheme();
  const { theme_terminal_colors, fit_game_colors, color_vision, readable_highlights } = config;
  const { bright_bold } = config;
  return useMemo(() => {
    const resolved = nativeThemeOf(
      theme,
      resolveThemeTerminalColors(theme_terminal_colors),
      fit_game_colors,
      color_vision,
    );
    return {
      palette: resolved.ansi,
      ground: readable_highlights ? resolved.background : null,
      brightBold: bright_bold,
      foreground: resolved.foreground,
    };
  }, [
    theme,
    theme_terminal_colors,
    fit_game_colors,
    color_vision,
    readable_highlights,
    bright_bold,
  ]);
}

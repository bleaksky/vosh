// The snoop window's look: your theme and your terminal's face, size,
// line height and colors, which the snoop terminals draw with (Snoop
// SN3). It reads the UI config as the window opens and shows the window
// once a frame with the theme has gone out, as Settings and Help do,
// then follows every change Settings or a profile switch sends. The
// main window owns the config and sends every change, so this window
// only listens.

import { useEffect, useMemo, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { subscribeCustomThemesChanged } from '../ipc/theme';
import {
  getUiConfig,
  subscribeBaseAnsiChanged,
  subscribeColorVisionChanged,
  subscribeFitGameColorsChanged,
  subscribeFontChanged,
  subscribeTerminalLineHeightChanged,
  subscribeThemeTerminalColorsChanged,
  TERMINAL_LINE_HEIGHTS,
  type TerminalLineHeight,
  type UiConfig,
} from '../ipc/uiConfig';
import { followReplacedUiConfig } from '../ipc/uiConfigBroadcast';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { loadFontStack, renderFontStack } from '../lib/fontLoader';
import { showAfterThemePaint } from '../lib/reveal';
import { setBaseAnsi } from '../theme/baseAnsi';
import { setColorVision, setFitGameColors } from '../theme/fitGameColors';
import { applyThemePrefs, subscribeThemePrefs } from '../theme/theme';
import { customToAppTheme, resolveThemeTerminalColors, setCustomThemes } from '../theme/themes';
import { DEFAULT_FONT_FAMILY } from './useUiConfigFollow';

/** What the snoop terminals draw with. */
export interface SnoopLook {
  /** The list the terminal draws with. */
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  themeTerminalColors: boolean;
}

export function useSnoopWindowLook(): SnoopLook {
  const [family, setFamily] = useState(DEFAULT_FONT_FAMILY);
  const [fontSize, setFontSize] = useState(14);
  const [lineHeight, setLineHeight] = useState<TerminalLineHeight>('default');
  const [themeTerminalColors, setThemeTerminalColors] = useState(false);
  const fontFamily = useMemo(() => renderFontStack(family), [family]);

  const take = (cfg: UiConfig) => {
    // The custom themes go in before the theme, which may be one.
    setCustomThemes((cfg.custom_themes ?? []).map(customToAppTheme));
    setBaseAnsi(cfg.terminal_base_ansi);
    applyThemePrefs(cfg);
    setFitGameColors(cfg.fit_game_colors);
    setColorVision(cfg.color_vision);
    setFamily(cfg.font_family || DEFAULT_FONT_FAMILY);
    setFontSize(cfg.font_size || 14);
    setLineHeight(cfg.terminal_line_height);
    setThemeTerminalColors(resolveThemeTerminalColors(cfg.theme_terminal_colors));
  };

  useEffect(() => {
    let revealed = false;
    const reveal = () => {
      if (revealed) return;
      revealed = true;
      const win = getCurrentWindow();
      void win.show().then(() => win.setFocus());
    };
    const fallback = window.setTimeout(reveal, 500);
    getUiConfig()
      .then(take)
      .catch((e: unknown) => console.error('[snoop] reading the config failed', e))
      .finally(() => showAfterThemePaint(reveal));
    return () => window.clearTimeout(fallback);
  }, []);

  useEffect(() => loadFontStack(fontFamily), [fontFamily]);

  useTauriEvent(
    (cb) =>
      followReplacedUiConfig(cb, (e) => console.error('[snoop] following the config failed', e)),
    take,
  );
  useTauriEvent(subscribeFontChanged, (change) => {
    setFamily(change.family || DEFAULT_FONT_FAMILY);
    setFontSize(change.size || 14);
  });
  useTauriEvent(subscribeTerminalLineHeightChanged, setLineHeight);
  useTauriEvent(subscribeThemeTerminalColorsChanged, (on) => setThemeTerminalColors(Boolean(on)));
  useTauriEvent(subscribeThemePrefs, (prefs) => applyThemePrefs(prefs));
  useTauriEvent(subscribeCustomThemesChanged, (list) =>
    setCustomThemes(list.map(customToAppTheme)),
  );
  useTauriEvent(subscribeBaseAnsiChanged, setBaseAnsi);
  useTauriEvent(subscribeFitGameColorsChanged, setFitGameColors);
  useTauriEvent(subscribeColorVisionChanged, setColorVision);

  return {
    fontFamily,
    fontSize,
    lineHeight: TERMINAL_LINE_HEIGHTS[lineHeight],
    themeTerminalColors,
  };
}

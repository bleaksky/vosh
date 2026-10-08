// The snoop window's look: the theme and the terminal's face, size,
// line height and colors of the profile its session plays, which the
// snoop terminals draw with. It reads that profile's UI config as the
// window opens and shows the window once a frame with the theme has gone
// out, as Settings and Help do. It reads it again when the session moves
// to another profile, and on every change Settings or a profile switch
// sends, since a change may be another profile's. The main window owns
// the config and sends every change, so this window only listens.

import { useEffect, useMemo, useRef, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { subscribeCustomThemesChanged } from '../ipc/theme';
import {
  getUiConfig,
  subscribeUiConfigReplaced,
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
import { useTauriEvent } from '../ipc/useTauriEvent';
import { loadFontStack, renderFontStack } from '../lib/fontLoader';
import { showAfterThemePaint } from '../lib/reveal';
import { setBaseAnsi } from '../theme/baseAnsi';
import { setColorVision, setFitGameColors } from '../theme/fitGameColors';
import { useProfileOf } from '../stores/session/sessionsStore';
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

export function useSnoopWindowLook(session: number): SnoopLook {
  const profile = useProfileOf(session);
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

  // Counts each read, so only the newest one applies.
  const reads = useRef(0);
  const read = () => {
    if (profile === undefined) return Promise.resolve();
    const mine = ++reads.current;
    return getUiConfig(profile)
      .then((cfg) => {
        if (mine === reads.current) take(cfg);
      })
      .catch((e: unknown) => console.error('[snoop] reading the config failed', e));
  };
  const readRef = useRef(read);
  readRef.current = read;

  const revealed = useRef(false);
  useEffect(() => {
    const fallback = window.setTimeout(() => reveal(revealed), 500);
    return () => window.clearTimeout(fallback);
  }, []);

  // The session's row names its profile once the first list comes, and
  // again when the session plays another.
  useEffect(() => {
    if (profile === undefined) return;
    void readRef.current().finally(() => showAfterThemePaint(() => reveal(revealed)));
  }, [profile]);

  useEffect(() => loadFontStack(fontFamily), [fontFamily]);

  // Each change may be another profile's, so the window reads its own.
  const follow = () => void read();
  useTauriEvent(subscribeUiConfigReplaced, follow);
  useTauriEvent(subscribeFontChanged, follow);
  useTauriEvent(subscribeTerminalLineHeightChanged, follow);
  useTauriEvent(subscribeThemeTerminalColorsChanged, follow);
  useTauriEvent(subscribeThemePrefs, follow);
  useTauriEvent(subscribeCustomThemesChanged, follow);
  useTauriEvent(subscribeBaseAnsiChanged, follow);
  useTauriEvent(subscribeFitGameColorsChanged, follow);
  useTauriEvent(subscribeColorVisionChanged, follow);

  return {
    fontFamily,
    fontSize,
    lineHeight: TERMINAL_LINE_HEIGHTS[lineHeight],
    themeTerminalColors,
  };
}

/** Show the window once. */
function reveal(revealed: { current: boolean }): void {
  if (revealed.current) return;
  revealed.current = true;
  const win = getCurrentWindow();
  void win.show().then(() => win.setFocus());
}

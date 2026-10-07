// The main window's UI config: the fonts, the sizes, the theme and the
// terminal settings it reads at launch, then every change a profile
// switch or a Settings save sends. It shows the window once the launch
// read applies, and brings the preset triggers in line with each profile
// that opens. The
// fonts, sizes and line height it caches let the next load paint in them
// from the first frame.

import { useEffect, useLayoutEffect, useMemo, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { runPresetPlan } from '../automation/presetPlan';
import { nativeSurfaceSetBrightBold, nativeSurfaceSetDividerColor } from '../ipc/nativeSurface';
import { subscribeProfileSwitched } from '../ipc/profiles';
import { subscribeCustomThemesChanged } from '../ipc/theme';
import {
  getUiConfig,
  subscribeBrightBoldChanged,
  subscribeBlinkTextChanged,
  subscribeReadableHighlightsChanged,
  subscribeColorVisionChanged,
  subscribeFitGameColorsChanged,
  subscribeBaseAnsiChanged,
  subscribeSplitDividerChanged,
  subscribeTerminalLineHeightChanged,
  subscribeFontChanged,
  subscribeThemeTerminalColorsChanged,
  normalizeTerminalLineHeight,
  type TerminalLineHeight,
  type UiConfig,
} from '../ipc/uiConfig';
import { followReplacedUiConfig } from '../ipc/uiConfigBroadcast';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { resolveBlinkText, useReduceMotion } from '../lib/blink';
import { loadFontStack, renderFontStack } from '../lib/fontLoader';
import { showAfterThemePaint } from '../lib/reveal';
import { normalizePanelFont, panelFontFamily, panelFontList } from '../panel/panelFont';
import { DEFAULT_PANEL_SIZE, normalizePanelSize, resolvePanelSize } from '../panel/panelSize';
import { setReadableHighlights } from '../terminal/highlightGround';
import { nativeSurfaceEnabled } from '../terminal/terminalRenderer';
import { setBaseAnsi } from '../theme/baseAnsi';
import { fitThemesInPlay } from '../theme/customThemeFits';
import { setColorVision, setFitGameColors } from '../theme/fitGameColors';
import {
  applyAndBroadcastTheme,
  applyThemePrefs,
  subscribeThemePrefs,
  type ThemePrefsOptions,
} from '../theme/theme';
import { customToAppTheme, resolveThemeTerminalColors, setCustomThemes } from '../theme/themes';

// CSS variable applied to the split-scrollback divider. Empty value
// removes the override so the rule falls back to the theme default.
function applySplitDividerColor(color: string | null): void {
  const root = document.documentElement;
  if (color && color.length > 0) {
    root.style.setProperty('--split-divider', color);
  } else {
    root.style.removeProperty('--split-divider');
  }
  // The native surface draws its own divider; keep it in the same color.
  if (nativeSurfaceEnabled()) {
    void nativeSurfaceSetDividerColor(color).catch(() => {});
  }
}

// Report the bright-bold setting to the native surface (xterm has no
// equivalent option, so this drives the GPU renderer only).
function sendBrightBold(on: boolean): void {
  if (nativeSurfaceEnabled()) {
    void nativeSurfaceSetBrightBold(on).catch(() => {});
  }
}

const DEFAULT_FONT_FAMILY = '"JetBrainsMono Bundled", Menlo, Consolas, ui-monospace, monospace';

interface UiConfigFollow {
  /** The terminal font as saved. */
  fontFamily: string;
  /** The list the terminal draws with, the one the native atlas walks. */
  renderFamily: string;
  fontSize: number;
  /** The size every pane and the status line draw at. */
  panelTextPx: number;
  terminalLineHeight: TerminalLineHeight;
  themeTerminalColors: boolean;
  brightBold: boolean;
  /** Whether blinking text blinks now. */
  blinkText: boolean;
}

/** Read the UI config at launch and follow it. `onThemesChanged` runs
 *  when another window changes the custom themes. */
export function useUiConfigFollow({
  onThemesChanged,
}: {
  onThemesChanged: () => void;
}): UiConfigFollow {
  // Boot with the last-known font instead of the compiled default.
  // The real value arrives async from the Rust config; booting on the
  // default and flipping when config lands rescales the whole input
  // row a beat after every page load, and the next keystroke visibly
  // shifts the layout as stale heights correct themselves.
  const [fontFamily, setFontFamily] = useState(() => {
    try {
      return localStorage.getItem('vosh.cache.fontFamily') || DEFAULT_FONT_FAMILY;
    } catch {
      return DEFAULT_FONT_FAMILY;
    }
  });
  // The list the terminal draws with, the one the native atlas walks.
  const renderFamily = useMemo(() => renderFontStack(fontFamily), [fontFamily]);
  // The Panel font, cached like the terminal font so the panes and the
  // status line paint in it from the first frame.
  const [panelFont, setPanelFont] = useState(() => {
    try {
      return normalizePanelFont(localStorage.getItem('vosh.cache.panelFont'));
    } catch {
      return '';
    }
  });
  const [fontSize, setFontSize] = useState(() => {
    try {
      const n = Number(localStorage.getItem('vosh.cache.fontSize'));
      return Number.isFinite(n) && n >= 6 && n <= 64 ? n : 14;
    } catch {
      return 14;
    }
  });
  // The panel size as saved, 0 for the terminal size, cached like the
  // panel font so the panes and the status line paint at it from the
  // first frame.
  const [panelSize, setPanelSize] = useState(() => {
    try {
      const cached = localStorage.getItem('vosh.cache.panelSize');
      return cached === null ? DEFAULT_PANEL_SIZE : normalizePanelSize(Number(cached));
    } catch {
      return DEFAULT_PANEL_SIZE;
    }
  });
  // The size every pane and the status line draw at.
  const panelTextPx = resolvePanelSize(panelSize, fontSize);
  // Cached like the font so the first paint uses the saved row spacing
  // instead of reflowing once the config arrives.
  const [terminalLineHeight, setTerminalLineHeight] = useState<TerminalLineHeight>(() => {
    try {
      return normalizeTerminalLineHeight(localStorage.getItem('vosh.cache.lineHeight'));
    } catch {
      return 'default';
    }
  });
  const [themeTerminalColors, setThemeTerminalColors] = useState(false);
  // Bright bold, which the native grid and the pinned band over it follow.
  const [brightBold, setBrightBold] = useState(false);
  // Blinking text: your choice, undefined until the config loads, and
  // with none, on unless your system reduces motion. It draws steady
  // until the config loads.
  const [blinkChoice, setBlinkChoice] = useState<boolean | null | undefined>(undefined);
  const reduceMotion = useReduceMotion();
  const blinkText = blinkChoice !== undefined && resolveBlinkText(blinkChoice, reduceMotion);
  // Apply a whole UI config here, at launch and when a profile switch or
  // an import replaces it. `theme` says how the theme reaches the other
  // windows.
  const applyConfig = (cfg: UiConfig, theme: ThemePrefsOptions) => {
    // Register user-authored themes BEFORE the theme apply so
    // the picked theme can actually be a custom entry.
    setCustomThemes((cfg.custom_themes ?? []).map(customToAppTheme));
    setBaseAnsi(cfg.terminal_base_ansi);
    applyThemePrefs(cfg, theme);
    setFontFamily(cfg.font_family || DEFAULT_FONT_FAMILY);
    setFontSize(cfg.font_size || 14);
    setPanelFont(cfg.panel_font);
    setPanelSize(cfg.panel_font_size);
    setTerminalLineHeight(cfg.terminal_line_height);
    setThemeTerminalColors(resolveThemeTerminalColors(cfg.theme_terminal_colors));
    setBrightBold(cfg.bright_bold);
    sendBrightBold(cfg.bright_bold);
    setBlinkChoice(cfg.blink_text);
    setFitGameColors(cfg.fit_game_colors);
    setColorVision(cfg.color_vision);
    fitThemesInPlay(cfg);
    setReadableHighlights(cfg.readable_highlights);
    applySplitDividerColor(cfg.split_divider_color);
  };

  useEffect(() => {
    // Tauri creates the main window with visible=false so the user
    // doesn't see a default-styled white flash. Reveal once theme and
    // font have applied and a frame with the theme has gone out.
    let revealed = false;
    const reveal = () => {
      if (revealed) return;
      revealed = true;
      const win = getCurrentWindow();
      win
        .show()
        .then(() => win.setFocus())
        .catch((e) => console.error('[main] window show failed', e));
    };
    const fallback = window.setTimeout(reveal, 500);
    const onUnmount = () => window.clearTimeout(fallback);
    getUiConfig()
      .then(async (cfg) => {
        // This window owns following the OS appearance, so its flips
        // reach the Terminal and every other window.
        applyConfig(cfg, { broadcast: true, broadcastFlips: true });

        // Bring the preset triggers in line with the presets that are on
        // and your edits to them.
        await runPresetPlan();
      })
      .catch(() => void applyAndBroadcastTheme('system'))
      .finally(() => showAfterThemePaint(reveal));
    return onUnmount;
  }, []);

  // A profile switch (#profile switch, a Settings click, or the
  // Char.Status swap after login), #profile load, #profile reset, and
  // an import each replace the whole UI config in the backend, while
  // this window still shows the old profile's theme, font, and the
  // rest. Read the new config, apply it here, and send every field to
  // every window, so Input, the vitals, the prompt, and each other
  // per-field listener settle too. The panes, the tracked affects, the
  // tick settings, and the chip style also come from the backend on
  // their own events.
  useTauriEvent(
    (cb) =>
      followReplacedUiConfig(
        cb,
        (e) => console.error('[app] reading the replaced config failed', e),
        { broadcast: true },
      ),
    (cfg: UiConfig) => {
      applyConfig(cfg, { broadcast: true });
      // The profile in front changed, or #profile load or an import
      // brought it other presets and edits.
      void runPresetPlan();
    },
  );

  // A switch opens another profile, whose stored preset triggers may be
  // stale or carry another profile's edits until the plan runs for it.
  useTauriEvent(subscribeProfileSwitched, (name) => void runPresetPlan(name));

  useEffect(() => {
    const root = document.documentElement;
    // Inject @font-face blocks for every named family in the stack so
    // WKWebView can render fonts it would otherwise refuse to match.
    loadFontStack(renderFamily);
    root.style.setProperty('--app-font-family', renderFamily);
    root.style.setProperty('--app-font-size', `${fontSize}px`);
    try {
      localStorage.setItem('vosh.cache.fontFamily', fontFamily);
      localStorage.setItem('vosh.cache.fontSize', String(fontSize));
    } catch {
      // cache only; config remains the source of truth
    }
  }, [fontFamily, renderFamily, fontSize]);

  // The panes and the status line draw in the panel faces, which
  // tokens.css reads from --panel-font-family: nothing under As designed,
  // where each keeps the face it was designed in, or the terminal face,
  // the system face, or a font you picked, which loads the way the
  // terminal font does.
  useEffect(() => {
    const list = panelFontList(panelFont);
    if (list) loadFontStack(list);
    const family = panelFontFamily(panelFont);
    const root = document.documentElement.style;
    if (family === null) root.removeProperty('--panel-font-family');
    else root.setProperty('--panel-font-family', family);
    try {
      localStorage.setItem('vosh.cache.panelFont', panelFont);
    } catch {
      // cache only; config remains the source of truth
    }
  }, [panelFont]);

  // The panes and the status line draw at --panel-text-px (panel.css,
  // frame.css). It lands before the paint that lays the panes out at the
  // new size, so the rows and their geometry move together.
  useLayoutEffect(() => {
    document.documentElement.style.setProperty('--panel-text-px', String(panelTextPx));
  }, [panelTextPx]);

  useEffect(() => {
    try {
      localStorage.setItem('vosh.cache.panelSize', String(panelSize));
    } catch {
      // cache only; config remains the source of truth
    }
  }, [panelSize]);

  // Cross-window emit from the settings save path. window CustomEvents
  // do not cross webviews, so we listen via the Tauri event bus here.
  useTauriEvent(subscribeFontChanged, (detail) => {
    setFontFamily(detail.family || DEFAULT_FONT_FAMILY);
    setFontSize(detail.size || 14);
    setPanelFont(normalizePanelFont(detail.panel));
    setPanelSize(normalizePanelSize(detail.panelSize));
  });

  useEffect(() => {
    try {
      localStorage.setItem('vosh.cache.lineHeight', terminalLineHeight);
    } catch {
      // cache only; config remains the source of truth
    }
  }, [terminalLineHeight]);

  // Settings save broadcasts the terminal line height.
  useTauriEvent(subscribeTerminalLineHeightChanged, (value) => {
    setTerminalLineHeight(value);
  });

  // Settings save broadcasts the bright-bold toggle. Apply it to the
  // native surface without a relaunch.
  useTauriEvent(subscribeBrightBoldChanged, (value) => {
    setBrightBold(value);
    sendBrightBold(value);
  });

  // Settings save broadcasts the Blinking text choice.
  useTauriEvent(subscribeBlinkTextChanged, (value) => setBlinkChoice(value));

  // Settings save broadcasts Fit game colors. The terminal, the prompt
  // band and the panes draw from it at once.
  useTauriEvent(subscribeFitGameColorsChanged, setFitGameColors);

  // Settings save broadcasts the color vision. The window paints its
  // status colors for it, and play draws the fit for it at once, a
  // custom theme once its fit lands.
  useTauriEvent(subscribeColorVisionChanged, setColorVision);

  // Settings save broadcasts Keep highlight colors readable. The
  // session takes it for the next line.
  useTauriEvent(subscribeReadableHighlightsChanged, setReadableHighlights);

  // Settings save broadcasts the new divider color. Apply it on the
  // main window without a relaunch.
  useTauriEvent(subscribeSplitDividerChanged, (color) => {
    applySplitDividerColor(color);
  });

  // Live-flip the terminal palette mode when the user toggles the
  // setting. The Terminal component re-applies the palette on the
  // prop change without recreating xterm.
  useTauriEvent(subscribeThemeTerminalColorsChanged, (on) => {
    setThemeTerminalColors(Boolean(on));
  });

  // Settings saved, or the palette picked, new theme fields. Keep this
  // window's copy current so the OS listener and the next palette pick
  // start from them. The sender already broadcast the resolved theme.
  useTauriEvent(subscribeThemePrefs, (prefs) => {
    applyThemePrefs(prefs);
  });

  // Custom-themes catalog updates from any other webview. Refreshes
  // the in-memory THEMES registry so a subsequent theme-changed event
  // can find a newly-saved custom theme.
  useTauriEvent(subscribeCustomThemesChanged, (list) => {
    setCustomThemes(list.map(customToAppTheme));
    onThemesChanged();
  });

  // Base ANSI palette edits from the settings window: update the
  // live override; the Terminal re-derives via its own subscription.
  useTauriEvent(subscribeBaseAnsiChanged, (colors) => {
    setBaseAnsi(colors);
  });

  return {
    fontFamily,
    renderFamily,
    fontSize,
    panelTextPx,
    terminalLineHeight,
    themeTerminalColors,
    brightBold,
    blinkText,
  };
}

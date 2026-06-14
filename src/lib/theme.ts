// Theme runtime. Applies the active theme to the document root via
// CSS custom properties + a `data-theme` attribute, and broadcasts a
// window event so the Terminal can refresh its xterm palette.

import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  chromeToCssVars,
  customToAppTheme,
  DEFAULT_THEME_ID,
  findTheme,
  setCustomThemes,
  type AppTheme,
} from './themes';

const SYNC_EVENT = 'vosh://theme-changed';

let cleanupContrastListener: (() => void) | null = null;
let currentThemeId: string = DEFAULT_THEME_ID;

function applyToRoot(theme: AppTheme) {
  const root = document.documentElement;
  root.setAttribute('data-theme', theme.id);
  const vars = chromeToCssVars(theme.chrome);
  for (const [key, value] of Object.entries(vars)) {
    root.style.setProperty(key, value);
  }
  // Expose the xterm background as a CSS var so the split history
  // overlay can paint an opaque undercoat that matches the renderer's
  // own background — covers xterm's sub-frame render gap during scroll.
  if (theme.xterm.background) {
    root.style.setProperty('--xterm-bg', theme.xterm.background);
  }
  currentThemeId = theme.id;
}

export function applyTheme(choice: string) {
  if (cleanupContrastListener) {
    cleanupContrastListener();
    cleanupContrastListener = null;
  }

  // The legacy `system` choice tracks the OS contrast preference. Map
  // it to high-contrast when the user has asked for more contrast,
  // otherwise the default theme.
  if (choice === 'system') {
    const mq = window.matchMedia('(prefers-contrast: more)');
    const update = () => {
      const target = mq.matches ? 'high-contrast' : DEFAULT_THEME_ID;
      applyToRoot(findTheme(target));
    };
    update();
    mq.addEventListener('change', update);
    cleanupContrastListener = () => mq.removeEventListener('change', update);
    return;
  }

  const found = findTheme(choice);
  if (found.id === choice || !choice) {
    applyToRoot(found);
    return;
  }
  // Requested theme wasn't found in the live registry — likely a
  // freshly-saved custom theme this window hasn't synced yet.
  // Re-fetch the catalog from the backend, register, retry. Falls
  // back to the matched-but-defaulted result if the refresh fails.
  applyToRoot(found);
  void refreshAndReapply(choice);
}

async function refreshAndReapply(choice: string): Promise<void> {
  try {
    // Inline import to dodge the session.ts <-> theme.ts cycle.
    const { getUiConfig } = await import('./session');
    const cfg = await getUiConfig();
    setCustomThemes((cfg.custom_themes ?? []).map(customToAppTheme));
    const refreshed = findTheme(choice);
    if (refreshed.id === choice) {
      applyToRoot(refreshed);
    }
  } catch {
    // Backend unavailable or config malformed; keep the fallback.
  }
}

/// Apply + broadcast so other windows (settings ↔ main) stay in sync.
export async function applyAndBroadcastTheme(choice: string): Promise<void> {
  applyTheme(choice);
  try {
    await emit(SYNC_EVENT, choice);
  } catch {
    // Tauri unavailable; local apply is the persistent fallback.
  }
}

export async function subscribeThemeChanges(
  callback: (themeId: string) => void,
): Promise<UnlistenFn> {
  return listen<string>(SYNC_EVENT, (event) => {
    if (typeof event.payload !== 'string') return;
    // No same-id guard. applyTheme is idempotent, and on startup the
    // local applyTheme runs before the broadcast lands, so the guard
    // would skip the only chance the Terminal has to pick up its
    // initial xterm palette.
    applyTheme(event.payload);
    callback(event.payload);
  });
}

export function getCurrentThemeId(): string {
  return currentThemeId;
}

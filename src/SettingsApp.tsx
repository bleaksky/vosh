import { useCallback, useEffect, useRef, useState, type ComponentType } from 'react';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  getUiConfig,
  loadoutsGetState,
  primeUiConfigBroadcast,
  primeUiConfigTheme,
  subscribeLoadoutsChanged,
  subscribeProfileSwitched,
  type UiConfig,
} from './lib/session';
import { applyTheme, subscribeThemeChanges } from './lib/theme';
import { customToAppTheme, setCustomThemes } from './lib/themes';
import { loadFontStack } from './lib/fontLoader';
import { isMacPlatform } from './lib/palette';
import {
  resolveSettingsTarget,
  settingsGroupLabel,
  settingsScrollIds,
  type SettingsGroup,
  type SettingsTarget,
} from './lib/settingsNav';
import { SETTINGS_GOTO_EVENT, SETTINGS_PENDING_KEY } from './lib/settingsLink';
import { revealSettingsAnchor } from './components/settings/revealAnchor';
import { Sidebar } from './components/settings/Sidebar';
import { WindowControls } from './components/settings/WindowControls';
import { ChevronRightIcon } from './components/settings/ui';
import type { SettingsPageProps } from './components/settings/pageTypes';
import { GeneralGroup } from './components/settings/groups/GeneralGroup';
import { AppearanceGroup } from './components/settings/groups/AppearanceGroup';
import { LayoutGroup } from './components/settings/groups/LayoutGroup';
import { InputGroup } from './components/settings/groups/InputGroup';
import { AutomationGroup } from './components/settings/groups/AutomationGroup';
import { CharactersPage } from './components/settings/pages/CharactersPage';

// The Settings window (the approved Settings boards). A 200 px sidebar
// with search and the six group nav, and a content column with the
// breadcrumb in the 32 px band over the group's page. On macOS the
// native traffic lights sit over the sidebar. Windows and Linux draw
// minimize, maximize, and close at the right of the band.
//
// Every way into Settings names a target (src/lib/settingsNav.ts): the
// nav, a search hit, a deep link from the main window. The frame shows
// the target's group, hands the page the target, and scrolls to the
// anchor the target names.

interface GroupPage {
  Page: ComponentType<SettingsPageProps>;
  /** The page pins its own bar and scrolls inside itself, so the
   *  content column does not scroll. */
  selfScroll?: boolean;
}

const PAGES: Record<SettingsGroup, GroupPage> = {
  general: { Page: GeneralGroup },
  appearance: { Page: AppearanceGroup },
  layout: { Page: LayoutGroup },
  input: { Page: InputGroup },
  automation: { Page: AutomationGroup },
  characters: { Page: CharactersPage },
};

/** The target a cold open should land on, left by the main window. */
function takePendingTarget(): SettingsTarget | null {
  try {
    const pending = localStorage.getItem(SETTINGS_PENDING_KEY);
    localStorage.removeItem(SETTINGS_PENDING_KEY);
    return pending ? resolveSettingsTarget(pending) : null;
  } catch {
    return null;
  }
}

function clearPendingTarget() {
  try {
    localStorage.removeItem(SETTINGS_PENDING_KEY);
  } catch {
    // Storage unavailable. Nothing was left there.
  }
}

export function SettingsApp() {
  const mac = isMacPlatform();
  const [nav, setNav] = useState<{ target: SettingsTarget; seq: number }>(() => ({
    target: takePendingTarget() ?? { group: 'general' },
    seq: 0,
  }));
  const [config, setConfig] = useState<UiConfig | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pathB, setPathB] = useState(false);
  const contentRef = useRef<HTMLDivElement | null>(null);

  const go = useCallback((target: SettingsTarget) => {
    setError(null);
    setNav((prev) => ({ target, seq: prev.seq + 1 }));
  }, []);

  // A deep link for a window that is already open. The main window
  // also left the target in storage for a cold open, so clear it, or
  // the next cold open would land here again.
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    void listen<string>(SETTINGS_GOTO_EVENT, (event) => {
      if (typeof event.payload !== 'string') return;
      clearPendingTarget();
      go(resolveSettingsTarget(event.payload));
      void getCurrentWindow().setFocus();
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, [go]);

  // Scroll to the anchor the target names once the page draws it, or
  // to the top for a bare group.
  useEffect(() => {
    const root = contentRef.current;
    if (!root) return;
    const ids = settingsScrollIds(nav.target);
    if (ids.length === 0) {
      root.scrollTop = 0;
      return;
    }
    return revealSettingsAnchor(root, ids);
  }, [nav]);

  useEffect(() => {
    let cancelled = false;
    const refresh = () =>
      loadoutsGetState()
        .then((s) => {
          if (!cancelled) setPathB(s.path_b_active);
        })
        .catch(() => {});
    void refresh();
    let unsub: (() => void) | undefined;
    void subscribeLoadoutsChanged(() => {
      if (!cancelled) void refresh();
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      if (unsub) unsub();
    };
  }, []);

  // Load current config and reveal the window once painted.
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
      .then((cfg) => {
        setCustomThemes((cfg.custom_themes ?? []).map(customToAppTheme));
        setConfig(cfg);
        applyTheme(cfg.theme);
      })
      .catch((e) => setError(String(e)))
      .finally(reveal);
    return () => window.clearTimeout(fallback);
  }, []);

  // A profile switch replaces the whole UI config in the backend. Every
  // save from this window sends the full snapshot, so re-read it here or
  // the next edit writes the old profile's tracked affects, custom
  // themes, and prompt template over the new profile.
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    void subscribeProfileSwitched(() => {
      getUiConfig()
        .then((cfg) => {
          if (cancelled) return;
          setCustomThemes((cfg.custom_themes ?? []).map(customToAppTheme));
          setConfig(cfg);
          applyTheme(cfg.theme);
          primeUiConfigBroadcast(cfg);
        })
        .catch((e) => setError(String(e)));
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, []);

  // Another window can change the theme (the palette's Choose theme).
  // subscribeThemeChanges repaints this window, and the config copy
  // takes the new choice so the next full save from any page carries
  // it instead of writing the old theme back.
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    void subscribeThemeChanges((themeId) => {
      primeUiConfigTheme(themeId);
      setConfig((prev) => (prev && prev.theme !== themeId ? { ...prev, theme: themeId } : prev));
    })
      .then((fn) => {
        if (cancelled) fn();
        else unsub = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, []);

  // MUD text in Settings (patterns, commands, host and port) uses your
  // terminal font through --font-mud, the way the main window does.
  const fontFamily = config?.font_family;
  useEffect(() => {
    if (!fontFamily) return;
    loadFontStack(fontFamily);
    document.documentElement.style.setProperty('--app-font-family', fontFamily);
  }, [fontFamily]);

  const group = nav.target.group;
  const { Page, selfScroll } = PAGES[group];

  return (
    <div className="st-app">
      <Sidebar group={group} onNavigate={go} pathB={pathB} mac={mac} />
      <main className="st-main">
        <header className="st-header" data-tauri-drag-region="">
          <div className="st-crumb" data-tauri-drag-region="">
            <span className="st-crumb-root" data-tauri-drag-region="">
              Settings
            </span>
            <ChevronRightIcon size={12} className="st-crumb-sep" />
            <h1 className="st-crumb-title" data-tauri-drag-region="">
              {settingsGroupLabel(group)}
            </h1>
          </div>
          {!mac && <WindowControls />}
        </header>
        <div ref={contentRef} className="st-content" data-scroll={selfScroll ? 'self' : undefined}>
          {error && (
            <p className="st-error" role="alert">
              {error}
            </p>
          )}
          <Page
            key={group}
            target={nav.target}
            navSeq={nav.seq}
            config={config}
            setConfig={setConfig}
            onError={setError}
            pathB={pathB}
            navigate={go}
          />
        </div>
      </main>
    </div>
  );
}

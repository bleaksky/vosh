import { useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { HighlightsDrawer } from './components/HighlightsDrawer';
import { LayoutEditorPanel } from './components/LayoutEditorPanel';
import { PluginsPanel } from './components/PluginsPanel';
import { SearchView } from './components/SearchView';
import { SettingsDrawer } from './components/SettingsDrawer';
import { TriggersDrawer } from './components/TriggersDrawer';
import { applyTheme } from './lib/theme';
import { getUiConfig } from './lib/session';

type TabId =
  | 'profile'
  | 'highlights'
  | 'triggers'
  | 'plugins'
  | 'search'
  | 'layout';

interface Tab {
  id: TabId;
  label: string;
}

const TABS: Tab[] = [
  { id: 'profile', label: 'Profile' },
  { id: 'highlights', label: 'Highlights' },
  { id: 'triggers', label: 'Triggers' },
  { id: 'plugins', label: 'Plugins' },
  { id: 'search', label: 'Search' },
  { id: 'layout', label: 'Layout' },
];

const DEFAULT_FONT_FAMILY =
  'BerkeleyMono Nerd Font, JetBrains Mono, Fira Code, Menlo, Consolas, ui-monospace, monospace';

export function SettingsApp() {
  const [active, setActive] = useState<TabId>('profile');
  const [error, setError] = useState<string | null>(null);

  // Pick up the persisted theme + font on first render so the settings
  // window matches the main window's look. The settings window has no
  // terminal of its own, but the chrome should still respect the user's
  // chosen font and contrast.
  //
  // The Tauri backend opens this window with visible=false so the user
  // doesn't see the default unstyled state during webview boot. Once
  // the theme is applied (or fails fast) we call show() so the window
  // appears already-painted.
  useEffect(() => {
    let revealed = false;
    const reveal = () => {
      if (revealed) return;
      revealed = true;
      const win = getCurrentWindow();
      win
        .show()
        .then(() => win.setFocus())
        .catch((e) => console.error('[settings] window show failed', e));
    };
    const fallback = window.setTimeout(reveal, 500);
    getUiConfig()
      .then((cfg) => {
        applyTheme(cfg.theme);
        const root = document.documentElement;
        root.style.setProperty(
          '--app-font-family',
          cfg.font_family || DEFAULT_FONT_FAMILY,
        );
        root.style.setProperty('--app-font-size', `${cfg.font_size || 14}px`);
      })
      .catch(() => applyTheme('system'))
      .finally(reveal);
    return () => window.clearTimeout(fallback);
  }, []);

  const handleClose = async () => {
    try {
      await getCurrentWindow().close();
    } catch (e) {
      console.error('failed to close settings window', e);
    }
  };

  const handleError = (message: string) => {
    setError(message);
    console.error('[settings]', message);
  };

  return (
    <main className="settings-window">
      <header className="settings-window-header">
        <div className="settings-tabs" role="tablist">
          {TABS.map((tab) => (
            <button
              key={tab.id}
              type="button"
              role="tab"
              aria-selected={active === tab.id}
              className={`settings-tab${active === tab.id ? ' is-active' : ''}`}
              onClick={() => setActive(tab.id)}
            >
              {tab.label}
            </button>
          ))}
        </div>
        <button
          type="button"
          onClick={() => void handleClose()}
          aria-label="close settings window"
          className="settings-close"
        >
          ×
        </button>
      </header>
      {error && (
        <div className="settings-error" role="alert">
          {error}
        </div>
      )}
      <div className="settings-window-body" role="tabpanel">
        {active === 'profile' && (
          <SettingsDrawer
            open
            onClose={() => void handleClose()}
            onError={handleError}
            chromeless
          />
        )}
        {active === 'highlights' && (
          <HighlightsDrawer
            open
            onClose={() => void handleClose()}
            onError={handleError}
            chromeless
          />
        )}
        {active === 'triggers' && (
          <TriggersDrawer
            open
            onClose={() => void handleClose()}
            onError={handleError}
            chromeless
          />
        )}
        {active === 'plugins' && <PluginsPanel onError={handleError} />}
        {active === 'search' && <SearchView onError={handleError} />}
        {active === 'layout' && <LayoutEditorPanel onError={handleError} />}
      </div>
    </main>
  );
}

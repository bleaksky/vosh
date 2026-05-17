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
  subtitle: string;
}

interface Group {
  id: string;
  label: string;
  tabs: Tab[];
}

const GROUPS: Group[] = [
  {
    id: 'session',
    label: 'Session',
    tabs: [
      {
        id: 'profile',
        label: 'Profile',
        subtitle:
          'Theme, font, auto-update, and TOML import/export for the whole profile.',
      },
      {
        id: 'highlights',
        label: 'Highlights',
        subtitle:
          'Curated trigger bundles for combat, defenses, buff drops, and ambient events.',
      },
      {
        id: 'triggers',
        label: 'Triggers',
        subtitle:
          'Raw JSON view of every trigger in the store. Power-user surface for hand authoring.',
      },
    ],
  },
  {
    id: 'tools',
    label: 'Tools',
    tabs: [
      {
        id: 'plugins',
        label: 'Plugins',
        subtitle:
          'Lua plugins loaded from your scripts directory. Toggle, reload, inspect.',
      },
      {
        id: 'search',
        label: 'Search',
        subtitle:
          'Full-text search across past sessions and chat logs.',
      },
      {
        id: 'layout',
        label: 'Layout',
        subtitle:
          'Arrange dock zones and reorder side-panel sections.',
      },
    ],
  },
];

const FLAT_TABS: Tab[] = GROUPS.flatMap((g) => g.tabs);

const DEFAULT_FONT_FAMILY =
  'BerkeleyMono Nerd Font, JetBrains Mono, Fira Code, Menlo, Consolas, ui-monospace, monospace';

const APP_VERSION = '0.0.1';

export function SettingsApp() {
  const [active, setActive] = useState<TabId>('profile');
  const [error, setError] = useState<string | null>(null);

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

  // ESC anywhere in the settings window closes it — advertised in the
  // close-pill hint at the bottom of the sidebar.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        void handleClose();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const handleError = (message: string) => {
    setError(message);
    console.error('[settings]', message);
  };

  const activeTab = FLAT_TABS.find((t) => t.id === active) ?? FLAT_TABS[0];
  const activeGroup = GROUPS.find((g) =>
    g.tabs.some((t) => t.id === active),
  );

  return (
    <main className="settings-shell">
      <aside className="settings-sidebar" aria-label="settings navigation">
        <div className="settings-brand">
          <span className="settings-brand-mark" aria-hidden="true">
            ◆
          </span>
          <div className="settings-brand-text">
            <span className="settings-brand-name">Vosh</span>
            <span className="settings-brand-version">v{APP_VERSION}</span>
          </div>
        </div>
        <nav className="settings-nav" role="tablist" aria-orientation="vertical">
          {GROUPS.map((group) => (
            <div key={group.id} className="settings-nav-group">
              <h2 className="settings-nav-label">{group.label}</h2>
              {group.tabs.map((tab) => {
                const isActive = active === tab.id;
                return (
                  <button
                    key={tab.id}
                    type="button"
                    role="tab"
                    aria-selected={isActive}
                    className={`settings-nav-item${isActive ? ' is-active' : ''}`}
                    onClick={() => setActive(tab.id)}
                  >
                    <span className="settings-nav-marker" aria-hidden="true">
                      {isActive ? '◆' : '◇'}
                    </span>
                    <span className="settings-nav-name">{tab.label}</span>
                  </button>
                );
              })}
            </div>
          ))}
        </nav>
        <button
          type="button"
          onClick={() => void handleClose()}
          className="settings-close-pill"
        >
          <span>close</span>
          <span className="settings-close-key" aria-hidden="true">
            ESC
          </span>
        </button>
      </aside>
      <section className="settings-content" role="tabpanel">
        <header className="settings-content-head">
          <div className="settings-content-eyebrow">
            {activeGroup?.label ?? ''} · {activeTab.label}
          </div>
          <h1 className="settings-content-title">{activeTab.label}</h1>
          <p className="settings-content-subtitle">{activeTab.subtitle}</p>
        </header>
        {error && (
          <div className="settings-error" role="alert">
            {error}
          </div>
        )}
        <div className="settings-content-body">
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
      </section>
    </main>
  );
}

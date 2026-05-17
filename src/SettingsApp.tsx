import { useEffect, useState } from 'react';
import { emit } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { TopBar } from './components/TopBar';
import {
  exportTriggers,
  getUiConfig,
  importTriggers,
  setUiConfig,
  type UiConfig,
} from './lib/session';
import { applyTheme } from './lib/theme';
import { THEMES } from './lib/themes';

const FONT_PRESETS = [
  'BerkeleyMono Nerd Font, JetBrains Mono, Fira Code, Menlo, monospace',
  'JetBrains Mono, Menlo, monospace',
  'Fira Code, Menlo, monospace',
  'Menlo, Consolas, monospace',
];

type TabId = 'general' | 'triggers';
const TABS: { id: TabId; label: string }[] = [
  { id: 'general', label: 'general' },
  { id: 'triggers', label: 'triggers' },
];

// Settings window. Frameless Ghostty chrome via the shared TopBar;
// body splits into named tabs along a thin top strip. T-now ships
// general (theme + font) and triggers (JSON in / out via the backend
// export/import commands).
export function SettingsApp() {
  const [tab, setTab] = useState<TabId>('general');
  const [config, setConfig] = useState<UiConfig | null>(null);
  const [error, setError] = useState<string | null>(null);

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
        setConfig(cfg);
        applyTheme(cfg.theme);
      })
      .catch((e) => setError(String(e)))
      .finally(reveal);
    return () => window.clearTimeout(fallback);
  }, []);

  return (
    <main className="app settings-app">
      <TopBar brand="[vosh : settings]" showAuxButtons={false} />
      <nav className="settings-tabs">
        {TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            className={`settings-tab${tab === t.id ? ' settings-tab-active' : ''}`}
            aria-pressed={tab === t.id}
            onClick={() => setTab(t.id)}
          >
            {t.label}
          </button>
        ))}
      </nav>
      <div className="settings-body">
        {error && <div className="settings-error">error: {error}</div>}
        {tab === 'general' && (
          <GeneralTab config={config} setConfig={setConfig} onError={setError} />
        )}
        {tab === 'triggers' && <TriggersTab onError={setError} />}
      </div>
    </main>
  );
}

interface GeneralProps {
  config: UiConfig | null;
  setConfig: (updater: (prev: UiConfig | null) => UiConfig | null) => void;
  onError: (e: string | null) => void;
}

function GeneralTab({ config, setConfig, onError }: GeneralProps) {
  const [savedAt, setSavedAt] = useState<number | null>(null);

  const update = (patch: Partial<UiConfig>) => {
    setConfig((prev) => (prev ? { ...prev, ...patch } : prev));
  };

  const save = async () => {
    if (!config) return;
    try {
      await setUiConfig(config);
      applyTheme(config.theme);
      await emit('vosh://theme-changed', config.theme);
      window.dispatchEvent(
        new CustomEvent('vosh:font-changed', {
          detail: { family: config.font_family, size: config.font_size },
        }),
      );
      setSavedAt(Date.now());
    } catch (e) {
      onError(String(e));
    }
  };

  const close = () => void getCurrentWindow().close();

  if (!config) return <div className="settings-loading">loading…</div>;

  return (
    <>
      <Row label="theme">
        <select
          value={config.theme}
          onChange={(e) => {
            update({ theme: e.target.value });
            applyTheme(e.target.value);
          }}
        >
          {THEMES.map((t) => (
            <option key={t.id} value={t.id}>
              {t.label}
            </option>
          ))}
        </select>
      </Row>
      <Row label="font">
        <select
          value={config.font_family}
          onChange={(e) => update({ font_family: e.target.value })}
        >
          {FONT_PRESETS.map((f) => (
            <option key={f} value={f}>
              {f.split(',')[0]}
            </option>
          ))}
        </select>
      </Row>
      <Row label="size">
        <input
          type="number"
          min={9}
          max={32}
          value={config.font_size}
          onChange={(e) =>
            update({ font_size: Math.max(9, Math.min(32, Number(e.target.value) || 14)) })
          }
        />
        <span className="settings-unit">px</span>
      </Row>
      <div className="settings-actions">
        <button type="button" className="settings-btn" onClick={() => void save()}>
          [save]
        </button>
        <button type="button" className="settings-btn settings-btn-mute" onClick={close}>
          [close]
        </button>
        {savedAt !== null && <span className="settings-saved">saved.</span>}
      </div>
    </>
  );
}

interface TriggersProps {
  onError: (e: string | null) => void;
}

function TriggersTab({ onError }: TriggersProps) {
  const [text, setText] = useState<string>('');
  const [count, setCount] = useState<number | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [savedAt, setSavedAt] = useState<number | null>(null);

  const reload = async () => {
    try {
      const json = await exportTriggers();
      setText(json);
      try {
        const parsed = JSON.parse(json);
        setCount(Array.isArray(parsed) ? parsed.length : null);
      } catch {
        setCount(null);
      }
      setLoaded(true);
    } catch (e) {
      onError(String(e));
    }
  };

  useEffect(() => {
    void reload();
    // reload is stable for this scope; no need to add to deps.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const save = async () => {
    try {
      const installed = await importTriggers(text);
      setSavedAt(Date.now());
      setCount(installed);
    } catch (e) {
      onError(String(e));
    }
  };

  if (!loaded) return <div className="settings-loading">loading triggers…</div>;

  return (
    <div className="settings-triggers">
      <div className="settings-triggers-meta">
        <span className="settings-triggers-count">
          {count === null ? 'unknown' : `${count} trigger${count === 1 ? '' : 's'}`}
        </span>
        <span className="settings-triggers-hint">
          json edit; save replaces the whole trigger store
        </span>
      </div>
      <textarea
        className="settings-triggers-text"
        spellCheck={false}
        value={text}
        onChange={(e) => setText(e.target.value)}
      />
      <div className="settings-actions">
        <button type="button" className="settings-btn" onClick={() => void save()}>
          [save]
        </button>
        <button
          type="button"
          className="settings-btn settings-btn-mute"
          onClick={() => void reload()}
        >
          [reload]
        </button>
        {savedAt !== null && <span className="settings-saved">saved.</span>}
      </div>
    </div>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="settings-row">
      <span className="settings-row-label">{label}</span>
      <span className="settings-row-control">{children}</span>
    </label>
  );
}

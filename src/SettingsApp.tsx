import { useEffect, useState } from 'react';
import { emit } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { TopBar } from './components/TopBar';
import { TriggerForm } from './components/TriggerForm';
import { AliasForm } from './components/AliasForm';
import {
  exportAliases,
  exportTriggers,
  getUiConfig,
  importAliases,
  importTriggers,
  listSystemFonts,
  setUiConfig,
  type SystemFontEntry,
  type UiConfig,
} from './lib/session';
import { applyTheme } from './lib/theme';
import { THEMES } from './lib/themes';
import { loadFontStack, loadSystemFont } from './lib/fontLoader';

// Quick-pick chips. The first two are bundled with the app via
// @font-face in styles.css so they always render regardless of what
// is or is not installed on the OS — WKWebView refuses to match user-
// installed fonts by name on recent macOS. The rest are macOS system
// fonts guaranteed to be present. Adding more bundled fonts is a
// matter of dropping a .ttf into src/assets/fonts/, adding a matching
// @font-face block in styles.css, and adding a chip here.
const FONT_PICKS: { label: string; value: string }[] = [
  { label: 'BerkeleyMono', value: '"BerkeleyMono Bundled", Menlo, monospace' },
  { label: 'JetBrainsMono', value: '"JetBrainsMono Bundled", Menlo, monospace' },
  { label: 'Menlo', value: 'Menlo, monospace' },
  { label: 'Monaco', value: 'Monaco, monospace' },
  { label: 'Courier New', value: '"Courier New", monospace' },
];

const PREVIEW_TEXT = 'The quick brown fox 0123456789  |  hp 850/1000  IlOo1';

type TabId = 'general' | 'triggers' | 'aliases';
const TABS: { id: TabId; label: string }[] = [
  { id: 'general', label: 'general' },
  { id: 'triggers', label: 'triggers' },
  { id: 'aliases', label: 'aliases' },
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
        {tab === 'triggers' && (
          <EditorModeSwitcher
            modeKey="triggers"
            formRender={() => (
              <TriggerForm load={exportTriggers} save={importTriggers} onError={setError} />
            )}
            jsonRender={() => (
              <JsonTab
                kind="triggers"
                singular="trigger"
                load={exportTriggers}
                save={importTriggers}
                onError={setError}
              />
            )}
          />
        )}
        {tab === 'aliases' && (
          <EditorModeSwitcher
            modeKey="aliases"
            formRender={() => (
              <AliasForm load={exportAliases} save={importAliases} onError={setError} />
            )}
            jsonRender={() => (
              <JsonTab
                kind="aliases"
                singular="alias"
                plural="aliases"
                load={exportAliases}
                save={importAliases}
                onError={setError}
              />
            )}
          />
        )}
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
  const [systemFonts, setSystemFonts] = useState<SystemFontEntry[]>([]);
  const [fontFilter, setFontFilter] = useState('');
  const [showOnlyMono, setShowOnlyMono] = useState(true);

  // Pull the system font catalog from the backend (font-kit) once on
  // mount. Sorted + deduped server-side; we just filter client-side.
  useEffect(() => {
    void listSystemFonts().then(setSystemFonts);
  }, []);

  // Whenever the live font_family value mentions a system family,
  // inject its @font-face so the preview block actually renders it.
  useEffect(() => {
    if (config?.font_family) loadFontStack(config.font_family);
  }, [config?.font_family]);

  const filteredFonts = systemFonts
    .filter((f) => !showOnlyMono || f.monospace)
    .filter((f) => f.family.toLowerCase().includes(fontFilter.toLowerCase()))
    .slice(0, 200);

  const pickSystemFont = (family: string) => {
    loadSystemFont(family);
    update({ font_family: `"${family}", Menlo, monospace` });
  };

  const update = (patch: Partial<UiConfig>) => {
    setConfig((prev) => (prev ? { ...prev, ...patch } : prev));
  };

  const save = async () => {
    if (!config) return;
    try {
      await setUiConfig(config);
      applyTheme(config.theme);
      // Cross-window emits so the running main window picks up both
      // changes without a relaunch. A window CustomEvent only fires
      // within the settings window; main is a separate webview.
      await emit('vosh://theme-changed', config.theme);
      await emit('vosh://font-changed', {
        family: config.font_family,
        size: config.font_size,
      });
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
        <input
          type="text"
          className="settings-font-input"
          spellCheck={false}
          value={config.font_family}
          placeholder='"BerkeleyMono Bundled", Menlo, monospace'
          onChange={(e) => update({ font_family: e.target.value })}
        />
      </Row>
      <div className="settings-row settings-row-picks">
        <span className="settings-row-label" />
        <div className="settings-font-picks">
          {FONT_PICKS.map((pick) => (
            <button
              key={pick.label}
              type="button"
              className="settings-font-pick"
              onClick={() => update({ font_family: pick.value })}
            >
              {pick.label}
            </button>
          ))}
        </div>
      </div>
      <div className="settings-row settings-row-picks">
        <span className="settings-row-label">system</span>
        <div className="settings-font-system">
          <div className="settings-font-system-controls">
            <input
              type="search"
              className="settings-font-input"
              spellCheck={false}
              placeholder={`filter ${systemFonts.length} installed fonts`}
              value={fontFilter}
              onChange={(e) => setFontFilter(e.target.value)}
            />
            <label className="settings-font-mono">
              <input
                type="checkbox"
                checked={showOnlyMono}
                onChange={(e) => setShowOnlyMono(e.target.checked)}
              />
              monospace only
            </label>
          </div>
          <div className="settings-font-list">
            {systemFonts.length === 0 ? (
              <span className="settings-font-empty">loading installed fonts…</span>
            ) : (
              filteredFonts.map((f) => (
                <button
                  key={f.family}
                  type="button"
                  className="settings-font-list-item"
                  style={{ fontFamily: `"${f.family}", Menlo, monospace` }}
                  onMouseEnter={() => loadSystemFont(f.family)}
                  onFocus={() => loadSystemFont(f.family)}
                  onClick={() => pickSystemFont(f.family)}
                  title={f.family}
                >
                  {f.family}
                </button>
              ))
            )}
          </div>
        </div>
      </div>
      <div className="settings-row">
        <span className="settings-row-label">preview</span>
        <div
          className="settings-font-preview"
          style={{ fontFamily: config.font_family, fontSize: config.font_size }}
        >
          {PREVIEW_TEXT}
        </div>
      </div>
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

interface JsonTabProps {
  // Used for storage key + the loading message ("loading triggers...").
  kind: string;
  // Singular noun for "1 trigger" / "1 alias".
  singular: string;
  // Plural noun for the count + hint. Defaults to `kind`.
  plural?: string;
  load: () => Promise<string>;
  save: (json: string) => Promise<number>;
  onError: (e: string | null) => void;
}

// Shared editor for triggers + aliases. Loads JSON from the backend on
// mount, lets you edit it in a textarea, and posts it back on save.
// Save replaces the whole store for both kinds — matches backend
// semantics.
function JsonTab({ kind, singular, plural, load, save, onError }: JsonTabProps) {
  const [text, setText] = useState<string>('');
  const [count, setCount] = useState<number | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [savedAt, setSavedAt] = useState<number | null>(null);
  const noun = plural ?? kind;

  const reload = async () => {
    try {
      const json = await load();
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
    setLoaded(false);
    setText('');
    setSavedAt(null);
    void reload();
    // reload closes over load/save which are stable per tab.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [kind]);

  const doSave = async () => {
    try {
      const installed = await save(text);
      setSavedAt(Date.now());
      setCount(installed);
    } catch (e) {
      onError(String(e));
    }
  };

  if (!loaded) return <div className="settings-loading">loading {noun}…</div>;

  return (
    <div className="settings-triggers">
      <div className="settings-triggers-meta">
        <span className="settings-triggers-count">
          {count === null ? 'unknown' : `${count} ${count === 1 ? singular : noun}`}
        </span>
        <span className="settings-triggers-hint">
          json edit; save replaces the whole {singular} store
        </span>
      </div>
      <textarea
        className="settings-triggers-text"
        spellCheck={false}
        value={text}
        onChange={(e) => setText(e.target.value)}
      />
      <div className="settings-actions">
        <button type="button" className="settings-btn" onClick={() => void doSave()}>
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

interface SwitcherProps {
  modeKey: string;
  formRender: () => React.ReactNode;
  jsonRender: () => React.ReactNode;
}

// Pill toggle at the top of triggers/aliases tabs that swaps between
// the structured form editor and the raw JSON editor. modeKey is the
// localStorage namespace so each tab remembers its own preference.
function EditorModeSwitcher({ modeKey, formRender, jsonRender }: SwitcherProps) {
  const storageKey = `vosh.settings.${modeKey}.mode`;
  const [mode, setMode] = useState<'form' | 'json'>(() => {
    try {
      return (localStorage.getItem(storageKey) as 'form' | 'json') || 'form';
    } catch {
      return 'form';
    }
  });

  useEffect(() => {
    try {
      localStorage.setItem(storageKey, mode);
    } catch {
      // ignore
    }
  }, [storageKey, mode]);

  return (
    <div className="editor-mode-wrap">
      <div className="editor-mode-toggle">
        <button
          type="button"
          className={`editor-mode-pill${mode === 'form' ? ' is-active' : ''}`}
          onClick={() => setMode('form')}
        >
          form
        </button>
        <button
          type="button"
          className={`editor-mode-pill${mode === 'json' ? ' is-active' : ''}`}
          onClick={() => setMode('json')}
        >
          json
        </button>
      </div>
      {mode === 'form' ? formRender() : jsonRender()}
    </div>
  );
}

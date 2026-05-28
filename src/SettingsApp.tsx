import { useEffect, useState } from 'react';
import { emit } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { TopBar } from './components/TopBar';
import { TriggerForm } from './components/TriggerForm';
import { AliasForm } from './components/AliasForm';
import { LogsTab } from './components/LogsTab';
import { MacrosTab } from './components/MacrosTab';
import { ImportTab } from './components/ImportTab';
import { ProfilesTab } from './components/ProfilesTab';
import { ThemesTab } from './components/ThemesTab';
import {
  broadcastTrackedAffects,
  dockLayoutGet,
  dockLayoutSet,
  exportAliases,
  exportTriggers,
  getUiConfig,
  importAliases,
  importTriggers,
  listSystemFonts,
  setUiConfig,
  subscribeDockLayoutChanged,
  type SystemFontEntry,
  type UiConfig,
} from './lib/session';
import { applyTheme } from './lib/theme';
import { customToAppTheme, setCustomThemes, THEMES } from './lib/themes';
import { loadFontStack, loadSystemFont } from './lib/fontLoader';
import {
  ALL_PANEL_IDS,
  DEFAULT_PANEL_ZONES,
  PANELS,
  panelZonesFromDock,
  panelZonesToDock,
  type PanelId,
  type Zone,
} from './lib/panels';

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

// Coerce a freeform color string to the #rrggbb form `<input type=color>`
// requires. Returns a sane default for empty/unparseable input so the
// picker still opens at something.
function normalizeForColorInput(color: string | null): string {
  if (!color) return '#888888';
  const trimmed = color.trim();
  if (/^#[0-9a-f]{6}$/i.test(trimmed)) return trimmed;
  if (/^#[0-9a-f]{3}$/i.test(trimmed)) {
    return `#${trimmed
      .slice(1)
      .split('')
      .map((c) => c + c)
      .join('')}`;
  }
  return '#888888';
}

type TabId =
  | 'general'
  | 'themes'
  | 'panels'
  | 'profiles'
  | 'triggers'
  | 'aliases'
  | 'macros'
  | 'import'
  | 'logs';
const TABS: { id: TabId; label: string }[] = [
  { id: 'general', label: 'general' },
  { id: 'themes', label: 'themes' },
  { id: 'panels', label: 'panels' },
  { id: 'profiles', label: 'profiles' },
  { id: 'triggers', label: 'triggers' },
  { id: 'aliases', label: 'aliases' },
  { id: 'macros', label: 'macros' },
  { id: 'import', label: 'import' },
  { id: 'logs', label: 'logs' },
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
        setCustomThemes((cfg.custom_themes ?? []).map(customToAppTheme));
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
        {tab === 'themes' && <ThemesTab config={config} setConfig={setConfig} onError={setError} />}
        {tab === 'panels' && <PanelsTab onError={setError} />}
        {tab === 'profiles' && <ProfilesTab onError={setError} />}
        {tab === 'macros' && <MacrosTab onError={setError} />}
        {tab === 'import' && <ImportTab onError={setError} />}
        {tab === 'logs' && <LogsTab onError={setError} />}
      </div>
      <footer className="settings-version">Vosh {__APP_VERSION__}</footer>
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
  const [trackedDraft, setTrackedDraft] = useState('');

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
      await emit('vosh://keep-last-changed', config.keep_last_command);
      await emit('vosh://theme-terminal-colors-changed', config.theme_terminal_colors);
      await broadcastTrackedAffects(config.tracked_affects);
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
      <Row label="terminal">
        <label className="settings-checkbox">
          <input
            type="checkbox"
            checked={config.theme_terminal_colors}
            onChange={(e) => update({ theme_terminal_colors: e.target.checked })}
          />
          <span>tint server output with theme palette</span>
        </label>
      </Row>
      <Row label="updates">
        <label className="settings-checkbox">
          <input
            type="checkbox"
            checked={config.auto_update}
            onChange={(e) => update({ auto_update: e.target.checked })}
          />
          <span>auto-check on launch</span>
        </label>
      </Row>
      <Row label="input">
        <label className="settings-checkbox">
          <input
            type="checkbox"
            checked={config.keep_last_command}
            onChange={(e) => update({ keep_last_command: e.target.checked })}
          />
          <span>keep last command (press enter to repeat)</span>
        </label>
      </Row>
      <Row label="split divider">
        <span className="settings-color-row">
          <input
            type="color"
            // input type=color requires a 7-char #rrggbb; fall back to
            // the theme border read off the CSS var when nothing is set.
            value={normalizeForColorInput(config.split_divider_color)}
            onChange={(e) => update({ split_divider_color: e.target.value })}
            aria-label="split divider color"
          />
          <input
            type="text"
            className="settings-color-text"
            spellCheck={false}
            placeholder="theme default (#rrggbb, rgba, named)"
            value={config.split_divider_color ?? ''}
            onChange={(e) => update({ split_divider_color: e.target.value || null })}
          />
          <button
            type="button"
            className="settings-btn settings-btn-mute"
            onClick={() => update({ split_divider_color: null })}
          >
            [clear]
          </button>
        </span>
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
      <div className="settings-row">
        <span className="settings-row-label">tracked affects</span>
        <div className="settings-tracked">
          <div className="settings-tracked-chips">
            {config.tracked_affects.length === 0 && (
              <span className="settings-font-empty">no affects tracked yet</span>
            )}
            {config.tracked_affects.map((name) => (
              <span key={name} className="settings-tracked-chip">
                <span>{name}</span>
                <button
                  type="button"
                  className="settings-tracked-remove"
                  aria-label={`remove ${name}`}
                  onClick={() =>
                    update({
                      tracked_affects: config.tracked_affects.filter((n) => n !== name),
                    })
                  }
                >
                  ×
                </button>
              </span>
            ))}
          </div>
          <div className="settings-tracked-add">
            <input
              type="text"
              spellCheck={false}
              placeholder="affect name + enter (or comma)"
              value={trackedDraft}
              onChange={(e) => {
                const v = e.target.value;
                if (v.endsWith(',')) {
                  const name = v.slice(0, -1).trim();
                  if (name && !config.tracked_affects.includes(name)) {
                    update({ tracked_affects: [...config.tracked_affects, name] });
                  }
                  setTrackedDraft('');
                } else {
                  setTrackedDraft(v);
                }
              }}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  const name = trackedDraft.trim();
                  if (name && !config.tracked_affects.includes(name)) {
                    update({ tracked_affects: [...config.tracked_affects, name] });
                  }
                  setTrackedDraft('');
                }
              }}
            />
          </div>
        </div>
      </div>
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

interface PanelsTabProps {
  onError: (e: string | null) => void;
}

function PanelsTab({ onError }: PanelsTabProps) {
  const [zones, setZones] = useState<Record<PanelId, Zone>>(DEFAULT_PANEL_ZONES);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    dockLayoutGet()
      .then((entries) => {
        if (cancelled) return;
        setZones(panelZonesFromDock(entries));
        setLoaded(true);
      })
      .catch((e) => onError(String(e)));
    subscribeDockLayoutChanged((entries) => {
      if (cancelled) return;
      setZones(panelZonesFromDock(entries));
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [onError]);

  const update = (id: PanelId, zone: Zone) => {
    setZones((prev) => {
      const next = { ...prev, [id]: zone };
      void dockLayoutSet(panelZonesToDock(next)).catch((e) => onError(String(e)));
      return next;
    });
  };

  const resetDefaults = () => {
    setZones(DEFAULT_PANEL_ZONES);
    void dockLayoutSet(panelZonesToDock(DEFAULT_PANEL_ZONES)).catch((e) => onError(String(e)));
  };

  if (!loaded) return <div className="settings-loading">loading panels…</div>;

  return (
    <>
      <p className="settings-hint">
        Each panel lives in one zone around the terminal. Map is limited to left or right because a
        horizontal map at full width is unusable.
      </p>
      {ALL_PANEL_IDS.map((id) => {
        const meta = PANELS[id];
        return (
          <Row key={id} label={meta.label}>
            <select value={zones[id]} onChange={(e) => update(id, e.target.value as Zone)}>
              {meta.allowedZones.map((z) => (
                <option key={z} value={z}>
                  {z}
                </option>
              ))}
            </select>
            <span className="settings-hint-inline">{meta.description}</span>
          </Row>
        );
      })}
      <div className="settings-actions">
        <button type="button" className="settings-btn settings-btn-mute" onClick={resetDefaults}>
          [reset to defaults]
        </button>
      </div>
    </>
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

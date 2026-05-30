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
  checkForUpdate,
  dockLayoutGet,
  dockLayoutSet,
  exportAliases,
  exportTriggers,
  getUiConfig,
  importAliases,
  importTriggers,
  installUpdateAndRelaunch,
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
  canMovePanelDown,
  canMovePanelUp,
  DEFAULT_PANEL_LAYOUT,
  groupPanels,
  movePanelInZone,
  PANELS,
  panelLayoutFromDock,
  panelLayoutToDock,
  type Align,
  type PanelId,
  type PanelLayout,
  type PanelPlacement,
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
            onClick={() => {
              setError(null);
              setTab(t.id);
            }}
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
        {tab === 'panels' && <PanelsTab config={config} setConfig={setConfig} onError={setError} />}
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
  const [updateStatus, setUpdateStatus] = useState<{
    kind: 'idle' | 'checking' | 'available' | 'current' | 'error' | 'installing';
    msg?: string;
    version?: string;
  }>({ kind: 'idle' });
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
        <span className="settings-updates-row">
          <label className="settings-checkbox">
            <input
              type="checkbox"
              checked={config.auto_update}
              onChange={(e) => update({ auto_update: e.target.checked })}
            />
            <span>auto-check on launch</span>
          </label>
          <button
            type="button"
            className="settings-btn settings-btn-mute"
            disabled={updateStatus.kind === 'checking' || updateStatus.kind === 'installing'}
            onClick={async () => {
              setUpdateStatus({ kind: 'checking' });
              try {
                const result = await checkForUpdate();
                if (result.available) {
                  setUpdateStatus({
                    kind: 'available',
                    version: result.version ?? 'unknown',
                  });
                } else {
                  setUpdateStatus({ kind: 'current' });
                }
              } catch (e) {
                setUpdateStatus({ kind: 'error', msg: String(e) });
              }
            }}
          >
            [check now]
          </button>
          {updateStatus.kind === 'available' && (
            <button
              type="button"
              className="settings-btn"
              onClick={async () => {
                setUpdateStatus({ kind: 'installing' });
                try {
                  await installUpdateAndRelaunch();
                } catch (e) {
                  setUpdateStatus({ kind: 'error', msg: String(e) });
                }
              }}
            >
              [install v{updateStatus.version} + restart]
            </button>
          )}
          <span className="settings-updates-status">
            {updateStatus.kind === 'checking' && 'checking…'}
            {updateStatus.kind === 'current' && 'up to date'}
            {updateStatus.kind === 'installing' && 'installing…'}
            {updateStatus.kind === 'error' && (
              <span className="settings-updates-error">{updateStatus.msg}</span>
            )}
          </span>
        </span>
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
  config: UiConfig | null;
  setConfig: (updater: (prev: UiConfig | null) => UiConfig | null) => void;
  onError: (e: string | null) => void;
}

function PanelsTab({ config, setConfig, onError }: PanelsTabProps) {
  const [layout, setLayout] = useState<PanelLayout>(DEFAULT_PANEL_LAYOUT);
  const [highlightId, setHighlightId] = useState<PanelId | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    dockLayoutGet()
      .then((entries) => {
        if (cancelled) return;
        setLayout(panelLayoutFromDock(entries));
        setLoaded(true);
      })
      .catch((e) => onError(String(e)));
    subscribeDockLayoutChanged((entries) => {
      if (cancelled) return;
      setLayout(panelLayoutFromDock(entries));
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [onError]);

  const update = (id: PanelId, next: PanelPlacement) => {
    setLayout((prev) => {
      const updated: PanelLayout = {
        placements: { ...prev.placements, [id]: next },
        order: prev.order,
      };
      void dockLayoutSet(panelLayoutToDock(updated)).catch((e) => onError(String(e)));
      return updated;
    });
  };

  const move = (id: PanelId, direction: 'up' | 'down') => {
    setLayout((prev) => {
      const updated = movePanelInZone(prev, id, direction);
      if (updated === prev) return prev;
      void dockLayoutSet(panelLayoutToDock(updated)).catch((e) => onError(String(e)));
      return updated;
    });
  };

  const resetDefaults = () => {
    setLayout(DEFAULT_PANEL_LAYOUT);
    void dockLayoutSet(panelLayoutToDock(DEFAULT_PANEL_LAYOUT)).catch((e) => onError(String(e)));
  };

  const sideFillOn = Boolean(config?.side_panels_fill_height);
  const toggleSideFill = () => {
    if (!config) return;
    const next: UiConfig = { ...config, side_panels_fill_height: !config.side_panels_fill_height };
    setConfig(() => next);
    void setUiConfig(next).catch((e) => onError(String(e)));
  };

  if (!loaded) return <div className="settings-loading">loading panels…</div>;

  return (
    <div className="panels-tab">
      <div className="panels-tab-header">
        <span>layout map</span>
        <span className="panels-tab-header-dim">live · changes save automatically</span>
      </div>
      <PanelsPreview layout={layout} highlightId={highlightId} onMove={move} />
      <label className="settings-checkbox panels-side-fill-toggle">
        <input type="checkbox" checked={sideFillOn} onChange={toggleSideFill} />
        <span>side panels span full height (input lives under terminal only)</span>
      </label>
      <div className="panels-rows">
        {layout.order.map((id) => (
          <PanelRow
            key={id}
            id={id}
            placement={layout.placements[id]}
            onChange={(next) => update(id, next)}
            onFocus={() => setHighlightId(id)}
            onBlur={() => setHighlightId(null)}
          />
        ))}
      </div>
      <div className="settings-actions">
        <button type="button" className="settings-btn settings-btn-mute" onClick={resetDefaults}>
          [reset to defaults]
        </button>
      </div>
    </div>
  );
}

function PanelsPreview({
  layout,
  highlightId,
  onMove,
}: {
  layout: PanelLayout;
  highlightId: PanelId | null;
  onMove: (id: PanelId, direction: 'up' | 'down') => void;
}) {
  const g = groupPanels(layout);
  const chip = (id: PanelId, hidden = false) => {
    const canUp = !hidden && canMovePanelUp(layout, id);
    const canDown = !hidden && canMovePanelDown(layout, id);
    return (
      <span
        key={id}
        className={[
          'panels-preview-chip',
          highlightId === id ? 'is-active' : '',
          hidden ? 'is-hidden' : '',
        ]
          .filter(Boolean)
          .join(' ')}
      >
        {PANELS[id].label}
        {!hidden && (canUp || canDown) && (
          <span className="panels-preview-chip-nudge">
            <button
              type="button"
              className="panels-preview-chip-arrow"
              disabled={!canUp}
              aria-label={`move ${PANELS[id].label} up`}
              onClick={() => onMove(id, 'up')}
            >
              ▲
            </button>
            <button
              type="button"
              className="panels-preview-chip-arrow"
              disabled={!canDown}
              aria-label={`move ${PANELS[id].label} down`}
              onClick={() => onMove(id, 'down')}
            >
              ▼
            </button>
          </span>
        )}
      </span>
    );
  };
  const stack = (ids: PanelId[]) =>
    ids.length > 0 ? <div className="panels-preview-stack">{ids.map((id) => chip(id))}</div> : null;
  return (
    <div className="panels-preview" role="img" aria-label="panel layout preview">
      <div className="panels-preview-zone panels-preview-zone-top">
        {g.top.length === 0 ? (
          <span className="panels-preview-empty">top</span>
        ) : (
          g.top.map((id) => chip(id))
        )}
      </div>
      <div className="panels-preview-row">
        <div className="panels-preview-zone panels-preview-zone-side">
          {stack(g.leftTop)}
          <div className="panels-preview-spacer" />
          {stack(g.leftBottom)}
          {g.leftTop.length === 0 && g.leftBottom.length === 0 && (
            <span className="panels-preview-empty">left</span>
          )}
        </div>
        <div className="panels-preview-center">
          <span className="panels-preview-center-label">terminal</span>
          <span className="panels-preview-cursor" aria-hidden>
            ▎
          </span>
        </div>
        <div className="panels-preview-zone panels-preview-zone-side">
          {stack(g.rightTop)}
          <div className="panels-preview-spacer" />
          {stack(g.rightBottom)}
          {g.rightTop.length === 0 && g.rightBottom.length === 0 && (
            <span className="panels-preview-empty">right</span>
          )}
        </div>
      </div>
      <div className="panels-preview-zone panels-preview-zone-bottom">
        {g.bottom.length === 0 ? (
          <span className="panels-preview-empty">bottom</span>
        ) : (
          g.bottom.map((id) => chip(id))
        )}
      </div>
      {g.hidden.length > 0 && (
        <div className="panels-preview-tray">
          <span className="panels-preview-tray-label">hidden</span>
          {g.hidden.map((id) => chip(id, true))}
        </div>
      )}
    </div>
  );
}

function PanelRow({
  id,
  placement,
  onChange,
  onFocus,
  onBlur,
}: {
  id: PanelId;
  placement: PanelPlacement;
  onChange: (next: PanelPlacement) => void;
  onFocus: () => void;
  onBlur: () => void;
}) {
  const meta = PANELS[id];
  const vertical = placement.zone === 'left' || placement.zone === 'right';
  // Fill panels (map) ignore align — they always take the middle of a
  // side zone with other panels stacking above or below.
  const alignDisabled = !vertical || Boolean(meta.fillsSideZone);
  return (
    <div
      className="panels-row"
      onFocusCapture={onFocus}
      onBlurCapture={onBlur}
      onMouseEnter={onFocus}
      onMouseLeave={onBlur}
    >
      <span className="panels-row-name">[{meta.label}]</span>
      <span className="panels-row-arrow" aria-hidden>
        →
      </span>
      <label className="panels-row-control">
        <span className="panels-row-control-label">zone</span>
        <select
          value={placement.zone}
          onChange={(e) => onChange({ ...placement, zone: e.target.value as Zone })}
        >
          {meta.allowedZones.map((z) => (
            <option key={z} value={z}>
              {z}
            </option>
          ))}
        </select>
      </label>
      <label className={`panels-row-control${alignDisabled ? ' is-disabled' : ''}`}>
        <span className="panels-row-control-label">align</span>
        <select
          value={placement.align}
          disabled={alignDisabled}
          onChange={(e) => onChange({ ...placement, align: e.target.value as Align })}
        >
          <option value="top">top</option>
          <option value="bottom">bottom</option>
        </select>
      </label>
      <span className="panels-row-hint">{meta.description}</span>
    </div>
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

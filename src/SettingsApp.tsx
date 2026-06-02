import { useEffect, useRef, useState, type ReactNode } from 'react';
import { tokenizeTemplate } from './lib/vitalsTemplate';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { TopBar } from './components/TopBar';
import { TriggerForm } from './components/TriggerForm';
import { AliasForm } from './components/AliasForm';
import { UnsavedDot } from './components/UnsavedDot';
import { useUnsavedWarning } from './lib/unsaved';
import { LogsTab } from './components/LogsTab';
import { TrackedAffectsEditor } from './components/TrackedAffectsEditor';
import { MacrosTab } from './components/MacrosTab';
import { ImportTab } from './components/ImportTab';
import { ProfilesTab } from './components/ProfilesTab';
import { LoadoutsTab } from './components/LoadoutsTab';
import { ThemesTab } from './components/ThemesTab';
import { loadoutsGetState, subscribeLoadoutsChanged } from './lib/session';
import {
  checkForUpdate,
  DEFAULT_VITALS_CONFIG,
  DEFAULT_VITALS_TEMPLATE,
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
  type VitalsBarStyle,
  type VitalsConfig,
  type VitalsLayout,
  type VitalsPercentColor,
} from './lib/session';
import { applyTheme } from './lib/theme';
import { customToAppTheme, setCustomThemes } from './lib/themes';
import { loadFontStack, loadSystemFont } from './lib/fontLoader';
import {
  canMovePanelDown,
  canMovePanelUp,
  DEFAULT_PANEL_LAYOUT,
  groupPanels,
  isInlineZone,
  movePanelInZone,
  PANELS,
  panelLayoutFromDock,
  panelLayoutToDock,
  zoneLabel,
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

type TabId =
  | 'general'
  | 'themes'
  | 'panels'
  | 'profiles'
  | 'loadouts'
  | 'triggers'
  | 'aliases'
  | 'macros'
  | 'import'
  | 'logs';
// Tab list with a `groupEnd` marker between buckets. The nav renders
// a divider after any tab flagged `groupEnd: true` so the three
// logical groups (look & layout / content / tools) read as distinct
// sections without changing the labels themselves. `pathBOnly` tabs
// only appear when Path B mode is active.
const TABS: { id: TabId; label: string; groupEnd?: boolean; pathBOnly?: boolean }[] = [
  { id: 'general', label: 'general' },
  { id: 'themes', label: 'themes' },
  { id: 'panels', label: 'panels', groupEnd: true },
  { id: 'profiles', label: 'profiles' },
  { id: 'loadouts', label: 'loadouts', pathBOnly: true },
  { id: 'triggers', label: 'triggers' },
  { id: 'aliases', label: 'aliases' },
  { id: 'macros', label: 'macros', groupEnd: true },
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
  const [pathBActive, setPathBActive] = useState(false);

  useEffect(() => {
    let cancelled = false;
    const refresh = () =>
      loadoutsGetState()
        .then((s) => {
          if (!cancelled) setPathBActive(s.path_b_active);
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

  const visibleTabs = TABS.filter((t) => !t.pathBOnly || pathBActive);

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
      <div className="settings-shell">
        <nav className="settings-tabs settings-tabs-vertical">
          {visibleTabs.map((t) => (
            <span key={t.id} className="settings-tab-wrap">
              <button
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
              {t.groupEnd && <span className="settings-tab-divider" aria-hidden="true" />}
            </span>
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
          {tab === 'themes' && (
            <ThemesTab config={config} setConfig={setConfig} onError={setError} />
          )}
          {tab === 'panels' && (
            <PanelsTab config={config} setConfig={setConfig} onError={setError} />
          )}
          {tab === 'profiles' && <ProfilesTab onError={setError} />}
          {tab === 'loadouts' && <LoadoutsTab onError={setError} />}
          {tab === 'macros' && <MacrosTab onError={setError} />}
          {tab === 'import' && <ImportTab onError={setError} />}
          {tab === 'logs' && <LogsTab onError={setError} />}
        </div>
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
  const [fontsState, setFontsState] = useState<'idle' | 'loading' | 'loaded'>('idle');
  const [fontFilter, setFontFilter] = useState('');
  const [showOnlyMono, setShowOnlyMono] = useState(true);

  // System font enumeration is lazy. font-kit's first pass costs
  // 200–500ms because it parses every installed font file to detect
  // the monospace flag; we used to eat that on Settings open, which
  // made the General tab feel sluggish. Now we trigger the fetch
  // only when the user actually engages the font picker (focuses the
  // filter, toggles monospace-only, etc.). The backend caches the
  // result in a OnceLock so the second-and-later call within a
  // session is instant.
  const ensureFontsLoaded = () => {
    if (fontsState !== 'idle') return;
    setFontsState('loading');
    void listSystemFonts()
      .then((list) => {
        setSystemFonts(list);
        setFontsState('loaded');
      })
      .catch(() => setFontsState('idle'));
  };

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

  // Debounced auto-save. Text inputs (font_family, tracked-affect
  // draft, paste-pacing number) can fire many updates in a row when
  // the user is typing; the debounce coalesces them into one
  // setUiConfig call after the typing settles. Toggles/dropdowns
  // also debounce by the same window, which is imperceptible.
  const saveTimerRef = useRef<number | null>(null);
  const scheduleAutoSave = (next: UiConfig) => {
    if (saveTimerRef.current) window.clearTimeout(saveTimerRef.current);
    saveTimerRef.current = window.setTimeout(() => {
      saveTimerRef.current = null;
      void (async () => {
        try {
          await setUiConfig(next);
          // setUiConfig now owns every cross-window emit and dedupes
          // them against the previous snapshot, so this branch only
          // needs the local-window theme refresh + the saved
          // indicator. Saved Phase 7 fans the per-save emit count
          // from 10-11 down to "only fields that actually moved".
          applyTheme(next.theme);
          setSavedAt(Date.now());
        } catch (e) {
          onError(String(e));
        }
      })();
    }, 250);
  };

  const update = (patch: Partial<UiConfig>) => {
    setConfig((prev) => {
      if (!prev) return prev;
      const next = { ...prev, ...patch };
      scheduleAutoSave(next);
      return next;
    });
  };

  // Fade the "saved." indicator after 1.5s so it does not linger as
  // stale chrome long after the user actually saved.
  useEffect(() => {
    if (savedAt === null) return;
    const id = window.setTimeout(() => setSavedAt(null), 1500);
    return () => window.clearTimeout(id);
  }, [savedAt]);

  const close = () => void getCurrentWindow().close();

  if (!config) return <div className="settings-loading">loading…</div>;

  return (
    <>
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
      <Row label="paste pacing">
        <span className="settings-paste-row">
          <input
            type="number"
            className="settings-num-input"
            min={0}
            max={10000}
            step={50}
            value={config.paste_line_delay_ms}
            onChange={(e) => {
              const n = Math.max(0, Math.min(10_000, Math.floor(Number(e.target.value) || 0)));
              update({ paste_line_delay_ms: n });
            }}
            aria-label="delay between pasted lines in milliseconds"
          />
          <span className="settings-paste-unit">ms between lines</span>
          <span className="settings-paste-hint">
            0 = no pacing. higher values dodge MUD flood filters when pasting long scripts.
          </span>
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
              placeholder={
                fontsState === 'loaded'
                  ? `filter ${systemFonts.length} installed fonts`
                  : fontsState === 'loading'
                    ? 'loading installed fonts…'
                    : 'click to load installed fonts'
              }
              value={fontFilter}
              onChange={(e) => setFontFilter(e.target.value)}
              onFocus={ensureFontsLoaded}
            />
            <label className="settings-font-mono">
              <input
                type="checkbox"
                checked={showOnlyMono}
                onChange={(e) => setShowOnlyMono(e.target.checked)}
                onFocus={ensureFontsLoaded}
              />
              monospace only
            </label>
          </div>
          <div className="settings-font-list" onMouseEnter={ensureFontsLoaded}>
            {fontsState === 'idle' ? (
              <span className="settings-font-empty">
                hover or click the filter to load the installed font list
              </span>
            ) : fontsState === 'loading' || systemFonts.length === 0 ? (
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
        <span className="settings-row-label">preview</span>
        <div
          className="settings-font-preview"
          style={{ fontFamily: config.font_family, fontSize: config.font_size }}
        >
          {PREVIEW_TEXT}
        </div>
      </div>
      <div className="settings-actions">
        <button type="button" className="settings-btn settings-btn-mute" onClick={close}>
          [close]
        </button>
        {/* This tab auto-saves on every change (250ms debounce); the
            "saved." pill blinks in to confirm. Static hint makes that
            explicit so the user is not searching for a [save] button
            on the affects / panels / fonts rows. */}
        <span className="settings-autosave-hint">changes save automatically</span>
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
  // Baseline = the last value the backend confirmed (after load or
  // save). Drives the "● unsaved" indicator so a stray space the
  // user typed and undid does not leave the form looking dirty.
  const [baseline, setBaseline] = useState<string>('');
  const [count, setCount] = useState<number | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [savedAt, setSavedAt] = useState<number | null>(null);
  const noun = plural ?? kind;
  const dirty = loaded && text !== baseline;

  // Fade the "saved." indicator after 1.5s. Same treatment the
  // PanelsTab gives its own saved flag — without this it lingers
  // forever as stale chrome long after the actual save landed.
  useEffect(() => {
    if (savedAt === null) return;
    const id = window.setTimeout(() => setSavedAt(null), 1500);
    return () => window.clearTimeout(id);
  }, [savedAt]);

  const reload = async () => {
    try {
      const json = await load();
      setText(json);
      setBaseline(json);
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
    setBaseline('');
    setSavedAt(null);
    void reload();
    // reload closes over load/save which are stable per tab.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [kind]);

  const doSave = async () => {
    try {
      const installed = await save(text);
      setSavedAt(Date.now());
      setBaseline(text);
      setCount(installed);
    } catch (e) {
      onError(String(e));
    }
  };

  // Warn before the user navigates away while there are unsaved
  // textarea edits. Only the JsonTab triggers this; structured
  // forms (TriggerForm / AliasForm) get their own dirty indicator
  // via the same useUnsavedWarning hook.
  useUnsavedWarning(dirty);

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
        {dirty && <UnsavedDot />}
        {savedAt !== null && <span className="settings-saved">saved.</span>}
      </div>
    </div>
  );
}

function Row({ label, children }: { label: string; children: ReactNode }) {
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

type PanelsSubView = 'layout' | 'panes' | 'chips';

function PanelsTab({ config, setConfig, onError }: PanelsTabProps) {
  const [layout, setLayout] = useState<PanelLayout>(DEFAULT_PANEL_LAYOUT);
  const [highlightId, setHighlightId] = useState<PanelId | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [subView, setSubView] = useState<PanelsSubView>(() => {
    try {
      const stored = localStorage.getItem('vosh.settings.panels.subview');
      if (stored === 'panes' || stored === 'chips') return stored;
    } catch {
      // ignore
    }
    return 'layout';
  });

  const pickSubView = (next: PanelsSubView) => {
    setSubView(next);
    try {
      localStorage.setItem('vosh.settings.panels.subview', next);
    } catch {
      // ignore
    }
  };

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
      <div className="panels-subtoggle" role="tablist" aria-label="panels view">
        <button
          type="button"
          role="tab"
          aria-selected={subView === 'layout'}
          className={`panels-subtoggle-btn${subView === 'layout' ? ' is-on' : ''}`}
          onClick={() => pickSubView('layout')}
        >
          layout
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={subView === 'panes'}
          className={`panels-subtoggle-btn${subView === 'panes' ? ' is-on' : ''}`}
          onClick={() => pickSubView('panes')}
        >
          panes
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={subView === 'chips'}
          className={`panels-subtoggle-btn${subView === 'chips' ? ' is-on' : ''}`}
          onClick={() => pickSubView('chips')}
        >
          chips
        </button>
      </div>

      {subView === 'layout' && (
        <PanelsLayoutSubview
          layout={layout}
          highlightId={highlightId}
          setHighlightId={setHighlightId}
          onUpdate={update}
          onMove={move}
          sideFillOn={sideFillOn}
          onToggleSideFill={toggleSideFill}
          onResetDefaults={resetDefaults}
        />
      )}
      {subView === 'panes' && (
        <PanelsPanesSubview config={config} setConfig={setConfig} onError={onError} />
      )}
      {subView === 'chips' && (
        <PanelsChipsSubview
          layout={layout}
          onUpdate={update}
          config={config}
          setConfig={setConfig}
          onError={onError}
        />
      )}
    </div>
  );
}

// === Layout sub-view ====================================================
// The visual placement view: the layout map preview, per-panel zone +
// align dropdowns, the side-panels-fill-height toggle, and a reset
// button. This is the pre-redesign Panels tab body, just split out so
// the sub-toggle can show one view at a time.
interface LayoutSubviewProps {
  layout: PanelLayout;
  highlightId: PanelId | null;
  setHighlightId: (id: PanelId | null) => void;
  onUpdate: (id: PanelId, next: PanelPlacement) => void;
  onMove: (id: PanelId, direction: 'up' | 'down') => void;
  sideFillOn: boolean;
  onToggleSideFill: () => void;
  onResetDefaults: () => void;
}
function PanelsLayoutSubview({
  layout,
  highlightId,
  setHighlightId,
  onUpdate,
  onMove,
  sideFillOn,
  onToggleSideFill,
  onResetDefaults,
}: LayoutSubviewProps) {
  return (
    <>
      <div className="panels-tab-header">
        <span>layout map</span>
        <span className="panels-tab-header-dim">live · changes save automatically</span>
      </div>
      <PanelsPreview layout={layout} highlightId={highlightId} onMove={onMove} />
      <label className="settings-checkbox panels-side-fill-toggle">
        <input type="checkbox" checked={sideFillOn} onChange={onToggleSideFill} />
        <span>side panels span full height (input lives under terminal only)</span>
      </label>
      <div className="panels-section-header">
        <span>panel placement</span>
        <span className="panels-tab-header-dim">zone and alignment for each panel</span>
      </div>
      <div className="panels-rows">
        {layout.order.map((id) => (
          <PanelRow
            key={id}
            id={id}
            placement={layout.placements[id]}
            onChange={(next) => onUpdate(id, next)}
            onFocus={() => setHighlightId(id)}
            onBlur={() => setHighlightId(null)}
          />
        ))}
      </div>
      <div className="settings-actions">
        <button type="button" className="settings-btn settings-btn-mute" onClick={onResetDefaults}>
          [reset to defaults]
        </button>
      </div>
    </>
  );
}

// === Panes sub-view =====================================================
// Per-pane content config as a collapsible accordion. Currently lists
// only panes with content config: vitals (template + glyphs + presets)
// and affects (tracked-affect editor). Tick and mud time appear in the
// "chips" sub-view because their host-routing is the primary knob;
// their content config (interval, format) lands in a follow-up commit.
// Map / chat / group / roomstrip have no content config yet, so they
// are not listed at all rather than rendering an empty row.
interface PanesSubviewProps {
  config: UiConfig | null;
  setConfig: (updater: (prev: UiConfig | null) => UiConfig | null) => void;
  onError: (e: string | null) => void;
}
function PanelsPanesSubview({ config, setConfig, onError }: PanesSubviewProps) {
  const [open, setOpen] = useState<Set<PanelId>>(new Set<PanelId>(['vitals']));
  const toggle = (id: PanelId) => {
    setOpen((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };
  if (!config) return <div className="settings-loading">loading panes…</div>;
  const update = (patch: Partial<UiConfig>) => {
    const next: UiConfig = { ...config, ...patch };
    setConfig(() => next);
    void setUiConfig(next).catch((e) => onError(String(e)));
  };
  return (
    <div className="panes-accordion">
      <PaneAccordionRow
        id="vitals"
        label="vitals"
        description="your hp / mn / mv bars and any GMCP fields you template in"
        open={open.has('vitals')}
        onToggle={() => toggle('vitals')}
      >
        <VitalsConfigSection config={config} setConfig={setConfig} onError={onError} />
      </PaneAccordionRow>
      <PaneAccordionRow
        id="affects"
        label="affects"
        description="tracked-affect pills with remaining duration"
        open={open.has('affects')}
        onToggle={() => toggle('affects')}
      >
        <TrackedAffectsEditor config={config} update={update} />
      </PaneAccordionRow>
    </div>
  );
}

interface AccordionRowProps {
  id: PanelId;
  label: string;
  description: string;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}
function PaneAccordionRow({ label, description, open, onToggle, children }: AccordionRowProps) {
  return (
    <div className={`panes-accordion-row${open ? ' is-open' : ''}`}>
      <button
        type="button"
        className="panes-accordion-head"
        aria-expanded={open}
        onClick={onToggle}
      >
        <span className="panes-accordion-name">{label}</span>
        <span className="panes-accordion-desc">{description}</span>
        <span className="panes-accordion-caret" aria-hidden="true">
          {open ? '▾' : '▸'}
        </span>
      </button>
      {open && <div className="panes-accordion-body">{children}</div>}
    </div>
  );
}

// === Chips sub-view =====================================================
// Tick, MUD time, and moons-phase host routing. These are the small
// ride-along elements in Vosh's chrome: they don't take their own
// panel slot, they hang off a host (vitals, roomstrip, affects, the
// statusbar). This view lets you pick a host per chip in one place,
// plus the moons-position knob (which is statusbar chrome only).
//
// Per-chip content config (tick interval / auto-fire / sound /
// warning, mud-time format) is intentionally left out of this commit
// so the structural change stays focused; it surfaces here in a
// follow-up.
interface ChipsSubviewProps {
  layout: PanelLayout;
  onUpdate: (id: PanelId, next: PanelPlacement) => void;
  config: UiConfig | null;
  setConfig: (updater: (prev: UiConfig | null) => UiConfig | null) => void;
  onError: (e: string | null) => void;
}
function PanelsChipsSubview({ layout, onUpdate, config, setConfig, onError }: ChipsSubviewProps) {
  const setHost = (id: 'tick' | 'time', zone: Zone) => {
    onUpdate(id, { ...layout.placements[id], zone });
  };
  return (
    <>
      <div className="panels-tab-header">
        <span>chip routing</span>
        <span className="panels-tab-header-dim">
          where the small ride-along elements live in the chrome
        </span>
      </div>
      <div className="chips-rows">
        <ChipRow
          id="tick"
          label="tick"
          description="tick countdown"
          zone={layout.placements.tick.zone}
          onChange={(z) => setHost('tick', z)}
        />
        <ChipRow
          id="time"
          label="mud time"
          description="in-game clock from GMCP World.Time"
          zone={layout.placements.time.zone}
          onChange={(z) => setHost('time', z)}
        />
        {config && (
          <MoonsRow
            position={config.moons_position}
            onChange={(p) => {
              const next: UiConfig = { ...config, moons_position: p };
              setConfig(() => next);
              void setUiConfig(next).catch((e) => onError(String(e)));
            }}
          />
        )}
      </div>
      <div className="chips-tab-footnote">
        Per-chip settings (tick interval, auto-fire, warning sound, mud-time format) are coming in a
        follow-up. Today: use slash commands like <code>#tick interval 30</code> for tick tuning.
      </div>
    </>
  );
}

interface ChipRowProps {
  id: PanelId;
  label: string;
  description: string;
  zone: Zone;
  onChange: (zone: Zone) => void;
}
function ChipRow({ id, label, description, zone, onChange }: ChipRowProps) {
  const meta = PANELS[id];
  return (
    <div className="chips-row">
      <span className="chips-row-name">[{label}]</span>
      <span className="chips-row-arrow" aria-hidden="true">
        →
      </span>
      <label className="chips-row-control">
        <span className="chips-row-control-label">host</span>
        <select value={zone} onChange={(e) => onChange(e.target.value as Zone)}>
          {meta.allowedZones.map((z) => (
            <option key={z} value={z}>
              {zoneLabel(z)}
            </option>
          ))}
        </select>
      </label>
      <span className="chips-row-hint">{description}</span>
    </div>
  );
}

interface MoonsRowProps {
  position: UiConfig['moons_position'];
  onChange: (next: UiConfig['moons_position']) => void;
}
function MoonsRow({ position, onChange }: MoonsRowProps) {
  return (
    <div className="chips-row">
      <span className="chips-row-name">[moons]</span>
      <span className="chips-row-arrow" aria-hidden="true">
        →
      </span>
      <label className="chips-row-control">
        <span className="chips-row-control-label">position</span>
        <select
          value={position}
          onChange={(e) => onChange(e.target.value as UiConfig['moons_position'])}
        >
          <option value="right-edge">far right (historical)</option>
          <option value="before-time">left of tick/time chip</option>
          <option value="after-time">right of tick/time chip</option>
        </select>
      </label>
      <span className="chips-row-hint">moon-phase glyphs in the status bar</span>
    </div>
  );
}

// Vitals appearance section. Lives at the bottom of the Panels tab so
// the user can dial in column toggles, custom bar glyphs, and bar
// width next to the rest of the panel-related layout knobs. Each
// edit autosaves through setUiConfig and broadcasts via
// vosh://vitals-config-changed so VitalsBar redraws live.
const VITALS_GLYPH_PRESETS: { label: string; filled: string; empty: string }[] = [
  { label: 'parallelogram', filled: '▰', empty: '▱' },
  { label: 'block', filled: '█', empty: '░' },
  { label: 'heavy/light', filled: '━', empty: '─' },
  { label: 'circle', filled: '●', empty: '○' },
  { label: 'square', filled: '◼', empty: '◻' },
  { label: 'vertical bar', filled: '▮', empty: '▯' },
];

interface VitalsConfigPreset {
  label: string;
  description: string;
  patch: Partial<VitalsConfig>;
}
const VITALS_PRESETS: VitalsConfigPreset[] = [
  {
    label: 'bars',
    description: 'all four columns on, default glyphs',
    patch: {
      show_bar: true,
      show_percent: true,
      show_numeric: true,
      show_delta: true,
    },
  },
  {
    label: 'compact',
    description: 'no bar, percent + numeric, no delta',
    patch: {
      show_bar: false,
      show_percent: true,
      show_numeric: true,
      show_delta: false,
    },
  },
  {
    label: 'numeric',
    description: 'just hp 850/1000 style readouts',
    patch: {
      show_bar: false,
      show_percent: false,
      show_numeric: true,
      show_delta: false,
    },
  },
  {
    label: 'percent',
    description: 'just hp 75% readouts',
    patch: {
      show_bar: false,
      show_percent: true,
      show_numeric: false,
      show_delta: false,
    },
  },
];

function VitalsConfigSection({
  config,
  setConfig,
  onError,
}: {
  config: UiConfig | null;
  setConfig: (updater: (prev: UiConfig | null) => UiConfig | null) => void;
  onError: (e: string | null) => void;
}) {
  if (!config) return null;
  const v = config.vitals;
  const apply = (patch: Partial<VitalsConfig>) => {
    const nextVitals: VitalsConfig = { ...v, ...patch };
    const next: UiConfig = { ...config, vitals: nextVitals };
    setConfig(() => next);
    void setUiConfig(next).catch((e) => onError(String(e)));
  };
  return (
    <section className="panels-vitals">
      <div className="panels-vitals-header">
        <span>vitals</span>
        <span className="panels-tab-header-dim">how the hp / mn / mv rows render</span>
      </div>
      <div className="panels-vitals-presets" role="group" aria-label="vitals presets">
        {VITALS_PRESETS.map((preset) => (
          <button
            key={preset.label}
            type="button"
            className="settings-btn settings-btn-mute panels-vitals-preset-chip"
            title={preset.description}
            onClick={() => apply(preset.patch)}
          >
            [{preset.label}]
          </button>
        ))}
      </div>
      <div className="panels-vitals-toggles">
        <Toggle label="bar glyphs" checked={v.show_bar} onChange={(c) => apply({ show_bar: c })} />
        <Toggle
          label="percent"
          checked={v.show_percent}
          onChange={(c) => apply({ show_percent: c })}
        />
        <Toggle
          label="numeric"
          checked={v.show_numeric}
          onChange={(c) => apply({ show_numeric: c })}
        />
        <Toggle
          label="per-tick delta"
          checked={v.show_delta}
          onChange={(c) => apply({ show_delta: c })}
        />
      </div>
      <div className={`panels-vitals-glyphs${v.show_bar ? '' : ' is-disabled'}`}>
        <label className="panels-vitals-glyph-field">
          <span className="panels-vitals-glyph-label">style</span>
          <select
            value={v.bar_style}
            disabled={!v.show_bar}
            onChange={(e) => apply({ bar_style: e.target.value as VitalsBarStyle })}
          >
            <option value="solid">solid (glyph per cell)</option>
            <option value="track">track (smooth CSS bar)</option>
          </select>
        </label>
        <label className="panels-vitals-glyph-field">
          <span className="panels-vitals-glyph-label">filled</span>
          <input
            type="text"
            className="panels-vitals-glyph-input"
            value={v.bar_filled}
            disabled={!v.show_bar || v.bar_style === 'track'}
            spellCheck={false}
            onChange={(e) =>
              apply({ bar_filled: e.target.value.length > 0 ? e.target.value : '▰' })
            }
            aria-label="filled glyph"
          />
        </label>
        <label className="panels-vitals-glyph-field">
          <span className="panels-vitals-glyph-label">empty</span>
          <input
            type="text"
            className="panels-vitals-glyph-input"
            value={v.bar_empty}
            disabled={!v.show_bar || v.bar_style === 'track'}
            spellCheck={false}
            onChange={(e) => apply({ bar_empty: e.target.value.length > 0 ? e.target.value : '▱' })}
            aria-label="empty glyph"
          />
        </label>
        <label className="panels-vitals-glyph-field">
          <span className="panels-vitals-glyph-label">width</span>
          <input
            type="number"
            className="settings-num-input"
            min={4}
            max={60}
            step={1}
            value={v.bar_width}
            disabled={!v.show_bar}
            onChange={(e) => {
              const n = Math.max(4, Math.min(60, Math.floor(Number(e.target.value) || 20)));
              apply({ bar_width: n });
            }}
            aria-label="bar width in glyphs"
          />
        </label>
      </div>
      <div className="panels-vitals-layout">
        <label className="panels-vitals-glyph-field">
          <span className="panels-vitals-glyph-label">layout</span>
          <select
            value={v.layout}
            disabled={v.template_enabled}
            onChange={(e) => apply({ layout: e.target.value as VitalsLayout })}
          >
            <option value="stacked">stacked rows (hp / mn / mv per row)</option>
            <option value="inline">inline (one row, prompt-style)</option>
          </select>
        </label>
        <label className="panels-vitals-glyph-field">
          <span className="panels-vitals-glyph-label">percent color</span>
          <select
            value={v.percent_color}
            onChange={(e) => apply({ percent_color: e.target.value as VitalsPercentColor })}
          >
            <option value="fill">per-vital (matches bar)</option>
            <option value="gradient">red → green gradient</option>
          </select>
        </label>
      </div>
      <VitalsTemplateEditor config={v} apply={apply} />
      <div
        className={`panels-vitals-quickpicks${
          !v.show_bar || v.bar_style === 'track' ? ' is-disabled' : ''
        }`}
      >
        <span className="panels-vitals-quickpicks-label">quick picks</span>
        {VITALS_GLYPH_PRESETS.map((preset) => {
          const active = preset.filled === v.bar_filled && preset.empty === v.bar_empty;
          return (
            <button
              key={preset.label}
              type="button"
              className={`panels-vitals-glyph-chip${active ? ' is-active' : ''}`}
              title={preset.label}
              disabled={!v.show_bar || v.bar_style === 'track'}
              onClick={() => apply({ bar_filled: preset.filled, bar_empty: preset.empty })}
            >
              {preset.filled.repeat(3)}
              {preset.empty.repeat(3)}
            </button>
          );
        })}
      </div>
      <div className="panels-vitals-preview" aria-label="vitals preview">
        <VitalsPreview config={v} />
      </div>
      <div className="settings-actions">
        <button
          type="button"
          className="settings-btn settings-btn-mute"
          onClick={() => apply(DEFAULT_VITALS_CONFIG)}
        >
          [reset vitals]
        </button>
      </div>
    </section>
  );
}

function VitalsTemplateEditor({
  config,
  apply,
}: {
  config: VitalsConfig;
  apply: (patch: Partial<VitalsConfig>) => void;
}) {
  // Plain div + Toggle (not <details>/<summary>) because nesting an
  // interactive checkbox inside <summary> caused the section to
  // collapse every time the user clicked the checkbox — the summary's
  // default toggle and the checkbox's onChange fired together. With
  // the body always rendered and conditionally disabled, the layout
  // is predictable and the toggle behaves as the single source of
  // truth.
  return (
    <div className="panels-vitals-template">
      <Toggle
        label="custom template (overrides layout)"
        checked={config.template_enabled}
        onChange={(c) => apply({ template_enabled: c })}
      />
      <div
        className={`panels-vitals-template-body${config.template_enabled ? '' : ' is-disabled'}`}
      >
        <textarea
          className="panels-vitals-template-input"
          spellCheck={false}
          value={config.template}
          disabled={!config.template_enabled}
          onChange={(e) => apply({ template: e.target.value })}
          rows={2}
          aria-label="vitals template"
        />
        <div className="panels-vitals-template-help">
          <span className="panels-vitals-template-help-title">curated tokens:</span>
          <code>%hp</code> <code>%mhp</code> <code>%mn</code> <code>%mmn</code> <code>%mv</code>{' '}
          <code>%mmv</code> <code>%pct_hp</code> <code>%pct_mn</code> <code>%pct_mv</code>{' '}
          <code>%dhp</code> <code>%dmn</code> <code>%dmv</code> <code>%bar_hp</code>{' '}
          <code>%bar_mn</code> <code>%bar_mv</code> <code>%tick</code> <code>%time</code>
          <br />
          <span className="panels-vitals-template-help-title">pass-through:</span>
          any field your server actually pushes via <code>Char.Vitals</code> or{' '}
          <code>Char.Worth</code> can be used as <code>%fieldname</code>. Available fields depend on
          the server — try a token; if the field exists, it renders; if not, it shows in red.
          <br />
          <span className="panels-vitals-template-help-title">explicit braces:</span>
          use <code>{'%{name}'}</code> when the token is immediately followed by letters / digits
          that would otherwise be consumed by the greedy match. e.g. <code>{'%{cp}cp'}</code>{' '}
          renders the <code>cp</code> field then literal <code>cp</code>, whereas <code>%cpcp</code>{' '}
          would look up the field <code>cpcp</code>. <code>%%</code> = literal <code>%</code>.
        </div>
        <div className="settings-actions">
          <button
            type="button"
            className="settings-btn settings-btn-mute"
            disabled={!config.template_enabled}
            onClick={() => apply({ template: DEFAULT_VITALS_TEMPLATE })}
          >
            [reset template]
          </button>
        </div>
      </div>
    </div>
  );
}

function Toggle({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (next: boolean) => void;
}) {
  return (
    <label className="settings-checkbox panels-vitals-toggle">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span>{label}</span>
    </label>
  );
}

// Static preview that mirrors the real VitalsBar render. Three vitals
// at 75 / 50 / 25% so the user can see the effect of toggles, glyphs,
// layout, and percent color in one glance.
// Tiny inline track-bar component for the Settings preview. Mirrors
// the `TrackBar` in VitalsBar.tsx; kept local to avoid a circular
// dep (VitalsBar imports session, SettingsApp imports both).
function PreviewTrackBar({ value, cells, color }: { value: number; cells: number; color: string }) {
  const pct = Math.max(0, Math.min(100, value));
  return (
    <span
      className="vitals-glyphs vitals-glyphs-track"
      style={{ width: `${cells}ch` }}
      aria-hidden="true"
    >
      <span className="vitals-glyphs-track-fill" style={{ width: `${pct}%`, background: color }} />
    </span>
  );
}

interface PreviewSample {
  label: string;
  cur: number;
  max: number;
  value: number;
  delta: number;
  color: string;
}

// Token reference for the Settings preview render path. Imported
// from session.ts wouldn't pull the live values; we mock them with
// the same sample fixture used by stacked/inline previews.
function previewTemplate(
  config: VitalsConfig,
  sample: PreviewSample[],
  track: boolean,
  gradient: boolean,
): ReactNode {
  const width = Math.max(4, Math.min(60, config.bar_width));
  const get = (label: string) => sample.find((s) => s.label === label)!;
  const resolveToken = (name: string): ReactNode => {
    switch (name) {
      case 'hp':
        return get('hp').cur;
      case 'mhp':
        return get('hp').max;
      case 'mn':
        return get('mn').cur;
      case 'mmn':
        return get('mn').max;
      case 'mv':
        return get('mv').cur;
      case 'mmv':
        return get('mv').max;
      case 'pct_hp':
      case 'pct_mn':
      case 'pct_mv': {
        const s = get(name.slice(4));
        const c = gradient ? s.color : 'var(--c-accent)';
        return <span style={{ color: c }}>{s.value}%</span>;
      }
      case 'dhp':
      case 'dmn':
      case 'dmv': {
        const s = get(name.slice(1));
        if (s.delta === 0) return null;
        const cls = s.delta > 0 ? 'vitals-delta vitals-delta-up' : 'vitals-delta vitals-delta-down';
        return (
          <span className={cls}>
            {s.delta > 0 ? '+' : ''}
            {s.delta}
          </span>
        );
      }
      case 'bar_hp':
      case 'bar_mn':
      case 'bar_mv': {
        const s = get(name.slice(4));
        const fill = gradient ? s.color : 'var(--c-accent)';
        if (track) {
          return <PreviewTrackBar value={s.value} cells={width} color={fill} />;
        }
        const filled = Math.round((s.value / 100) * width);
        const empty = width - filled;
        return (
          <span className="vitals-glyphs" aria-hidden="true">
            <span style={{ color: fill }}>{config.bar_filled.repeat(filled)}</span>
            <span className="vitals-empty">{config.bar_empty.repeat(empty)}</span>
          </span>
        );
      }
      case 'tick':
        return '19s';
      case 'time':
        return '3PM';
      default:
        // Preview has no access to live Char.Vitals / Char.Worth, so
        // any pass-through token always renders as the red `%name`
        // literal here — mirroring exactly what the runtime renderer
        // does when the server hasn't pushed that field. Lets the
        // user spot in advance which tokens won't resolve.
        return <span className="vitals-template-unknown">%{name}</span>;
    }
  };
  const segments = tokenizeTemplate(config.template);
  return segments.map((seg, i) => {
    if (seg.kind === 'text') return <span key={i}>{seg.text}</span>;
    const node = resolveToken(seg.name);
    if (node === null || node === undefined) return null;
    return <span key={i}>{node}</span>;
  });
}

function VitalsPreview({ config }: { config: VitalsConfig }) {
  const sample = [
    { label: 'hp', cur: 750, max: 1000, value: 75, delta: 12, color: '#5fdc6a' },
    { label: 'mn', cur: 150, max: 300, value: 50, delta: -8, color: '#dccd44' },
    { label: 'mv', cur: 50, max: 200, value: 25, delta: 0, color: '#dc8a44' },
  ];
  const width = Math.max(4, Math.min(60, config.bar_width));
  const gradient = config.percent_color === 'gradient';
  const track = config.bar_style === 'track';

  // Template preview takes priority because the template field
  // overrides layout. Renders sample values for each token using the
  // same look the runtime VitalsBar would produce.
  if (config.template_enabled) {
    return (
      <div className="vitals-bar vitals-bar-template">
        <div className="vitals-row vitals-row-template">
          {previewTemplate(config, sample, track, gradient)}
        </div>
      </div>
    );
  }

  if (config.layout === 'inline') {
    return (
      <div className="vitals-bar vitals-bar-inline">
        <div className="vitals-row vitals-row-inline">
          {sample.map((s) => (
            <span key={s.label} className="vitals-inline-chip">
              {config.show_numeric && <span className="vitals-numeric">{s.cur}</span>}
              {config.show_percent && (
                <span
                  className="vitals-inline-pct"
                  style={{ color: gradient ? s.color : 'var(--c-accent)' }}
                >
                  ({s.value}%)
                </span>
              )}
              <span className="vitals-inline-letter">{s.label[0]}</span>
              {config.show_delta && s.delta !== 0 && (
                <span
                  className={`vitals-delta${s.delta > 0 ? ' vitals-delta-up' : ' vitals-delta-down'}`}
                >
                  {s.delta > 0 ? '+' : ''}
                  {s.delta}
                </span>
              )}
            </span>
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="vitals-bar">
      {sample.map((s) => {
        const filled = Math.round((s.value / 100) * width);
        const empty = width - filled;
        const percentColor = gradient ? s.color : 'var(--c-accent)';
        return (
          <div key={s.label} className="vitals-row">
            <span className="vitals-label">{s.label}</span>
            {config.show_bar &&
              (track ? (
                <PreviewTrackBar value={s.value} cells={width} color={percentColor} />
              ) : (
                <span className="vitals-glyphs" aria-hidden="true">
                  <span style={{ color: percentColor }}>{config.bar_filled.repeat(filled)}</span>
                  <span className="vitals-empty">{config.bar_empty.repeat(empty)}</span>
                </span>
              ))}
            {config.show_percent && (
              <span className="vitals-percent" style={{ color: percentColor }}>
                {s.value}%
              </span>
            )}
            {config.show_numeric && (
              <span className="vitals-numeric">
                {s.cur}/{s.max}
              </span>
            )}
            {config.show_delta && (
              <span className="vitals-delta-slot">
                {s.delta !== 0 && (
                  <span
                    className={`vitals-delta${s.delta > 0 ? ' vitals-delta-up' : ' vitals-delta-down'}`}
                  >
                    {s.delta > 0 ? '+' : ''}
                    {s.delta}
                  </span>
                )}
              </span>
            )}
          </div>
        );
      })}
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
  // Map each potential host id ("vitals", "roomstrip", ...) to the list
  // of panels currently embedded inside it via an `in:<host>` zone. The
  // host chip shows these as small "+ tick" attachments so the user can
  // see at a glance where the embedded panel ended up.
  const embeddedBy: Partial<Record<PanelId, PanelId[]>> = {};
  for (const id of layout.order) {
    const z = layout.placements[id].zone;
    if (!isInlineZone(z)) continue;
    const hostId = z.slice('in:'.length) as PanelId;
    (embeddedBy[hostId] ??= []).push(id);
  }
  const chip = (id: PanelId, hidden = false) => {
    const canUp = !hidden && canMovePanelUp(layout, id);
    const canDown = !hidden && canMovePanelDown(layout, id);
    const guests = embeddedBy[id] ?? [];
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
        {guests.map((gid) => (
          <span key={gid} className="panels-preview-chip-embed">
            + {PANELS[gid].label}
          </span>
        ))}
        {!hidden && (canUp || canDown) && (
          <span className="panels-preview-chip-nudge">
            <button
              type="button"
              className="panels-preview-chip-arrow"
              disabled={!canUp}
              title={`move ${PANELS[id].label} earlier in this zone`}
              aria-label={`move ${PANELS[id].label} up`}
              onClick={() => onMove(id, 'up')}
            >
              ▲
            </button>
            <button
              type="button"
              className="panels-preview-chip-arrow"
              disabled={!canDown}
              title={`move ${PANELS[id].label} later in this zone`}
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
      {/* The statusbar is permanent chrome, not a movable panel, so it has
          no chip of its own in the preview. When a panel is embedded in
          it (e.g. tick via in:statusbar), render a thin label row so the
          user can see where the embedded panel ended up. */}
      {(() => {
        const guests = layout.order.filter((id) => layout.placements[id].zone === 'in:statusbar');
        if (guests.length === 0) return null;
        return (
          <div className="panels-preview-zone panels-preview-zone-statusbar">
            <span className="panels-preview-statusbar-label">statusbar</span>
            {guests.map((gid) => (
              <span key={gid} className="panels-preview-chip-embed">
                + {PANELS[gid].label}
              </span>
            ))}
          </div>
        );
      })()}
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
  // side zone with other panels stacking above or below. Inline-host
  // zones also have no align: the panel renders embedded inside its
  // host, so there's no column to anchor in. For both these cases we
  // hide the align control entirely below.
  const inline = isInlineZone(placement.zone);
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
              {zoneLabel(z)}
            </option>
          ))}
        </select>
      </label>
      {vertical && !meta.fillsSideZone && !inline ? (
        <label className="panels-row-control">
          <span className="panels-row-control-label">align</span>
          <select
            value={placement.align}
            onChange={(e) => onChange({ ...placement, align: e.target.value as Align })}
          >
            <option value="top">top</option>
            <option value="bottom">bottom</option>
          </select>
        </label>
      ) : (
        // Alignment is meaningful only in left/right zones. For top /
        // bottom / hidden / inline-host / fill zones we used to render
        // a greyed-out dropdown that read as confusing chrome; render
        // a placeholder span instead so the grid columns still line up
        // with other rows.
        <span className="panels-row-control panels-row-control-placeholder" aria-hidden />
      )}
      <span className="panels-row-hint">{meta.description}</span>
    </div>
  );
}

interface SwitcherProps {
  modeKey: string;
  formRender: () => ReactNode;
  jsonRender: () => ReactNode;
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
        <span className="editor-mode-label">editor:</span>
        <button
          type="button"
          className={`editor-mode-pill${mode === 'form' ? ' is-active' : ''}`}
          title="structured form: one field per setting"
          onClick={() => setMode('form')}
        >
          form
        </button>
        <button
          type="button"
          className={`editor-mode-pill${mode === 'json' ? ' is-active' : ''}`}
          title="raw JSON: bulk edit, paste from clipboard"
          onClick={() => setMode('json')}
        >
          json
        </button>
      </div>
      {mode === 'form' ? formRender() : jsonRender()}
    </div>
  );
}

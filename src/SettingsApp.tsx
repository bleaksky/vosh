import { useEffect, useRef, useState, type ReactNode } from 'react';
import { tokenizeTemplate } from './lib/vitalsTemplate';
import { colorForVital, colorForPercent } from './lib/vitalsColor';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { TopBar } from './components/TopBar';
import { TriggerForm } from './components/TriggerForm';
import { AliasForm } from './components/AliasForm';
import { UnsavedDot } from './components/UnsavedDot';
import { CodeEditor } from './components/CodeEditor';
import { useUnsavedWarning } from './lib/unsaved';
import { LogsTab } from './components/LogsTab';
import { TrackedAffectsEditor } from './components/TrackedAffectsEditor';
import { MacrosTab } from './components/MacrosTab';
import { ImportTab } from './components/ImportTab';
import { ProfilesTab } from './components/ProfilesTab';
import { LoadoutsTab } from './components/LoadoutsTab';
import { ThemesTab } from './components/ThemesTab';
import {
  loadoutsGetState,
  subscribeLoadoutsChanged,
  subscribeTickConfigChanged,
  tickGetConfig,
  tickSetConfig,
  type TickConfig,
} from './lib/session';
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
  type VitalsInlineStyle,
  type VitalsLayout,
  type VitalsPercentColor,
  type VitalsPercentColorMode,
} from './lib/session';
import { applyTheme } from './lib/theme';
import { customToAppTheme, setCustomThemes } from './lib/themes';
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
        <label className="settings-checkbox">
          <input
            type="checkbox"
            checked={config.spellcheck_prompt}
            onChange={(e) => update({ spellcheck_prompt: e.target.checked })}
          />
          <span>spell check chat lines (say / tell / chat / ooc / clan / etc.)</span>
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
      <CodeEditor
        className="settings-triggers-text"
        ariaLabel={`${noun} json store`}
        fill
        value={text}
        onChange={(next) => setText(next)}
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
        <PanelsChipsSubview config={config} setConfig={setConfig} onError={onError} />
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
  config: UiConfig | null;
  setConfig: (updater: (prev: UiConfig | null) => UiConfig | null) => void;
  onError: (e: string | null) => void;
}

const CHIP_STYLE_OPTIONS: {
  id: NonNullable<UiConfig['chip_style']>;
  label: string;
  description: string;
  preview: { caption: boolean; icon: boolean };
}[] = [
  {
    id: 'value_only',
    label: 'value only',
    description: 'minimal chrome. the chip is just the number.',
    preview: { caption: false, icon: false },
  },
  {
    id: 'caption_value',
    label: 'caption + value',
    description: 'explicit labels next to the value. easier to learn at a glance.',
    preview: { caption: true, icon: false },
  },
  {
    id: 'icon_value',
    label: 'icon + value',
    description: 'small unicode icons in place of captions. compact and recognizable.',
    preview: { caption: false, icon: true },
  },
];

function PanelsChipsSubview({ config, setConfig, onError }: ChipsSubviewProps) {
  const chipStyle = config?.chip_style ?? 'value_only';
  const pickStyle = (id: NonNullable<UiConfig['chip_style']>) => {
    if (!config) return;
    const next: UiConfig = { ...config, chip_style: id };
    setConfig(() => next);
    void setUiConfig(next).catch((e) => onError(String(e)));
  };

  return (
    <>
      <div className="panels-tab-header">
        <span>chip style</span>
        <span className="panels-tab-header-dim">
          rendering of the tick + mud time chip on the input row
        </span>
      </div>
      <div className="chip-style-picker">
        {CHIP_STYLE_OPTIONS.map((opt) => (
          <button
            key={opt.id}
            type="button"
            className={`chip-style-card${chipStyle === opt.id ? ' is-on' : ''}`}
            onClick={() => pickStyle(opt.id)}
            aria-pressed={chipStyle === opt.id}
          >
            <span className="chip-style-name">
              {opt.label}
              {chipStyle === opt.id && <span className="chip-preset-tag">[active]</span>}
            </span>
            <ChipStyleSample preview={opt.preview} />
            <span className="chip-style-desc">{opt.description}</span>
          </button>
        ))}
      </div>

      <div className="panels-tab-header" style={{ marginTop: 18 }}>
        <span>tick timer</span>
        <span className="panels-tab-header-dim">interval, auto-fire, sound, warning</span>
      </div>
      <TickConfigEditor onError={onError} />
    </>
  );
}

// Tiny inline sample of how a chip renders in the picked style. Uses
// a fixed value (`14s`) so the user sees just the visual difference.
function ChipStyleSample({ preview }: { preview: { caption: boolean; icon: boolean } }) {
  return (
    <span className="chip-style-sample">
      {preview.caption && <span className="chip-style-sample-caption">tick</span>}
      {preview.icon && (
        <span className="chip-style-sample-icon" aria-hidden="true">
          ⏱
        </span>
      )}
      <span className="chip-style-sample-value">14s</span>
    </span>
  );
}

// Tick chip's content config: interval, auto-fire, sound, reset
// pattern, and the warning timer / message / color. Rides directly
// under the tick chip's host-routing row so the placement and the
// content sit together. Backed by tick_get_config / tick_set_config.
// Empty strings round-trip as null on save so the persisted state
// stays clean (the backend trims them too as a belt-and-braces).
interface TickConfigEditorProps {
  onError: (e: string | null) => void;
}
function TickConfigEditor({ onError }: TickConfigEditorProps) {
  const [cfg, setCfg] = useState<TickConfig | null>(null);

  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    tickGetConfig()
      .then((c) => {
        if (!cancelled) setCfg(c);
      })
      .catch((e) => onError(String(e)));
    void subscribeTickConfigChanged((c) => {
      if (!cancelled) setCfg(c);
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, [onError]);

  if (!cfg) {
    return (
      <div className="chips-tick-config-loading">
        <span className="chips-row-control-label">loading tick config…</span>
      </div>
    );
  }

  const commit = (patch: Partial<TickConfig>) => {
    const next: TickConfig = { ...cfg, ...patch };
    setCfg(next);
    void tickSetConfig(next).catch((e) => onError(String(e)));
  };
  const warnOn = cfg.warn_at_secs !== null && cfg.warn_at_secs > 0;

  return (
    <div className="chips-tick-config">
      <div className="chips-tick-config-row">
        <label className="chips-tick-config-field chips-tick-config-toggle">
          <input
            type="checkbox"
            checked={cfg.enabled}
            onChange={(e) => commit({ enabled: e.target.checked })}
          />
          <span>tick timer enabled</span>
        </label>
        <label className="chips-tick-config-field chips-tick-config-toggle">
          <input
            type="checkbox"
            checked={cfg.sound}
            onChange={(e) => commit({ sound: e.target.checked })}
          />
          <span>sound on fire</span>
        </label>
      </div>
      <div className="chips-tick-config-row">
        <label className="chips-tick-config-field">
          <span>interval</span>
          <input
            type="number"
            min={1}
            max={3600}
            value={cfg.interval_secs}
            onChange={(e) => {
              const v = Number(e.target.value);
              if (Number.isFinite(v) && v > 0) commit({ interval_secs: Math.floor(v) });
            }}
          />
          <span className="chips-tick-config-unit">sec</span>
        </label>
        <label className="chips-tick-config-field chips-tick-config-field-wide">
          <span>auto-fire</span>
          <input
            type="text"
            spellCheck={false}
            placeholder="command to send on every tick (blank = off)"
            value={cfg.auto_fire ?? ''}
            onChange={(e) =>
              commit({ auto_fire: e.target.value.length > 0 ? e.target.value : null })
            }
          />
        </label>
      </div>
      <div className="chips-tick-config-row">
        <label className="chips-tick-config-field chips-tick-config-field-wide">
          <span>reset on</span>
          <input
            type="text"
            spellCheck={false}
            placeholder="regex; resets the tick on every match (blank = off)"
            value={cfg.reset_pattern ?? ''}
            onChange={(e) =>
              commit({ reset_pattern: e.target.value.length > 0 ? e.target.value : null })
            }
          />
        </label>
      </div>
      <div className="chips-tick-config-row">
        <label className="chips-tick-config-field chips-tick-config-toggle">
          <input
            type="checkbox"
            checked={warnOn}
            onChange={(e) =>
              commit({
                warn_at_secs: e.target.checked ? (cfg.warn_at_secs ?? 5) : null,
              })
            }
          />
          <span>warn before fire</span>
        </label>
        <label className="chips-tick-config-field">
          <span>at</span>
          <input
            type="number"
            min={1}
            max={300}
            disabled={!warnOn}
            value={cfg.warn_at_secs ?? 5}
            onChange={(e) => {
              const v = Number(e.target.value);
              if (Number.isFinite(v) && v > 0) commit({ warn_at_secs: Math.floor(v) });
            }}
          />
          <span className="chips-tick-config-unit">sec before</span>
        </label>
      </div>
      <div className="chips-tick-config-row">
        <label className="chips-tick-config-field chips-tick-config-field-wide">
          <span>warn text</span>
          <input
            type="text"
            spellCheck={false}
            disabled={!warnOn}
            placeholder="default: tick incoming"
            value={cfg.warn_message ?? ''}
            onChange={(e) =>
              commit({ warn_message: e.target.value.length > 0 ? e.target.value : null })
            }
          />
        </label>
        <label className="chips-tick-config-field">
          <span>color</span>
          <input
            type="text"
            spellCheck={false}
            disabled={!warnOn}
            placeholder="bright-red"
            value={cfg.warn_color ?? ''}
            onChange={(e) =>
              commit({ warn_color: e.target.value.length > 0 ? e.target.value : null })
            }
          />
        </label>
      </div>
    </div>
  );
}

// Vitals appearance section. Lives at the bottom of the Panels tab so
// the user can dial in column toggles, custom bar glyphs, and bar
// width next to the rest of the panel-related layout knobs. Each
// edit autosaves through setUiConfig and broadcasts via
// vosh://vitals-config-changed so VitalsBar redraws live.
/** Resolve the unified style dropdown value from a live `VitalsConfig`.
 *  Returns `track`, `solid|<filled>|<empty>`, `ramped|<filled>|<empty>`,
 *  or `__custom` when the glyph pair does not match a known preset. */
function styleKeyFor(v: VitalsConfig): string {
  // Style dropdown reflects bar_style + glyph preset only — the
  // bar_layout (plain / with_history) lives in its own dropdown
  // beside this one, so this returns the same key for both layouts.
  if (v.bar_style === 'track') return 'track';
  const preset = VITALS_GLYPH_PRESETS.find(
    (p) => p.filled === v.bar_filled && p.empty === v.bar_empty,
  );
  if ((v.bar_style === 'solid' || v.bar_style === 'ramped') && preset) {
    return `${v.bar_style}|${preset.filled}|${preset.empty}`;
  }
  return '__custom';
}

/** Translate a `styleKeyFor` value back into a partial `VitalsConfig`
 *  patch. Used by the unified style dropdown's onChange. */
function applyStyleKey(key: string, current: VitalsConfig): Partial<VitalsConfig> {
  if (key === 'track') return { bar_style: 'track' };
  if (key === '__custom') {
    // Selecting "custom" from the dropdown leaves bar_filled / bar_empty
    // alone (the user is presumably about to edit them) and snaps the
    // bar_style off track since track has no glyph customization.
    const fallback = current.bar_style === 'track' ? 'solid' : current.bar_style;
    return { bar_style: fallback };
  }
  const [style, filled, empty] = key.split('|');
  return {
    bar_style: style as VitalsBarStyle,
    bar_filled: filled,
    bar_empty: empty,
  };
}

/** Quick-pick bar font stacks. Empty string means "inherit the app font"
 *  (the historical behavior). Berkeley and JetBrains are bundled with
 *  Vosh; MonoLisa, Menlo, Consolas, Courier are looked up from the
 *  system font catalog and registered via the font:// URI scheme on
 *  pick. Users can also paste a custom CSS font-family stack via the
 *  text input that shows when "custom" is selected. */
const BAR_FONT_PRESETS: { key: string; label: string; stack: string }[] = [
  { key: '', label: 'use the app font', stack: '' },
  {
    key: 'berkeley',
    label: 'Berkeley Mono (bundled)',
    stack:
      '"BerkeleyMono Bundled", "JetBrainsMono Bundled", Menlo, Consolas, ui-monospace, monospace',
  },
  {
    key: 'jetbrains',
    label: 'JetBrains Mono (bundled)',
    stack: '"JetBrainsMono Bundled", Menlo, Consolas, ui-monospace, monospace',
  },
  {
    key: 'monolisa',
    label: 'MonoLisa (must be installed locally)',
    // MonoLisa ships under several family names depending on which
    // build you have (regular, Variable, the trial). List every common
    // variant so CSS picks the first one your OS exposes.
    stack:
      '"MonoLisa", "MonoLisa Variable", "MonoLisaVariable", "MonoLisa Script", "MonoLisa Trial", monospace',
  },
  { key: 'menlo', label: 'Menlo (macOS default)', stack: 'Menlo, monospace' },
  { key: 'consolas', label: 'Consolas (Windows default)', stack: 'Consolas, monospace' },
  { key: 'courier', label: 'Courier New', stack: '"Courier New", monospace' },
];

/** Family-name candidates for the MonoLisa-detection helper. Picks up
 *  every common installer variant; the first one document.fonts.check
 *  approves is the family the CSS stack will actually resolve to. */
const MONOLISA_CANDIDATES = [
  'MonoLisa',
  'MonoLisa Variable',
  'MonoLisaVariable',
  'MonoLisa Script',
  'MonoLisa Trial',
];

function detectMonoLisaFamily(): string | null {
  for (const name of MONOLISA_CANDIDATES) {
    try {
      if (document.fonts.check(`12px "${name}"`)) return name;
    } catch {
      // Some browsers throw on bare names; ignore and try the next.
    }
  }
  return null;
}

function barFontKeyFor(v: VitalsConfig): string {
  if (!v.bar_font) return '';
  const match = BAR_FONT_PRESETS.find((p) => p.stack === v.bar_font);
  return match ? match.key : '__custom';
}

/** Status line under the bar-font dropdown when the user picks MonoLisa.
 *  Re-runs detection every time the FontFaceSet emits `loadingdone` so
 *  the line flips from "not found" to "detected as X" once the runtime-
 *  injected @font-face for the matching family finishes loading. */
function BarFontMonoLisaStatus() {
  const [matched, setMatched] = useState<string | null>(() => detectMonoLisaFamily());
  useEffect(() => {
    const recheck = () => setMatched(detectMonoLisaFamily());
    recheck();
    const fonts = document.fonts as FontFaceSet | undefined;
    if (!fonts) return;
    fonts.addEventListener('loadingdone', recheck);
    return () => fonts.removeEventListener('loadingdone', recheck);
  }, []);
  return (
    <span className={matched ? 'panels-vitals-style-ok' : 'panels-vitals-style-warn'}>
      {matched
        ? `MonoLisa detected as "${matched}" · bars are using it`
        : 'no MonoLisa family found yet · install MonoLisa and reopen Settings, or check the bars visually since some installers use a name not in the probe list'}
    </span>
  );
}

const VITALS_GLYPH_PRESETS: { label: string; filled: string; empty: string }[] = [
  { label: 'parallelogram', filled: '▰', empty: '▱' },
  { label: 'block', filled: '█', empty: '░' },
  { label: 'block / medium shade', filled: '█', empty: '▒' },
  { label: 'dark / light shade', filled: '▓', empty: '░' },
  { label: 'heavy / light line', filled: '━', empty: '─' },
  { label: 'circle', filled: '●', empty: '○' },
  { label: 'square', filled: '◼', empty: '◻' },
  { label: 'vertical bar', filled: '▮', empty: '▯' },
  { label: 'braille full', filled: '⣿', empty: '⣀' },
  { label: 'braille mid', filled: '⠿', empty: '⠤' },
  { label: 'braille thin', filled: '⠶', empty: '⠀' },
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
  // Inject @font-face for any system font named in the bar font
  // stack so the in-Settings preview can actually resolve it. WKWebView
  // refuses bare family names without @font-face registration. Mirrors
  // the same call inside VitalsBar so live + preview stay in sync.
  useEffect(() => {
    if (config?.vitals.bar_font) loadFontStack(config.vitals.bar_font);
  }, [config?.vitals.bar_font]);
  if (!config) return null;
  const v = config.vitals;
  const apply = (patch: Partial<VitalsConfig>) => {
    const nextVitals: VitalsConfig = { ...v, ...patch };
    const next: UiConfig = { ...config, vitals: nextVitals };
    setConfig(() => next);
    void setUiConfig(next).catch((e) => onError(String(e)));
  };
  const fontPresetKey = barFontKeyFor(v);
  const fontPresetLabel =
    fontPresetKey === ''
      ? 'app font'
      : (BAR_FONT_PRESETS.find((p) => p.key === fontPresetKey)?.label.split(' (')[0] ?? 'custom');
  return (
    <section className="vsplit-frame">
      <aside className="vsplit-rack">
        <header className="vsplit-rack-head">
          <h2>vitals · console</h2>
          <span className="vsplit-led" aria-hidden="true" />
        </header>

        <div className="vsplit-group">
          <div className="vsplit-group-label">mode</div>
          <div className="vsplit-pip-grid vsplit-pip-grid-one">
            <PipToggle
              label="low hp vignette"
              checked={v.low_hp_vignette}
              onChange={(c) => apply({ low_hp_vignette: c })}
            />
          </div>
        </div>

        <div className="vsplit-group">
          <div className="vsplit-group-label">show</div>
          <div className="vsplit-pip-grid">
            <PipToggle label="bars" checked={v.show_bar} onChange={(c) => apply({ show_bar: c })} />
            <PipToggle
              label="percent"
              checked={v.show_percent}
              onChange={(c) => apply({ show_percent: c })}
            />
            <PipToggle
              label="numeric"
              checked={v.show_numeric}
              onChange={(c) => apply({ show_numeric: c })}
            />
            <PipToggle
              label="delta"
              checked={v.show_delta}
              onChange={(c) => apply({ show_delta: c })}
            />
          </div>
        </div>

        <div className={`vsplit-group${v.show_bar ? '' : ' is-disabled'}`}>
          <div className="vsplit-group-label">appearance</div>
          <div className="vsplit-knob">
            <span className="vsplit-knob-label">style</span>
            <select
              value={styleKeyFor(v)}
              disabled={!v.show_bar}
              onChange={(e) => apply(applyStyleKey(e.target.value, v))}
            >
              {styleKeyFor(v) === '__custom' && (
                <option value="__custom">
                  custom · {v.bar_style} · {v.bar_filled} {v.bar_empty}
                </option>
              )}
              <optgroup label="solid · one glyph per cell">
                {VITALS_GLYPH_PRESETS.map((p) => (
                  <option key={`solid|${p.label}`} value={`solid|${p.filled}|${p.empty}`}>
                    {p.label} · {p.filled} {p.empty}
                  </option>
                ))}
              </optgroup>
              <optgroup label="ramped · sub-character smoothness">
                {VITALS_GLYPH_PRESETS.map((p) => (
                  <option key={`ramped|${p.label}`} value={`ramped|${p.filled}|${p.empty}`}>
                    {p.label} · {p.filled} {p.empty}
                  </option>
                ))}
              </optgroup>
              <optgroup label="track">
                <option value="track">track · smooth CSS bar, no glyphs</option>
              </optgroup>
              {styleKeyFor(v) !== '__custom' && (
                <optgroup label="other">
                  <option value="__custom">custom · set glyphs below</option>
                </optgroup>
              )}
            </select>
          </div>
          <div className="vsplit-knob">
            <span className="vsplit-knob-label">layout</span>
            <select
              value={v.bar_layout}
              disabled={!v.show_bar}
              onChange={(e) => apply({ bar_layout: e.target.value as 'plain' | 'with_history' })}
              aria-label="bar layout"
            >
              <option value="plain">plain · bar only</option>
              <option value="with_history">history · bar + braille trend grid</option>
            </select>
          </div>
          <div className="vsplit-knob">
            <span className="vsplit-knob-label">width</span>
            <input
              type="number"
              className="settings-num-input vsplit-num"
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
          </div>
          <div className="vsplit-knob">
            <span className="vsplit-knob-label">bar font</span>
            <select
              value={fontPresetKey}
              disabled={!v.show_bar}
              onChange={(e) => {
                const key = e.target.value;
                if (key === '__custom') return;
                const preset = BAR_FONT_PRESETS.find((p) => p.key === key);
                if (preset) apply({ bar_font: preset.stack });
              }}
            >
              {BAR_FONT_PRESETS.map((p) => (
                <option key={p.key} value={p.key}>
                  {p.label}
                </option>
              ))}
              {fontPresetKey === '__custom' && (
                <option value="__custom">custom · (edit below)</option>
              )}
            </select>
          </div>
          {fontPresetKey === 'monolisa' && (
            <div className="vsplit-knob-hint">
              <BarFontMonoLisaStatus />
            </div>
          )}
          {styleKeyFor(v) === '__custom' && v.bar_style !== 'track' && (
            <>
              <div className="vsplit-knob">
                <span className="vsplit-knob-label">filled</span>
                <input
                  type="text"
                  className="panels-vitals-glyph-input"
                  value={v.bar_filled}
                  disabled={!v.show_bar}
                  spellCheck={false}
                  onChange={(e) =>
                    apply({ bar_filled: e.target.value.length > 0 ? e.target.value : '▰' })
                  }
                  aria-label="filled glyph"
                />
              </div>
              <div className="vsplit-knob">
                <span className="vsplit-knob-label">empty</span>
                <input
                  type="text"
                  className="panels-vitals-glyph-input"
                  value={v.bar_empty}
                  disabled={!v.show_bar}
                  spellCheck={false}
                  onChange={(e) =>
                    apply({ bar_empty: e.target.value.length > 0 ? e.target.value : '▱' })
                  }
                  aria-label="empty glyph"
                />
              </div>
            </>
          )}
          <details className="vsplit-adv">
            <summary>custom CSS font-family</summary>
            <input
              type="text"
              className="panels-vitals-glyph-input vsplit-adv-input"
              spellCheck={false}
              value={v.bar_font}
              disabled={!v.show_bar}
              placeholder='blank = use the app font · or "MonoLisa", "Iosevka", monospace'
              onChange={(e) => apply({ bar_font: e.target.value })}
            />
          </details>
        </div>

        <div className="vsplit-group">
          <div className="vsplit-group-label">layout</div>
          <div className="vsplit-knob">
            <span className="vsplit-knob-label">arrange</span>
            <select
              value={v.layout}
              disabled={v.template_enabled}
              onChange={(e) => apply({ layout: e.target.value as VitalsLayout })}
            >
              <option value="stacked">stacked rows (hp / mn / mv per row)</option>
              <option value="inline">inline (one row, prompt-style)</option>
            </select>
          </div>
          {v.layout === 'inline' && (
            <div className="vsplit-knob">
              <span className="vsplit-knob-label">inline style</span>
              <select
                value={v.inline_style}
                disabled={v.template_enabled}
                onChange={(e) => apply({ inline_style: e.target.value as VitalsInlineStyle })}
              >
                <option value="plain">plain (tintin nprompt text)</option>
                <option value="underline">underline (drain bar beneath each)</option>
                <option value="badge">badge (chip + corner percent)</option>
              </select>
            </div>
          )}
          <div className="vsplit-knob">
            <span className="vsplit-knob-label">% color</span>
            <select
              value={v.percent_color}
              onChange={(e) => apply({ percent_color: e.target.value as VitalsPercentColor })}
            >
              <option value="fill">per-vital (matches bar)</option>
              <option value="gradient">red → green gradient</option>
            </select>
          </div>
          {v.layout === 'inline' && v.inline_style !== 'plain' && (
            <div className="vsplit-knob">
              <span className="vsplit-knob-label">% mode</span>
              <select
                value={v.percent_color_mode}
                onChange={(e) =>
                  apply({ percent_color_mode: e.target.value as VitalsPercentColorMode })
                }
              >
                <option value="drain">drain to red as low (default)</option>
                <option value="stat">match stat color</option>
                <option value="accent">brand pink</option>
              </select>
            </div>
          )}
        </div>

        <div className="vsplit-group">
          <div className="vsplit-group-label">colors</div>
          <VitalColorRow
            label="hp"
            value={v.hp_color}
            fallback="green"
            onChange={(c) => apply({ hp_color: c })}
          />
          <VitalColorRow
            label="mn"
            value={v.mn_color}
            fallback="blue"
            onChange={(c) => apply({ mn_color: c })}
          />
          <VitalColorRow
            label="mv"
            value={v.mv_color}
            fallback="orange"
            onChange={(c) => apply({ mv_color: c })}
          />
          <div className="vsplit-pip-grid vsplit-pip-grid-one">
            <PipToggle
              label="drain through red as the bar empties"
              checked={v.use_color_ramp}
              onChange={(c) => apply({ use_color_ramp: c })}
            />
          </div>
        </div>

        <div className="vsplit-group vsplit-group-template">
          <details className="vsplit-adv">
            <summary>template · advanced</summary>
            <VitalsTemplateEditor config={v} apply={apply} />
          </details>
        </div>

        <footer className="vsplit-rack-foot">
          <span className="vsplit-rack-foot-live">live</span>
          <button
            type="button"
            className="settings-btn settings-btn-mute"
            onClick={() => apply(DEFAULT_VITALS_CONFIG)}
          >
            [reset vitals]
          </button>
        </footer>
      </aside>

      <main className="vsplit-monitor">
        <header className="vsplit-monitor-head">
          <h3>live monitor</h3>
          <span className="vsplit-monitor-hint">
            drag a bar to scrub · changes reflect in the running HUD
          </span>
        </header>
        <div className="vsplit-monitor-stage" aria-label="vitals preview">
          <VitalsPreview config={v} />
        </div>
        <footer className="vsplit-monitor-foot">
          <span>
            <strong>layout</strong> {v.template_enabled ? 'template' : v.layout}
          </span>
          <span>
            <strong>style</strong> {v.bar_style}
          </span>
          <span>
            <strong>width</strong> {v.bar_width} cells
          </span>
          <span>
            <strong>font</strong> {fontPresetLabel}
          </span>
        </footer>
      </main>
    </section>
  );
}

function PipToggle({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (next: boolean) => void;
}) {
  return (
    <button
      type="button"
      className={`vsplit-pip${checked ? ' is-on' : ''}`}
      onClick={() => onChange(!checked)}
      role="switch"
      aria-checked={checked}
    >
      {label}
    </button>
  );
}

function VitalColorRow({
  label,
  value,
  fallback,
  onChange,
}: {
  label: string;
  value: string;
  fallback: string;
  onChange: (next: string) => void;
}) {
  const hex =
    value.trim().length > 0 && /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.test(value.trim())
      ? value.trim()
      : '#888888';
  return (
    <label className="panels-vitals-color-row">
      <span className="panels-vitals-color-label">{label}</span>
      <input
        type="color"
        value={hex}
        onChange={(e) => onChange(e.target.value)}
        aria-label={`${label} swatch`}
      />
      <input
        type="text"
        className="panels-vitals-color-hex"
        spellCheck={false}
        value={value}
        placeholder={`built-in ${fallback}`}
        onChange={(e) => onChange(e.target.value)}
      />
      <button
        type="button"
        className="settings-btn settings-btn-mute"
        onClick={() => onChange('')}
        disabled={value.trim().length === 0}
      >
        [clear]
      </button>
    </label>
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
function PreviewTrackBar({
  value,
  cells,
  color,
  font,
}: {
  value: number;
  cells: number;
  color: string;
  font?: string;
}) {
  const pct = Math.max(0, Math.min(100, value));
  return (
    <span
      className="vitals-glyphs vitals-glyphs-track"
      style={{ width: `${cells}ch`, fontFamily: font || undefined }}
      aria-hidden="true"
    >
      <span className="vitals-glyphs-track-fill" style={{ width: `${pct}%`, background: color }} />
    </span>
  );
}

// 1/8-step partial-block ladder used to render the boundary cell of a
// ramped bar in the Settings preview. Mirrors `RAMP_PARTIALS` in
// VitalsBar.tsx so the preview matches the runtime exactly.
const PREVIEW_RAMP_PARTIALS = ['', '▏', '▎', '▍', '▌', '▋', '▊', '▉'];

/** Mirrors HistoryWrapper from VitalsBar.tsx for the Settings
 *  preview — per-cell flex grid so each bar cell and grid cell
 *  share identical x-ranges by construction. */
function PreviewHistoryWrapper({
  cells,
  color,
  fillValue,
  filled,
  empty,
  font,
}: {
  cells: number;
  color: string;
  fillValue: number;
  filled: string;
  empty: string;
  font?: string;
}) {
  const filledCount = Math.max(0, Math.min(cells, Math.round((fillValue / 100) * cells)));
  return (
    <span
      className="vitals-glyphs vitals-spark"
      style={{
        flexBasis: `${cells}ch`,
        maxWidth: `${cells}ch`,
        fontFamily: font || undefined,
      }}
      aria-hidden="true"
    >
      <span className="vitals-spark-row vitals-spark-bar-row">
        {Array.from({ length: cells }, (_, i) => {
          const isFilled = i < filledCount;
          return (
            <span key={i} className="vitals-spark-cell" style={isFilled ? { color } : undefined}>
              {isFilled ? filled : empty}
            </span>
          );
        })}
      </span>
      <span className="vitals-spark-row vitals-spark-grid-row">
        {Array.from({ length: cells }, (_, i) => (
          <span key={i} className="vitals-spark-cell vitals-spark-grid-cell">
            <span className="vitals-spark-grid-base">⣿</span>
            <span className="vitals-spark-grid-trace" style={{ color }}>
              {previewBrailleCell(fillValue / 100, fillValue / 100)}
            </span>
          </span>
        ))}
      </span>
    </span>
  );
}

const PREVIEW_DOTS_L = [0x01, 0x02, 0x04, 0x40];
const PREVIEW_DOTS_R = [0x08, 0x10, 0x20, 0x80];
function previewBrailleCell(v0: number, v1: number): string {
  const lit0 = Math.max(0, Math.min(4, Math.round(v0 * 4)));
  const lit1 = Math.max(0, Math.min(4, Math.round(v1 * 4)));
  let bits = 0;
  for (let d = 0; d < 4; d++) {
    const fromBottom = 3 - d;
    if (fromBottom < lit0) bits |= PREVIEW_DOTS_L[d];
    if (fromBottom < lit1) bits |= PREVIEW_DOTS_R[d];
  }
  return String.fromCodePoint(0x2800 | bits);
}

function previewRampedBar({
  value,
  cells,
  filled,
  empty,
  color,
  font,
}: {
  value: number;
  cells: number;
  filled: string;
  empty: string;
  color: string;
  font?: string;
}): ReactNode {
  const exact = (Math.max(0, Math.min(100, value)) / 100) * cells;
  const whole = Math.floor(exact);
  const fraction = exact - whole;
  const stepIdx = Math.round(fraction * 8);
  let boundary = '';
  if (whole < cells && stepIdx > 0) {
    boundary = stepIdx === 8 ? filled : PREVIEW_RAMP_PARTIALS[stepIdx];
  }
  const emptyCount = Math.max(0, cells - whole - (boundary ? 1 : 0));
  return (
    <span className="vitals-glyphs" style={{ fontFamily: font || undefined }} aria-hidden="true">
      {whole > 0 && <span style={{ color }}>{filled.repeat(whole)}</span>}
      {boundary && <span style={{ color }}>{boundary}</span>}
      {emptyCount > 0 && <span className="vitals-empty">{empty.repeat(emptyCount)}</span>}
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
        const label = name.slice(4);
        const s = get(label);
        const c = gradient ? s.color : colorForVital(label, s.value, config);
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
        const label = name.slice(4);
        const s = get(label);
        const fill = gradient ? s.color : colorForVital(label, s.value, config);
        if (track) {
          return (
            <PreviewTrackBar value={s.value} cells={width} color={fill} font={config.bar_font} />
          );
        }
        if (config.bar_style === 'ramped') {
          return previewRampedBar({
            value: s.value,
            cells: width,
            filled: config.bar_filled,
            empty: config.bar_empty,
            color: fill,
            font: config.bar_font,
          });
        }
        const filled = Math.round((s.value / 100) * width);
        const empty = width - filled;
        return (
          <span
            className="vitals-glyphs"
            style={{ fontFamily: config.bar_font || undefined }}
            aria-hidden="true"
          >
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

type PreviewLabel = 'hp' | 'mn' | 'mv';
const PREVIEW_MAX: Record<PreviewLabel, number> = { hp: 1000, mn: 300, mv: 200 };
const PREVIEW_DELTA: Record<PreviewLabel, number> = { hp: 12, mn: -8, mv: 0 };

/**
 * Build the sample row for one vital from a percent value. Computes
 * `cur` from `value * max / 100` so the numeric column tracks the
 * dragged value, and `color` from `colorForPercent` so the gradient
 * percent-color path follows the live value too.
 */
function previewSample(label: PreviewLabel, value: number): PreviewSample {
  const v = Math.max(0, Math.min(100, Math.round(value)));
  return {
    label,
    cur: Math.round((v / 100) * PREVIEW_MAX[label]),
    max: PREVIEW_MAX[label],
    value: v,
    delta: PREVIEW_DELTA[label],
    color: colorForPercent(v),
  };
}

function VitalsPreview({ config }: { config: VitalsConfig }) {
  // Dragable per-vital fill state. Seed values match the previous
  // static preview (hp 75% / mn 50% / mv 25%) so a Settings reopen
  // starts at the same starting point users were used to.
  const [values, setValues] = useState<Record<PreviewLabel, number>>({
    hp: 75,
    mn: 50,
    mv: 25,
  });
  const sample = (['hp', 'mn', 'mv'] as PreviewLabel[]).map((l) => previewSample(l, values[l]));
  const width = Math.max(4, Math.min(60, config.bar_width));
  const gradient = config.percent_color === 'gradient';
  const track = config.bar_style === 'track';

  // Mouse-drag scrubber. On mousedown we capture the bar wrapper's
  // bounding rect once and then translate cursor X into a 0..100
  // percent for the rest of the gesture. window-level listeners catch
  // moves and releases that drift outside the bar, which matters when
  // the user drags past either edge.
  const startDrag = (label: PreviewLabel) => (e: React.MouseEvent<HTMLDivElement>) => {
    e.preventDefault();
    const rect = e.currentTarget.getBoundingClientRect();
    const setFromX = (clientX: number) => {
      const x = clientX - rect.left;
      const pct = Math.max(0, Math.min(100, Math.round((x / rect.width) * 100)));
      setValues((prev) => (prev[label] === pct ? prev : { ...prev, [label]: pct }));
    };
    setFromX(e.clientX);
    const onMove = (ev: MouseEvent) => setFromX(ev.clientX);
    const onUp = () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  };

  const headerText = `preview · hp ${values.hp}% / mn ${values.mn}% / mv ${values.mv}% · drag a bar`;

  // Wrap the rendered bar in a draggable hit-target. The wrapper is
  // capped at the bar's cell width so its bounding rect matches the
  // visible bar exactly — dragging at 50% of the wrapper lands at 50%
  // of the bar, not 50% of the whole row. Without the cap the wrapper
  // grew into the trailing row whitespace, making fine-grained drag
  // on narrow bars unusable.
  const draggableBar = (label: PreviewLabel, cells: number, node: ReactNode) => (
    <div
      className="vitals-preview-drag"
      style={{ flexBasis: `${cells}ch`, maxWidth: `${cells}ch` }}
      onMouseDown={startDrag(label)}
      role="slider"
      aria-label={`${label} fill percent`}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={values[label]}
    >
      {node}
    </div>
  );

  // Template preview takes priority because the template field
  // overrides layout. Renders sample values for each token using the
  // same look the runtime VitalsBar would produce. The template
  // variant is not dragable in this commit (the bar token lives
  // anywhere inside an arbitrary template, and the per-line layout
  // makes the wrapper hit-region ambiguous); the stacked and inline
  // variants are.
  if (config.template_enabled) {
    return (
      <>
        <div className="vitals-preview-head">{headerText}</div>
        <div className="vitals-bar vitals-bar-template">
          <div className="vitals-row vitals-row-template">
            {previewTemplate(config, sample, track, gradient)}
          </div>
        </div>
      </>
    );
  }

  if (config.layout === 'inline') {
    return (
      <>
        <div className="vitals-preview-head">{headerText}</div>
        <div className="vitals-bar vitals-bar-inline">
          <div className="vitals-row vitals-row-inline">
            {sample.map((s) => (
              <span key={s.label} className="vitals-inline-chip">
                {config.show_numeric && <span className="vitals-numeric">{s.cur}</span>}
                {config.show_percent && (
                  <span
                    className="vitals-inline-pct"
                    style={{
                      color: gradient ? s.color : colorForVital(s.label, s.value, config),
                    }}
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
      </>
    );
  }

  return (
    <>
      <div className="vitals-preview-head">{headerText}</div>
      <div className="vitals-bar">
        {sample.map((s) => {
          const filled = Math.round((s.value / 100) * width);
          const empty = width - filled;
          const percentColor = gradient ? s.color : colorForVital(s.label, s.value, config);
          const innerBar = track ? (
            <PreviewTrackBar
              value={s.value}
              cells={width}
              color={percentColor}
              font={config.bar_font}
            />
          ) : config.bar_style === 'ramped' ? (
            previewRampedBar({
              value: s.value,
              cells: width,
              filled: config.bar_filled,
              empty: config.bar_empty,
              color: percentColor,
              font: config.bar_font,
            })
          ) : (
            <span
              className="vitals-glyphs"
              style={{ fontFamily: config.bar_font || undefined }}
              aria-hidden="true"
            >
              <span style={{ color: percentColor }}>{config.bar_filled.repeat(filled)}</span>
              <span className="vitals-empty">{config.bar_empty.repeat(empty)}</span>
            </span>
          );
          // history layout: render the per-cell-flex bar + grid.
          // Per-cell layout means the bar is always rendered as
          // bar_filled / bar_empty glyphs (any chosen bar_style
          // collapses to solid in spark mode) — the trade-off for
          // guaranteed cell-by-cell alignment between bar and grid
          // rows regardless of font metrics.
          const bar =
            config.bar_layout === 'with_history' ? (
              <PreviewHistoryWrapper
                cells={width}
                color={percentColor}
                fillValue={s.value}
                filled={config.bar_filled}
                empty={config.bar_empty}
                font={config.bar_font}
              />
            ) : (
              innerBar
            );
          return (
            <div
              key={s.label}
              className={`vitals-row${config.bar_layout === 'with_history' ? ' vitals-row-spark' : ''}`}
            >
              <span className="vitals-label">{s.label}</span>
              {config.show_bar && draggableBar(s.label as PreviewLabel, width, bar)}
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
    </>
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
  // (Embedded-zone tracking removed with the tick/time placement
  // model — chips no longer ride inside hosts.)
  const chip = (id: PanelId, hidden = false) => {
    const canUp = !hidden && canMovePanelUp(layout, id);
    const canDown = !hidden && canMovePanelDown(layout, id);
    const guests: PanelId[] = [];
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
  // side zone with other panels stacking above or below. For those
  // we hide the align control entirely below.
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
      {vertical && !meta.fillsSideZone ? (
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
        // bottom / hidden / fill zones we render a placeholder span so
        // the grid columns still line up with other rows.
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

import { useEffect, useRef, useState, type ReactNode } from 'react';
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
import { VitalsConfigSection } from './components/VitalsSettings';
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
  dockLayoutGet,
  dockLayoutSet,
  exportAliases,
  exportTriggers,
  getUiConfig,
  importAliases,
  importTriggers,
  installUpdateAndRelaunch,
  listSystemFonts,
  resolveThemeTerminalColors,
  setUiConfig,
  subscribeDockLayoutChanged,
  type SystemFontEntry,
  type UiConfig,
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
  | 'vitals'
  | 'tick'
  | 'panels'
  | 'profiles'
  | 'loadouts'
  | 'triggers'
  | 'aliases'
  | 'macros'
  | 'import'
  | 'logs';
// Tab list grouped into three labeled buckets. The nav prints the
// `group` name as an uppercase header above the first tab of each
// bucket and a hairline between buckets, so look & layout / content /
// tools read as distinct sections. `vitals` and `tick` are top-level
// tabs (promoted out of the old Panels sub-toggle); `panels` keeps the
// placement map and chip routing. `pathBOnly` tabs only appear when
// Path B mode is active.
type TabGroup = 'look' | 'content' | 'tools';
const TABS: { id: TabId; label: string; group: TabGroup; pathBOnly?: boolean }[] = [
  { id: 'general', label: 'general', group: 'look' },
  { id: 'themes', label: 'themes', group: 'look' },
  { id: 'vitals', label: 'vitals', group: 'look' },
  { id: 'tick', label: 'tick', group: 'look' },
  { id: 'panels', label: 'panels', group: 'look' },
  { id: 'profiles', label: 'profiles', group: 'content' },
  { id: 'loadouts', label: 'loadouts', group: 'content', pathBOnly: true },
  { id: 'triggers', label: 'triggers', group: 'content' },
  { id: 'aliases', label: 'aliases', group: 'content' },
  { id: 'macros', label: 'macros', group: 'content' },
  { id: 'import', label: 'import', group: 'tools' },
  { id: 'logs', label: 'logs', group: 'tools' },
];

// One 14px stroke icon per rail item, from the Ember icon set. Stroke
// color rides currentColor so the active accent tint applies for free.
function TabIcon({ id }: { id: TabId }) {
  const paths: Record<TabId, JSX.Element> = {
    general: (
      <>
        <path d="M1.5 3.2h3.3M8 3.2h4.5M1.5 7h5.8M10.4 7h2.1M1.5 10.8h1.5M6.1 10.8h6.4" />
        <circle cx="6.5" cy="3.2" r="1.5" />
        <circle cx="8.9" cy="7" r="1.5" />
        <circle cx="4.6" cy="10.8" r="1.5" />
      </>
    ),
    themes: (
      <>
        <rect x="2" y="2" width="4.2" height="4.2" rx="0.8" />
        <rect x="7.8" y="2" width="4.2" height="4.2" rx="0.8" />
        <rect x="2" y="7.8" width="4.2" height="4.2" rx="0.8" />
        <rect x="7.8" y="7.8" width="4.2" height="4.2" rx="0.8" />
      </>
    ),
    vitals: <path d="M1 7.2h2.6l1.6-3.8 2.4 7.4 1.6-3.6H13" />,
    tick: (
      <>
        <circle cx="7" cy="7" r="5.3" />
        <path d="M7 4.2V7l2.1 1.5" />
      </>
    ),
    panels: (
      <>
        <rect x="1.5" y="2.5" width="11" height="9" rx="1" />
        <path d="M6.2 2.5v9M9.6 2.5v9" />
      </>
    ),
    profiles: (
      <>
        <circle cx="7" cy="4.6" r="2.4" />
        <path d="M2.6 12.4c0-2.7 2-4.4 4.4-4.4s4.4 1.7 4.4 4.4" />
      </>
    ),
    loadouts: (
      <>
        <rect x="2" y="4" width="10" height="7.5" rx="1" />
        <path d="M5 4V2.8A0.8 0.8 0 0 1 5.8 2h2.4a0.8 0.8 0 0 1 0.8 0.8V4" />
      </>
    ),
    triggers: <path d="M7.9 1.4 3.6 8h2.9l-1 4.6L9.9 6H7z" />,
    aliases: <path d="M3.6 3.6 7 7l-3.4 3.4M7.6 3.6 11 7l-3.4 3.4" />,
    macros: (
      <>
        <rect x="1.5" y="3.5" width="11" height="7" rx="1" />
        <path d="M3.8 5.8h0.02M6.2 5.8h0.02M8.6 5.8h0.02M11 5.8h0.02M4.4 8.4h5.2" />
      </>
    ),
    import: (
      <path d="M7 1.5v7M4.2 5.7 7 8.5l2.8-2.8M2 10v1.5A1 1 0 0 0 3 12.5h8a1 1 0 0 0 1-1V10" />
    ),
    logs: (
      <>
        <rect x="2.5" y="1.5" width="9" height="11" rx="1" />
        <path d="M4.8 4.5h4.4M4.8 7h4.4M4.8 9.5h2.6" />
      </>
    ),
  };
  return (
    <svg
      className="settings-tab-icon"
      width="14"
      height="14"
      viewBox="0 0 14 14"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {paths[id]}
    </svg>
  );
}

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
      <TopBar brand="Settings" showAuxButtons={false} />
      <div className="settings-shell">
        <nav className="settings-tabs settings-tabs-vertical">
          {visibleTabs.map((t, i) => {
            const groupStart = i === 0 || visibleTabs[i - 1].group !== t.group;
            return (
              <span key={t.id} className="settings-tab-wrap">
                {groupStart && (
                  <span className="settings-tab-group" aria-hidden="true">
                    {t.group}
                  </span>
                )}
                <button
                  type="button"
                  className={`settings-tab${tab === t.id ? ' settings-tab-active' : ''}`}
                  aria-pressed={tab === t.id}
                  onClick={() => {
                    setError(null);
                    setTab(t.id);
                  }}
                >
                  <TabIcon id={t.id} />
                  {t.label}
                </button>
              </span>
            );
          })}
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
          {tab === 'vitals' &&
            (config ? (
              <div className="settings-pane">
                <div className="settings-pane-title">vitals</div>
                <div className="settings-pane-sub">
                  how the hp / mn / mv readout looks. changes save automatically.
                </div>
                <VitalsConfigSection config={config} setConfig={setConfig} onError={setError} />
              </div>
            ) : (
              <div className="settings-loading">loading…</div>
            ))}
          {tab === 'tick' && (
            <div className="settings-pane">
              <div className="settings-pane-title">tick timer</div>
              <div className="settings-pane-sub">
                the ROM tick countdown. changes save automatically.
              </div>
              <TickConfigEditor onError={setError} />
            </div>
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

// GPU rendering toggle. Backed by localStorage rather than the
// profile-synced UiConfig because WebGL availability is a property of
// the host machine, not the user's profile. Toggling writes the flag
// and shows a reload hint; the renderer swap requires a fresh terminal
// mount because xterm 5.5 has no clean way to hot-swap renderers.
function WebglToggle() {
  const [enabled, setEnabled] = useState(() =>
    typeof localStorage !== 'undefined' ? localStorage.getItem('vosh.webgl') !== '0' : true,
  );
  const [dirty, setDirty] = useState(false);
  return (
    <label className="settings-checkbox" title="GPU rendering via WebGL2 (on by default)">
      <input
        type="checkbox"
        checked={enabled}
        onChange={(e) => {
          const next = e.target.checked;
          setEnabled(next);
          setDirty(true);
          try {
            // On is the default, so clear the flag; off writes an
            // explicit '0' that the terminal reads as opt-out.
            if (next) localStorage.removeItem('vosh.webgl');
            else localStorage.setItem('vosh.webgl', '0');
          } catch {
            // localStorage may be disabled in some Tauri contexts; ignore
          }
        }}
      />
      <span>GPU rendering{dirty ? ' (reload pending)' : ' (on by default)'}</span>
    </label>
  );
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
            checked={resolveThemeTerminalColors(config.theme, config.theme_terminal_colors)}
            onChange={(e) => update({ theme_terminal_colors: e.target.checked })}
          />
          <span>tint output with theme</span>
        </label>
        <label className="settings-checkbox">
          <input
            type="checkbox"
            checked={config.bright_bold}
            onChange={(e) => update({ bright_bold: e.target.checked })}
          />
          <span>bright text as bold (native renderer)</span>
        </label>
        <WebglToggle />
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
            check now
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
              install v{updateStatus.version} + restart
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
          <span>keep last command</span>
        </label>
        <label className="settings-checkbox">
          <input
            type="checkbox"
            checked={config.spellcheck_prompt}
            onChange={(e) => update({ spellcheck_prompt: e.target.checked })}
          />
          <span>spell check chat lines</span>
        </label>
        <label className="settings-checkbox">
          <input
            type="checkbox"
            checked={config.echo_macros}
            onChange={(e) => update({ echo_macros: e.target.checked })}
          />
          <span>echo macro commands</span>
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
          <span className="settings-paste-hint">0 = no pacing. raise to dodge flood filters.</span>
        </span>
      </Row>
      <Row label="prompt">
        <label className="settings-checkbox">
          <input
            type="checkbox"
            checked={config.prompt_template_enabled}
            onChange={(e) => update({ prompt_template_enabled: e.target.checked })}
          />
          <span>replace gagged prompts</span>
        </label>
        <input
          type="text"
          className="settings-font-input"
          spellCheck={false}
          value={config.prompt_template}
          placeholder="[%hp_bar:10 %hp/%maxhp hp] > "
          onChange={(e) => update({ prompt_template: e.target.value })}
          aria-label="custom prompt template"
        />
        <span className="settings-paste-hint">
          {
            'tokens %hp %pct_hp %hp_bar:W:COLOR %c_hp (auto) %c_red %{c:255,128,0} %{bg:#330033} %s_bold %s_italic %s_underline %c_reset %time %date. needs a prompt-capture trigger that gags and emits vars.'
          }
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
          close
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
          save
        </button>
        <button
          type="button"
          className="settings-btn settings-btn-mute"
          onClick={() => void reload()}
        >
          reload
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
          reset to defaults
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
  const [open, setOpen] = useState<Set<PanelId>>(new Set<PanelId>(['affects']));
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
            placeholder="bright-red or #rrggbb"
            title="ANSI name (bright-red), hex (#rrggbb), or 256-palette index (196)"
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

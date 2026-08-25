import { useEffect, useRef, useState, type ReactNode } from 'react';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { TopBar } from './components/TopBar';
import { TriggerForm } from './components/TriggerForm';
import { AliasForm } from './components/AliasForm';
import { UnsavedDot } from './components/UnsavedDot';
import { Chevron } from './components/Icons';
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
  | 'typography'
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
// Tab list grouped into six labeled buckets, per the settings-redo
// canvas. Existing tab ids stay stable; `typography` is the one new
// id (font settings moved out of general). `tick` keeps its id but
// now also hosts the input-row chip style and the moons position.
// `pathBOnly` tabs only appear when Path B mode is active.
type TabGroup = 'appearance' | 'hud' | 'automation' | 'characters' | 'session' | 'tools';
const TABS: { id: TabId; label: string; group: TabGroup; pathBOnly?: boolean }[] = [
  { id: 'themes', label: 'themes', group: 'appearance' },
  { id: 'typography', label: 'typography', group: 'appearance' },
  { id: 'vitals', label: 'vitals', group: 'hud' },
  { id: 'tick', label: 'tick & chips', group: 'hud' },
  { id: 'panels', label: 'panels', group: 'hud' },
  { id: 'triggers', label: 'triggers', group: 'automation' },
  { id: 'aliases', label: 'aliases', group: 'automation' },
  { id: 'macros', label: 'macros', group: 'automation' },
  { id: 'profiles', label: 'profiles', group: 'characters' },
  { id: 'loadouts', label: 'loadouts', group: 'characters', pathBOnly: true },
  { id: 'general', label: 'general', group: 'session' },
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
    typography: <path d="M2 3.5V2h10v1.5M7 2v10M5 12h4" />,
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

// What each tab answers for when the user types into the settings
// search. Labels are always searchable; these add the row-level terms
// a person actually remembers ("font" lives in general, "color" in
// themes).
const TAB_KEYWORDS: Record<TabId, string> = {
  general: 'update input paste prompt spell echo history gpu webgl performance',
  themes: 'theme color accent ansi palette custom divider sent command dark tint',
  typography: 'font size family mono bold bright preview face system',
  vitals: 'hp mana moves bar percent layout column track gauge',
  tick: 'tick timer warn sound duration chip moons position time',
  panels: 'panel layout zone map chat group affects dock tracked',
  profiles: 'profile character host switch auto match',
  loadouts: 'loadout group set active',
  triggers: 'trigger pattern highlight gag replace route wash script regex',
  aliases: 'alias shortcut command expansion',
  macros: 'macro key f1 binding keyboard',
  import: 'import tintin mushclient migrate',
  logs: 'log search history session export',
};

// Settings window. Frameless Ghostty chrome via the shared TopBar;
// body splits into named tabs along a left rail.
export function SettingsApp() {
  const [tab, setTab] = useState<TabId>('general');
  const [config, setConfig] = useState<UiConfig | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pathBActive, setPathBActive] = useState(false);
  const [query, setQuery] = useState('');
  const searchRef = useRef<HTMLInputElement | null>(null);

  // Cmd/Ctrl+F focuses the settings search, Escape clears it.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'f') {
        e.preventDefault();
        searchRef.current?.focus();
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, []);

  // Palette deep-link: a tab id arrives via localStorage when this
  // window cold-starts, or via the goto event when it is already up.
  useEffect(() => {
    const isTab = (v: string | null): v is TabId => TABS.some((t) => t.id === v);
    try {
      const pending = localStorage.getItem('vosh.settings.pendingTab');
      if (isTab(pending)) setTab(pending);
      localStorage.removeItem('vosh.settings.pendingTab');
    } catch {
      // storage unavailable; event path still works
    }
    let cancelled = false;
    let unsub: (() => void) | undefined;
    void listen<string>('vosh://settings-goto-tab', (event) => {
      if (isTab(event.payload)) {
        setError(null);
        setTab(event.payload);
        void getCurrentWindow().setFocus();
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      if (unsub) unsub();
    };
  }, []);

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

  const trimmedQuery = query.trim().toLowerCase();
  const matchesQuery = (t: (typeof TABS)[number]) =>
    trimmedQuery.length === 0 ||
    t.label.includes(trimmedQuery) ||
    TAB_KEYWORDS[t.id].includes(trimmedQuery);
  const visibleTabs = TABS.filter((t) => (!t.pathBOnly || pathBActive) && matchesQuery(t));

  const jumpToFirstMatch = () => {
    if (visibleTabs.length > 0) {
      setError(null);
      setTab(visibleTabs[0].id);
      setQuery('');
      searchRef.current?.blur();
    }
  };

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
      <TopBar
        brand="settings"
        showAuxButtons={false}
        titleExtra={
          <div className="settings-search">
            <svg width="12" height="12" viewBox="0 0 14 14" fill="none" strokeWidth="1.4">
              <circle cx="6" cy="6" r="4.2" />
              <path d="M9.2 9.2 12.6 12.6" />
            </svg>
            <input
              ref={searchRef}
              type="text"
              value={query}
              placeholder="search settings"
              spellCheck={false}
              aria-label="search settings"
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') jumpToFirstMatch();
                if (e.key === 'Escape') {
                  setQuery('');
                  e.currentTarget.blur();
                }
              }}
            />
            <span className="kbd">&#8984;F</span>
          </div>
        }
      />
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
          {tab === 'typography' && (
            <TypographyTab config={config} setConfig={setConfig} onError={setError} />
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
                <TabHead title="vitals" />
                <VitalsConfigSection config={config} setConfig={setConfig} onError={setError} />
              </div>
            ) : (
              <div className="settings-loading">loading…</div>
            ))}
          {tab === 'tick' && (
            <TickChipsTab config={config} setConfig={setConfig} onError={setError} />
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

// Debounced auto-save shared by the config-backed tabs. Text inputs
// can fire many updates in a row while the user types; the debounce
// coalesces them into one setUiConfig call after typing settles.
// setUiConfig owns every cross-window emit and dedupes against the
// previous snapshot, so callers only get the local theme refresh and
// the saved indicator.
function useSettingsAutoSave(
  setConfig: GeneralProps['setConfig'],
  onError: GeneralProps['onError'],
) {
  const [savedAt, setSavedAt] = useState<number | null>(null);
  const saveTimerRef = useRef<number | null>(null);
  const scheduleAutoSave = (next: UiConfig) => {
    if (saveTimerRef.current) window.clearTimeout(saveTimerRef.current);
    saveTimerRef.current = window.setTimeout(() => {
      saveTimerRef.current = null;
      void (async () => {
        try {
          await setUiConfig(next);
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
  return { update, savedAt };
}

// Tab content header: slab title + the one autosave note.
function TabHead({ title, right }: { title: string; right?: ReactNode }) {
  return (
    <div className="settings-tab-head">
      <div className="settings-pane-title">{title}</div>
      <span className="settings-tab-head-spacer" />
      {right ?? <span className="settings-autosave-hint">changes save automatically</span>}
    </div>
  );
}

function GeneralTab({ config, setConfig, onError }: GeneralProps) {
  const { update, savedAt } = useSettingsAutoSave(setConfig, onError);
  const [updateStatus, setUpdateStatus] = useState<{
    kind: 'idle' | 'checking' | 'available' | 'current' | 'error' | 'installing';
    msg?: string;
    version?: string;
  }>({ kind: 'idle' });

  const close = () => void getCurrentWindow().close();

  if (!config) return <div className="settings-loading">loading…</div>;

  return (
    <>
      <TabHead title="general" />
      <div className="settings-sect settings-sect-first">
        <span className="settings-section-label">input</span>
        <div className="settings-frow">
          <span className="settings-flabel">history</span>
          <span className="settings-fctrl">
            <label className="settings-checkbox">
              <input
                type="checkbox"
                checked={config.keep_last_command}
                onChange={(e) => update({ keep_last_command: e.target.checked })}
              />
              <span>keep last command</span>
            </label>
          </span>
          <span className="settings-fhelp">
            restores and selects the last line so Enter resends it
          </span>
        </div>
        <div className="settings-frow">
          <span className="settings-flabel">spell check</span>
          <span className="settings-fctrl">
            <label className="settings-checkbox">
              <input
                type="checkbox"
                checked={config.spellcheck_prompt}
                onChange={(e) => update({ spellcheck_prompt: e.target.checked })}
              />
              <span>chat lines only</span>
            </label>
          </span>
        </div>
        <div className="settings-frow">
          <span className="settings-flabel">macros</span>
          <span className="settings-fctrl">
            <label className="settings-checkbox">
              <input
                type="checkbox"
                checked={config.echo_macros}
                onChange={(e) => update({ echo_macros: e.target.checked })}
              />
              <span>echo macro commands</span>
            </label>
          </span>
        </div>
        <div className="settings-frow">
          <span className="settings-flabel">paste pacing</span>
          <span className="settings-fctrl">
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
          </span>
          <span className="settings-fhelp">
            0 = no pacing. raise it to dodge server flood filters.
          </span>
        </div>
      </div>

      <div className="settings-sect">
        <span className="settings-section-label">prompt</span>
        <div className="settings-frow">
          <span className="settings-flabel">replace gagged</span>
          <span className="settings-fctrl">
            <label className="settings-checkbox">
              <input
                type="checkbox"
                checked={config.prompt_template_enabled}
                onChange={(e) => update({ prompt_template_enabled: e.target.checked })}
              />
              <span>render a template where the server prompt was</span>
            </label>
          </span>
        </div>
        <div className="settings-frow">
          <span className="settings-flabel">template</span>
          <span className="settings-fctrl">
            <input
              type="text"
              className="settings-font-input"
              spellCheck={false}
              value={config.prompt_template}
              placeholder="[%hp_bar:10 %hp/%maxhp hp] > "
              onChange={(e) => update({ prompt_template: e.target.value })}
              aria-label="custom prompt template"
            />
          </span>
          <span className="settings-fhelp">
            {
              'tokens %hp %pct_hp %hp_bar:W:COLOR %c_hp (auto) %c_red %{c:255,128,0} %{bg:#330033} %s_bold %s_italic %s_underline %c_reset %time %date. needs a prompt-capture trigger that gags and emits vars.'
            }
          </span>
        </div>
      </div>

      <div className="settings-sect">
        <span className="settings-section-label">app</span>
        <div className="settings-frow">
          <span className="settings-flabel">updates</span>
          <span className="settings-fctrl settings-updates-row">
            <label className="settings-checkbox">
              <input
                type="checkbox"
                checked={config.auto_update}
                onChange={(e) => update({ auto_update: e.target.checked })}
              />
              <span>check on launch</span>
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
        </div>
        <div className="settings-frow">
          <span className="settings-flabel">performance</span>
          <span className="settings-fctrl">
            <WebglToggle />
          </span>
          <span className="settings-fhelp">this machine only &#183; takes effect after reload</span>
        </div>
      </div>

      <div className="settings-actions">
        <button type="button" className="settings-btn settings-btn-mute" onClick={close}>
          close
        </button>
        {savedAt !== null && <span className="settings-saved">saved.</span>}
      </div>
    </>
  );
}

// Typography: the terminal face, size, rendering, the system font
// browser, and the live preview. Moved out of general per the
// settings-redo canvas so appearance settings live together.
function TypographyTab({ config, setConfig, onError }: GeneralProps) {
  const { update, savedAt } = useSettingsAutoSave(setConfig, onError);
  const [systemFonts, setSystemFonts] = useState<SystemFontEntry[]>([]);
  const [fontsState, setFontsState] = useState<'idle' | 'loading' | 'loaded'>('idle');
  const [fontFilter, setFontFilter] = useState('');
  const [showOnlyMono, setShowOnlyMono] = useState(true);
  // Size field draft. While the input is focused the user owns the
  // text, including an empty field mid backspace. Valid in range
  // values apply live as a preview, anything else just sits in the
  // draft, and blur snaps the field back to the stored value. The
  // old controlled input re clamped every keystroke, which made
  // clearing the field to type a new size impossible.
  const [sizeDraft, setSizeDraft] = useState<string | null>(null);

  // System font enumeration is lazy. font-kit's first pass costs
  // 200-500ms because it parses every installed font file to detect
  // the monospace flag; the fetch fires only when the user actually
  // engages the picker. The backend caches the result in a OnceLock
  // so later calls within a session are instant.
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

  if (!config) return <div className="settings-loading">loading…</div>;

  return (
    <>
      <TabHead
        title="typography"
        right={
          <>
            {savedAt !== null && <span className="settings-saved">saved.</span>}
            <span className="settings-autosave-hint">changes save automatically</span>
          </>
        }
      />
      <div className="settings-sect settings-sect-first">
        <span className="settings-section-label">terminal face</span>
        <div className="settings-frow">
          <span className="settings-flabel">family</span>
          <span className="settings-fctrl">
            <input
              type="text"
              className="settings-font-input"
              spellCheck={false}
              value={config.font_family}
              placeholder='"BerkeleyMono Bundled", Menlo, monospace'
              onChange={(e) => update({ font_family: e.target.value })}
            />
          </span>
          <span className="settings-fhelp">
            <span className="settings-font-picks">
              {FONT_PICKS.map((pick) => (
                <button
                  key={pick.label}
                  type="button"
                  className="opt-chip"
                  onClick={() => update({ font_family: pick.value })}
                >
                  {pick.label}
                </button>
              ))}
            </span>
          </span>
        </div>
        <div className="settings-frow">
          <span className="settings-flabel">size</span>
          <span className="settings-fctrl settings-size-ctrl">
            <button
              type="button"
              className="opt-chip settings-size-step"
              aria-label="smaller"
              onClick={() => update({ font_size: Math.max(9, config.font_size - 1) })}
            >
              &#8722;
            </button>
            <input
              type="number"
              min={9}
              max={32}
              value={sizeDraft ?? String(config.font_size)}
              onChange={(e) => {
                const raw = e.target.value;
                setSizeDraft(raw);
                const n = Number(raw);
                if (raw !== '' && Number.isFinite(n) && n >= 9 && n <= 32) {
                  update({ font_size: Math.round(n) });
                }
              }}
              onBlur={() => setSizeDraft(null)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') e.currentTarget.blur();
              }}
            />
            <button
              type="button"
              className="opt-chip settings-size-step"
              aria-label="larger"
              onClick={() => update({ font_size: Math.min(32, config.font_size + 1) })}
            >
              +
            </button>
            <span className="settings-unit">px</span>
          </span>
        </div>
        <div className="settings-frow">
          <span className="settings-flabel">rendering</span>
          <span className="settings-fctrl">
            <label className="settings-checkbox">
              <input
                type="checkbox"
                checked={config.bright_bold}
                onChange={(e) => update({ bright_bold: e.target.checked })}
              />
              <span>bright text as bold</span>
            </label>
          </span>
          <span className="settings-fhelp">
            native renderer only. SGR bright (8-15) takes the heavier cut.
          </span>
        </div>
      </div>

      <div className="settings-sect">
        <span className="settings-section-label">system fonts</span>
        <div className="settings-frow">
          <span className="settings-flabel">browse</span>
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
      </div>

      <div className="settings-sect">
        <span className="settings-section-label">preview</span>
        <div
          className="settings-font-preview"
          style={{ fontFamily: config.font_family, fontSize: config.font_size }}
        >
          {PREVIEW_TEXT}
        </div>
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

  const updateConfig = (patch: Partial<UiConfig>) => {
    if (!config) return;
    const next: UiConfig = { ...config, ...patch };
    setConfig(() => next);
    void setUiConfig(next).catch((e) => onError(String(e)));
  };

  if (!loaded) return <div className="settings-loading">loading panels…</div>;

  return (
    <div className="panels-tab">
      <TabHead title="panels" />
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
      {/* Tracked affects, promoted from three clicks deep (the old
          panes sub-view accordion) to a first-class section per the
          settings-redo canvas. */}
      {config && (
        <div className="settings-sect">
          <span className="settings-section-label">tracked affects</span>
          <span className="settings-fhelp" style={{ gridColumn: 'auto' }}>
            affects listed here render in the affects pane with remaining duration
          </span>
          <TrackedAffectsEditor config={config} update={updateConfig} />
        </div>
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

// The tick &amp; chips tab: the ROM tick countdown, the input-row chip
// style, and the status-strip moons position. The three live together
// because the chip renders the tick and the moons share the strip —
// per the settings-redo canvas. The moons position control is new;
// the config field and cross-window event existed with no UI at all.
function TickChipsTab({ config, setConfig, onError }: ChipsSubviewProps) {
  const chipStyle = config?.chip_style ?? 'value_only';
  if (!config) return <div className="settings-loading">loading…</div>;
  const updateConfig = (patch: Partial<UiConfig>) => {
    if (!config) return;
    const next: UiConfig = { ...config, ...patch };
    setConfig(() => next);
    void setUiConfig(next).catch((e) => onError(String(e)));
  };

  const moonsPosition = config?.moons_position ?? 'right-edge';
  const MOONS_OPTIONS: { id: UiConfig['moons_position']; label: string }[] = [
    { id: 'right-edge', label: 'right edge' },
    { id: 'before-time', label: 'before the clock' },
    { id: 'after-time', label: 'after the clock' },
  ];

  return (
    <div className="settings-pane">
      <TabHead title="tick & chips" />
      <div className="settings-sect settings-sect-first">
        <span className="settings-section-label">tick timer</span>
        <TickConfigEditor onError={onError} />
      </div>

      <div className="settings-sect">
        <span className="settings-section-label">input row chip</span>
        <span className="settings-fhelp" style={{ gridColumn: 'auto' }}>
          how the tick and mud time readout renders at the right edge of the input row
        </span>
        <div className="chip-style-picker">
          {CHIP_STYLE_OPTIONS.map((opt) => (
            <button
              key={opt.id}
              type="button"
              className={`chip-style-card${chipStyle === opt.id ? ' is-on' : ''}`}
              onClick={() => updateConfig({ chip_style: opt.id })}
              aria-pressed={chipStyle === opt.id}
            >
              <span className="chip-style-name">{opt.label}</span>
              <ChipStyleSample preview={opt.preview} />
              <span className="chip-style-desc">{opt.description}</span>
            </button>
          ))}
        </div>
      </div>

      <div className="settings-sect">
        <span className="settings-section-label">status strip</span>
        <div className="settings-frow">
          <span className="settings-flabel">moons</span>
          <span className="settings-fctrl">
            {MOONS_OPTIONS.map((opt) => (
              <button
                key={opt.id}
                type="button"
                className={`opt-chip${moonsPosition === opt.id ? ' is-on' : ''}`}
                aria-pressed={moonsPosition === opt.id}
                onClick={() => updateConfig({ moons_position: opt.id })}
              >
                {opt.label}
              </button>
            ))}
          </span>
          <span className="settings-fhelp">
            where the Aabahran moon phases sit in the status strip
          </span>
        </div>
      </div>
    </div>
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
// pattern, and the warning timer / message / color. Renders as
// .settings-frow rows inside the "tick timer" section TickChipsTab
// provides. Backed by tick_get_config / tick_set_config.
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
    return <div className="settings-loading">loading tick config…</div>;
  }

  const commit = (patch: Partial<TickConfig>) => {
    const next: TickConfig = { ...cfg, ...patch };
    setCfg(next);
    void tickSetConfig(next).catch((e) => onError(String(e)));
  };
  const warnOn = cfg.warn_at_secs !== null && cfg.warn_at_secs > 0;

  return (
    <>
      <div className="settings-frow">
        <span className="settings-flabel">timer</span>
        <span className="settings-fctrl">
          <label className="settings-checkbox">
            <input
              type="checkbox"
              checked={cfg.enabled}
              onChange={(e) => commit({ enabled: e.target.checked })}
            />
            <span>enabled</span>
          </label>
          <label className="settings-checkbox">
            <input
              type="checkbox"
              checked={cfg.sound}
              onChange={(e) => commit({ sound: e.target.checked })}
            />
            <span>sound on fire</span>
          </label>
        </span>
      </div>
      <div className="settings-frow">
        <span className="settings-flabel">interval</span>
        <span className="settings-fctrl">
          <input
            type="number"
            className="settings-num-input"
            min={1}
            max={3600}
            value={cfg.interval_secs}
            onChange={(e) => {
              const v = Number(e.target.value);
              if (Number.isFinite(v) && v > 0) commit({ interval_secs: Math.floor(v) });
            }}
            aria-label="tick interval in seconds"
          />
          <span className="settings-unit">sec</span>
        </span>
      </div>
      <div className="settings-frow">
        <span className="settings-flabel">auto-fire</span>
        <span className="settings-fctrl">
          <input
            type="text"
            className="settings-font-input"
            spellCheck={false}
            value={cfg.auto_fire ?? ''}
            onChange={(e) =>
              commit({ auto_fire: e.target.value.length > 0 ? e.target.value : null })
            }
            aria-label="auto-fire command"
          />
        </span>
        <span className="settings-fhelp">command to send on every tick. blank = off</span>
      </div>
      <div className="settings-frow">
        <span className="settings-flabel">reset on</span>
        <span className="settings-fctrl">
          <input
            type="text"
            className="settings-font-input"
            spellCheck={false}
            value={cfg.reset_pattern ?? ''}
            onChange={(e) =>
              commit({ reset_pattern: e.target.value.length > 0 ? e.target.value : null })
            }
            aria-label="tick reset pattern"
          />
        </span>
        <span className="settings-fhelp">regex; resets the tick on every match</span>
      </div>
      <div className="settings-frow">
        <span className="settings-flabel">warn</span>
        <span className="settings-fctrl">
          <label className="settings-checkbox">
            <input
              type="checkbox"
              checked={warnOn}
              onChange={(e) =>
                commit({
                  warn_at_secs: e.target.checked ? (cfg.warn_at_secs ?? 5) : null,
                })
              }
              aria-label="warn before fire"
            />
          </label>
          <input
            type="number"
            className="settings-num-input"
            min={1}
            max={300}
            disabled={!warnOn}
            value={cfg.warn_at_secs ?? 5}
            onChange={(e) => {
              const v = Number(e.target.value);
              if (Number.isFinite(v) && v > 0) commit({ warn_at_secs: Math.floor(v) });
            }}
            aria-label="warn seconds before the tick"
          />
          <span className="settings-unit">sec before</span>
        </span>
      </div>
      <div className="settings-frow">
        <span className="settings-flabel">warn text</span>
        <span className="settings-fctrl">
          <input
            type="text"
            className="settings-font-input"
            spellCheck={false}
            disabled={!warnOn}
            placeholder="tick incoming"
            value={cfg.warn_message ?? ''}
            onChange={(e) =>
              commit({ warn_message: e.target.value.length > 0 ? e.target.value : null })
            }
            aria-label="warn message"
          />
          <span className="settings-unit">color</span>
          <input
            type="text"
            spellCheck={false}
            disabled={!warnOn}
            placeholder="bright-red"
            value={cfg.warn_color ?? ''}
            onChange={(e) =>
              commit({ warn_color: e.target.value.length > 0 ? e.target.value : null })
            }
            aria-label="warn color"
          />
        </span>
        <span className="settings-fhelp">
          blank falls back to the default text. color takes an ANSI name, #rrggbb hex, or a
          256-palette index
        </span>
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
              title={`move ${PANELS[id].label} earlier in this zone`}
              aria-label={`move ${PANELS[id].label} up`}
              onClick={() => onMove(id, 'up')}
            >
              <Chevron open={false} up />
            </button>
            <button
              type="button"
              className="panels-preview-chip-arrow"
              disabled={!canDown}
              title={`move ${PANELS[id].label} later in this zone`}
              aria-label={`move ${PANELS[id].label} down`}
              onClick={() => onMove(id, 'down')}
            >
              <Chevron open />
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
      <span className="panels-row-name">{meta.label}</span>
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
      const stored = localStorage.getItem(storageKey);
      return stored === 'json' ? 'json' : 'form';
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

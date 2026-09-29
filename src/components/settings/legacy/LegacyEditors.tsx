import { useEffect, useState, type ReactNode } from 'react';
import { CodeEditor } from '../../CodeEditor';
import { UnsavedDot } from '../../UnsavedDot';
import { useUnsavedWarning } from '../../../lib/unsaved';
import { loadFontStack, loadSystemFont } from '../../../lib/fontLoader';
import {
  checkForUpdate,
  INPUT_CURSOR_STYLES,
  installUpdateAndRelaunch,
  listSystemFonts,
  subscribeTickConfigChanged,
  tickGetConfig,
  tickSetConfig,
  type InputCursorStyle,
  type SystemFontEntry,
  type TickConfig,
  type UiConfig,
} from '../../../lib/session';
import type { SetUiConfig } from '../pageTypes';
import { useSettingsAutoSave } from './useSettingsAutoSave';

// The editors the old Settings window drew, moved here from
// SettingsApp.tsx so the new frame can show them while the boarded
// pages and the General, Layout, and Input boards are built. Their
// markup and copy are as they were. They render inside LegacyIsland,
// which gives them the legacy .settings-app rules and marks them
// data-interim so the next pass finds every one.

/** The wrapper every interim editor sits in. */
export function LegacyIsland({ children }: { children: ReactNode }) {
  return (
    <div className="settings-app settings-body st-legacy" data-interim="">
      {children}
    </div>
  );
}

interface LegacyConfigProps {
  config: UiConfig;
  update: (patch: Partial<UiConfig>) => void;
}

// Caret shapes, in the order they appear in the picker: solid first,
// then the lighter variants of the same box.
const CURSOR_LABELS: Record<InputCursorStyle, string> = {
  block: 'block',
  block_outline: 'outline',
  half_block: 'half block',
  underline: 'underline',
  underline_thick: 'thick underline',
  pipe: 'pipe',
  pipe_thick: 'thick pipe',
};

/** The old General tab's input section. */
export function LegacyCommandLine({ config, update }: LegacyConfigProps) {
  return (
    <div className="settings-sect settings-sect-first">
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
        <span className="settings-flabel">cursor</span>
        <span className="settings-fctrl settings-caret-picks">
          {INPUT_CURSOR_STYLES.map((id) => (
            <button
              key={id}
              type="button"
              className={`opt-chip${config.input_cursor_style === id ? ' is-on' : ''}`}
              onClick={() => update({ input_cursor_style: id })}
              aria-pressed={config.input_cursor_style === id}
            >
              <span className="caret-sample-box" aria-hidden="true">
                <span className={`caret-sample caret-shape--${id}`} />
              </span>
              <span>{CURSOR_LABELS[id]}</span>
            </button>
          ))}
        </span>
        <span className="settings-fhelp">the shape of the caret on the command line</span>
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
  );
}

/** The old General tab's prompt section. */
export function LegacyPrompt({ config, update }: LegacyConfigProps) {
  return (
    <div className="settings-sect settings-sect-first">
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
  );
}

/** The old General tab's updates row. */
export function LegacyUpdates({ config, update }: LegacyConfigProps) {
  const [updateStatus, setUpdateStatus] = useState<{
    kind: 'idle' | 'checking' | 'available' | 'current' | 'error' | 'installing';
    msg?: string;
    version?: string;
  }>({ kind: 'idle' });
  return (
    <div className="settings-sect settings-sect-first">
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
    </div>
  );
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

/** The old General tab's performance row. It only drives the xterm
 *  renderer, which macOS does not show, so General draws it on Windows
 *  and Linux only. */
export function LegacyRendering() {
  return (
    <div className="settings-sect settings-sect-first">
      <div className="settings-frow">
        <span className="settings-flabel">performance</span>
        <span className="settings-fctrl">
          <WebglToggle />
        </span>
        <span className="settings-fhelp">this machine only &#183; takes effect after reload</span>
      </div>
    </div>
  );
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

interface TypographyProps {
  config: UiConfig | null;
  setConfig: SetUiConfig;
  onError: (e: string | null) => void;
}

// Typography: the terminal face, size, rendering, the system font
// browser, and the live preview.
export function TypographyTab({ config, setConfig, onError }: TypographyProps) {
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
export function JsonTab({ kind, singular, plural, load, save, onError }: JsonTabProps) {
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

  // Fade the "saved." indicator after 1.5s. Without this it lingers
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

interface SwitcherProps {
  modeKey: string;
  formRender: () => ReactNode;
  jsonRender: () => ReactNode;
}

// Pill toggle at the top of triggers/aliases tabs that swaps between
// the structured form editor and the raw JSON editor. modeKey is the
// localStorage namespace so each tab remembers its own preference.
export function EditorModeSwitcher({ modeKey, formRender, jsonRender }: SwitcherProps) {
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

// Tick chip's content config: interval, auto-fire, sound, reset
// pattern, and the warning timer / message / color. Backed by
// tick_get_config / tick_set_config. Empty strings round-trip as null
// on save so the persisted state stays clean (the backend trims them
// too as a belt-and-braces).
export function TickConfigEditor({ onError }: { onError: (e: string | null) => void }) {
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
    <div className="settings-sect settings-sect-first">
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
    </div>
  );
}

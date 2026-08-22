import { emit } from '@tauri-apps/api/event';
import { useEffect, useRef, useState } from 'react';
import {
  BUILTIN_THEMES,
  customToAppTheme,
  DEFAULT_THEME_ID,
  findTheme,
  setCustomThemes,
  type AppTheme,
} from '../lib/themes';
import { applyTheme } from '../lib/theme';
import { ANSI_SLOTS, CANONICAL_ANSI_16, setBaseAnsi } from '../lib/baseAnsi';
import {
  resolveThemeTerminalColors,
  setUiConfig,
  type CustomTheme,
  type UiConfig,
} from '../lib/session';

interface Props {
  config: UiConfig | null;
  setConfig: (updater: (prev: UiConfig | null) => UiConfig | null) => void;
  onError: (e: string | null) => void;
}

// Settings tab for the theme catalog + custom theme editor. One card
// grid holds every theme — built-ins (read-only), then user-authored
// ones, then the dashed "+ new from active" card. Clicking a card
// sets the active theme. Whenever the active theme is a custom one,
// the editor renders directly below the catalog: a tile grid of
// color slots grouped by role (surfaces / text / borders / accent /
// semantic / terminal surfaces / ANSI 16).
//
// Edits live-apply to the running window via applyTheme + the
// theme-changed event so the user sees the result instantly. The
// debounced save path persists changes through ui_set_config.
export function ThemesTab({ config, setConfig, onError }: Props) {
  const debounceRef = useRef<number | null>(null);

  if (!config) return <div className="settings-loading">loading…</div>;

  // Auto-persist theme actions so the live preview also becomes
  // the saved state. Without this, clicking a theme flips the CSS
  // but the backend keeps the old value — closing settings keeps
  // the preview live, but reopening reads stale state and reverts.
  const persist = (cfg: UiConfig) => {
    setUiConfig(cfg).catch((e) => onError(String(e)));
  };

  const persistDebounced = (cfg: UiConfig, delayMs = 350) => {
    if (debounceRef.current !== null) {
      window.clearTimeout(debounceRef.current);
    }
    debounceRef.current = window.setTimeout(() => {
      debounceRef.current = null;
      persist(cfg);
    }, delayMs);
  };

  const updateCustom = (id: string, patch: Partial<CustomTheme>) => {
    setConfig((prev) => {
      if (!prev) return prev;
      const next = prev.custom_themes.map((t) => (t.id === id ? { ...t, ...patch } : t));
      setCustomThemes(next.map(customToAppTheme));
      if (prev.theme === id) {
        const updated = next.find((t) => t.id === id);
        if (updated) {
          const merged = customToAppTheme(updated);
          applyTheme(merged.id);
          // The main window resolves theme-changed against its own
          // copy of the custom catalog, so ship the fresh catalog
          // first and only then nudge the re-apply. Emitting the id
          // alone made the main window repaint with the colors it
          // already had, so edits never showed up outside this window.
          void emit('vosh://custom-themes-changed', next).then(() =>
            emit('vosh://theme-changed', merged.id),
          );
        }
      }
      const updatedConfig = { ...prev, custom_themes: next };
      // Color slot changes can fire on every drag tick of the
      // native color picker; debounce so we are not slamming the
      // backend with redundant writes.
      persistDebounced(updatedConfig);
      return updatedConfig;
    });
  };

  const handleCreate = () => {
    // Compute newId from the current snapshot; collisions are very
    // unlikely in a settings UI but the updater below will re-check
    // against fresh state.
    const existingIds = new Set([
      ...BUILTIN_THEMES.map((t) => t.id),
      ...config.custom_themes.map((t) => t.id),
    ]);
    let counter = 1;
    let newId = 'custom';
    while (existingIds.has(newId)) {
      counter += 1;
      newId = `custom-${counter}`;
    }
    setConfig((prev) => {
      if (!prev) return prev;
      // Read the base theme from PREV — not the closure-captured
      // config — so a theme switched seconds earlier in this same
      // tab is honored even if React has not yet committed a
      // re-render of ThemesTab.
      const baseId = prev.theme;
      const base = findTheme(baseId);
      const newTheme: CustomTheme = {
        id: newId,
        label: `${base.label} (custom)`,
        description: `Forked from ${base.label}`,
        xterm: { ...(base.xterm as unknown as Record<string, string>) },
        chrome: { ...(base.chrome as unknown as Record<string, string>) },
      };
      const next = [...prev.custom_themes, newTheme];
      setCustomThemes(next.map(customToAppTheme));
      // The editor keys off the active theme, so activate the fork
      // right away — it is a byte-for-byte copy of the base, so the
      // visible colors do not change. Ship the catalog before the
      // id for the same reason updateCustom does.
      applyTheme(newId);
      void emit('vosh://custom-themes-changed', next).then(() =>
        emit('vosh://theme-changed', newId),
      );
      const updated = { ...prev, custom_themes: next, theme: newId };
      persist(updated);
      return updated;
    });
  };

  const handleDelete = (id: string) => {
    // No confirm dialog — WKWebView's `confirm()` is unreliable
    // (sometimes returns undefined without showing) and the action
    // is recoverable by recreating the theme from any base.
    setConfig((prev) => {
      if (!prev) return prev;
      const next = prev.custom_themes.filter((t) => t.id !== id);
      setCustomThemes(next.map(customToAppTheme));
      const nextTheme = prev.theme === id ? DEFAULT_THEME_ID : prev.theme;
      if (prev.theme === id) {
        applyTheme(nextTheme);
        void emit('vosh://theme-changed', nextTheme);
      }
      const updated = { ...prev, custom_themes: next, theme: nextTheme };
      persist(updated);
      return updated;
    });
    onError(null);
  };

  const handleSelect = (id: string) => {
    setConfig((prev) => {
      if (!prev) return prev;
      const updated = { ...prev, theme: id };
      applyTheme(id);
      void emit('vosh://theme-changed', id);
      persist(updated);
      return updated;
    });
  };

  const activeCustom = config.custom_themes.find((t) => t.id === config.theme) ?? null;

  return (
    <div className="themes-tab">
      <div className="themes-head">
        <div className="settings-pane-title">themes</div>
        <span className="themes-autosave">changes save automatically</span>
      </div>

      <div className="theme-cards">
        {BUILTIN_THEMES.map((t) => (
          <ThemeCard
            key={t.id}
            theme={t}
            isActive={config.theme === t.id}
            isCustom={false}
            onSelect={() => handleSelect(t.id)}
          />
        ))}
        {config.custom_themes.map((t) => (
          <ThemeCard
            key={t.id}
            theme={customToAppTheme(t)}
            isActive={config.theme === t.id}
            isCustom
            onSelect={() => handleSelect(t.id)}
            onDelete={() => handleDelete(t.id)}
          />
        ))}
        <button type="button" className="theme-card theme-card-new" onClick={handleCreate}>
          + new from active
        </button>
      </div>

      {activeCustom && (
        <ThemeEditor
          theme={customToAppTheme(activeCustom)}
          onMetaChange={(label, description) =>
            updateCustom(activeCustom.id, { label, description })
          }
          onColorChange={(slot, key, value) => {
            const next = { ...activeCustom[slot], [key]: value };
            updateCustom(activeCustom.id, { [slot]: next } as Partial<CustomTheme>);
          }}
        />
      )}

      <div className="settings-sect">
        <span className="settings-section-label">tint</span>
        <div className="settings-frow">
          <span className="settings-flabel">terminal tint</span>
          <span className="settings-fctrl">
            <label className="settings-checkbox">
              <input
                type="checkbox"
                checked={resolveThemeTerminalColors(config.theme, config.theme_terminal_colors)}
                onChange={(e) => {
                  const updated = { ...config, theme_terminal_colors: e.target.checked };
                  setConfig(() => updated);
                  persist(updated);
                }}
              />
              <span>tint output with theme</span>
            </label>
          </span>
          <span className="settings-fhelp">
            follows the theme by default &#183; ember ships its pastel ANSI on
          </span>
        </div>
      </div>

      <BaseAnsiSection config={config} setConfig={setConfig} onError={onError} />
      <SplitDividerRow config={config} setConfig={setConfig} onError={onError} />
      <InputEchoColorRow config={config} setConfig={setConfig} onError={onError} />
    </div>
  );
}

// The base terminal palette: ANSI 0-15 as used while tint-output-with-
// theme is off. Editing any slot materializes the full 16-entry list
// from the canonical chart; reset drops back to null (canonical).
function BaseAnsiSection({ config, setConfig, onError }: Props) {
  if (!config) return null;
  const active = config.terminal_base_ansi ?? ANSI_SLOTS.map((s) => CANONICAL_ANSI_16[s]);
  const isCustom = config.terminal_base_ansi !== null;
  const persist = (next: string[] | null) => {
    const updated: UiConfig = { ...config, terminal_base_ansi: next };
    setConfig(() => updated);
    setBaseAnsi(next);
    void setUiConfig(updated).catch((e) => onError(String(e)));
  };
  return (
    <div className="base-ansi">
      <div className="base-ansi-head">
        <span className="caps" style={{ color: 'var(--c-text-faint)' }}>
          terminal base palette
        </span>
        <span className="base-ansi-note">
          the ANSI 0-15 colors while tint output with theme is off
        </span>
        {isCustom && (
          <button
            type="button"
            className="settings-btn settings-btn-mute"
            onClick={() => persist(null)}
          >
            reset to canonical
          </button>
        )}
      </div>
      <div className="color-groups">
        <div className="color-group">
          <div className="color-group-grid">
            {ANSI_SLOTS.map((slot, i) => (
              <ColorSlot
                key={slot}
                slotName={slot}
                value={active[i]}
                onChange={(v) => {
                  const next = [...active];
                  next[i] = v;
                  persist(next);
                }}
              />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

// Text input for color values that drafts keystrokes locally and only
// propagates values the browser can render (or empty, which callers map
// to "use the theme default"). Propagating every keystroke live-applied
// partial hex like "#010e0" straight into CSS variables, and every
// surface reading that variable collapsed to transparent while you
// typed. Leaving the field with an unrenderable value snaps the text
// back to the last applied color.
function ColorTextField({
  className,
  placeholder,
  value,
  allowEmpty,
  onCommit,
}: {
  className: string;
  placeholder?: string;
  value: string;
  allowEmpty: boolean;
  onCommit: (v: string) => void;
}) {
  const [draft, setDraft] = useState(value);
  const focusedRef = useRef(false);
  useEffect(() => {
    // Track external changes (the color picker, clear, theme switches)
    // while the field is not being typed in.
    if (!focusedRef.current) setDraft(value);
  }, [value]);
  return (
    <input
      type="text"
      className={className}
      spellCheck={false}
      placeholder={placeholder}
      value={draft}
      onFocus={() => {
        focusedRef.current = true;
      }}
      onChange={(e) => {
        const v = e.target.value;
        setDraft(v);
        const renderable = typeof CSS !== 'undefined' && CSS.supports('color', v);
        if ((v === '' && allowEmpty) || renderable) {
          onCommit(v);
        }
      }}
      onBlur={() => {
        focusedRef.current = false;
        setDraft(value);
      }}
    />
  );
}

function SplitDividerRow({ config, setConfig, onError }: Props) {
  if (!config) return null;
  const value = config.split_divider_color ?? '';
  const update = (next: string | null) => {
    const updated: UiConfig = { ...config, split_divider_color: next };
    setConfig(() => updated);
    void setUiConfig(updated).catch((e) => onError(String(e)));
    // Cross-window emit so the running main window picks up the
    // new color without a relaunch.
    void emit('vosh://split-divider-changed', next);
  };
  return (
    <div className="settings-row">
      <span className="settings-row-label">split scrollback divider</span>
      <span className="settings-color-row">
        <input
          type="color"
          value={normalizeForColorInput(value)}
          onChange={(e) => update(e.target.value)}
          aria-label="split divider color"
        />
        <ColorTextField
          className="settings-color-text"
          placeholder="theme default (#rrggbb, rgba, named)"
          value={value}
          allowEmpty
          onCommit={(v) => update(v || null)}
        />
        <button
          type="button"
          className="settings-btn settings-btn-mute"
          onClick={() => update(null)}
        >
          clear
        </button>
      </span>
    </div>
  );
}

function InputEchoColorRow({ config, setConfig, onError }: Props) {
  if (!config) return null;
  const value = config.input_echo_color ?? '';
  const update = (next: string | null) => {
    const updated: UiConfig = { ...config, input_echo_color: next };
    setConfig(() => updated);
    void setUiConfig(updated).catch((e) => onError(String(e)));
    // Cross-window emit so the running main window picks up the
    // new color without a relaunch.
    void emit('vosh://input-echo-color-changed', next);
  };
  return (
    <div className="settings-row">
      <span className="settings-row-label">sent command color</span>
      <span className="settings-color-row">
        <input
          type="color"
          value={normalizeForColorInput(value)}
          onChange={(e) => update(e.target.value)}
          aria-label="sent command color"
        />
        <ColorTextField
          className="settings-color-text"
          placeholder="theme default (#rrggbb, rgba, named)"
          value={value}
          allowEmpty
          onCommit={(v) => update(v || null)}
        />
        <button
          type="button"
          className="settings-btn settings-btn-mute"
          onClick={() => update(null)}
        >
          clear
        </button>
      </span>
    </div>
  );
}

function normalizeForColorInput(color: string): string {
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

interface CardProps {
  theme: AppTheme;
  isActive: boolean;
  isCustom: boolean;
  onSelect: () => void;
  onDelete?: () => void;
}

// One catalog card: state dot, name, (custom tag), swatch trio, and a
// delete affordance on inactive custom cards. The whole card is the
// select target; delete stops propagation so it never also selects.
function ThemeCard({ theme, isActive, isCustom, onSelect, onDelete }: CardProps) {
  return (
    <div
      className={`theme-card${isActive ? ' is-active' : ''}`}
      role="button"
      tabIndex={0}
      title={theme.description}
      onClick={onSelect}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') onSelect();
      }}
    >
      <span className="theme-card-dot" />
      <span className="theme-card-name">{theme.label}</span>
      {isCustom && <span className="caps theme-card-tag">custom</span>}
      <div className="theme-card-swatches" aria-hidden="true">
        {SWATCH_KEYS.map((k) => (
          <span
            key={k}
            className="theme-card-swatch"
            style={{ background: (theme.chrome as unknown as Record<string, string>)[k] }}
          />
        ))}
      </div>
      {isCustom && !isActive && onDelete && (
        <button
          type="button"
          className="theme-card-delete"
          aria-label={`delete ${theme.label}`}
          onClick={(e) => {
            e.stopPropagation();
            onDelete();
          }}
        >
          ×
        </button>
      )}
    </div>
  );
}

// Swatch trio on each card — surface / accent / warn read as a theme
// fingerprint at a glance.
const SWATCH_KEYS = ['surface', 'accent', 'warn'];

// Grouped color slots for the editor grid. Each group renders as a
// labeled section with the slots inside. Keeping the keys here
// rather than scattered across the JSX means adding a new chrome
// slot is a one-line change.
const CHROME_GROUPS: { label: string; keys: string[] }[] = [
  {
    label: 'surfaces',
    keys: ['surfaceDeep', 'surface', 'surfacePane', 'surfaceLift', 'surfaceEmphasis'],
  },
  {
    label: 'text',
    keys: ['textStrong', 'text', 'textMuted', 'textFaint', 'textDim'],
  },
  {
    label: 'borders',
    keys: ['borderSoft', 'border', 'borderStrong', 'borderHover'],
  },
  {
    label: 'accent',
    keys: ['accent', 'accentSoft'],
  },
  {
    label: 'semantic',
    keys: ['warn', 'danger', 'info', 'success'],
  },
];

const XTERM_GROUPS: { label: string; keys: string[] }[] = [
  {
    label: 'terminal surfaces',
    keys: ['background', 'foreground', 'cursor', 'cursorAccent'],
  },
  {
    label: 'selection',
    keys: ['selectionBackground', 'selectionForeground'],
  },
  {
    label: 'ANSI 0-7',
    keys: ['black', 'red', 'green', 'yellow', 'blue', 'magenta', 'cyan', 'white'],
  },
  {
    label: 'ANSI 8-15 (bright)',
    keys: [
      'brightBlack',
      'brightRed',
      'brightGreen',
      'brightYellow',
      'brightBlue',
      'brightMagenta',
      'brightCyan',
      'brightWhite',
    ],
  },
];

function ThemeEditor({
  theme,
  onMetaChange,
  onColorChange,
}: {
  theme: AppTheme;
  onMetaChange: (label: string, description: string) => void;
  onColorChange: (slot: 'xterm' | 'chrome', key: string, value: string) => void;
}) {
  return (
    <div className="theme-editor">
      <div className="theme-meta">
        <label className="theme-field theme-field-mono">
          <span className="caps">label</span>
          <input
            type="text"
            spellCheck={false}
            value={theme.label}
            onChange={(e) => onMetaChange(e.target.value, theme.description)}
          />
        </label>
        <label className="theme-field">
          <span className="caps">description</span>
          <input
            type="text"
            spellCheck={false}
            value={theme.description}
            onChange={(e) => onMetaChange(theme.label, e.target.value)}
          />
        </label>
      </div>

      <div className="theme-editor-section">
        <div className="caps">chrome</div>
        <div className="color-groups">
          {CHROME_GROUPS.map((g) => (
            <ColorGroup
              key={g.label}
              label={g.label}
              slot="chrome"
              keys={g.keys}
              values={theme.chrome as unknown as Record<string, string>}
              onColorChange={onColorChange}
            />
          ))}
        </div>
      </div>

      <div className="theme-editor-section">
        <div className="caps">terminal</div>
        <div className="color-groups">
          {XTERM_GROUPS.map((g) => (
            <ColorGroup
              key={g.label}
              label={g.label}
              slot="xterm"
              keys={g.keys}
              values={theme.xterm as unknown as Record<string, string>}
              onColorChange={onColorChange}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

function ColorGroup({
  label,
  slot,
  keys,
  values,
  onColorChange,
}: {
  label: string;
  slot: 'xterm' | 'chrome';
  keys: string[];
  values: Record<string, string>;
  onColorChange: (slot: 'xterm' | 'chrome', key: string, value: string) => void;
}) {
  return (
    <div className="color-group">
      <div className="color-group-label">{label}</div>
      <div className="color-group-grid">
        {keys.map((k) => (
          <ColorSlot
            key={k}
            slotName={k}
            value={values[k] ?? '#000000'}
            onChange={(v) => onColorChange(slot, k, v)}
          />
        ))}
      </div>
    </div>
  );
}

function ColorSlot({
  slotName,
  value,
  onChange,
}: {
  slotName: string;
  value: string;
  onChange: (v: string) => void;
}) {
  // Native color inputs need a 6-digit hex; rgba() values from
  // accentSoft etc. don't parse. Strip alpha when feeding the
  // picker; user can still edit the raw value via the text field.
  const hex = toHexLike(value);
  return (
    <label className="color-slot" title={slotName}>
      <input type="color" value={hex} onChange={(e) => onChange(e.target.value)} />
      <ColorTextField
        className="color-slot-text"
        value={value}
        allowEmpty={false}
        onCommit={onChange}
      />
      <span className="color-slot-name">{slotName}</span>
    </label>
  );
}

function toHexLike(value: string): string {
  // Already 7-char hex.
  if (/^#[0-9a-f]{6}$/i.test(value)) return value;
  // 3-char hex; expand.
  const short = /^#([0-9a-f])([0-9a-f])([0-9a-f])$/i.exec(value);
  if (short) return `#${short[1]}${short[1]}${short[2]}${short[2]}${short[3]}${short[3]}`;
  // rgba() — strip alpha, sample the rgb.
  const rgba = /rgba?\(([^,]+),([^,]+),([^,)]+)/i.exec(value);
  if (rgba) {
    const [r, g, b] = [Number(rgba[1]), Number(rgba[2]), Number(rgba[3])];
    if ([r, g, b].every((n) => Number.isFinite(n))) {
      const h = (n: number) =>
        Math.max(0, Math.min(255, Math.round(n)))
          .toString(16)
          .padStart(2, '0');
      return `#${h(r)}${h(g)}${h(b)}`;
    }
  }
  return '#000000';
}

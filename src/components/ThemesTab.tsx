import { emit } from '@tauri-apps/api/event';
import { useRef, useState } from 'react';
import {
  BUILTIN_THEMES,
  customToAppTheme,
  findTheme,
  setCustomThemes,
  type AppTheme,
} from '../lib/themes';
import { applyTheme } from '../lib/theme';
import { setUiConfig, type CustomTheme, type UiConfig } from '../lib/session';

interface Props {
  config: UiConfig | null;
  setConfig: (updater: (prev: UiConfig | null) => UiConfig | null) => void;
  onError: (e: string | null) => void;
}

// Settings tab for the theme catalog + custom theme editor. Lists
// built-in themes (read-only) and user-authored ones below them.
// Selecting any row sets the active theme. Custom rows expand to a
// color-picker grid grouped by role (surfaces / text / borders /
// accent / semantic / terminal surfaces / ANSI 16).
//
// Edits live-apply to the running window via applyTheme + the
// theme-changed event so the user sees the result instantly. The
// debounced save path persists changes through ui_set_config.
export function ThemesTab({ config, setConfig, onError }: Props) {
  const [editing, setEditing] = useState<string | null>(null);
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
          void emit('vosh://theme-changed', merged.id);
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
      const updated = { ...prev, custom_themes: [...prev.custom_themes, newTheme] };
      setCustomThemes(updated.custom_themes.map(customToAppTheme));
      persist(updated);
      return updated;
    });
    setEditing(newId);
  };

  const handleDelete = (id: string) => {
    // No confirm dialog — WKWebView's `confirm()` is unreliable
    // (sometimes returns undefined without showing) and the action
    // is recoverable by recreating the theme from any base.
    setConfig((prev) => {
      if (!prev) return prev;
      const next = prev.custom_themes.filter((t) => t.id !== id);
      setCustomThemes(next.map(customToAppTheme));
      const nextTheme = prev.theme === id ? 'kanso-zen' : prev.theme;
      if (prev.theme === id) {
        applyTheme(nextTheme);
        void emit('vosh://theme-changed', nextTheme);
      }
      const updated = { ...prev, custom_themes: next, theme: nextTheme };
      persist(updated);
      return updated;
    });
    if (editing === id) setEditing(null);
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

  return (
    <div className="themes-tab">
      <div className="themes-help">
        Built-in themes ship with the app and are read-only. Custom themes can be created from any
        starting point, then every chrome and terminal color slot can be tuned to taste. Every
        change saves automatically.
      </div>

      <div className="themes-section-title">built-in</div>
      <div className="themes-list">
        {BUILTIN_THEMES.map((t) => (
          <ThemeRow
            key={t.id}
            theme={t}
            isActive={config.theme === t.id}
            isCustom={false}
            isEditing={false}
            onSelect={() => handleSelect(t.id)}
          />
        ))}
      </div>

      <div className="themes-section-title themes-section-title-row">
        <span>custom</span>
        <button type="button" className="settings-btn" onClick={handleCreate}>
          [+ new from active]
        </button>
      </div>
      <div className="themes-list">
        {config.custom_themes.length === 0 && (
          <div className="settings-font-empty">no custom themes yet</div>
        )}
        {config.custom_themes.map((t) => (
          <ThemeRow
            key={t.id}
            theme={customToAppTheme(t)}
            isActive={config.theme === t.id}
            isCustom
            isEditing={editing === t.id}
            onSelect={() => handleSelect(t.id)}
            onEditToggle={() => setEditing(editing === t.id ? null : t.id)}
            onDelete={() => handleDelete(t.id)}
            onMetaChange={(label, description) => updateCustom(t.id, { label, description })}
            onColorChange={(slot, key, value) => {
              const next = { ...t[slot], [key]: value };
              updateCustom(t.id, { [slot]: next } as Partial<CustomTheme>);
            }}
          />
        ))}
      </div>

      <SplitDividerRow config={config} setConfig={setConfig} onError={onError} />
      <InputEchoColorRow config={config} setConfig={setConfig} onError={onError} />
    </div>
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
        <input
          type="text"
          className="settings-color-text"
          spellCheck={false}
          placeholder="theme default (#rrggbb, rgba, named)"
          value={value}
          onChange={(e) => update(e.target.value || null)}
        />
        <button
          type="button"
          className="settings-btn settings-btn-mute"
          onClick={() => update(null)}
        >
          [clear]
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
        <input
          type="text"
          className="settings-color-text"
          spellCheck={false}
          placeholder="theme default (#rrggbb, rgba, named)"
          value={value}
          onChange={(e) => update(e.target.value || null)}
        />
        <button
          type="button"
          className="settings-btn settings-btn-mute"
          onClick={() => update(null)}
        >
          [clear]
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

interface RowProps {
  theme: AppTheme;
  isActive: boolean;
  isCustom: boolean;
  isEditing: boolean;
  onSelect: () => void;
  onEditToggle?: () => void;
  onDelete?: () => void;
  onMetaChange?: (label: string, description: string) => void;
  onColorChange?: (slot: 'xterm' | 'chrome', key: string, value: string) => void;
}

function ThemeRow({
  theme,
  isActive,
  isCustom,
  isEditing,
  onSelect,
  onEditToggle,
  onDelete,
  onMetaChange,
  onColorChange,
}: RowProps) {
  return (
    <div className={`theme-row${isActive ? ' is-active' : ''}`}>
      <div className="theme-row-head">
        <button
          type="button"
          className="theme-row-name"
          onClick={onSelect}
          title={theme.description}
        >
          <span className="theme-row-marker">{isActive ? '●' : '○'}</span>
          <span>{theme.label}</span>
          <span className="theme-row-id">{theme.id}</span>
        </button>
        <div className="theme-row-swatches" aria-hidden="true">
          {SWATCH_KEYS.map((k) => (
            <span
              key={k}
              className="theme-row-swatch"
              style={{ background: (theme.chrome as unknown as Record<string, string>)[k] }}
            />
          ))}
        </div>
        {isCustom && (
          <div className="theme-row-actions">
            <button type="button" className="settings-btn" onClick={onEditToggle}>
              {isEditing ? '[done]' : '[edit]'}
            </button>
            {!isActive && (
              <button type="button" className="settings-btn settings-btn-danger" onClick={onDelete}>
                [delete]
              </button>
            )}
          </div>
        )}
      </div>
      {isEditing && isCustom && onColorChange && onMetaChange && (
        <ThemeEditor theme={theme} onMetaChange={onMetaChange} onColorChange={onColorChange} />
      )}
    </div>
  );
}

// Small swatch row in each theme header — picks colors that read
// well at a glance (surface + accent + text-strong + warn + danger).
const SWATCH_KEYS = ['surface', 'accent', 'textStrong', 'warn', 'danger'];

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
      <div className="theme-editor-meta">
        <label className="theme-editor-field">
          <span>label</span>
          <input
            type="text"
            spellCheck={false}
            value={theme.label}
            onChange={(e) => onMetaChange(e.target.value, theme.description)}
          />
        </label>
        <label className="theme-editor-field">
          <span>description</span>
          <input
            type="text"
            spellCheck={false}
            value={theme.description}
            onChange={(e) => onMetaChange(theme.label, e.target.value)}
          />
        </label>
      </div>

      <div className="theme-editor-section-title">chrome</div>
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

      <div className="theme-editor-section-title">terminal</div>
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
      <input
        type="text"
        className="color-slot-text"
        spellCheck={false}
        value={value}
        onChange={(e) => onChange(e.target.value)}
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

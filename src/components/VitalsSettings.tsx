import { useEffect, useState, type ReactNode } from 'react';
import { tokenizeTemplate } from '../lib/vitalsTemplate';
import { colorForVital, colorForPercent } from '../lib/vitalsColor';
import { loadFontStack } from '../lib/fontLoader';
import {
  setUiConfig,
  DEFAULT_VITALS_CONFIG,
  DEFAULT_VITALS_TEMPLATE,
  type UiConfig,
  type VitalsConfig,
  type VitalsPctChipStyle,
} from '../lib/session';

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

// Segmented pill button used across the vitals rows. `on` paints the
// accent state; multi-select rows (columns) just toggle each pill
// independently, enum rows pick one.
function SegBtn({
  on,
  onClick,
  disabled,
  children,
}: {
  on: boolean;
  onClick: () => void;
  disabled?: boolean;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      className={`settings-seg-btn${on ? ' is-on' : ''}`}
      aria-pressed={on}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

// Compact number field with stepper buttons. Used for bar width.
function NumberStepper({
  value,
  min,
  max,
  onChange,
  ariaLabel,
}: {
  value: number;
  min: number;
  max: number;
  onChange: (n: number) => void;
  ariaLabel?: string;
}) {
  const clamp = (n: number) => Math.max(min, Math.min(max, Math.floor(n)));
  return (
    <span className="settings-stepper">
      <input
        type="number"
        className="settings-stepper-input"
        min={min}
        max={max}
        value={value}
        onChange={(e) => onChange(clamp(Number(e.target.value) || min))}
        aria-label={ariaLabel}
      />
      <button
        type="button"
        className="settings-stepper-btn"
        tabIndex={-1}
        aria-label="decrease"
        onClick={() => onChange(clamp(value - 1))}
      >
        −
      </button>
      <button
        type="button"
        className="settings-stepper-btn"
        tabIndex={-1}
        aria-label="increase"
        onClick={() => onChange(clamp(value + 1))}
      >
        +
      </button>
    </span>
  );
}

// Vitals appearance settings. Clean row + segmented-pill layout: the
// common controls (columns, bar style, glyphs, layout, percent) sit as
// rows with a live inline preview below, and the rarely-touched knobs
// (colors, bar font, inline sub-styles, percent chip, template) fold
// into the Advanced disclosure. Each edit autosaves through setUiConfig
// and broadcasts via vosh://vitals-config-changed so VitalsBar redraws
// live.
export function VitalsConfigSection({
  config,
  setConfig,
  onError,
}: {
  config: UiConfig | null;
  setConfig: (updater: (prev: UiConfig | null) => UiConfig | null) => void;
  onError: (e: string | null) => void;
}) {
  // Inject @font-face for any system font named in the bar font stack
  // so the in-Settings preview can resolve it (WKWebView refuses bare
  // family names). Mirrors the same call in VitalsBar.
  useEffect(() => {
    if (config?.vitals.bar_font) loadFontStack(config.vitals.bar_font);
  }, [config?.vitals.bar_font]);
  if (!config) return null;
  const v = config.vitals;
  const apply = (patch: Partial<VitalsConfig>) => {
    const next: UiConfig = { ...config, vitals: { ...v, ...patch } };
    setConfig(() => next);
    void setUiConfig(next).catch((e) => onError(String(e)));
  };
  const track = v.bar_style === 'track';
  const inline = v.layout === 'inline';
  // Ember draws fixed 4px track bars with per-vital fixed colors, so
  // the columns / bar style / glyph / width rows all fall away.
  const ember = v.layout === 'ember';
  const glyphIdx = VITALS_GLYPH_PRESETS.findIndex(
    (p) => p.filled === v.bar_filled && p.empty === v.bar_empty,
  );
  const glyphValue = glyphIdx >= 0 ? String(glyphIdx) : '__custom';
  const fontPresetKey = barFontKeyFor(v);

  return (
    <div className="vitals-settings">
      {!ember && (
        <div className="settings-row vitals-seg-row">
          <span className="settings-row-label">columns</span>
          <span className="settings-row-control">
            <span className="settings-seg">
              <SegBtn on={v.show_bar} onClick={() => apply({ show_bar: !v.show_bar })}>
                bar
              </SegBtn>
              <SegBtn on={v.show_percent} onClick={() => apply({ show_percent: !v.show_percent })}>
                percent
              </SegBtn>
              <SegBtn on={v.show_numeric} onClick={() => apply({ show_numeric: !v.show_numeric })}>
                numeric
              </SegBtn>
              <SegBtn on={v.show_delta} onClick={() => apply({ show_delta: !v.show_delta })}>
                delta
              </SegBtn>
            </span>
          </span>
        </div>
      )}

      {!ember && (
        <div className="settings-row vitals-seg-row">
          <span className="settings-row-label">bar style</span>
          <span className="settings-row-control">
            <span className="settings-seg">
              <SegBtn on={v.bar_style === 'solid'} onClick={() => apply({ bar_style: 'solid' })}>
                solid
              </SegBtn>
              <SegBtn on={v.bar_style === 'ramped'} onClick={() => apply({ bar_style: 'ramped' })}>
                ramped
              </SegBtn>
              <SegBtn on={track} onClick={() => apply({ bar_style: 'track' })}>
                track
              </SegBtn>
            </span>
          </span>
        </div>
      )}

      {!ember && !track && (
        <div className="settings-row vitals-seg-row">
          <span className="settings-row-label">glyphs</span>
          <span className="settings-row-control">
            <select
              value={glyphValue}
              disabled={!v.show_bar}
              aria-label="bar glyphs"
              onChange={(e) => {
                const p = VITALS_GLYPH_PRESETS[Number(e.target.value)];
                if (p) apply({ bar_filled: p.filled, bar_empty: p.empty });
              }}
            >
              {VITALS_GLYPH_PRESETS.map((p, i) => (
                <option key={p.label} value={i}>
                  {p.filled} {p.empty} · {p.label}
                </option>
              ))}
              {glyphValue === '__custom' && (
                <option value="__custom">
                  {v.bar_filled} {v.bar_empty} · custom
                </option>
              )}
            </select>
            <NumberStepper
              value={v.bar_width}
              min={4}
              max={60}
              onChange={(n) => apply({ bar_width: n })}
              ariaLabel="bar width"
            />
            <span className="settings-paste-hint">cells</span>
          </span>
        </div>
      )}
      {!ember && track && (
        <div className="settings-row vitals-seg-row">
          <span className="settings-row-label">width</span>
          <span className="settings-row-control">
            <NumberStepper
              value={v.bar_width}
              min={4}
              max={60}
              onChange={(n) => apply({ bar_width: n })}
              ariaLabel="bar width"
            />
            <span className="settings-paste-hint">cells</span>
          </span>
        </div>
      )}

      <div className="settings-row vitals-seg-row">
        <span className="settings-row-label">layout</span>
        <span className="settings-row-control">
          <span className="settings-seg">
            <SegBtn
              on={ember}
              disabled={v.template_enabled}
              onClick={() => apply({ layout: 'ember' })}
            >
              ember
            </SegBtn>
            <SegBtn
              on={!inline && !ember}
              disabled={v.template_enabled}
              onClick={() => apply({ layout: 'stacked' })}
            >
              stacked
            </SegBtn>
            <SegBtn
              on={inline}
              disabled={v.template_enabled}
              onClick={() => apply({ layout: 'inline' })}
            >
              inline
            </SegBtn>
          </span>
          {ember && (
            <span className="settings-paste-hint">the ember layout draws fixed thin bars</span>
          )}
          {!track && !inline && !ember && (
            <>
              <span className="settings-paste-hint">grid</span>
              <span className="settings-seg">
                <SegBtn
                  on={v.bar_layout === 'plain'}
                  onClick={() => apply({ bar_layout: 'plain' })}
                >
                  plain
                </SegBtn>
                <SegBtn
                  on={v.bar_layout === 'with_history'}
                  onClick={() => apply({ bar_layout: 'with_history' })}
                >
                  + history
                </SegBtn>
              </span>
            </>
          )}
        </span>
      </div>

      <div className="settings-row vitals-seg-row">
        <span className="settings-row-label">percent</span>
        <span className="settings-row-control">
          <span className="settings-seg">
            <SegBtn
              on={v.percent_color === 'fill'}
              onClick={() => apply({ percent_color: 'fill' })}
            >
              per-vital
            </SegBtn>
            <SegBtn
              on={v.percent_color === 'gradient'}
              onClick={() => apply({ percent_color: 'gradient' })}
            >
              red → green
            </SegBtn>
          </span>
        </span>
      </div>

      <div className="vitals-preview-block">
        <div className="vitals-preview-block-label">live preview</div>
        <VitalsPreview config={v} />
      </div>

      <details className="vitals-advanced">
        <summary className="vitals-advanced-summary">advanced</summary>
        <div className="vitals-advanced-body">
          <div className="vitals-advanced-label">colors</div>
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
          <label className="settings-checkbox vitals-advanced-check">
            <input
              type="checkbox"
              checked={v.use_color_ramp}
              onChange={(e) => apply({ use_color_ramp: e.target.checked })}
            />
            <span>drain through red as bars empty</span>
          </label>

          {!track && glyphValue === '__custom' && (
            <div className="settings-row vitals-seg-row">
              <span className="settings-row-label">custom</span>
              <span className="settings-row-control">
                <input
                  type="text"
                  className="panels-vitals-glyph-input"
                  value={v.bar_filled}
                  disabled={!v.show_bar}
                  spellCheck={false}
                  aria-label="filled glyph"
                  onChange={(e) =>
                    apply({ bar_filled: e.target.value.length > 0 ? e.target.value : '▰' })
                  }
                />
                <input
                  type="text"
                  className="panels-vitals-glyph-input"
                  value={v.bar_empty}
                  disabled={!v.show_bar}
                  spellCheck={false}
                  aria-label="empty glyph"
                  onChange={(e) =>
                    apply({ bar_empty: e.target.value.length > 0 ? e.target.value : '▱' })
                  }
                />
                <span className="settings-paste-hint">filled / empty glyph</span>
              </span>
            </div>
          )}

          <div className="vitals-advanced-label">bar font</div>
          <div className="settings-row vitals-seg-row">
            <span className="settings-row-label">font</span>
            <span className="settings-row-control">
              <select
                value={fontPresetKey}
                disabled={!v.show_bar}
                aria-label="bar font"
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
                  <option value="__custom">custom (edit below)</option>
                )}
              </select>
            </span>
          </div>
          {fontPresetKey === 'monolisa' && (
            <div className="vitals-advanced-hint">
              <BarFontMonoLisaStatus />
            </div>
          )}
          <details className="vsplit-adv">
            <summary>custom CSS font-family</summary>
            <input
              type="text"
              className="panels-vitals-glyph-input vsplit-adv-input"
              spellCheck={false}
              value={v.bar_font}
              disabled={!v.show_bar}
              placeholder='blank = app font · or "MonoLisa", "Iosevka", monospace'
              onChange={(e) => apply({ bar_font: e.target.value })}
            />
          </details>

          {inline && (
            <>
              <div className="vitals-advanced-label">inline</div>
              <div className="settings-row vitals-seg-row">
                <span className="settings-row-label">style</span>
                <span className="settings-row-control">
                  <span className="settings-seg">
                    <SegBtn
                      on={v.inline_style === 'plain'}
                      disabled={v.template_enabled}
                      onClick={() => apply({ inline_style: 'plain' })}
                    >
                      plain
                    </SegBtn>
                    <SegBtn
                      on={v.inline_style === 'drain'}
                      disabled={v.template_enabled}
                      onClick={() => apply({ inline_style: 'drain' })}
                    >
                      drain
                    </SegBtn>
                    <SegBtn
                      on={v.inline_style === 'badge'}
                      disabled={v.template_enabled}
                      onClick={() => apply({ inline_style: 'badge' })}
                    >
                      badge
                    </SegBtn>
                  </span>
                </span>
              </div>
              {v.inline_style !== 'plain' && (
                <div className="settings-row vitals-seg-row">
                  <span className="settings-row-label">% mode</span>
                  <span className="settings-row-control">
                    <span className="settings-seg">
                      <SegBtn
                        on={v.percent_color_mode === 'drain'}
                        onClick={() => apply({ percent_color_mode: 'drain' })}
                      >
                        drain
                      </SegBtn>
                      <SegBtn
                        on={v.percent_color_mode === 'stat'}
                        onClick={() => apply({ percent_color_mode: 'stat' })}
                      >
                        stat
                      </SegBtn>
                      <SegBtn
                        on={v.percent_color_mode === 'accent'}
                        onClick={() => apply({ percent_color_mode: 'accent' })}
                      >
                        accent
                      </SegBtn>
                    </span>
                  </span>
                </div>
              )}
            </>
          )}

          {(v.template_enabled || (inline && v.inline_style === 'plain')) && (
            <div className="settings-row vitals-seg-row">
              <span className="settings-row-label">% chip</span>
              <span className="settings-row-control">
                <span className="settings-seg">
                  <SegBtn
                    on={v.pct_chip_style === 'none'}
                    onClick={() => apply({ pct_chip_style: 'none' })}
                  >
                    none
                  </SegBtn>
                  <SegBtn
                    on={v.pct_chip_style === 'pill'}
                    onClick={() => apply({ pct_chip_style: 'pill' })}
                  >
                    pill
                  </SegBtn>
                  <SegBtn
                    on={v.pct_chip_style === 'soft'}
                    onClick={() => apply({ pct_chip_style: 'soft' })}
                  >
                    soft
                  </SegBtn>
                  <SegBtn
                    on={v.pct_chip_style === 'glow'}
                    onClick={() => apply({ pct_chip_style: 'glow' })}
                  >
                    glow
                  </SegBtn>
                  <SegBtn
                    on={v.pct_chip_style === 'drain'}
                    onClick={() => apply({ pct_chip_style: 'drain' })}
                  >
                    drain
                  </SegBtn>
                </span>
              </span>
            </div>
          )}

          <label className="settings-checkbox vitals-advanced-check">
            <input
              type="checkbox"
              checked={v.low_hp_vignette}
              onChange={(e) => apply({ low_hp_vignette: e.target.checked })}
            />
            <span>pulse red vignette under 30% hp</span>
          </label>

          <div className="vitals-advanced-label">template</div>
          <VitalsTemplateEditor config={v} apply={apply} />

          <div className="settings-actions">
            <button
              type="button"
              className="settings-btn settings-btn-mute"
              onClick={() => apply(DEFAULT_VITALS_CONFIG)}
            >
              [reset vitals]
            </button>
          </div>
        </div>
      </details>
    </div>
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
  label: PreviewLabel;
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
        if (config.pct_chip_style !== 'none') {
          return (
            <span className="vitals-pct-chip-host">
              <PreviewPercentChip value={s.value} color={c} style={config.pct_chip_style} />
            </span>
          );
        }
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

// Mirrors the runtime PercentChip from VitalsBar.tsx so the
// Settings preview shows the user the exact chip they will see
// in-game. Kept here (rather than imported) to avoid pulling the
// runtime component's session-related deps into Settings.
function PreviewPercentChip({
  value,
  color,
  style,
}: {
  value: number;
  color: string;
  style: VitalsPctChipStyle;
}) {
  if (style === 'none') {
    return (
      <span className="vitals-inline-pct" style={{ color }}>
        ({value}%)
      </span>
    );
  }
  const styleVars: React.CSSProperties = {
    color,
    ['--pct-color' as string]: color,
    ['--pct-fill' as string]: `${Math.max(0, Math.min(100, value))}%`,
  };
  return (
    <span className={`vitals-pct-chip vitals-pct-chip-${style}`} style={styleVars}>
      {value}%
    </span>
  );
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

  // Ember preview — mirrors the runtime ember branch in VitalsBar:
  // pane head over three thin track bars with fixed per-vital fills.
  // The track itself is the drag hit-target (ember has no cell
  // width, so the ch-based draggableBar wrapper does not apply).
  if (config.layout === 'ember') {
    return (
      <>
        <div className="vitals-preview-head">{headerText}</div>
        <div className="vitals-bar vitals-ember">
          <div className="vitals-ember-head">
            <span className="caps">vitals</span>
            <span className="vitals-ember-tick">tick 12s</span>
          </div>
          <div className="vitals-ember-rows">
            {sample.map((s) => (
              <div key={s.label} className="vitals-ember-row">
                <span className="vitals-ember-label">{s.label}</span>
                <div
                  className="vitals-ember-track"
                  onMouseDown={startDrag(s.label)}
                  role="slider"
                  aria-label={`${s.label} fill percent`}
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={s.value}
                >
                  <div
                    className={`vitals-ember-fill vitals-ember-fill-${s.label}`}
                    style={{ width: `${s.value}%` }}
                  />
                </div>
                <span className="vitals-ember-value">
                  {s.cur}/{s.max}
                </span>
              </div>
            ))}
          </div>
        </div>
      </>
    );
  }

  if (config.layout === 'inline') {
    // Mirror the runtime resolvePercentColor in VitalsBar.tsx so the
    // preview shows what the user is about to see.
    const previewPercentColor = (label: string, value: number): string => {
      switch (config.percent_color_mode) {
        case 'stat':
          return colorForVital(label, 100, config);
        case 'accent':
          return 'var(--c-accent)';
        case 'drain':
        default:
          if (value >= 75) return colorForVital(label, 100, config);
          if (value >= 50) return '#e6c384';
          if (value >= 25) return '#d99268';
          return '#e46876';
      }
    };
    return (
      <>
        <div className="vitals-preview-head">{headerText}</div>
        <div className={`vitals-bar vitals-bar-inline vitals-inline-${config.inline_style}`}>
          <div className="vitals-row vitals-row-inline">
            {sample.map((s) => {
              const statColor = colorForVital(s.label, 100, config);
              const pctColor =
                config.inline_style === 'plain'
                  ? gradient
                    ? s.color
                    : colorForVital(s.label, s.value, config)
                  : previewPercentColor(s.label, s.value);
              if (config.inline_style === 'badge') {
                const isDanger = s.value < 20;
                return (
                  <span
                    key={s.label}
                    className={`vitals-inline-badge${isDanger ? ' is-danger' : ''}`}
                    style={{ color: statColor }}
                  >
                    <span className="vitals-inline-badge-cap">{s.label}</span>
                    {(config.show_numeric || !config.show_percent) && (
                      <span className="vitals-inline-badge-val" style={{ color: statColor }}>
                        {s.cur}
                        <span className="vitals-inline-badge-max">/{s.max}</span>
                      </span>
                    )}
                    {config.show_percent && (
                      <span
                        className="vitals-inline-badge-pct"
                        style={{
                          color: pctColor,
                          borderColor: isDanger ? 'var(--c-danger)' : undefined,
                        }}
                      >
                        {s.value}%
                      </span>
                    )}
                    {config.show_delta && s.delta !== 0 && (
                      <span
                        className={`vitals-delta${s.delta > 0 ? ' vitals-delta-up' : ' vitals-delta-down'}`}
                      >
                        {s.delta > 0 ? '+' : ''}
                        {s.delta}
                      </span>
                    )}
                  </span>
                );
              }
              if (config.inline_style === 'drain') {
                const isDanger = s.value < 20;
                return (
                  <span
                    key={s.label}
                    className={`vitals-inline-drain${isDanger ? ' is-danger' : ''}`}
                    style={
                      {
                        ['--vital-color']: statColor,
                        ['--vital-fill']: `${s.value}%`,
                      } as React.CSSProperties
                    }
                  >
                    <span className="vitals-inline-drain-bg" aria-hidden="true" />
                    <span className="vitals-inline-drain-top">
                      <span className="vitals-inline-drain-cap">{s.label}</span>
                      {config.show_percent && (
                        <span className="vitals-inline-drain-pct" style={{ color: pctColor }}>
                          {s.value}%
                        </span>
                      )}
                    </span>
                    <span className="vitals-inline-drain-val" style={{ color: statColor }}>
                      {config.show_numeric ? (
                        <>
                          {s.cur}
                          <span className="vitals-inline-drain-max">/{s.max}</span>
                        </>
                      ) : (
                        `${s.value}%`
                      )}
                      {config.show_delta && s.delta !== 0 && (
                        <span
                          className={`vitals-delta${s.delta > 0 ? ' vitals-delta-up' : ' vitals-delta-down'}`}
                        >
                          {s.delta > 0 ? '+' : ''}
                          {s.delta}
                        </span>
                      )}
                    </span>
                  </span>
                );
              }
              return (
                <span key={s.label} className="vitals-inline-chip">
                  {config.show_numeric && <span className="vitals-numeric">{s.cur}</span>}
                  {config.show_percent && (
                    <PreviewPercentChip
                      value={s.value}
                      color={pctColor}
                      style={config.pct_chip_style}
                    />
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
              );
            })}
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

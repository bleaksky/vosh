import { useContext, useId } from 'react';
import { HIGHLIGHT_COLORS } from '../../automation/automationTriggers';
import { fixedColorHex, flaggedColors } from '../../automation/presetEdits';
import type { Preset, PresetColor } from '../../automation/presets';
import type { PresetEdit } from '../../ipc/presetEdits';
import { ColorField, cx, Select } from '../../ui';
import { FixChoice } from './fields';
import { SamplePaintContext } from './samplePaint';

// The Colors block of a preset's card (Presets board 1, Q3, Q4 and Q8):
// one swatch for each color the preset paints, on the grid Appearance
// lays its colors on. A swatch whose color sits in a template takes any
// color and shows the preset's own while you leave it empty. A swatch
// whose color sits in a Highlight picks from the theme's sixteen. A
// changed swatch offers the preset's color back under it. A swatch a fix
// changed under your color wears the warn ring and offers Take the fix
// and Keep mine in its place (board 4).

const NAMES = ['black', 'red', 'green', 'yellow', 'blue', 'magenta', 'cyan', 'white'];

/** The index in the theme's sixteen of a theme color token, as red,
 *  bright_red or bold_red, else -1. */
function themeIndex(token: string): number {
  const name = token.replace(/^bold_/, '');
  const bright = name.startsWith('bright_');
  const at = NAMES.indexOf(bright ? name.slice(7) : name);
  return at < 0 ? -1 : at + (bright ? 8 : 0);
}

/** A theme color token in words, as `red` or `bright red`. */
function themeWords(token: string): string {
  return token.replace(/^bold_/, '').replace('_', ' ');
}

/** The preset's color as its back link names it: `178`, `#8fa7d9`, or
 *  a theme color in words. */
function backName(token: string): string {
  const fixed = /^fg:(\d+)$/.exec(token);
  if (fixed) return fixed[1];
  return themeIndex(token) >= 0 ? `theme ${themeWords(token)}` : token;
}

/** What an empty template swatch says: `Theme red`, `244, #808080` or
 *  the true color. */
function placeholder(token: string): string {
  const fixed = /^fg:(\d+)$/.exec(token);
  if (fixed) return `${fixed[1]}, ${fixedColorHex(token) ?? ''}`;
  return themeIndex(token) >= 0 ? `Theme ${themeWords(token)}` : token;
}

/** A color token as CSS: a theme color from your terminal's sixteen, else
 *  its hex. */
function tokenCss(token: string, palette: readonly string[] | undefined): string {
  const at = themeIndex(token);
  if (at >= 0) return palette?.[at] ?? 'transparent';
  return fixedColorHex(token) ?? 'transparent';
}

interface PresetColorsProps {
  preset: Preset;
  edit: PresetEdit | undefined;
  /** Set the swatch `key` to `value`, or clear it with null. */
  onColor: (key: string, value: string | null) => void;
  /** Keep mine on the flagged swatch `key`. */
  onKeep: (key: string) => void;
}

export function PresetColors({ preset, edit, onColor, onKeep }: PresetColorsProps) {
  const descId = useId();
  const keys = Object.keys(preset.colors);
  if (keys.length === 0) return null;
  const flagged = flaggedColors(preset, edit);
  return (
    <div className="st-row st-auto-block">
      <div className="st-row-text">
        <span className="st-row-label">Colors</span>
        <span id={descId} className="st-row-desc">
          Each one paints every trigger of the preset that uses it.
        </span>
      </div>
      <div className="st-color-grid" role="group" aria-label="Colors" aria-describedby={descId}>
        {keys.map((key) => (
          <Swatch
            key={key}
            color={preset.colors[key]}
            value={edit?.colors?.[key]?.value}
            flagged={Object.hasOwn(flagged, key)}
            onColor={(value) => onColor(key, value)}
            onKeep={() => onKeep(key)}
          />
        ))}
      </div>
    </div>
  );
}

function Swatch({
  color,
  value,
  flagged,
  onColor,
  onKeep,
}: {
  color: PresetColor;
  value: unknown;
  /** A fix changed the preset's color under yours. */
  flagged: boolean;
  onColor: (value: string | null) => void;
  onKeep: () => void;
}) {
  const id = useId();
  const palette = useContext(SamplePaintContext)?.palette;
  const own = typeof value === 'string' ? value : undefined;
  const shipped = tokenCss(color.token, palette);
  const back = color.sits === 'highlight' ? themeWords(color.token) : backName(color.token);
  return (
    <div className={cx('st-color-cell', flagged && 'is-warn')}>
      <label htmlFor={id} className="st-color-cell-label">
        {color.label}
      </label>
      {color.sits === 'highlight' ? (
        <Select
          id={id}
          width="100%"
          value={own ?? color.token}
          options={HIGHLIGHT_COLORS}
          swatch={tokenCss(own ?? color.token, palette)}
          onChange={onColor}
        />
      ) : (
        <ColorField
          id={id}
          width="100%"
          value={own ?? ''}
          allowEmpty
          hexOnly
          placeholder={placeholder(color.token)}
          emptySwatch={shipped}
          pickerLabel={`Choose a color for ${color.label.toLowerCase()}`}
          onChange={(next) => onColor(next === '' ? null : next)}
        />
      )}
      {flagged && (
        <p className="st-auto-under">
          <FixChoice verb="has" onTake={() => onColor(null)} onKeep={onKeep}>
            <span className="st-auto-fix-color">
              <span className="st-auto-chip" style={{ background: shipped }} aria-hidden="true" />
              {back}
            </span>
          </FixChoice>
        </p>
      )}
      {own !== undefined && !flagged && (
        <p className="st-auto-under">
          <button
            type="button"
            className="st-auto-link is-back"
            aria-label={`Back to the preset's color, ${back}`}
            onClick={() => onColor(null)}
          >
            <span className="st-auto-chip" style={{ background: shipped }} aria-hidden="true" />
            Back to {back}
          </button>
        </p>
      )}
    </div>
  );
}

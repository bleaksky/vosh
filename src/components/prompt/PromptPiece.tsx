import { useEffect, useState, type ReactNode } from 'react';
import type { BandEnv } from '../../lib/bandCells';
import { indexedRgb } from '../../lib/bandCells';
import {
  breakHint,
  colorHint,
  customColor,
  customText,
  rowsOf,
  swatchOf,
  THEME_SWATCHES,
  WHEN_FIXED_HINT,
} from '../../lib/promptPieces';
import type {
  PromptColorChoice,
  PromptEditOp,
  PromptFormatName,
  PromptPiece,
  PromptStyleChoice,
  PromptWhen,
} from '../../lib/session';
import {
  Button,
  ColorField,
  cx,
  Field,
  NumberField,
  PlusIcon,
  Segmented,
  type SegmentedOption,
} from '../settings/ui';

// The part you picked on your prompt (P5, P7, P8a, P8b, P10). The name line
// says what it is and what it reads now, with the part's own codes as you
// wrote them. The rows show its effective look, so an italic it inherits
// reads as on. A value has Show as, each segment labeled with what it
// draws now, a bar Width in place of Style, text its words, and a line
// break only When. Every change goes through prompt_edit, which keeps
// every other part's look.

const WHEN_OPTIONS: SegmentedOption<PromptWhen>[] = [
  { value: 'always', label: 'Always' },
  { value: 'fight', label: 'In a fight' },
  { value: 'not_fight', label: 'Out of a fight' },
];

const rgbHex = ([r, g, b]: readonly number[]) =>
  `#${[r, g, b].map((n) => n.toString(16).padStart(2, '0')).join('')}`;

interface PromptPieceProps {
  piece: PromptPiece;
  env: BandEnv;
  onEdit: (op: PromptEditOp) => void;
  onInsertValue: () => void;
}

/** One row of the part: a label column of 88 and its control. */
function PieceRow({
  label,
  children,
  first = false,
  htmlFor,
}: {
  label: string;
  children: ReactNode;
  first?: boolean;
  htmlFor?: string;
}) {
  return (
    <div className={cx('pc-piece-row', first && 'is-first')} role="group" aria-label={label}>
      {htmlFor ? (
        <label className="pc-piece-label" htmlFor={htmlFor}>
          {label}
        </label>
      ) : (
        <span className="pc-piece-label" aria-hidden="true">
          {label}
        </span>
      )}
      {children}
    </div>
  );
}

export function PromptPieceBody({ piece, env, onEdit, onInsertValue }: PromptPieceProps) {
  const rows = rowsOf(piece);
  const at = piece.piece;
  const swatch = swatchOf(piece.color);
  const palette = (index: number) => rgbHex(indexedRgb(index, env.palette));
  const custom = customText(piece.color, palette);
  const setColor = (color: PromptColorChoice) => onEdit({ op: 'set_color', piece: at, color });
  const setStyle = (style: PromptStyleChoice, on: boolean) =>
    onEdit({ op: 'set_style', piece: at, style, on });
  const breaks = piece.kind === 'nl';
  const hint = breaks ? breakHint(piece.when) : null;
  let first = true;
  const isFirst = () => {
    const was = first;
    first = false;
    return was;
  };
  return (
    <div className="pc-body pc-piece">
      <p className="pc-piece-name">
        <span className="pc-piece-title">{piece.label}</span>
        {piece.meta && <span className="pc-piece-meta">{piece.meta}</span>}
        <span className="pc-spacer" />
        <code className="pc-piece-code">{piece.text}</code>
      </p>
      {rows.showAs && (
        <PieceRow label="Show as" first={isFirst()}>
          <Segmented
            label="Show as"
            className="pc-seg"
            options={piece.forms.map((f) => ({
              value: f.format,
              label: f.segment,
              // The name holds the text the segment shows, then what it
              // is, so you can say what you see to pick it.
              ...(f.segment === f.label ? {} : { name: `${f.segment}, ${f.label}` }),
            }))}
            value={piece.format}
            onChange={(format: PromptFormatName) =>
              onEdit({ op: 'set_format', piece: at, format: { format } })
            }
          />
        </PieceRow>
      )}
      {rows.text && <TextRow piece={piece} first={isFirst()} onEdit={onEdit} />}
      <PieceRow label="When" first={isFirst()}>
        <Segmented
          label="When"
          className="pc-seg"
          options={WHEN_OPTIONS.map((o) => ({ ...o, disabled: piece.when_fixed }))}
          value={piece.when}
          onChange={(when) => onEdit({ op: 'set_when', piece: at, when })}
        />
      </PieceRow>
      {piece.when_fixed && <p className="pc-piece-hint">{WHEN_FIXED_HINT}</p>}
      {hint && <p className="pc-piece-hint is-nowrap">{hint}</p>}
      {rows.width && (
        <PieceRow label="Width" htmlFor={`pc-width-${at}`}>
          <NumberField
            id={`pc-width-${at}`}
            value={piece.width ?? 10}
            min={1}
            max={80}
            unit="cells"
            onChange={(width) =>
              onEdit({ op: 'set_format', piece: at, format: { format: 'bar', width } })
            }
          />
        </PieceRow>
      )}
      {rows.color && (
        <>
          <PieceRow label="Color">
            <span className="pc-swatches">
              <button
                type="button"
                className="pc-swatch"
                aria-label="Terminal text"
                title="Terminal text"
                aria-pressed={swatch === 'default'}
                style={{ background: env.fg }}
                onClick={() => setColor({ kind: 'default' })}
              />
              {piece.by_value && (
                <button
                  type="button"
                  className="pc-swatch is-by-value"
                  aria-label="By value"
                  title="By value"
                  aria-pressed={swatch === 'by_value'}
                  onClick={() => setColor({ kind: 'by_value' })}
                >
                  <span style={{ background: env.palette[2] }} />
                  <span style={{ background: env.palette[3] }} />
                  <span style={{ background: env.palette[1] }} />
                </button>
              )}
            </span>
            <span className="pc-swatches is-theme">
              {THEME_SWATCHES.map((s) => (
                <button
                  key={s.index}
                  type="button"
                  className="pc-swatch"
                  aria-label={s.label}
                  title={s.label}
                  aria-pressed={swatch === s.index}
                  style={{ background: env.palette[s.index] }}
                  onClick={() => setColor({ kind: 'named', index: s.index })}
                />
              ))}
            </span>
            <ColorField
              className={cx('pc-custom', swatch === 'custom' && 'is-on')}
              value={custom}
              width={112}
              placeholder="Custom"
              aria-label="Custom color"
              pickerLabel="Choose a custom color"
              hexOnly
              allowEmpty
              onChange={(hex) => {
                const color = customColor(hex);
                if (color) setColor(color);
              }}
            />
          </PieceRow>
          <p className={cx('pc-piece-hint', swatchOf(piece.color) !== 'by_value' && 'is-nowrap')}>
            {colorHint(piece.color)}
          </p>
        </>
      )}
      {rows.style && (
        <PieceRow label="Style">
          <span className="pc-styles">
            <button
              type="button"
              className="pc-style is-bold"
              aria-label="Bold"
              aria-pressed={piece.bold}
              onClick={() => setStyle('bold', !piece.bold)}
            >
              B
            </button>
            <button
              type="button"
              className="pc-style is-italic"
              aria-label="Italic"
              aria-pressed={piece.italic}
              onClick={() => setStyle('italic', !piece.italic)}
            >
              I
            </button>
            <button
              type="button"
              className="pc-style is-underline"
              aria-label="Underline"
              aria-pressed={piece.underline}
              onClick={() => setStyle('underline', !piece.underline)}
            >
              <span>U</span>
            </button>
          </span>
        </PieceRow>
      )}
      <div className="pc-piece-actions">
        <Button icon={<PlusIcon />} onClick={onInsertValue}>
          Insert value…
        </Button>
        <Button variant="danger" onClick={() => onEdit({ op: 'remove', piece: at })}>
          Remove
        </Button>
      </div>
    </div>
  );
}

/** A text part's words, in the terminal's face. Each change saves as you
 *  type. Emptying it removes the part. */
function TextRow({
  piece,
  first,
  onEdit,
}: {
  piece: PromptPiece;
  first: boolean;
  onEdit: (op: PromptEditOp) => void;
}) {
  const [draft, setDraft] = useState(piece.literal ?? '');
  // A change from elsewhere, an undo among them, replaces the draft.
  useEffect(() => setDraft(piece.literal ?? ''), [piece.literal]);
  const id = `pc-text-${piece.piece}`;
  return (
    <PieceRow label="Text" first={first} htmlFor={id}>
      <Field
        id={id}
        mono
        width={440}
        value={draft}
        spellCheck={false}
        onChange={(text: string) => {
          setDraft(text);
          if (text.length > 0) onEdit({ op: 'set_text', piece: piece.piece, text });
        }}
        onBlur={() => {
          if (draft.length === 0) onEdit({ op: 'set_text', piece: piece.piece, text: '' });
        }}
      />
    </PieceRow>
  );
}

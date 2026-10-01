import { useEffect, useState, type ReactNode } from 'react';
import type { BandEnv } from '../../lib/bandCells';
import { indexedRgb } from '../../lib/bandCells';
import {
  breakHint,
  colorHex,
  colorHint,
  customColor,
  customText,
  groundSwatchOf,
  MORE_STYLES,
  moreLabel,
  rowsOf,
  swatchOf,
  THEME_SWATCHES,
  UNDERLINE_KINDS,
  WHEN_FIXED_HINT,
} from '../../lib/promptPieces';
import type {
  PromptColorChoice,
  PromptEditOp,
  PromptFormatName,
  PromptPiece,
  PromptStyleChoice,
  PromptUnderlineStyle,
  PromptWhen,
} from '../../lib/session';
import {
  Button,
  CheckIcon,
  ChevronDownIcon,
  ColorField,
  cx,
  Field,
  NumberField,
  PlusIcon,
  Segmented,
  type SegmentedOption,
} from '../settings/ui';
import { CardMenu } from './CardMenu';

// The part you picked on your prompt (P5, P7, P8a, P8b, P10). The name line
// says what it is and what it reads now, with the part's own codes as you
// wrote them. The rows show its effective look, so an italic it inherits
// reads as on. A value has Show as, each segment labeled with what it
// draws now, a bar Width in place of Style, text its words, and a line
// break only When. Every change goes through prompt_edit, which keeps
// every other part's look.
//
// The colors and styles follow the styles board: Color and Background
// each take the terminal's own, a theme color or any true color. Style
// keeps B, I and U, and More styles holds strikethrough, dim and reverse.
// While an underline is on, the Underline row picks its kind, each drawn
// in its own line, and its color, empty for the text color.

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

/** What a color paints: the text, its ground, or its underline. */
type Layer = 'text' | 'background' | 'underline';

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
  const ground = groundSwatchOf(piece.background);
  const palette = (index: number) => rgbHex(indexedRgb(index, env.palette));
  const custom = customText(piece.color, palette);
  const groundCustom = customText(piece.background, palette);
  const paint = (layer: Layer) => (color: PromptColorChoice) =>
    onEdit({
      op: 'set_color',
      piece: at,
      color,
      ...(layer === 'background' ? { background: true } : {}),
      ...(layer === 'underline' ? { underline: true } : {}),
    });
  const setColor = paint('text');
  const setGround = paint('background');
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
          {/* The hint stays right under Color, as P5 draws it, since By
              value's rule is the Color row's alone. */}
          <p className={cx('pc-piece-hint', swatchOf(piece.color) !== 'by_value' && 'is-nowrap')}>
            {colorHint(piece.color)}
          </p>
          {rows.background && (
            <PieceRow label="Background">
              <span className="pc-swatches">
                <button
                  type="button"
                  className="pc-swatch"
                  aria-label="Terminal background"
                  title="Terminal background"
                  aria-pressed={ground === 'default'}
                  style={{ background: env.bg }}
                  onClick={() => setGround({ kind: 'default' })}
                />
                {/* Holds By value's place, so the theme colors line up
                    with the Color row's. */}
                {piece.by_value && <span className="pc-swatch-slot" aria-hidden="true" />}
              </span>
              <span className="pc-swatches is-theme">
                {THEME_SWATCHES.map((s) => (
                  <button
                    key={s.index}
                    type="button"
                    className="pc-swatch"
                    aria-label={s.label}
                    title={s.label}
                    aria-pressed={ground === s.index}
                    style={{ background: env.palette[s.index] }}
                    onClick={() => setGround({ kind: 'named', index: s.index })}
                  />
                ))}
              </span>
              <ColorField
                className={cx('pc-custom', ground === 'custom' && 'is-on')}
                value={groundCustom}
                width={112}
                placeholder="Custom"
                aria-label="Custom background"
                pickerLabel="Choose a custom background"
                hexOnly
                allowEmpty
                onChange={(hex) => {
                  const color = customColor(hex);
                  if (color) setGround(color);
                }}
              />
            </PieceRow>
          )}
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
          <MoreStyles piece={piece} onStyle={setStyle} />
        </PieceRow>
      )}
      {rows.underline && (
        <UnderlineRow
          piece={piece}
          text={textHex(piece.color, env, palette)}
          palette={palette}
          onStyle={(style) => setStyle(style, true)}
          onColor={paint('underline')}
        />
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

/** The color a part's text draws in, as #rrggbb: the terminal's text for
 *  its own color, and the full color of By value. */
function textHex(color: PromptColorChoice, env: BandEnv, palette: (index: number) => string) {
  if (color.kind === 'default') return env.fg;
  if (color.kind === 'by_value') return env.palette[2];
  return colorHex(color, palette) || env.fg;
}

/** The Style row's More styles button and its menu: strikethrough, dim
 *  and reverse, each drawn in its own look with a check while it is on.
 *  The button reads the ones that are on, in the pressed look of B, I
 *  and U, so you see them without opening it. */
function MoreStyles({
  piece,
  onStyle,
}: {
  piece: PromptPiece;
  onStyle: (style: PromptStyleChoice, on: boolean) => void;
}) {
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  const label = moreLabel(piece);
  const on = MORE_STYLES.some((s) => piece[s.style]);
  return (
    <>
      <button
        type="button"
        className={cx('pc-style-more', on && 'is-on')}
        aria-haspopup="menu"
        aria-expanded={anchor !== null}
        aria-label={on ? `More styles, ${label} on` : undefined}
        onClick={(e) => setAnchor(anchor ? null : e.currentTarget)}
      >
        <span>{label}</span>
        <ChevronDownIcon size={12} />
      </button>
      {anchor && (
        <CardMenu
          anchor={anchor}
          place="below-start"
          width={184}
          label="More styles"
          onClose={() => setAnchor(null)}
        >
          <MoreStyleItems
            piece={piece}
            onToggle={(style, next) => {
              setAnchor(null);
              onStyle(style, next);
            }}
          />
        </CardMenu>
      )}
    </>
  );
}

/** The items of More styles, a check before each one that is on. */
export function MoreStyleItems({
  piece,
  onToggle,
}: {
  piece: Pick<PromptPiece, 'strike' | 'dim' | 'inverse'>;
  onToggle: (style: PromptStyleChoice, on: boolean) => void;
}) {
  return (
    <>
      {MORE_STYLES.map((s) => {
        const checked = piece[s.style];
        return (
          <li key={s.style} role="none">
            <button
              type="button"
              role="menuitemcheckbox"
              aria-checked={checked}
              className="pc-style-item"
              onClick={() => onToggle(s.style, !checked)}
            >
              {checked && <CheckIcon className="pc-start-check" />}
              <span className={`pc-style-sample is-${s.style}`}>{s.label}</span>
            </button>
          </li>
        );
      })}
    </>
  );
}

/** The Underline row, while an underline is on: its kind, each segment
 *  drawn in its own line and in the underline's color, then the color.
 *  An empty color draws the line in the text's color, which its swatch
 *  shows. */
function UnderlineRow({
  piece,
  text,
  palette,
  onStyle,
  onColor,
}: {
  piece: PromptPiece;
  text: string;
  palette: (index: number) => string;
  onStyle: (style: PromptUnderlineStyle) => void;
  onColor: (color: PromptColorChoice) => void;
}) {
  const hex = colorHex(piece.underline_color, palette);
  return (
    <PieceRow label="Underline">
      <Segmented
        label="Underline"
        className="pc-seg pc-lines"
        options={UNDERLINE_KINDS.map((k) => ({
          value: k.style,
          label: (
            <span
              className="pc-line"
              style={{ textDecorationStyle: k.line, textDecorationColor: hex || undefined }}
            >
              {k.label}
            </span>
          ),
        }))}
        value={piece.underline_style}
        onChange={onStyle}
      />
      <ColorField
        className={cx('pc-custom', hex !== '' && 'is-on')}
        value={hex}
        width={112}
        placeholder="Text color"
        emptySwatch={text}
        aria-label="Underline color"
        pickerLabel="Choose the underline color"
        hexOnly
        allowEmpty
        onChange={(next) => {
          if (next === '') {
            onColor({ kind: 'default' });
            return;
          }
          const color = customColor(next);
          if (color) onColor(color);
        }}
      />
    </PieceRow>
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

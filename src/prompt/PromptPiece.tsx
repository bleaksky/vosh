import { useEffect, useState, type ReactNode } from 'react';
import type { BandEnv } from '../terminal/bandCells';
import { indexedRgb, toHex } from '../theme/color';
import {
  breakHint,
  byValueName,
  colorHex,
  colorHint,
  customColor,
  customText,
  MORE_STYLES,
  MORE_UNDERLINES,
  moreLabel,
  type MoreStylesPiece,
  rowsOf,
  swatchOf,
  THEME_HINT,
  THEME_SWATCHES,
  UNDERLINE_KINDS,
  WHEN_FIXED_HINT,
} from './promptPieces';
import type {
  PromptColorChoice,
  PromptEditOp,
  PromptFormatName,
  PromptPiece,
  PromptStyleChoice,
  PromptUnderlineStyle,
  PromptWhen,
} from '../ipc/promptDesign';
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
} from '../ui';
import { MenuSeparator } from '../ui/MenuSurface';
import { focusUnderPointer } from '../ui/menuAim';
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
// each take the terminal's own, By value on a value, a theme color or any
// true color. Style keeps B, I and U, and More styles holds
// strikethrough, dim, reverse and blink, then the underline kinds past the
// single line. While an underline is on, the Underline row picks its
// kind, each drawn in its own line, and its color, empty for the text
// color. A color by value that no swatch shows, such as one
// typed in Edit as text, names itself in the Custom field.

const WHEN_OPTIONS: SegmentedOption<PromptWhen>[] = [
  { value: 'always', label: 'Always' },
  { value: 'fight', label: 'In a fight' },
  { value: 'not_fight', label: 'Out of a fight' },
];

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
  const palette = (index: number) => toHex(indexedRgb(index, env.palette));
  const hint = colorHint(piece.color, piece.background);
  const paint = (layer: Layer) => (color: PromptColorChoice) =>
    onEdit({
      op: 'set_color',
      piece: at,
      color,
      ...(layer === 'background' ? { background: true } : {}),
      ...(layer === 'underline' ? { underline: true } : {}),
    });
  const setStyle = (style: PromptStyleChoice, on: boolean) =>
    onEdit({ op: 'set_style', piece: at, style, on });
  const breaks = piece.kind === 'nl';
  const whenHint = breaks ? breakHint(piece.when) : null;
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
      {whenHint && <p className="pc-piece-hint is-nowrap">{whenHint}</p>}
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
          <ColorRow
            label="Color"
            color={piece.color}
            byValue={piece.by_value}
            own={{ label: 'Terminal text', color: env.fg }}
            field={{ label: 'Custom color', picker: 'Choose a custom color' }}
            env={env}
            palette={palette}
            onColor={paint('text')}
          />
          {/* The hint stays right under Color, as P5 draws it. By value's
              rule holds for the ground too, so it also shows for one. */}
          <p className={cx('pc-piece-hint', hint === THEME_HINT && 'is-nowrap')}>{hint}</p>
          {rows.background && (
            <ColorRow
              label="Background"
              color={piece.background}
              byValue={piece.by_value}
              own={{ label: 'Terminal background', color: env.bg }}
              field={{ label: 'Custom background', picker: 'Choose a custom background' }}
              env={env}
              palette={palette}
              onColor={paint('background')}
            />
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
          env={env}
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

/** By value's green, yellow and red side by side, for the swatch of a
 *  color field too small to hold three of its own. */
function byValueGround(env: BandEnv): string {
  const [red, green, yellow] = [env.palette[1], env.palette[2], env.palette[3]];
  return `linear-gradient(90deg, ${green} 0 33.333%, ${yellow} 0 66.667%, ${red} 0)`;
}

/** By value's swatch: its green, yellow and red side by side. */
function ByValueSwatch({
  env,
  pressed,
  onClick,
}: {
  env: BandEnv;
  pressed: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className="pc-swatch is-by-value"
      aria-label="By value"
      title="By value"
      aria-pressed={pressed}
      onClick={onClick}
    >
      <span style={{ background: env.palette[2] }} />
      <span style={{ background: env.palette[3] }} />
      <span style={{ background: env.palette[1] }} />
    </button>
  );
}

/** A row of colors (P5, styles board): the terminal's own, By value on a
 *  value, the theme's seven, then Custom for any true color. The Color
 *  and Background rows are each one, so their swatches line up. A color
 *  by value no swatch shows, such as by another value, names itself in
 *  Custom. */
function ColorRow({
  label,
  color,
  byValue,
  own,
  field,
  env,
  palette,
  onColor,
}: {
  label: string;
  color: PromptColorChoice;
  byValue: boolean;
  own: { label: string; color: string };
  field: { label: string; picker: string };
  env: BandEnv;
  palette: (index: number) => string;
  onColor: (color: PromptColorChoice) => void;
}) {
  const swatch = swatchOf(color);
  const named = swatch === 'custom' ? byValueName(color) : null;
  return (
    <PieceRow label={label}>
      <span className="pc-swatches">
        <button
          type="button"
          className="pc-swatch"
          aria-label={own.label}
          title={own.label}
          aria-pressed={swatch === 'default'}
          style={{ background: own.color }}
          onClick={() => onColor({ kind: 'default' })}
        />
        {byValue && (
          <ByValueSwatch
            env={env}
            pressed={swatch === 'by_value'}
            onClick={() => onColor({ kind: 'by_value' })}
          />
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
            onClick={() => onColor({ kind: 'named', index: s.index })}
          />
        ))}
      </span>
      <ColorField
        className={cx('pc-custom', swatch === 'custom' && 'is-on')}
        value={customText(color, palette)}
        width={112}
        placeholder={named ?? 'Custom'}
        {...(named ? { emptySwatch: byValueGround(env) } : {})}
        aria-label={field.label}
        pickerLabel={field.picker}
        hexOnly
        allowEmpty
        onChange={(hex) => {
          const next = customColor(hex);
          if (next) onColor(next);
        }}
      />
    </PieceRow>
  );
}

/** The color a part's text draws in, as #rrggbb: the terminal's text for
 *  its own color, and the full color of By value. */
function textHex(color: PromptColorChoice, env: BandEnv, palette: (index: number) => string) {
  if (color.kind === 'default') return env.fg;
  if (color.kind === 'by_value') return env.palette[2];
  return colorHex(color, palette) || env.fg;
}

/** More styles opens above its button. Below, it covers the Underline
 *  row, so the underline kind you picked hides while you choose. In a
 *  window too short for that, it opens below (menuPosition). */
export const MORE_STYLES_PLACE = 'above-start' as const;

/** More styles' height in px, as prompt.css draws it: a row of 30 for
 *  each style and underline kind, the rule of 13 between them, and the
 *  menu's padding of 6 above and below. The placement tests read it. */
export const MORE_STYLES_HEIGHT = (MORE_STYLES.length + MORE_UNDERLINES.length) * 30 + 13 + 12;

/** The Style row's More styles button and its menu: strikethrough, dim,
 *  reverse and blink, then the underline kinds, each drawn in its own
 *  look with a check while it is on. The button reads the ones that are
 *  on, in the pressed look of B, I and U, so you see them without
 *  opening it. */
function MoreStyles({
  piece,
  onStyle,
}: {
  piece: PromptPiece;
  onStyle: (style: PromptStyleChoice, on: boolean) => void;
}) {
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  const label = moreLabel(piece);
  const on = label !== 'More styles';
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
          place={MORE_STYLES_PLACE}
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

/** The items of More styles, a check before each one that is on: the
 *  styles, a rule, and the underline kinds. Each item turns its style on
 *  while it is off and off while it is on, so an underline kind turns
 *  the underline on in that kind, and the kind that is on turns it off.
 *  The styles are checkboxes, and the kinds are radios, since one kind
 *  is on at a time. Blink draws its shown half and never blinks here. */
export function MoreStyleItems({
  piece,
  onToggle,
}: {
  piece: MoreStylesPiece;
  onToggle: (style: PromptStyleChoice, on: boolean) => void;
}) {
  const item = (
    style: PromptStyleChoice,
    label: string,
    checked: boolean,
    role: 'menuitemcheckbox' | 'menuitemradio',
  ) => (
    <li key={style} role="none">
      <button
        type="button"
        role={role}
        aria-checked={checked}
        className="menu-item pc-style-item"
        onPointerMove={focusUnderPointer}
        onClick={() => onToggle(style, !checked)}
      >
        {checked && <CheckIcon className="pc-start-check" />}
        <span className={`pc-style-sample is-${style}`}>{label}</span>
      </button>
    </li>
  );
  return (
    <>
      {MORE_STYLES.map((s) => item(s.style, s.label, piece[s.style], 'menuitemcheckbox'))}
      <MenuSeparator />
      {MORE_UNDERLINES.map((k) =>
        item(k.style, k.label, piece.underline_style === k.style, 'menuitemradio'),
      )}
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
  env,
  palette,
  onStyle,
  onColor,
}: {
  piece: PromptPiece;
  text: string;
  env: BandEnv;
  palette: (index: number) => string;
  onStyle: (style: PromptUnderlineStyle) => void;
  onColor: (color: PromptColorChoice) => void;
}) {
  const hex = colorHex(piece.underline_color, palette);
  // A color by value has no hex, so the field names it, and the kinds
  // draw in its full color, as the text's own By value does.
  const named = byValueName(piece.underline_color);
  const line = named ? textHex(piece.underline_color, env, palette) : hex;
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
              style={{ textDecorationStyle: k.line, textDecorationColor: line || undefined }}
            >
              {k.label}
            </span>
          ),
        }))}
        value={piece.underline_style}
        onChange={onStyle}
      />
      <ColorField
        className={cx('pc-custom', (hex !== '' || named !== null) && 'is-on')}
        value={hex}
        width={112}
        placeholder={named ?? 'Text color'}
        emptySwatch={named ? byValueGround(env) : text}
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

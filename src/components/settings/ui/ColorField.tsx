import { useRef, useState } from 'react';
import { colorInputValue, readColorText, rgbStringToHex } from '../../../lib/colorField';
import { cx } from './cx';
import { useRowIds } from './rowContext';

export interface ColorFieldProps {
  /** The saved color as CSS text, or an empty string for none. */
  value: string;
  /** Runs with each color the field reads, from the picker or the text.
   *  A hex color arrives as lowercase #rrggbb. Text that does not read
   *  as a color yet stays in the field, and leaving the field puts the
   *  saved color back. */
  onChange: (value: string) => void;
  /** Clearing the text runs onChange with an empty string, for a color
   *  that falls back to the theme. */
  allowEmpty?: boolean;
  /** Shown while the field is empty, like `Theme default`. */
  placeholder?: string;
  /** Width in px or any CSS length. 160 by default, the boards' color
   *  field. */
  width?: number | string;
  /** What the swatch shows while the field is empty, like the theme
   *  color the setting falls back to. Any CSS color, var() included. */
  emptySwatch?: string;
  /** The accessible name of the swatch's color picker, like `Choose
   *  the divider color`. */
  pickerLabel?: string;
  id?: string;
  'aria-label'?: string;
  'aria-describedby'?: string;
  className?: string;
}

function drawable(text: string): boolean {
  return typeof CSS !== 'undefined' && CSS.supports('color', text);
}

/** What text in the field saves: an empty string for none, the color
 *  to save, or null while it is not a color yet. Hex digits follow the
 *  rules in src/lib/colorField.ts, so typing #fffc41 does not save #fff
 *  on the way. A hex with alpha and any other CSS color save as typed
 *  once the page can draw them. */
function readFieldText(text: string, final: boolean): string | null {
  const trimmed = text.trim();
  const read = readColorText(trimmed, final);
  if (read.kind === 'default') return '';
  if (read.kind === 'color') return read.hex;
  if (/^#?[0-9a-f]+$/i.test(trimmed)) {
    const digits = trimmed.replace('#', '').length;
    const whole = digits === 8 || (final && digits === 4);
    return whole && trimmed.startsWith('#') && drawable(trimmed) ? trimmed.toLowerCase() : null;
  }
  return drawable(trimmed) ? trimmed : null;
}

/** A color field: a 16 px swatch that opens the system color picker,
 *  then the color as text in the UI font. 28 high, radius 8, the field
 *  fill. Inside a Row, the row label names the text. */
export function ColorField({
  value,
  onChange,
  allowEmpty = false,
  placeholder,
  width = 160,
  emptySwatch = 'transparent',
  pickerLabel = 'Choose a color',
  id,
  className,
  'aria-label': ariaLabel,
  'aria-describedby': describedBy,
}: ColorFieldProps) {
  const row = useRowIds();
  // The text while you type in the field, or null when it shows the
  // saved color.
  const [draft, setDraft] = useState<string | null>(null);
  const pickRef = useRef<HTMLInputElement | null>(null);
  const swatchRef = useRef<HTMLSpanElement | null>(null);
  const swatch = value !== '' && drawable(value) ? value : emptySwatch;

  const save = (text: string, final: boolean) => {
    const next = readFieldText(text, final);
    if (next === null || next === value) return;
    if (next === '' && !allowEmpty) return;
    onChange(next);
  };

  // The picker opens on the saved color, or on the color the swatch
  // draws, the theme color included.
  const primePicker = () => {
    const pick = pickRef.current;
    if (!pick) return;
    const shown = swatchRef.current
      ? rgbStringToHex(getComputedStyle(swatchRef.current).backgroundColor)
      : null;
    pick.value = colorInputValue(value === '' ? null : value) ?? shown ?? '#808080';
  };

  return (
    <span className={cx('st-color', className)} style={{ width }}>
      <span ref={swatchRef} className="st-color-swatch" style={{ background: swatch }}>
        <input
          ref={pickRef}
          type="color"
          className="st-color-picker"
          aria-label={pickerLabel}
          title={pickerLabel}
          onPointerDown={primePicker}
          onFocus={primePicker}
          onChange={(e) => {
            const hex = colorInputValue(e.target.value);
            if (hex && hex !== value) onChange(hex);
          }}
        />
      </span>
      <input
        type="text"
        className="st-color-text"
        id={id ?? row?.controlId}
        aria-label={ariaLabel}
        aria-describedby={describedBy ?? row?.descriptionId}
        spellCheck={false}
        autoComplete="off"
        placeholder={placeholder}
        value={draft ?? value}
        onFocus={() => setDraft(value)}
        onBlur={(e) => {
          save(e.currentTarget.value, true);
          setDraft(null);
        }}
        onChange={(e) => {
          setDraft(e.target.value);
          save(e.target.value, false);
        }}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            e.currentTarget.blur();
          } else if (e.key === 'Escape' && draft !== null && draft !== value) {
            // Escape puts the saved color back without leaving the
            // field or closing anything around it.
            e.preventDefault();
            e.stopPropagation();
            setDraft(value);
          }
        }}
      />
    </span>
  );
}

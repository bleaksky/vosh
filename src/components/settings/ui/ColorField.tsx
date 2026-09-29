import { useState } from 'react';
import { colorInputValue } from '../../../lib/appearanceSettings';
import { cx } from './cx';
import { useRowIds } from './rowContext';

export interface ColorFieldProps {
  /** The saved color as CSS text, or an empty string for none. */
  value: string;
  /** Runs with each color the page can draw, from the picker or the
   *  text. Text that does not read as a color yet stays in the field
   *  and leaving the field puts the saved color back. */
  onChange: (value: string) => void;
  /** Clearing the text runs onChange with an empty string, for a color
   *  that falls back to the theme. */
  allowEmpty?: boolean;
  placeholder?: string;
  /** Width in px or any CSS length. 120 by default. */
  width?: number | string;
  /** What the swatch shows while the field is empty, like the theme
   *  color the setting falls back to. Any CSS color, var() included. */
  emptySwatch?: string;
  /** The accessible name of the swatch's color picker, like `Choose
   *  the divider color`. */
  pickerLabel?: string;
  id?: string;
  'aria-label'?: string;
  className?: string;
}

function drawable(text: string): boolean {
  return typeof CSS !== 'undefined' && CSS.supports('color', text);
}

/** A color field: a 16 px swatch that opens the system color picker,
 *  then the color as text. 28 high, radius 8, the field fill. Inside a
 *  Row, the row label names the text. */
export function ColorField({
  value,
  onChange,
  allowEmpty = false,
  placeholder,
  width = 120,
  emptySwatch = 'transparent',
  pickerLabel = 'Choose a color',
  id,
  className,
  'aria-label': ariaLabel,
}: ColorFieldProps) {
  const row = useRowIds();
  // The text while you type in the field, or null when it shows the
  // saved color.
  const [draft, setDraft] = useState<string | null>(null);
  const swatch = value !== '' && drawable(value) ? value : emptySwatch;
  return (
    <span className={cx('st-color', className)} style={{ width }}>
      <span className="st-color-swatch" style={{ background: swatch }}>
        <input
          type="color"
          className="st-color-picker"
          aria-label={pickerLabel}
          value={colorInputValue(value)}
          onChange={(e) => onChange(e.target.value)}
        />
      </span>
      <input
        type="text"
        className="st-color-text"
        id={id ?? row?.controlId}
        aria-label={ariaLabel}
        aria-describedby={row?.descriptionId}
        spellCheck={false}
        autoComplete="off"
        placeholder={placeholder}
        value={draft ?? value}
        onFocus={() => setDraft(value)}
        onBlur={() => setDraft(null)}
        onChange={(e) => {
          const text = e.target.value;
          setDraft(text);
          const trimmed = text.trim();
          if ((trimmed === '' && allowEmpty) || (trimmed !== '' && drawable(trimmed))) {
            onChange(trimmed);
          }
        }}
        onKeyDown={(e) => {
          if (e.key === 'Enter') e.currentTarget.blur();
        }}
      />
    </span>
  );
}

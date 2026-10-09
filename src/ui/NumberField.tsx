import { useEffect, useId, useRef, useState, type InputHTMLAttributes } from 'react';
import { clampWhole, readNumberText } from './numberText';
import { cx } from './cx';
import { useRowIds } from './rowContext';

export interface NumberFieldProps extends Omit<
  InputHTMLAttributes<HTMLInputElement>,
  'value' | 'onChange' | 'width' | 'size' | 'min' | 'max' | 'step' | 'type'
> {
  value: number;
  /** Runs with a whole number inside [min, max] when you press Enter,
   *  leave the field, or step with the arrow keys. */
  onChange: (value: number) => void;
  min: number;
  max: number;
  /** The unit in the tertiary color inside the field's right edge,
   *  like `ms` or `pt`. */
  unit?: string;
  /** The unit as a screen reader should say it, like `milliseconds`.
   *  The field is described by it. The unit itself when left out. */
  unitName?: string;
  /** Width in px. 88 by default. */
  width?: number;
  /** Up and Down step by this much, Shift with them by ten times it. */
  step?: number;
}

/** A number field: the field recipe at 88 wide with the unit 10 px in
 *  from the right edge. What you type stays a draft until Enter or
 *  leaving the field saves it, clamped to the bounds. Escape puts the
 *  saved value back. */
export function NumberField({
  value,
  onChange,
  min,
  max,
  unit,
  unitName,
  width = 88,
  step = 1,
  id,
  className,
  ...rest
}: NumberFieldProps) {
  const row = useRowIds();
  const [draft, setDraft] = useState(String(value));
  const typing = useRef(false);
  const unitId = useId();
  const describedBy =
    [rest['aria-describedby'] ?? row?.descriptionId, unit !== undefined ? unitId : undefined]
      .filter(Boolean)
      .join(' ') || undefined;

  // Follow a value that changes elsewhere, like the panel edge dragged
  // in the main window, while you are not typing.
  useEffect(() => {
    if (!typing.current) setDraft(String(value));
  }, [value]);

  const commit = (text: string) => {
    const n = readNumberText(text, min, max);
    if (n === null) {
      setDraft(String(value));
      return;
    }
    setDraft(String(n));
    if (n !== value) onChange(n);
  };

  return (
    <span className={cx('st-number', className)} style={{ width }}>
      <input
        spellCheck={false}
        autoComplete="off"
        inputMode="numeric"
        {...rest}
        type="text"
        id={id ?? row?.controlId}
        className="st-field st-number-input"
        style={unit ? { paddingRight: `calc(20px + ${unit.length}ch)` } : undefined}
        value={draft}
        aria-describedby={describedBy}
        onFocus={(e) => {
          typing.current = true;
          rest.onFocus?.(e);
        }}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={(e) => {
          typing.current = false;
          commit(draft);
          rest.onBlur?.(e);
        }}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            commit(draft);
          } else if (e.key === 'Escape' && draft !== String(value)) {
            e.preventDefault();
            e.stopPropagation();
            setDraft(String(value));
          } else if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
            e.preventDefault();
            const from = readNumberText(draft, min, max) ?? value;
            const delta = (e.key === 'ArrowUp' ? step : -step) * (e.shiftKey ? 10 : 1);
            commit(String(clampWhole(from + delta, min, max)));
          }
          rest.onKeyDown?.(e);
        }}
      />
      {unit !== undefined && (
        <>
          <span className="st-number-unit" aria-hidden="true">
            {unit}
          </span>
          <span id={unitId} hidden>
            {unitName ?? unit}
          </span>
        </>
      )}
    </span>
  );
}

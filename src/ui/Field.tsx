import { forwardRef, type InputHTMLAttributes, type ReactNode } from 'react';
import { cx } from './cx';
import { useRowIds } from './rowContext';

export interface FieldProps extends Omit<
  InputHTMLAttributes<HTMLInputElement>,
  'value' | 'onChange' | 'width' | 'size'
> {
  value: string;
  onChange: (value: string) => void;
  /** Width in px or any CSS length. 240 by default, the width of
   *  most Settings fields. */
  width?: number | string;
  /** Monospace for MUD text only: patterns, sent commands, macro keys,
   *  host and port. Everything else stays in the UI font. */
  mono?: boolean;
  /** A leading 16 px icon 10 px from the left edge, with the text
   *  starting at 32. The Automation filter field uses the search icon. */
  icon?: ReactNode;
  /** Text Vosh will not take, like a profile name you already have:
   *  a danger ring inside the fill, and aria-invalid. */
  invalid?: boolean;
}

/** A settings text field: 28 high, radius 8, white 6% fill (white with
 *  a hairline ring on light themes), padding 0 10, 13 px text. */
export const Field = forwardRef<HTMLInputElement, FieldProps>(function Field(
  {
    value,
    onChange,
    width = 240,
    mono = false,
    icon,
    invalid = false,
    id,
    className,
    type = 'text',
    ...rest
  },
  ref,
) {
  const row = useRowIds();
  const input = (
    <input
      spellCheck={false}
      autoComplete="off"
      {...rest}
      ref={ref}
      type={type}
      id={id ?? row?.controlId}
      className={cx(
        'st-field',
        mono && 'st-field-mono',
        icon !== undefined && 'st-field-iconed',
        icon === undefined && className,
      )}
      value={value}
      aria-invalid={invalid || undefined}
      aria-describedby={rest['aria-describedby'] ?? row?.descriptionId}
      onChange={(e) => onChange(e.target.value)}
      style={icon === undefined ? { width } : undefined}
    />
  );
  if (icon === undefined) return input;
  return (
    <span className={cx('st-field-wrap', className)} style={{ width }}>
      <span className="st-field-icon" aria-hidden="true">
        {icon}
      </span>
      {input}
    </span>
  );
});

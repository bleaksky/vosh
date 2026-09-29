import type { SelectHTMLAttributes } from 'react';
import { cx } from './cx';
import { ChevronDownIcon } from './icons';
import { useRowIds } from './rowContext';

export interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

export interface SelectProps extends Omit<
  SelectHTMLAttributes<HTMLSelectElement>,
  'value' | 'onChange' | 'children'
> {
  value: string;
  onChange: (value: string) => void;
  options: readonly SelectOption[];
  /** Width in px. The boards use 160 (Appearance) and 240 (Automation,
   *  Characters). */
  width?: number;
}

/** A native select drawn as a settings field: 28 high, radius 8, white
 *  6% fill (white with a hairline ring on light themes), padding 0 28
 *  0 10, 13 px text, and a 12 px chevron in the tertiary color 10 px
 *  from the right edge. */
export function Select({
  value,
  onChange,
  options,
  width = 160,
  id,
  className,
  ...rest
}: SelectProps) {
  const row = useRowIds();
  return (
    <span className={cx('st-select', className)} style={{ width }}>
      <select
        {...rest}
        id={id ?? row?.controlId}
        className="st-select-input"
        value={value}
        aria-describedby={rest['aria-describedby'] ?? row?.descriptionId}
        onChange={(e) => onChange(e.target.value)}
      >
        {options.map((option) => (
          <option key={option.value} value={option.value} disabled={option.disabled}>
            {option.label}
          </option>
        ))}
      </select>
      <ChevronDownIcon size={12} className="st-select-chevron" />
    </span>
  );
}

import type { SelectHTMLAttributes } from 'react';
import { cx } from './cx';
import { ChevronDownIcon } from './icons';
import { useRowIds } from './rowContext';

export interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
  /** The heading this option sits under. Options that share one sit
   *  together under it, after the options with none. */
  group?: string;
}

export interface SelectProps extends Omit<
  SelectHTMLAttributes<HTMLSelectElement>,
  'value' | 'onChange' | 'children'
> {
  value: string;
  onChange: (value: string) => void;
  options: readonly SelectOption[];
  /** Width in px or any CSS length. The boards use 160 (Appearance)
   *  and 240 (Automation, Characters). */
  width?: number | string;
  /** A 16 px swatch at the start of the field in this CSS color, for a
   *  select of colors. */
  swatch?: string;
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
  swatch,
  id,
  className,
  ...rest
}: SelectProps) {
  const row = useRowIds();
  return (
    <span className={cx('st-select', swatch && 'has-swatch', className)} style={{ width }}>
      {swatch && (
        <span className="st-select-swatch" style={{ background: swatch }} aria-hidden="true" />
      )}
      <select
        {...rest}
        id={id ?? row?.controlId}
        className="st-select-input"
        value={value}
        aria-describedby={rest['aria-describedby'] ?? row?.descriptionId}
        onChange={(e) => onChange(e.target.value)}
      >
        {options.filter((option) => !option.group).map(renderOption)}
        {groupsOf(options).map(([group, grouped]) => (
          <optgroup key={group} label={group}>
            {grouped.map(renderOption)}
          </optgroup>
        ))}
      </select>
      <ChevronDownIcon size={12} className="st-select-chevron" />
    </span>
  );
}

function renderOption(option: SelectOption) {
  return (
    <option key={option.value} value={option.value} disabled={option.disabled}>
      {option.label}
    </option>
  );
}

/** The options under each heading, in the order the headings come. */
function groupsOf(options: readonly SelectOption[]): [string, SelectOption[]][] {
  const groups = new Map<string, SelectOption[]>();
  for (const option of options) {
    if (!option.group) continue;
    const list = groups.get(option.group) ?? [];
    list.push(option);
    groups.set(option.group, list);
  }
  return [...groups];
}

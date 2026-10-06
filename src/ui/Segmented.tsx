import type { ReactNode } from 'react';
import { cx } from './cx';
import { useRowIds } from './rowContext';

export interface SegmentedOption<T extends string> {
  value: T;
  label: ReactNode;
  disabled?: boolean;
  /** The segment's name when its label is a picture, like the Input
   *  caret shapes. It is also the tooltip. */
  name?: string;
}

export interface SegmentedProps<T extends string> {
  options: readonly SegmentedOption<T>[];
  /** The pressed segment, or null when none is (Automation while its
   *  import view shows). */
  value: T | null;
  onChange: (value: T) => void;
  /** The group's accessible name when it does not sit in a Row, like
   *  the Automation kind switcher's hidden `Kind`. Inside a Row the row
   *  label names it. */
  label?: string;
  /** The id of a visible label outside a Row that names the group, like
   *  the theme gallery's Vision switch. */
  labelledBy?: string;
  className?: string;
}

/** A segmented control: a 28 px track with 2 px padding and radius 8,
 *  segments 24 high at radius 6 with 12/500 text. The pressed segment
 *  fills with the selected row color in the text color and carries
 *  aria-pressed. The others read in the secondary color. */
export function Segmented<T extends string>({
  options,
  value,
  onChange,
  label,
  labelledBy,
  className,
}: SegmentedProps<T>) {
  const row = useRowIds();
  return (
    <div
      role="group"
      aria-label={label}
      aria-labelledby={labelledBy ?? (label === undefined ? row?.labelId : undefined)}
      aria-describedby={row?.descriptionId}
      className={cx('st-seg', className)}
    >
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          className="st-seg-item"
          aria-pressed={option.value === value}
          aria-label={option.name}
          title={option.name}
          disabled={option.disabled}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

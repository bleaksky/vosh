import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from 'react';
import { cx } from './cx';
import { CloseIcon } from './icons';

export interface ChipProps {
  children: ReactNode;
  /** Draws the trailing close button and runs on press. */
  onRemove?: () => void;
  /** The close button's accessible name, like `Stop tracking Haste`. */
  removeLabel?: string;
  /** `li` when the chips sit in a list, as the tracked affects do. */
  as?: 'li' | 'span';
  className?: string;
}

/** A chip: 24 high, radius 999, a 1 px hairline ring, 12 px text,
 *  padding 0 6 0 10 with a gap of 4, ending in a 24×24 close button
 *  that holds a 12 px close icon in the tertiary color. Set chips in a
 *  wrapping row 8 px apart. */
export function Chip({ children, onRemove, removeLabel, as: Tag = 'span', className }: ChipProps) {
  return (
    <Tag className={cx('st-chip', onRemove !== undefined && 'st-chip-removable', className)}>
      <span className="st-chip-label">{children}</span>
      {onRemove !== undefined && (
        <button
          type="button"
          className="st-chip-close"
          aria-label={removeLabel ?? 'Remove'}
          onClick={onRemove}
        >
          <CloseIcon size={12} />
        </button>
      )}
    </Tag>
  );
}

export interface ChipButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** A leading 12 px icon in the tertiary color. */
  icon?: ReactNode;
}

/** A chip shaped button for the end of a chip row, like `Add affect…`:
 *  same height, radius, and ring, padding 0 10 0 7, text in the
 *  secondary color. */
export const ChipButton = forwardRef<HTMLButtonElement, ChipButtonProps>(function ChipButton(
  { icon, className, children, ...rest },
  ref,
) {
  return (
    <button
      type="button"
      {...rest}
      ref={ref}
      className={cx('st-chip', 'st-chip-button', className)}
    >
      {icon !== undefined && (
        <span className="st-chip-icon" aria-hidden="true">
          {icon}
        </span>
      )}
      {children}
    </button>
  );
});

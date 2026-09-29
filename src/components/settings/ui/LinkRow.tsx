import type { ButtonHTMLAttributes, ReactNode } from 'react';
import { cx } from './cx';
import { ChevronRightIcon } from './icons';

export interface LinkRowProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children'> {
  /** The row label at 12/16. */
  label: ReactNode;
  /** Optional line under the label at 11/15 in the secondary color. */
  description?: ReactNode;
  /** Search and deep link anchor, like Row's. */
  anchor?: string;
}

/** A row that goes somewhere else in Settings, like Layout's `Panes and
 *  tracked affects` row that opens Characters. It is the Disclosure
 *  recipe, the label and description on the left and a 16 px chevron
 *  right in the tertiary color, as a button that runs `onClick`. As the
 *  last child of a Card it takes the card's bottom corners. */
export function LinkRow({ label, description, anchor, className, ...rest }: LinkRowProps) {
  return (
    <button
      type="button"
      {...rest}
      className={cx('st-disclosure', className)}
      data-st-anchor={anchor}
      data-st-flash={anchor ? '' : undefined}
    >
      <span className="st-row-text">
        <span className="st-row-label">{label}</span>
        {description !== undefined && <span className="st-row-desc">{description}</span>}
      </span>
      <ChevronRightIcon className="st-disclosure-chevron" />
    </button>
  );
}

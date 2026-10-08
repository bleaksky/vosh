import type { ButtonHTMLAttributes, HTMLAttributes, ReactNode } from 'react';
import { cx } from './cx';
import { ChevronRightIcon } from './icons';

export interface DisclosureProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children'> {
  /** The row label at 12/16. */
  label: ReactNode;
  /** Optional line under the label at 11/15 in the secondary color. */
  description?: ReactNode;
  /** Whether the content it controls shows. Render that content after
   *  the Disclosure and point aria-controls at it. */
  expanded: boolean;
  /** Search and deep link anchor, like Row's. */
  anchor?: string;
  /** More under the description, like the count of edits a preset
   *  trigger's Advanced holds. */
  note?: ReactNode;
}

/** A row sized button that opens more settings, like the boards'
 *  `Advanced` row: the label and description on the left and a 16 px
 *  chevron right in the tertiary color that turns down while open. As
 *  the last child of a Card it takes the card's bottom corners. */
export function Disclosure({
  label,
  description,
  expanded,
  anchor,
  note,
  className,
  ...rest
}: DisclosureProps) {
  return (
    <button
      type="button"
      {...rest}
      className={cx('st-disclosure', className)}
      aria-expanded={expanded}
      data-st-anchor={anchor}
      data-st-flash={anchor ? '' : undefined}
    >
      <span className="st-row-text">
        <span className="st-row-label">{label}</span>
        {description !== undefined && <span className="st-row-desc">{description}</span>}
        {note}
      </span>
      <ChevronRightIcon className="st-disclosure-chevron" />
    </button>
  );
}

/** The rows a Disclosure opens, right after it in the same Card. Give
 *  it the id the Disclosure's aria-controls names. Each row or block
 *  inside draws the hairline above it, and the last one takes the
 *  card's bottom corners. */
export function DisclosurePanel({ className, ...rest }: HTMLAttributes<HTMLDivElement>) {
  return <div {...rest} className={cx('st-disclosure-panel', className)} />;
}

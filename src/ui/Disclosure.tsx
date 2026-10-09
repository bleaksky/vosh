import { useId, type ButtonHTMLAttributes, type HTMLAttributes, type ReactNode } from 'react';
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

/** A row sized button that opens more settings, like the
 *  `Advanced` rows: the label and description on the left and a 16 px
 *  chevron right in the tertiary color that turns down while open. As
 *  the last child of a Card it takes the card's bottom corners. The
 *  label is its name, and the description and the note are read after
 *  it as the description. */
export function Disclosure({
  label,
  description,
  expanded,
  anchor,
  note,
  className,
  'aria-describedby': describedBy,
  ...rest
}: DisclosureProps) {
  const id = useId();
  const labelId = `${id}-label`;
  const descId = description !== undefined ? `${id}-desc` : undefined;
  const noteId = note !== undefined ? `${id}-note` : undefined;
  const describedByIds = [descId, noteId, describedBy].filter(Boolean).join(' ') || undefined;
  return (
    <button
      type="button"
      {...rest}
      className={cx('st-disclosure', className)}
      aria-expanded={expanded}
      aria-labelledby={labelId}
      aria-describedby={describedByIds}
      data-st-anchor={anchor}
      data-st-flash={anchor ? '' : undefined}
    >
      <span className="st-row-text">
        <span id={labelId} className="st-row-label">
          {label}
        </span>
        {description !== undefined && (
          <span id={descId} className="st-row-desc">
            {description}
          </span>
        )}
        {note !== undefined && (
          <span id={noteId} className="st-disclosure-note">
            {note}
          </span>
        )}
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

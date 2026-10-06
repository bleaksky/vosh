import { useId, useMemo, type ReactNode } from 'react';
import { cx } from './cx';
import { RowContext } from './rowContext';

export interface RowProps {
  /** The label at 12/16. It labels the first Toggle, Select, or Field
   *  inside the row. */
  label: ReactNode;
  /** Optional line under the label at 11/15 in the secondary color,
   *  4 px below. The row's control is described by it. */
  description?: ReactNode;
  /** `danger` sets the description in the danger tone, for a line that
   *  says why the control's value is refused. */
  descriptionTone?: 'danger';
  /** Search and deep link anchor. The frame scrolls the row into view
   *  and flashes it. Keep it in step with src/settings/settingsSearch.ts. */
  anchor?: string;
  /** The control, right aligned. */
  children?: ReactNode;
  className?: string;
}

/** One card row: min height 44, padding 10 16, the label and optional
 *  description on the left and the control on the right. Every row
 *  after the first in a card draws a 1 px hairline inset 16. */
export function Row({
  label,
  description,
  descriptionTone,
  anchor,
  children,
  className,
}: RowProps) {
  const controlId = useId();
  const labelId = useId();
  const descriptionId = useId();
  const hasDescription = description !== undefined && description !== null && description !== '';
  const ids = useMemo(
    () => ({
      controlId,
      labelId,
      descriptionId: hasDescription ? descriptionId : undefined,
    }),
    [controlId, labelId, descriptionId, hasDescription],
  );
  return (
    <div
      className={cx('st-row', className)}
      data-st-anchor={anchor}
      data-st-flash={anchor ? '' : undefined}
    >
      <div className="st-row-text">
        <label id={labelId} htmlFor={controlId} className="st-row-label">
          {label}
        </label>
        {hasDescription && (
          <span id={descriptionId} className="st-row-desc" data-tone={descriptionTone}>
            {description}
          </span>
        )}
      </div>
      {children !== undefined && (
        <div className="st-row-control">
          <RowContext.Provider value={ids}>{children}</RowContext.Provider>
        </div>
      )}
    </div>
  );
}

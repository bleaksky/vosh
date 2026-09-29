import type { HTMLAttributes } from 'react';
import { cx } from './cx';

export interface CardProps extends HTMLAttributes<HTMLDivElement> {
  /** 16 px of padding on every side, for a card that holds a block
   *  instead of rows (the Characters panel layout card). */
  padded?: boolean;
  /** Set the rows two by two, for short parallel rows like the General
   *  scope toggles. A 1 px line runs between the columns, inset 10 at
   *  the top and bottom, and only rows below the first pair draw the
   *  hairline. */
  columns?: boolean;
}

/** The settings card (otty's settings UI): radius 12 on the input band
 *  fill. Rows inside it draw their own hairlines. */
export function Card({ padded = false, columns = false, className, ...rest }: CardProps) {
  return (
    <div
      {...rest}
      className={cx('st-card', padded && 'st-card-padded', columns && 'st-card-columns', className)}
    />
  );
}

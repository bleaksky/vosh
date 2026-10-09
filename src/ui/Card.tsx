import type { HTMLAttributes } from 'react';
import { cx } from './cx';

export interface CardProps extends HTMLAttributes<HTMLDivElement> {
  /** Set the rows two by two, for short parallel rows like the General
   *  scope toggles. A 1 px line runs between the columns, inset 10 at
   *  the top and bottom, and only rows below the first pair draw the
   *  hairline. */
  columns?: boolean;
}

/** The settings card: radius 12 on the input band fill. Rows inside it
 *  draw their own hairlines. */
export function Card({ columns = false, className, ...rest }: CardProps) {
  return <div {...rest} className={cx('st-card', columns && 'st-card-columns', className)} />;
}

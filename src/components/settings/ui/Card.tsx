import type { HTMLAttributes } from 'react';
import { cx } from './cx';

export interface CardProps extends HTMLAttributes<HTMLDivElement> {
  /** 16 px of padding on every side, for a card that holds a block
   *  instead of rows (the Characters panel layout card). */
  padded?: boolean;
}

/** The settings card (otty's settings UI): radius 12 on the input band
 *  fill. Rows inside it draw their own hairlines. */
export function Card({ padded = false, className, ...rest }: CardProps) {
  return <div {...rest} className={cx('st-card', padded && 'st-card-padded', className)} />;
}

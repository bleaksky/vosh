import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from 'react';
import { cx } from './cx';

export interface IconButtonProps extends Omit<
  ButtonHTMLAttributes<HTMLButtonElement>,
  'aria-label' | 'children'
> {
  /** The accessible name, like `Erelei options` or `Move Haste up`.
   *  The button shows only its icon, so this is required. */
  label: string;
  /** A 16 px icon from icons.tsx. */
  icon: ReactNode;
}

/** A button that shows only an icon: 28×24, radius 8, no fill until
 *  hover, the icon in the secondary color. The frame's window controls
 *  draw the same button. Use it for a row's more button and for small
 *  actions beside a field. */
export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton(
  { label, icon, className, ...rest },
  ref,
) {
  return (
    <button
      type="button"
      {...rest}
      ref={ref}
      aria-label={label}
      className={cx('st-icon-button', className)}
    >
      {icon}
    </button>
  );
});

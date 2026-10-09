import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from 'react';
import { cx } from './cx';

export type ButtonVariant = 'secondary' | 'primary' | 'danger';

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** secondary (the default) has no fill and a 1 px hairline ring.
   *  primary fills with the accent and sets the text in --on-accent.
   *  danger keeps the ring and sets the text in the danger color. */
  variant?: ButtonVariant;
  /** 24 high at 12/500 with 10 side padding, like the prompt card foot. */
  small?: boolean;
  /** A leading 16 px icon in the secondary color, 6 px before the
   *  label, like `New profile` and `New trigger`. */
  icon?: ReactNode;
}

/** The one button: 28 high, radius 8, padding 0 12, 13/500. */
export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = 'secondary', small = false, icon, className, children, ...rest },
  ref,
) {
  return (
    <button
      type="button"
      {...rest}
      ref={ref}
      className={cx(
        'btn',
        variant !== 'secondary' && `is-${variant}`,
        small && 'is-small',
        icon !== undefined && 'has-icon',
        className,
      )}
    >
      {icon !== undefined && (
        <span className="btn-icon" aria-hidden="true">
          {icon}
        </span>
      )}
      {children}
    </button>
  );
});

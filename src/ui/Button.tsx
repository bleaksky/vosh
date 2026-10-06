import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from 'react';
import { cx } from './cx';

export type ButtonVariant = 'secondary' | 'primary' | 'danger';

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** secondary (the default) has no fill and a 1 px hairline ring.
   *  primary fills with the accent and sets the text in --on-accent.
   *  danger is text in the danger color with no fill. */
  variant?: ButtonVariant;
  /** A leading 16 px icon in the secondary color, 6 px before the
   *  label, like `New profile` and `New trigger`. */
  icon?: ReactNode;
}

/** A settings button: 28 high, radius 8, padding 0 12, 13/500. */
export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = 'secondary', icon, className, children, ...rest },
  ref,
) {
  return (
    <button
      type="button"
      {...rest}
      ref={ref}
      className={cx(
        'st-button',
        `st-button-${variant}`,
        icon !== undefined && 'st-button-iconed',
        className,
      )}
    >
      {icon !== undefined && (
        <span className="st-button-icon" aria-hidden="true">
          {icon}
        </span>
      )}
      {children}
    </button>
  );
});

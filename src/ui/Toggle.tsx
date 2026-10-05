import type { InputHTMLAttributes } from 'react';
import { cx } from './cx';
import { useRowIds } from './rowContext';

export interface ToggleProps extends Omit<
  InputHTMLAttributes<HTMLInputElement>,
  'type' | 'role' | 'checked' | 'onChange'
> {
  checked: boolean;
  onChange: (checked: boolean) => void;
}

/** The settings switch: a real checkbox with role switch over a 38×22
 *  track. The track is the accent when on and white 16% (black 14% on
 *  light themes) when off. The 18 px knob slides in 150 ms, or jumps
 *  under reduced motion. Inside a Row, the row label names it. Outside
 *  one, pass aria-label. */
export function Toggle({ checked, onChange, id, className, ...rest }: ToggleProps) {
  const row = useRowIds();
  return (
    <span className={cx('st-toggle', className)}>
      <input
        {...rest}
        id={id ?? row?.controlId}
        type="checkbox"
        role="switch"
        className="st-toggle-input"
        checked={checked}
        aria-describedby={rest['aria-describedby'] ?? row?.descriptionId}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span className="st-toggle-track" aria-hidden="true">
        <span className="st-toggle-knob" />
      </span>
    </span>
  );
}

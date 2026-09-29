import {
  forwardRef,
  useCallback,
  useLayoutEffect,
  useRef,
  type TextareaHTMLAttributes,
} from 'react';
import { cx } from './cx';
import { useRowIds } from './rowContext';

export interface FieldAreaProps extends Omit<
  TextareaHTMLAttributes<HTMLTextAreaElement>,
  'value' | 'onChange' | 'rows'
> {
  value: string;
  onChange: (value: string) => void;
  /** Width in px or any CSS length. 240 by default, like Field. */
  width?: number | string;
  /** Monospace for MUD text, like Field. */
  mono?: boolean;
}

/** A Field that holds more than one line. At one line it looks exactly
 *  like Field, 28 high, and it grows with each line you add. Use it
 *  where a newline means something, like a command list a trigger or
 *  timer sends, since a plain text field would drop the newlines. */
export const FieldArea = forwardRef<HTMLTextAreaElement, FieldAreaProps>(function FieldArea(
  { value, onChange, width = 240, mono = false, id, className, ...rest },
  forwarded,
) {
  const row = useRowIds();
  const own = useRef<HTMLTextAreaElement | null>(null);
  const setRef = useCallback(
    (el: HTMLTextAreaElement | null) => {
      own.current = el;
      if (typeof forwarded === 'function') forwarded(el);
      else if (forwarded) forwarded.current = el;
    },
    [forwarded],
  );

  // Fit the height to the text. Reset first so it can also shrink.
  useLayoutEffect(() => {
    const el = own.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = `${el.scrollHeight}px`;
  }, [value, width, mono]);

  return (
    <textarea
      spellCheck={false}
      autoComplete="off"
      {...rest}
      ref={setRef}
      rows={1}
      id={id ?? row?.controlId}
      className={cx('st-field', 'st-field-area', mono && 'st-field-mono', className)}
      value={value}
      aria-describedby={rest['aria-describedby'] ?? row?.descriptionId}
      onChange={(e) => onChange(e.target.value)}
      style={{ width }}
    />
  );
});

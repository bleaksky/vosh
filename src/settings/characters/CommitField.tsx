import { useEffect, useRef, useState } from 'react';
import { Field, type FieldProps } from '../../ui';

export interface CommitFieldProps extends Omit<FieldProps, 'value' | 'onChange'> {
  /** The saved value. The field follows it while you are not typing. */
  value: string;
  /** Runs with what you typed when you press Return or leave the
   *  field, if it differs from the saved value. */
  onCommit: (value: string) => void;
}

/** A Field that saves when you are done with it instead of on every
 *  key: Return or leaving the field saves, Esc puts the saved value
 *  back. */
export function CommitField({
  value,
  onCommit,
  onFocus,
  onBlur,
  onKeyDown,
  ...rest
}: CommitFieldProps) {
  const [draft, setDraft] = useState(value);
  const editing = useRef(false);

  useEffect(() => {
    if (!editing.current) setDraft(value);
  }, [value]);

  const commit = () => {
    if (draft !== value) onCommit(draft);
  };

  return (
    <Field
      {...rest}
      value={draft}
      onChange={setDraft}
      onFocus={(e) => {
        editing.current = true;
        onFocus?.(e);
      }}
      onBlur={(e) => {
        editing.current = false;
        commit();
        onBlur?.(e);
      }}
      onKeyDown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          commit();
        } else if (e.key === 'Escape' && draft !== value) {
          e.preventDefault();
          e.stopPropagation();
          setDraft(value);
        }
        onKeyDown?.(e);
      }}
    />
  );
}

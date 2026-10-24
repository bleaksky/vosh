import { useEffect, useRef, useState } from 'react';
import { useEscape } from '../../lib/escapeStack';
import { Field } from '../../ui';

interface GroupNameFieldProps {
  group: string;
  /** Resolves true when the field can close, false to keep it open
   *  after a name Vosh refused. */
  onCommit: (name: string) => Promise<boolean>;
  onCancel: () => void;
}

/** A group's new name, typed in place of its heading. Return or leaving
 *  the field renames it, as a rename in Finder does, and Esc backs out.
 *  A name Vosh refuses keeps the field open, with why at the top of the
 *  page. */
export function GroupNameField({ group, onCommit, onCancel }: GroupNameFieldProps) {
  const [value, setValue] = useState(group);
  const ref = useRef<HTMLInputElement | null>(null);
  // Closed for good, or waiting on the backend. Refs, so a blur right
  // after Return sees the rename already under way.
  const done = useRef(false);
  const saving = useRef(false);

  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);

  const cancel = () => {
    if (done.current || saving.current) return;
    done.current = true;
    onCancel();
  };

  useEscape(true, cancel);

  const commit = () => {
    if (done.current || saving.current) return;
    if (value.trim() === '' || value.trim() === group) {
      cancel();
      return;
    }
    saving.current = true;
    void onCommit(value.trim()).then((closed) => {
      saving.current = false;
      if (closed) done.current = true;
    });
  };

  return (
    <div className="st-auto-headrename">
      <Field
        ref={ref}
        value={value}
        onChange={setValue}
        width="100%"
        aria-label={`New name for ${group}`}
        placeholder="Group name"
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            commit();
          }
        }}
        onBlur={commit}
      />
    </div>
  );
}

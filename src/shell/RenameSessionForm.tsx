import { useEffect, useRef, useState, type FormEvent } from 'react';
import { sessionLabel, typedName } from '../lib/sessionLabel';
import { getSelected, rename, useSessions } from '../stores/session/sessionsStore';

// The Rename session form the session popover swaps in for its list
// while no sessions sidebar shows, as with one session. The sidebar
// names a session in a field in its row, and with no row to name it in, the
// popover's own form recipe holds that one field. Name starts on what
// the session reads, its text selected, and shows what it reads with no
// name once you clear it. Save keeps what you typed, and a blank Name
// clears the name, so the session reads its character again.

interface Props {
  onCancel: () => void;
  /** Close the popover. */
  onClose: () => void;
}

export function RenameSessionForm({ onCancel, onClose }: Props) {
  // The session selected as the form opened, which Save names even if
  // the selection moves meanwhile.
  const [session] = useState(getSelected);
  const rows = useSessions();
  const row = rows.find((r) => r.id === session);
  const [initial] = useState(() => (row ? sessionLabel(row, rows).name : ''));
  const [text, setText] = useState(initial);
  const ref = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const name = typedName(text, initial);
    if (name !== undefined) void rename(session, name);
    onClose();
  };

  return (
    <form className="shell-form" onSubmit={submit}>
      <h2 className="shell-form-title">Rename session</h2>
      <label className="shell-field">
        <span className="shell-field-label">Name</span>
        <input
          ref={ref}
          className="shell-textfield"
          type="text"
          value={text}
          placeholder={row ? sessionLabel({ ...row, name: null }, rows).name : undefined}
          spellCheck={false}
          autoComplete="off"
          autoCorrect="off"
          autoCapitalize="off"
          onChange={(e) => setText(e.target.value)}
        />
      </label>
      <div className="shell-form-actions">
        <button type="button" className="shell-btn" onClick={onCancel}>
          Cancel
        </button>
        <button type="submit" className="shell-btn shell-btn-primary">
          Save
        </button>
      </div>
    </form>
  );
}

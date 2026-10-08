import { useEffect, useRef, useState, type FormEvent, type ReactNode } from 'react';
import type { ConnectionTarget } from '../ipc/session';
import { parseTarget } from '../stores/session/useConnection';
import { Button } from '../ui';

// The host, port and TLS form the session popover swaps in for its list,
// for Edit connection… and New session…. Rows a form adds sit between
// the address and the buttons.

interface Props {
  title: string;
  submitLabel: string;
  initial: ConnectionTarget;
  /** Put the caret at the end of the port, which is what a builder
   *  changes. Otherwise the popover focuses the host. */
  focusPort?: boolean;
  /** Hears each edit, with the target it makes, or null while the host
   *  is blank or the port is not a TCP port. */
  onEdit?: (target: ConnectionTarget | null) => void;
  children?: ReactNode;
  onCancel: () => void;
  onSubmit: (target: ConnectionTarget) => void;
}

export function ConnectionForm({
  title,
  submitLabel,
  initial,
  focusPort = false,
  onEdit,
  children,
  onCancel,
  onSubmit,
}: Props) {
  const [host, setHost] = useState(initial.host);
  const [port, setPort] = useState(String(initial.port));
  const [tls, setTls] = useState(initial.tls);
  const portRef = useRef<HTMLInputElement | null>(null);
  const parsed = parseTarget({ host, port, tls });

  useEffect(() => {
    const field = portRef.current;
    if (!focusPort || !field) return;
    field.focus();
    field.setSelectionRange(field.value.length, field.value.length);
  }, [focusPort]);

  const edit = (next: { host: string; port: string; tls: boolean }) => {
    setHost(next.host);
    setPort(next.port);
    setTls(next.tls);
    onEdit?.(parseTarget(next));
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (parsed) onSubmit(parsed);
  };

  return (
    <form className="shell-form" onSubmit={submit}>
      <h2 className="shell-form-title">{title}</h2>
      <label className="shell-field">
        <span className="shell-field-label">Host</span>
        <input
          className="shell-textfield is-mono"
          type="text"
          value={host}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          autoComplete="off"
          placeholder="mud.example.org"
          onChange={(e) => edit({ host: e.target.value, port, tls })}
        />
      </label>
      <div className="shell-form-row">
        <label className="shell-field shell-field-port">
          <span className="shell-field-label">Port</span>
          <input
            ref={portRef}
            className="shell-textfield is-mono"
            type="text"
            inputMode="numeric"
            value={port}
            spellCheck={false}
            autoComplete="off"
            placeholder="4000"
            onChange={(e) => edit({ host, port: e.target.value.replace(/[^0-9]/g, ''), tls })}
          />
        </label>
        <label className="shell-toggle">
          <input
            className="shell-toggle-input"
            type="checkbox"
            checked={tls}
            onChange={(e) => edit({ host, port, tls: e.target.checked })}
          />
          <span className="shell-toggle-track" aria-hidden="true" />
          <span>Use TLS</span>
        </label>
      </div>
      {children}
      <div className="shell-form-actions">
        <Button onClick={onCancel}>Cancel</Button>
        <Button variant="primary" type="submit" disabled={!parsed}>
          {submitLabel}
        </Button>
      </div>
    </form>
  );
}

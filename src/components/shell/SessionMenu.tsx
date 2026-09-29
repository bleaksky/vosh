import { useState, type FormEvent } from 'react';
import { shortcutLabel } from '../../lib/palette';
import {
  parseTarget,
  worldName,
  type Connection,
  type ConnectionTarget,
} from '../../lib/useConnection';
import { ShellMenu, ShellMenuItem, ShellMenuSeparator } from './ShellMenu';

// The session popover under the title button (Session.dc.html): Connect
// to the saved world or Disconnect, Edit connection, and New
// connection. The two connection items swap the list for a host, port,
// and TLS form in the same popover. Disconnect is destructive, so it
// sits last in the danger tone and is never the row focus lands on.

const MENU_WIDTH = 272;

type Mode = 'menu' | 'edit' | 'new';

interface Props {
  connection: Connection;
  anchor: HTMLElement | null;
  onClose: () => void;
}

export function SessionMenu({ connection, anchor, onClose }: Props) {
  const [mode, setMode] = useState<Mode>('menu');
  const { live, target } = connection;

  const run = (action: () => Promise<void> | void) => {
    onClose();
    void action();
  };

  if (mode !== 'menu') {
    return (
      <ShellMenu
        anchor={anchor}
        align="center"
        width={MENU_WIDTH}
        label={mode === 'edit' ? 'Edit connection' : 'New connection'}
        kind="dialog"
        onClose={onClose}
      >
        <ConnectionForm
          mode={mode}
          initial={mode === 'edit' ? target : null}
          onCancel={() => setMode('menu')}
          onSubmit={(next) => {
            if (mode === 'edit') {
              connection.saveTarget(next);
              onClose();
            } else {
              run(() => connection.connectNew(next));
            }
          }}
        />
      </ShellMenu>
    );
  }

  return (
    <ShellMenu anchor={anchor} align="center" width={MENU_WIDTH} label="Session" onClose={onClose}>
      {!live && (
        <ShellMenuItem shortcut={shortcutLabel('Mod+R')} onSelect={() => run(connection.connect)}>
          Connect to {worldName(target.host)}
        </ShellMenuItem>
      )}
      <ShellMenuItem onSelect={() => setMode('edit')}>Edit connection…</ShellMenuItem>
      <ShellMenuSeparator />
      <ShellMenuItem onSelect={() => setMode('new')}>New connection…</ShellMenuItem>
      {live && (
        <>
          <ShellMenuSeparator />
          <ShellMenuItem danger onSelect={() => run(connection.disconnect)}>
            Disconnect
          </ShellMenuItem>
        </>
      )}
    </ShellMenu>
  );
}

interface FormProps {
  mode: 'edit' | 'new';
  /** Prefill for Edit. New starts blank. */
  initial: ConnectionTarget | null;
  onCancel: () => void;
  onSubmit: (target: ConnectionTarget) => void;
}

function ConnectionForm({ mode, initial, onCancel, onSubmit }: FormProps) {
  const [host, setHost] = useState(initial?.host ?? '');
  const [port, setPort] = useState(initial ? String(initial.port) : '');
  const [tls, setTls] = useState(initial?.tls ?? false);
  const parsed = parseTarget({ host, port, tls });

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (parsed) onSubmit(parsed);
  };

  return (
    <form className="shell-form" onSubmit={submit}>
      <h2 className="shell-form-title">{mode === 'edit' ? 'Edit connection' : 'New connection'}</h2>
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
          onChange={(e) => setHost(e.target.value)}
        />
      </label>
      <div className="shell-form-row">
        <label className="shell-field shell-field-port">
          <span className="shell-field-label">Port</span>
          <input
            className="shell-textfield is-mono"
            type="text"
            inputMode="numeric"
            value={port}
            spellCheck={false}
            autoComplete="off"
            placeholder="4000"
            onChange={(e) => setPort(e.target.value.replace(/[^0-9]/g, ''))}
          />
        </label>
        <label className="shell-toggle">
          <input
            className="shell-toggle-input"
            type="checkbox"
            checked={tls}
            onChange={(e) => setTls(e.target.checked)}
          />
          <span className="shell-toggle-track" aria-hidden="true" />
          <span>Use TLS</span>
        </label>
      </div>
      <div className="shell-form-actions">
        <button type="button" className="shell-btn" onClick={onCancel}>
          Cancel
        </button>
        <button type="submit" className="shell-btn shell-btn-primary" disabled={!parsed}>
          {mode === 'edit' ? 'Save' : 'Connect'}
        </button>
      </div>
    </form>
  );
}

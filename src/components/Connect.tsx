import { useState, type FormEvent } from 'react';
import { connectSession, disconnectSession } from '../lib/session';

interface Props {
  status: ConnectionStatus;
  onError: (message: string) => void;
  onToggleTriggers?: () => void;
  triggersOpen?: boolean;
  onToggleSidePanel?: () => void;
  sidePanelOpen?: boolean;
  onToggleSettings?: () => void;
  settingsOpen?: boolean;
}

export type ConnectionStatus =
  | { kind: 'idle' }
  | { kind: 'connecting'; host: string; port: number; tls: boolean }
  | { kind: 'connected'; host: string; port: number; tls: boolean }
  | { kind: 'error'; message: string };

const DEFAULT_HOST = 'play.theforsakenlands.com';
const DEFAULT_PORT = 1848;

export function Connect({
  status,
  onError,
  onToggleTriggers,
  triggersOpen,
  onToggleSidePanel,
  sidePanelOpen,
  onToggleSettings,
  settingsOpen,
}: Props) {
  const [host, setHost] = useState(DEFAULT_HOST);
  const [port, setPort] = useState(DEFAULT_PORT);
  const [tls, setTls] = useState(false);
  const isLive = status.kind === 'connecting' || status.kind === 'connected';

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      if (isLive) {
        await disconnectSession();
      } else {
        await connectSession(host, port, tls);
      }
    } catch (e) {
      onError(String(e));
    }
  };

  const renderStatus = () => {
    switch (status.kind) {
      case 'idle':
        return 'idle';
      case 'connecting':
        return `connecting to ${status.host}:${status.port}${status.tls ? ' (tls)' : ''}`;
      case 'connected':
        return `connected to ${status.host}:${status.port}${status.tls ? ' (tls)' : ''}`;
      case 'error':
        return `error ${status.message}`;
    }
  };

  return (
    <form className="connect" onSubmit={handleSubmit}>
      <input
        type="text"
        value={host}
        disabled={isLive}
        spellCheck={false}
        onChange={(e) => setHost(e.target.value)}
        aria-label="host"
      />
      <input
        type="number"
        value={port}
        disabled={isLive}
        min={1}
        max={65535}
        onChange={(e) => setPort(Number(e.target.value))}
        aria-label="port"
      />
      <label>
        <input
          type="checkbox"
          checked={tls}
          disabled={isLive}
          onChange={(e) => setTls(e.target.checked)}
        />
        tls
      </label>
      <button type="submit">{isLive ? 'disconnect' : 'connect'}</button>
      <span className="status">{renderStatus()}</span>
      {onToggleSidePanel && (
        <button
          type="button"
          className="secondary"
          aria-pressed={sidePanelOpen ?? false}
          onClick={onToggleSidePanel}
        >
          panes
        </button>
      )}
      {onToggleTriggers && (
        <button
          type="button"
          className="secondary"
          aria-pressed={triggersOpen ?? false}
          onClick={onToggleTriggers}
        >
          triggers
        </button>
      )}
      {onToggleSettings && (
        <button
          type="button"
          className="secondary"
          aria-pressed={settingsOpen ?? false}
          onClick={onToggleSettings}
        >
          profile
        </button>
      )}
    </form>
  );
}

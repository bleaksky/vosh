import { useState, type FormEvent } from 'react';
import { connectSession, disconnectSession } from '../lib/session';

interface Props {
  status: ConnectionStatus;
  onError: (message: string) => void;
}

export type ConnectionStatus =
  | { kind: 'idle' }
  | { kind: 'connecting'; host: string; port: number; tls: boolean }
  | { kind: 'connected'; host: string; port: number; tls: boolean }
  | { kind: 'error'; message: string };

const DEFAULT_HOST = 'play.theforsakenlands.com';
const DEFAULT_PORT = 1848;

export function Connect({ status, onError }: Props) {
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

  return (
    <form className={`connect${isLive ? ' is-live' : ''}`} onSubmit={handleSubmit}>
      <span className="connect-label">host</span>
      <input
        type="text"
        value={host}
        disabled={isLive}
        spellCheck={false}
        onChange={(e) => setHost(e.target.value)}
        aria-label="host"
      />
      <span className="connect-label">port</span>
      <input
        type="number"
        value={port}
        disabled={isLive}
        min={1}
        max={65535}
        onChange={(e) => setPort(Number(e.target.value))}
        aria-label="port"
      />
      <label className="connect-tls">
        <input
          type="checkbox"
          checked={tls}
          disabled={isLive}
          onChange={(e) => setTls(e.target.checked)}
        />
        tls
      </label>
      <button type="submit" className="connect-action">
        {isLive ? '[disconnect]' : '[connect]'}
      </button>
    </form>
  );
}

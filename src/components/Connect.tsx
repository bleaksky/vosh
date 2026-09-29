import { useEffect, useRef, useState, type FormEvent } from 'react';
import { disconnectSession } from '../lib/session';
import {
  connectTo,
  loadTarget,
  useCharacterName,
  type ConnectionStatus,
} from '../lib/useConnection';

export type { ConnectionStatus } from '../lib/useConnection';

interface Props {
  status: ConnectionStatus;
  onError: (message: string) => void;
}

// Session chip in the top bar. Replaces the old full-width connect
// row: a status dot plus the live host:port, with the host / port /
// tls form and the connect or disconnect action in a dropdown. The
// dropdown follows the loadouts menu pattern so the two read as one
// family of top bar controls. The connect path itself, with the
// profile auto-match, lives in lib/useConnection.
export function Connect({ status, onError }: Props) {
  const [host, setHost] = useState(() => loadTarget().host);
  const [port, setPort] = useState(() => loadTarget().port);
  const [tls, setTls] = useState(() => loadTarget().tls);
  const [open, setOpen] = useState(false);
  // Logged-in character, from Char.Status / Char.Name GMCP. Gives the
  // chip its identity segment; cleared on disconnect.
  const charName = useCharacterName();
  const rootRef = useRef<HTMLDivElement | null>(null);
  const isLive = status.kind === 'connecting' || status.kind === 'connected';

  useEffect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    document.addEventListener('pointerdown', onDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('pointerdown', onDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  // The palette's connect entry lands here so the chip stays the one
  // owner of host / port / auto-match behavior.
  useEffect(() => {
    const onRequest = () => {
      if (!isLive) void doConnect();
    };
    window.addEventListener('vosh:connect-request', onRequest);
    return () => window.removeEventListener('vosh:connect-request', onRequest);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isLive, host, port, tls]);

  const doConnect = async () => {
    try {
      await connectTo({ host, port, tls });
      setOpen(false);
    } catch (e) {
      onError(String(e));
    }
  };

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    if (isLive) {
      try {
        await disconnectSession();
        setOpen(false);
      } catch (e) {
        onError(String(e));
      }
      return;
    }
    await doConnect();
  };

  const dotKind =
    status.kind === 'connected'
      ? 'is-connected'
      : status.kind === 'connecting'
        ? 'is-connecting'
        : status.kind === 'error'
          ? 'is-error'
          : 'is-idle';
  const liveHostPort =
    status.kind === 'connected' || status.kind === 'connecting'
      ? `${status.host}:${status.port}`
      : `${host}:${port}`;

  return (
    <div className="session-chip-wrap" ref={rootRef}>
      <button
        type="button"
        className={`session-chip${isLive ? ' is-live' : ''}`}
        aria-expanded={open}
        aria-haspopup="true"
        onClick={() => setOpen((v) => !v)}
        title={status.kind === 'error' ? status.message : undefined}
      >
        <span className={`session-chip-dot ${dotKind}`} aria-hidden="true" />
        {isLive ? (
          <>
            {charName && <span className="session-chip-name">{charName.toLowerCase()}</span>}
            <span className="session-chip-host">{liveHostPort}</span>
          </>
        ) : (
          <span className="session-chip-label">connect</span>
        )}
      </button>
      {open && (
        <form className="session-menu" data-occludes-surface="true" onSubmit={handleSubmit}>
          <label className="session-menu-field">
            <span className="session-menu-label">host</span>
            <input
              type="text"
              value={host}
              disabled={isLive}
              spellCheck={false}
              onChange={(e) => setHost(e.target.value)}
              aria-label="host"
            />
          </label>
          <label className="session-menu-field">
            <span className="session-menu-label">port</span>
            <input
              type="number"
              value={port}
              disabled={isLive}
              min={1}
              max={65535}
              onChange={(e) => setPort(Number(e.target.value))}
              aria-label="port"
            />
          </label>
          <label className="session-menu-field session-menu-tls">
            <span className="session-menu-label">tls</span>
            <input
              type="checkbox"
              checked={tls}
              disabled={isLive}
              onChange={(e) => setTls(e.target.checked)}
            />
          </label>
          <button type="submit" className={`session-menu-action${isLive ? ' is-live' : ''}`}>
            {isLive ? 'disconnect' : 'connect'}
          </button>
        </form>
      )}
    </div>
  );
}

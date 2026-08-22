import { useEffect, useRef, useState, type FormEvent } from 'react';
import {
  connectSession,
  disconnectSession,
  onGmcpPackage,
  onState,
  profileResolveMatch,
  profileSwitch,
  profilesList,
} from '../lib/session';

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

// Session chip in the top bar. Replaces the old full-width connect
// row: a status dot plus the live host:port, with the host / port /
// tls form and the connect or disconnect action in a dropdown. The
// dropdown follows the loadouts menu pattern so the two read as one
// family of top bar controls.
export function Connect({ status, onError }: Props) {
  const [host, setHost] = useState(DEFAULT_HOST);
  const [port, setPort] = useState(DEFAULT_PORT);
  const [tls, setTls] = useState(false);
  const [open, setOpen] = useState(false);
  // Logged-in character, from Char.Status / Char.Name GMCP. Gives the
  // chip its identity segment; cleared on disconnect.
  const [charName, setCharName] = useState<string | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const isLive = status.kind === 'connecting' || status.kind === 'connected';

  useEffect(() => {
    let cancelled = false;
    const unsubs: (() => void)[] = [];
    const takeName = (data: { name?: unknown }) => {
      if (typeof data?.name === 'string' && data.name.trim().length > 0) {
        setCharName(data.name.trim());
      }
    };
    void onGmcpPackage('Char.Status', takeName).then((fn) => {
      if (cancelled) fn();
      else unsubs.push(fn);
    });
    void onGmcpPackage('Char.Name', takeName).then((fn) => {
      if (cancelled) fn();
      else unsubs.push(fn);
    });
    void onState((payload) => {
      if (payload.kind === 'disconnected') setCharName(null);
    }).then((fn) => {
      if (cancelled) fn();
      else unsubs.push(fn);
    });
    return () => {
      cancelled = true;
      for (const fn of unsubs) fn();
    };
  }, []);

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
    // Pre-login profile auto-match. Resolve by host (plus port if a
    // profile pins one) and switch ahead of the connection. Profiles
    // pinned to a specific character soft-skip here because the
    // character is unknown until the MUD sends Char.Status after
    // login; the session GMCP handler picks them up and swaps then.
    try {
      try {
        const matchName = await profileResolveMatch(host, port, null);
        if (matchName) {
          const current = await profilesList();
          if (matchName !== current.active) {
            await profileSwitch(matchName);
          }
        }
      } catch (matchErr) {
        // Profile resolve is best-effort — never block a connect on
        // a transient profile-system error.
        console.warn('[profile match]', matchErr);
      }
      await connectSession(host, port, tls);
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

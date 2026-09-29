import { useCallback, useEffect, useRef, useState } from 'react';
import {
  connectSession,
  disconnectSession,
  onGmcpPackage,
  onState,
  profileResolveMatch,
  profileSwitch,
  profilesList,
} from './session';

// The session the title band shows and the session menu drives. Moved
// out of the old top bar chip (Connect.tsx) so the connect logic lives
// in App for the life of the window instead of in whichever control
// happens to be mounted. The palette's connect entry and the Cmd+R
// shortcut reach it through the `vosh:connect-request` window event.

export type ConnectionStatus =
  | { kind: 'idle' }
  | { kind: 'connecting'; host: string; port: number; tls: boolean }
  | { kind: 'connected'; host: string; port: number; tls: boolean }
  | { kind: 'error'; message: string };

/** Where Connect dials. */
export interface ConnectionTarget {
  host: string;
  port: number;
  tls: boolean;
}

export const DEFAULT_TARGET: ConnectionTarget = {
  host: 'play.theforsakenlands.com',
  port: 1848,
  tls: false,
};

// The last target you saved or dialed, so Connect after a relaunch
// dials the world you last used instead of the stock one. This is a
// per-machine convenience in browser storage. The backend has no saved
// connection model yet.
const TARGET_KEY = 'vosh.connection.target';

/** A world known by name, and where you connect to play it. */
export interface KnownWorld {
  /** A host matches this domain or any subdomain of it. */
  domain: string;
  name: string;
  host: string;
  port: number;
}

/** Worlds known by name. Any other host shows as typed. Mirrors
 *  KNOWN_WORLDS in src-tauri/src/profile_set.rs. */
export const KNOWN_WORLDS: readonly KnownWorld[] = [
  {
    domain: 'theforsakenlands.com',
    name: 'The Forsaken Lands',
    host: 'play.theforsakenlands.com',
    port: 1848,
  },
];

/** The display name for a host, like `The Forsaken Lands` for
 *  `play.theforsakenlands.com`. Unknown hosts show as typed. */
export function worldName(host: string): string {
  const clean = host.trim().toLowerCase().replace(/\.$/, '');
  const known = KNOWN_WORLDS.find((w) => clean === w.domain || clean.endsWith(`.${w.domain}`));
  return known ? known.name : host.trim();
}

/** A target read back from storage or a form, or null when the host is
 *  blank or the port is not a TCP port. */
export function parseTarget(value: unknown): ConnectionTarget | null {
  if (!value || typeof value !== 'object') return null;
  const v = value as Record<string, unknown>;
  const host = typeof v.host === 'string' ? v.host.trim() : '';
  const port = typeof v.port === 'number' ? v.port : Number(v.port);
  if (host.length === 0 || !Number.isInteger(port) || port < 1 || port > 65535) return null;
  return { host, port, tls: v.tls === true };
}

export function loadTarget(): ConnectionTarget {
  try {
    const raw = localStorage.getItem(TARGET_KEY);
    return (raw && parseTarget(JSON.parse(raw))) || DEFAULT_TARGET;
  } catch {
    return DEFAULT_TARGET;
  }
}

function storeTarget(target: ConnectionTarget): void {
  try {
    localStorage.setItem(TARGET_KEY, JSON.stringify(target));
  } catch {
    // Storage unavailable. The target still holds for this session.
  }
}

/** Switch to the profile that matches the host, then connect. Profiles
 *  pinned to a character soft skip here because the character is
 *  unknown until the MUD sends Char.Status after login, and the
 *  session's GMCP handler swaps to them then. */
export async function connectTo(target: ConnectionTarget): Promise<void> {
  try {
    const matchName = await profileResolveMatch(target.host, target.port, null);
    if (matchName) {
      const current = await profilesList();
      if (matchName !== current.active) await profileSwitch(matchName);
    }
  } catch (matchErr) {
    // Profile resolve is best effort. A profile system error never
    // blocks a connect.
    console.warn('[profile match]', matchErr);
  }
  await connectSession(target.host, target.port, target.tls);
}

/** The logged in character from Char.Status or Char.Name, cleared when
 *  the session ends. */
export function useCharacterName(): string | null {
  const [name, setName] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    const unsubs: (() => void)[] = [];
    const keep = (p: Promise<() => void>) =>
      void p.then((fn) => {
        if (cancelled) fn();
        else unsubs.push(fn);
      });
    const take = (data: { name?: unknown }) => {
      if (typeof data?.name === 'string' && data.name.trim().length > 0) {
        setName(data.name.trim());
      }
    };
    keep(onGmcpPackage('Char.Status', take));
    keep(onGmcpPackage('Char.Name', take));
    keep(
      onState((payload) => {
        if (payload.kind === 'disconnected') setName(null);
      }),
    );
    return () => {
      cancelled = true;
      for (const fn of unsubs) fn();
    };
  }, []);
  return name;
}

export interface Connection {
  status: ConnectionStatus;
  /** Connecting or connected. */
  live: boolean;
  /** Where Connect dials next. */
  target: ConnectionTarget;
  /** The world the title names: the live host while a session runs,
   *  else the target. */
  world: string;
  character: string | null;
  /** Dial the target. */
  connect: () => Promise<void>;
  /** Save a new target and dial it, replacing a live session. */
  connectNew: (target: ConnectionTarget) => Promise<void>;
  disconnect: () => Promise<void>;
  /** Save the target for the next Connect without dialing. */
  saveTarget: (target: ConnectionTarget) => void;
}

/** Connection state and actions for the title band, the session menu,
 *  the palette, and Cmd+R. Mount it once, in App. */
export function useConnection(
  status: ConnectionStatus,
  onError: (message: string) => void,
): Connection {
  const [target, setTarget] = useState<ConnectionTarget>(loadTarget);
  const character = useCharacterName();
  const live = status.kind === 'connecting' || status.kind === 'connected';

  // The event listener and the shortcut read the newest values through
  // refs, so they never dial a stale target.
  const targetRef = useRef(target);
  const liveRef = useRef(live);
  const onErrorRef = useRef(onError);
  useEffect(() => {
    targetRef.current = target;
    liveRef.current = live;
    onErrorRef.current = onError;
  });

  const saveTarget = useCallback((next: ConnectionTarget) => {
    targetRef.current = next;
    setTarget(next);
    storeTarget(next);
  }, []);

  const dial = useCallback(async (to: ConnectionTarget) => {
    try {
      await connectTo(to);
    } catch (e) {
      onErrorRef.current(String(e));
    }
  }, []);

  const connect = useCallback(() => dial(targetRef.current), [dial]);

  const connectNew = useCallback(
    (next: ConnectionTarget) => {
      saveTarget(next);
      return dial(next);
    },
    [dial, saveTarget],
  );

  const disconnect = useCallback(async () => {
    try {
      await disconnectSession();
    } catch (e) {
      onErrorRef.current(String(e));
    }
  }, []);

  useEffect(() => {
    const onRequest = () => {
      if (!liveRef.current) void connect();
    };
    window.addEventListener('vosh:connect-request', onRequest);
    return () => window.removeEventListener('vosh:connect-request', onRequest);
  }, [connect]);

  const world = worldName(
    status.kind === 'connected' || status.kind === 'connecting' ? status.host : target.host,
  );

  return { status, live, target, world, character, connect, connectNew, disconnect, saveTarget };
}

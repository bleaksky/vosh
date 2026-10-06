import { useCallback, useEffect, useRef, useState } from 'react';
import { profileResolveMatch, profileSwitch, profilesList } from '../../ipc/profiles';
import {
  connectSession,
  disconnectSession,
  emitConnectionTargetChanged,
  onGmcpPackage,
  onState,
  subscribeConnectionTargetChanged,
  type ConnectionTarget,
} from '../../ipc/session';
import { useTauriEvent } from '../../ipc/useTauriEvent';
import { worldName } from '../../lib/knownWorlds';
import { errorText } from '../../lib/text';
import { pushToast } from '../toasts';

// The session the title band shows and the session menu drives, with
// the saved target Connect dials.
// MainWindow mounts the hook once, so the connection state lives as long
// as the window and not only while the session menu or another control
// that shows it is mounted. The palette's connect entry and the Cmd+R
// shortcut call it through MainWindow.

export type ConnectionStatus =
  | { kind: 'idle' }
  | { kind: 'connecting'; host: string; port: number; tls: boolean }
  | { kind: 'connected'; host: string; port: number; tls: boolean }
  | { kind: 'error'; message: string };

export const DEFAULT_TARGET: ConnectionTarget = {
  host: 'play.theforsakenlands.com',
  port: 1848,
  tls: false,
};

// The last target you saved or dialed, so Connect after a relaunch
// dials the world you last used instead of the stock one. This is a
// per-machine convenience in browser storage. The backend has no saved
// connection model yet. It is the one saved target: the session
// popover and Settings › General › Connection both edit it through
// saveConnectionTarget, and every window follows it through
// subscribeConnectionTarget.
const TARGET_KEY = 'vosh.connection.target';

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

/** Save where Connect and ⌘R dial, and tell every window. */
export function saveConnectionTarget(target: ConnectionTarget): void {
  storeTarget(target);
  emitConnectionTargetChanged(target).catch(() => {
    // No other window to tell. Storage still holds the target.
  });
}

/** Follow the saved target as any window saves it, this one included.
 *  Returns the unsubscribe. */
export function subscribeConnectionTarget(cb: (target: ConnectionTarget) => void): () => void {
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  subscribeConnectionTargetChanged((payload) => {
    const target = parseTarget(payload);
    if (target && !cancelled) cb(target);
  })
    .then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    })
    .catch(() => {
      // No event bridge. The target only changes from this window.
    });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/** The saved target and a setter that saves it for every window. */
export function useSavedTarget(): [ConnectionTarget, (target: ConnectionTarget) => void] {
  const [target, setTarget] = useState<ConnectionTarget>(loadTarget);
  useEffect(() => subscribeConnectionTarget(setTarget), []);
  const save = useCallback((next: ConnectionTarget) => {
    setTarget(next);
    saveConnectionTarget(next);
  }, []);
  return [target, save];
}

/** The sentence a profile switch that failed at connect shows you. The
 *  backend answers with a sentence that names the profile you are still
 *  using, and it passes through. */
export function profileSwitchErrorMessage(error: unknown): string {
  const text = errorText(error);
  return text || 'Vosh could not switch profiles, so you connect with the profile you were using.';
}

/** Switch to the profile that matches the host, then connect. Profiles
 *  pinned to a character soft skip here because the character is
 *  unknown until the MUD sends Char.Status after login, and the
 *  session's GMCP handler swaps to them then. A switch that fails
 *  leaves you on the profile you were using, says so in a toast, and
 *  connects under that profile. */
export async function connectTo(target: ConnectionTarget): Promise<void> {
  let switchTo: string | null = null;
  try {
    const matchName = await profileResolveMatch(target.host, target.port, null);
    if (matchName) {
      const current = await profilesList();
      if (matchName !== current.active) switchTo = matchName;
    }
  } catch (matchErr) {
    // Profile resolve is best effort. A profile system error never
    // blocks a connect.
    console.warn('[profile match]', matchErr);
  }
  if (switchTo) {
    try {
      await profileSwitch(switchTo);
    } catch (switchErr) {
      console.warn('[profile switch]', switchErr);
      pushToast({ kind: 'error', message: profileSwitchErrorMessage(switchErr) });
    }
  }
  await connectSession(target.host, target.port, target.tls);
}

/** The logged in character from Char.Status or Char.Name, cleared when
 *  the session ends. */
export function useCharacterName(): string | null {
  const [name, setName] = useState<string | null>(null);
  const take = (data: { name?: unknown }) => {
    if (typeof data?.name === 'string' && data.name.trim().length > 0) {
      setName(data.name.trim());
    }
  };
  useTauriEvent((cb) => onGmcpPackage('Char.Status', cb), take);
  useTauriEvent((cb) => onGmcpPackage('Char.Name', cb), take);
  useTauriEvent(onState, (payload) => {
    if (payload.kind === 'disconnected') setName(null);
  });
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
 *  the palette, and Cmd+R. Mount it once, in MainWindow. */
export function useConnection(
  status: ConnectionStatus,
  onError: (message: string) => void,
): Connection {
  const [target, setTarget] = useState<ConnectionTarget>(loadTarget);
  const character = useCharacterName();
  const live = status.kind === 'connecting' || status.kind === 'connected';

  // The actions read the newest values through refs, so they never
  // dial a stale target.
  const targetRef = useRef(target);
  const onErrorRef = useRef(onError);
  useEffect(() => {
    targetRef.current = target;
    onErrorRef.current = onError;
  });

  const saveTarget = useCallback((next: ConnectionTarget) => {
    targetRef.current = next;
    setTarget(next);
    saveConnectionTarget(next);
  }, []);

  // Settings edits the same saved target. Follow it, so the popover,
  // the title, and Cmd+R dial what Settings shows.
  useEffect(
    () =>
      subscribeConnectionTarget((next) => {
        targetRef.current = next;
        setTarget(next);
      }),
    [],
  );

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

  const world = worldName(
    status.kind === 'connected' || status.kind === 'connecting' ? status.host : target.host,
  );

  return { status, live, target, world, character, connect, connectNew, disconnect, saveTarget };
}

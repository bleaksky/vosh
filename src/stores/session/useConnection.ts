import { useCallback, useEffect, useRef, useState } from 'react';
import { profileResolveMatch, profileSwitch, profilesList } from '../../ipc/profiles';
import {
  connectSession,
  disconnectSession,
  emitConnectionTargetChanged,
  subscribeConnectionTargetChanged,
  type ConnectionTarget,
} from '../../ipc/session';
import { worldName } from '../../lib/knownWorlds';
import { errorText } from '../../lib/text';
import { pushToast } from '../toasts';
import { useSessionConnection, type ConnectionStatus } from './connectionStore';
import { getSelected, getSessions } from './sessionsStore';

// The session the title band shows and the session menu drives, which
// is the selected session, with the saved target Connect dials.
// MainWindow mounts the hook once. The connection store keeps each
// session's state, so it lives as long as the window and not only while
// the session menu or another control that shows it is mounted. The
// palette's connect entry and the Cmd+R shortcut call it through
// MainWindow. Each action names the session selected as it starts, so a
// selection the app has not heard yet never sends to another session.

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

/** The profile `session` plays, as its row names it, or the profile in
 *  front before the list names it. */
async function profileOf(session: number): Promise<string | null> {
  const row = getSessions().find((r) => r.id === session);
  return row?.profile ?? (await profilesList()).active;
}

/** Switch `session` to the profile that matches the host, then connect
 *  it. Profiles pinned to a character soft skip here because the
 *  character is unknown until the MUD sends Char.Status after login, and
 *  the session's GMCP handler swaps to them then. A switch that fails
 *  leaves the session on the profile it was using, says so in a toast,
 *  and connects under that profile. */
export async function connectTo(target: ConnectionTarget, session: number): Promise<void> {
  let switchTo: string | null = null;
  try {
    const matchName = await profileResolveMatch(target.host, target.port, null);
    if (matchName && matchName !== (await profileOf(session))) switchTo = matchName;
  } catch (matchErr) {
    // Profile resolve is best effort. A profile system error never
    // blocks a connect.
    console.warn('[profile match]', matchErr);
  }
  if (switchTo) {
    try {
      await profileSwitch(switchTo, session);
    } catch (switchErr) {
      console.warn('[profile switch]', switchErr);
      pushToast({ kind: 'error', message: profileSwitchErrorMessage(switchErr) });
    }
  }
  await connectSession(target.host, target.port, target.tls, session);
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

/** The selected session's connection state, and actions for the title
 *  band, the session menu, the palette, and Cmd+R. `onError` hears an
 *  action that failed, with the session it was for. Mount it once, in
 *  MainWindow. */
export function useConnection(onError: (message: string, session: number) => void): Connection {
  const [target, setTarget] = useState<ConnectionTarget>(loadTarget);
  const { status, character } = useSessionConnection();
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
    const session = getSelected();
    try {
      await connectTo(to, session);
    } catch (e) {
      onErrorRef.current(String(e), session);
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
    const session = getSelected();
    try {
      await disconnectSession(session);
    } catch (e) {
      onErrorRef.current(String(e), session);
    }
  }, []);

  const world = worldName(
    status.kind === 'connected' || status.kind === 'connecting' ? status.host : target.host,
  );

  return { status, live, target, world, character, connect, connectNew, disconnect, saveTarget };
}

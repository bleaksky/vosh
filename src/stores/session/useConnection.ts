import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { profileResolveMatch, profileSwitch, profilesList } from '../../ipc/profiles';
import {
  connectSession,
  disconnectSession,
  emitConnectionTargetChanged,
  reconnectNow,
  setSessionAddress,
  subscribeConnectionTargetChanged,
  type ConnectionTarget,
  type SessionRow,
} from '../../ipc/session';
import { worldName } from '../../lib/knownWorlds';
import { errorText } from '../../lib/text';
import { pushToast } from '../toasts';
import { useSessionConnection, type ConnectionStatus } from './connectionStore';
import { useReconnect, waitingTarget } from './reconnectStore';
import { getSelected, getSessions, useSelectedRow } from './sessionsStore';

// The session the title band shows and the session menu drives, which
// is the selected session, with the target Connect dials for it.
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

// The saved world, the last target you saved or dialed from a form, in
// browser storage on this machine. Each session keeps its own target in
// its row, which the app keeps for the next launch. The saved
// world is where a New session form starts, and where a session dials
// before it has a target of its own. The session popover and Settings ›
// General › Connection save both, and every window follows the saved
// world through subscribeConnectionTarget.
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

/** Save the saved world, and tell every window. */
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

/** Where `row`'s session dials: the target it keeps, else the saved
 *  world. A session keeps none before its first connect or form, nor a
 *  lone session with no name after a relaunch. */
export function targetOf(
  row: Pick<SessionRow, 'host' | 'port' | 'tls'> | null,
  saved: ConnectionTarget,
): ConnectionTarget {
  if (!row || row.host === null || row.port === null) return saved;
  return { host: row.host, port: row.port, tls: row.tls };
}

/** Keep `target` as where `session` dials, and as the saved world the
 *  next New session form starts from. */
export async function keepTarget(target: ConnectionTarget, session: number): Promise<void> {
  saveConnectionTarget(target);
  await setSessionAddress(session, target);
}

/** Where the selected session dials, and a setter that keeps a new
 *  target for it. Every window follows both, and the target keeps its
 *  identity until one of its parts changes. */
export function useSessionTarget(): [ConnectionTarget, (target: ConnectionTarget) => void] {
  const [saved, setSaved] = useState<ConnectionTarget>(loadTarget);
  useEffect(() => subscribeConnectionTarget(setSaved), []);
  const row = useSelectedRow();
  const host = row?.host ?? null;
  const port = row?.port ?? null;
  const tls = row?.tls ?? false;
  const target = useMemo(() => targetOf({ host, port, tls }, saved), [host, port, tls, saved]);
  const keep = useCallback((next: ConnectionTarget) => {
    setSaved(next);
    keepTarget(next, getSelected()).catch((e: unknown) => console.warn('[session target]', e));
  }, []);
  return [target, keep];
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

/** Connect `session` to `target`, or while a redial of it waits to dial
 *  that same target, dial that try now, as Reconnect now on the notice
 *  does. A connect to another world ends the series. Cmd+R, the session
 *  menu's Connect to row and the palette reach it. */
export async function connectOrRedial(target: ConnectionTarget, session: number): Promise<void> {
  const waits = waitingTarget(session);
  const same =
    waits !== null &&
    waits.host === target.host &&
    waits.port === target.port &&
    waits.tls === target.tls;
  if (same) await reconnectNow(session);
  else await connectTo(target, session);
}

/** Dial a session its New session form opened, on the profile the form
 *  chose, so no profile match runs first. The target becomes the saved
 *  world the next form starts from, and the session keeps it as its own
 *  as it dials. */
export async function connectOpened(target: ConnectionTarget, session: number): Promise<void> {
  saveConnectionTarget(target);
  await connectSession(target.host, target.port, target.tls, session);
}

export interface Connection {
  status: ConnectionStatus;
  /** Connecting or connected. */
  live: boolean;
  /** A redial waits or dials after a drop, so Disconnect can end it. */
  redialing: boolean;
  /** Where Connect dials the selected session next, its own target or
   *  else the saved world. */
  target: ConnectionTarget;
  /** The world the title names: the live host while a session runs,
   *  else the target. */
  world: string;
  character: string | null;
  /** Dial the selected session at its target, or its waiting redial
   *  now. */
  connect: () => Promise<void>;
  /** Dial a session its New session form opened, as connectOpened
   *  does. */
  connectNew: (target: ConnectionTarget, session: number) => Promise<void>;
  disconnect: () => Promise<void>;
  /** Keep a target for the selected session without dialing, as
   *  keepTarget does. */
  saveTarget: (target: ConnectionTarget) => void;
}

/** The selected session's connection state, and actions for the title
 *  band, the session menu, the palette, and Cmd+R. `onError` hears an
 *  action that failed, with the session it was for. Mount it once, in
 *  MainWindow. */
export function useConnection(onError: (message: string, session: number) => void): Connection {
  const [target, saveTarget] = useSessionTarget();
  const { status, character } = useSessionConnection();
  const live = status.kind === 'connecting' || status.kind === 'connected';
  const redial = useReconnect().kind;
  const redialing = redial === 'waiting' || redial === 'dialing';

  // The actions read the newest values through refs, so they never
  // dial a stale target.
  const targetRef = useRef(target);
  const onErrorRef = useRef(onError);
  useEffect(() => {
    targetRef.current = target;
    onErrorRef.current = onError;
  });

  const dial = useCallback(async (to: ConnectionTarget) => {
    const session = getSelected();
    try {
      await connectOrRedial(to, session);
    } catch (e) {
      onErrorRef.current(String(e), session);
    }
  }, []);

  const connect = useCallback(() => dial(targetRef.current), [dial]);

  const connectNew = useCallback(async (to: ConnectionTarget, session: number) => {
    try {
      await connectOpened(to, session);
    } catch (e) {
      onErrorRef.current(String(e), session);
    }
  }, []);

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

  return {
    status,
    live,
    redialing,
    target,
    world,
    character,
    connect,
    connectNew,
    disconnect,
    saveTarget,
  };
}

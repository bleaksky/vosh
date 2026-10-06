import { onReconnect } from '../../ipc/session';
import { createSessionStore } from '../sessionStore';

// Where each session's connection stands and who is logged in on it,
// for the title band, the window title, the status line and the macOS
// menu bar, which show the selected session. session://state moves the
// status, Char.Status and Char.Name name the character, and a disconnect
// puts both back. A drop that gives a reason marks the session with it,
// as an error a connect or a send met does, until its next connect. A
// redial try that fails ends its link with no reason, so the reason the
// try then gives marks the session again, and the error holds through
// the series. Your Disconnect gives no reason and leaves it idle.

export type ConnectionStatus =
  | { kind: 'idle' }
  | { kind: 'connecting'; host: string; port: number; tls: boolean }
  | { kind: 'connected'; host: string; port: number; tls: boolean }
  | { kind: 'error'; message: string };

export interface SessionConnection {
  status: ConnectionStatus;
  /** The character logged in, from Char.Status or Char.Name. */
  character: string | null;
}

const IDLE: SessionConnection = { status: { kind: 'idle' }, character: null };

/** A session whose link ended for `message`, with nobody logged in. */
const failed = (message: string): SessionConnection => ({
  status: { kind: 'error', message },
  character: null,
});

/** Take the character a packet names, when it names one. */
function named(now: SessionConnection, data: unknown): SessionConnection {
  const name = (data as { name?: unknown } | null)?.name;
  if (typeof name !== 'string' || name.trim().length === 0) return now;
  const character = name.trim();
  return character === now.character ? now : { ...now, character };
}

const store = createSessionStore<SessionConnection>({
  state: IDLE,
  packages: { 'Char.Status': named, 'Char.Name': named },
  connection: (now, payload) => {
    if (payload.kind === 'disconnected') return payload.reason ? failed(payload.reason) : IDLE;
    const { kind, host, port, tls } = payload;
    return { ...now, status: { kind, host, port, tls } };
  },
  events: [
    (apply) =>
      onReconnect((payload, session) => {
        if (payload.kind === 'failed') apply(session, () => failed(payload.reason));
      }),
  ],
});

export const startConnectionStore = store.start;
export const useSessionConnection = store.use;

/** Whether `session` dials or plays, as this window last heard. */
export function sessionLive(session: number): boolean {
  const { kind } = store.stateOf(session).status;
  return kind === 'connecting' || kind === 'connected';
}

/** Mark `session` with an error its connect or a send met. */
export function noteConnectionError(session: number, message: string): void {
  store.apply(session, (now) => ({ ...now, status: { kind: 'error', message } }));
}

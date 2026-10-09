import {
  onReconnect,
  type ConnectionTarget,
  type ReconnectPayload,
  type StatePayload,
} from '../../ipc/session';
import { createSessionStore } from '../sessionStore';

// Where the redial of each session stands after a drop, for the
// reconnect notice. The notice of the selected session counts down
// while a try waits, says which try dials, and once the tries run out
// holds until you connect again. session://reconnect moves it. A try
// that fails leaves it as it is, since the next wait or the end of the
// series follows. A try that reaches the game, your Cancel and a drop
// Vosh does not redial clear it. A stopped notice clears at the
// session's next connect, so Try again or Cmd+R takes it away. The
// store also keeps where the session last dialed, which is where a
// series that drop starts dials each try, since Rust keeps the address
// the drop left and not the one you save after it.

export type Reconnect =
  | { kind: 'none' }
  /** A try waits until `until`, a time in ms as Date.now() counts. */
  | { kind: 'waiting'; try: number; tries: number; until: number }
  | { kind: 'dialing'; try: number; tries: number }
  | { kind: 'stopped'; tries: number };

const NONE: Reconnect = { kind: 'none' };

interface Redial {
  redial: Reconnect;
  /** Where the session last dialed, from its connecting and connected
   *  states. */
  at: ConnectionTarget | null;
}

function redialed(now: Reconnect, payload: ReconnectPayload): Reconnect {
  switch (payload.kind) {
    case 'waiting':
      return {
        kind: 'waiting',
        try: payload.try,
        tries: payload.tries,
        until: Date.now() + payload.seconds * 1000,
      };
    case 'dialing':
      return { kind: 'dialing', try: payload.try, tries: payload.tries };
    case 'failed':
      return now;
    case 'stopped':
      return { kind: 'stopped', tries: payload.tries };
    case 'reached':
    case 'cancelled':
    case 'declined':
      return NONE;
  }
}

/** Each try dials as a connect does, so only a stopped notice clears at
 *  a connect, and a drop changes nothing. */
function connected(now: Redial, payload: StatePayload): Redial {
  if (payload.kind === 'disconnected') return now;
  const { host, port, tls } = payload;
  const redial = payload.kind === 'connecting' && now.redial.kind === 'stopped' ? NONE : now.redial;
  return { redial, at: { host, port, tls } };
}

const store = createSessionStore<Redial, Reconnect>({
  state: { redial: NONE, at: null },
  connection: connected,
  events: [
    (apply) =>
      onReconnect((payload, session) =>
        apply(session, (now) => {
          const redial = redialed(now.redial, payload);
          return redial === now.redial ? now : { ...now, redial };
        }),
      ),
  ],
  view: (state) => state.redial,
});

export const startReconnectStore = store.start;

/** Where the redial of the selected session stands. */
export const useReconnect = store.use;

/** Where the redial of `session` stands. */
export const reconnectOf = (session: number): Reconnect => store.stateOf(session).redial;

/** Where the waiting try of `session` dials, or null while no try
 *  waits. */
export function waitingTarget(session: number): ConnectionTarget | null {
  return reconnectOf(session).kind === 'waiting' ? store.stateOf(session).at : null;
}

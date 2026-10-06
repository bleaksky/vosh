import { onReconnect, type ReconnectPayload, type StatePayload } from '../../ipc/session';
import { createSessionStore } from '../sessionStore';

// Where the redial of each session stands after a drop, for the
// reconnect notice of the Alerts review (Q13 and Q14). The notice of
// the selected session counts down while a try waits, says which try
// dials, and once the tries run out holds until you connect again.
// session://reconnect moves it. A try that fails leaves it as it is,
// since the next wait or the end of the series follows. A try that
// reaches the game, your Cancel and a drop Vosh does not redial clear
// it. A stopped notice clears at the session's next connect, so Try
// again or Cmd+R takes it away.

export type Reconnect =
  | { kind: 'none' }
  /** A try waits until `until`, a time in ms as Date.now() counts. */
  | { kind: 'waiting'; try: number; tries: number; until: number }
  | { kind: 'dialing'; try: number; tries: number }
  | { kind: 'stopped'; tries: number };

const NONE: Reconnect = { kind: 'none' };

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
const connected = (now: Reconnect, payload: StatePayload): Reconnect =>
  payload.kind === 'connecting' && now.kind === 'stopped' ? NONE : now;

const store = createSessionStore<Reconnect>({
  state: NONE,
  connection: connected,
  events: [
    (apply) => onReconnect((payload, session) => apply(session, (now) => redialed(now, payload))),
  ],
});

export const startReconnectStore = store.start;

/** Where the redial of the selected session stands. */
export const useReconnect = store.use;

/** Where the redial of `session` stands. */
export const reconnectOf = store.stateOf;

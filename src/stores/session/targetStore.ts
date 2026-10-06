import { getTarget, onTarget, type TargetPayload } from '../../ipc/session';
import { createSessionStore } from '../sessionStore';

// The client target you set with the target command, for the status
// line and the target marker in the room rows, with the quick keys that
// act on it. The backend owns both and broadcasts session://target on
// every change, including the clear on disconnect. This is not the
// Char.Combat opponent (see combatStore).
//
// Each session keeps its own target and its own set of quick keys, which
// start as the stock slots and last until the session closes, as you
// answered on October 4. The store keeps them for each session and
// publishes the selected session's. A window that shows a session for
// the first time asks target_get for it.

const EMPTY: TargetPayload = { name: null, room_idx: null, quick_keys: [] };

const store = createSessionStore<TargetPayload>({
  state: EMPTY,
  events: [(apply) => onTarget((payload, session) => apply(session, () => payload))],
  // The backend sends its own clear, but only when a target was set.
  // Clearing here too keeps a stale name off the status line if that
  // event is missed. The quick keys belong to the session and stay.
  connection: (now, payload) =>
    payload.kind === 'disconnected' && (now.name !== null || now.room_idx !== null)
      ? { ...now, name: null, room_idx: null }
      : now,
  snapshot: { ask: getTarget, take: (_now, data) => data as TargetPayload },
});

export const startTargetStore = store.start;
export const getTargetState = store.get;
export const subscribeTargetState = store.subscribe;
export const useTarget = store.use;

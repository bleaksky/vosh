import { onRoundTrip } from '../../ipc/roundTrip';
import { createSessionStore } from '../sessionStore';

// The round trip to the game for the status line. Each session reads
// its own every two seconds and sends it on session://round-trip while
// it moves, so the store keeps each session's last reading and shows
// the selected one's. Nothing shows before the first reading, and a
// disconnect puts back null, as the session sends too.

const store = createSessionStore<number | null>({
  state: null,
  events: [(apply) => onRoundTrip((payload, session) => apply(session, () => payload.ms))],
});

export const startRoundTripStore = store.start;
export const getRoundTrip = store.get;
export const subscribeRoundTrip = store.subscribe;
export const useRoundTrip = store.use;

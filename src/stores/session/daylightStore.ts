import { daylightGet, subscribeDaylightChanged, type Daylight } from '../../ipc/tick';
import { createStore } from '../store';
import { getSelected, subscribeSelected } from './sessionsStore';

// The day or night in the selected session's game, which Switch themes
// With the game follows (Alerts Q16, Sessions Q23). Rust keeps each
// session's beside its tick, so every window can read it, and only the
// main window hears GMCP. Each window keeps its own copy. It reads
// daylight_get as it starts and again each time the selection moves to
// another session, a banner click on vosh://session-selected among them,
// and takes each vosh://daylight-changed of the selected session. A turn
// in a session behind changes nothing here.
//
// A turn heard or a selection made after a read began wins over its
// answer, so an older answer never puts back the phase it replaced.
// Null until the selected session's game sends World.Time.

const store = createStore<Daylight | null>(null);
let started = false;
/** Counts each read begun and each turn heard. */
let generation = 0;

/** Read the phase of `session`, or of the selected session in Rust. */
function read(session?: number): void {
  generation += 1;
  const mine = generation;
  daylightGet(session)
    .then((phase) => {
      if (mine === generation) store.set(phase);
    })
    .catch(() => undefined);
}

/** Start hearing the selected session's day and night. It runs once,
 *  and every subscribe starts it too. */
export function startDaylightStore(): void {
  if (started) return;
  started = true;
  subscribeSelected(() => read(getSelected()));
  // The listener is in first, so a turn that lands meanwhile is either
  // in the answer or newer than it.
  void subscribeDaylightChanged((phase, session) => {
    if (session !== getSelected()) return;
    generation += 1;
    store.set(phase);
  })
    .then(() => read())
    .catch(() => undefined);
}

/** The selected session's day or night, or null before its game said. */
export function getDaylight(): Daylight | null {
  return store.get();
}

/** Hear the selected session's day or night change. */
export function subscribeDaylight(cb: () => void): () => void {
  startDaylightStore();
  return store.subscribe(cb);
}

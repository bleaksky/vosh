import { hiddenGet, onHidden, onState, type HiddenPayload } from '../session';
import { createStore } from './store';

// Which values the game hides right now, from session://hidden. The
// prompt engine in the backend works it out on every server build from
// the latest Char.Vitals, Char.Affects, Group.Info and Char.Combat and
// from your prompt, and never stores it. It reports each change once, so
// the store also asks hidden_get for the last report when it starts.
//
// The new build marks each hidden packet with `"hidden": true`, and
// the stores read that flag themselves, so this store adds nothing
// there. An older build sends the true values under lamented tears and
// only Char.Affects names the song, so this store is how the panes
// learn to hide them. vitalsStore, affectsStore, combatStore and
// groupStore each OR their field from here into their own hidden flag.

export type HiddenState = HiddenPayload;

/** Nothing hidden, the state before the first report and after a
 *  disconnect. */
export const NOTHING_HIDDEN: HiddenState = {
  vitals: false,
  tank: false,
  opponent: false,
  affects: false,
  group: false,
};

/** Read a session://hidden payload. A field that is not `true` reads
 *  as shown. */
export function parseHidden(payload: unknown): HiddenState {
  const p = payload && typeof payload === 'object' ? (payload as Record<string, unknown>) : {};
  return {
    vitals: p.vitals === true,
    tank: p.tank === true,
    opponent: p.opponent === true,
    affects: p.affects === true,
    group: p.group === true,
  };
}

function same(a: HiddenState, b: HiddenState): boolean {
  return (
    a.vitals === b.vitals &&
    a.tank === b.tank &&
    a.opponent === b.opponent &&
    a.affects === b.affects &&
    a.group === b.group
  );
}

const store = createStore<HiddenState>(NOTHING_HIDDEN);
let started = false;
// Bumped by every report and every disconnect. The answer to hidden_get
// applies only when neither arrived after it was asked for, so it never
// replaces a newer report or brings back a stale one.
let generation = 0;

function take(payload: unknown): void {
  const next = parseHidden(payload);
  if (!same(store.get(), next)) store.set(next);
}

export function startHiddenStore(): void {
  if (started) return;
  started = true;
  const reports = onHidden((payload) => {
    generation += 1;
    take(payload);
  });
  const states = onState((payload) => {
    if (payload.kind !== 'disconnected') return;
    generation += 1;
    store.set(NOTHING_HIDDEN);
  });
  // The backend reports each change once. A window that opens or reloads
  // while the game hides something, Settings among them, asks for the
  // last report, or an older build would show the true values the song
  // hides. Asked once both listeners are in, so a report that lands
  // meanwhile is either in the answer or newer than it.
  void Promise.all([reports, states])
    .then(() => {
      const mine = generation;
      return hiddenGet().then((payload) => {
        if (mine === generation && payload != null) take(payload);
      });
    })
    .catch(() => undefined);
}

export function getHidden(): HiddenState {
  return store.get();
}

export function subscribeHidden(cb: () => void): () => void {
  startHiddenStore();
  return store.subscribe(cb);
}

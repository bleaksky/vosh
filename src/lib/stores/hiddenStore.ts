import { useSyncExternalStore } from 'react';
import { onHidden, onState, type HiddenPayload } from '../session';
import { createStore } from './store';

// Which values the game hides right now, from session://hidden. The
// prompt engine in the backend works it out on every server build from
// the latest Char.Vitals, Char.Affects, Group.Info and Char.Combat and
// from your prompt, and never stores it.
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

export function startHiddenStore(): void {
  if (started) return;
  started = true;
  void onHidden((payload) => {
    const next = parseHidden(payload);
    if (!same(store.get(), next)) store.set(next);
  });
  void onState((payload) => {
    if (payload.kind === 'disconnected') store.set(NOTHING_HIDDEN);
  });
}

export function getHidden(): HiddenState {
  return store.get();
}

export function subscribeHidden(cb: () => void): () => void {
  startHiddenStore();
  return store.subscribe(cb);
}

/** Which values the game hides right now. */
export function useHidden(): HiddenState {
  return useSyncExternalStore(subscribeHidden, getHidden);
}

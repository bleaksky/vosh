import { useSyncExternalStore } from 'react';
import type { AffectFulls } from '../affectsView';
import { affectFullGet, onState, subscribeAffectFullChanged } from '../session';
import { createStore } from './store';

// How full each affect was cast, for the Affects pane's gauges (the
// Countdown meter and the Grouped chips). The backend decides full
// (src-tauri/src/affects/full.rs) and sends the whole map on
// vosh://affect-full-changed whenever it changes. Seeded from
// affect_full_get for a window that opens mid session, and emptied on
// the disconnected state, as the backend empties its own.

const EMPTY: AffectFulls = Object.freeze({});
const store = createStore<AffectFulls>(EMPTY);
let started = false;
// Bumped by every change. The first read applies only when nothing
// arrived after it was asked for, so it never puts back an older map.
let generation = 0;

/** A map off the bus: whole hours only, anything else dropped. */
export function normalizeAffectFulls(raw: unknown): AffectFulls {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return EMPTY;
  const out: Record<string, number> = {};
  for (const [key, value] of Object.entries(raw as Record<string, unknown>)) {
    if (typeof value === 'number' && Number.isFinite(value)) out[key] = value;
  }
  return Object.keys(out).length === 0 ? EMPTY : out;
}

/** Keep the current snapshot when nothing in it moved, so the pane does
 *  not render again on a list that changed no full. */
function put(next: AffectFulls): void {
  const prev = store.get();
  const keys = Object.keys(next);
  if (keys.length === Object.keys(prev).length && keys.every((k) => prev[k] === next[k])) {
    return;
  }
  store.set(next);
}

export function startAffectFullStore(): void {
  if (started) return;
  started = true;
  const changes = subscribeAffectFullChanged((raw) => {
    generation += 1;
    put(normalizeAffectFulls(raw));
  });
  const states = onState((payload) => {
    if (payload.kind !== 'disconnected') return;
    generation += 1;
    put(EMPTY);
  });
  void Promise.all([changes, states])
    .then(() => {
      const mine = generation;
      return affectFullGet().then((raw) => {
        if (mine === generation) put(normalizeAffectFulls(raw));
      });
    })
    .catch(() => undefined);
}

export function getAffectFull(): AffectFulls {
  return store.get();
}

export function subscribeAffectFull(cb: () => void): () => void {
  startAffectFullStore();
  return store.subscribe(cb);
}

export function useAffectFull(): AffectFulls {
  return useSyncExternalStore(subscribeAffectFull, getAffectFull);
}

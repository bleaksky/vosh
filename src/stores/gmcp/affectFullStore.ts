import { affectFullGet, subscribeAffectFullChanged, type AffectFulls } from '../../ipc/affects';
import { createGmcpStore } from './gmcpStore';

// How full each affect was cast, for the Affects pane's gauges (the
// Countdown meter and the Grouped chips). The backend decides full
// (src-tauri/src/affects/full.rs) and sends the whole map on
// vosh://affect-full-changed whenever it changes, for each session.
// Seeded from affect_full_get the first time a window shows a session,
// and emptied on that session's disconnect, as the backend empties its
// own.

const EMPTY: AffectFulls = Object.freeze({});

/** A map off the bus: whole hours only, anything else dropped. */
export function normalizeAffectFulls(raw: unknown): AffectFulls {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return EMPTY;
  const out: Record<string, number> = {};
  for (const [key, value] of Object.entries(raw as Record<string, unknown>)) {
    if (typeof value === 'number' && Number.isFinite(value)) out[key] = value;
  }
  return Object.keys(out).length === 0 ? EMPTY : out;
}

/** The fulls in `raw`, or the current map when nothing in it moved, so
 *  the pane does not render again on a list that changed no full. */
function nextFulls(fulls: AffectFulls, raw: unknown): AffectFulls {
  const next = normalizeAffectFulls(raw);
  const keys = Object.keys(next);
  const same = keys.length === Object.keys(fulls).length && keys.every((k) => fulls[k] === next[k]);
  return same ? fulls : next;
}

const store = createGmcpStore<AffectFulls>({
  state: EMPTY,
  events: [
    (apply) =>
      subscribeAffectFullChanged((raw, session) =>
        apply(session, (fulls) => nextFulls(fulls, raw)),
      ),
  ],
  snapshot: { ask: affectFullGet, take: nextFulls },
});

export const startAffectFullStore = store.start;
export const getAffectFull = store.get;
export const useAffectFull = store.use;

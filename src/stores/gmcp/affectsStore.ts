import { useSyncExternalStore } from 'react';
import type { AffectModifier } from '../../lib/affects';
import { affectsSnapshotGet } from '../../ipc/affects';
import { createSessionStore } from '../sessionStore';
import { getHidden, subscribeHidden } from './hiddenStore';
import { asNumber, asText, isHiddenFlag } from '../store';

// Your current affects from Char.Affects, one row per affect name.
// Aabahran sends one entry per (affect, modifier) pair, resends the
// whole list every tick, on each add or remove, and at login. Duration
// is ticks left and -1 means permanent. The store keeps the list at
// module scope, so a pane that remounts shows it at once instead of
// going empty until the next tick. A window that opens between ticks,
// Settings among them, reads the last list the backend kept instead of
// waiting for the next one.
//
// Under lamented tears the list comes empty with `"hidden": true`. The
// store is then hidden until a list without the flag arrives, so the
// Affects pane says the game hides them instead of marking every
// tracked affect missing. An older server build sends the list with
// the song in it and no flag, and hiddenStore's `affects` hides the
// pane the same way.

export type AffectKind = 'spell' | 'song';

export interface CurrentAffect {
  name: string;
  /** Aabahran marks each row spell or song. null on servers that do
   *  not say. */
  kind: AffectKind | null;
  /** Ticks left. -1 is permanent. null when the server sent none. */
  duration: number | null;
  level: number | null;
  /** One entry per location the affect modifies, in server order. */
  modifiers: AffectModifier[];
}

/** Fold the raw rows into one row per name, in order of first
 *  appearance. When rows of one name disagree on duration the longest
 *  wins, since the name stays up until the last of them drops. */
export function groupCurrentAffects(data: unknown): CurrentAffect[] {
  const raw: unknown = Array.isArray(data)
    ? data
    : data && typeof data === 'object'
      ? (data as { affects?: unknown }).affects
      : null;
  if (!Array.isArray(raw)) return [];
  const byName = new Map<string, CurrentAffect>();
  for (const row of raw) {
    if (!row || typeof row !== 'object') continue;
    const r = row as Record<string, unknown>;
    const name = asText(r.name);
    if (!name) continue;
    const duration = durationOf(r.duration);
    let group = byName.get(name);
    if (!group) {
      group = {
        name,
        kind: r.kind === 'spell' || r.kind === 'song' ? r.kind : null,
        duration,
        level: asNumber(r.level),
        modifiers: [],
      };
      byName.set(name, group);
    } else if (outlasts(duration, group.duration)) {
      group.duration = duration;
    }
    const location = asText(r.location);
    if (location && location !== 'none') {
      const modifier =
        typeof r.modifier === 'number' || typeof r.modifier === 'string' ? r.modifier : 0;
      group.modifiers.push({ location, modifier });
    }
  }
  return [...byName.values()];
}

/** What the store keeps. `list` is null until the first Char.Affects
 *  since you connected, so the pane can tell "no affects on you" from
 *  "the server has not said yet". */
interface AffectsState {
  list: CurrentAffect[] | null;
  /** The game hides your affects. The list is empty meanwhile. */
  hidden: boolean;
}

/** One Char.Affects packet: its rows and whether the game hides
 *  them. */
export function parseAffectsPacket(data: unknown): { list: CurrentAffect[]; hidden: boolean } {
  return { list: groupCurrentAffects(data), hidden: isHiddenFlag(data) };
}

function durationOf(value: unknown): number | null {
  const n = asNumber(value);
  if (n === null) return null;
  return n < 0 ? -1 : Math.floor(n);
}

function outlasts(next: number | null, prev: number | null): boolean {
  if (next === null) return false;
  if (prev === null) return true;
  if (prev < 0) return false;
  return next < 0 || next > prev;
}

const EMPTY: AffectsState = { list: null, hidden: false };

const store = createSessionStore<AffectsState>({
  state: EMPTY,
  packages: { 'Char.Affects': (_, data) => parseAffectsPacket(data) },
  snapshot: {
    ask: affectsSnapshotGet,
    take: (state, data) => (data == null ? state : parseAffectsPacket(data)),
  },
});

export const startAffectsStore = store.start;

export function getAffects(): CurrentAffect[] | null {
  return store.get().list;
}

/** True while the game hides your affects, by the packet's own flag
 *  or by what the backend worked out. Both read the selected session,
 *  the list from this store and the flags from the hidden store. */
export function getAffectsHidden(): boolean {
  return store.get().hidden || getHidden().affects;
}

/** Hear the list and the hidden state the backend works out, each for
 *  the selected session, and each again on a selection. The hidden
 *  store joins here and not as one of the store's events, since each
 *  event the store hears counts against its snapshot, and a report that
 *  lands while it asks would throw away the list the backend kept. */
function subscribe(cb: () => void): () => void {
  const lists = store.subscribe(cb);
  const hidden = subscribeHidden(cb);
  return () => {
    lists();
    hidden();
  };
}

/** Current affects, or null until the server has sent the list. */
export function useAffects(): CurrentAffect[] | null {
  return useSyncExternalStore(subscribe, getAffects);
}

/** True while the game hides your affects. */
export function useAffectsHidden(): boolean {
  return useSyncExternalStore(subscribe, getAffectsHidden);
}

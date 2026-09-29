import { useSyncExternalStore } from 'react';
import type { AffectModifier } from '../affects';
import { affectsSnapshotGet, onGmcpPackage, onState } from '../session';
import { asNumber, asText, createStore, isHiddenFlag } from './store';

// Your current affects from Char.Affects, one row per affect name.
// Aabahran sends one entry per (affect, modifier) pair, resends the
// whole list every tick, on each add or remove, and at login. Duration
// is ticks left and -1 means permanent. Lifted from AffectsBar, which
// held this in component state and went empty on every remount until
// the next tick.
//
// Under lamented tears the list comes empty with `"hidden": true`. The
// store is then hidden until a list without the flag arrives, so the
// Affects pane says the game hides them instead of marking every
// tracked affect missing.

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
const store = createStore<AffectsState>(EMPTY);
let started = false;
// Bumped by every list and every disconnect. The snapshot applies only
// when neither arrived after it was asked for, so it never replaces a
// newer list or brings back a stale one.
let generation = 0;

export function startAffectsStore(): void {
  if (started) return;
  started = true;
  const lists = onGmcpPackage<unknown>('Char.Affects', (data) => {
    generation += 1;
    store.set(parseAffectsPacket(data));
  });
  const states = onState((payload) => {
    if (payload.kind !== 'disconnected') return;
    generation += 1;
    store.set(EMPTY);
  });
  // A window that opens between ticks, Settings among them, reads the
  // last list the backend kept instead of waiting for the next one.
  // Asked once both listeners are in, so a list that lands meanwhile
  // is either in the snapshot or newer than it.
  void Promise.all([lists, states])
    .then(() => {
      const mine = generation;
      return affectsSnapshotGet().then((data) => {
        if (mine === generation && data != null) store.set(parseAffectsPacket(data));
      });
    })
    .catch(() => undefined);
}

export function getAffects(): CurrentAffect[] | null {
  return store.get().list;
}

/** True while the game hides your affects. */
export function getAffectsHidden(): boolean {
  return store.get().hidden;
}

export function subscribeAffects(cb: () => void): () => void {
  startAffectsStore();
  return store.subscribe(cb);
}

/** Current affects, or null until the server has sent the list. */
export function useAffects(): CurrentAffect[] | null {
  return useSyncExternalStore(subscribeAffects, getAffects);
}

/** True while the game hides your affects. */
export function useAffectsHidden(): boolean {
  return useSyncExternalStore(subscribeAffects, getAffectsHidden);
}

import { useSyncExternalStore } from 'react';
import type { AffectModifier } from '../affects';
import { onGmcpPackage, onState } from '../session';
import { asNumber, asText, createStore } from './store';

// Your current affects from Char.Affects, one row per affect name.
// Aabahran sends one entry per (affect, modifier) pair, resends the
// whole list every tick, on each add or remove, and at login. Duration
// is ticks left and -1 means permanent. Lifted from AffectsBar, which
// held this in component state and went empty on every remount until
// the next tick.

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

// null until the first Char.Affects since you connected, so the pane
// can tell "no affects on you" from "the server has not said yet".
const store = createStore<CurrentAffect[] | null>(null);
let started = false;

export function startAffectsStore(): void {
  if (started) return;
  started = true;
  void onGmcpPackage<unknown>('Char.Affects', (data) => {
    store.set(groupCurrentAffects(data));
  });
  void onState((payload) => {
    if (payload.kind === 'disconnected') store.set(null);
  });
}

export function getAffects(): CurrentAffect[] | null {
  return store.get();
}

export function subscribeAffects(cb: () => void): () => void {
  startAffectsStore();
  return store.subscribe(cb);
}

/** Current affects, or null until the server has sent the list. */
export function useAffects(): CurrentAffect[] | null {
  return useSyncExternalStore(subscribeAffects, getAffects);
}

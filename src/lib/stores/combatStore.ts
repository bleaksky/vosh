import { useSyncExternalStore } from 'react';
import { onGmcpPackage, onState } from '../session';
import { asNumber, asText, createStore } from './store';

// The opponent you are fighting, from Char.Combat. Aabahran sends
// `{target, condition, hp_pct}` on each prompt in a fight and `{}` when
// the fight ends. This is the server's view of the fight, separate
// from the client target in targetStore. Lifted from useCombat, which
// held it per mount.

export interface CombatOpponent {
  name: string;
  /** Opponent health, whole percent 0..100. null when not sent. */
  hp_pct: number | null;
  /** The server's wording, like "big nasty wounds". */
  condition: string | null;
}

/** Parse a Char.Combat payload. null when no fight is on. */
export function parseCombat(data: unknown): CombatOpponent | null {
  if (!data || typeof data !== 'object') return null;
  const obj = data as Record<string, unknown>;
  const name = asText(obj.target);
  if (!name) return null;
  const hp = asNumber(obj.hp_pct);
  return {
    name,
    hp_pct: hp === null ? null : Math.max(0, Math.min(100, Math.round(hp))),
    condition: asText(obj.condition),
  };
}

function sameOpponent(a: CombatOpponent | null, b: CombatOpponent | null): boolean {
  if (a === null || b === null) return a === b;
  return a.name === b.name && a.hp_pct === b.hp_pct && a.condition === b.condition;
}

const store = createStore<CombatOpponent | null>(null);
let started = false;

export function startCombatStore(): void {
  if (started) return;
  started = true;
  void onGmcpPackage<unknown>('Char.Combat', (data) => {
    const next = parseCombat(data);
    // Char.Combat rides every prompt, so skip the ones that repeat.
    if (!sameOpponent(store.get(), next)) store.set(next);
  });
  void onState((payload) => {
    if (payload.kind === 'disconnected') store.set(null);
  });
}

export function getCombat(): CombatOpponent | null {
  return store.get();
}

export function subscribeCombat(cb: () => void): () => void {
  startCombatStore();
  return store.subscribe(cb);
}

/** The opponent you are fighting, or null out of combat. */
export function useCombat(): CombatOpponent | null {
  return useSyncExternalStore(subscribeCombat, getCombat);
}

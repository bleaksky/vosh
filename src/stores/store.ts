// Shared plumbing for the module stores under src/lib/stores. Each
// store keeps one immutable snapshot at module scope, so a pane that
// remounts (split, moved, shown here instead) reads the last value at
// once instead of going blank until the server pushes again. The
// backend keeps no GMCP replay buffer, and packages like World.Moons
// or Room.Info can be minutes or hours apart.
//
// A snapshot is replaced, never mutated, and `set` skips the notify
// when the caller hands back the same object. That keeps
// useSyncExternalStore consumers from re-rendering on a no-op push.

export interface Store<T> {
  get(): T;
  set(next: T): void;
  subscribe(cb: () => void): () => void;
}

export function createStore<T>(initial: T): Store<T> {
  let value = initial;
  const listeners = new Set<() => void>();
  return {
    get: () => value,
    set(next: T) {
      if (Object.is(next, value)) return;
      value = next;
      for (const cb of listeners) cb();
    },
    subscribe(cb: () => void) {
      listeners.add(cb);
      return () => {
        listeners.delete(cb);
      };
    },
  };
}

/** Parse a GMCP number that may arrive as a number or a numeric
 *  string. Returns null for anything else. */
export function asNumber(value: unknown): number | null {
  if (typeof value === 'number') return Number.isFinite(value) ? value : null;
  if (typeof value === 'string' && value.trim().length > 0) {
    const n = Number(value);
    return Number.isFinite(n) ? n : null;
  }
  return null;
}

/** True when a GMCP payload carries `"hidden": true`. Aabahran adds it
 *  to Char.Vitals, Char.Affects, Group.Info and Char.Combat while the
 *  game withholds their values, under lamented tears among other
 *  things, and leaves it out once it shows them again. A hidden value
 *  is never filled from another source and never reads as a warning. */
export function isHiddenFlag(data: unknown): boolean {
  return !!data && typeof data === 'object' && (data as { hidden?: unknown }).hidden === true;
}

/** Trimmed non-empty string, else null. */
export function asText(value: unknown): string | null {
  if (typeof value !== 'string') return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

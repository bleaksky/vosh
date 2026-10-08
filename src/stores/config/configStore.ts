import { useSyncExternalStore } from 'react';
import { subscribeProfileSwitched } from '../../ipc/profiles';
import { createStore } from '../store';

// A store for one setting the active profile keeps. start reads the
// setting at once and again on every vosh://profile-switched, since
// each profile keeps its own, and takes each value `follow` hears as it
// lands. A heard value bumps the generation, and a read applies only
// when nothing was heard after it began, so a slow read for the old
// profile cannot undo a pick made since. subscribe starts the store
// too, so a window that never ran startStores still fills it. set takes
// a value the window saved itself, the same way.

interface ConfigStoreOptions<T> {
  /** The value until the first read lands. */
  initial: T;
  /** Read the active profile's value. */
  read: () => Promise<T>;
  /** Hear each new value the backend or another window sends. */
  follow: (cb: (value: T) => void) => Promise<unknown>;
  /** True when two values show the same, so the store keeps the
   *  snapshot it has and nothing renders again. Without it a new
   *  object always replaces the snapshot. */
  same?: (a: T, b: T) => boolean;
}

export function createConfigStore<T>({ initial, read, follow, same }: ConfigStoreOptions<T>) {
  const store = createStore<T>(initial);
  let started = false;
  let generation = 0;

  function put(next: T): void {
    if (same?.(store.get(), next)) return;
    store.set(next);
  }

  function reread(): void {
    const mine = ++generation;
    read()
      .then((value) => {
        if (mine === generation) put(value);
      })
      .catch(() => undefined);
  }

  function start(): void {
    if (started) return;
    started = true;
    reread();
    void follow((value) => {
      generation += 1;
      put(value);
    });
    void subscribeProfileSwitched(() => reread());
  }

  function subscribe(cb: () => void): () => void {
    start();
    return store.subscribe(cb);
  }

  function use(): T {
    return useSyncExternalStore(subscribe, store.get);
  }

  /** Take a value this window just saved itself, which no event brings
   *  back. A read already on its way no longer applies. */
  function set(value: T): void {
    generation += 1;
    put(value);
  }

  return { start, get: store.get, set, subscribe, use };
}

import { useSyncExternalStore } from 'react';
import { subscribeProfileSwitched } from '../../ipc/profiles';
import { createStore } from '../store';

// A store for one setting the active profile keeps. start reads the
// setting at once and again on every vosh://profile-switched, since
// each profile keeps its own, and takes each value `follow` hears as it
// lands. A heard value bumps the generation, and a read applies only
// when nothing was heard after it began, so a slow read for the old
// profile cannot undo a pick made since. subscribe starts the store
// too, so a window that never ran startStores still fills it.

interface ConfigStoreOptions<T> {
  /** The value until the first read lands. */
  initial: T;
  /** Read the active profile's value. */
  read: () => Promise<T>;
  /** Hear each new value the backend or another window sends. */
  follow: (cb: (value: T) => void) => Promise<unknown>;
}

export function createConfigStore<T>({ initial, read, follow }: ConfigStoreOptions<T>) {
  const store = createStore<T>(initial);
  let started = false;
  let generation = 0;

  function reread(): void {
    const mine = ++generation;
    read()
      .then((value) => {
        if (mine === generation) store.set(value);
      })
      .catch(() => undefined);
  }

  function start(): void {
    if (started) return;
    started = true;
    reread();
    void follow((value) => {
      generation += 1;
      store.set(value);
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

  return { start, get: store.get, subscribe, use };
}

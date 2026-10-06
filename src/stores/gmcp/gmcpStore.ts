import { useSyncExternalStore } from 'react';
import { onGmcpPackage, onState, type StatePayload } from '../../ipc/session';
import { createStore } from '../store';

// A store for what the game sends over GMCP. Its state is one value,
// so a later change can keep one for each session in its place. Every
// input moves that value only through a change, a function that takes
// the state and returns the next one, or the same state when nothing
// moved. Each change then publishes through createStore, which skips a
// snapshot that did not change. start runs once and registers every
// listener before it returns. subscribe starts the store too, and get
// does not.

type Change<S> = (state: S) => S;

interface GmcpStoreSpec<S> {
  /** The state before anything is heard. */
  state: S;
  /** The change each package's data makes, by package name. */
  packages: Record<string, (state: S, data: unknown) => S>;
  /** The change a connection state makes. Without it a disconnect
   *  puts back `state`, and connecting or connected changes nothing. */
  connection?: (state: S, payload: StatePayload) => S;
}

export function createGmcpStore<S>({ state: initial, packages, connection }: GmcpStoreSpec<S>) {
  const store = createStore<S>(initial);
  const connectionChange =
    connection ??
    ((state: S, payload: StatePayload) => (payload.kind === 'disconnected' ? initial : state));
  let started = false;

  function apply(change: Change<S>): void {
    store.set(change(store.get()));
  }

  function start(): void {
    if (started) return;
    started = true;
    for (const [name, change] of Object.entries(packages)) {
      void onGmcpPackage<unknown>(name, (data) => apply((state) => change(state, data)));
    }
    void onState((payload) => apply((state) => connectionChange(state, payload)));
  }

  function subscribe(cb: () => void): () => void {
    start();
    return store.subscribe(cb);
  }

  function use(): S {
    return useSyncExternalStore(subscribe, store.get);
  }

  return { start, get: store.get, subscribe, use };
}

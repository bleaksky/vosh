import { useSyncExternalStore } from 'react';
import type { UnlistenFn } from '@tauri-apps/api/event';
import { onGmcpPackage, onState, type StatePayload } from '../../ipc/session';
import { createStore } from '../store';

// A store for what the game sends over GMCP. Its state is one value,
// so a later change can keep one for each session in its place. Every
// input moves that value only through a change, a function that takes
// the state and returns the next one, or the same state when nothing
// moved. After each change the store publishes what the panes read
// through createStore, which skips a value that did not change. That is
// the state, or a view of it that hands back what the panes read when
// nothing they read moved. start runs once and registers every
// listener before it returns. subscribe starts the store too, and get
// does not.
//
// A store with a snapshot asks the backend for the last value it kept
// once every listener is in, so a value that lands meanwhile is either
// in the answer or newer than it. The generation counts each packet and
// event the store hears and each disconnect, even one that finds the
// state empty, since the backend empties what it kept then too. The
// answer applies only when the generation has not moved since the ask,
// so it never replaces a newer value or brings back a stale one.

type Change<S> = (state: S) => S;

interface GmcpStoreSpec<S, V> {
  /** The state before anything is heard. */
  state: S;
  /** The change each package's data makes, by package name. */
  packages?: Record<string, (state: S, data: unknown) => S>;
  /** The change a connection state makes. Without it a disconnect
   *  puts back `state`, and connecting or connected changes nothing. */
  connection?: (state: S, payload: StatePayload) => S;
  /** The store's other session events. Each one starts hearing its
   *  event and runs every change it hears through `apply`, which
   *  publishes the result. */
  events?: ((apply: (change: Change<S>) => void) => Promise<UnlistenFn> | (() => void))[];
  /** The last value the backend kept, for a window that opens mid
   *  session. `ask` reads it and `take` is the change its answer
   *  makes. */
  snapshot?: { ask: () => Promise<unknown>; take: (state: S, data: unknown) => S };
  /** What the panes read, from the state and from what they read now,
   *  which is undefined for the first view. It runs after every change,
   *  one that keeps the state too, so a view that reads another store
   *  can follow it. Without it the panes read the state. */
  view?: (state: S, last?: V) => V;
}

export function createGmcpStore<S, V = S>({
  state: initial,
  packages = {},
  connection,
  events = [],
  snapshot,
  view = (state) => state as unknown as V,
}: GmcpStoreSpec<S, V>) {
  let current = initial;
  const store = createStore<V>(view(current));
  const connectionChange =
    connection ??
    ((state: S, payload: StatePayload) => (payload.kind === 'disconnected' ? initial : state));
  let started = false;
  let generation = 0;

  function apply(change: Change<S>): void {
    current = change(current);
    store.set(view(current, store.get()));
  }

  /** Apply a change a packet or an event brought, and count it. */
  function hear(change: Change<S>): void {
    generation += 1;
    apply(change);
  }

  function start(): void {
    if (started) return;
    started = true;
    const listening: unknown[] = [
      ...Object.entries(packages).map(([name, change]) =>
        onGmcpPackage<unknown>(name, (data) => hear((state) => change(state, data))),
      ),
      ...events.map((event) => event(hear)),
      onState((payload) => {
        if (payload.kind === 'disconnected') generation += 1;
        apply((state) => connectionChange(state, payload));
      }),
    ];
    if (!snapshot) return;
    const { ask, take } = snapshot;
    void Promise.all(listening)
      .then(() => {
        const mine = generation;
        return ask().then((data) => {
          if (mine === generation) apply((state) => take(state, data));
        });
      })
      .catch(() => undefined);
  }

  function subscribe(cb: () => void): () => void {
    start();
    return store.subscribe(cb);
  }

  function use(): V {
    return useSyncExternalStore(subscribe, store.get);
  }

  return { start, get: store.get, subscribe, use };
}

import { useSyncExternalStore } from 'react';
import type { UnlistenFn } from '@tauri-apps/api/event';
import { onGmcpPackage, onState, type StatePayload } from '../ipc/session';
import { createStore } from './store';
import {
  getSelected,
  getSessions,
  subscribeSelected,
  subscribeSessions,
} from './session/sessionsStore';

// A store that keeps one state for each session, since a session behind
// keeps playing, and shows the selected one's. The GMCP stores build on
// it with the packages each hears, and the session stores, such as your
// target, the tick and the rows of the sessions sidebar, with the
// session events each hears. Every input moves the state of the session
// it names only through a change, a function that takes that state and
// returns the next one, or the same state when nothing moved. A
// session's state starts the first time anything names it, and goes
// once the session leaves the list.
//
// The panes read the selected session. After each change to it, and on
// each selection, the store publishes what they read through
// createStore, which skips a value that did not change. That is the
// state, or a view of it that hands back what the panes read when
// nothing they read moved. A view keeps nothing of its own, so a
// selection carries nothing from one session to the next. start runs
// once and registers every listener before it returns. subscribe starts
// the store too, and get does not.
//
// Another store can read one session's state with stateOf and hear each
// change that moves it with subscribeStates, as the stores that lay the
// hidden flags over their own read the flags of the same session. The
// page changes a session's state itself through apply, as an error a
// connect met marks the session it was for.
//
// A store with a snapshot asks the backend for the last value it kept
// for a session the first time that session is selected, or the first
// time ask names it, for a window that shows one session whatever is
// selected, as the snoop window does. It asks once every
// listener is in, so a value that lands meanwhile is either in the
// answer or newer than it. Each session's generation counts each packet
// and event the store hears for it and each of its disconnects, even
// one that finds the state empty, since the backend empties what it kept
// then too. The answer applies only when that generation has not moved
// since the ask, so it never replaces a newer value or brings back a
// stale one.

type Change<S> = (state: S) => S;

/** Apply a change to the state of the session it names. */
type Apply<S> = (session: number, change: Change<S>) => void;

interface SessionStoreSpec<S, V> {
  /** The state before anything is heard. A function gives it afresh
   *  for a session as its state starts and at a disconnect that puts it
   *  back, for a state that starts from what another store holds. */
  state: S | ((session: number) => S);
  /** The change each package's data makes, by package name. */
  packages?: Record<string, (state: S, data: unknown) => S>;
  /** The change a connection state makes. Without it a disconnect
   *  puts back `state`, and connecting or connected changes nothing. */
  connection?: (state: S, payload: StatePayload) => S;
  /** The store's other session events. Each one starts hearing its
   *  event and runs every change it hears through `apply`, with the
   *  session it names, which publishes the result. */
  events?: ((apply: Apply<S>) => Promise<UnlistenFn> | (() => void))[];
  /** The last value the backend kept, for a session a window first
   *  shows mid session. `ask` reads it for a session and `take` is the
   *  change its answer makes. */
  snapshot?: { ask: (session: number) => Promise<unknown>; take: (state: S, data: unknown) => S };
  /** What the panes read, from the selected session's state, from what
   *  they read now, which is undefined for the first view, and from that
   *  session's id. It reads what they read now only to hand it back when
   *  nothing in it moved, so the state holds all the store knows. It runs
   *  after every change to the selected session, one that keeps the state
   *  too, so a view that reads another store can follow it. Without it
   *  the panes read the state. */
  view?: (state: S, last: V | undefined, session: number) => V;
}

/** One session's state, with what its snapshot needs. */
interface Slot<S> {
  state: S;
  /** The packets, events and disconnects heard for the session. */
  generation: number;
  /** The snapshot was asked for. */
  asked: boolean;
}

export function createSessionStore<S, V = S>({
  state: initial,
  packages = {},
  connection,
  events = [],
  snapshot,
  view = (state) => state as unknown as V,
}: SessionStoreSpec<S, V>) {
  const fresh = typeof initial === 'function' ? (initial as (session: number) => S) : () => initial;
  const slots = new Map<number, Slot<S>>();
  const first = getSelected();
  const store = createStore<V>(view(fresh(first), undefined, first));
  /** Who hears each change that moves a session's state. */
  const moved = new Set<(session: number) => void>();
  const connectionChange =
    connection ??
    ((state: S, payload: StatePayload) =>
      payload.kind === 'disconnected' ? fresh(payload.session) : state);
  let started = false;
  /** Settles once every listener is in, for a store with a snapshot. */
  let listening: Promise<unknown> = Promise.resolve();

  /** The session's slot, which starts the first time anything names the
   *  session. */
  function slot(session: number): Slot<S> {
    let held = slots.get(session);
    if (!held) {
      held = { state: fresh(session), generation: 0, asked: false };
      slots.set(session, held);
    }
    return held;
  }

  /** Publish what the panes read of the selected session. */
  function publish(): void {
    const session = getSelected();
    store.set(view(slot(session).state, store.get(), session));
  }

  /** Apply a change to the state of the session it names, and publish
   *  it when that session shows. */
  function apply(session: number, change: Change<S>): void {
    const held = slot(session);
    const before = held.state;
    held.state = change(before);
    if (held.state !== before) for (const cb of moved) cb(session);
    if (session === getSelected()) publish();
  }

  /** Apply a change a packet or an event brought, and count it. */
  function hear(session: number, change: Change<S>): void {
    slot(session).generation += 1;
    apply(session, change);
  }

  /** Ask for a session's snapshot, the first time only. */
  function askOnce(session: number = getSelected()): void {
    if (!snapshot) return;
    const held = slot(session);
    if (held.asked) return;
    held.asked = true;
    const { ask, take } = snapshot;
    void listening
      .then(() => {
        const mine = held.generation;
        return ask(session).then((data) => {
          if (slots.get(session) === held && mine === held.generation) {
            apply(session, (state) => take(state, data));
          }
        });
      })
      .catch(() => undefined);
  }

  /** Show the session now selected. */
  function follow(): void {
    publish();
    askOnce();
  }

  /** Drop the state of each session that left the list. */
  function dropGone(): void {
    const rows = getSessions();
    // The list has not come yet.
    if (rows.length === 0) return;
    const held = new Set(rows.map((row) => row.id));
    for (const session of slots.keys()) {
      if (!held.has(session)) slots.delete(session);
    }
  }

  function start(): void {
    if (started) return;
    started = true;
    publish();
    const heard: unknown[] = [
      ...Object.entries(packages).map(([name, change]) =>
        onGmcpPackage<unknown>(name, (data, session) =>
          hear(session, (state) => change(state, data)),
        ),
      ),
      ...events.map((event) => event(hear)),
      onState((payload) => {
        if (payload.kind === 'disconnected') slot(payload.session).generation += 1;
        apply(payload.session, (state) => connectionChange(state, payload));
      }),
    ];
    subscribeSelected(follow);
    subscribeSessions(dropGone);
    if (!snapshot) return;
    listening = Promise.all(heard);
    askOnce();
  }

  /** Start the store and ask for the snapshot of `session`, the first
   *  time only, for a window that shows a session other than the one
   *  selected. */
  function ask(session: number): void {
    start();
    askOnce(session);
  }

  function subscribe(cb: () => void): () => void {
    start();
    return store.subscribe(cb);
  }

  function use(): V {
    return useSyncExternalStore(subscribe, store.get);
  }

  /** The state of the session `session` names, as it starts when
   *  nothing named it yet. */
  function stateOf(session: number): S {
    return slots.get(session)?.state ?? fresh(session);
  }

  /** Hear each change that moves a session's state, with that session. */
  function subscribeStates(cb: (session: number) => void): () => void {
    start();
    moved.add(cb);
    return () => {
      moved.delete(cb);
    };
  }

  return { start, get: store.get, subscribe, use, apply, stateOf, subscribeStates, ask };
}

import { useSyncExternalStore } from 'react';
import {
  FIRST_SESSION,
  listSessions,
  onSessionSelected,
  onSessionsChanged,
  selectSession,
  type SessionRow,
} from '../../ipc/session';
import { createStore } from '../store';

// The sessions the app holds, in the order the sidebar lists them, and
// the one selected, which the GMCP stores show. Each window keeps its
// own copy. It reads sessions_list as it starts and takes the rows of
// each vosh://sessions-changed, which the app sends after every step
// that changes what a row shows, a selection among them. A banner click
// that selects a session sends vosh://session-selected too, and the
// store reads the list again then.
//
// Until the first list comes, the selected session is the one the app
// starts with. A read applies only when no list came and no selection
// was made here after it began, so its answer never puts back older
// rows or another selection.

interface Sessions {
  rows: SessionRow[];
  selected: number;
}

const store = createStore<Sessions>({ rows: [], selected: FIRST_SESSION });
let started = false;
/** Counts each list heard and each selection made here. */
let generation = 0;

/** Take the rows of a list and the selection it marks. */
function take(rows: SessionRow[]): void {
  const selected = rows.find((row) => row.selected)?.id ?? store.get().selected;
  store.set({ rows, selected });
}

/** Read the list again. */
function read(): void {
  const mine = generation;
  listSessions()
    .then((rows) => {
      if (mine === generation) take(rows);
    })
    .catch(() => undefined);
}

/** Start hearing the list. It runs once, and every subscribe starts it
 *  too. */
export function startSessionsStore(): void {
  if (started) return;
  started = true;
  const listening = [
    onSessionsChanged((rows) => {
      generation += 1;
      take(rows);
    }),
    onSessionSelected(() => read()),
  ];
  // Every listener is in first, so a list that lands meanwhile is either
  // in the answer or newer than it.
  void Promise.all(listening)
    .then(read)
    .catch(() => undefined);
}

/** Select a session. Every view here shows it at once, and the app
 *  hears it after. A selection the app refuses reads the list again. */
export function select(id: number): void {
  generation += 1;
  const now = store.get();
  if (now.selected !== id) store.set({ ...now, selected: id });
  selectSession(id).catch(() => read());
}

/** The selected session's id. */
export function getSelected(): number {
  return store.get().selected;
}

/** Every session's row, in the order the sidebar lists them. Empty until
 *  the first list comes. */
export function getSessions(): SessionRow[] {
  return store.get().rows;
}

/** Hear each change to the rows or to the selection. */
export function subscribeSessions(cb: () => void): () => void {
  startSessionsStore();
  return store.subscribe(cb);
}

/** Hear the selection move to another session. */
export function subscribeSelected(cb: () => void): () => void {
  let last = getSelected();
  return subscribeSessions(() => {
    const now = getSelected();
    if (now === last) return;
    last = now;
    cb();
  });
}

function selectedRow(): SessionRow | null {
  const { rows, selected } = store.get();
  return rows.find((row) => row.id === selected) ?? null;
}

/** Every session's row, in the order the sidebar lists them. */
export function useSessions(): SessionRow[] {
  return useSyncExternalStore(subscribeSessions, getSessions);
}

/** The selected session's row, or null until a list names it. */
export function useSelectedRow(): SessionRow | null {
  return useSyncExternalStore(subscribeSessions, selectedRow);
}

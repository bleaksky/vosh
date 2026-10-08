import { useSyncExternalStore } from 'react';
import {
  FIRST_SESSION,
  listSessions,
  moveSession,
  onSessionSelected,
  onSessionsChanged,
  renameSession,
  selectSession,
  type SessionRow,
} from '../../ipc/session';
import { errorText } from '../../lib/text';
import { createStore } from '../store';
import { pushToast } from '../toasts';

// The sessions the app holds, in the order the sidebar lists them, and
// the one selected, which the GMCP stores show. Each window keeps its
// own copy. It reads sessions_list as it starts and takes the rows of
// each vosh://sessions-changed, which the app sends after every step
// that changes what a row shows, a selection among them. A banner click
// that selects a session sends vosh://session-selected too, and the
// store reads the list again then.
//
// Until the first list comes, the selected session is the one the app
// starts with. A read applies only when no list came and no selection,
// rename or move was made here after it began, so its answer never puts
// back older rows or another selection. A rename or a move shows here at
// once, as a selection does, and the list the app sends after it carries
// the name or the order.
//
// The store also keeps the sessions this window opened, which the main
// window gives a terminal each. That is the session the first list
// selects, which launch started, and each session whose selection the
// app finished since, the one a banner click makes among them. A
// session launch restored reads its scrollback only as its first
// selection finishes, so its terminal waits for that. A session
// leaves the list as it closes.

interface Sessions {
  rows: SessionRow[];
  selected: number;
  /** The sessions this window opened, in the order it opened them. */
  opened: number[];
}

const store = createStore<Sessions>({ rows: [], selected: FIRST_SESSION, opened: [] });
let started = false;
/** Whether a list came yet. */
let listed = false;
/** Counts each list heard and each selection, rename and move made
 *  here. */
let generation = 0;

/** Take the rows of a list and the selection it marks. */
function take(rows: SessionRow[]): void {
  const now = store.get();
  const selected = rows.find((row) => row.selected)?.id ?? now.selected;
  const kept = (listed ? now.opened : [selected]).filter((id) => rows.some((row) => row.id === id));
  listed = true;
  const opened =
    kept.length === now.opened.length && kept.every((id, i) => id === now.opened[i])
      ? now.opened
      : kept;
  store.set({ rows, selected, opened });
}

/** The app finished selecting `id`, so this window may open it. */
function opens(id: number): void {
  const now = store.get();
  if (!now.opened.includes(id)) store.set({ ...now, opened: [...now.opened, id] });
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
    onSessionSelected((session) => {
      opens(session);
      read();
    }),
  ];
  // Every listener is in first, so a list that lands meanwhile is either
  // in the answer or newer than it.
  void Promise.all(listening)
    .then(read)
    .catch(() => undefined);
}

/** Select a session. Every view here shows it at once, and the app
 *  hears it after. The window opens it once the app finished, which the
 *  answer waits for. A selection the app refuses reads the list again. */
export function select(id: number): Promise<void> {
  generation += 1;
  const now = store.get();
  if (now.selected !== id) store.set({ ...now, selected: id });
  return selectSession(id).then(
    () => opens(id),
    () => read(),
  );
}

/** Give `id` the name `name`, or none, so it reads its character again.
 *  Every view here shows it at once, and the app keeps it after and
 *  sends the rows. A rename the app refuses reads the list again and
 *  says so. */
export function rename(id: number, name: string | null): Promise<void> {
  generation += 1;
  const now = store.get();
  store.set({ ...now, rows: now.rows.map((row) => (row.id === id ? { ...row, name } : row)) });
  return renameSession(id, name).catch((e: unknown) => {
    read();
    pushToast({ kind: 'error', message: errorText(e) || 'Vosh could not rename the session.' });
  });
}

/** Move `id` to the place `to` among the other rows, as a drag of its
 *  row does. Every view here shows the new order at once, so ⌘1 to ⌘9
 *  follow it, and the app keeps it after and sends the rows. A move the
 *  app refuses reads the list again and says so. */
export function move(id: number, to: number): Promise<void> {
  generation += 1;
  const now = store.get();
  const moved = now.rows.find((row) => row.id === id);
  if (moved) {
    const rows = now.rows.filter((row) => row !== moved);
    rows.splice(Math.min(to, rows.length), 0, moved);
    store.set({ ...now, rows });
  }
  return moveSession(id, to).catch((e: unknown) => {
    read();
    pushToast({ kind: 'error', message: errorText(e) || 'Vosh could not move the session.' });
  });
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

/** The profile the selected session plays, the one in front, or null
 *  before a list names it. */
export function profileInFront(): string | null {
  return selectedRow()?.profile ?? null;
}

/** Whether a session plays `profile`, which keeps it open. True before
 *  the first list, and for a profile left out, which names the selected
 *  session's. */
export function playsProfile(profile: string | undefined): boolean {
  const { rows } = store.get();
  return profile === undefined || rows.length === 0 || rows.some((row) => row.profile === profile);
}

/** Every other session that plays the profile `session` plays. They
 *  share its panel and its font, so the size of its pane. */
export function othersOnProfile(session: number): number[] {
  const { rows } = store.get();
  const profile = rows.find((row) => row.id === session)?.profile;
  if (profile == null) return [];
  return rows.filter((row) => row.id !== session && row.profile === profile).map((row) => row.id);
}

/** The sessions this window opened, in the order it opened them. */
export function getOpened(): number[] {
  return store.get().opened;
}

/** The session `step` rows from the selected one, going round the ends,
 *  or null while fewer than two are open. */
export function sessionStep(step: 1 | -1): number | null {
  const { rows, selected } = store.get();
  if (rows.length < 2) return null;
  const at = rows.findIndex((row) => row.id === selected);
  return rows[(at + step + rows.length) % rows.length].id;
}

/** The session at `place` in the list, counting from 1, or null. */
export function sessionAt(place: number): number | null {
  return store.get().rows[place - 1]?.id ?? null;
}

/** Bring `session` to the front, unless it is there already or there is
 *  none. */
export function goTo(session: number | null): void {
  if (session !== null && session !== getSelected()) void select(session);
}

function selectedRow(): SessionRow | null {
  const { rows, selected } = store.get();
  return rows.find((row) => row.id === selected) ?? null;
}

// The title band names the selected session among the rows, so these
// two also answer a band drawn as static markup, as its tests draw it.

/** Every session's row, in the order the sidebar lists them. */
export function useSessions(): SessionRow[] {
  return useSyncExternalStore(subscribeSessions, getSessions, getSessions);
}

/** The selected session's id, which a view keys what it shows by. */
export function useSelected(): number {
  return useSyncExternalStore(subscribeSessions, getSelected, getSelected);
}

/** The selected session's row, or null until a list names it. */
export function useSelectedRow(): SessionRow | null {
  return useSyncExternalStore(subscribeSessions, selectedRow);
}

/** The sessions this window opened, in the order it opened them. Empty
 *  until the first list comes. */
export function useOpened(): number[] {
  return useSyncExternalStore(subscribeSessions, getOpened);
}

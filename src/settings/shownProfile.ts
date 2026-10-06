import { useSyncExternalStore } from 'react';
import { createStore } from '../stores/store';
import { getSelected, getSessions, subscribeSessions } from '../stores/session/sessionsStore';

// The profile Settings shows and edits, by Q14 and board 7 of the
// Sessions review. Settings shows the profile the selected session
// plays and follows the selection to another profile.

export interface Shown {
  /** The profile Settings shows, null until the session list names one. */
  profile: string | null;
  /** The session the header names, the selected one while it plays
   *  `profile`, null until the list names one. */
  session: number | null;
}

const store = createStore<Shown>({ profile: null, session: null });
let started = false;

/** Follow the selected session and the profile it plays. */
function settle(): void {
  const selected = getSelected();
  const row = getSessions().find((r) => r.id === selected);
  if (!row?.profile) return;
  const now = store.get();
  if (now.profile === row.profile && now.session === selected) return;
  store.set({ profile: row.profile, session: selected });
}

function start(): void {
  if (started) return;
  started = true;
  subscribeSessions(settle);
  settle();
}

/** What Settings shows now. */
export function getShown(): Shown {
  start();
  return store.get();
}

function subscribe(cb: () => void): () => void {
  start();
  return store.subscribe(cb);
}

/** What Settings shows, for a view that draws it. */
export function useShown(): Shown {
  return useSyncExternalStore(subscribe, getShown, getShown);
}

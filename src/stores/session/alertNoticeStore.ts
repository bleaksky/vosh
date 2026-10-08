import { useSyncExternalStore } from 'react';
import { onAlert, onAlertsEnded } from '../../ipc/alerts';
import { createStore } from '../store';
import {
  getSelected,
  getSessions,
  goTo,
  subscribeSelected,
  subscribeSessions,
} from './sessionsStore';

// The corner notice of an alert from a session you are not looking at,
// board 5 of the Sessions review (Q10). While Vosh is in front, Rust
// sends such an alert with `notice` on in place of a banner, and the
// main window shows it with Close and Show. Show selects its session,
// and Close clears the notice alone, so the session's row keeps its dot
// and its count until you look there. The window
// keeps one notice, the newest alert's. It goes once its session shows,
// by Show or any other way, once the Lua that raised it ends its alerts,
// and once its session leaves the list as it closes.

export interface AlertNotice {
  /** The session the alert rang in. */
  session: number;
  /** The alert's title, such as `Tell from Maren`. */
  title: string;
  /** The session as its row reads. */
  label: string | null;
  /** The owner tag of the Lua that raised it, or null for a trigger or a
   *  preset. */
  owner: string | null;
}

const store = createStore<AlertNotice | null>(null);
let started = false;

/** Clear the notice when `gone` says it no longer holds. */
function clearIf(gone: (notice: AlertNotice) => boolean): void {
  const notice = store.get();
  if (notice && gone(notice)) store.set(null);
}

/** Start hearing alerts. It runs once. */
export function startAlertNoticeStore(): void {
  if (started) return;
  started = true;
  void onAlert(({ notice, session, title, label, owner }) => {
    if (notice && session !== getSelected()) store.set({ session, title, label, owner });
  });
  void onAlertsEnded((session, owner) =>
    clearIf((notice) => notice.session === session && notice.owner === owner),
  );
  subscribeSelected(() => clearIf((notice) => notice.session === getSelected()));
  // An empty list is one that has not come yet.
  subscribeSessions(() => {
    const rows = getSessions();
    clearIf((notice) => rows.length > 0 && !rows.some((row) => row.id === notice.session));
  });
}

/** The notice the corner shows, or null. */
export const getAlertNotice = store.get;

/** The notice the corner shows, kept current. */
export function useAlertNotice(): AlertNotice | null {
  return useSyncExternalStore(store.subscribe, getAlertNotice, getAlertNotice);
}

/** Show the notice's session, which clears the notice. */
export function showAlertNotice(): void {
  goTo(getAlertNotice()?.session ?? null);
}

/** Clear the notice alone. The row of its session keeps its dot and its
 *  count, which wait until you look at that session. */
export function closeAlertNotice(): void {
  store.set(null);
}

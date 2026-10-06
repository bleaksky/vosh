// The events of the alerts that ring in your sessions, the page side of
// src-tauri/src/alert.rs.

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { ALERT, MARK } from './events';
import { sessionOf } from './session';

/** Hear each alert that rings, with the session it rang in and where
 *  it came from, `preset:<id>`, `trigger:<name>` or `lua:<owner>`. The
 *  payload is an `AlertPayload` in src-tauri/src/alert.rs. The sessions
 *  sidebar marks the row. */
export async function onAlert(cb: (session: number, source: string) => void): Promise<UnlistenFn> {
  return listen<{ session?: number; source: string }>(ALERT, (event) => {
    cb(sessionOf(event.payload), event.payload.source);
  });
}

/** Hear each alert that rang nothing in a session behind, an alert
 *  preset that is off or an alert held back, with that session and where
 *  the alert came from, as `onAlert` gives it (Sessions Q9). One comes
 *  for each such alert. The sessions sidebar marks the row as it does
 *  for an alert that rang. */
export async function onMark(cb: (session: number, source: string) => void): Promise<UnlistenFn> {
  return listen<{ session?: number; source: string }>(MARK, (event) => {
    cb(sessionOf(event.payload), event.payload.source);
  });
}

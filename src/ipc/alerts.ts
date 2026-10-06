// The events of the alerts that ring in your sessions, the page side of
// src-tauri/src/alert.rs.

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { ALERT } from './events';
import { sessionOf } from './session';

/** Hear each alert that rings, with the session it rang in. The payload
 *  is an `AlertPayload` in src-tauri/src/alert.rs. The sessions sidebar
 *  reads only the session, to mark its row. */
export async function onAlert(cb: (session: number) => void): Promise<UnlistenFn> {
  return listen<{ session?: number }>(ALERT, (event) => {
    cb(sessionOf(event.payload));
  });
}

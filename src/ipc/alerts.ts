// The alerts a session rings, from a trigger's alert, an alert preset or
// mud.alert in Lua.

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { ALERT } from './events';

/** An alert that rang, on session://alert. Mirrors AlertPayload in
 *  src-tauri/src/alert.rs. */
export interface AlertPayload {
  /** The session that rang it, as every session event names it. */
  session: number;
  /** The banner's title, such as `Tell from Tolliver`. */
  title: string;
  /** The session as its row reads, which the banner names under the
   *  title. */
  label: string | null;
  /** What was said or the line that matched, with Title and words on. */
  words: string | null;
  /** The tone the page plays, or null when the alert has none or Vosh
   *  played a system sound in its place. */
  sound: string | null;
  /** A system banner went out. */
  banner: boolean;
  /** Vosh is in front and you look at another session, so the page
   *  shows a notice of its own in place of a banner. */
  notice: boolean;
  /** Where it came from, `trigger:<name>`, `preset:<id>` or
   *  `lua:<owner>`. */
  source: string;
  /** The owner tag of the Lua that raised it, such as
   *  `plugin:vitals_alert`. */
  owner: string | null;
}

export async function onAlert(cb: (alert: AlertPayload) => void): Promise<UnlistenFn> {
  return listen<AlertPayload>(ALERT, (event) => {
    cb(event.payload);
  });
}

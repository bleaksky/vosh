import type { SessionRow } from '../ipc/session';
import { worldName } from '../lib/knownWorlds';
import { sessionLabel, type LabelSource } from '../lib/sessionLabel';
import { countWord, listJoin, possessive } from '../lib/text';

// The words Vosh asks with before it closes a session, the main window
// or the app, so a live session never ends by a slip. Close session
// asks while its session is connected, Close window while any session
// is, and Quit while two or more are. A session goes by its label, the
// name you gave it or else its character, with the world and the port
// where its row shows the port. With two or more sessions open, a
// question names each connected session and leaves the others out. Two
// read as the words below, and three or more read the same way, with
// the count and all three in place of both.

/** A session as a question reads it, its row and whether it is
 *  connected. */
export type CloseRow = LabelSource & Pick<SessionRow, 'connected'>;

export interface CloseQuestion {
  title: string;
  body: string;
  /** The button that closes, in the danger tone. */
  confirm: string;
}

/** What a close ends, `both` or `all three`. */
function allOf(n: number): string {
  return n === 2 ? 'both' : `all ${countWord(n).toLowerCase()}`;
}

/** Where a session plays, ` to The Forsaken Lands 1825`, or nothing
 *  before it has an address. */
function toPlace(place: string | null): string {
  return place ? ` to ${place}` : '';
}

/** A connected session as a list names it, `Orla on The Forsaken Lands
 *  1825`, or `one on The Forsaken Lands 1825` at the login. */
function named(row: CloseRow, rows: readonly CloseRow[]): string {
  const { who, place, name } = sessionLabel(row, rows);
  return place ? `${who ?? 'one'} on ${place}` : name;
}

/** `Two sessions are connected, Tolliver on The Forsaken Lands and Orla
 *  on The Forsaken Lands 1825.` */
function connectedSentence(live: readonly CloseRow[], rows: readonly CloseRow[]): string {
  const list = listJoin(live.map((row) => named(row, rows)));
  return `${countWord(live.length)} sessions are connected, ${list}.`;
}

/** What Close session asks before it closes `session` among the open
 *  `rows`, or null while the session is not connected, which closes at
 *  once. A session at the login, with no name or character yet, goes by
 *  the world it plays. */
export function closeSessionQuestion(
  session: number,
  rows: readonly CloseRow[],
): CloseQuestion | null {
  const row = rows.find((r) => r.id === session);
  if (!row?.connected) return null;
  const { who, place } = sessionLabel(row, rows);
  const to = toPlace(place);
  const confirm = 'Close session';
  if (!who) {
    return {
      title: 'Close this session?',
      body: `This session is connected${to}. Closing it ends the connection and removes the row.`,
      confirm,
    };
  }
  return {
    title: `Close ${possessive(who)} session?`,
    body: `${who} is connected${to}. Closing this session disconnects ${who} and removes the row.`,
    confirm,
  };
}

/** What Close window asks while a session is connected, or null while
 *  none is, which closes at once. With one session open it asks in the
 *  words it used before sessions. */
export function closeWindowQuestion(rows: readonly CloseRow[]): CloseQuestion | null {
  const live = rows.filter((row) => row.connected);
  if (live.length === 0) return null;
  const ask = (body: string) => ({ title: 'Close this window?', body, confirm: 'Close window' });
  if (rows.length === 1) {
    const { host } = rows[0];
    const to = toPlace(host === null ? null : worldName(host));
    return ask(`You are connected${to}. Closing this window ends your session and quits Vosh.`);
  }
  if (live.length === 1) {
    const { who, place } = sessionLabel(live[0], rows);
    const to = toPlace(place);
    return ask(
      who
        ? `${who} is connected${to}. Closing this window disconnects ${who} and quits Vosh.`
        : `A session is connected${to}. Closing this window ends it and quits Vosh.`,
    );
  }
  return ask(
    `${connectedSentence(live, rows)} Closing this window ends ${allOf(live.length)} and quits Vosh.`,
  );
}

/** What Quit asks while two or more sessions are connected, or null
 *  with fewer, which quits at once as Quit did before sessions. */
export function quitQuestion(rows: readonly CloseRow[]): CloseQuestion | null {
  const live = rows.filter((row) => row.connected);
  if (live.length < 2) return null;
  return {
    title: 'Quit Vosh?',
    body: `${connectedSentence(live, rows)} Quitting ends ${allOf(live.length)}.`,
    confirm: 'Quit',
  };
}

import type { SessionRow } from '../ipc/session';
import { worldName } from '../lib/knownWorlds';
import { sessionLabel, type LabelSource } from '../lib/sessionLabel';
import { possessive } from '../lib/text';

// The words Vosh asks with before it closes a session or the main
// window, by Q13 and board 6 of the Sessions review. Close session asks
// while its session is connected, and Close window while any session
// is. A session goes by its label, the name you gave it or else its
// character, with the world and the port where its row shows the port.

/** A session as a question reads it, its row and whether it is
 *  connected. */
export type CloseRow = LabelSource & Pick<SessionRow, 'connected'>;

export interface CloseQuestion {
  title: string;
  body: string;
  /** The button that closes, in the danger tone. */
  confirm: string;
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
  const to = place ? ` to ${place}` : '';
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
 *  none is, which closes at once. */
export function closeWindowQuestion(rows: readonly CloseRow[]): CloseQuestion | null {
  const live = rows.find((row) => row.connected);
  if (!live) return null;
  const to = live.host ? ` to ${worldName(live.host)}` : '';
  return {
    title: 'Close this window?',
    body: `You are connected${to}. Closing this window ends your session and quits Vosh.`,
    confirm: 'Close window',
  };
}

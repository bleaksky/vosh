// Whether you are selecting text or reading back in a session's xterm,
// which that session hears so a clock piece in your design waits to
// repaint its prompt while you do, so a repaint never pulls the text
// out from under you. Each session keeps its own, since its terminal
// keeps its selection and its scroll while another session shows. The
// native grid holds its own selection and scroll, which the session
// reads itself.

import { terminalReaderBusy } from '../ipc/terminal';

/** What can keep you busy with the text: a selection in the live pane
 *  or in the split's history pane, the live pane off its newest rows,
 *  and the split open for reading back. */
export type ReaderPart = 'liveSelection' | 'historySelection' | 'liveBack' | 'split';

export interface ReaderTracker {
  /** `part` started or stopped holding. */
  note: (part: ReaderPart, on: boolean) => void;
}

/** Track the parts and call `send` with whether any holds, each time
 *  that changes. The first note always sends, so a window that loads
 *  again sets the session straight. */
export function readerTracker(send: (busy: boolean) => void): ReaderTracker {
  const parts = new Set<ReaderPart>();
  let sent: boolean | null = null;
  return {
    note: (part, on) => {
      if (on) parts.add(part);
      else parts.delete(part);
      const busy = parts.size > 0;
      if (busy === sent) return;
      sent = busy;
      send(busy);
    },
  };
}

/** Each session's tracker, by session. */
const readers = new Map<number, ReaderTracker>();

/** Tell `session` that `part` started or stopped holding. */
export function noteReader(part: ReaderPart, on: boolean, session: number): void {
  let reader = readers.get(session);
  if (!reader) {
    reader = readerTracker((busy) => {
      terminalReaderBusy(busy, session).catch(() => {});
    });
    readers.set(session, reader);
  }
  reader.note(part, on);
}

import { onScreenReader, type ScreenReaderFeed } from '../../ipc/screenReader';
import { createSessionStore } from '../sessionStore';

// What each session gave the screen reader, for the hidden log the page
// reads from (R21 and R25 review, board 13, Q19 to Q21): the last 500
// lines that showed, newest last, and your latest prompt, which a key
// reads on demand. The session sends one event per read only while Read
// new game lines is on, and each folds into the state in one slice, so
// the history never grows past 500 lines whatever the game sends.
//
// A disconnect keeps what was read, so the lines before it stay in the
// log, and the state goes once the session leaves the list.

/** The most lines a session keeps for the reader, as many as one read
 *  sends at most (READ_LINES in src-tauri/src/session/reader.rs). */
export const READER_LINES = 500;

export interface ReaderState {
  /** The plain text of the last lines that showed, newest last. */
  lines: readonly string[];
  /** How many lines showed in all, the dropped ones included, so the
   *  log keys each line by its place in the whole run. */
  total: number;
  /** Your prompt's text, its lines joined, as the latest read that
   *  brought one left it. */
  prompt: string | null;
}

const EMPTY_READER: ReaderState = { lines: [], total: 0, prompt: null };

/** The state one read leaves: its lines after the ones kept, cut to the
 *  newest 500, and its prompt in place of the last when it brought
 *  one. */
export function foldReader(now: ReaderState, feed: ScreenReaderFeed): ReaderState {
  if (feed.lines.length === 0 && feed.prompt === null) return now;
  return {
    lines: feed.lines.length === 0 ? now.lines : [...now.lines, ...feed.lines].slice(-READER_LINES),
    total: now.total + feed.count,
    prompt: feed.prompt ?? now.prompt,
  };
}

const store = createSessionStore<ReaderState>({
  state: EMPTY_READER,
  connection: (state) => state,
  events: [
    (apply) => onScreenReader((feed, session) => apply(session, (now) => foldReader(now, feed))),
  ],
});

export const startReaderStore = store.start;
export const getReader = store.get;
export const useReader = store.use;

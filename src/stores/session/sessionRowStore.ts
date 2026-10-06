import { useSyncExternalStore } from 'react';
import { onAlert } from '../../ipc/alerts';
import { onGameLine } from '../../ipc/terminal';
import { createGmcpStore } from '../gmcp/gmcpStore';
import { getSelected, subscribeSelected } from './sessionsStore';

// What each session's row in the sessions sidebar says beyond its name,
// board 3 of the Sessions review. A session you are not looking at earns
// two marks (Q9). Its name brightens once the game prints a line there,
// and the accent dot shows once an alert rings there. Selecting the
// session clears both, and the selected row never takes either.

export interface SessionRowState {
  /** The game printed a line since you last looked. */
  lines: boolean;
  /** An alert rang since you last looked. */
  alert: boolean;
}

const QUIET: SessionRowState = { lines: false, alert: false };

/** Whether the session shows, so nothing marks it. */
const shown = (session: number) => session === getSelected();

/** Whether a line from the game would mark the session's row, so a
 *  write to one shown or marked already decodes nothing. */
function waitsForLines(session: number): boolean {
  return !shown(session) && !store.stateOf(session).lines;
}

const store = createGmcpStore<SessionRowState>({
  state: QUIET,
  // A link that comes or goes leaves the marks as they are.
  connection: (now) => now,
  events: [
    (apply) =>
      onGameLine(waitsForLines, (session) => apply(session, (now) => ({ ...now, lines: true }))),
    (apply) =>
      onAlert((session) => {
        if (!shown(session)) apply(session, (now) => (now.alert ? now : { ...now, alert: true }));
      }),
    (apply) =>
      subscribeSelected(() =>
        apply(getSelected(), (now) => (now.lines || now.alert ? { ...now, ...QUIET } : now)),
      ),
  ],
});

export const startSessionRowStore = store.start;

/** What the row of `session` says. A session nothing named yet reads
 *  quiet. */
export const getSessionRow = store.stateOf;

const subscribeRows = (cb: () => void) => store.subscribeStates(() => cb());

/** What the row of `session` says, which the sidebar draws. */
export function useSessionRow(session: number): SessionRowState {
  const get = () => getSessionRow(session);
  return useSyncExternalStore(subscribeRows, get, get);
}

/** The glyph a row shows at its right, in the meta's place. */
export type RowGlyph = 'dot';

export interface RowLook {
  glyph: RowGlyph | null;
  /** The name brightens for new lines. */
  tone: 'new' | null;
}

const PLAIN: RowLook = { glyph: null, tone: null };

/** How a row draws `state`. The selected row shows no mark, since you
 *  are looking at it. */
export function rowLook(state: SessionRowState, selected: boolean): RowLook {
  if (selected) return PLAIN;
  return { glyph: state.alert ? 'dot' : null, tone: state.lines ? 'new' : null };
}

import { useSyncExternalStore } from 'react';
import { onAlert, onMark } from '../../ipc/alerts';
import {
  onReconnect,
  type ReconnectPayload,
  type SessionRow,
  type StatePayload,
} from '../../ipc/session';
import { onGameLine } from '../../ipc/terminal';
import { knownWorld } from '../../lib/knownWorlds';
import { createSessionStore } from '../sessionStore';
import { getSelected, subscribeSelected } from './sessionsStore';

// What each session's row in the sessions sidebar says beyond its name,
// board 3 of the Sessions review.
//
// A row shows one glyph in the meta's place, the most urgent first
// (Q8). The triangle when the first dial fails, or when the link
// dropped and Vosh does not dial again. The hand while the game waits
// for your login. The spinner while Vosh dials, and through every try
// of a redial. The dot for an alert. A session that is not connected
// shows none, and its name dims.
//
// A session you are not looking at earns two marks (Q9). Its name
// brightens once the game prints a line there, and the dot shows once
// something for you happens there, the events the alert presets watch
// whether or not their alerts are on, and any alert a trigger or Lua
// raises. One that rings comes as session://alert, and one that rings
// nothing as session://mark. Selecting the session clears both, and
// the selected row never takes either.
//
// The link follows session://state and session://reconnect. Until an
// event of the session names it, a row reads the link from the session
// list, connected or not, and the login from the character it names.
// The list keeps naming the character a session played through a drop,
// every try of a redial and a disconnect, and forgets it at a connect
// you start, so a redial that reached the game shows the hand on a row
// that still names its character.

/** Where a session's link stands, as its events last said. `failed`
 *  needs you to connect again yourself. */
type Link = 'dialing' | 'live' | 'down' | 'failed';

export interface SessionRowState {
  /** Where the link stands, or null until an event of the session says,
   *  while the session list says. */
  link: Link | null;
  /** A series of redials runs, from its first wait to its end. */
  redialing: boolean;
  /** A redial reached the game, which waits for your login. */
  reached: boolean;
  /** You play on this link. Char.Status says so as you log in, and the
   *  vitals the game sends only in play say so after a redial took you
   *  back into a character left link dead, which sends no Char.Status
   *  (`check_reconnect` in comm.c). */
  playing: boolean;
  /** The game printed a line since you last looked. */
  lines: boolean;
  /** An alert rang since you last looked. */
  alert: boolean;
}

const QUIET: SessionRowState = {
  link: null,
  redialing: false,
  reached: false,
  playing: false,
  lines: false,
  alert: false,
};

/** `now` with `changes` laid over it, or `now` itself when nothing in
 *  it moves, so a row draws again only when it changes. */
function moved(now: SessionRowState, changes: Partial<SessionRowState>): SessionRowState {
  const keys = Object.keys(changes) as (keyof SessionRowState)[];
  return keys.every((key) => now[key] === changes[key]) ? now : { ...now, ...changes };
}

/** A connect starts a new link, which ends when the session says so. A
 *  first dial that fails needs you to connect again, while a redial's
 *  try that fails leaves the series running. */
function linked(now: SessionRowState, payload: StatePayload): SessionRowState {
  switch (payload.kind) {
    case 'connecting':
      return moved(now, { link: 'dialing', reached: false, playing: false });
    case 'connected':
      return moved(now, { link: 'live', playing: false });
    case 'disconnected': {
      const refused = !now.redialing && now.link === 'dialing' && !!payload.reason;
      return moved(now, { link: refused ? 'failed' : 'down', reached: false, playing: false });
    }
  }
}

/** The steps of a redial (Alerts Q13 and Sessions Q8). */
function redialed(now: SessionRowState, payload: ReconnectPayload): SessionRowState {
  switch (payload.kind) {
    case 'waiting':
    case 'dialing':
    case 'failed':
      return moved(now, { redialing: true });
    case 'reached':
      return moved(now, { redialing: false, reached: true, link: 'live' });
    case 'stopped':
      return moved(now, { redialing: false, link: 'failed' });
    case 'declined':
      return moved(now, { redialing: false, link: payload.why === 'off' ? 'failed' : 'down' });
    case 'cancelled':
      // A cancel can cut a try short as it dials, so no state follows.
      return moved(now, { redialing: false, link: now.link === 'live' ? 'live' : 'down' });
  }
}

const playing = (now: SessionRowState) => moved(now, { playing: true, reached: false });

/** Whether the session shows, so nothing marks it. */
const shown = (session: number) => session === getSelected();

/** Applies a change to the row of the session it names. */
type Apply = (session: number, change: (now: SessionRowState) => SessionRowState) => void;

/** Put the dot on the row of `session`, unless it shows. */
function mark(apply: Apply, session: number): void {
  if (!shown(session)) apply(session, (now) => moved(now, { alert: true }));
}

/** Whether a line from the game would mark the session's row, so a
 *  write to one shown or marked already decodes nothing. */
function waitsForLines(session: number): boolean {
  return !shown(session) && !store.stateOf(session).lines;
}

const store = createSessionStore<SessionRowState>({
  state: QUIET,
  packages: { 'Char.Status': playing, 'Char.Vitals': playing },
  connection: linked,
  events: [
    (apply) => onReconnect((payload, session) => apply(session, (now) => redialed(now, payload))),
    (apply) =>
      onGameLine(waitsForLines, (session) => apply(session, (now) => moved(now, { lines: true }))),
    (apply) => onAlert((session) => mark(apply, session)),
    (apply) => onMark((session) => mark(apply, session)),
    (apply) =>
      subscribeSelected(() =>
        apply(getSelected(), (now) => moved(now, { lines: false, alert: false })),
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
export type RowGlyph = 'triangle' | 'hand' | 'spinner' | 'dot';

export interface RowLook {
  glyph: RowGlyph | null;
  /** The name brightens for new lines, and dims while the session is
   *  not connected. */
  tone: 'new' | 'off' | null;
}

/** How the row of `row` draws `state`. One glyph, the most urgent: the
 *  triangle, then the hand, then the spinner, then the dot. The hand
 *  shows on a world Vosh knows sends Char.Status, or after a redial
 *  reached any world, since nothing else says when you logged in. The
 *  selected row shows no mark, since you are looking at it. */
export function rowLook(state: SessionRowState, row: SessionRow, selected: boolean): RowLook {
  const link = state.link ?? (row.connected ? 'live' : 'down');
  const busy = state.redialing || link === 'dialing';
  const knows = state.reached || (row.host !== null && knownWorld(row.host) !== undefined);
  const login =
    link === 'live' && !state.playing && (state.reached || row.character === null) && knows;
  const glyph: RowGlyph | null =
    link === 'failed'
      ? 'triangle'
      : login
        ? 'hand'
        : busy
          ? 'spinner'
          : state.alert && !selected
            ? 'dot'
            : null;
  if (selected) return { glyph, tone: null };
  const tone = link === 'down' && !busy ? 'off' : state.lines ? 'new' : null;
  return { glyph, tone };
}

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
// Every row shows one status mark, the selected one too, the most
// urgent first (Q8, and S2 of the Sessions Sidebar review). The
// triangle when the first dial fails, or when the link dropped and Vosh
// does not dial again. The hand while the game waits for your login.
// The spinner while Vosh dials, and through every try of a redial. The
// green dot while it plays, and the ring while it is not connected,
// when its name dims too.
//
// A session you are not looking at earns two marks (Q9). Its name
// brightens once the game prints a line there, and a count shows once
// something for you happens there. The row keeps each such thing as
// what waits for you (S4): a tell, your
// name, a fight that starts on you and low health, the events four of
// the alert presets watch whether or not their alerts are on. One that
// rings comes as session://alert, and one that rings nothing as
// session://mark, each with its source. Each tell, name and fight counts,
// and low health counts once however often it falls, since it is a
// state. The connection preset never counts, since a session in trouble
// shows it in its mark, and neither does an alert of a trigger or Lua.
// Selecting the session clears both marks, and the selected row never
// takes either.
//
// The link follows session://state and session://reconnect. Until an
// event of the session names it, a row reads the link from the session
// list, connected or not, and the login from the character it names.
// The list keeps naming the character a session played through a drop,
// every try of a redial and a disconnect, and forgets it at a connect
// you start, so a redial that reached the game shows the hand on a row
// that still names its character.
//
// The row also keeps when its link went down, for how long ago it
// dropped, whether the first dial of a connect never reached the game,
// and the try a redial is on out of how many.

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
  /** The source of each thing that waits for you, such as
   *  `preset:alert_tells`, in the order they came since you last
   *  looked. */
  waiting: readonly string[];
  /** When a drop or a failed dial took the link down, by Date.now, kept
   *  through a redial until a connect. */
  downAt: number | null;
  /** The link went down at the first dial of a connect, which never
   *  reached the game, rather than as a link dropped. */
  refused: boolean;
  /** The try a redial is on and how many it has, while it waits, dials
   *  or just failed one. */
  try: number | null;
  tries: number | null;
}

const NOTHING: readonly string[] = [];

const QUIET: SessionRowState = {
  link: null,
  redialing: false,
  reached: false,
  playing: false,
  lines: false,
  waiting: NOTHING,
  downAt: null,
  refused: false,
  try: null,
  tries: null,
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
      return moved(now, { link: 'live', playing: false, downAt: null });
    case 'disconnected': {
      const refused = !now.redialing && now.link === 'dialing' && !!payload.reason;
      // A drop or a refusal gives a reason, and your Disconnect none.
      const downAt = now.downAt ?? (payload.reason ? Date.now() : null);
      return moved(now, {
        link: refused ? 'failed' : 'down',
        reached: false,
        playing: false,
        downAt,
        refused,
      });
    }
  }
}

/** A redial series that ended. */
const ENDED = { redialing: false, try: null, tries: null } as const;

/** The steps of a redial (Alerts Q13 and Sessions Q8). */
function redialed(now: SessionRowState, payload: ReconnectPayload): SessionRowState {
  switch (payload.kind) {
    case 'waiting':
    case 'dialing':
    case 'failed':
      return moved(now, { redialing: true, try: payload.try, tries: payload.tries });
    case 'reached':
      return moved(now, { ...ENDED, reached: true, link: 'live' });
    case 'stopped':
      return moved(now, { ...ENDED, link: 'failed' });
    case 'declined':
      return moved(now, { ...ENDED, link: payload.why === 'off' ? 'failed' : 'down' });
    case 'cancelled':
      // A cancel can cut a try short as it dials, so no state follows.
      return moved(now, { ...ENDED, link: now.link === 'live' ? 'live' : 'down' });
  }
}

const playing = (now: SessionRowState) => moved(now, { playing: true, reached: false });

/** Whether the session shows, so nothing marks it. */
const shown = (session: number) => session === getSelected();

/** Applies a change to the row of the session it names. */
type Apply = (session: number, change: (now: SessionRowState) => SessionRowState) => void;

const TELLS = 'preset:alert_tells';
const NAME = 'preset:alert_name';
const ATTACKED = 'preset:alert_attacked';
const LOW_HEALTH = 'preset:alert_low_health';

/** The sources that count as waiting for you. */
const COUNTED = new Set([TELLS, NAME, ATTACKED, LOW_HEALTH]);

/** Add what `source` says waits on the row of `session`, unless the
 *  session shows or the source does not count. */
function mark(apply: Apply, session: number, source: string): void {
  if (shown(session) || !COUNTED.has(source)) return;
  apply(session, (now) =>
    source === LOW_HEALTH && now.waiting.includes(LOW_HEALTH)
      ? now
      : { ...now, waiting: [...now.waiting, source] },
  );
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
    (apply) => onAlert((session, source) => mark(apply, session, source)),
    (apply) => onMark((session, source) => mark(apply, session, source)),
    (apply) =>
      subscribeSelected(() =>
        apply(getSelected(), (now) => moved(now, { lines: false, waiting: NOTHING })),
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

/** How many of `source` wait in `waiting`. */
const many = (waiting: readonly string[], source: string) =>
  waiting.filter((heard) => heard === source).length;

/** What waits for you in plain words, for the hover card, such as A
 *  fight started, health is low. Null when nothing waits. */
export function waitingWords(waiting: readonly string[]): string | null {
  const tells = many(waiting, TELLS);
  const names = many(waiting, NAME);
  const fights = many(waiting, ATTACKED);
  const words = [
    tells === 1 ? 'a tell' : tells > 1 ? `${tells} tells` : null,
    names === 1 ? 'your name came up' : names > 1 ? `your name came up ${names} times` : null,
    fights === 1 ? 'a fight started' : fights > 1 ? `${fights} fights started` : null,
    waiting.includes(LOW_HEALTH) ? 'health is low' : null,
  ]
    .filter((part) => part !== null)
    .join(', ');
  return words ? words[0].toUpperCase() + words.slice(1) : null;
}

/** How many things wait for you in every session of `rows` but the
 *  selected one, for the session button in the title band. */
export function waitingElsewhere(rows: readonly SessionRow[], selected: number): number {
  return rows.reduce(
    (sum, row) => (row.id === selected ? sum : sum + getSessionRow(row.id).waiting.length),
    0,
  );
}

/** `waitingElsewhere`, kept current as the rows change. */
export function useWaitingElsewhere(rows: readonly SessionRow[], selected: number): number {
  const get = () => waitingElsewhere(rows, selected);
  return useSyncExternalStore(subscribeRows, get, get);
}

/** The status mark at the left of a row. */
export type RowMark = 'live' | 'off' | 'hand' | 'spinner' | 'triangle';

/** What a screen reader says for each mark, in the title band's words,
 *  where the triangle means connect again yourself. */
export const MARK_WORDS: Record<RowMark, string> = {
  live: 'Playing',
  off: 'Not connected',
  hand: 'Logging in',
  spinner: 'Connecting',
  triangle: 'Connect again',
};

export interface RowLook {
  mark: RowMark;
  /** How many things wait for you there. */
  count: number;
  /** The name brightens for new lines, and dims while the session is
   *  not connected. */
  tone: 'new' | 'off' | null;
}

/** How the row of `row` draws `state`. One mark, the most urgent: the
 *  triangle, then the hand, then the spinner, then the dot or the ring.
 *  The hand shows on a world Vosh knows sends Char.Status, or after a
 *  redial reached any world, since nothing else says when you logged
 *  in. The selected row takes no tone, since you are looking at it. */
export function rowLook(state: SessionRowState, row: SessionRow, selected: boolean): RowLook {
  const link = state.link ?? (row.connected ? 'live' : 'down');
  const busy = state.redialing || link === 'dialing';
  const knows = state.reached || (row.host !== null && knownWorld(row.host) !== undefined);
  const login =
    link === 'live' && !state.playing && (state.reached || row.character === null) && knows;
  const mark: RowMark =
    link === 'failed'
      ? 'triangle'
      : login
        ? 'hand'
        : busy
          ? 'spinner'
          : link === 'live'
            ? 'live'
            : 'off';
  const count = state.waiting.length;
  if (selected) return { mark, count, tone: null };
  return { mark, count, tone: mark === 'off' ? 'off' : state.lines ? 'new' : null };
}

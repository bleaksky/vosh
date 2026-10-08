import { useMemo, useSyncExternalStore } from 'react';
import type { SessionRow } from '../ipc/session';
import { worldName } from '../lib/knownWorlds';
import { getCombatOf, subscribeCombatOf, type CombatOpponent } from '../stores/gmcp/combatStore';
import { getRoomOf, subscribeRoomOf, type RoomInfoBase } from '../stores/gmcp/roomStore';
import {
  getVitalsOf,
  subscribeVitalsOf,
  vitalPercent,
  type Vitals,
} from '../stores/gmcp/vitalsStore';
import { rowLook, useSessionRow, type SessionRowState } from '../stores/session/sessionRowStore';

// The second line of a session's row. While you play it reads the room
// from Room.Info, or who you fight from Char.Combat while the fight
// lasts, with your health as a whole percent at the right. The health
// takes the danger tone on the same latch the Low health alert rings
// on, and shows nothing while the game hides your vitals. Otherwise the
// line says what happened in plain words, following the row's mark: the
// game waits for your login, Vosh dials or dials again, the link
// dropped or never reached the game, or, while the session is not
// connected, where it would dial. A session you named starts the line
// with its character, so you never lose who plays it.
//
// Room, fight and vitals come from the GMCP stores, which keep a state
// for every session behind, each read through the view its panes read.

export interface SessionLine {
  /** The character of a session you named, which starts the line. */
  who: string | null;
  /** What the line says, null for a session with nowhere to dial. */
  text: string | null;
  /** Your health as a whole percent, null when the line shows none. */
  health: number | null;
  /** The health is low, on the Low health alert's latch. */
  low: boolean;
}

/** How long `ms` lasts in short words, like `4 min` or `1 h 12 min`,
 *  null under a minute. */
export function howLong(ms: number): string | null {
  const minutes = Math.floor(ms / 60_000);
  if (minutes < 1) return null;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  if (hours === 0) return `${rest} min`;
  return rest === 0 ? `${hours} h` : `${hours} h ${rest} min`;
}

/** What the line says while the session is not playing, by its mark,
 *  or nothing for a session that plays with no room heard yet. */
function story(row: SessionRow, state: SessionRowState, now: number): string | null {
  switch (rowLook(state, row, false).mark) {
    case 'hand':
      return 'Waiting for your login';
    case 'spinner':
      return state.redialing && state.try !== null && state.tries !== null
        ? `Reconnecting, try ${state.try} of ${state.tries}`
        : 'Connecting…';
    case 'triangle': {
      if (state.refused) return 'Couldn’t connect';
      if (state.downAt === null) return 'Dropped';
      const ago = howLong(now - state.downAt);
      return ago ? `Dropped ${ago} ago` : 'Dropped just now';
    }
    case 'live':
      // Playing, before the first room this page heard, such as after
      // the page loads again while the link stays up. The world reads
      // only while the session is not connected.
      return null;
    case 'off':
      return row.host === null ? null : worldName(row.host);
  }
}

/** The second line of `row`, from its row state, the room, fight and
 *  vitals of its session, and the time now by Date.now. */
export function secondLine(
  row: SessionRow,
  state: SessionRowState,
  room: RoomInfoBase | null,
  combat: CombatOpponent | null,
  vitals: Vitals | null,
  now: number,
): SessionLine {
  const named = row.name?.trim() || null;
  const character = row.character?.trim() || null;
  const who = named && character && named !== character ? character : null;
  if (rowLook(state, row, false).mark !== 'live') {
    return { who, text: story(row, state, now), health: null, low: false };
  }
  const text = combat ? `Fighting ${combat.name}` : (room?.name ?? story(row, state, now));
  const shows = vitals !== null && !vitals.hidden && vitals.maxhp > 0;
  return {
    who,
    text,
    health: shows ? vitalPercent(vitals.hp, vitals.maxhp) : null,
    low: shows && vitals.low.hp,
  };
}

// One clock for every row, which moves once a minute while any row
// reads it, for how long ago a session dropped and how long it has been
// online.
let clock = Date.now();
const readers = new Set<() => void>();
let timer: ReturnType<typeof setInterval> | undefined;

function subscribeClock(cb: () => void): () => void {
  if (readers.size === 0) {
    clock = Date.now();
    timer = setInterval(() => {
      clock = Date.now();
      for (const reader of readers) reader();
    }, 60_000);
  }
  readers.add(cb);
  return () => {
    readers.delete(cb);
    if (readers.size === 0) clearInterval(timer);
  };
}

/** The time now by Date.now, which moves once a minute. */
export function useMinuteClock(): number {
  const get = () => clock;
  return useSyncExternalStore(subscribeClock, get, get);
}

/** A getter as both the snapshot and the server snapshot, so a row
 *  renders to markup too. */
const both = <T>(get: () => T): [() => T, () => T] => [get, get];

/** What a session shows on its row and its card: its row state, room,
 *  fight and vitals, and the minute clock. */
export interface SessionView {
  state: SessionRowState;
  room: RoomInfoBase | null;
  combat: CombatOpponent | null;
  vitals: Vitals | null;
  now: number;
}

/** What `session` shows, which follows it as it plays. */
export function useSessionView(session: number): SessionView {
  const state = useSessionRow(session);
  const room = useSyncExternalStore(subscribeRoomOf, ...both(() => getRoomOf(session)));
  const combat = useSyncExternalStore(subscribeCombatOf, ...both(() => getCombatOf(session)));
  const vitals = useSyncExternalStore(subscribeVitalsOf, ...both(() => getVitalsOf(session)));
  const now = useMinuteClock();
  return useMemo(() => ({ state, room, combat, vitals, now }), [state, room, combat, vitals, now]);
}

/** The second line of `row`, which follows its session as it plays. */
export function useSessionLine(row: SessionRow): SessionLine {
  const { state, room, combat, vitals, now } = useSessionView(row.id);
  return useMemo(
    () => secondLine(row, state, room, combat, vitals, now),
    [row, state, room, combat, vitals, now],
  );
}

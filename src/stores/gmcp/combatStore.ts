import { createSessionStore } from '../sessionStore';
import { getHiddenOf, subscribeHiddenOf, type HiddenState } from './hiddenStore';
import { asNumber, asText, isHiddenFlag } from '../store';

// The opponent you are fighting, from Char.Combat. Aabahran sends
// `{target, condition, hp_pct}` on each prompt in a fight and `{}` when
// the fight ends. This is the server's view of the fight, separate
// from the client target in targetStore. Every pane reads one copy at
// module scope, so a pane that mounts mid fight shows the opponent at
// once.
//
// Whenever the text battle line would not print (under lamented tears,
// blind, against mirror image, or with the target in another room) the
// packet leaves out condition and hp_pct and adds `"hidden": true`. It
// adds `tank: {name, hp_pct}` while your opponent hits someone in your
// group, the prompt's %n and %p, and drops the tank's hp_pct under
// lamented tears. The prompt engine in the backend keeps its own copy
// to draw the tank in your prompt, so no pane shows the tank yet.
//
// An older server build sends the opponent's health and condition
// under the song, with no flag. The backend works out that they are
// hidden, and hiddenStore's `opponent` and `tank` hide them here the
// same way the flag and a tank without health do.

/** The groupmate your opponent hits. */
export interface CombatTank {
  name: string;
  /** Tank health, whole percent 0..100. null when the game withholds
   *  it. */
  hp_pct: number | null;
}

export interface CombatOpponent {
  name: string;
  /** Opponent health, whole percent 0..100. null when not sent. */
  hp_pct: number | null;
  /** The server's wording, like "big nasty wounds". */
  condition: string | null;
  /** The game withholds the opponent's health and condition. Both read
   *  null, and every view shows the health as hidden. */
  hidden: boolean;
  /** The groupmate your opponent hits, or null when it hits no one in
   *  your group. */
  tank: CombatTank | null;
}

function percent(value: unknown): number | null {
  const n = asNumber(value);
  return n === null ? null : Math.max(0, Math.min(100, Math.round(n)));
}

function parseTank(value: unknown): CombatTank | null {
  if (!value || typeof value !== 'object') return null;
  const t = value as Record<string, unknown>;
  const name = asText(t.name);
  if (!name) return null;
  return { name, hp_pct: percent(t.hp_pct) };
}

/** Parse a Char.Combat payload. null when no fight is on. */
export function parseCombat(data: unknown): CombatOpponent | null {
  if (!data || typeof data !== 'object') return null;
  const obj = data as Record<string, unknown>;
  const name = asText(obj.target);
  if (!name) return null;
  const hidden = isHiddenFlag(data);
  return {
    name,
    hp_pct: hidden ? null : percent(obj.hp_pct),
    condition: hidden ? null : asText(obj.condition),
    hidden,
    tank: parseTank(obj.tank),
  };
}

function sameTank(a: CombatTank | null, b: CombatTank | null): boolean {
  if (a === null || b === null) return a === b;
  return a.name === b.name && a.hp_pct === b.hp_pct;
}

function sameOpponent(a: CombatOpponent | null, b: CombatOpponent | null): boolean {
  if (a === null || b === null) return a === b;
  return (
    a.name === b.name &&
    a.hp_pct === b.hp_pct &&
    a.condition === b.condition &&
    a.hidden === b.hidden &&
    sameTank(a.tank, b.tank)
  );
}

/** The fight with what the backend worked out laid over it. A hidden
 *  opponent loses its health and condition, and a hidden tank loses
 *  its health. Returns `opponent` itself when that hides nothing new. */
export function withHidden(
  opponent: CombatOpponent | null,
  hidden: HiddenState,
): CombatOpponent | null {
  if (opponent === null) return null;
  const hideOpponent = hidden.opponent && !opponent.hidden;
  const hideTank = hidden.tank && opponent.tank !== null && opponent.tank.hp_pct !== null;
  if (!hideOpponent && !hideTank) return opponent;
  const next: CombatOpponent = hideOpponent
    ? { ...opponent, hp_pct: null, condition: null, hidden: true }
    : { ...opponent };
  if (hideTank && opponent.tank) next.tank = { ...opponent.tank, hp_pct: null };
  return next;
}

// The state is the last fight as Char.Combat sent it, and the panes read
// it with the hidden state laid over it.
const store = createSessionStore<CombatOpponent | null>({
  state: null,
  packages: { 'Char.Combat': (_, data) => parseCombat(data) },
  // The view reads the session's flags in the hidden store, so each
  // report that moves them runs the view again.
  events: [(apply) => subscribeHiddenOf((session) => apply(session, (sent) => sent))],
  view: (sent, last, session) => {
    const next = withHidden(sent, getHiddenOf(session));
    // Char.Combat rides every prompt, so keep what the panes read when
    // it repeats.
    return last !== undefined && sameOpponent(last, next) ? last : next;
  },
});

export const startCombatStore = store.start;
export const getCombat = store.get;
export const subscribeCombat = store.subscribe;

/** The opponent you are fighting, or null out of combat. */
export const useCombat = store.use;
/** The fight of the session `session` names as Char.Combat sent it.
 *  Its hidden flags, which withHidden lays over it, hide only the
 *  opponent's health and condition. */
export const getCombatOf = store.stateOf;
/** Hear each change to a session's fight, with that session. */
export const subscribeCombatOf = store.subscribeStates;

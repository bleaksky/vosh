import { useSyncExternalStore } from 'react';
import { onPromptVars, type PromptVarsPayload } from '../../ipc/prompt';
import { createSessionStore } from '../sessionStore';
import { getSelected, subscribeSelected } from '../session/sessionsStore';
import { getHiddenOf, subscribeHiddenOf } from './hiddenStore';
import { asNumber, isHiddenFlag } from '../store';

/** Below this percent a vital enters the low state. */
const LEDGER_LOW_ENTER = 20;
/** A low vital leaves the state only once it climbs back to this
 *  percent, so regen straddling the line does not flicker. */
const LEDGER_LOW_EXIT = 25;

// Your hp, mana and moves for the pinned vitals and the compact status
// line. Char.Vitals is the base. Prompt vars that a trigger sets with
// mud.set_prompt_var (hp, maxhp, mana, maxmana, move, maxmove) win
// over it, so a prompt regex can drive the meters on a server without
// GMCP.
//
// Under lamented tears Aabahran sends Char.Vitals as zeros with
// `"hidden": true`. The snapshot is then hidden until a Char.Vitals
// without the flag arrives. Prompt vars never fill it in meanwhile,
// and nothing reads low.
//
// The prompt vars a hidden Char.Vitals finds, and any a capture sets
// while hidden, are held back after that too. A capture under the
// song reads the zeros the text prompt prints, and one from before it
// is out of date. Char.Vitals fills each held vital until the prompt
// sets that var to a new value. The capture stops when you go AFK or
// turn your prompt off, and the game still sends Char.Vitals, so a
// held var can stay held for a long time.
//
// An older server build sends true values under the song, with no
// flag. The backend works out that they are hidden and says so on
// session://hidden, and hiddenStore's `vitals` hides the snapshot the
// same way the flag does. Its values then read as zeros, so no view
// can show the true ones.
//
// The store also keeps the last 60 Char.Vitals the game showed, each
// with the time it came, for the Traces style. A hidden packet stays
// out of it, and a disconnect empties it, so it starts over at each
// login and never reaches the disk.

export interface VitalValues {
  hp: number;
  maxhp: number;
  mana: number;
  maxmana: number;
  move: number;
  maxmove: number;
}

export type VitalKey = 'hp' | 'mana' | 'move';

export interface Vitals extends VitalValues {
  /** Low state per vital. Enters under LEDGER_LOW_ENTER percent and
   *  leaves only at LEDGER_LOW_EXIT, so regen across the line does not
   *  flicker. A vital with no max is never low. */
  low: Record<VitalKey, boolean>;
  /** The game hides your vitals. The values are the zeros it sent,
   *  every view shows `?` in their place, and nothing is low. */
  hidden: boolean;
}

/** One Char.Vitals packet: its values and whether the game hides
 *  them. */
export interface VitalsPacket {
  values: VitalValues;
  hidden: boolean;
}

const KEYS: readonly (keyof VitalValues)[] = ['hp', 'maxhp', 'mana', 'maxmana', 'move', 'maxmove'];
const ZERO: VitalValues = { hp: 0, maxhp: 0, mana: 0, maxmana: 0, move: 0, maxmove: 0 };
const MAX_OF: Record<VitalKey, keyof VitalValues> = {
  hp: 'maxhp',
  mana: 'maxmana',
  move: 'maxmove',
};
const NOT_LOW: Record<VitalKey, boolean> = { hp: false, mana: false, move: false };

/** One Char.Vitals of the history and when it came, in ms since the
 *  epoch. */
export interface VitalSample {
  at: number;
  values: VitalValues;
}

/** How many Char.Vitals the history keeps, a minute or two of play. */
export const VITALS_HISTORY = 60;

const NO_HISTORY: readonly VitalSample[] = Object.freeze([]);

/** `history` with `values` at `at` added, the oldest let go past
 *  VITALS_HISTORY. */
export function nextHistory(
  history: readonly VitalSample[],
  values: VitalValues,
  at: number,
): readonly VitalSample[] {
  const next = [...history, { at, values }];
  return next.length > VITALS_HISTORY ? next.slice(next.length - VITALS_HISTORY) : next;
}

/** The history a vitals snapshot carries, as the backend kept it. */
export function parseHistory(raw: unknown): readonly VitalSample[] {
  if (!Array.isArray(raw)) return NO_HISTORY;
  return raw.slice(-VITALS_HISTORY).flatMap((item: unknown) => {
    const sample = item && typeof item === 'object' ? (item as Record<string, unknown>) : {};
    const at = asNumber(sample.at);
    return at === null ? [] : [{ at, values: parseVitals(sample.vitals) }];
  });
}

/** Parse a Char.Vitals payload. Missing fields read as 0. */
export function parseVitals(data: unknown): VitalValues {
  const d = data && typeof data === 'object' ? (data as Record<string, unknown>) : {};
  return {
    hp: asNumber(d.hp) ?? 0,
    maxhp: asNumber(d.maxhp) ?? 0,
    mana: asNumber(d.mana) ?? 0,
    maxmana: asNumber(d.maxmana) ?? 0,
    move: asNumber(d.move) ?? 0,
    maxmove: asNumber(d.maxmove) ?? 0,
  };
}

/** Parse a Char.Vitals packet with its hidden flag. */
export function parseVitalsPacket(data: unknown): VitalsPacket {
  return { values: parseVitals(data), hidden: isHiddenFlag(data) };
}

/** Lay prompt vars over the GMCP values. Null when neither source has
 *  given a max yet, which is the "Vitals appear when you log in"
 *  state. */
export function mergeVitals(
  gmcp: VitalValues | null,
  promptVars: PromptVarsPayload,
): VitalValues | null {
  const merged = {} as VitalValues;
  for (const key of KEYS) {
    const fromVar = asNumber(promptVars[key]);
    merged[key] = fromVar ?? gmcp?.[key] ?? 0;
  }
  if (gmcp === null && merged.maxhp === 0 && merged.maxmana === 0 && merged.maxmove === 0) {
    return null;
  }
  return merged;
}

/** The vital prompt vars and their values now. Taken when a hidden
 *  Char.Vitals arrives and on each capture while hidden. */
export function holdPromptVitals(vars: PromptVarsPayload): PromptVarsPayload {
  const held: PromptVarsPayload = {};
  for (const key of KEYS) {
    const value = vars[key];
    if (value !== undefined) held[key] = value;
  }
  return held;
}

/** Let go of each held var the prompt has since set to a new value.
 *  Returns `held` itself when nothing changed. */
export function releasePromptVitals(
  held: PromptVarsPayload,
  vars: PromptVarsPayload,
): PromptVarsPayload {
  const keys = Object.keys(held);
  const kept = keys.filter((key) => vars[key] === held[key]);
  if (kept.length === keys.length) return held;
  const next: PromptVarsPayload = {};
  for (const key of kept) next[key] = held[key];
  return next;
}

/** The prompt vars without the held ones. */
export function withoutHeld(vars: PromptVarsPayload, held: PromptVarsPayload): PromptVarsPayload {
  const keys = Object.keys(held);
  if (keys.length === 0) return vars;
  const out = { ...vars };
  for (const key of keys) delete out[key];
  return out;
}

/** Whole percent, clamped to 0..100. 0 when there is no max. */
export function vitalPercent(current: number, max: number): number {
  if (max <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((current / max) * 100)));
}

/** Next low latch for one vital given the last one. */
export function nextLow(wasLow: boolean, current: number, max: number): boolean {
  if (max <= 0) return false;
  const pct = vitalPercent(current, max);
  return wasLow ? pct < LEDGER_LOW_EXIT : pct < LEDGER_LOW_ENTER;
}

/** Build the next snapshot, reusing the previous object when nothing
 *  changed so a prompt that repeats the same numbers does not render.
 *  A hidden snapshot is never low, and the latch starts over once the
 *  game shows your vitals again. */
export function nextVitals(
  prev: Vitals | null,
  values: VitalValues | null,
  hidden = false,
): Vitals | null {
  if (values === null) return null;
  const was = prev?.low ?? NOT_LOW;
  const low: Record<VitalKey, boolean> = hidden
    ? NOT_LOW
    : {
        hp: nextLow(was.hp, values.hp, values[MAX_OF.hp]),
        mana: nextLow(was.mana, values.mana, values[MAX_OF.mana]),
        move: nextLow(was.move, values.move, values[MAX_OF.move]),
      };
  if (
    prev &&
    prev.hidden === hidden &&
    KEYS.every((k) => prev[k] === values[k]) &&
    low.hp === prev.low.hp &&
    low.mana === prev.low.mana &&
    low.move === prev.low.move
  ) {
    return prev;
  }
  return { ...values, low, hidden };
}

interface VitalsState {
  /** The last Char.Vitals. */
  packet: VitalsPacket | null;
  /** The prompt vars now. */
  vars: PromptVarsPayload;
  /** The vital prompt vars held back since the game hid your vitals. */
  held: PromptVarsPayload;
  /** The backend works out that the game hides your vitals. The store
   *  keeps its own copy of the session's `vitals` in hiddenStore, since
   *  the change that turns it on holds back the prompt vars. */
  hiddenByBackend: boolean;
  /** What the panes read. Each change works it out from the one before,
   *  so the low latch of each vital stays with the rest of the state. */
  shown: Vitals | null;
  /** The last Char.Vitals the game showed, oldest first. */
  history: readonly VitalSample[];
}

/** True while the game hides your vitals, by the packet's own flag or
 *  by what the backend worked out. */
function hiddenNow({ packet, hiddenByBackend }: VitalsState): boolean {
  return packet?.hidden === true || hiddenByBackend;
}

/** The state with what the panes read worked out again. Hidden vitals
 *  stand alone. Prompt vars and GMCP never fill them in. */
function show(state: VitalsState): VitalsState {
  const { packet, vars, held, shown: last } = state;
  const shown = hiddenNow(state)
    ? nextVitals(last, ZERO, true)
    : nextVitals(last, mergeVitals(packet?.values ?? null, withoutHeld(vars, held)));
  return shown === last ? state : { ...state, shown };
}

/** Nothing heard yet from a session, hidden as the backend says now.
 *  `last` is what the session's panes read before, which they keep when
 *  the empty state shows the same. */
function empty(session: number, last: Vitals | null = null): VitalsState {
  return show({
    packet: null,
    vars: {},
    held: {},
    hiddenByBackend: getHiddenOf(session).vitals,
    shown: last,
    history: NO_HISTORY,
  });
}

const store = createSessionStore<VitalsState, Vitals | null>({
  state: (session) => empty(session),
  packages: {
    'Char.Vitals': (state, data) => {
      const next = { ...state, packet: parseVitalsPacket(data) };
      if (hiddenNow(next)) return show({ ...next, held: holdPromptVitals(state.vars) });
      return show({ ...next, history: nextHistory(state.history, next.packet.values, Date.now()) });
    },
  },
  connection: (state, { kind, session }) =>
    kind === 'disconnected' ? empty(session, state.shown) : state,
  events: [
    (apply) =>
      onPromptVars((payload, session) =>
        apply(session, (state) => {
          const vars = payload && typeof payload === 'object' ? payload : {};
          const held = hiddenNow(state)
            ? holdPromptVitals(vars)
            : releasePromptVitals(state.held, vars);
          return show({ ...state, vars, held });
        }),
      ),
    (apply) =>
      subscribeHiddenOf((session) =>
        apply(session, (state) => {
          const hiddenByBackend = getHiddenOf(session).vitals;
          if (hiddenByBackend === state.hiddenByBackend) return state;
          const held = hiddenByBackend ? holdPromptVitals(state.vars) : state.held;
          return show({ ...state, hiddenByBackend, held });
        }),
      ),
  ],
  view: ({ shown }) => shown,
});

export const startVitalsStore = store.start;
export const getVitals = store.get;
export const useVitals = store.use;
/** The vitals of the session `session` names, as the panes read them. */
export const getVitalsOf = (session: number): Vitals | null => store.stateOf(session).shown;
/** Hear each change to a session's vitals, with that session. */
export const subscribeVitalsOf = store.subscribeStates;

/** The vitals history of the session `session` names, oldest first. */
export const getVitalsHistoryOf = (session: number): readonly VitalSample[] =>
  store.stateOf(session).history;

function subscribeHistory(cb: () => void): () => void {
  const offStates = store.subscribeStates(cb);
  const offSelected = subscribeSelected(cb);
  return () => {
    offStates();
    offSelected();
  };
}

/** The selected session's vitals history, oldest first. */
export function useVitalsHistory(): readonly VitalSample[] {
  return useSyncExternalStore(subscribeHistory, () => getVitalsHistoryOf(getSelected()));
}

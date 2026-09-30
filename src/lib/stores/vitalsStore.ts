import { useSyncExternalStore } from 'react';
import { onGmcpPackage, onPromptVars, onState, type PromptVarsPayload } from '../session';
import { LEDGER_LOW_ENTER, LEDGER_LOW_EXIT } from '../vitalsLayouts';
import { getHidden, subscribeHidden } from './hiddenStore';
import { asNumber, createStore, isHiddenFlag } from './store';

// Your hp, mana and moves for the pinned vitals and the compact status
// line. Char.Vitals is the base. Prompt vars that a trigger sets with
// mud.set_prompt_var (hp, maxhp, mana, maxmana, move, maxmove) win
// over it, so a prompt regex can drive the meters on a server without
// GMCP. Lifted from the VitalsBar data effect.
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

/** Parse a Char.Vitals payload. Missing fields read as 0, the way the
 *  VitalsBar did. */
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

const store = createStore<Vitals | null>(null);
let gmcp: VitalsPacket | null = null;
let promptVars: PromptVarsPayload = {};
let held: PromptVarsPayload = {};
let started = false;

/** True while the game hides your vitals, by the packet's own flag or
 *  by what the backend worked out. */
function hiddenNow(): boolean {
  return gmcp?.hidden === true || getHidden().vitals;
}

function publish(): void {
  // Hidden vitals stand alone. Prompt vars and GMCP never fill them in.
  const next = hiddenNow()
    ? nextVitals(store.get(), ZERO, true)
    : nextVitals(store.get(), mergeVitals(gmcp?.values ?? null, withoutHeld(promptVars, held)));
  store.set(next);
}

export function startVitalsStore(): void {
  if (started) return;
  started = true;
  void onGmcpPackage<unknown>('Char.Vitals', (data) => {
    gmcp = parseVitalsPacket(data);
    if (hiddenNow()) held = holdPromptVitals(promptVars);
    publish();
  });
  void onPromptVars((payload) => {
    promptVars = payload && typeof payload === 'object' ? payload : {};
    held = hiddenNow() ? holdPromptVitals(promptVars) : releasePromptVitals(held, promptVars);
    publish();
  });
  let hiddenByBackend = getHidden().vitals;
  subscribeHidden(() => {
    const now = getHidden().vitals;
    if (now === hiddenByBackend) return;
    hiddenByBackend = now;
    if (now) held = holdPromptVitals(promptVars);
    publish();
  });
  void onState((payload) => {
    if (payload.kind === 'disconnected') {
      gmcp = null;
      promptVars = {};
      held = {};
      store.set(null);
    }
  });
}

export function getVitals(): Vitals | null {
  return store.get();
}

export function subscribeVitals(cb: () => void): () => void {
  startVitalsStore();
  return store.subscribe(cb);
}

export function useVitals(): Vitals | null {
  return useSyncExternalStore(subscribeVitals, getVitals);
}

import { useSyncExternalStore } from 'react';
import { onGmcpPackage, onPromptVars, onState, type PromptVarsPayload } from '../session';
import { LEDGER_LOW_ENTER, LEDGER_LOW_EXIT } from '../vitalsLayouts';
import { asNumber, createStore } from './store';

// Your hp, mana and moves for the pinned vitals and the compact status
// line. Char.Vitals is the base. Prompt vars that a trigger sets with
// mud.set_prompt_var (hp, maxhp, mana, maxmana, move, maxmove) win
// over it, so a prompt regex can drive the meters on a server without
// GMCP. Lifted from the VitalsBar data effect.

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
}

const KEYS: readonly (keyof VitalValues)[] = ['hp', 'maxhp', 'mana', 'maxmana', 'move', 'maxmove'];
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
 *  changed so a prompt that repeats the same numbers does not render. */
export function nextVitals(prev: Vitals | null, values: VitalValues | null): Vitals | null {
  if (values === null) return null;
  const was = prev?.low ?? NOT_LOW;
  const low: Record<VitalKey, boolean> = {
    hp: nextLow(was.hp, values.hp, values[MAX_OF.hp]),
    mana: nextLow(was.mana, values.mana, values[MAX_OF.mana]),
    move: nextLow(was.move, values.move, values[MAX_OF.move]),
  };
  if (
    prev &&
    KEYS.every((k) => prev[k] === values[k]) &&
    low.hp === prev.low.hp &&
    low.mana === prev.low.mana &&
    low.move === prev.low.move
  ) {
    return prev;
  }
  return { ...values, low };
}

const store = createStore<Vitals | null>(null);
let gmcp: VitalValues | null = null;
let promptVars: PromptVarsPayload = {};
let started = false;

function publish(): void {
  store.set(nextVitals(store.get(), mergeVitals(gmcp, promptVars)));
}

export function startVitalsStore(): void {
  if (started) return;
  started = true;
  void onGmcpPackage<unknown>('Char.Vitals', (data) => {
    gmcp = parseVitals(data);
    publish();
  });
  void onPromptVars((payload) => {
    promptVars = payload && typeof payload === 'object' ? payload : {};
    publish();
  });
  void onState((payload) => {
    if (payload.kind === 'disconnected') {
      gmcp = null;
      promptVars = {};
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

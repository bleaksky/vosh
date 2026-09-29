import { useSyncExternalStore } from 'react';
import {
  onState,
  onTick,
  subscribeProfileSwitched,
  subscribeTickConfigChanged,
  tickGetConfig,
  type TickConfig,
  type TickPayload,
} from '../session';
import { createStore } from './store';

// Seconds to the next tick for the status line. The backend tick timer
// is the source. It resets on the World.Time hour change and on your
// reset pattern, and the session loop reports it on session://tick
// every 250 ms while it runs. useTickState counted up from its own
// guess of the last hour change and warned five seconds out no matter
// what you set. This store counts down from the backend's remaining
// time and warns at the warn_at_secs you set in the tick config.

/** Warn threshold when the tick config sets none. Matches the five
 *  seconds useTickState used. */
export const DEFAULT_TICK_WARN_SECS = 5;

/** The session loop reports four times a second. Longer than this
 *  without a report means the timer stopped (disabled with #tick, or
 *  the loop ended), so the count hides instead of freezing. */
const STALE_MS = 1500;

export interface TickState {
  /** The timer runs and the backend is reporting it. */
  active: boolean;
  /** Whole seconds until the next tick. null while inactive. */
  secsToTick: number | null;
  /** Warn at or under this many seconds. */
  warnAt: number;
  /** Active and at or under the threshold. */
  warn: boolean;
}

export function computeTick(payload: TickPayload | null, config: TickConfig | null): TickState {
  const warnAt =
    config?.warn_at_secs && config.warn_at_secs > 0 ? config.warn_at_secs : DEFAULT_TICK_WARN_SECS;
  const active = payload !== null && payload.enabled && config?.enabled !== false;
  if (!active) return { active: false, secsToTick: null, warnAt, warn: false };
  const secsToTick = Math.max(0, Math.ceil(payload.remaining_ms / 1000));
  return { active, secsToTick, warnAt, warn: secsToTick <= warnAt };
}

function sameTick(a: TickState, b: TickState): boolean {
  return (
    a.active === b.active &&
    a.secsToTick === b.secsToTick &&
    a.warnAt === b.warnAt &&
    a.warn === b.warn
  );
}

const store = createStore<TickState>(computeTick(null, null));
let payload: TickPayload | null = null;
let config: TickConfig | null = null;
let staleTimer: number | undefined;
let configGeneration = 0;
let started = false;

function publish(): void {
  const next = computeTick(payload, config);
  // Reports land four times a second. Publish only when the shown
  // number or state moves.
  if (!sameTick(store.get(), next)) store.set(next);
}

function refetchConfig(): void {
  const mine = ++configGeneration;
  tickGetConfig()
    .then((cfg) => {
      if (mine !== configGeneration) return;
      config = cfg;
      publish();
    })
    .catch(() => undefined);
}

export function startTickStore(): void {
  if (started) return;
  started = true;
  refetchConfig();
  void onTick((next) => {
    payload = next;
    window.clearTimeout(staleTimer);
    staleTimer = window.setTimeout(() => {
      payload = null;
      publish();
    }, STALE_MS);
    publish();
  });
  void subscribeTickConfigChanged((cfg) => {
    configGeneration += 1;
    config = cfg;
    publish();
  });
  // Tick config is per profile, and a switch does not broadcast it.
  void subscribeProfileSwitched(() => refetchConfig());
  void onState((state) => {
    if (state.kind === 'disconnected') {
      payload = null;
      window.clearTimeout(staleTimer);
      publish();
    }
  });
}

export function getTick(): TickState {
  return store.get();
}

export function subscribeTick(cb: () => void): () => void {
  startTickStore();
  return store.subscribe(cb);
}

export function useTick(): TickState {
  return useSyncExternalStore(subscribeTick, getTick);
}

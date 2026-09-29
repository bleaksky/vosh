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

// Seconds since the last tick for the status line. The backend tick
// timer is the source. It resets on the World.Time hour change and on
// your reset pattern, and the session loop reports it on session://tick
// every 250 ms while it runs, as the time left in the interval. This
// store counts up, the interval minus that time left, the way the old
// input row chip read, and warns in the last warn_at_secs you set in
// the tick config.

/** Warn threshold when the tick config sets none. Matches the five
 *  seconds the old chip used. */
export const DEFAULT_TICK_WARN_SECS = 5;

/** Interval when neither the report nor the config names one. The
 *  backend default. */
const DEFAULT_INTERVAL_MS = 30_000;

/** The session loop reports four times a second. Longer than this
 *  without a report means the timer stopped (disabled with #tick, or
 *  the loop ended), so the count hides instead of freezing. */
const STALE_MS = 1500;

export interface TickState {
  /** The timer runs and the backend is reporting it. */
  active: boolean;
  /** Whole seconds since the last tick. null while inactive. */
  secsSinceTick: number | null;
  /** Warn in the last this many seconds before the tick. */
  warnAt: number;
  /** Active and inside the warn window. */
  warn: boolean;
}

export function computeTick(payload: TickPayload | null, config: TickConfig | null): TickState {
  const warnAt =
    config?.warn_at_secs && config.warn_at_secs > 0 ? config.warn_at_secs : DEFAULT_TICK_WARN_SECS;
  const active = payload !== null && payload.enabled && config?.enabled !== false;
  if (!active) return { active: false, secsSinceTick: null, warnAt, warn: false };
  const intervalMs =
    payload.interval_ms > 0
      ? payload.interval_ms
      : config && config.interval_secs > 0
        ? config.interval_secs * 1000
        : DEFAULT_INTERVAL_MS;
  const remainingMs = Math.min(Math.max(0, payload.remaining_ms), intervalMs);
  const secsSinceTick = Math.floor((intervalMs - remainingMs) / 1000);
  // The warn window is the time left, so it holds the same last
  // seconds whatever the interval.
  const warn = Math.ceil(remainingMs / 1000) <= warnAt;
  return { active, secsSinceTick, warnAt, warn };
}

function sameTick(a: TickState, b: TickState): boolean {
  return (
    a.active === b.active &&
    a.secsSinceTick === b.secsSinceTick &&
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

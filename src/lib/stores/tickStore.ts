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

// The tick for the status line. The backend tick timer is the source.
// The game's own tick decides it, a World.Time hour change or a line
// that matches your Reset on pattern, and the session loop reports it
// on session://tick every 250 ms while it runs, with the time since the
// last tick. This store turns that into whole seconds since the tick,
// the whole seconds left until the expected one, and whether the game
// is running late, and warns in the last warn_at_secs you set in the
// tick config and all the while the tick is late. It passes the
// interval on as well, so the ring before the tick in the Icon style
// fills against it.

/** Warn threshold when the tick config sets none. Matches the five
 *  seconds the old chip used. */
export const DEFAULT_TICK_WARN_SECS = 5;

/** The session loop reports four times a second. Longer than this
 *  without a report means the timer stopped (disabled with #tick, or
 *  the loop ended), so the count hides instead of freezing. */
const STALE_MS = 1500;

export interface TickState {
  /** The timer runs and the backend is reporting it. */
  active: boolean;
  /** Whole seconds since the last tick. It keeps counting past the
   *  interval while the game runs late. null while inactive. */
  secsSinceTick: number | null;
  /** Whole seconds left until the expected tick, the interval minus the
   *  time since the tick rounded up. It reads the interval right after
   *  a tick, 1 in the last second, 0 at the expected tick, and below
   *  zero while the game runs late. Never minus zero. null while
   *  inactive or while the interval is unknown. */
  secsLeft: number | null;
  /** The interval the count runs against, in seconds, for the tick
   *  ring. The one you set under Every, as the backend reports it.
   *  null while inactive or unknown. */
  intervalSecs: number | null;
  /** Warn in the last this many seconds before the tick. */
  warnAt: number;
  /** Active and inside the warn window, or past the expected tick. */
  warn: boolean;
  /** The expected tick has come and the game's tick has not. */
  overdue: boolean;
  /** The game's own tick decides when the timer fires. */
  synced: boolean;
}

function inactive(warnAt: number): TickState {
  return {
    active: false,
    secsSinceTick: null,
    secsLeft: null,
    intervalSecs: null,
    warnAt,
    warn: false,
    overdue: false,
    synced: false,
  };
}

/** Whole seconds in `ms`, rounded up, with minus zero read as zero. */
function wholeSecsUp(ms: number): number {
  const secs = Math.ceil(ms / 1000);
  return secs === 0 ? 0 : secs;
}

export function computeTick(payload: TickPayload | null, config: TickConfig | null): TickState {
  const warnAt =
    config?.warn_at_secs && config.warn_at_secs > 0 ? config.warn_at_secs : DEFAULT_TICK_WARN_SECS;
  const active = payload !== null && payload.enabled && config?.enabled !== false;
  if (!active) return inactive(warnAt);
  const intervalMs =
    payload.interval_ms > 0
      ? payload.interval_ms
      : config && config.interval_secs > 0
        ? config.interval_secs * 1000
        : null;
  const elapsedMs = Number.isFinite(payload.elapsed_ms) ? Math.max(0, payload.elapsed_ms) : 0;
  const secsLeft = intervalMs === null ? null : wholeSecsUp(intervalMs - elapsedMs);
  return {
    active,
    secsSinceTick: Math.floor(elapsedMs / 1000),
    secsLeft,
    intervalSecs: intervalMs === null ? null : intervalMs / 1000,
    warnAt,
    // The warn window is the time left, so it holds the same last
    // seconds whatever the interval, and stays on while the tick is
    // late.
    warn: secsLeft !== null && secsLeft <= warnAt,
    overdue: intervalMs !== null && elapsedMs >= intervalMs,
    synced: payload.synced === true,
  };
}

function sameTick(a: TickState, b: TickState): boolean {
  return (
    a.active === b.active &&
    a.secsSinceTick === b.secsSinceTick &&
    a.secsLeft === b.secsLeft &&
    a.intervalSecs === b.intervalSecs &&
    a.warnAt === b.warnAt &&
    a.warn === b.warn &&
    a.overdue === b.overdue &&
    a.synced === b.synced
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

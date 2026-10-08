import { subscribeProfileSwitched } from '../../ipc/profiles';
import {
  onTick,
  subscribeTickConfigChanged,
  tickGetConfig,
  type TickConfig,
  type TickPayload,
} from '../../ipc/tick';
import { type TickCount } from '../../ipc/uiConfig';
import { createSessionStore } from '../sessionStore';
import { getSelected } from './sessionsStore';
import { playTickSound } from './tickSound';

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
//
// Each session runs its own count, so the store keeps the last
// report of each session and shows the selected one's. The tick settings
// are the profile's, and the store reads those of the profile in front,
// the selected session's, again on each vosh://profile-switched, which a
// selection across profiles sends too.

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
  // The report is the live timer. A connection starts the tick whatever
  // the profile saved, so the config read at launch can say off while
  // the tick runs.
  const active = payload !== null && payload.enabled;
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

/** The number the status line shows for the tick, counting `count`.
 *  Up is the whole seconds since the last tick, past the interval while
 *  the game runs late. Down is the whole seconds left until the expected
 *  tick, waiting at 0 until the tick lands. Down past 0 is the same
 *  count, going on below zero while the tick is late. Without a known
 *  interval there is nothing to count down from, so every way counts
 *  up, and `count` says which way it ran. Null while the tick hides. */
export function shownTick(
  state: TickState,
  count: TickCount,
): { secs: number; count: TickCount } | null {
  if (!state.active || state.secsSinceTick === null) return null;
  if (count === 'up' || state.secsLeft === null) return { secs: state.secsSinceTick, count: 'up' };
  if (count === 'down') return { secs: Math.max(0, state.secsLeft), count };
  return { secs: state.secsLeft, count };
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

/** The tick settings of the profile in front. */
let config: TickConfig | null = null;
let configGeneration = 0;
/** For each session, the timer that hides its count once its reports
 *  stop. */
const staleTimers = new Map<number, number>();

type Apply = (session: number, change: (now: TickPayload | null) => TickPayload | null) => void;

/** Read the settings of the profile in front, and show the selected
 *  session's count by them. A change that keeps its report still runs
 *  the view, which reads the settings again. */
function readConfig(apply: Apply): void {
  const mine = ++configGeneration;
  tickGetConfig()
    .then((cfg) => {
      if (mine !== configGeneration) return;
      config = cfg;
      apply(getSelected(), (now) => now);
    })
    .catch(() => undefined);
}

// Each session's state is its last report, or null while its count
// hides. A disconnect hides it, as the factory puts back null.
const store = createSessionStore<TickPayload | null, TickState>({
  state: null,
  events: [
    (apply) =>
      onTick((next, session) => {
        // The report that lands the tick, once per tick.
        if (next.fired && next.sound) playTickSound(session);
        window.clearTimeout(staleTimers.get(session));
        staleTimers.set(
          session,
          window.setTimeout(() => {
            staleTimers.delete(session);
            apply(session, () => null);
          }, STALE_MS),
        );
        apply(session, () => next);
      }),
    (apply) => {
      readConfig(apply);
      return subscribeTickConfigChanged((cfg) => {
        configGeneration += 1;
        config = cfg;
        // Turned off, the timer stops reporting. Hide the count now
        // rather than when the last report goes stale. A session behind
        // on the same profile hides its own once its reports stop.
        apply(getSelected(), (now) => (cfg.enabled ? now : null));
      });
    },
    // Tick config is per profile, and a switch does not broadcast it.
    (apply) => subscribeProfileSwitched(() => readConfig(apply)),
  ],
  // Reports land four times a second. Hand back what the status line
  // reads when the shown number and state did not move.
  view: (payload, last) => {
    const next = computeTick(payload, config);
    return last && sameTick(last, next) ? last : next;
  },
});

export const startTickStore = store.start;
export const getTick = store.get;
export const subscribeTick = store.subscribe;
export const useTick = store.use;

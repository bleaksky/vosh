import { useSyncExternalStore } from 'react';
import { onGmcpPackage, onState, onTick, type TickPayload } from './session';

// Phase 6 perf fix: tick state is held in a module-level singleton
// so every consumer (VitalsBar, VitalsTailRow, TemplateVitalsRow's
// TickSecondsToken leaf, InlineTick, TickPanel, StatusTimeChip)
// shares ONE `setInterval` and ONE set of session subscriptions
// instead of running its own copy of each. The previous per-hook
// version had four to five concurrent 250ms timers and four to five
// duplicate GMCP/onTick/onState subscriptions doing the same work.
//
// Consumers read through `useTickState()`, which is a thin
// `useSyncExternalStore` adapter over the singleton. The snapshot
// is cached and only rebuilt when one of its fields changes, so a
// 250ms tick that does not advance the seconds counter (e.g. three
// of every four wallclock fires) skips re-rendering entirely
// instead of dispatching a no-op `setState` per consumer.

export interface TickState {
  active: boolean;
  tickSecs: number;
  warn: boolean;
}

let tickPayload: TickPayload | null = null;
let tickSecs = 0;
let tickResetAt = Date.now();
let prevHour: number | string | null = null;
let started = false;
let snapshot: TickState = { active: false, tickSecs: 0, warn: false };

const subscribers = new Set<() => void>();

function recompute(): TickState {
  const active = Boolean(tickPayload?.enabled);
  const intervalSec =
    tickPayload?.interval_ms && tickPayload.interval_ms > 0
      ? Math.max(1, Math.round(tickPayload.interval_ms / 1000))
      : 30;
  const warn = active && tickSecs >= Math.max(0, intervalSec - 5);
  return { active, tickSecs, warn };
}

function notify(): void {
  const next = recompute();
  if (
    next.active === snapshot.active &&
    next.tickSecs === snapshot.tickSecs &&
    next.warn === snapshot.warn
  ) {
    return;
  }
  snapshot = next;
  for (const cb of subscribers) cb();
}

function ensureStarted(): void {
  if (started) return;
  started = true;
  // Single 250ms timer for the whole app. The poll cadence matches
  // the historical per-hook interval so the perceived UI smoothness
  // (countdown updates four times per second) is identical.
  window.setInterval(() => {
    const next = Math.floor((Date.now() - tickResetAt) / 1000);
    if (next === tickSecs) return;
    tickSecs = next;
    notify();
  }, 250);

  void onGmcpPackage<Record<string, unknown>>('World.Time', (data) => {
    if (!data || typeof data !== 'object') return;
    const hour = data.hour as number | string | undefined | null;
    if (hour === undefined || hour === null) return;
    if (prevHour !== null && prevHour !== hour) {
      tickResetAt = Date.now();
      tickSecs = 0;
      notify();
    }
    prevHour = hour;
  });

  void onTick((payload) => {
    tickPayload = payload;
    notify();
  });

  void onState((payload) => {
    if (payload.kind === 'disconnected') {
      tickPayload = null;
      prevHour = null;
      tickResetAt = Date.now();
      tickSecs = 0;
      notify();
    } else if (payload.kind === 'connected') {
      tickResetAt = Date.now();
      tickSecs = 0;
      notify();
    }
  });
}

function subscribe(cb: () => void): () => void {
  ensureStarted();
  subscribers.add(cb);
  return () => {
    subscribers.delete(cb);
  };
}

function getSnapshot(): TickState {
  return snapshot;
}

export function useTickState(): TickState {
  return useSyncExternalStore(subscribe, getSnapshot);
}

import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import {
  HIT_KEYS,
  hitView,
  nextStep,
  nextTrail,
  type HitKey,
  type HitViews,
  type Trail,
} from './vitalsHit';

// Keeps a Show each hit trail for each vital and your opponent while
// the switch is on (vitalsHit.ts), and wakes at each step of one so the
// footer draws the hold, the drain and the drop of the peak. A new
// opponent starts its trail over, so the first health of a fight is no
// hit.

/** Each mark's fill in percent, null for one the game hides or none. */
export type HitFills = Record<HitKey, number | null>;

type Trails = Partial<Record<HitKey, Trail>>;

export function useVitalsHits(fills: HitFills, on: boolean, foe: string | null): HitViews {
  const last = useRef<{ fills: HitFills | null; foe: string | null }>({ fills: null, foe: null });
  const [trails, setTrails] = useState<Trails>({});
  const [now, setNow] = useState(0);
  const { hp, mana, move, foe: foePct } = fills;

  useLayoutEffect(() => {
    const was = last.current;
    const fillsNow: HitFills = { hp, mana, move, foe: foePct };
    last.current = { fills: fillsNow, foe };
    if (!on) {
      setTrails((prev) => (Object.keys(prev).length === 0 ? prev : {}));
      return;
    }
    const at = Date.now();
    setTrails((prev) => {
      let next = prev;
      for (const key of HIT_KEYS) {
        const before = key === 'foe' && was.foe !== foe ? null : (was.fills?.[key] ?? null);
        const trail = nextTrail(prev[key] ?? null, before, fillsNow[key], at);
        if (trail === (prev[key] ?? null)) continue;
        next = { ...next };
        if (trail) next[key] = trail;
        else delete next[key];
      }
      return next;
    });
    setNow(at);
  }, [on, hp, mana, move, foePct, foe]);

  useEffect(() => {
    const steps = Object.values(trails).flatMap((trail) => nextStep(trail, now) ?? []);
    if (steps.length === 0) return;
    const wait = Math.max(0, Math.min(...steps) - Date.now());
    const timer = setTimeout(() => setNow(Date.now()), wait);
    return () => clearTimeout(timer);
  }, [trails, now]);

  const views: HitViews = {};
  for (const key of HIT_KEYS) {
    const trail = trails[key];
    const view = trail && hitView(trail, now);
    if (view) views[key] = view;
  }
  return views;
}

import type { Vital } from '../ipc/uiConfig';

// Show each hit (More Vitals Styles, Q21 and Q26). A hit leaves the
// part it took pale between the new fill and the old one for 600 ms,
// then the pale part drains toward the fill in 400 ms. Another hit
// while it holds extends the same trail. A heal draws the other way
// round: the part it gained shows pale first, and the fill follows
// 300 ms later. Ladders also holds the segment the vital stood at
// before the last hit lit for 1.5 s. Kept pure for the unit tests.
// useVitalsHits.ts keeps a trail for each vital and your opponent and
// wakes at each step, and panel.css draws the drain, which Reduce
// motion turns into a cut.

/** How long the part a hit took holds before it drains. */
export const HIT_HOLD = 600;
/** How long it drains. */
export const HIT_DRAIN = 400;
/** How long a heal's pale lead shows before the fill follows. */
export const HEAL_LEAD = 300;
/** How long a fill of the new styles takes to ease to its place. */
export const FILL_EASE = 160;
/** How long Ladders holds the peak lit. */
export const PEAK_HOLD = 1500;

/** What Show each hit follows: your vitals and your opponent's health. */
export type HitKey = Vital | 'foe';

export const HIT_KEYS: readonly HitKey[] = ['hp', 'mana', 'move', 'foe'];

/** One change and what it leaves, in percent of the max. */
export interface Trail {
  kind: 'hit' | 'heal';
  /** The fill before the change, or the top of a trail it extends. */
  from: number;
  /** The fill now. */
  to: number;
  /** When it began, in ms. */
  at: number;
  /** Where the vital stood before the last hit, and when that came. */
  peak: number | null;
  peakAt: number;
}

/** What a mark draws now: its fill, the far end of the pale part or
 *  null for none, whether the pale part drains toward the fill, which
 *  each mark draws in its own way, and the Ladders peak. */
export interface HitView {
  fill: number;
  ghost: number | null;
  draining: boolean;
  peak: number | null;
}

/** The peak `trail` still holds at `now`, or null once it dropped. */
function peakAt(trail: Trail | null, now: number): number | null {
  return trail && trail.peak !== null && now - trail.peakAt < PEAK_HOLD ? trail.peak : null;
}

/** The trail after a vital moves from `before` to `after` at `at`. A
 *  value the game hides, or none before, starts nothing over. */
export function nextTrail(
  trail: Trail | null,
  before: number | null,
  after: number | null,
  at: number,
): Trail | null {
  if (after === null || before === null) return null;
  if (after === before) return trail;
  const peak = peakAt(trail, at);
  if (after < before) {
    return {
      kind: 'hit',
      from: trail?.kind === 'hit' && at - trail.at < HIT_HOLD ? trail.from : before,
      to: after,
      at,
      peak: Math.max(before, peak ?? before),
      peakAt: at,
    };
  }
  return { kind: 'heal', from: before, to: after, at, peak, peakAt: trail?.peakAt ?? at };
}

/** What `trail` draws at `now`, or null once it left nothing. */
export function hitView(trail: Trail, now: number): HitView | null {
  const peak = peakAt(trail, now);
  const since = now - trail.at;
  let view: HitView = { fill: trail.to, ghost: null, draining: false, peak };
  if (trail.kind === 'hit') {
    if (since < HIT_HOLD) view = { ...view, ghost: trail.from };
    else if (since < HIT_HOLD + HIT_DRAIN) view = { ...view, ghost: trail.from, draining: true };
  } else if (since < HEAL_LEAD) {
    view = { ...view, fill: trail.from, ghost: trail.to };
  } else if (since < HEAL_LEAD + FILL_EASE) {
    view = { ...view, ghost: trail.to };
  }
  return view.ghost === null && peak === null ? null : view;
}

/** The next time after `now` that what `trail` draws changes, or null
 *  once it never will. */
export function nextStep(trail: Trail, now: number): number | null {
  const steps =
    trail.kind === 'hit'
      ? [trail.at + HIT_HOLD, trail.at + HIT_HOLD + HIT_DRAIN]
      : [trail.at + HEAL_LEAD, trail.at + HEAL_LEAD + FILL_EASE];
  if (trail.peak !== null) steps.push(trail.peakAt + PEAK_HOLD);
  const later = steps.filter((step) => step > now);
  return later.length === 0 ? null : Math.min(...later);
}

/** What each mark draws now, by vital and for your opponent. */
export type HitViews = Partial<Record<HitKey, HitView>>;

/** A mark at `pct` with what Show each hit leaves on it: where it
 *  fills, the far end of the pale part or null, and whether that
 *  drains. A value the game hides fills nothing. */
export function hitFill(
  pct: number | null,
  view: HitView | undefined,
): { fill: number | null; ghost: number | null; draining: boolean } {
  if (pct === null || !view) return { fill: pct, ghost: null, draining: false };
  return { fill: view.fill, ghost: view.ghost, draining: view.draining };
}

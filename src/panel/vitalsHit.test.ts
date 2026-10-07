import { describe, expect, it } from 'vitest';
import {
  FILL_EASE,
  HEAL_LEAD,
  HIT_DRAIN,
  HIT_HOLD,
  hitFill,
  hitView,
  nextStep,
  nextTrail,
  PEAK_HOLD,
} from './vitalsHit';

// Tolliver's health from 851 to 744 of 1038, as board 4 draws it.
const BEFORE = 82;
const AFTER = 72;

describe('Show each hit', () => {
  it('holds the part a hit took for 600 ms, drains it in 400, then leaves nothing', () => {
    const trail = nextTrail(null, BEFORE, AFTER, 1000)!;
    expect(hitView(trail, 1000)).toEqual({
      fill: AFTER,
      ghost: BEFORE,
      draining: false,
      peak: BEFORE,
    });
    expect(hitView(trail, 1000 + HIT_HOLD)).toEqual({
      fill: AFTER,
      ghost: AFTER,
      draining: true,
      peak: BEFORE,
    });
    expect(hitView(trail, 1000 + HIT_HOLD + HIT_DRAIN)?.ghost).toBeNull();
    expect(hitView(trail, 1000 + PEAK_HOLD)).toBeNull();
  });

  it('extends a trail that still holds, and starts over once it drains', () => {
    const first = nextTrail(null, 90, 82, 0)!;
    expect(nextTrail(first, 82, 72, HIT_HOLD - 1)).toMatchObject({ from: 90, to: 72 });
    expect(nextTrail(first, 82, 72, HIT_HOLD)).toMatchObject({ from: 82, to: 72 });
  });

  it('shows a heal pale first and has the fill follow 300 ms later', () => {
    const trail = nextTrail(null, AFTER, BEFORE, 0)!;
    expect(hitView(trail, 0)).toEqual({ fill: AFTER, ghost: BEFORE, draining: false, peak: null });
    expect(hitView(trail, HEAL_LEAD)).toMatchObject({ fill: BEFORE, ghost: BEFORE });
    expect(hitView(trail, HEAL_LEAD + FILL_EASE)).toBeNull();
  });

  it('keeps the highest peak while hits come, for 1.5 s from the last', () => {
    const one = nextTrail(null, 90, 82, 0)!;
    const two = nextTrail(one, 82, 72, 1000)!;
    expect(two.peak).toBe(90);
    expect(hitView(two, 1000 + PEAK_HOLD - 1)?.peak).toBe(90);
    expect(hitView(two, 1000 + PEAK_HOLD)).toBeNull();
  });

  it('starts nothing for a value the game hides or one that did not move', () => {
    expect(nextTrail(null, null, AFTER, 0)).toBeNull();
    expect(nextTrail(null, BEFORE, null, 0)).toBeNull();
    const trail = nextTrail(null, BEFORE, AFTER, 0);
    expect(nextTrail(trail, AFTER, AFTER, 10)).toBe(trail);
  });

  it('wakes at each step and then never again', () => {
    const trail = nextTrail(null, BEFORE, AFTER, 0)!;
    expect(nextStep(trail, 0)).toBe(HIT_HOLD);
    expect(nextStep(trail, HIT_HOLD)).toBe(HIT_HOLD + HIT_DRAIN);
    expect(nextStep(trail, HIT_HOLD + HIT_DRAIN)).toBe(PEAK_HOLD);
    expect(nextStep(trail, PEAK_HOLD)).toBeNull();
  });

  it('fills a hidden value with nothing, trail or not', () => {
    const view = hitView(nextTrail(null, BEFORE, AFTER, 0)!, 0)!;
    expect(hitFill(null, view)).toEqual({ fill: null, ghost: null, draining: false });
    expect(hitFill(50, undefined)).toEqual({ fill: 50, ghost: null, draining: false });
  });
});

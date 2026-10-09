import { describe, expect, it } from 'vitest';
import type { WorldTime } from '../stores/gmcp/worldStore';
import { isDaytime } from './daylight';
import {
  SUN_ARC_PATH,
  SUN_ARC_RADIUS,
  SUN_CENTER_X,
  SUN_HORIZON_PATH,
  SUN_HORIZON_Y,
  sunDot,
  type SunDot,
} from './sunPath';

function at(hour: number | null, sunlight: string | null = null): WorldTime {
  return { hour, minute: null, day: null, month: null, year: null, sunlight, sky: null };
}

/** The dot the status line draws for a World.Time, the way StatusLine
 *  reads it. Sunlight decides up or down, the hour decides where. */
function dotFor(time: WorldTime | null): SunDot {
  return sunDot(time?.hour ?? null, isDaytime(time));
}

/** The point on the arc for a share of the day from 0 to 1. */
function onArc(t: number): { x: number; y: number } {
  const angle = Math.PI * (1 - t);
  return {
    x: SUN_CENTER_X + SUN_ARC_RADIUS * Math.cos(angle),
    y: SUN_HORIZON_Y - SUN_ARC_RADIUS * Math.sin(angle),
  };
}

function expectUpAt(dot: SunDot, x: number, y: number): void {
  expect(dot.kind).toBe('up');
  if (dot.kind !== 'up') return;
  expect(dot.x).toBeCloseTo(x, 9);
  expect(dot.y).toBeCloseTo(y, 9);
}

describe('the sun path', () => {
  it('lays a horizon under a 5.5 arc centered on it', () => {
    expect(SUN_HORIZON_PATH).toBe('M1.5 10.5h13');
    expect(SUN_ARC_PATH).toBe('M2.5 10.5A5.5 5.5 0 0 1 13.5 10.5');
  });
});

describe('sunDot', () => {
  it('rises on the left just after 6:00', () => {
    const dot = dotFor(at(6));
    const { x, y } = onArc(0.5 / 13);
    expectUpAt(dot, x, y);
    expect(x).toBeCloseTo(2.54, 2);
    expect(y).toBeCloseTo(9.84, 2);
  });

  it('stands at the top of the arc at 12:00', () => {
    expectUpAt(dotFor(at(12)), 8, 5);
    expectUpAt(dotFor(at(12, 'light')), 8, 5);
  });

  it('sets on the right by 19:00', () => {
    const dot = dotFor(at(18, 'set'));
    const { x, y } = onArc(12.5 / 13);
    expectUpAt(dot, x, y);
    expect(x).toBeCloseTo(13.46, 2);
    expect(y).toBeCloseTo(9.84, 2);
  });

  it('moves left to right across the day', () => {
    const xs = Array.from({ length: 13 }, (_, i) => {
      const dot = dotFor(at(6 + i));
      return dot.kind === 'up' ? dot.x : Number.NaN;
    });
    for (let i = 1; i < xs.length; i++) expect(xs[i]).toBeGreaterThan(xs[i - 1]);
  });

  it('drops under the horizon as an open dot at 19:00 and at 2:00', () => {
    expect(dotFor(at(19))).toEqual({ kind: 'down' });
    expect(dotFor(at(2))).toEqual({ kind: 'down' });
    expect(dotFor(at(2, 'dark'))).toEqual({ kind: 'down' });
  });

  it('follows sunlight dark at noon under the horizon', () => {
    expect(dotFor(at(12, 'dark'))).toEqual({ kind: 'down' });
  });

  it('puts the dot at the top of the arc while the sun is up and the hour is unknown', () => {
    expectUpAt(dotFor(at(null, 'light')), 8, 5);
    expectUpAt(sunDot(null, true), 8, 5);
  });

  it('keeps a server hour outside the day on the arc', () => {
    expectUpAt(dotFor(at(4, 'rise')), 2.5, 10.5);
    expectUpAt(dotFor(at(21, 'set')), 13.5, 10.5);
  });

  it('draws no dot while neither the sun nor the hour is known', () => {
    expect(dotFor(null)).toEqual({ kind: 'none' });
    expect(dotFor(at(null))).toEqual({ kind: 'none' });
    expect(sunDot(null, null)).toEqual({ kind: 'none' });
  });
});

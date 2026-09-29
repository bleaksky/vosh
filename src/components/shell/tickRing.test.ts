import { describe, expect, it } from 'vitest';
import { TICK_RING_CENTER, TICK_RING_RADIUS, tickArc, tickArcLeft, type TickArc } from './tickRing';

/** The arc's end point, or null when nothing partial draws on top. */
function end(arc: TickArc): { x: number; y: number } | null {
  return arc.kind === 'part' ? arc.end : null;
}

describe('tickArc', () => {
  it('sits on the 5.75 ring at the center of the 16 unit grid', () => {
    expect(TICK_RING_CENTER).toBe(8);
    expect(TICK_RING_RADIUS).toBe(5.75);
  });

  it('draws nothing on top at 0 seconds', () => {
    expect(tickArc(0, 60)).toEqual({ kind: 'empty' });
    expect(tickArc(0, 30)).toEqual({ kind: 'empty' });
  });

  it('ends at 3 o clock a quarter of the way through', () => {
    const arc = tickArc(15, 60);
    expect(arc.kind).toBe('part');
    expect(end(arc)?.x).toBeCloseTo(13.75, 9);
    expect(end(arc)?.y).toBeCloseTo(8, 9);
    expect(arc).toMatchObject({ share: 0.25, path: 'M8 2.25A5.75 5.75 0 0 1 13.75 8' });
  });

  it('ends at 6 o clock halfway through', () => {
    const arc = tickArc(30, 60);
    expect(end(arc)?.x).toBeCloseTo(8, 9);
    expect(end(arc)?.y).toBeCloseTo(13.75, 9);
    expect(arc).toMatchObject({ share: 0.5, path: 'M8 2.25A5.75 5.75 0 0 1 8 13.75' });
  });

  it('takes the long way round past the half', () => {
    const arc = tickArc(45, 60);
    expect(end(arc)?.x).toBeCloseTo(2.25, 9);
    expect(end(arc)?.y).toBeCloseTo(8, 9);
    expect(arc).toMatchObject({ share: 0.75, path: 'M8 2.25A5.75 5.75 0 1 1 2.25 8' });
  });

  it('runs clockwise from the top for the share of the interval gone', () => {
    const arc = tickArc(3, 30);
    const angle = Math.PI / 5;
    expect(end(arc)?.x).toBeCloseTo(8 + 5.75 * Math.sin(angle), 9);
    expect(end(arc)?.y).toBeCloseTo(8 - 5.75 * Math.cos(angle), 9);
    expect(arc).toMatchObject({ share: 0.1, path: 'M8 2.25A5.75 5.75 0 0 1 11.38 3.35' });
  });

  it('closes the whole circle at the interval and past it', () => {
    expect(tickArc(60, 60)).toEqual({ kind: 'whole' });
    expect(tickArc(30, 30)).toEqual({ kind: 'whole' });
    expect(tickArc(75, 60)).toEqual({ kind: 'whole' });
  });

  it('closes the circle when a long interval is a hair short of the tick', () => {
    // At two decimals the end of these arcs lands back on 8,2.25, and
    // SVG drops an arc whose ends meet, so the ring would read empty.
    expect(tickArc(7299, 7300)).toEqual({ kind: 'whole' });
    expect(tickArc(9999, 10000)).toEqual({ kind: 'whole' });
    expect(tickArc(86399, 86400)).toEqual({ kind: 'whole' });
    expect(tickArc(29.999, 30)).toEqual({ kind: 'whole' });
  });

  it('keeps a nearly closed arc while its end still stands apart from the top', () => {
    expect(tickArc(9998, 10000)).toMatchObject({
      kind: 'part',
      path: 'M8 2.25A5.75 5.75 0 1 1 7.99 2.25',
    });
  });

  it('draws nothing on top while a long interval has barely begun', () => {
    expect(tickArc(1, 10000)).toEqual({ kind: 'empty' });
    expect(tickArc(1, 86400)).toEqual({ kind: 'empty' });
    expect(tickArc(0.001, 30)).toEqual({ kind: 'empty' });
  });

  it('never draws an arc that ends where it starts', () => {
    for (const interval of [30, 3600, 7226, 7300, 10000, 36000, 86400]) {
      for (let secs = 1; secs < interval; secs++) {
        const arc = tickArc(secs, interval);
        if (arc.kind === 'part') expect(arc.path.endsWith(' 8 2.25')).toBe(false);
        else expect(arc.kind).toBe(secs / interval > 0.5 ? 'whole' : 'empty');
      }
    }
  });

  it('clamps a count below zero to an empty ring', () => {
    expect(tickArc(-3, 30)).toEqual({ kind: 'empty' });
    expect(tickArc(Number.NaN, 30)).toEqual({ kind: 'empty' });
  });

  it('leaves the faint ring alone while the interval is unknown or not positive', () => {
    expect(tickArc(14, null)).toEqual({ kind: 'empty' });
    expect(tickArc(14, 0)).toEqual({ kind: 'empty' });
    expect(tickArc(14, -30)).toEqual({ kind: 'empty' });
    expect(tickArc(14, Number.NaN)).toEqual({ kind: 'empty' });
  });
});

/** Where the count stands on a partial arc, its moving edge, or null. */
function edge(arc: TickArc): { x: number; y: number } | null {
  return arc.kind === 'part' ? arc.end : null;
}

describe('tickArcLeft', () => {
  it('closes the whole circle right after a tick', () => {
    expect(tickArcLeft(30, 30)).toEqual({ kind: 'whole' });
    expect(tickArcLeft(60, 60)).toEqual({ kind: 'whole' });
  });

  it('draws the share left from where the count stands, clockwise to the top', () => {
    // 15 of 60 seconds left: the quarter from 9 o clock up to 12.
    const arc = tickArcLeft(15, 60);
    expect(arc).toMatchObject({ share: 0.25, path: 'M2.25 8A5.75 5.75 0 0 1 8 2.25' });
    expect(edge(arc)?.x).toBeCloseTo(2.25, 9);
    expect(edge(arc)?.y).toBeCloseTo(8, 9);
    // Half left: the left half, from 6 o clock up to 12.
    expect(tickArcLeft(30, 60)).toMatchObject({ path: 'M8 13.75A5.75 5.75 0 0 1 8 2.25' });
    // Three quarters left takes the long way round from 3 o clock.
    expect(tickArcLeft(45, 60)).toMatchObject({ path: 'M13.75 8A5.75 5.75 0 1 1 8 2.25' });
  });

  it('empties toward the tick at the top as the seconds run out', () => {
    const arc = tickArcLeft(1, 30);
    expect(arc).toMatchObject({ kind: 'part', share: 1 / 30 });
    const angle = (29 / 30) * 2 * Math.PI;
    expect(edge(arc)?.x).toBeCloseTo(8 + 5.75 * Math.sin(angle), 9);
    expect(edge(arc)?.y).toBeCloseTo(8 - 5.75 * Math.cos(angle), 9);
  });

  it('starts where the arc counting up ends, so the edge moves clockwise in every way', () => {
    for (let secs = 1; secs < 30; secs++) {
      const up = tickArc(secs, 30);
      const left = tickArcLeft(30 - secs, 30);
      expect(up.kind === 'part' && left.kind === 'part').toBe(true);
      if (up.kind === 'part' && left.kind === 'part') {
        expect(left.end.x).toBeCloseTo(up.end.x, 9);
        expect(left.end.y).toBeCloseTo(up.end.y, 9);
      }
    }
  });

  it('leaves the faint ring alone at 0 and below zero', () => {
    expect(tickArcLeft(0, 30)).toEqual({ kind: 'empty' });
    expect(tickArcLeft(-5, 30)).toEqual({ kind: 'empty' });
    expect(tickArcLeft(Number.NaN, 30)).toEqual({ kind: 'empty' });
  });

  it('never draws an arc that ends where it starts', () => {
    expect(tickArcLeft(9999, 10000)).toEqual({ kind: 'whole' });
    expect(tickArcLeft(1, 10000)).toEqual({ kind: 'empty' });
    for (const interval of [30, 3600, 7300, 10000, 86400]) {
      for (let secs = 1; secs < interval; secs++) {
        const arc = tickArcLeft(secs, interval);
        if (arc.kind === 'part') expect(arc.path.startsWith('M8 2.25A')).toBe(false);
        else expect(arc.kind).toBe(secs / interval > 0.5 ? 'whole' : 'empty');
      }
    }
  });

  it('leaves the faint ring alone while the interval is unknown or not positive', () => {
    expect(tickArcLeft(14, null)).toEqual({ kind: 'empty' });
    expect(tickArcLeft(14, 0)).toEqual({ kind: 'empty' });
  });
});

import { describe, expect, it } from 'vitest';
import { MOON_CENTER, MOON_DRAWN_TERMINATOR, MOON_RADIUS, moonPhaseShape } from './moonPhase';

// The lit path, flattened to a polygon, so the tests measure the shape
// the icon draws rather than the numbers that made it.

type Pt = [number, number];

function angleBetween(ux: number, uy: number, vx: number, vy: number): number {
  return Math.atan2(ux * vy - uy * vx, ux * vx + uy * vy);
}

// SVG arc from endpoints to center (SVG 1.1 F.6.5), no rotation.
function arcPoints(
  from: Pt,
  rxIn: number,
  ryIn: number,
  large: number,
  sweep: number,
  to: Pt,
  steps = 96,
): Pt[] {
  const [x1, y1] = from;
  const [x2, y2] = to;
  const hx = (x1 - x2) / 2;
  const hy = (y1 - y2) / 2;
  let rx = Math.abs(rxIn);
  let ry = Math.abs(ryIn);
  const lambda = (hx * hx) / (rx * rx) + (hy * hy) / (ry * ry);
  if (lambda > 1) {
    rx *= Math.sqrt(lambda);
    ry *= Math.sqrt(lambda);
  }
  const num = rx * rx * ry * ry - rx * rx * hy * hy - ry * ry * hx * hx;
  const den = rx * rx * hy * hy + ry * ry * hx * hx;
  const coef = (large !== sweep ? 1 : -1) * Math.sqrt(Math.max(0, num / den));
  const cxp = (coef * rx * hy) / ry;
  const cyp = (-coef * ry * hx) / rx;
  const cx = cxp + (x1 + x2) / 2;
  const cy = cyp + (y1 + y2) / 2;
  const ux = (hx - cxp) / rx;
  const uy = (hy - cyp) / ry;
  const vx = (-hx - cxp) / rx;
  const vy = (-hy - cyp) / ry;
  const start = angleBetween(1, 0, ux, uy);
  let delta = angleBetween(ux, uy, vx, vy);
  if (sweep === 0 && delta > 0) delta -= 2 * Math.PI;
  if (sweep === 1 && delta < 0) delta += 2 * Math.PI;
  const out: Pt[] = [];
  for (let i = 1; i <= steps; i += 1) {
    const t = start + (delta * i) / steps;
    out.push([cx + rx * Math.cos(t), cy + ry * Math.sin(t)]);
  }
  return out;
}

function flatten(d: string): Pt[] {
  const tokens = d.match(/[MALZ]|-?\d*\.?\d+/g) ?? [];
  const pts: Pt[] = [];
  let i = 0;
  let at: Pt = [0, 0];
  const next = () => Number(tokens[i++]);
  while (i < tokens.length) {
    const cmd = tokens[i++];
    if (cmd === 'M' || cmd === 'L') {
      at = [next(), next()];
      pts.push(at);
    } else if (cmd === 'A') {
      const rx = next();
      const ry = next();
      next();
      const large = next();
      const sweep = next();
      const to: Pt = [next(), next()];
      pts.push(...arcPoints(at, rx, ry, large, sweep, to));
      at = to;
    } else if (cmd !== 'Z') {
      throw new Error(`unexpected path command ${cmd}`);
    }
  }
  return pts;
}

function area(poly: Pt[]): number {
  let sum = 0;
  for (let i = 0; i < poly.length; i += 1) {
    const [x1, y1] = poly[i];
    const [x2, y2] = poly[(i + 1) % poly.length];
    sum += x1 * y2 - x2 * y1;
  }
  return Math.abs(sum) / 2;
}

function centroidX(poly: Pt[]): number {
  let a = 0;
  let cx = 0;
  for (let i = 0; i < poly.length; i += 1) {
    const [x1, y1] = poly[i];
    const [x2, y2] = poly[(i + 1) % poly.length];
    const cross = x1 * y2 - x2 * y1;
    a += cross;
    cx += (x1 + x2) * cross;
  }
  return cx / (3 * a);
}

function contains(poly: Pt[], [x, y]: Pt): boolean {
  let inside = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i, i += 1) {
    const [xi, yi] = poly[i];
    const [xj, yj] = poly[j];
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}

// The lit length along the horizontal at y, from where the polygon's
// edges cross it.
function litWidth(poly: Pt[], y: number): number {
  const xs: number[] = [];
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i, i += 1) {
    const [xi, yi] = poly[i];
    const [xj, yj] = poly[j];
    if (yi > y !== yj > y) xs.push(((xj - xi) * (y - yi)) / (yj - yi) + xi);
  }
  xs.sort((a, b) => a - b);
  let width = 0;
  for (let i = 0; i + 1 < xs.length; i += 2) width += xs[i + 1] - xs[i];
  return width;
}

const PHASES = [0, 1, 2, 3, 4, 5, 6, 7];
const DISC = Math.PI * MOON_RADIUS * MOON_RADIUS;

function litArea(phase: number): number {
  const d = moonPhaseShape(phase)?.litPath;
  return d ? area(flatten(d)) : 0;
}

function litWidthAt(phase: number, y = MOON_CENTER): number {
  const d = moonPhaseShape(phase)?.litPath;
  return d ? litWidth(flatten(d), y) : 0;
}

describe('moonPhaseShape', () => {
  it('lights nothing at new and the whole disc at full', () => {
    expect(moonPhaseShape(0)).toMatchObject({ litFraction: 0, side: null, litPath: null });
    expect(litArea(0)).toBe(0);
    const full = moonPhaseShape(4);
    expect(full).toMatchObject({ litFraction: 1, side: null });
    expect(litArea(4)).toBeCloseTo(DISC, 1);
  });

  it('grows the lit area from new to full and shrinks it after', () => {
    const areas = PHASES.map(litArea);
    for (let p = 1; p <= 4; p += 1) expect(areas[p]).toBeGreaterThan(areas[p - 1]);
    for (let p = 5; p <= 7; p += 1) expect(areas[p]).toBeLessThan(areas[p - 1]);
    expect(areas[7]).toBeGreaterThan(0);
  });

  it('keeps the true share of the disc the phase angle gives', () => {
    for (const p of PHASES) {
      const want = (1 - Math.cos((p * Math.PI) / 4)) / 2;
      expect(moonPhaseShape(p)?.litFraction).toBeCloseTo(want, 9);
    }
    expect(moonPhaseShape(1)?.litFraction).toBeCloseTo((1 - Math.cos(Math.PI / 4)) / 2, 12);
  });

  it('draws new, half, and full at their true size', () => {
    for (const p of [0, 2, 4, 6]) {
      expect(litArea(p) / DISC).toBeCloseTo(moonPhaseShape(p)?.litFraction ?? NaN, 2);
    }
  });

  it('draws the crescents and the nearly full phases wider than the sky', () => {
    // A crescent lights a sliver 0.6 of the radius wide on the
    // horizontal through the center, 3.75 units, about twice the true
    // 1.83. A nearly full moon leaves a dark sliver just as wide.
    for (const p of [1, 7]) expect(litWidthAt(p)).toBeCloseTo(3.75, 3);
    for (const p of [3, 5]) expect(2 * MOON_RADIUS - litWidthAt(p)).toBeCloseTo(3.75, 3);
    // So the crescents light more of the disc than the sky does and the
    // nearly full phases less.
    for (const p of [1, 7]) {
      expect(litArea(p) / DISC).toBeCloseTo((1 - 0.4) / 2, 2);
      expect(litArea(p) / DISC).toBeGreaterThan(moonPhaseShape(p)?.litFraction ?? NaN);
    }
    for (const p of [3, 5]) {
      expect(litArea(p) / DISC).toBeCloseTo((1 + 0.4) / 2, 2);
      expect(litArea(p) / DISC).toBeLessThan(moonPhaseShape(p)?.litFraction ?? NaN);
    }
  });

  it('draws the half phases with a straight terminator', () => {
    for (const p of [2, 6]) {
      expect(moonPhaseShape(p)?.terminatorRx).toBe(0);
      // Every horizontal lights exactly the half of its chord on the lit
      // side, so the terminator runs down the middle.
      for (const y of [3, 5.5, MOON_CENTER, 10.5, 13]) {
        const halfChord = Math.sqrt(MOON_RADIUS ** 2 - (y - MOON_CENTER) ** 2);
        expect(litWidthAt(p, y)).toBeCloseTo(halfChord, 2);
      }
    }
  });

  it('mirrors a fading phase onto the growing one', () => {
    for (const p of [1, 2, 3]) expect(litArea(8 - p)).toBeCloseTo(litArea(p), 3);
  });

  it('lights the right side while growing and the left while fading', () => {
    const right: Pt = [MOON_CENTER + MOON_RADIUS - 0.5, MOON_CENTER];
    const left: Pt = [MOON_CENTER - MOON_RADIUS + 0.5, MOON_CENTER];
    for (const p of [1, 2, 3]) {
      const shape = moonPhaseShape(p);
      expect(shape?.side).toBe('right');
      const poly = flatten(shape?.litPath ?? '');
      expect(centroidX(poly)).toBeGreaterThan(MOON_CENTER);
      expect(contains(poly, right)).toBe(true);
      expect(contains(poly, left)).toBe(false);
    }
    for (const p of [5, 6, 7]) {
      const shape = moonPhaseShape(p);
      expect(shape?.side).toBe('left');
      const poly = flatten(shape?.litPath ?? '');
      expect(centroidX(poly)).toBeLessThan(MOON_CENTER);
      expect(contains(poly, left)).toBe(true);
      expect(contains(poly, right)).toBe(false);
    }
    expect(centroidX(flatten(moonPhaseShape(4)?.litPath ?? ''))).toBeCloseTo(MOON_CENTER, 3);
  });

  it('reports the terminator it draws', () => {
    const rx = PHASES.map((p) => moonPhaseShape(p)?.terminatorRx ?? NaN);
    const drawn = MOON_RADIUS * 0.4;
    const want = [MOON_RADIUS, drawn, 0, drawn, MOON_RADIUS, drawn, 0, drawn];
    rx.forEach((v, i) => expect(v).toBeCloseTo(want[i], 9));
    expect(MOON_DRAWN_TERMINATOR).toBe(0.4);
    // A crescent keeps its middle dark and a gibbous moon lights it.
    const middle: Pt = [MOON_CENTER, MOON_CENTER];
    expect(contains(flatten(moonPhaseShape(1)?.litPath ?? ''), middle)).toBe(false);
    expect(contains(flatten(moonPhaseShape(3)?.litPath ?? ''), middle)).toBe(true);
    expect(contains(flatten(moonPhaseShape(7)?.litPath ?? ''), middle)).toBe(false);
    expect(contains(flatten(moonPhaseShape(5)?.litPath ?? ''), middle)).toBe(true);
  });

  it('traces the limb on the lit side and no other', () => {
    expect(moonPhaseShape(0)?.litLimbPath).toBeNull();
    expect(moonPhaseShape(4)?.litLimbPath).toBe(moonPhaseShape(4)?.litPath);
    for (const p of [1, 2, 3, 5, 6, 7]) {
      const shape = moonPhaseShape(p);
      const limb = flatten(shape?.litLimbPath ?? '');
      expect(limb.length).toBeGreaterThan(10);
      for (const [x, y] of limb) {
        expect(Math.hypot(x - MOON_CENTER, y - MOON_CENTER)).toBeCloseTo(MOON_RADIUS, 6);
        if (shape?.side === 'right') expect(x).toBeGreaterThanOrEqual(MOON_CENTER - 1e-9);
        else expect(x).toBeLessThanOrEqual(MOON_CENTER + 1e-9);
      }
      expect(shape?.litPath?.startsWith(shape.litLimbPath ?? '')).toBe(true);
    }
  });

  it('stays inside the disc', () => {
    for (const p of PHASES) {
      for (const [x, y] of flatten(moonPhaseShape(p)?.litPath ?? '')) {
        expect(Math.hypot(x - MOON_CENTER, y - MOON_CENTER)).toBeLessThanOrEqual(
          MOON_RADIUS + 1e-6,
        );
      }
    }
  });

  it('has no shape for a phase outside the table', () => {
    for (const p of [null, -1, 8, 2.5, Number.NaN]) expect(moonPhaseShape(p)).toBeNull();
  });
});

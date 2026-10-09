import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  HOLD_MS,
  aiming,
  headsInto,
  notePointer,
  pointAt,
  pointerLeft,
  resetMenuAim,
} from './menuAim';

// The writing card's title menu as the harness measured it: New is the
// row at y 320 to 350, Sent sits under it, and the New submenu opens
// 4 px right of the menu, from y 314 down to 603.
const SUB = { left: 370, right: 530, top: 314, bottom: 603 };
// The same submenu flipped to the left of its menu.
const LEFT = { left: 40, right: 200, top: 314, bottom: 603 };

/** The pointer moving through `points`, as the document hears it. */
function travel(...points: [number, number][]) {
  for (const [x, y] of points) notePointer(x, y);
}

beforeEach(() => resetMenuAim());

describe('headsInto', () => {
  it('reads a pointer moving right, straight or on a slant, as heading in', () => {
    expect(headsInto({ x: 194, y: 335 }, { x: 205, y: 335 }, SUB)).toBe(true);
    // Down toward Your history, the last row, crossing Sent.
    expect(headsInto({ x: 194, y: 335 }, { x: 204, y: 345 }, SUB)).toBe(true);
    // Up toward the first row.
    expect(headsInto({ x: 194, y: 335 }, { x: 204, y: 334 }, SUB)).toBe(true);
  });

  it('reads a submenu to the left the same way', () => {
    expect(headsInto({ x: 300, y: 335 }, { x: 290, y: 345 }, LEFT)).toBe(true);
    expect(headsInto({ x: 300, y: 335 }, { x: 310, y: 345 }, LEFT)).toBe(false);
  });

  it('lets a pointer turning away, or straight down the menu, take the row', () => {
    expect(headsInto({ x: 194, y: 335 }, { x: 184, y: 345 }, SUB)).toBe(false);
    expect(headsInto({ x: 194, y: 335 }, { x: 194, y: 365 }, SUB)).toBe(false);
    // Steeper than the line to the submenu's bottom corner.
    expect(headsInto({ x: 194, y: 335 }, { x: 200, y: 400 }, SUB)).toBe(false);
    // Resting, or already over the submenu.
    expect(headsInto({ x: 194, y: 335 }, { x: 194, y: 335 }, SUB)).toBe(false);
    expect(headsInto({ x: 360, y: 335 }, { x: 400, y: 335 }, SUB)).toBe(false);
  });
});

describe('aiming', () => {
  it('needs the pointer to have moved', () => {
    expect(aiming([SUB])).toBe(false);
    travel([194, 335]);
    expect(aiming([SUB])).toBe(false);
    travel([210, 350]);
    expect(aiming([SUB])).toBe(true);
    expect(aiming([])).toBe(false);
  });

  it('keeps a slant that began just after a turn', () => {
    // Down onto New, then off toward the bottom of the submenu. The
    // oldest point alone reads too steep. The newer ones read the slant.
    travel([134, 320], [134, 335], [152, 349], [169, 362]);
    expect(aiming([SUB])).toBe(true);
  });
});

describe('pointAt', () => {
  const row = {} as Element;
  const other = {} as Element;

  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('gives a row the pointer reaches the highlight at once', () => {
    const take = vi.fn();
    travel([194, 335], [194, 365]);
    pointAt(row, take, [SUB]);
    expect(take).toHaveBeenCalledOnce();
  });

  it('holds a row crossed on the way into a submenu, and drops it when the pointer leaves', () => {
    const take = vi.fn();
    travel([194, 335], [210, 352]);
    pointAt(row, take, [SUB]);
    expect(take).not.toHaveBeenCalled();
    pointerLeft(other);
    vi.advanceTimersByTime(HOLD_MS - 1);
    expect(take).not.toHaveBeenCalled();
    pointerLeft(row);
    vi.advanceTimersByTime(HOLD_MS);
    expect(take).not.toHaveBeenCalled();
  });

  it('gives the row the highlight once the pointer rests on it', () => {
    const take = vi.fn();
    travel([194, 335], [210, 352]);
    pointAt(row, take, [SUB]);
    vi.advanceTimersByTime(HOLD_MS);
    expect(take).toHaveBeenCalledOnce();
  });

  it('gives the row the highlight as soon as the pointer turns away', () => {
    const held = vi.fn();
    travel([194, 335], [210, 352]);
    pointAt(row, held, [SUB]);
    const take = vi.fn();
    travel([200, 362], [195, 368], [190, 372], [186, 376]);
    pointAt(row, take, [SUB]);
    expect(take).toHaveBeenCalledOnce();
    vi.advanceTimersByTime(HOLD_MS);
    expect(held).not.toHaveBeenCalled();
  });

  it('holds one row at a time', () => {
    const first = vi.fn();
    const second = vi.fn();
    travel([194, 335], [210, 352]);
    pointAt(row, first, [SUB]);
    travel([226, 368]);
    pointAt(other, second, [SUB]);
    vi.advanceTimersByTime(HOLD_MS);
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledOnce();
  });
});

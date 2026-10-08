import { describe, expect, it } from 'vitest';
import {
  dragTo,
  foldBelow,
  heightForRows,
  rowLimits,
  SNOOP_CHROME,
  SNOOP_FOLDED,
  snoopHeight,
} from './snoopSplitSize';

// How tall the snoop split stands: whole rows, never under the strip
// and four rows, always six rows left to you, and folded to the strip
// when dragged to the top.

const ROW = 17;
/** The column at 1280 by 800, and at the narrowest window, 640 by 480. */
const WIDE = { column: 698, row: ROW };
const NARROW = { column: 378, row: ROW };

describe('the snoop split size', () => {
  it('stands in whole rows under its strip, line and foot', () => {
    expect(SNOOP_FOLDED).toBe(33);
    expect(SNOOP_CHROME).toBe(39);
    expect(heightForRows(4, ROW)).toBe(39 + 68);
    // A row of a fraction of a pixel rounds the height up, so xterm
    // still fits every row.
    expect(heightForRows(10, 16.8)).toBe(39 + 168);
  });

  it('keeps four rows and leaves your terminal six', () => {
    // 698 less your 12 of insets and six rows, less the chrome.
    expect(rowLimits(WIDE)).toEqual({ min: 4, max: Math.floor((698 - 12 - 102 - 39) / ROW) });
    // Too short a column for both keeps the snoop's four.
    expect(rowLimits({ column: 200, row: ROW })).toEqual({ min: 4, max: 4 });
  });

  it('opens at 40 percent of the column in whole rows', () => {
    // 40 percent of 698 is 279.2, nearest 14 rows of 17 under 39.
    expect(snoopHeight(0.4, WIDE)).toBe(39 + 14 * ROW);
    expect(snoopHeight(0.4, NARROW)).toBe(39 + 7 * ROW);
    expect(snoopHeight(0.05, WIDE)).toBe(heightForRows(4, ROW));
    const { max } = rowLimits(WIDE);
    expect(snoopHeight(0.95, WIDE)).toBe(heightForRows(max, ROW));
  });

  it('snaps a drag to rows and saves the share it lands on', () => {
    const to = dragTo(39 + 10 * ROW + 6, WIDE);
    expect(to).toEqual({ folded: false, height: 39 + 10 * ROW, share: (39 + 10 * ROW) / 698 });
    // The share it saves opens at the same height again.
    if (!to.folded) expect(snoopHeight(to.share, WIDE)).toBe(to.height);
    // Past the limits it holds at them.
    expect(dragTo(700, WIDE)).toMatchObject({ height: heightForRows(rowLimits(WIDE).max, ROW) });
    expect(dragTo(foldBelow(WIDE), WIDE)).toMatchObject({ height: heightForRows(4, ROW) });
  });

  it('folds to the strip when dragged to the top', () => {
    // Halfway from the strip to four rows.
    expect(foldBelow(WIDE)).toBe((33 + 39 + 68) / 2);
    expect(dragTo(foldBelow(WIDE) - 1, WIDE)).toEqual({ folded: true });
    expect(dragTo(0, WIDE)).toEqual({ folded: true });
    expect(dragTo(-40, NARROW)).toEqual({ folded: true });
  });
});

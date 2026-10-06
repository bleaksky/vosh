import { describe, expect, it } from 'vitest';
import { dropPlace, liftTravel, partShift, ROW_PITCH } from './useRowDrag';

// Where a dragged row of the sessions sidebar lands, board 8: the row
// lands on the slot its middle is over, among the other rows, and the
// rows it passed step one slot toward its place.

describe('dragging a session row', () => {
  it('lands on the slot the row is over, as frame b8-drag draws it', () => {
    // The Forsaken Lands 1825 lifts from the third place, 31 up, so its
    // middle is over the second slot and the line marks that place.
    expect(dropPlace(2, -31, 3)).toBe(1);
    expect(partShift(0, 2, 1)).toBe(0);
    expect(partShift(1, 2, 1)).toBe(ROW_PITCH);
  });

  it('stays in its place until its middle passes into the next slot', () => {
    expect(dropPlace(1, 22, 4)).toBe(1);
    expect(dropPlace(1, 24, 4)).toBe(2);
    expect(dropPlace(1, -24, 4)).toBe(0);
    expect(dropPlace(1, -22, 4)).toBe(1);
  });

  it('lands on the first or the last place past the ends of the list', () => {
    expect(dropPlace(1, -400, 4)).toBe(0);
    expect(dropPlace(1, 400, 4)).toBe(3);
  });

  it('holds the row inside the list', () => {
    expect(liftTravel(1, -400, 4)).toBe(-ROW_PITCH);
    expect(liftTravel(1, 400, 4)).toBe(2 * ROW_PITCH);
    expect(liftTravel(1, 12, 4)).toBe(12);
  });

  it('parts the rows between the row and where it lands, toward its place', () => {
    // Down from the first place to the third: the second and the third
    // step up.
    expect([0, 1, 2, 3].map((at) => partShift(at, 0, 2))).toEqual([0, -ROW_PITCH, -ROW_PITCH, 0]);
    // Up from the fourth place to the second: the second and the third
    // step down.
    expect([0, 1, 2, 3].map((at) => partShift(at, 3, 1))).toEqual([0, ROW_PITCH, ROW_PITCH, 0]);
    expect([0, 1, 2].map((at) => partShift(at, 1, 1))).toEqual([0, 0, 0]);
  });
});

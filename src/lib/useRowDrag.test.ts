import { describe, expect, it } from 'vitest';
import { dropPlace, keyedPlace, liftTravel, partShift } from './useRowDrag';

// Where a dragged row lands: on the slot its middle is over, among the
// other rows, and the rows it passed step one slot toward its place.
// The sessions sidebar sets its rows 46 apart.

const ROW_PITCH = 46;

describe('dragging a session row', () => {
  it('lands on the slot the row is over, as frame b8-drag draws it', () => {
    // The Forsaken Lands 1825 lifts from the third place, 31 up, so its
    // middle is over the second slot and the line marks that place.
    expect(dropPlace(2, -31, 3, ROW_PITCH)).toBe(1);
    expect(partShift(0, 2, 1, ROW_PITCH)).toBe(0);
    expect(partShift(1, 2, 1, ROW_PITCH)).toBe(ROW_PITCH);
  });

  it('stays in its place until its middle passes into the next slot', () => {
    expect(dropPlace(1, 22, 4, ROW_PITCH)).toBe(1);
    expect(dropPlace(1, 24, 4, ROW_PITCH)).toBe(2);
    expect(dropPlace(1, -24, 4, ROW_PITCH)).toBe(0);
    expect(dropPlace(1, -22, 4, ROW_PITCH)).toBe(1);
  });

  it('lands on the first or the last place past the ends of the list', () => {
    expect(dropPlace(1, -400, 4, ROW_PITCH)).toBe(0);
    expect(dropPlace(1, 400, 4, ROW_PITCH)).toBe(3);
  });

  it('holds the row inside the list', () => {
    expect(liftTravel(1, -400, 4, ROW_PITCH)).toBe(-ROW_PITCH);
    expect(liftTravel(1, 400, 4, ROW_PITCH)).toBe(2 * ROW_PITCH);
    expect(liftTravel(1, 12, 4, ROW_PITCH)).toBe(12);
  });

  it('parts the rows between the row and where it lands, toward its place', () => {
    // Down from the first place to the third: the second and the third
    // step up.
    expect([0, 1, 2, 3].map((at) => partShift(at, 0, 2, ROW_PITCH))).toEqual([
      0,
      -ROW_PITCH,
      -ROW_PITCH,
      0,
    ]);
    // Up from the fourth place to the second: the second and the third
    // step down.
    expect([0, 1, 2, 3].map((at) => partShift(at, 3, 1, ROW_PITCH))).toEqual([
      0,
      ROW_PITCH,
      ROW_PITCH,
      0,
    ]);
    expect([0, 1, 2].map((at) => partShift(at, 1, 1, ROW_PITCH))).toEqual([0, 0, 0]);
  });
});

// The vitals list under Customize vitals sets its rows 40 apart and
// moves a vital from the keyboard a place at a time.

describe('moving a vital', () => {
  const PITCH = 40;

  it('lands Moves first, as frame b3 Moves in your hand draws it', () => {
    // Moves lifts from the third place and rises two rows.
    expect(dropPlace(2, -2 * PITCH, 3, PITCH)).toBe(0);
    expect([0, 1, 2].map((at) => partShift(at, 2, 0, PITCH))).toEqual([PITCH, PITCH, 0]);
  });

  it('moves a place for each arrow key and stops at the ends', () => {
    expect(keyedPlace('ArrowUp', 2, 3)).toBe(1);
    expect(keyedPlace('ArrowUp', 0, 3)).toBe(0);
    expect(keyedPlace('ArrowDown', 1, 3)).toBe(2);
    expect(keyedPlace('ArrowDown', 2, 3)).toBe(2);
    expect(keyedPlace('Enter', 1, 3)).toBeNull();
  });
});

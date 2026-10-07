import { describe, expect, it } from 'vitest';
import { FIT_ALL, NAME_MIN, statusLineFit, type StatusLineWidths } from './statusLineFit';

// Widths near what the 12 px panel face draws on board 4: Health, Mana
// and Moves with `1020 / 1020`, `800 / 800` and `930 / 930`, 54% held
// at 100%, and the tick, the time and three moons.
const LINE: StatusLineWidths = {
  room: 1000,
  lead: 0,
  vitals: [
    { label: 38, value: 64, current: 28 },
    { label: 32, value: 56, current: 21 },
    { label: 36, value: 56, current: 21 },
  ],
  foe: 29,
  fighting: false,
  target: 0,
  tick: 22,
  time: 28,
  moons: 50,
};

/** The width of the line at FIT_ALL without a name. */
const FULL = 38 + 6 + 64 + 32 + 6 + 56 + 36 + 6 + 56 + 29 + (22 + 8 + 28 + 8 + 50) + 20 * 4;

describe('statusLineFit', () => {
  it('shows everything with room', () => {
    expect(statusLineFit(LINE)).toEqual(FIT_ALL);
    expect(statusLineFit({ ...LINE, room: FULL })).toEqual(FIT_ALL);
  });

  it('keeps room for an opponent at 100 out of a fight, so a fight moves nothing', () => {
    const calm = statusLineFit({ ...LINE, room: FULL - 1 });
    expect(calm.labels).toBe(false);
    expect(statusLineFit({ ...LINE, room: FULL - 1, foe: null })).toEqual(FIT_ALL);
  });

  it('lets the name go first, once it has less than its least', () => {
    const fight = { ...LINE, fighting: true };
    expect(statusLineFit({ ...fight, room: FULL + 6 + NAME_MIN })).toEqual(FIT_ALL);
    expect(statusLineFit({ ...fight, room: FULL + 6 + NAME_MIN - 1 })).toEqual({
      ...FIT_ALL,
      names: false,
    });
  });

  it('counts a Target item on another mob with the name', () => {
    const both = { ...LINE, fighting: true, target: 44 };
    const room = FULL + 6 + NAME_MIN + 20 + 44 + NAME_MIN;
    expect(statusLineFit({ ...both, room }).names).toBe(true);
    expect(statusLineFit({ ...both, room: room - 1 }).names).toBe(false);
  });

  it('gives way in order: labels, then Current, then the moons, then the time', () => {
    const labels = FULL - (38 + 6 + 32 + 6 + 36 + 6);
    const current = labels - (64 - 28 + 56 - 21 + 56 - 21);
    const moons = current - (8 + 50);
    const time = moons - (8 + 28);
    expect(statusLineFit({ ...LINE, room: labels })).toEqual({
      ...FIT_ALL,
      names: false,
      labels: false,
    });
    expect(statusLineFit({ ...LINE, room: labels - 1 }).current).toBe(true);
    expect(statusLineFit({ ...LINE, room: current }).moons).toBe(true);
    expect(statusLineFit({ ...LINE, room: current - 1 })).toEqual({
      names: false,
      labels: false,
      current: true,
      moons: false,
      time: true,
    });
    expect(statusLineFit({ ...LINE, room: moons - 1 }).time).toBe(false);
    expect(statusLineFit({ ...LINE, room: time }).time).toBe(false);
  });

  it('keeps the tick and ends at the last step when nothing fits', () => {
    expect(statusLineFit({ ...LINE, room: 10 })).toEqual({
      names: false,
      labels: false,
      current: true,
      moons: false,
      time: false,
    });
  });

  it('counts what leads the line, like Not connected', () => {
    expect(statusLineFit({ ...LINE, room: FULL }).labels).toBe(true);
    expect(statusLineFit({ ...LINE, room: FULL, lead: 80 }).labels).toBe(false);
  });

  it('fits a line with no vitals and no clock', () => {
    const bare = { ...LINE, vitals: [], foe: null, tick: 0, time: 0, moons: 0 };
    expect(statusLineFit({ ...bare, room: 0 })).toEqual(FIT_ALL);
  });
});

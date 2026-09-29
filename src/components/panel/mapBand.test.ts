import { describe, expect, it } from 'vitest';
import { MAP_BAND_MAX_ROWS, mapBandPeople, mapBandRows } from './mapBand';

describe('mapBandRows', () => {
  it('fills what the drawing leaves above its floor, up to three rows', () => {
    // A tall pane: the drawing keeps the rest.
    expect(MAP_BAND_MAX_ROWS).toBe(3);
    expect(mapBandRows(480, 96, 22)).toBe(3);
    // Room for two rows over the floor, with a pixel to spare.
    expect(mapBandRows(96 + 45, 96, 22)).toBe(2);
    expect(mapBandRows(96 + 44, 96, 22)).toBe(2);
    expect(mapBandRows(96 + 43, 96, 22)).toBe(1);
  });

  it('keeps the room row in a pane at its floor or below', () => {
    expect(mapBandRows(96 + 22, 96, 22)).toBe(1);
    expect(mapBandRows(96, 96, 22)).toBe(1);
    expect(mapBandRows(40, 96, 22)).toBe(1);
    expect(mapBandRows(480, 96, 0)).toBe(1);
  });
});

describe('mapBandPeople', () => {
  const people = ['guard', 'Tarvik', 'merchant', 'beggar', 'Selune'];

  it('shows everyone who fits', () => {
    expect(mapBandPeople(people.slice(0, 3), 3)).toEqual({
      shown: ['guard', 'Tarvik', 'merchant'],
      rest: [],
    });
    expect(mapBandPeople([], 3)).toEqual({ shown: [], rest: [] });
  });

  it('counts the rest on the last slot', () => {
    expect(mapBandPeople(people, 3)).toEqual({
      shown: ['guard', 'Tarvik'],
      rest: ['merchant', 'beggar', 'Selune'],
    });
    expect(mapBandPeople(people, 1)).toEqual({ shown: [], rest: people });
  });

  it('shows no people when only the room row fits', () => {
    expect(mapBandPeople(people, 0)).toEqual({ shown: [], rest: [] });
  });
});

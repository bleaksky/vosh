import { describe, expect, it } from 'vitest';
import { SECTORS, sectorForCode, sectorIndex } from './mapPalette';

describe('sectorForCode', () => {
  it('colors desert, lava and snow as Map.Tiles sends them, as numbers', () => {
    // gmcp.c writes the sector as "s":%d, so these arrive as 10, 11 and
    // 12 and read as "10", "11" and "12". They used to fall back to the
    // Inside sector and draw grey.
    expect(sectorForCode('10')).toBe(SECTORS[10]);
    expect(sectorForCode('11')).toBe(SECTORS[11]);
    expect(sectorForCode('12')).toBe(SECTORS[12]);
  });

  it('keeps the letter codes and the digits', () => {
    expect(sectorForCode('a')).toBe(SECTORS[10]);
    expect(sectorForCode('b')).toBe(SECTORS[11]);
    expect(sectorForCode('c')).toBe(SECTORS[12]);
    expect(sectorForCode('1')).toBe(SECTORS[1]);
    expect(sectorForCode('0')).toBe(SECTORS[0]);
  });

  it('falls back to Inside for a code it does not know', () => {
    expect(sectorForCode('13')).toBe(SECTORS[0]);
    expect(sectorForCode(undefined)).toBe(SECTORS[0]);
  });
});

describe('sectorIndex', () => {
  it('gives the table index for a code, Inside for one it does not know', () => {
    expect(sectorIndex('0')).toBe(0);
    expect(sectorIndex('7')).toBe(7);
    expect(sectorIndex('12')).toBe(12);
    expect(sectorIndex('b')).toBe(11);
    expect(sectorIndex('13')).toBe(0);
    expect(sectorIndex('')).toBe(0);
  });
});

import { describe, expect, it } from 'vitest';
import { SECTORS, sectorForCode } from './mapPalette';

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

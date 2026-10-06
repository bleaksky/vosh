import { describe, expect, it } from 'vitest';
import { parseHex } from '../../theme/color';
import { SECTORS, roomFill, sectorForCode, sectorIndex } from './mapPalette';

describe('SECTORS', () => {
  // The glyphs and the Squares borders take a halo or a border at an
  // alpha through hexToRgba, which hands any other text back as it is.
  it('writes every color in hex', () => {
    for (const sector of Object.values(SECTORS)) {
      for (const color of [sector.fill, sector.border, sector.halo]) {
        expect(parseHex(color), `${sector.name} ${color}`).not.toBeNull();
      }
    }
  });
});

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

describe('roomFill', () => {
  it('mixes each sector border 22 percent into the paper in OKLab, as Chrome draws it', () => {
    // What Chrome paints for color-mix(in oklab, <border> 22%, #f7f4ee),
    // Vellum's panel, read back off a canvas, Inside through Snow.
    const paper = '#f7f4ee';
    expect(Object.values(SECTORS).map((sector) => roomFill(sector, paper, true))).toEqual([
      '#d2d0ce',
      '#e3dccd',
      '#d1dcc9',
      '#cad9c2',
      '#dedcc9',
      '#d2d0d0',
      '#ccd5dc',
      '#c8d1d8',
      '#ceccbf',
      '#d4dcdf',
      '#e6dcc7',
      '#eacbc1',
      '#e2e0dc',
    ]);
  });

  it('keeps the sector fill on a dark theme', () => {
    expect(roomFill(SECTORS[1], '#050403', false)).toBe('#28221a');
    expect(roomFill(SECTORS[0], '#050403', false)).toBe('#222228');
  });

  it('keeps the sector fill when the panel is not hex', () => {
    expect(roomFill(SECTORS[1], 'rgba(247, 244, 238, 1)', true)).toBe('#28221a');
  });
});

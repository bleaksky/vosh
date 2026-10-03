import { describe, expect, it } from 'vitest';
import { aabahranMapPacket } from '../test/aabahranGmcp';
import { SPRITE_SIZE, TERRAIN, paintSprite, spriteMean, spriteVariant } from './mapAtlas';
import { SECTORS } from './mapPalette';
import { getCell, gridDims, gridRooms, type MapTilesPayload } from './mapTiles';

describe('TERRAIN', () => {
  it('shades each of the thirteen sectors', () => {
    expect(TERRAIN).toHaveLength(Object.keys(SECTORS).length);
  });

  it('lifts each roof from the border toward the halo, as the atlas does', () => {
    // Inside, #5a5a64 toward #8888a0, three tenths of the way and
    // truncated.
    expect(TERRAIN[0].top).toEqual({ r: 103, g: 103, b: 118 });
    // City, #a08a5a toward #c4a872.
    expect(TERRAIN[1].top).toEqual({ r: 170, g: 147, b: 97 });
    expect(TERRAIN[1].border).toEqual({ r: 160, g: 138, b: 90 });
  });
});

describe('paintSprite', () => {
  it('paints the same opaque pixels each time, from a few ramp colors', () => {
    for (let sector = 0; sector < TERRAIN.length; sector++) {
      for (let variant = 0; variant < 3; variant++) {
        const px = paintSprite(sector, variant);
        expect(px).toHaveLength(SPRITE_SIZE * SPRITE_SIZE);
        expect(paintSprite(sector, variant)).toEqual(px);
        expect(px.every((p) => p >>> 24 === 255)).toBe(true);
        const colors = new Set(px).size;
        expect(colors, `sector ${sector}`).toBeGreaterThan(1);
        expect(colors, `sector ${sector}`).toBeLessThanOrEqual(5);
      }
    }
  });

  it('gives each variant its own look and paints an unknown sector as Inside', () => {
    expect(paintSprite(3, 0)).not.toEqual(paintSprite(3, 1));
    expect(paintSprite(99, 0)).toEqual(paintSprite(0, 0));
  });
});

describe('spriteVariant', () => {
  // You stand West of the City Fountain in Caranduin, then step west
  // onto The Common Road. The packets come from fixtures/gmcp/aabahran.
  const tiles = (name: string) => aabahranMapPacket(name).data as MapTilesPayload;
  const before = tiles('caranduin-west-of-the-fountain.gmcp');
  const after = tiles('caranduin-the-common-road.gmcp');

  it('keeps each room to its variant as you walk', () => {
    // Most rooms sit one cell further east after the step. A packet
    // names no room, so a room counts as the same one where it leads to
    // the same rooms.
    const { rows, cols } = gridDims(before);
    let kept = 0;
    for (const { row, col, cell } of gridRooms(before, rows, cols)) {
      const moved = getCell(after, row, col + 1);
      if (!moved || JSON.stringify(moved.ex) !== JSON.stringify(cell.ex)) continue;
      expect(spriteVariant(moved.ex), `${row}, ${col}`).toBe(spriteVariant(cell.ex));
      kept++;
    }
    expect(kept).toBeGreaterThan(60);
  });

  it('spreads the rooms of a packet over all three variants', () => {
    const { rows, cols } = gridDims(before);
    const seen = new Set(gridRooms(before, rows, cols).map((r) => spriteVariant(r.cell.ex)));
    expect([...seen].sort()).toEqual([0, 1, 2]);
    expect(spriteVariant(undefined)).toBe(0);
  });
});

describe('spriteMean', () => {
  it('averages the channels of the pixels', () => {
    // Red 10, green 20, blue 30, opaque, as ImageData packs it.
    const px = new Uint32Array(4).fill(((255 << 24) | (30 << 16) | (20 << 8) | 10) >>> 0);
    expect(spriteMean(px)).toEqual({ r: 10, g: 20, b: 30 });
  });
});

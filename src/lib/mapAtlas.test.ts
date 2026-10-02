import { describe, expect, it } from 'vitest';
import { SPRITE_SIZE, TERRAIN, paintSprite, spriteMean, spriteVariant } from './mapAtlas';
import { SECTORS } from './mapPalette';

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
  it('picks one of three variants for a spot, the same each time', () => {
    const seen = new Set<number>();
    for (let x = 0; x < 21; x++) {
      for (let y = 0; y < 21; y++) {
        const v = spriteVariant(x, y);
        expect(v).toBe(spriteVariant(x, y));
        seen.add(v);
      }
    }
    expect([...seen].sort()).toEqual([0, 1, 2]);
  });
});

describe('spriteMean', () => {
  it('averages the channels of the pixels', () => {
    // Red 10, green 20, blue 30, opaque, as ImageData packs it.
    const px = new Uint32Array(4).fill(((255 << 24) | (30 << 16) | (20 << 8) | 10) >>> 0);
    expect(spriteMean(px)).toEqual({ r: 10, g: 20, b: 30 });
  });
});

import { describe, expect, it } from 'vitest';
import {
  normalizeTextSize,
  offeredTextSizes,
  roundHalf,
  SCALABLE,
  snapTextSize,
  textSizeNote,
} from './textSize';

const WHOLE = { half_sizes: false, strikes: [] };
const STRIKES = { half_sizes: false, strikes: [12, 14, 16] };

describe('roundHalf', () => {
  it('puts a size on the nearest half step, halves up as Rust rounds them', () => {
    expect(roundHalf(13)).toBe(13);
    expect(roundHalf(13.5)).toBe(13.5);
    expect(roundHalf(13.3)).toBe(13.5);
    expect(roundHalf(13.2)).toBe(13);
    expect(roundHalf(13.75)).toBe(14);
    expect(roundHalf(13.25)).toBe(13.5);
  });
});

describe('normalizeTextSize', () => {
  it('reads whole and half sizes as they are', () => {
    expect(normalizeTextSize(13, 14)).toBe(13);
    expect(normalizeTextSize(13.5, 14)).toBe(13.5);
  });

  it('rounds an odd fraction and clamps to 6 to 64', () => {
    expect(normalizeTextSize(13.3, 14)).toBe(13.5);
    expect(normalizeTextSize(3, 14)).toBe(6);
    expect(normalizeTextSize(90.5, 14)).toBe(64);
    expect(normalizeTextSize(64.5, 14)).toBe(64);
  });

  it('keeps 0 only where it follows the terminal', () => {
    expect(normalizeTextSize(0, 12, true)).toBe(0);
    expect(normalizeTextSize(0.2, 12, true)).toBe(0);
    expect(normalizeTextSize(0.5, 12, true)).toBe(6);
    expect(normalizeTextSize(0, 14)).toBe(6);
  });

  it('reads anything but a number as the fallback', () => {
    for (const value of [undefined, null, '13.5', Number.NaN, Infinity]) {
      expect(normalizeTextSize(value, 14)).toBe(14);
    }
  });
});

describe('snapTextSize', () => {
  it('leaves any size alone for a font that scales', () => {
    expect(snapTextSize(13.5, SCALABLE)).toBe(13.5);
  });

  it('rounds to a whole size for a bitmap font that lists no strikes', () => {
    expect(snapTextSize(13.5, WHOLE)).toBe(14);
    expect(snapTextSize(13, WHOLE)).toBe(13);
  });

  it('moves to the nearest strike, the smaller one on a tie', () => {
    expect(snapTextSize(13.5, STRIKES)).toBe(14);
    expect(snapTextSize(13, STRIKES)).toBe(12);
    expect(snapTextSize(30, STRIKES)).toBe(16);
  });

  it('keeps 0, which follows the terminal', () => {
    expect(snapTextSize(0, STRIKES)).toBe(0);
    expect(snapTextSize(0, WHOLE)).toBe(0);
  });

  it('ignores strikes Vosh cannot save', () => {
    expect(snapTextSize(13.5, { half_sizes: false, strikes: [4, 109] })).toBe(14);
  });
});

describe('offeredTextSizes', () => {
  it('offers each half step from 11 to 18 for a font that scales', () => {
    const sizes = offeredTextSizes(SCALABLE);
    expect(sizes[0]).toBe(11);
    expect(sizes.at(-1)).toBe(18);
    expect(sizes).toHaveLength(15);
    for (let i = 1; i < sizes.length; i += 1) expect(sizes[i] - sizes[i - 1]).toBe(0.5);
  });

  it('offers whole sizes or the strikes for a bitmap font', () => {
    expect(offeredTextSizes(WHOLE)).toEqual([11, 12, 13, 14, 15, 16, 18]);
    expect(offeredTextSizes(STRIKES)).toEqual([12, 14, 16]);
  });
});

describe('textSizeNote', () => {
  it('says nothing for a font that scales', () => {
    expect(textSizeNote(SCALABLE)).toBe('');
  });

  it('names the sizes a bitmap font keeps to', () => {
    expect(textSizeNote(WHOLE)).toBe('This font comes in whole sizes only.');
    expect(textSizeNote({ half_sizes: false, strikes: [16] })).toBe(
      'This font comes in 16 pt only.',
    );
    expect(textSizeNote(STRIKES)).toBe('This font comes in 12, 14 and 16 pt only.');
  });
});

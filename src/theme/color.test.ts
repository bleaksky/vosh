import { describe, expect, it } from 'vitest';
import {
  composite,
  contrast,
  deltaE2000,
  deltaE2000Lab,
  luminance,
  oklchToRgb,
  oklchToRgbInGamut,
  parseHex,
  rgbToOklab,
  rgbToOklch,
  shiftLightness,
  solveAlphaForContrast,
  toHex,
  toRgba,
  WHITE,
  BLACK,
  type Lab,
} from './color';

const hex = (h: string) => {
  const c = parseHex(h);
  if (!c) throw new Error(`bad hex ${h}`);
  return c;
};

describe('parseHex and toHex', () => {
  it('reads long and short hex, with or without the hash', () => {
    expect(parseHex('#2e3440')).toEqual({ r: 46, g: 52, b: 64 });
    expect(parseHex('fff')).toEqual({ r: 255, g: 255, b: 255 });
    expect(parseHex('  #ABC ')).toEqual({ r: 170, g: 187, b: 204 });
  });

  it('rejects anything that is not 3 or 6 digit hex', () => {
    expect(parseHex('rgba(1, 2, 3, 0.5)')).toBeNull();
    expect(parseHex('#12345')).toBeNull();
    expect(parseHex('#11223344')).toBeNull();
    expect(parseHex('')).toBeNull();
  });

  it('rounds and clamps when formatting', () => {
    expect(toHex({ r: 12.6, g: -4, b: 300 })).toBe('#0d00ff');
    expect(toRgba({ r: 136, g: 192, b: 208 }, 0.22)).toBe('rgba(136, 192, 208, 0.22)');
  });
});

describe('OKLab and OKLCH', () => {
  it('matches the reference values for white and black', () => {
    expect(rgbToOklab(WHITE).L).toBeCloseTo(1, 4);
    expect(rgbToOklab(BLACK).L).toBeCloseTo(0, 6);
    expect(rgbToOklch(WHITE).C).toBeLessThan(1e-4);
  });

  it('round trips through OKLCH', () => {
    for (const h of ['#ef8f2f', '#3f6690', '#88c0d0', '#050403', '#f7f4ee']) {
      expect(toHex(oklchToRgb(rgbToOklch(hex(h))))).toBe(h);
    }
  });

  it('shifts lightness at the same hue', () => {
    const bg = hex('#050403');
    const lifted = shiftLightness(bg, 0.04);
    expect(toHex(lifted)).toBe('#0c0a08');
    expect(rgbToOklab(lifted).L - rgbToOklab(bg).L).toBeCloseTo(0.04, 2);
  });

  it('gives up chroma, not hue, to land inside sRGB', () => {
    for (const h of ['#ef8f2f', '#3f6690', '#88c0d0', '#050403', '#f7f4ee']) {
      expect(toHex(oklchToRgbInGamut(rgbToOklch(hex(h))))).toBe(h);
    }
    // Everforest yellow a step darker no longer fits sRGB at its chroma.
    // A clamp per channel turns it orange.
    const yellow = rgbToOklch(hex('#dfa000'));
    const deeper = { ...yellow, L: yellow.L - 0.12 };
    const clamped = rgbToOklch(oklchToRgb(deeper));
    const mapped = rgbToOklch(oklchToRgbInGamut(deeper));
    expect(yellow.h - clamped.h).toBeGreaterThan(3);
    expect(Math.abs(mapped.h - yellow.h)).toBeLessThan(0.01);
    expect(mapped.L).toBeCloseTo(deeper.L, 4);
    expect(mapped.C).toBeLessThan(yellow.C);
  });
});

describe('contrast', () => {
  it('follows WCAG 2', () => {
    expect(luminance(WHITE)).toBeCloseTo(1, 6);
    expect(contrast(WHITE, BLACK)).toBeCloseTo(21, 6);
    expect(contrast(BLACK, WHITE)).toBeCloseTo(21, 6);
    expect(contrast(hex('#777777'), WHITE)).toBeCloseTo(4.48, 2);
  });

  it('composites like the browser', () => {
    expect(toHex(composite(WHITE, hex('#2e3440'), 0.045))).toBe('#373d49');
    expect(toHex(composite(BLACK, hex('#f0ede7'), 0.09))).toBe('#dad8d2');
  });

  it('solves the smallest alpha that reaches a contrast target', () => {
    const fg = hex('#c0bdbb');
    const bg = hex('#0c0a08');
    const { alpha, color } = solveAlphaForContrast(fg, bg, 6);
    expect(contrast(color, bg)).toBeGreaterThanOrEqual(6);
    expect(contrast(composite(fg, bg, alpha - 0.01), bg)).toBeLessThan(6);
  });

  it('falls back to the full color when the target is out of reach', () => {
    const fg = hex('#444444');
    expect(solveAlphaForContrast(fg, BLACK, 21)).toEqual({ alpha: 1, color: fg });
  });
});

describe('deltaE2000', () => {
  it('is zero for equal colors and symmetric', () => {
    expect(deltaE2000(hex('#88c0d0'), hex('#88c0d0'))).toBe(0);
    const a = hex('#ef8f2f');
    const b = hex('#e5c057');
    expect(deltaE2000(a, b)).toBeCloseTo(deltaE2000(b, a), 10);
  });

  it('matches the Sharma reference data', () => {
    // Pairs 1, 7, 17 and 25 from Sharma, Wu and Dalal (2005).
    const pairs: [Lab, Lab, number][] = [
      [{ L: 50, a: 2.6772, b: -79.7751 }, { L: 50, a: 0, b: -82.7485 }, 2.0425],
      [{ L: 50, a: 0, b: 0 }, { L: 50, a: -1, b: 2 }, 2.3669],
      [{ L: 50, a: 2.5, b: 0 }, { L: 73, a: 25, b: -18 }, 27.1492],
      [{ L: 60.2574, a: -34.0099, b: 36.2677 }, { L: 60.4626, a: -34.1751, b: 39.4387 }, 1.2644],
    ];
    for (const [p, q, expected] of pairs) {
      expect(deltaE2000Lab(p, q)).toBeCloseTo(expected, 4);
    }
  });
});

import { describe, expect, it } from 'vitest';
import { clampWhole, readNumberText } from './numberText';

describe('clampWhole', () => {
  it('rounds and clamps', () => {
    expect(clampWhole(494.4, 200, 800)).toBe(494);
    expect(clampWhole(120, 200, 800)).toBe(200);
    expect(clampWhole(1200, 200, 800)).toBe(800);
  });
});

describe('readNumberText', () => {
  it('reads a whole number inside the bounds', () => {
    expect(readNumberText('494', 200, 800)).toBe(494);
    expect(readNumberText(' 500 ', 0, 10_000)).toBe(500);
  });

  it('clamps to the bounds', () => {
    expect(readNumberText('50', 200, 800)).toBe(200);
    expect(readNumberText('20000', 0, 10_000)).toBe(10_000);
    expect(readNumberText('-5', 0, 10_000)).toBe(0);
  });

  it('rounds a fraction', () => {
    expect(readNumberText('300.6', 200, 800)).toBe(301);
  });

  it('takes a unit typed after the number', () => {
    expect(readNumberText('500 ms', 0, 10_000)).toBe(500);
    expect(readNumberText('320pt', 200, 800)).toBe(320);
  });

  it('refuses text that is not a number', () => {
    expect(readNumberText('', 0, 10)).toBeNull();
    expect(readNumberText('wide', 0, 10)).toBeNull();
    expect(readNumberText('5 5', 0, 10)).toBeNull();
  });
});

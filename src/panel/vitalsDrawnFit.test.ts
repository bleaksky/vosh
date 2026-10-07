import { describe, expect, it } from 'vitest';
import { FOE_LADDER, LADDER, litSegments, rowMarkFit } from './vitalsDrawnFit';
import type { MeasureText } from './vitalsLedgerFit';

/** A face with tabular digits 0.62 em wide, near the system face. */
const MEASURE: MeasureText = (text, px) => {
  let em = 0;
  for (const ch of text.replace(/[0-9]/g, '0')) {
    em += ch === '0' ? 0.62 : ch === ' ' ? 0.28 : ch === '/' ? 0.32 : ch === '%' ? 0.85 : 0.5;
  }
  return Math.ceil(em * px);
};

// Board 1's labels and maxes, with the guard at 100 percent.
const LABELS = ['Health', 'Mana', 'Moves'];
const VALUES = ['1038 / 1038', '870 / 870', '521 / 521', '100%'];

describe('litSegments', () => {
  it('lights each segment once the vital reaches its share', () => {
    expect(litSegments(78, LADDER)).toBe(19);
    expect(litSegments(100, LADDER)).toBe(24);
    expect(litSegments(0, LADDER)).toBe(0);
    expect(litSegments(54, FOE_LADDER)).toBe(26);
  });

  it('lights none for a value the game hides', () => {
    expect(litSegments(null, LADDER)).toBe(0);
  });
});

describe('rowMarkFit', () => {
  it('keeps the mark beside the label and value at 300 pt', () => {
    expect(rowMarkFit(300, 12, LABELS, VALUES, MEASURE)).toBe('beside');
    expect(rowMarkFit(300, 16, LABELS, VALUES, MEASURE)).toBe('beside');
  });

  it('drops the mark under the label and value at 200 pt, as board 5 draws', () => {
    expect(rowMarkFit(200, 12, LABELS, VALUES, MEASURE)).toBe('under');
  });

  it('keeps a mark of 72 at least', () => {
    // Health is 36 wide and 1038 / 1038 71 at 12 px, so the insets, the
    // label, the value and two gaps of 10 leave exactly 72 at 229.
    expect(rowMarkFit(229, 12, LABELS, VALUES, MEASURE)).toBe('beside');
    expect(rowMarkFit(228, 12, LABELS, VALUES, MEASURE)).toBe('under');
  });

  it('fits nothing while it draws your opponent alone', () => {
    expect(rowMarkFit(200, 16, [], ['100%'], MEASURE)).toBe('beside');
  });
});

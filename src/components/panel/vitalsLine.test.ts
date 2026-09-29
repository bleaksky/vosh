import { describe, expect, it } from 'vitest';
import { VITALS_LINE_LABEL_MIN_WIDTH, vitalsLineFit, type VitalsLineItem } from './vitalsLine';

// Widths SF 12 draws, measured in the page: the labels at 400, the
// values at 500 with tabular digits.
const labels = { Health: 37, Mana: 31, Moves: 37 };

// Erelei at full: 1020 / 1020, 800 / 800, 930 / 930.
const ERELEI: VitalsLineItem[] = [
  { label: labels.Health, value: 73 },
  { label: labels.Mana, value: 57 },
  { label: labels.Moves, value: 57 },
];

// A newer character: 300 / 300, 90 / 90, 120 / 120.
const NEWER: VitalsLineItem[] = [
  { label: labels.Health, value: 50 },
  { label: labels.Mana, value: 36 },
  { label: labels.Moves, value: 43 },
];

describe('vitalsLineFit', () => {
  it('shows the labels from 360 pt when they fit', () => {
    expect(VITALS_LINE_LABEL_MIN_WIDTH).toBe(360);
    expect(vitalsLineFit(360, NEWER)).toBe('labels');
    expect(vitalsLineFit(359, NEWER)).toBe('values');
  });

  it('drops the labels until they fit beside longer values', () => {
    // 30 of padding, two 16 gaps, three labels 6 before their values.
    expect(vitalsLineFit(371, ERELEI)).toBe('values');
    expect(vitalsLineFit(372, ERELEI)).toBe('labels');
    expect(vitalsLineFit(494, ERELEI)).toBe('labels');
  });

  it('keeps the values on a narrow panel', () => {
    expect(vitalsLineFit(300, ERELEI)).toBe('values');
    expect(vitalsLineFit(249, ERELEI)).toBe('values');
  });

  it('stacks the vitals in rows when even the values do not fit', () => {
    expect(vitalsLineFit(248, ERELEI)).toBe('rows');
    expect(vitalsLineFit(200, ERELEI)).toBe('rows');
    expect(vitalsLineFit(0, ERELEI)).toBe('rows');
  });

  it('makes room for two vitals when the MUD sends no moves', () => {
    expect(vitalsLineFit(200, ERELEI.slice(0, 2))).toBe('values');
  });
});

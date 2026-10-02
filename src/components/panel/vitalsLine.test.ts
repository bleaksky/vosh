import { describe, expect, it } from 'vitest';
import { vitalsLineFit, type VitalsLineItem } from './vitalsLine';

// Widths SF 12 draws, measured in the page: the labels at 400, the
// values at 500 with tabular digits.
const labels = { Health: 37, Mana: 31, Moves: 37 };

// Ilsabet at full: 1020 / 1020, 800 / 800, 930 / 930.
const ILSABET: VitalsLineItem[] = [
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

// Ilsabet at full under Values Current (1020, 800, 930) and Percent
// (100% three times).
const CURRENT: VitalsLineItem[] = [
  { label: labels.Health, value: 29 },
  { label: labels.Mana, value: 22 },
  { label: labels.Moves, value: 22 },
];
const PERCENT: VitalsLineItem[] = [
  { label: labels.Health, value: 32 },
  { label: labels.Mana, value: 32 },
  { label: labels.Moves, value: 32 },
];

describe('vitalsLineFit', () => {
  it('shows the labels whenever they fit beside the values', () => {
    // 30 of padding, two 16 gaps, three labels 6 before their values.
    expect(vitalsLineFit(314, NEWER)).toBe('labels');
    expect(vitalsLineFit(313, NEWER)).toBe('values');
  });

  it('keeps the labels at 300 pt for Current and Percent', () => {
    expect(vitalsLineFit(300, CURRENT)).toBe('labels');
    expect(vitalsLineFit(300, PERCENT)).toBe('labels');
    expect(vitalsLineFit(300, ILSABET)).toBe('values');
  });

  it('drops the labels until they fit beside longer values', () => {
    // 30 of padding, two 16 gaps, three labels 6 before their values.
    expect(vitalsLineFit(371, ILSABET)).toBe('values');
    expect(vitalsLineFit(372, ILSABET)).toBe('labels');
    expect(vitalsLineFit(494, ILSABET)).toBe('labels');
  });

  it('keeps the values on a narrow panel', () => {
    expect(vitalsLineFit(300, ILSABET)).toBe('values');
    expect(vitalsLineFit(249, ILSABET)).toBe('values');
  });

  it('stacks the vitals in rows when even the values do not fit', () => {
    expect(vitalsLineFit(248, ILSABET)).toBe('rows');
    expect(vitalsLineFit(200, ILSABET)).toBe('rows');
    expect(vitalsLineFit(0, ILSABET)).toBe('rows');
  });

  it('makes room for two vitals when the MUD sends no moves', () => {
    expect(vitalsLineFit(200, ILSABET.slice(0, 2))).toBe('values');
  });
});

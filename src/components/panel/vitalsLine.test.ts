import { describe, expect, it } from 'vitest';
import { VITALS_LINE_LABEL_MIN_WIDTH, vitalsLineShowsLabels } from './vitalsLine';

describe('vitalsLineShowsLabels', () => {
  it('drops the labels below about 360 pt', () => {
    expect(VITALS_LINE_LABEL_MIN_WIDTH).toBe(360);
    expect(vitalsLineShowsLabels(200)).toBe(false);
    expect(vitalsLineShowsLabels(300)).toBe(false);
    expect(vitalsLineShowsLabels(359)).toBe(false);
  });

  it('keeps them from 360 up', () => {
    expect(vitalsLineShowsLabels(360)).toBe(true);
    expect(vitalsLineShowsLabels(494)).toBe(true);
    expect(vitalsLineShowsLabels(800)).toBe(true);
  });

  it('drops them before the panel has a width', () => {
    expect(vitalsLineShowsLabels(0)).toBe(false);
  });
});

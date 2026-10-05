import { describe, expect, it } from 'vitest';
import { listJoin } from './text';

describe('listJoin', () => {
  it('reads nothing as an empty string and one item as itself', () => {
    expect(listJoin([])).toBe('');
    expect(listJoin(['Orla'])).toBe('Orla');
  });

  it('joins two items with and', () => {
    expect(listJoin(['Maren', 'Orla'])).toBe('Maren and Orla');
  });

  it('puts a serial comma before the last of three or more', () => {
    expect(listJoin(['Tolliver', 'Maren', 'Orla'])).toBe('Tolliver, Maren, and Orla');
    expect(listJoin(['Tolliver', 'Maren', 'Orla', 'the tick'])).toBe(
      'Tolliver, Maren, Orla, and the tick',
    );
  });
});

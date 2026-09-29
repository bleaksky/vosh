import { describe, expect, it } from 'vitest';
import { parseCombat } from './combatStore';

describe('parseCombat', () => {
  it('reads the Aabahran payload', () => {
    expect(
      parseCombat({
        target: 'a member of the Blackwatch Guard',
        condition: 'big nasty wounds',
        hp_pct: 38,
      }),
    ).toEqual({
      name: 'a member of the Blackwatch Guard',
      hp_pct: 38,
      condition: 'big nasty wounds',
    });
  });

  it('reads the empty payload as no fight', () => {
    expect(parseCombat({})).toBeNull();
    expect(parseCombat({ target: '  ' })).toBeNull();
    expect(parseCombat(null)).toBeNull();
  });

  it('clamps health and tolerates missing fields', () => {
    expect(parseCombat({ target: 'rat', hp_pct: '-4' })).toEqual({
      name: 'rat',
      hp_pct: 0,
      condition: null,
    });
    expect(parseCombat({ target: 'rat', hp_pct: 140.6 })?.hp_pct).toBe(100);
    expect(parseCombat({ target: 'rat' })?.hp_pct).toBeNull();
  });
});

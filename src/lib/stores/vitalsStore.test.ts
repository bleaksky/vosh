import { describe, expect, it } from 'vitest';
import { aabahranPacket } from '../../test/aabahranGmcp';
import {
  mergeVitals,
  nextLow,
  nextVitals,
  parseVitals,
  parseVitalsPacket,
  type VitalValues,
} from './vitalsStore';

const full: VitalValues = {
  hp: 1020,
  maxhp: 1020,
  mana: 800,
  maxmana: 800,
  move: 930,
  maxmove: 930,
};

describe('parseVitals', () => {
  it('reads numbers and numeric strings, zero for the rest', () => {
    expect(parseVitals({ hp: 186, maxhp: '1020', mana: 'x' })).toEqual({
      hp: 186,
      maxhp: 1020,
      mana: 0,
      maxmana: 0,
      move: 0,
      maxmove: 0,
    });
    expect(parseVitals(null).maxhp).toBe(0);
  });
});

describe('parseVitalsPacket', () => {
  it('reads the Aabahran packet as shown', () => {
    expect(parseVitalsPacket(aabahranPacket('char-vitals.gmcp').data)).toEqual({
      values: { hp: 850, maxhp: 900, mana: 760, maxmana: 820, move: 250, maxmove: 250 },
      hidden: false,
    });
  });

  it('reads the lamented tears packet as hidden', () => {
    expect(parseVitalsPacket(aabahranPacket('char-vitals-hidden.gmcp').data)).toEqual({
      values: { hp: 0, maxhp: 0, mana: 0, maxmana: 0, move: 0, maxmove: 0 },
      hidden: true,
    });
    // Only a true flag hides.
    expect(parseVitalsPacket({ hp: 1, maxhp: 2, hidden: false }).hidden).toBe(false);
    expect(parseVitalsPacket({ hp: 1, maxhp: 2, hidden: 'yes' }).hidden).toBe(false);
  });
});

describe('mergeVitals', () => {
  it('is null before any source reports a max', () => {
    expect(mergeVitals(null, {})).toBeNull();
    expect(mergeVitals(null, { hp: '10' })).toBeNull();
  });

  it('lays prompt vars over the GMCP values', () => {
    expect(mergeVitals(full, { hp: '186', maxmana: 'oops' })).toEqual({ ...full, hp: 186 });
  });

  it('builds vitals from prompt vars alone', () => {
    expect(mergeVitals(null, { hp: '50', maxhp: '100' })).toEqual({
      hp: 50,
      maxhp: 100,
      mana: 0,
      maxmana: 0,
      move: 0,
      maxmove: 0,
    });
  });
});

describe('low latch', () => {
  it('enters under 20 percent and leaves at 25', () => {
    expect(nextLow(false, 20, 100)).toBe(false);
    expect(nextLow(false, 19, 100)).toBe(true);
    expect(nextLow(true, 24, 100)).toBe(true);
    expect(nextLow(true, 25, 100)).toBe(false);
  });

  it('never marks a vital with no max as low', () => {
    expect(nextLow(true, 0, 0)).toBe(false);
  });

  it('carries the latch from one snapshot to the next', () => {
    const hurt = nextVitals(null, { ...full, hp: 186 });
    expect(hurt?.low).toEqual({ hp: true, mana: false, move: false });
    const regen = nextVitals(hurt, { ...full, hp: 230 });
    expect(regen?.low.hp).toBe(true);
    const healed = nextVitals(regen, { ...full, hp: 260 });
    expect(healed?.low.hp).toBe(false);
  });

  it('returns the same snapshot when nothing changed', () => {
    const first = nextVitals(null, full);
    expect(nextVitals(first, { ...full })).toBe(first);
    expect(nextVitals(first, null)).toBeNull();
  });
});

describe('hidden vitals', () => {
  const zeros: VitalValues = { hp: 0, maxhp: 0, mana: 0, maxmana: 0, move: 0, maxmove: 0 };
  const notLow = { hp: false, mana: false, move: false };

  it('never read low, even from a low latch', () => {
    const hurt = nextVitals(null, { ...full, hp: 186 });
    expect(hurt?.low.hp).toBe(true);
    const hidden = nextVitals(hurt, zeros, true);
    expect(hidden).toEqual({ ...zeros, low: notLow, hidden: true });
    expect(nextVitals(hidden, { ...zeros }, true)).toBe(hidden);
  });

  it('show again on the next packet without the flag', () => {
    const hidden = nextVitals(null, zeros, true);
    expect(nextVitals(hidden, full)).toEqual({ ...full, low: notLow, hidden: false });
  });

  it('tell hidden zeros from shown zeros', () => {
    const shown = nextVitals(null, zeros);
    expect(shown?.hidden).toBe(false);
    expect(nextVitals(shown, zeros, true)?.hidden).toBe(true);
  });
});

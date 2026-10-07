import { describe, expect, it } from 'vitest';
import latch from '../../../fixtures/alerts/low-latch.json';
import { aabahranPacket } from '../../test/aabahranGmcp';
import {
  holdPromptVitals,
  mergeVitals,
  nextHistory,
  nextLow,
  nextVitals,
  parseHistory,
  parseVitals,
  parseVitalsPacket,
  releasePromptVitals,
  VITALS_HISTORY,
  withoutHeld,
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

describe('held prompt vars', () => {
  const lament = { hp: '0', maxhp: '0', mana: '0', maxmana: '0', move: '0', maxmove: '0' };

  it('hold the six vitals the prompt gives and nothing else', () => {
    expect(holdPromptVitals({ ...lament, target: 'guard' })).toEqual(lament);
    expect(holdPromptVitals({ hp: '0', move: '0' })).toEqual({ hp: '0', move: '0' });
    expect(holdPromptVitals({})).toEqual({});
  });

  it('let go of each var the prompt sets to a new value', () => {
    const held = holdPromptVitals(lament);
    expect(releasePromptVitals(held, { ...lament })).toBe(held);
    expect(releasePromptVitals(held, { ...lament, hp: '850' })).toEqual({
      maxhp: '0',
      mana: '0',
      maxmana: '0',
      move: '0',
      maxmove: '0',
    });
    // A var the prompt no longer sets is let go too.
    expect(releasePromptVitals({ hp: '0' }, {})).toEqual({});
  });

  it('leave the held vars out of the merge', () => {
    const vars = { ...lament, hp: '850', target: 'guard' };
    expect(withoutHeld(vars, {})).toBe(vars);
    expect(withoutHeld(vars, { maxhp: '0', mana: '0' })).toEqual({
      hp: '850',
      maxmana: '0',
      move: '0',
      maxmove: '0',
      target: 'guard',
    });
    expect(mergeVitals(full, withoutHeld(lament, holdPromptVitals(lament)))).toEqual(full);
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

// fixtures/alerts/low-latch.json holds this latch to its Rust twin, which
// rings the Low health alert (next_low in src-tauri/src/alert/presets.rs).
describe('the low latch the Low health alert shares', () => {
  it('steps as the shared cases say', () => {
    for (const step of latch.steps) {
      expect(nextLow(step.was, step.current, step.max), JSON.stringify(step)).toBe(step.low);
    }
  });

  it('runs through hidden vitals as the shared cases say', () => {
    for (const run of latch.runs) {
      let prev: ReturnType<typeof nextVitals> = null;
      for (const v of run.vitals) {
        prev = nextVitals(prev, { ...full, hp: v.hp, maxhp: v.maxhp }, v.hidden);
        expect(prev?.low.hp, JSON.stringify(v)).toBe(v.low);
      }
    }
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

describe('the vitals history', () => {
  it('keeps the last sixty packets with their times, oldest first', () => {
    let history = nextHistory([], full, 1000);
    for (let n = 1; n <= VITALS_HISTORY; n++)
      history = nextHistory(history, { ...full, hp: n }, 1000 + n);
    expect(history).toHaveLength(VITALS_HISTORY);
    expect(history[0]).toEqual({ at: 1001, values: { ...full, hp: 1 } });
    expect(history[VITALS_HISTORY - 1]?.at).toBe(1000 + VITALS_HISTORY);
  });

  it('reads the history a snapshot carries and skips what it cannot', () => {
    expect(
      parseHistory([
        { at: 5, vitals: { hp: 744, maxhp: 1038 } },
        { at: 'soon', vitals: { hp: 1 } },
        null,
      ]),
    ).toEqual([{ at: 5, values: { ...parseVitals({ hp: 744, maxhp: 1038 }) } }]);
    expect(parseHistory(undefined)).toEqual([]);
  });
});

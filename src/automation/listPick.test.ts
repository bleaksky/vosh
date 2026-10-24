import { describe, expect, it } from 'vitest';
import {
  allPick,
  groupPick,
  NO_PICK,
  pickState,
  prunePick,
  rangePick,
  startPick,
  togglePick,
} from './listPick';

const ORDER = ['a', 'b', 'c', 'd', 'e'];

describe('picking rows', () => {
  it('starts with the row you were on checked', () => {
    expect(startPick('b')).toEqual({ on: true, uids: ['b'], anchor: 'b' });
    expect(startPick(null)).toEqual({ on: true, uids: [], anchor: null });
  });

  it('checks the row you were on and the one a Cmd click names', () => {
    const p = togglePick(NO_PICK, 'd', 'b');
    expect(p).toEqual({ on: true, uids: ['b', 'd'], anchor: 'd' });
    expect(togglePick(p, 'b', 'd').uids).toEqual(['d']);
  });

  it('checks the run from the anchor, and a later Shift click shrinks it', () => {
    const p = rangePick(NO_PICK, 'd', ORDER, 'b');
    expect(p).toEqual({ on: true, uids: ['b', 'c', 'd'], anchor: 'b' });
    expect(rangePick(p, 'a', ORDER, 'd').uids).toEqual(['a', 'b']);
    expect(rangePick(p, 'c', ORDER, 'd').uids).toEqual(['b', 'c']);
  });

  it('starts a run at the row clicked when the anchor is out of sight', () => {
    const p = { on: true, uids: ['z'], anchor: 'z' };
    expect(rangePick(p, 'c', ORDER, 'c')).toEqual({ on: true, uids: ['c'], anchor: 'c' });
    expect(rangePick(p, 'q', ORDER, 'c')).toBe(p);
  });

  it('checks and clears a whole group from its heading', () => {
    const p = startPick('a');
    const checked = groupPick(p, ['c', 'd'], true);
    expect(checked.uids).toEqual(['a', 'c', 'd']);
    expect(groupPick(checked, ['c', 'd'], false).uids).toEqual(['a']);
    expect(groupPick(NO_PICK, ['c'], true)).toEqual({ on: true, uids: ['c'], anchor: 'c' });
  });

  it('checks every row you can see', () => {
    expect(allPick(ORDER)).toEqual({ on: true, uids: ORDER, anchor: 'a' });
  });

  it('drops rows that went, and keeps the same pick when none did', () => {
    const p = { on: true, uids: ['a', 'b', 'c'], anchor: 'b' };
    expect(prunePick(p, () => true)).toBe(p);
    expect(prunePick(p, (u) => u !== 'b')).toEqual({ on: true, uids: ['a', 'c'], anchor: 'a' });
  });

  it('says whether a group is all, some or none checked', () => {
    const checked = new Set(['a', 'b']);
    expect(pickState(checked, ['a', 'b'])).toBe('all');
    expect(pickState(checked, ['b', 'c'])).toBe('some');
    expect(pickState(checked, ['c'])).toBe('none');
  });
});

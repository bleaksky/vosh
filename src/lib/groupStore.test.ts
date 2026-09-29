import { describe, expect, it } from 'vitest';
import { dedupeMembers, memberKey } from './groupStore';

describe('dedupeMembers', () => {
  it('keeps masked members apart when their ids differ', () => {
    const info = {
      leader: 'Erelei',
      members: [
        { id: 1, name: 'Erelei', hp_pct: 100 },
        { id: 2, name: 'someone', hp_pct: 76 },
        { id: 3, name: 'someone', hp_pct: 41 },
      ],
    };
    expect(dedupeMembers(info)).toBe(info);
  });

  it('folds repeats of one id into the last row, in first seen order', () => {
    const out = dedupeMembers({
      members: [
        { id: 2, name: 'someone', hp_pct: 90 },
        { id: 1, name: 'Erelei', hp_pct: 100 },
        { id: 2, name: 'someone', hp_pct: 40 },
      ],
    });
    expect(out.members).toEqual([
      { id: 2, name: 'someone', hp_pct: 40 },
      { id: 1, name: 'Erelei', hp_pct: 100 },
    ]);
  });

  it('falls back to the name when the server sends no id', () => {
    const out = dedupeMembers({
      members: [
        { name: 'Tarvik', hp_pct: 90 },
        { name: 'Tarvik', hp_pct: 70 },
      ],
    });
    expect(out.members).toEqual([{ name: 'Tarvik', hp_pct: 70 }]);
  });

  it('keys by id, then name', () => {
    expect(memberKey({ id: 7, name: 'x' })).toBe('id:7');
    expect(memberKey({ id: '', name: 'x' })).toBe('name:x');
    expect(memberKey({})).toBe('name:?');
  });
});

import { describe, expect, it } from 'vitest';
import { aabahranPacket } from '../../test/aabahranGmcp';
import { dedupeMembers, memberKey, parseGroupInfo } from './groupStore';

describe('parseGroupInfo', () => {
  it('reads the Aabahran roster', () => {
    const info = parseGroupInfo(aabahranPacket('group-info.gmcp').data);
    expect(info.hidden).toBeUndefined();
    expect(info.leader).toBe('Tester');
    expect(info.members?.map((m) => [m.name, m.hp_pct])).toEqual([
      ['Tester', 78],
      ['a loyal wolf', 91],
    ]);
  });

  it('reads solo as an empty group', () => {
    expect(parseGroupInfo(aabahranPacket('group-info-solo.gmcp').data)).toEqual({});
    expect(parseGroupInfo(null)).toEqual({});
    expect(parseGroupInfo('x')).toEqual({});
  });

  it('reads the lamented tears group as hidden with no roster', () => {
    expect(parseGroupInfo(aabahranPacket('group-info-hidden.gmcp').data)).toEqual({
      hidden: true,
    });
    // A roster that rides a hidden packet never shows.
    expect(
      parseGroupInfo({ hidden: true, leader: 'Tester', members: [{ name: 'Tester', hp_pct: 5 }] }),
    ).toEqual({ hidden: true });
  });
});

describe('dedupeMembers', () => {
  it('keeps masked members apart when their ids differ', () => {
    const info = {
      leader: 'Ilsabet',
      members: [
        { id: 1, name: 'Ilsabet', hp_pct: 100 },
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
        { id: 1, name: 'Ilsabet', hp_pct: 100 },
        { id: 2, name: 'someone', hp_pct: 40 },
      ],
    });
    expect(out.members).toEqual([
      { id: 2, name: 'someone', hp_pct: 40 },
      { id: 1, name: 'Ilsabet', hp_pct: 100 },
    ]);
  });

  it('falls back to the name when the server sends no id', () => {
    const out = dedupeMembers({
      members: [
        { name: 'Dovic', hp_pct: 90 },
        { name: 'Dovic', hp_pct: 70 },
      ],
    });
    expect(out.members).toEqual([{ name: 'Dovic', hp_pct: 70 }]);
  });

  it('keys by id, then name', () => {
    expect(memberKey({ id: 7, name: 'x' })).toBe('id:7');
    expect(memberKey({ id: '', name: 'x' })).toBe('name:x');
    expect(memberKey({})).toBe('name:?');
  });
});

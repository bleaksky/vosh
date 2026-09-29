import { describe, expect, it } from 'vitest';
import { groupCurrentAffects } from './affectsStore';

describe('groupCurrentAffects', () => {
  it('folds the Aabahran rows into one row per name', () => {
    const rows = groupCurrentAffects({
      affects: [
        {
          kind: 'spell',
          name: 'bless',
          duration: 6,
          level: 30,
          location: 'hit roll',
          modifier: 3,
        },
        {
          kind: 'spell',
          name: 'bless',
          duration: 6,
          level: 30,
          location: 'save vs spell',
          modifier: -3,
        },
        { kind: 'song', name: 'ballad of valor', duration: -1, level: 20, location: 'none' },
      ],
    });
    expect(rows).toEqual([
      {
        name: 'bless',
        kind: 'spell',
        duration: 6,
        level: 30,
        modifiers: [
          { location: 'hit roll', modifier: 3 },
          { location: 'save vs spell', modifier: -3 },
        ],
      },
      { name: 'ballad of valor', kind: 'song', duration: -1, level: 20, modifiers: [] },
    ]);
  });

  it('keeps the longest duration among rows of one name', () => {
    const rows = groupCurrentAffects({
      affects: [
        { name: 'haste', duration: 3 },
        { name: 'haste', duration: 9 },
        { name: 'armor', duration: 4 },
        { name: 'armor', duration: -1 },
        { name: 'armor', duration: 12 },
        { name: 'fly' },
        { name: 'fly', duration: '5' },
      ],
    });
    expect(rows.map((r) => [r.name, r.duration])).toEqual([
      ['haste', 9],
      ['armor', -1],
      ['fly', 5],
    ]);
  });

  it('accepts a bare array and skips rows without a name', () => {
    const rows = groupCurrentAffects([{ name: ' ' }, null, { name: 'sanctuary', duration: 12 }]);
    expect(rows.map((r) => [r.name, r.kind, r.duration])).toEqual([['sanctuary', null, 12]]);
  });

  it('reads an empty or malformed payload as no affects', () => {
    expect(groupCurrentAffects({})).toEqual([]);
    expect(groupCurrentAffects({ affects: 'x' })).toEqual([]);
    expect(groupCurrentAffects(null)).toEqual([]);
  });
});

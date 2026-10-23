import { describe, expect, it } from 'vitest';
import { groupChoices, moveClashes, moveClashNote, sharedGroup } from './groupMoves';

interface Item {
  name: string;
  group?: string;
}

const regroup = (v: Item, group: string): Item => (group ? { ...v, group } : { name: v.name });
const keyOf = (v: Item) => `${v.group ?? ''}\u001f${v.name}`;
const nameOf = (v: Item) => v.name;
const ALIASES = { one: 'alias', many: 'aliases' };

describe('moving items to a group', () => {
  const values: Item[] = [
    { name: 'ds', group: 'Maren' },
    { name: 'res', group: 'Maren' },
    { name: 'ds', group: 'Orla' },
    { name: 'res' },
    { name: 'heal', group: 'Orla' },
  ];

  it('names what two items would share after the move, each once', () => {
    expect(moveClashes(values, new Set([2, 3, 4]), 'Maren', regroup, keyOf, nameOf)).toEqual([
      { name: 'ds', count: 2 },
      { name: 'res', count: 2 },
    ]);
    expect(moveClashes(values, new Set([4]), 'Maren', regroup, keyOf, nameOf)).toEqual([]);
  });

  it('counts a name three items would share', () => {
    const three = [...values, { name: 'ds' }];
    expect(moveClashes(three, new Set([2, 5]), 'Maren', regroup, keyOf, nameOf)).toEqual([
      { name: 'ds', count: 3 },
    ]);
  });

  it('never clashes for a kind its key does not tie to a group', () => {
    const byName = (v: Item) => v.name;
    expect(moveClashes(values, new Set([0]), 'Orla', regroup, byName, nameOf)).toEqual([]);
  });

  it('says what to fix before you save', () => {
    expect(moveClashNote(ALIASES, [{ name: 'ds', count: 2 }], 'Maren')).toBe(
      'Maren would then have two aliases named ds. Rename one before you save.',
    );
    expect(
      moveClashNote(
        ALIASES,
        [
          { name: 'ds', count: 2 },
          { name: 'res', count: 3 },
        ],
        '',
      ),
    ).toBe(
      'Your aliases with no group would then include two named ds and three named res. Rename one of each before you save.',
    );
    expect(moveClashNote(ALIASES, [], 'Maren')).toBeNull();
  });

  it('offers each group once, in heading order', () => {
    expect(groupChoices(['orla', 'Maren', ''], [' Maren ', 'Tolliver'])).toEqual([
      'Maren',
      'orla',
      'Tolliver',
    ]);
  });

  it('starts the move from the group the items share', () => {
    expect(sharedGroup(['Maren', ' Maren'])).toBe('Maren');
    expect(sharedGroup(['', ''])).toBe('');
    expect(sharedGroup(['Maren', 'Orla'])).toBeNull();
    expect(sharedGroup([])).toBeNull();
  });
});

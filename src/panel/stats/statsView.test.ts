import { describe, expect, it } from 'vitest';
import {
  armorTitle,
  armorTone,
  saveTitle,
  saveTone,
  signed,
  statFill,
  statTitle,
  statTone,
  statsStyleChoices,
  trainedAt,
} from './statsView';

describe('the Stats pane tones', () => {
  it('colors a stat as the score sheet does, green above what you trained and yellow below', () => {
    expect(statTone({ cur: 22, perm: 20, max: 23, over: 1 })).toBe('up');
    expect(statTone({ cur: 15, perm: 17, max: 19, over: 0 })).toBe('down');
    expect(statTone({ cur: 18, perm: 18, max: 20, over: 0 })).toBe('');
  });

  it('warns on armor that counts under 80% and under 50%, the sheet limits', () => {
    expect(armorTone(94)).toBe('');
    expect(armorTone(80)).toBe('');
    expect(armorTone(79)).toBe('warn');
    expect(armorTone(50)).toBe('warn');
    expect(armorTone(49)).toBe('danger');
    expect(armorTone(null)).toBe('');
  });

  it('reads a save above zero as against you and zero as quiet', () => {
    expect(saveTone(2)).toBe('danger');
    expect(saveTone(0)).toBe('quiet');
    expect(saveTone(-12)).toBe('');
  });
});

describe('the Stats pane numbers', () => {
  it('fills a stat from 3, the lowest a stat goes, to its max, and marks what you trained', () => {
    const s = { cur: 15, perm: 17, max: 19, over: 0 };
    expect(statFill(s)).toBe(75);
    expect(trainedAt(s)).toBe(87.5);
    expect(statFill({ cur: 3, perm: 3, max: 3, over: 0 })).toBe(100);
  });

  it('signs armor and saves with a true minus', () => {
    expect(signed(-310)).toBe('−310');
    expect(signed(2)).toBe('+2');
    expect(signed(0)).toBe('0');
  });

  it('says every number on the pointer', () => {
    expect(statTitle('str', { cur: 22, perm: 20, max: 23, over: 1 })).toBe(
      'Strength 22 of 23, trained to 20, 1 past the max, which counts for nothing',
    );
    expect(statTitle('int', { cur: 18, perm: 18, max: 20, over: 0 })).toBe(
      'Intelligence 18 of 20, trained to 18',
    );
    expect(armorTitle('slash', -310, 71)).toBe('Slash armor −310, 71% of it counts');
    expect(saveTitle('breath', 2)).toBe('Breath save +2, against you');
  });

  it('checks the style you picked in the pane menu', () => {
    expect(statsStyleChoices('rows')).toEqual([
      { value: 'sheet', label: 'Score sheet', checked: false },
      { value: 'rows', label: 'Rows', checked: true },
      { value: 'ledger', label: 'Ledger', checked: false },
    ]);
  });
});

import { describe, expect, it } from 'vitest';
import { findMarks, solidFindMarks } from './findMarks';
import { findTheme } from './themes';

describe('findMarks', () => {
  it('marks Obsidian Ember matches in its yellow at 28 and 60', () => {
    expect(findTheme('obsidian-ember').xterm.yellow).toBe('#d8b56a');
    expect(findMarks(findTheme('obsidian-ember').xterm)).toEqual({
      match: 'rgba(216, 181, 106, 0.28)',
      current: 'rgba(216, 181, 106, 0.6)',
    });
  });

  it('marks Rubric matches in its yellow at 28 and 60', () => {
    expect(findTheme('rubric').xterm.yellow).toBe('#5d4000');
    expect(findMarks(findTheme('rubric').xterm)).toEqual({
      match: 'rgba(93, 64, 0, 0.28)',
      current: 'rgba(93, 64, 0, 0.6)',
    });
  });

  it('gives no marks for a yellow that is not a color', () => {
    expect(findMarks({ yellow: 'not a color' })).toBeNull();
    expect(findMarks({ yellow: '' })).toBeNull();
  });

  it('lays the marks over a ground as solid colors for xterm', () => {
    const ember = findTheme('obsidian-ember').xterm;
    expect(solidFindMarks(ember, ember.background)).toEqual({
      match: '#403620',
      current: '#846e41',
    });
    const rubric = findTheme('rubric').xterm;
    expect(solidFindMarks(rubric, rubric.background)).toEqual({
      match: '#c7b795',
      current: '#988253',
    });
    expect(solidFindMarks({ yellow: 'nope' }, '#000000')).toBeNull();
    expect(solidFindMarks(ember, 'nope')).toBeNull();
  });
});

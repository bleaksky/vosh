import { describe, expect, it } from 'vitest';
import {
  numberMarks,
  numberRuns,
  shapesOnScreen,
  tailOnScreen,
  wholeMarks,
  wrappedRows,
} from './promptScreen';

// The game's own lines on the terminal screen, for the card's marks
// while the profile reads no prompt yet.

const PROMPT = '<1020/1020hp 800/800m 930/930mv> ';

const screen = [
  'You walk east along the harbor road.',
  PROMPT.trimEnd() + ' east',
  'The Lighthouse Steps',
  'You sit down on the step and rest.',
  '<980/1020hp 800/800m 930/930mv>',
  '',
  '',
];

describe('the game line at the end of the screen', () => {
  it('is the last text on screen, with a blank row under it or none', () => {
    const rows = [...screen.slice(0, 4), PROMPT.trimEnd(), '', ''];
    expect(tailOnScreen(rows, [PROMPT], 80)).toEqual({ row: 4, lines: [PROMPT] });
    // A partial the cursor still sits after.
    expect(tailOnScreen(rows.slice(0, 5), [PROMPT], 80)).toEqual({ row: 4, lines: [PROMPT] });
  });

  it('is nothing once other text came after it', () => {
    const rows = [PROMPT.trimEnd(), 'The wind picks up.'];
    expect(tailOnScreen(rows, [PROMPT], 80)).toBeNull();
    expect(tailOnScreen(['', ''], [PROMPT], 80)).toBeNull();
  });

  it('takes the rows the word wrap breaks a long line into, and every line of a block', () => {
    const tank = 'Tester: [===|===|===|---]';
    const rows = ['A guard attacks you!', tank, '<1020/1020hp 800/800m', '930/930mv>', ''];
    expect(wrappedRows(PROMPT.trimEnd(), 26)).toEqual(['<1020/1020hp 800/800m', '930/930mv>']);
    expect(tailOnScreen(rows, [tank, PROMPT], 26)).toEqual({ row: 1, lines: [tank, PROMPT] });
  });
});

describe('the lines of the same shape on screen', () => {
  it('finds each row whose numbers alone differ, your echo after it or not', () => {
    expect(shapesOnScreen(screen, PROMPT, 80)).toEqual([
      { row: 1, lines: ['<1020/1020hp 800/800m 930/930mv>'] },
      { row: 4, lines: ['<980/1020hp 800/800m 930/930mv>'] },
    ]);
    // Text right after it is not your echo, so it is another line.
    expect(shapesOnScreen(['<1020/1020hp 800/800m 930/930mv>x'], PROMPT, 80)).toEqual([]);
    const rows = [PROMPT.trimEnd(), 'east', PROMPT.trimEnd()];
    expect(shapesOnScreen(rows, PROMPT, 80).map((f) => f.row)).toEqual([0, 2]);
  });

  it('finds nothing for a blank line or one wider than the screen', () => {
    expect(shapesOnScreen(['', '  '], '  ', 80)).toEqual([]);
    expect(shapesOnScreen([PROMPT.trimEnd()], PROMPT, 10)).toEqual([]);
  });
});

describe('the marks on a line found on screen', () => {
  it('covers each line whole', () => {
    expect(wholeMarks(['ab ', 'Tester: x'])).toEqual([
      { row: 0, col: 0, width: 2, warn: false },
      { row: 1, col: 0, width: 9, warn: false },
    ]);
  });

  it('covers each number you named, counted in its own line', () => {
    const line = '<980/1020hp 800/800m 930/930mv>';
    expect(numberRuns(line)).toEqual([
      { start: 1, end: 4 },
      { start: 5, end: 9 },
      { start: 12, end: 15 },
      { start: 16, end: 19 },
      { start: 21, end: 24 },
      { start: 25, end: 28 },
    ]);
    expect(numberMarks(line, [true, true, false, false, true])).toEqual([
      { row: 0, col: 1, width: 3, warn: false },
      { row: 0, col: 5, width: 4, warn: false },
      { row: 0, col: 21, width: 3, warn: false },
    ]);
  });
});

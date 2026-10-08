import { describe, expect, it } from 'vitest';
import {
  breakText,
  columns,
  count,
  cutLine,
  flow,
  fold,
  marks,
  pasted,
  rewrapAll,
  rewrapParagraph,
  spamRun,
  storedBytes,
  type Row,
} from './text';

const hard = (lines: string[]): Row[] => lines.map((text) => ({ text, flows: false }));
const texts = (rows: Row[]) => rows.map((r) => r.text);

describe('columns', () => {
  it('counts what a looker sees, codes and trailing spaces aside', () => {
    expect(columns('A tall elf.')).toBe(11);
    expect(columns('`#Bold yellow``.')).toBe(12);
    expect(columns('ends here   ')).toBe(9);
  });
});

describe('breakText', () => {
  it('breaks at the last space that fits and keeps that space on the line', () => {
    const { lines } = breakText('one two three four', 9);
    expect(lines).toEqual(['one two ', 'three ', 'four']);
    expect(lines.join('')).toBe('one two three four');
  });

  it('never cuts a word and never opens a line with a dot', () => {
    expect(breakText('a verylongword b', 5).lines).toEqual(['a ', 'verylongword ', 'b']);
    expect(breakText('he waits ...then moves', 12).lines).toEqual([
      'he ',
      'waits ...then ',
      'moves',
    ]);
  });

  it('starts a line at a word that opens with a code', () => {
    expect(breakText('plain `#bold text', 40).lines).toEqual(['plain ', '`#bold text']);
  });
});

describe('flow', () => {
  it('hands the last word of a long line down and keeps the caret on it', () => {
    const rows: Row[] = [
      { text: 'one two three ', flows: true },
      { text: 'four', flows: false },
    ];
    const typed = [{ text: 'one two three!x ', flows: true }, rows[1]];
    const { rows: out, caret } = flow(typed, { row: 0, col: 15 }, 14);
    expect(texts(out)).toEqual(['one two ', 'three!x four']);
    expect(caret).toEqual({ row: 1, col: 7 });
  });

  it('takes words up when a line gets short, and leaves a break you made', () => {
    const rows: Row[] = [
      { text: 'one ', flows: true },
      { text: 'two three', flows: false },
      { text: 'four', flows: false },
    ];
    const { rows: out } = flow(rows, { row: 0, col: 4 }, 20);
    expect(out).toEqual([
      { text: 'one two three', flows: false },
      { text: 'four', flows: false },
    ]);
  });
});

describe('rewrap', () => {
  const read = hard([
    'A tall elf stands',
    'with a straight back.',
    '',
    '   An indent keeps',
    'her own line.',
  ]);

  it('rewraps one paragraph, keeping every word', () => {
    const out = rewrapParagraph(read, 0, 30);
    expect(texts(out).slice(0, 2)).toEqual(['A tall elf stands with a ', 'straight back.']);
    expect(texts(out).slice(2)).toEqual(['', '   An indent keeps', 'her own line.']);
  });

  it('rewraps every paragraph and keeps an indent', () => {
    const out = rewrapAll(read, 75);
    expect(texts(out)).toEqual([
      'A tall elf stands with a straight back.',
      '',
      '   An indent keeps her own line.',
    ]);
    // A line that starts with spaces starts a paragraph of its own.
    expect(texts(rewrapAll(hard(['first', '  second']), 75))).toEqual(['first', '  second']);
  });
});

describe('pasted', () => {
  it('wraps each long line and folds what the game drops', () => {
    const para = 'She hasn’t moved in an hour — not once …';
    const { rows, wrapped, folded } = pasted(`${para}\n\nShort.`, 20);
    expect(wrapped).toBe(1);
    // The ellipsis would open a line, so once goes down with it.
    expect(texts(rows)).toEqual([
      "She hasn't moved in ",
      'an hour -- not ',
      'once ...',
      '',
      'Short.',
    ]);
    expect(folded).toEqual([
      { what: 'curly apostrophe', count: 1 },
      { what: 'long dash', count: 1 },
      { what: 'ellipsis', count: 1 },
    ]);
  });

  it('turns tabs and no break spaces into spaces', () => {
    expect(fold('a\tb c').text).toBe('a b c');
  });
});

describe('marks', () => {
  it('marks what runs past the width in danger where a help sets it', () => {
    const line = 'x'.repeat(76);
    expect(marks(line, 75, true, false)).toEqual([{ from: 75, to: 76, kind: 'over' }]);
    expect(marks(line, 75, false, false)).toEqual([{ from: 75, to: 76, kind: 'soft' }]);
  });

  it('marks a quote, a dropped character, a code inside the line and a dot at the start', () => {
    expect(marks('."hi" café `!red', 75, true, false).map((m) => m.kind)).toEqual([
      'command',
      'struck',
      'quote',
      'quote',
      'dropped',
    ]);
    // From trust 55 the game keeps a code anywhere.
    expect(marks('plain `!red', 75, true, true)).toEqual([]);
  });
});

describe('count', () => {
  it('counts lines with text, the empty ones between, and the room they take', () => {
    expect(count(['one', '', 'two', '', ''], 75)).toEqual({
      lines: 2,
      empty: 1,
      past: 0,
      bytes: 13,
    });
    expect(storedBytes(['ab', ''])).toBe(7);
  });

  it('finds where the editor stops taking lines', () => {
    const lines = Array.from({ length: 80 }, () => 'x'.repeat(70));
    expect(cutLine(lines)).toBe(63);
    expect(cutLine(['short'])).toBeNull();
  });

  it('finds a run the game takes as spam', () => {
    expect(spamRun(Array.from({ length: 26 }, () => '|  |'))).toBe(0);
    expect(spamRun(Array.from({ length: 26 }, () => ''))).toBeNull();
  });
});

import { describe, expect, it } from 'vitest';
import { countLine, lineNote, metaLine, pasteNote, resultNote } from './words';

describe('the footer count', () => {
  it('counts a description against ten to thirty and names the empty lines apart', () => {
    expect(countLine('description', { lines: 18, empty: 1, past: 0, bytes: 900 })).toEqual({
      main: '18 lines',
      tone: 'n',
      rest: ', 1 empty · 10 to 30',
    });
    expect(countLine('description', { lines: 4, empty: 0, past: 0, bytes: 200 })).toEqual({
      main: '4 lines',
      tone: 'warn',
      rest: ', 6 short of 10',
    });
    expect(countLine('description', { lines: 31, empty: 2, past: 1, bytes: 2400 }).rest).toBe(
      ', 2 empty, one past 30 · 1 past 75',
    );
  });

  it('counts a note against the room the game gives, less for a report', () => {
    expect(countLine('note', { lines: 14, empty: 2, past: 0, bytes: 1234 })).toEqual({
      main: '14 lines',
      tone: 'n',
      rest: ', 2 empty · 1,234 of 4,604 characters',
    });
    expect(countLine('bug', { lines: 1, empty: 0, past: 0, bytes: 4600 }).tone).toBe('bad');
    expect(countLine('note', { lines: 0, empty: 0, past: 0, bytes: 0 }).main).toBe('Empty');
  });
});

describe('the note about the caret’s line', () => {
  it('names a line past the width and offers to rewrap it', () => {
    const line = `${'word '.repeat(15)}x`;
    expect(lineNote(line, 3, 75, true, false)).toEqual({
      note: { lead: 'Line 4 is one character too long.', rest: ' Lines stop at 75.', tone: 'bad' },
      rewrap: true,
    });
    expect(lineNote(line, 3, 75, false, false)?.note.tone).toBe('warn');
  });

  it('names a word with no space to break at', () => {
    expect(lineNote('x'.repeat(80), 0, 75, true, false)?.note.lead).toBe(
      'Line 1 has no space to break at.',
    );
  });

  it('names a dot at the start, a code inside the line and a double quote', () => {
    expect(lineNote('...the nightgaunt', 0, 75, true, false)?.note.lead).toBe(
      'Line 1 starts with a dot.',
    );
    expect(lineNote('`#Bold yellow``, then plain', 1, 75, true, false)?.note.rest).toContain(
      'stays bold yellow',
    );
    expect(lineNote('`#Bold yellow``, then plain', 1, 75, true, true)).toBeNull();
    expect(lineNote('he said "no"', 3, 75, true, false)?.note.lead).toBe(
      'Line 4 has double quotes.',
    );
    expect(lineNote('a clean line', 0, 75, true, false)).toBeNull();
  });
});

describe('the header line', () => {
  const base = {
    name: 'Orla',
    board: false,
    job: null,
    read: false,
    fresh: false,
    done: null,
    dropped: null,
  } as const;

  it('says whose draft it is and how it stands', () => {
    expect(metaLine(base)).toBe('Orla’s draft, not sent yet');
    expect(metaLine({ ...base, board: true })).toBe('Orla’s draft, not posted yet');
    expect(metaLine({ ...base, read: true })).toBe('From the game, for Orla');
    expect(metaLine({ ...base, done: 'sent' })).toBe('Sent for Orla');
    expect(metaLine({ ...base, fresh: true, name: 'Tolliver' })).toBe('New draft for Tolliver');
    expect(metaLine({ ...base, dropped: { sent: 8, total: 19 } })).toBe(
      '8 of 19 lines sent for Orla',
    );
    expect(metaLine({ ...base, board: true, dropped: { sent: 8, total: 19 } })).toBe(
      '8 of 19 lines sent for Orla',
    );
    const find = {
      id: 1,
      kind: 'journal',
      action: 'find',
      stage: 'reading',
      sent: 0,
      total: 0,
    } as const;
    expect(metaLine({ ...base, board: true, job: find })).toBe('Checking the board for Orla');
  });
});

describe('what a paste and a job leave', () => {
  it('says what a paste wrapped and folded', () => {
    expect(pasteNote(2, 75, [{ what: 'curly apostrophe', count: 1 }], 'hasn’t').rest).toBe(
      'Vosh wrapped 2 lines of your paste at 75 and straightened 1 curly apostrophe in hasn’t.',
    );
  });

  it('points to the game’s own reason for a refusal', () => {
    expect(resultNote({ kind: 'refused', field: 'post', line: 'x' }, 'application')).toEqual({
      lead: 'The game turned your application down.',
      rest: ' It says why just below. Your draft is safe.',
      tone: 'bad',
    });
    expect(resultNote({ kind: 'dropped', sent: 8, posted: false }, 'description')?.lead).toBe(
      'Your connection dropped after line 8.',
    );
    expect(resultNote({ kind: 'dropped', sent: 8, posted: false }, 'journal')?.rest).toBe(
      ' Nothing was posted.',
    );
    expect(resultNote({ kind: 'other_note', board: 'idea', note: null }, 'note')?.lead).toBe(
      'You’d already started an idea in the game.',
    );
  });

  it('names the first line of a note that differs in the game', () => {
    expect(resultNote({ kind: 'failed', why: 'differs', line: 3 }, 'note')).toEqual({
      lead: 'Line 3 came out different in the game,',
      rest: ' so Vosh cleared the note and didn’t post it.',
      tone: 'bad',
    });
    expect(resultNote({ kind: 'failed', why: 'differs', line: null }, 'note')?.lead).toBe(
      'Your note came out different in the game,',
    );
  });

  it('says what a drop on a board left, and what the board’s list then showed', () => {
    expect(resultNote({ kind: 'dropped', sent: 8, posted: false }, 'journal')).toEqual({
      lead: 'Your connection dropped after line 8.',
      rest: ' Nothing was posted.',
      tone: 'bad',
    });
    expect(resultNote({ kind: 'dropped', sent: 19, posted: true }, 'journal')).toEqual({
      lead: 'Your connection dropped while this was posting.',
      rest: ' Check the board before you post it again.',
      tone: 'bad',
    });
    expect(resultNote({ kind: 'found', number: 3 }, 'journal')).toEqual({
      lead: '',
      rest: 'It went through before your connection dropped. It’s note 3 on the board.',
      tone: 'ok',
    });
    expect(resultNote({ kind: 'cant_tell' }, 'bug')).toEqual({
      lead: 'Your connection dropped while this was posting.',
      rest: ' Bug reports don’t show up anywhere you can check, so there’s no telling if it went through.',
      tone: 'warn',
    });
    // Not found, the card says the drop's own words again.
    expect(resultNote({ kind: 'not_found' }, 'journal')).toBeNull();
  });
});

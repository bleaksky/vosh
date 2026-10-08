import { describe, expect, it } from 'vitest';
import { footFor, type FootInput } from './cardFoot';

const base: FootInput = {
  kind: 'description',
  running: null,
  ended: null,
  paste: null,
  over: null,
  spam: null,
  flagged: null,
  busy: false,
  count: { main: '19 lines', tone: 'n', rest: ' · 10 to 30' },
  live: true,
  phase: 'edit',
  sentView: false,
  canSend: true,
  canPost: true,
  finding: false,
  matches: false,
  hasGame: true,
};

const ids = (f: FootInput) => footFor(f).buttons.map((b) => b.id);

describe('the footer', () => {
  it('counts, and sends a description', () => {
    expect(footFor(base).left).toEqual({ count: base.count });
    expect(ids(base)).toEqual(['send']);
    expect(footFor({ ...base, live: false }).left).toEqual({
      count: { ...base.count, rest: ' · 10 to 30 · Not connected' },
    });
  });

  it('puts the fix for the caret’s line beside Send', () => {
    const flagged = {
      note: { lead: 'Line 4 runs 76 columns', rest: ', one past 75.', tone: 'bad' as const },
      rewrap: true,
    };
    expect(ids({ ...base, flagged })).toEqual(['rewrap', 'send']);
    expect(footFor({ ...base, flagged }).left).toEqual({ note: flagged.note });
  });

  it('offers the check beside Done once the game holds the text, and after a send', () => {
    expect(ids({ ...base, matches: true })).toEqual(['check', 'done']);
    expect(ids({ ...base, kind: 'history', matches: true })).toEqual(['check', 'done']);
    expect(ids({ ...base, kind: 'beast', matches: true })).toEqual(['send']);
    expect(ids({ ...base, phase: 'sent' })).toEqual(['check', 'done']);
    expect(ids({ ...base, kind: 'beast', phase: 'sent' })).toEqual(['done']);
  });

  it('posts a note and stops a job until its post goes', () => {
    expect(ids({ ...base, kind: 'note' })).toEqual(['post']);
    const running = {
      id: 1,
      kind: 'note' as const,
      action: 'post' as const,
      stage: 'sending' as const,
      sent: 8,
      total: 19,
    };
    expect(footFor({ ...base, kind: 'note', running })).toEqual({
      left: { progress: 'Sending line 9 of 19' },
      buttons: [{ id: 'stop', label: 'Stop', disabled: false }],
    });
    expect(
      footFor({ ...base, running: { ...running, stage: 'posting' } }).buttons[0].disabled,
    ).toBe(true);
  });

  it('offers Restore and Send again after a drop', () => {
    const ended = {
      note: { lead: 'The link dropped after line 8.', rest: '', tone: 'bad' as const },
      actions: ['restore' as const, 'again' as const],
    };
    expect(footFor({ ...base, ended }).buttons.map((b) => b.label)).toEqual([
      'Restore the game’s copy',
      'Send again',
    ]);
  });

  it('offers Post again after a drop on a board, and Done once the list finds it', () => {
    const ended = {
      note: {
        lead: 'You were disconnected after line 8.',
        rest: ' Nothing was posted.',
        tone: 'bad' as const,
      },
      actions: ['again' as const],
    };
    expect(footFor({ ...base, kind: 'journal', ended })).toEqual({
      left: { note: ended.note },
      buttons: [{ id: 'post', label: 'Post again', primary: true, disabled: false }],
    });
    expect(
      ids({ ...base, kind: 'journal', ended: { ...ended, actions: [] }, phase: 'posted' }),
    ).toEqual(['done']);
  });

  it('keeps Post… off while the board’s list after a drop is not read yet', () => {
    const ended = {
      note: {
        lead: 'You were disconnected as the note posted.',
        rest: ' Check the board before you post it again.',
        tone: 'bad' as const,
      },
      actions: [],
    };
    expect(footFor({ ...base, kind: 'journal', ended, finding: true }).buttons).toEqual([
      { id: 'post', label: 'Post…', primary: true, disabled: true },
    ]);
    expect(footFor({ ...base, kind: 'journal', finding: true }).buttons[0].disabled).toBe(true);
  });

  it('ends on Done when the game turns an application down', () => {
    const ended = {
      note: { lead: 'The game turned your application down.', rest: '', tone: 'bad' as const },
      actions: ['done' as const],
    };
    expect(ids({ ...base, kind: 'application', ended })).toEqual(['done']);
  });

  it('keeps Post… off while another board’s note waits in the game', () => {
    const ended = {
      note: { lead: 'You had an idea started in the game.', rest: '', tone: 'warn' as const },
      actions: ['clear-other' as const],
      other: 'idea' as const,
    };
    expect(footFor({ ...base, kind: 'note', ended }).buttons).toEqual([
      { id: 'clear-other', label: 'Clear it…' },
      { id: 'post', label: 'Post…', primary: true, disabled: true },
    ]);
  });

  it('keeps the busy note and the room past the game’s behind a job’s end', () => {
    expect(footFor({ ...base, busy: true }).left).toEqual({
      note: {
        lead: 'The game is waiting in a line editor.',
        rest: ' End it with @, then send.',
        tone: 'warn',
      },
    });
    expect(footFor({ ...base, kind: 'journal', over: 1300 }).left).toEqual({
      note: {
        lead: 'This is 1,300 characters too long for the game.',
        rest: ' Cut it down or split it in two.',
        tone: 'bad',
      },
    });
  });
});

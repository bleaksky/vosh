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

  it('offers the check once the game holds the text, and Done after a send', () => {
    expect(ids({ ...base, matches: true })).toEqual(['check', 'send']);
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

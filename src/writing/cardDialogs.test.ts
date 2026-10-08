import { describe, expect, it } from 'vitest';
import type { WritingCharacter } from '../ipc/writing';
import { checkAnswer, checkedNote, checkHeld, sentNote } from './cardDialogs';
import { checkWaits, withCheckWaiting } from './draftsStore';

// The game's answers to dcheck (recycle.c) and history check
// (act_comm.c), verbatim.
const APPROVED = 'Your description has already been approved.';
const SUBMITTED = "You've already submitted your description for approval. You can't send another.";
const INTENTIONS = "You've already made your intentions clear to the Gods.";
const RECOGNIZE = 'The gods already recognize you and your history.';
const AT_EASE = 'You feel more at ease as you make yourself known to the Gods.';

describe('checkAnswer', () => {
  it('reads a check the game already holds as waiting', () => {
    expect(checkAnswer([SUBMITTED])).toBe('waits');
    expect(checkAnswer([INTENTIONS])).toBe('waits');
  });

  it('reads a check the immortals took as decided', () => {
    expect(checkAnswer([APPROVED])).toBe('decided');
    expect(checkAnswer([RECOGNIZE])).toBe('decided');
  });

  it('reads anything else as a check that went through', () => {
    expect(checkAnswer([AT_EASE])).toBe('sent');
    expect(checkAnswer([])).toBe('sent');
  });
});

describe('checkedNote', () => {
  it('gives a decided history its own line, not a warn', () => {
    expect(checkedNote('history', [RECOGNIZE])).toEqual({
      lead: '',
      rest: 'The immortals have already read your history.',
      tone: 'ok',
    });
  });

  it('keeps the warn for a history the game already took', () => {
    expect(checkedNote('history', [INTENTIONS]).tone).toBe('warn');
  });
});

describe('sentNote', () => {
  it('says only the read back with no check waiting', () => {
    expect(sentNote(12, true, null)).toEqual({
      lead: '',
      rest: 'The game has all 12 lines, just as you wrote them',
      tone: 'ok',
    });
  });

  it('says a waiting description check still reads the old text', () => {
    expect(sentNote(12, true, 'description').rest).toBe(
      'The game has all 12 lines, just as you wrote them. If your check is still waiting, it reads the description you sent back then.',
    );
  });

  it('says a waiting history check still reads the old text', () => {
    expect(sentNote(4, true, 'history').rest).toBe(
      'The game has all 4 lines, just as you wrote them. If your history check is still waiting, it reads the history you sent back then.',
    );
  });

  it('warns about a miss whether or not a check waits', () => {
    expect(sentNote(4, false, 'history')).toEqual(sentNote(4, false, null));
    expect(sentNote(4, false, null).tone).toBe('warn');
  });
});

describe('checkHeld', () => {
  it('keeps a check that went through waiting, as one already held', () => {
    expect(checkHeld([AT_EASE])).toBe(true);
    expect(
      checkHeld(['Your description has been sent for approval. ALL dcheck submissions will']),
    ).toBe(true);
    expect(checkHeld([SUBMITTED])).toBe(true);
    expect(checkHeld([INTENTIONS])).toBe(true);
  });

  it('leaves none waiting once the immortals decided', () => {
    expect(checkHeld([APPROVED])).toBe(false);
    expect(checkHeld([RECOGNIZE])).toBe(false);
  });

  it('marks a check that went through as waiting for the next send', () => {
    const orla: WritingCharacter = {
      host: 'example.org',
      port: 9999,
      name: 'Orla',
      drafts: [],
      sent: [],
    };
    const after = withCheckWaiting(orla, 'description', checkHeld([AT_EASE]));
    expect(checkWaits(after, 'description')).toBe(true);
  });
});

describe('withCheckWaiting', () => {
  const orla: WritingCharacter = {
    host: 'example.org',
    port: 9999,
    name: 'Orla',
    drafts: [],
    sent: [],
  };

  it('keeps a waiting check for one kind only', () => {
    const waiting = withCheckWaiting(orla, 'description', true);
    expect(waiting.checks).toEqual(['description']);
    expect(checkWaits(waiting, 'description')).toBe(true);
    expect(checkWaits(waiting, 'history')).toBe(false);
  });

  it('clears it and leaves the other kind', () => {
    const both = withCheckWaiting(withCheckWaiting(orla, 'description', true), 'history', true);
    expect(withCheckWaiting(both, 'description', false).checks).toEqual(['history']);
  });

  it('returns the same character when nothing changes', () => {
    expect(withCheckWaiting(orla, 'history', false)).toBe(orla);
    const waiting = withCheckWaiting(orla, 'history', true);
    expect(withCheckWaiting(waiting, 'history', true)).toBe(waiting);
  });
});

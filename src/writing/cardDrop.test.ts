import { describe, expect, it } from 'vitest';
import type { WriteJob } from '../ipc/writing';
import { afterDrop, findToStart, type Find } from './cardDrop';

const post: WriteJob = { id: 1, kind: 'journal', action: 'post', subject: 'The Great Milieu' };
const look: WriteJob = { id: 2, kind: 'journal', action: 'find', subject: 'The Great Milieu' };
const waiting: Find = {
  drop: { sent: 19, total: 19 },
  subject: 'The Great Milieu',
  started: true,
};

describe('the card after a drop', () => {
  it('counts a drop mid send and offers Post again', () => {
    const next = afterDrop(null, { kind: 'dropped', sent: 8, posted: false }, post, 'journal', 19);
    expect(next).toEqual({
      dropped: { sent: 8, total: 19 },
      find: null,
      ended: {
        note: expect.objectContaining({ lead: 'Your connection dropped after line 8.' }),
        actions: ['again'],
      },
      posted: false,
    });
  });

  it('keeps Restore for a text the game saves in place', () => {
    const send = { ...post, kind: 'description' as const, action: 'send' as const };
    const next = afterDrop(
      null,
      { kind: 'dropped', sent: 3, posted: false },
      send,
      'description',
      9,
    );
    expect(next?.ended.actions).toEqual(['restore', 'again']);
  });

  it('waits for the board’s list after a drop that came as it posted', () => {
    const next = afterDrop(null, { kind: 'dropped', sent: 19, posted: true }, post, 'journal', 19);
    expect(next).toEqual({
      dropped: { sent: 19, total: 19 },
      find: { drop: { sent: 19, total: 19 }, subject: 'The Great Milieu', started: false },
      ended: {
        note: expect.objectContaining({ lead: 'Your connection dropped while this was posting.' }),
        actions: [],
      },
      posted: false,
    });
  });

  it('looks for the subject the post went out with', () => {
    const next = afterDrop(null, { kind: 'dropped', sent: 19, posted: true }, post, 'journal', 20);
    expect(next?.find?.subject).toBe('The Great Milieu');
    expect(findToStart(next!.find, true, false)?.subject).toBe('The Great Milieu');
  });

  it('starts the find once the session plays and no job runs', () => {
    const fresh = { ...waiting, started: false };
    expect(findToStart(fresh, false, false)).toBeNull();
    expect(findToStart(fresh, true, true)).toBeNull();
    expect(findToStart(waiting, true, false)).toBeNull();
    expect(findToStart(null, true, false)).toBeNull();
    expect(findToStart(fresh, true, false)).toBe(fresh);
  });

  it('runs a find the link cut again', () => {
    const next = afterDrop(
      waiting,
      { kind: 'dropped', sent: 0, posted: false },
      look,
      'journal',
      19,
    );
    expect(next?.find).toEqual({ ...waiting, started: false });
    expect(next?.dropped).toEqual(waiting.drop);
    expect(next?.ended.actions).toEqual([]);
  });

  it('moves the note to Sent when the list holds it', () => {
    const next = afterDrop(waiting, { kind: 'found', number: 3 }, look, 'journal', 19);
    expect(next).toEqual({
      dropped: null,
      find: null,
      ended: {
        note: expect.objectContaining({
          rest: 'It went through before your connection dropped. It’s note 3 on the board.',
        }),
        actions: [],
      },
      posted: true,
    });
  });

  it('offers Post again when the list does not hold it', () => {
    const next = afterDrop(waiting, { kind: 'not_found' }, look, 'journal', 19);
    expect(next).toEqual({
      dropped: waiting.drop,
      find: null,
      ended: {
        note: expect.objectContaining({ lead: 'Your connection dropped after line 19.' }),
        actions: ['again'],
      },
      posted: false,
    });
  });

  it('offers Post again when only immortals read the board', () => {
    const next = afterDrop(waiting, { kind: 'cant_tell' }, { ...look, kind: 'idea' }, 'idea', 19);
    expect(next?.ended.note.rest).toContain('there’s no telling if it went through');
    expect(next?.ended.actions).toEqual(['again']);
    expect(next?.posted).toBe(false);
  });

  it('goes back to Post again when you stop the find', () => {
    const next = afterDrop(waiting, { kind: 'stopped', sent: 0 }, look, 'journal', 19);
    expect(next).toEqual({
      dropped: waiting.drop,
      find: null,
      ended: {
        note: expect.objectContaining({ lead: 'Your connection dropped while this was posting.' }),
        actions: ['again'],
      },
      posted: false,
    });
  });

  it('leaves every other end to the card', () => {
    expect(afterDrop(null, { kind: 'stopped', sent: 2 }, post, 'journal', 19)).toBeNull();
    expect(
      afterDrop(null, { kind: 'posted', forum: true, vote: false }, post, 'journal', 19),
    ).toBeNull();
  });
});

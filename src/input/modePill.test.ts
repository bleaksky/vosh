import { describe, expect, it } from 'vitest';
import type { WalkProgress } from '../ipc/session';
import type { WritingState } from '../ipc/writing';
import { WRITING_IDLE } from '../stores/session/writingStore';
import { modeOf } from './modePill';

const IDLE_WALK: WalkProgress = { kind: 'idle' };
const walking = (done: number, total: number): WalkProgress => ({
  kind: 'walking',
  done,
  total,
  left: 'w',
  route: false,
});
const editing = (lines: number | null, editor: WritingState['editor'] = 'description') => ({
  ...WRITING_IDLE,
  game: 'editor' as const,
  editor,
  lines,
});
const at = (o: { password?: boolean; writing?: WritingState; walk?: WalkProgress }) =>
  modeOf({ password: false, writing: WRITING_IDLE, walk: IDLE_WALK, ...o });

describe('the mode pill', () => {
  it('shows nothing with no mode, so the mark stays', () => {
    expect(at({})).toBeNull();
  });

  it('puts a password over the editor, the editor over the pager, and the pager over a walk', () => {
    const walk = walking(1, 2);
    expect(at({ password: true, writing: editing(3), walk })?.mode).toBe('password');
    expect(at({ writing: editing(3), walk })?.mode).toBe('editor');
    expect(at({ writing: { ...WRITING_IDLE, game: 'pager' }, walk })?.mode).toBe('pager');
    expect(at({ walk })?.mode).toBe('walk');
  });

  it('names the text and counts the line you are on against the help’s limit', () => {
    expect(at({ writing: editing(3) })).toEqual({
      mode: 'editor',
      name: 'Description',
      count: '4 of 30',
      warn: false,
      hint: 'Type @ on a blank line to finish',
      label: 'Description, line 4 of 30',
    });
    expect(at({ writing: editing(29) })?.warn).toBe(false);
    const past = at({ writing: editing(30) });
    expect(past?.count).toBe('31 of 30');
    expect(past?.warn).toBe(true);
  });

  it('counts a text with no limit by its line', () => {
    const note = at({ writing: editing(3, 'note') });
    expect(note?.name).toBe('Note');
    expect(note?.count).toBe('line 4');
    expect(note?.warn).toBe(false);
    expect(note?.label).toBe('Note, line 4');
  });

  it('names a tome, a vote, paper and a pet and counts their lines with no limit', () => {
    const names = { tome: 'Tome', vote: 'Cabal vote', paper: 'Paper', pet: 'Pet description' };
    for (const [kind, name] of Object.entries(names)) {
      const pill = at({ writing: editing(40, kind as WritingState['editor']) });
      expect(pill?.name).toBe(name);
      expect(pill?.count).toBe('line 41');
      expect(pill?.warn).toBe(false);
    }
  });

  it('shows the name alone while the lines are not counted', () => {
    const pill = at({ writing: editing(null) });
    expect(pill?.name).toBe('Description');
    expect(pill?.count).toBeNull();
    expect(pill?.label).toBe('Description');
  });

  it('shows More at the pager, over the text the editor holds', () => {
    const pager = { ...editing(3), game: 'pager' as const };
    expect(at({ writing: pager })).toEqual({
      mode: 'pager',
      name: 'More',
      count: null,
      warn: false,
      hint: 'Press Enter for the next page',
      label: 'More',
    });
  });

  it('shows no pill while the card runs a job', () => {
    const job = {
      id: 1,
      kind: 'description' as const,
      action: 'send' as const,
      stage: 'sending' as const,
      sent: 2,
      total: 5,
    };
    expect(at({ writing: { ...editing(3), job } })).toBeNull();
    expect(at({ writing: { ...editing(3), game: 'pager', job } })).toBeNull();
  });

  it('counts the steps a walk has left', () => {
    const one = at({ walk: walking(1, 2) });
    expect(one?.count).toBe('1 step left');
    expect(one?.label).toBe('Walking, 1 step left');
    expect(one?.hint).toBe('Esc stops the walk');
    expect(at({ walk: walking(1, 3) })?.count).toBe('2 steps left');
    expect(at({ password: true })?.hint).toBeNull();
  });
});

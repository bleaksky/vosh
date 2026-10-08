import { describe, expect, it } from 'vitest';
import type { WritingCharacter } from '../ipc/writing';
import { clearedBy } from './checkWatch';

const maren: WritingCharacter = {
  host: 'example.org',
  port: 9999,
  name: 'Maren',
  drafts: [],
  sent: [],
  checks: ['description', 'history'],
};

describe('clearedBy', () => {
  it('clears the kind a new decision names', () => {
    expect(clearedBy({ id: 4, kind: 'description' }, 3, maren)?.checks).toEqual(['history']);
    expect(clearedBy({ id: 1, kind: 'history' }, undefined, maren)?.checks).toEqual([
      'description',
    ]);
  });

  it('does nothing for a decision it already heard', () => {
    expect(clearedBy({ id: 4, kind: 'description' }, 4, maren)).toBeNull();
  });

  it('does nothing when no decision came or no check waits', () => {
    expect(clearedBy(null, undefined, maren)).toBeNull();
    expect(clearedBy({ id: 5, kind: 'description' }, 4, { ...maren, checks: [] })).toBeNull();
  });
});

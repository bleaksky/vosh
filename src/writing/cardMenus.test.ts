import { describe, expect, it } from 'vitest';
import { checkable, moreRows } from './cardMenus';

type Check = Parameters<typeof checkable>[0];

const sent: Check = {
  live: true,
  running: false,
  matches: false,
  phase: 'sent',
  game: ['The first line of your text.', 'The second.'],
};

const checkRow = (canCheck: boolean, kind: 'history' | 'description' = 'history') =>
  moreRows({
    kind,
    language: false,
    customRace: false,
    spelling: false,
    sentView: false,
    canRead: true,
    canRestore: false,
    canCheck,
  }).find((r) => r !== 'separator' && r.id === 'check');

describe('the check', () => {
  const never: [string, Check][] = [
    ['after an empty send', { ...sent, game: [] }],
    ['when the game holds only blank lines', { ...sent, game: ['', '   ', ''] }],
    ['before the card knows the game copy', { ...sent, game: null }],
    ['offline', { ...sent, live: false }],
    ['while a job runs', { ...sent, running: true }],
    ['in the editor with no fresh read', { ...sent, phase: 'edit' }],
  ];

  it.each(never)('stays off %s', (_, m) => {
    expect(checkable(m)).toBe(false);
    expect(checkRow(checkable(m))).toMatchObject({ disabled: true });
    expect(checkRow(checkable(m), 'description')).toMatchObject({ disabled: true });
  });

  it('turns on once a send leaves your text in the game', () => {
    expect(checkable(sent)).toBe(true);
    expect(checkRow(checkable(sent))).toMatchObject({
      label: 'Send for review…',
      disabled: false,
    });
  });

  it('turns on when a fresh read matches your text', () => {
    const read = { ...sent, phase: 'edit' as const, matches: true };
    expect(checkable(read)).toBe(true);
    expect(checkRow(checkable(read), 'description')).toMatchObject({
      label: 'Send for approval…',
      disabled: false,
    });
  });
});

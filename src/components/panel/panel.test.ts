import { describe, expect, it, vi } from 'vitest';
import { defaultLayout, splitPane } from '../../lib/paneLayout';
import type { ImmQueues } from '../../lib/immStore';
import { immRows, immSummary } from './immRows';
import {
  affectStateWord,
  chatTime,
  exitsLabel,
  splitSpeaker,
  ticksLabel,
  vitalValue,
} from './paneText';

// paneActions pulls in the Tauri bridge through the layout store, so
// stub it. The pure tree helper under test never calls it.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const { setLeafProps } = await import('./paneActions');

describe('splitSpeaker', () => {
  it('splits a one word speaker off a channel line', () => {
    expect(splitSpeaker('Tarvik: anyone up for a Temple run tonight')).toEqual({
      speaker: 'Tarvik',
      text: 'anyone up for a Temple run tonight',
    });
  });

  it('keeps colons inside the message', () => {
    expect(splitSpeaker('Selune: meet at 8:42: the bank')).toEqual({
      speaker: 'Selune',
      text: 'meet at 8:42: the bank',
    });
  });

  it('leaves routed lines whole', () => {
    for (const line of [
      '[OOC] Selune: anyone seen the gate open today?',
      "Tarvik tells you 'back soon'",
      'no speaker here',
    ]) {
      expect(splitSpeaker(line)).toEqual({ speaker: null, text: line });
    }
  });
});

describe('chatTime', () => {
  const at = (h: number, m: number) => new Date(2026, 8, 28, h, m).getTime();

  it('follows a 12 hour locale without AM or PM', () => {
    expect(chatTime(at(8, 5), 'en-US')).toBe('8:05');
    expect(chatTime(at(20, 41), 'en-US')).toBe('8:41');
    // Just after midnight reads as a clock, not a duration.
    expect(chatTime(at(0, 55), 'en-US')).toBe('12:55');
  });

  it('follows a 24 hour locale', () => {
    expect(chatTime(at(20, 41), 'en-GB')).toBe('20:41');
    expect(chatTime(at(0, 55), 'en-GB')).toBe('00:55');
    expect(chatTime(at(8, 5), 'de-DE')).toBe('08:05');
  });

  it('keeps the minutes at two digits in your own locale', () => {
    expect(chatTime(at(8, 5))).toMatch(/^\d{1,2}[:.]05$/);
  });
});

describe('ticksLabel', () => {
  it('says missing, the ticks, or permanent', () => {
    expect(ticksLabel('missing', null)).toBe('missing');
    expect(ticksLabel('present', 12)).toBe('12');
    expect(ticksLabel('expiring', 0)).toBe('0');
    expect(ticksLabel('present', -1)).toBe('permanent');
    expect(ticksLabel('untracked', null)).toBe('');
  });
});

describe('affectStateWord', () => {
  it('names the states only the marker color shows', () => {
    expect(affectStateWord('expiring')).toBe('expiring');
    expect(affectStateWord('harmful')).toBe('harmful');
    expect(affectStateWord('missing')).toBeNull();
    expect(affectStateWord('present')).toBeNull();
    expect(affectStateWord('untracked')).toBeNull();
  });
});

describe('row text', () => {
  it('formats vitals and exits the way the mockup does', () => {
    expect(vitalValue(186, 1020)).toBe('186 / 1020');
    expect(exitsLabel(['north', 'east', 'south', 'west'])).toBe('north east south west');
  });
});

describe('setLeafProps', () => {
  it('sets a prop on one leaf and keeps every id', () => {
    const tree = splitPane(defaultLayout().root, 'affects', 'column', 'chat');
    const next = setLeafProps(tree, 'chat', { channel: 'ooc' });
    const chat = JSON.stringify(next).includes('"channel":"ooc"');
    expect(chat).toBe(true);
    expect(JSON.stringify(next).match(/"id":"[^"]+"/g)).toEqual(
      JSON.stringify(tree).match(/"id":"[^"]+"/g),
    );
  });

  it('removes a prop on an empty value and returns the same tree for no change', () => {
    const tree = splitPane(defaultLayout().root, 'affects', 'column', 'chat');
    const withChannel = setLeafProps(tree, 'chat', { channel: 'ooc' });
    expect(setLeafProps(withChannel, 'chat', { channel: 'ooc' })).toBe(withChannel);
    const cleared = setLeafProps(withChannel, 'chat', { channel: '' });
    expect(JSON.stringify(cleared)).not.toContain('channel');
    expect(setLeafProps(tree, 'nope', { channel: 'ooc' })).toBe(tree);
  });
});

const ZERO: ImmQueues = {
  dcheck: 0,
  votes: 0,
  appsOpen: 0,
  appsUnread: 0,
  journalsUnread: 0,
  journalsUnawarded: 0,
  penalties: 0,
  bugs: 0,
  typos: 0,
  ideas: 0,
  notes: 0,
  overdueApps: 0,
  overdueJournals: 0,
  overdueDcheck: 0,
  nearingApps: 0,
  nearingJournals: 0,
  nearingDcheck: 0,
};

describe('immRows', () => {
  it('shows nothing and no summary when every queue is empty', () => {
    expect(immRows(ZERO)).toEqual([]);
    expect(immSummary(ZERO)).toBeNull();
  });

  it('puts overdue first, then nearing, then the bigger backlog', () => {
    const rows = immRows({
      ...ZERO,
      notes: 9,
      bugs: 2,
      appsOpen: 1,
      appsUnread: 1,
      nearingApps: 1,
      journalsUnread: 1,
      overdueJournals: 1,
    });
    expect(rows.map((r) => r.key)).toEqual(['journalsUnread', 'appsOpen', 'notes', 'bugs']);
    expect(rows[0]).toMatchObject({ tier: 'overdue', note: '1 overdue' });
    expect(rows[1]).toMatchObject({ tier: 'nearing', note: '1 nearing' });
    expect(immSummary({ ...ZERO, overdueJournals: 1, nearingApps: 2 })).toEqual({
      text: '1 overdue',
      tier: 'overdue',
    });
  });

  it('keeps a queue with deadline pressure at a zero count', () => {
    const rows = immRows({ ...ZERO, overdueJournals: 1, journalsUnawarded: 1 });
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ key: 'journalsUnread', count: 0, tier: 'overdue' });
  });

  it('notes the unread subset when no deadline presses', () => {
    const [row] = immRows({ ...ZERO, appsOpen: 3, appsUnread: 2 });
    expect(row).toMatchObject({ label: 'Applications', note: '2 unread', tier: 'none' });
  });
});

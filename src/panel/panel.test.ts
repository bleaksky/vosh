import { describe, expect, it, vi } from 'vitest';
import { defaultLayout, paneRef, splitPane } from './paneLayout';
import type { ImmQueues } from '../stores/gmcp/immStore';
import { immRows, immSummary } from './imm/immRows';
import { affectHours, affectWords, chatTime, exitsLabel } from './paneText';

// paneActions pulls in the Tauri bridge through the layout store, so
// stub it. The pure tree helper under test never calls it.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const { setLeafProps } = await import('./paneActions');

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

describe('affectHours', () => {
  it('prints the hours with the marks of the game affects bar', () => {
    expect(affectHours('missing', null)).toBe('-');
    expect(affectHours('present', -1)).toBe('+');
    expect(affectHours('present', 12)).toBe('12');
    expect(affectHours('untracked', 188)).toBe('188');
    expect(affectHours('expiring', 0)).toBe('0');
    expect(affectHours('untracked', null)).toBe('');
  });
});

describe('affectWords', () => {
  it('says what the hours column and the marks show, for a screen reader', () => {
    expect(affectWords('missing', null)).toBe(', missing');
    expect(affectWords('present', -1)).toBe(', permanent');
    expect(affectWords('present', 31)).toBe(', 31 hours');
    expect(affectWords('expiring', 2)).toBe(', 2 hours, running out');
    expect(affectWords('expiring', 1)).toBe(', 1 hour, running out');
    expect(affectWords('expiring', 0)).toBe(', 0 hours, running out');
    expect(affectWords('harmful', 3)).toBe(', 3 hours, harmful');
    expect(affectWords('harmful', null)).toBe(', harmful');
    expect(affectWords('untracked', 8)).toBe(', 8 hours');
    expect(affectWords('untracked', null)).toBe('');
  });
});

describe('row text', () => {
  // The vitals values moved to vitalsView.ts with the Values row.
  it('formats exits the way the mockup does', () => {
    expect(exitsLabel(['north', 'east', 'south', 'west'])).toBe('north east south west');
  });
});

describe('setLeafProps', () => {
  it('sets a prop on one leaf and keeps every id', () => {
    const tree = splitPane(defaultLayout().root, 'affects', 'column', paneRef('chat'));
    const next = setLeafProps(tree, 'chat', { channel: 'ooc' });
    const chat = JSON.stringify(next).includes('"channel":"ooc"');
    expect(chat).toBe(true);
    expect(JSON.stringify(next).match(/"id":"[^"]+"/g)).toEqual(
      JSON.stringify(tree).match(/"id":"[^"]+"/g),
    );
  });

  it('removes a prop on an empty value and returns the same tree for no change', () => {
    const tree = splitPane(defaultLayout().root, 'affects', 'column', paneRef('chat'));
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

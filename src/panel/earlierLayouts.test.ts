import { describe, expect, it } from 'vitest';
import { earlierNote, untilLabel } from './earlierLayouts';

describe('untilLabel', () => {
  const now = new Date(2026, 9, 15, 22, 0).getTime();

  it('names the time alone for today and yesterday', () => {
    expect(untilLabel(new Date(2026, 9, 15, 21, 41).getTime(), now)).toBe('Until 9:41 PM today');
    expect(untilLabel(new Date(2026, 9, 14, 23, 15).getTime(), now)).toBe(
      'Until 11:15 PM yesterday',
    );
  });

  it('names the day before that', () => {
    expect(untilLabel(new Date(2026, 9, 12, 8, 3).getTime(), now)).toBe('Until Oct 12 at 8:03 AM');
  });
});

describe('earlierNote', () => {
  it('names the step that replaced a layout', () => {
    expect(earlierNote({ why: 'update', version: '0.95.6' })).toBe('Before 0.95.6');
    expect(earlierNote({ why: 'reset' })).toBe('Before Reset');
    expect(earlierNote({ why: 'restore' })).toBe('Before a restore');
  });

  it('adds nothing for a change or arranging', () => {
    expect(earlierNote({ why: 'change' })).toBeNull();
    expect(earlierNote({ why: 'arrange' })).toBeNull();
  });
});

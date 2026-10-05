import { describe, expect, it } from 'vitest';
import type { WorldTime } from '../stores/gmcp/worldStore';
import { formatGameTime } from './gameTime';

function at(hour: number | null, minute: number | null = null): WorldTime {
  return { hour, minute, day: null, month: null, year: null, sunlight: null, sky: null };
}

describe('formatGameTime', () => {
  it('reads hours and minutes on a 24 hour clock', () => {
    expect(formatGameTime(at(8, 42))).toBe('8:42');
    expect(formatGameTime(at(20, 5))).toBe('20:05');
    expect(formatGameTime(at(0, 0))).toBe('0:00');
  });

  it('reads an hour without minutes as the top of the hour', () => {
    expect(formatGameTime(at(0))).toBe('0:00');
    expect(formatGameTime(at(8))).toBe('8:00');
    expect(formatGameTime(at(23))).toBe('23:00');
  });

  it('shows nothing until the hour is known', () => {
    expect(formatGameTime(null)).toBeNull();
    expect(formatGameTime(at(null, 30))).toBeNull();
  });

  it('reads the 24 hour clock unless you pick the 12 hour one', () => {
    expect(formatGameTime(at(18), '24h')).toBe('18:00');
    expect(formatGameTime(at(18))).toBe('18:00');
  });

  it('reads AM and PM on the 12 hour clock', () => {
    expect(formatGameTime(at(6), '12h')).toBe('6:00 AM');
    expect(formatGameTime(at(18), '12h')).toBe('6:00 PM');
    expect(formatGameTime(at(8, 42), '12h')).toBe('8:42 AM');
    expect(formatGameTime(at(20, 5), '12h')).toBe('8:05 PM');
    expect(formatGameTime(at(11, 59), '12h')).toBe('11:59 AM');
    expect(formatGameTime(at(23), '12h')).toBe('11:00 PM');
  });

  it('reads midnight as 12:00 AM and noon as 12:00 PM', () => {
    expect(formatGameTime(at(0), '12h')).toBe('12:00 AM');
    expect(formatGameTime(at(0, 30), '12h')).toBe('12:30 AM');
    expect(formatGameTime(at(12), '12h')).toBe('12:00 PM');
    expect(formatGameTime(at(12, 15), '12h')).toBe('12:15 PM');
    expect(formatGameTime(at(1), '12h')).toBe('1:00 AM');
    expect(formatGameTime(at(13), '12h')).toBe('1:00 PM');
  });

  it('shows nothing on the 12 hour clock until the hour is known', () => {
    expect(formatGameTime(null, '12h')).toBeNull();
    expect(formatGameTime(at(null, 30), '12h')).toBeNull();
  });
});

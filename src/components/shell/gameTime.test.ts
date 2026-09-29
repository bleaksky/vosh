import { describe, expect, it } from 'vitest';
import type { WorldTime } from '../../lib/stores/worldStore';
import { formatGameTime } from './gameTime';

function at(hour: number | null, minute: number | null = null): WorldTime {
  return { hour, minute, day: null, month: null, year: null, sunlight: null, sky: null };
}

describe('formatGameTime', () => {
  it('reads the hour on a twelve hour clock', () => {
    expect(formatGameTime(at(0))).toBe('12 AM');
    expect(formatGameTime(at(8))).toBe('8 AM');
    expect(formatGameTime(at(12))).toBe('12 PM');
    expect(formatGameTime(at(23))).toBe('11 PM');
  });

  it('adds minutes when the server sends them', () => {
    expect(formatGameTime(at(8, 42))).toBe('8:42 AM');
    expect(formatGameTime(at(20, 5))).toBe('8:05 PM');
  });

  it('shows nothing until the hour is known', () => {
    expect(formatGameTime(null)).toBeNull();
    expect(formatGameTime(at(null, 30))).toBeNull();
  });
});

import { describe, expect, it } from 'vitest';
import type { WorldTime } from '../../lib/stores/worldStore';
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
});

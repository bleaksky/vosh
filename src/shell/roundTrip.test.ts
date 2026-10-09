import { describe, expect, it } from 'vitest';
import { roundTripText, roundTripTone } from './roundTrip';

describe('round trip', () => {
  it('reads milliseconds under a second, then seconds to one decimal', () => {
    expect(roundTripText(38)).toBe('38ms');
    expect(roundTripText(999)).toBe('999ms');
    expect(roundTripText(1000)).toBe('1.0s');
    expect(roundTripText(1400)).toBe('1.4s');
  });

  it('is tertiary under 300 ms, warn from 300 ms, danger from a second', () => {
    expect(roundTripTone(299)).toBe('fine');
    expect(roundTripTone(300)).toBe('warn');
    expect(roundTripTone(999)).toBe('warn');
    expect(roundTripTone(1000)).toBe('danger');
  });
});

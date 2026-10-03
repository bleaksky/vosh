import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  BLINK_MS,
  blinkShown,
  onBlinkFlips,
  REDUCE_MOTION_QUERY,
  resolveBlinkText,
  subscribeReduceMotion,
  systemReducesMotion,
  untilBlinkFlip,
  untilBlinkShows,
} from './blink';

describe('the blink clock', () => {
  it('shows and hides for 600 ms each, counted from the epoch as the native grid counts', () => {
    // The same instants as blink_flips_every_600_ms_on_the_wall_clock in
    // src-tauri/src/native/gpu/tests.rs.
    expect(BLINK_MS).toBe(600);
    expect(blinkShown(0) && blinkShown(599)).toBe(true);
    expect(blinkShown(600) || blinkShown(1199)).toBe(false);
    expect(blinkShown(1200)).toBe(true);
    expect(untilBlinkFlip(0)).toBe(600);
    expect(untilBlinkFlip(599)).toBe(1);
    expect(untilBlinkFlip(1250)).toBe(550);
    // The next shown half, from the shown half and from the hidden one.
    expect(untilBlinkShows(100)).toBe(1100);
    expect(untilBlinkShows(700)).toBe(500);
    expect(untilBlinkShows(1200)).toBe(1200);
  });
});

describe('the flips a drawer waits for', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('runs no timer while nothing blinks', () => {
    vi.useFakeTimers();
    const ticks: boolean[] = [];
    const stop = onBlinkFlips(false, (shown) => ticks.push(shown));
    expect(vi.getTimerCount()).toBe(0);
    vi.advanceTimersByTime(10_000);
    expect(ticks).toEqual([]);
    stop();
  });

  it('flips with the clock while text blinks, and stops when told', () => {
    vi.useFakeTimers();
    vi.setSystemTime(5 * 2 * BLINK_MS + 100);
    const ticks: boolean[] = [];
    const stop = onBlinkFlips(true, (shown) => ticks.push(shown));
    expect(ticks).toEqual([true]);
    // The flip comes 500 ms on, and the timer lands just past it.
    vi.advanceTimersByTime(499);
    expect(ticks).toEqual([true]);
    vi.advanceTimersByTime(3);
    expect(ticks).toEqual([true, false]);
    vi.advanceTimersByTime(BLINK_MS);
    expect(ticks).toEqual([true, false, true]);
    stop();
    expect(vi.getTimerCount()).toBe(0);
  });
});

describe('Blinking text', () => {
  it('is on by default and starts off while your system reduces motion', () => {
    expect(resolveBlinkText(null, false)).toBe(true);
    expect(resolveBlinkText(null, true)).toBe(false);
  });

  it('keeps the choice you made, whatever the system says', () => {
    expect(resolveBlinkText(true, true)).toBe(true);
    expect(resolveBlinkText(false, false)).toBe(false);
  });

  it('reads reduce motion from the system and follows it as it changes', () => {
    const listeners = new Set<(e: { matches: boolean }) => void>();
    const media = {
      matches: true,
      addEventListener: (_: string, cb: (e: { matches: boolean }) => void) => listeners.add(cb),
      removeEventListener: (_: string, cb: (e: { matches: boolean }) => void) =>
        listeners.delete(cb),
    };
    const queries: string[] = [];
    const win = {
      matchMedia: (query: string) => {
        queries.push(query);
        return media;
      },
    };
    expect(systemReducesMotion(win)).toBe(true);
    const seen: boolean[] = [];
    const stop = subscribeReduceMotion((on) => seen.push(on), win);
    for (const cb of listeners) cb({ matches: false });
    expect(seen).toEqual([false]);
    stop();
    expect(listeners.size).toBe(0);
    expect(queries.every((q) => q === REDUCE_MOTION_QUERY)).toBe(true);
    // A window with no matchMedia reduces nothing.
    expect(systemReducesMotion({})).toBe(false);
    expect(() => subscribeReduceMotion(() => {}, {})()).not.toThrow();
  });
});

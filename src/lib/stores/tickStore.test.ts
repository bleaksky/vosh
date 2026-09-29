import { describe, expect, it } from 'vitest';
import type { TickConfig, TickCount, TickPayload } from '../session';
import { computeTick, DEFAULT_TICK_WARN_SECS, shownTick } from './tickStore';

/** A report `elapsed_ms` into a tick of `interval_ms`, the way the
 *  session loop sends it. */
const payload = (
  elapsed_ms: number,
  enabled = true,
  interval_ms = 30_000,
  extra: Partial<TickPayload> = {},
): TickPayload => ({
  enabled,
  interval_ms,
  remaining_ms: Math.max(0, interval_ms - elapsed_ms),
  elapsed_ms,
  overdue: interval_ms > 0 && elapsed_ms >= interval_ms,
  synced: false,
  fired: false,
  sound: false,
  ...extra,
});

const config = (warn_at_secs: number | null, enabled = true, interval_secs = 30): TickConfig => ({
  enabled,
  interval_secs,
  auto_fire: null,
  sound: false,
  reset_pattern: null,
  warn_at_secs,
  warn_message: null,
  warn_color: null,
});

describe('computeTick', () => {
  it('counts whole seconds up since the last tick', () => {
    expect(computeTick(payload(14_000), config(null))).toEqual({
      active: true,
      secsSinceTick: 14,
      secsLeft: 16,
      intervalSecs: 30,
      warnAt: DEFAULT_TICK_WARN_SECS,
      warn: false,
      overdue: false,
      synced: false,
    });
    expect(computeTick(payload(14_800), null).secsSinceTick).toBe(14);
    expect(computeTick(payload(0), null).secsSinceTick).toBe(0);
    expect(computeTick(payload(30_000), null).secsSinceTick).toBe(30);
  });

  it('keeps counting up past the interval while the game runs late', () => {
    expect(computeTick(payload(31_000), null)).toMatchObject({ secsSinceTick: 31, overdue: true });
    expect(computeTick(payload(32_400), null)).toMatchObject({ secsSinceTick: 32, overdue: true });
    expect(computeTick(payload(61_000), null).secsSinceTick).toBe(61);
  });

  it('counts the whole seconds left until the expected tick', () => {
    const left = (elapsed: number) => computeTick(payload(elapsed), null).secsLeft;
    expect(left(0)).toBe(30);
    expect(left(500)).toBe(30);
    expect(left(1_000)).toBe(29);
    expect(left(29_000)).toBe(1);
    expect(left(29_500)).toBe(1);
    expect(left(30_000)).toBe(0);
    expect(Object.is(left(30_500), 0)).toBe(true);
    expect(left(35_000)).toBe(-5);
  });

  it('is overdue from the expected tick until the tick lands', () => {
    expect(computeTick(payload(29_999), null).overdue).toBe(false);
    expect(computeTick(payload(30_000), null).overdue).toBe(true);
    expect(computeTick(payload(0), null).overdue).toBe(false);
  });

  it('reads the interval from the report, then the config', () => {
    expect(computeTick(payload(15_000, true, 60_000), null).secsLeft).toBe(45);
    expect(computeTick(payload(15_000, true, 0), config(null, true, 60)).secsLeft).toBe(45);
    expect(computeTick(payload(15_000, true, 0), config(null, true, 60)).intervalSecs).toBe(60);
  });

  it('reports the interval it counts against, for the tick ring', () => {
    expect(computeTick(payload(15_000, true, 60_000), config(null)).intervalSecs).toBe(60);
    expect(computeTick(payload(15_000, true, 12_500), null).intervalSecs).toBe(12.5);
  });

  it('counts up alone while the interval is unknown or zero', () => {
    const unknown = computeTick(payload(20_000, true, 0), null);
    expect(unknown).toMatchObject({
      active: true,
      secsSinceTick: 20,
      secsLeft: null,
      intervalSecs: null,
      warn: false,
      overdue: false,
    });
    expect(computeTick(payload(20_000, true, 0), config(null, true, 0)).secsLeft).toBeNull();
  });

  it('restarts the count when a tick lands early', () => {
    expect(computeTick(payload(24_000), null).secsLeft).toBe(6);
    expect(computeTick(payload(0, true, 30_000, { fired: true }), null)).toMatchObject({
      secsSinceTick: 0,
      secsLeft: 30,
      overdue: false,
    });
  });

  it('warns in the last seconds you set in the tick config', () => {
    expect(computeTick(payload(22_000), config(8))).toMatchObject({
      secsSinceTick: 22,
      warn: true,
    });
    expect(computeTick(payload(21_999), config(8))).toMatchObject({
      secsSinceTick: 21,
      warn: false,
    });
    expect(computeTick(payload(25_000), config(null))).toMatchObject({
      secsSinceTick: 25,
      warnAt: 5,
      warn: true,
    });
    expect(computeTick(payload(24_000), config(0))).toMatchObject({ warnAt: 5, warn: false });
    expect(computeTick(payload(30_000), config(null)).warn).toBe(true);
  });

  it('keeps warning while the tick is overdue', () => {
    expect(computeTick(payload(31_000), config(null))).toMatchObject({ warn: true, overdue: true });
    expect(computeTick(payload(58_000), config(8))).toMatchObject({ warn: true, overdue: true });
  });

  it('says whether the game tick decides', () => {
    expect(computeTick(payload(3_000, true, 30_000, { synced: true }), null).synced).toBe(true);
    expect(computeTick(payload(3_000), null).synced).toBe(false);
  });

  it('reads a report with no time since the tick as a fresh tick', () => {
    const bad = { ...payload(3_000), elapsed_ms: Number.NaN };
    expect(computeTick(bad, null)).toMatchObject({ secsSinceTick: 0, secsLeft: 30 });
  });

  it('is inactive without a report or with the timer off in the report', () => {
    const off = {
      active: false,
      secsSinceTick: null,
      secsLeft: null,
      intervalSecs: null,
      warnAt: 5,
      warn: false,
      overdue: false,
      synced: false,
    };
    expect(computeTick(null, config(null))).toEqual(off);
    expect(computeTick(payload(3_000, false), null)).toEqual(off);
  });

  it('shows a tick the session reports running even when the config read says off', () => {
    // A connection starts the tick whatever the profile saved, and the
    // config read at launch can predate it.
    expect(computeTick(payload(3_000), config(null, false))).toMatchObject({
      active: true,
      secsSinceTick: 3,
    });
  });
});

describe('shownTick', () => {
  /** What the status line shows `elapsed_ms` into a tick, counting
   *  `count`. */
  const shown = (count: TickCount, elapsed_ms: number, interval_ms = 30_000) =>
    shownTick(computeTick(payload(elapsed_ms, true, interval_ms), null), count);
  const secs = (count: TickCount, elapsed_ms: number) => shown(count, elapsed_ms)?.secs;

  it('counts up the seconds since the last tick, past the interval while late', () => {
    expect(shown('up', 14_000)).toEqual({ secs: 14, count: 'up' });
    expect(secs('up', 0)).toBe(0);
    expect(secs('up', 30_000)).toBe(30);
    expect(secs('up', 31_000)).toBe(31);
    expect(secs('up', 32_900)).toBe(32);
  });

  it('counts down the seconds left and waits at 0 until the tick lands', () => {
    expect(shown('down', 0)).toEqual({ secs: 30, count: 'down' });
    expect(secs('down', 500)).toBe(30);
    expect(secs('down', 1_000)).toBe(29);
    expect(secs('down', 29_000)).toBe(1);
    expect(secs('down', 29_500)).toBe(1);
    expect(secs('down', 30_000)).toBe(0);
    expect(Object.is(secs('down', 30_500), 0)).toBe(true);
    expect(secs('down', 35_000)).toBe(0);
    expect(secs('down', 59_000)).toBe(0);
  });

  it('counts down past 0 below zero until the tick lands', () => {
    expect(shown('down_past_zero', 0)).toEqual({ secs: 30, count: 'down_past_zero' });
    expect(secs('down_past_zero', 500)).toBe(30);
    expect(secs('down_past_zero', 1_000)).toBe(29);
    expect(secs('down_past_zero', 29_000)).toBe(1);
    expect(secs('down_past_zero', 29_500)).toBe(1);
    expect(secs('down_past_zero', 30_000)).toBe(0);
    // Half a second late still reads 0, and never minus zero.
    expect(Object.is(secs('down_past_zero', 30_500), 0)).toBe(true);
    expect(secs('down_past_zero', 31_000)).toBe(-1);
    expect(secs('down_past_zero', 35_000)).toBe(-5);
  });

  it('restarts from the interval when a tick lands early', () => {
    for (const count of ['down', 'down_past_zero'] as const) {
      expect(secs(count, 24_000)).toBe(6);
      expect(secs(count, 0)).toBe(30);
    }
    expect(secs('up', 0)).toBe(0);
  });

  it('counts up instead while the interval is unknown or zero', () => {
    for (const count of ['up', 'down', 'down_past_zero'] as const) {
      expect(shown(count, 20_000, 0)).toEqual({ secs: 20, count: 'up' });
    }
  });

  it('shows nothing while the timer is off or silent', () => {
    expect(shownTick(computeTick(null, null), 'down')).toBeNull();
    expect(shownTick(computeTick(payload(3_000, false), null), 'up')).toBeNull();
  });

  it('leaves the warning and the late tick the same in every count', () => {
    const at = (elapsed_ms: number) => computeTick(payload(elapsed_ms), config(null));
    for (const count of ['up', 'down', 'down_past_zero'] as const) {
      expect(shownTick(at(24_000), count), count).not.toBeNull();
    }
    expect(at(24_000)).toMatchObject({ warn: false, overdue: false });
    expect(at(25_000)).toMatchObject({ warn: true, overdue: false });
    expect(at(30_000)).toMatchObject({ warn: true, overdue: true });
    expect(at(41_000)).toMatchObject({ warn: true, overdue: true });
  });
});

import { describe, expect, it } from 'vitest';
import type { TickConfig, TickPayload } from '../session';
import { computeTick, DEFAULT_TICK_WARN_SECS } from './tickStore';

const payload = (remaining_ms: number, enabled = true): TickPayload => ({
  enabled,
  interval_ms: 30_000,
  remaining_ms,
  fired: false,
  sound: false,
});

const config = (warn_at_secs: number | null, enabled = true): TickConfig => ({
  enabled,
  interval_secs: 30,
  auto_fire: null,
  sound: false,
  reset_pattern: null,
  warn_at_secs,
  warn_message: null,
  warn_color: null,
});

describe('computeTick', () => {
  it('counts whole seconds down to the next tick', () => {
    expect(computeTick(payload(14_000), config(null))).toEqual({
      active: true,
      secsToTick: 14,
      warnAt: DEFAULT_TICK_WARN_SECS,
      warn: false,
    });
    expect(computeTick(payload(13_200), null).secsToTick).toBe(14);
    expect(computeTick(payload(0), null).secsToTick).toBe(0);
  });

  it('warns at the threshold you set in the tick config', () => {
    expect(computeTick(payload(8_000), config(8)).warn).toBe(true);
    expect(computeTick(payload(8_001), config(8)).warn).toBe(false);
    expect(computeTick(payload(3_000), config(null))).toMatchObject({ warnAt: 5, warn: true });
    expect(computeTick(payload(6_000), config(0))).toMatchObject({ warnAt: 5, warn: false });
  });

  it('is inactive without a report or with the timer off', () => {
    const off = { active: false, secsToTick: null, warnAt: 5, warn: false };
    expect(computeTick(null, config(null))).toEqual(off);
    expect(computeTick(payload(3_000, false), null)).toEqual(off);
    expect(computeTick(payload(3_000), config(null, false))).toEqual(off);
  });
});

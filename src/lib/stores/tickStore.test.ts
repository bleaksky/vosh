import { describe, expect, it } from 'vitest';
import type { TickConfig, TickPayload } from '../session';
import { computeTick, DEFAULT_TICK_WARN_SECS } from './tickStore';

const payload = (remaining_ms: number, enabled = true, interval_ms = 30_000): TickPayload => ({
  enabled,
  interval_ms,
  remaining_ms,
  fired: false,
  sound: false,
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
    expect(computeTick(payload(16_000), config(null))).toEqual({
      active: true,
      secsSinceTick: 14,
      warnAt: DEFAULT_TICK_WARN_SECS,
      warn: false,
    });
    expect(computeTick(payload(15_200), null).secsSinceTick).toBe(14);
    expect(computeTick(payload(30_000), null).secsSinceTick).toBe(0);
    expect(computeTick(payload(0), null).secsSinceTick).toBe(30);
  });

  it('reads the interval from the report, then the config', () => {
    expect(computeTick(payload(45_000, true, 60_000), null).secsSinceTick).toBe(15);
    expect(computeTick(payload(45_000, true, 0), config(null, true, 60)).secsSinceTick).toBe(15);
    expect(computeTick(payload(20_000, true, 0), null).secsSinceTick).toBe(10);
  });

  it('never counts below zero or past the interval', () => {
    expect(computeTick(payload(31_000), null).secsSinceTick).toBe(0);
    expect(computeTick(payload(-500), null).secsSinceTick).toBe(30);
  });

  it('warns in the last seconds you set in the tick config', () => {
    expect(computeTick(payload(8_000), config(8))).toMatchObject({ secsSinceTick: 22, warn: true });
    expect(computeTick(payload(8_001), config(8))).toMatchObject({
      secsSinceTick: 21,
      warn: false,
    });
    expect(computeTick(payload(5_000), config(null))).toMatchObject({
      secsSinceTick: 25,
      warnAt: 5,
      warn: true,
    });
    expect(computeTick(payload(6_000), config(0))).toMatchObject({ warnAt: 5, warn: false });
    expect(computeTick(payload(0), config(null)).warn).toBe(true);
  });

  it('is inactive without a report or with the timer off', () => {
    const off = { active: false, secsSinceTick: null, warnAt: 5, warn: false };
    expect(computeTick(null, config(null))).toEqual(off);
    expect(computeTick(payload(3_000, false), null)).toEqual(off);
    expect(computeTick(payload(3_000), config(null, false))).toEqual(off);
  });
});

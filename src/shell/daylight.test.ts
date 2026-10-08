import { describe, expect, it } from 'vitest';
import { STATUS_TEXT_CONTRAST } from '../theme/chrome';
import { contrast, parseHex, rgbToOklch } from '../theme/color';
import type { WorldTime } from '../stores/gmcp/worldStore';
import { BUILTIN_THEMES, findTheme, themeTokens } from '../theme/themes';
import { DAYLIGHT_SLOTS, daylightPhase, daylightTint, isDaytime } from './daylight';

const HOURS = Array.from({ length: 24 }, (_, h) => h);

function at(hour: number | null, sunlight: string | null = null): WorldTime {
  return { hour, minute: null, day: null, month: null, year: null, sunlight, sky: null };
}

describe('daylightPhase', () => {
  it('keeps the old chip boundaries', () => {
    const phases = HOURS.map(daylightPhase);
    expect(phases).toEqual([
      ...Array(5).fill('late-night'),
      'dawn',
      'dawn',
      ...Array(4).fill('morning'),
      ...Array(3).fill('midday'),
      ...Array(3).fill('afternoon'),
      'dusk',
      'dusk',
      ...Array(3).fill('evening'),
      'late-night',
      'late-night',
    ]);
  });

  it('knows nothing without a real hour', () => {
    expect(daylightPhase(null)).toBeNull();
    expect(daylightPhase(-1)).toBeNull();
    expect(daylightPhase(24)).toBeNull();
    expect(daylightPhase(Number.NaN)).toBeNull();
  });
});

describe('daylightTint', () => {
  it('reads each part of the day from its ANSI slot', () => {
    expect(DAYLIGHT_SLOTS).toEqual({
      'late-night': 'blue',
      dawn: 'brightRed',
      morning: 'yellow',
      midday: 'brightYellow',
      afternoon: 'yellow',
      dusk: 'red',
      evening: 'magenta',
    });
  });

  it('keeps a slot that already reads as words', () => {
    const nord = findTheme('nord');
    expect(daylightTint(23, nord.xterm, themeTokens(nord))).toBe(nord.xterm.blue);
    const ember = findTheme('obsidian-ember');
    expect(daylightTint(12, ember.xterm, themeTokens(ember))).toBe(ember.xterm.brightYellow);
  });

  it('darkens a pale slot on a light theme and keeps its hue', () => {
    // Rubric's vermilion draws dusk at about 3:1 on the paper.
    const rubric = findTheme('rubric');
    const tokens = themeTokens(rubric);
    const tint = daylightTint(17, rubric.xterm, tokens);
    expect(tint).not.toBeNull();
    expect(tint).not.toBe(rubric.xterm.red);
    const bg = parseHex(tokens.bg)!;
    expect(contrast(parseHex(tint!)!, bg)).toBeGreaterThanOrEqual(STATUS_TEXT_CONTRAST);
    const hue = (hex: string) => rgbToOklch(parseHex(hex)!).h;
    expect(Math.abs(hue(tint!) - hue(rubric.xterm.red))).toBeLessThan(10);
  });

  it('reads as words on every built in theme at every hour', () => {
    for (const theme of BUILTIN_THEMES) {
      const tokens = themeTokens(theme);
      const bg = parseHex(tokens.bg)!;
      for (const hour of HOURS) {
        const tint = daylightTint(hour, theme.xterm, tokens);
        expect(tint, `${theme.id} at ${hour}`).not.toBeNull();
        expect(contrast(parseHex(tint!)!, bg), `${theme.id} at ${hour}`).toBeGreaterThanOrEqual(
          STATUS_TEXT_CONTRAST,
        );
      }
    }
  });

  it('leaves the time plain while the hour is unknown or the slot is not a color', () => {
    const nord = findTheme('nord');
    expect(daylightTint(null, nord.xterm, themeTokens(nord))).toBeNull();
    const broken = { ...nord.xterm, blue: 'var(--blue)' };
    expect(daylightTint(1, broken, themeTokens(nord))).toBeNull();
    expect(daylightTint(1, nord.xterm, { bg: 'transparent', appearance: 'dark' })).toBe(
      nord.xterm.blue,
    );
  });
});

describe('isDaytime', () => {
  it('follows World.Time sunlight when the server sends it', () => {
    expect(isDaytime(at(3, 'dark'))).toBe(false);
    expect(isDaytime(at(12, 'dark'))).toBe(false);
    expect(isDaytime(at(6, 'rise'))).toBe(true);
    expect(isDaytime(at(12, 'light'))).toBe(true);
    expect(isDaytime(at(19, 'Set'))).toBe(true);
    expect(isDaytime(at(null, 'light'))).toBe(true);
  });

  it('falls back to the hour, day from 6 until 19', () => {
    expect(isDaytime(at(5))).toBe(false);
    expect(isDaytime(at(6))).toBe(true);
    expect(isDaytime(at(18))).toBe(true);
    expect(isDaytime(at(19))).toBe(false);
    expect(isDaytime(at(0, 'foggy'))).toBe(false);
  });

  it('knows nothing without sunlight or an hour', () => {
    expect(isDaytime(null)).toBeNull();
    expect(isDaytime(at(null))).toBeNull();
  });
});

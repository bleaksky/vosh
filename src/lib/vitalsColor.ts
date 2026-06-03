// Per-vital color resolution. Shared between the live vitals
// renderer (components/VitalsBar.tsx) and the Settings preview
// (SettingsApp.tsx) so the in-Settings preview lights up with the
// user's per-vital color overrides instead of falling back to the
// theme accent.
//
// Behavior matrix for `colorForVital`:
//   - override set, ramp on  → lerp drain-red ↔ override by percent
//   - override set, ramp off → static override color
//   - no override, ramp on   → built-in green/blue/orange ramp
//   - no override, ramp off  → static "full" color of the built-in ramp

import type { VitalsConfig } from './session';

type Stop = [pct: number, rgb: [number, number, number]];

export const VITAL_RAMPS: Record<string, Stop[]> = {
  // green → yellow → orange → red
  hp: [
    [0, [228, 104, 118]],
    [25, [217, 154, 108]],
    [55, [230, 195, 132]],
    [100, [135, 169, 135]],
  ],
  // blue → teal → purple → red
  mn: [
    [0, [228, 104, 118]],
    [30, [192, 130, 168]],
    [65, [134, 153, 188]],
    [100, [127, 180, 202]],
  ],
  // orange → deep red
  mv: [
    [0, [228, 104, 118]],
    [60, [220, 130, 100]],
    [100, [217, 154, 108]],
  ],
};

/** The shared "danger red" all built-in ramps drain through. Custom
 *  overrides lerp toward this color as the bar empties when
 *  `use_color_ramp` is on. */
const DRAIN_RED: [number, number, number] = [228, 104, 118];

export function colorForVital(label: string, value: number, config?: VitalsConfig): string {
  const overrideHex = config
    ? label === 'hp'
      ? config.hp_color
      : label === 'mn'
        ? config.mn_color
        : label === 'mv'
          ? config.mv_color
          : ''
    : '';
  const useRamp = config ? config.use_color_ramp : true;
  const v = Math.max(0, Math.min(100, value));

  if (overrideHex) {
    const rgb = hexToRgb(overrideHex);
    if (rgb) {
      if (!useRamp) return rgbString(rgb);
      const t = v / 100;
      return rgbString([
        Math.round(DRAIN_RED[0] + (rgb[0] - DRAIN_RED[0]) * t),
        Math.round(DRAIN_RED[1] + (rgb[1] - DRAIN_RED[1]) * t),
        Math.round(DRAIN_RED[2] + (rgb[2] - DRAIN_RED[2]) * t),
      ]);
    }
    // Malformed hex (the user is mid-type, or pasted something the
    // color input would not accept) falls through to the built-in
    // path so the bar never goes invisible.
  }

  const ramp = VITAL_RAMPS[label] ?? VITAL_RAMPS.hp;
  if (!useRamp) {
    const top = ramp[ramp.length - 1][1];
    return rgbString(top);
  }
  let lower = ramp[0];
  let upper = ramp[ramp.length - 1];
  for (let i = 0; i < ramp.length - 1; i++) {
    if (v >= ramp[i][0] && v <= ramp[i + 1][0]) {
      lower = ramp[i];
      upper = ramp[i + 1];
      break;
    }
  }
  const range = upper[0] - lower[0];
  const t = range > 0 ? (v - lower[0]) / range : 0;
  const lerp = (a: number, b: number) => Math.round(a + (b - a) * t);
  return rgbString([
    lerp(lower[1][0], upper[1][0]),
    lerp(lower[1][1], upper[1][1]),
    lerp(lower[1][2], upper[1][2]),
  ]);
}

export function rgbString(rgb: [number, number, number]): string {
  return `rgb(${rgb[0]}, ${rgb[1]}, ${rgb[2]})`;
}

/** Parse `#rrggbb` or `#rgb` into an RGB triple. Returns null for
 *  anything else; the caller falls back to the built-in ramp. The
 *  Settings color picker emits `#rrggbb`, so this is enough. */
export function hexToRgb(hex: string): [number, number, number] | null {
  const trimmed = hex.trim();
  const m6 = /^#([0-9a-f]{6})$/i.exec(trimmed);
  if (m6) {
    const n = parseInt(m6[1], 16);
    return [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff];
  }
  const m3 = /^#([0-9a-f]{3})$/i.exec(trimmed);
  if (m3) {
    const c = m3[1];
    return [parseInt(c[0] + c[0], 16), parseInt(c[1] + c[1], 16), parseInt(c[2] + c[2], 16)];
  }
  return null;
}

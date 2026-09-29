// The ring before the tick in the Icon style, on the 16 unit grid of
// the One Window icons. A faint ring, and on top an arc from 12 o clock
// running clockwise for the share of the tick interval gone since the
// last tick. Nothing draws on top at 0 seconds, and the whole circle
// closes once the interval has passed. It is the same count as the
// number beside it, drawn so you can read it from the corner of your
// eye.

export const TICK_RING_CENTER = 8;
export const TICK_RING_RADIUS = 5.75;
/** The faint ring under the arc, currentColor at this opacity. */
export const TICK_RING_TRACK_OPACITY = 0.35;

export type TickArc =
  /** The faint ring alone. At 0 seconds, or while the interval is
   *  unknown. */
  | { kind: 'empty' }
  /** The whole circle on top, at the interval and past it. */
  | { kind: 'whole' }
  /** An arc from the top to `end`, clockwise. */
  | { kind: 'part'; share: number; end: { x: number; y: number }; path: string };

function num(n: number): string {
  return String(Math.round(n * 100) / 100);
}

/** The arc for `secs` since the last tick against an interval of
 *  `interval` seconds. The share clamps to 0..1. An interval that is
 *  unknown or not positive leaves the faint ring alone. */
export function tickArc(secs: number, interval: number | null): TickArc {
  if (interval === null || !Number.isFinite(interval) || interval <= 0) return { kind: 'empty' };
  if (Number.isNaN(secs)) return { kind: 'empty' };
  const share = Math.min(Math.max(secs / interval, 0), 1);
  if (share <= 0) return { kind: 'empty' };
  if (share >= 1) return { kind: 'whole' };
  const angle = share * 2 * Math.PI;
  const c = TICK_RING_CENTER;
  const r = TICK_RING_RADIUS;
  const end = { x: c + r * Math.sin(angle), y: c - r * Math.cos(angle) };
  const large = share > 0.5 ? 1 : 0;
  const path = `M${num(c)} ${num(c - r)}A${num(r)} ${num(r)} 0 ${large} 1 ${num(end.x)} ${num(end.y)}`;
  return { kind: 'part', share, end, path };
}

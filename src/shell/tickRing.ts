// The ring before the tick in the Icon style, on the 16 unit grid of
// Vosh's icons. A faint ring, and on top an arc for the tick.
// Counting up, the arc runs clockwise from 12 o clock for the share of
// the interval gone since the last tick. Nothing draws on top at 0
// seconds, and the whole circle closes once the interval has passed.
// Counting down, the arc is the share left instead, from where the
// count stands clockwise to 12 o clock, so it empties toward the tick.
// Either way the moving edge sits at the same place and travels
// clockwise. It is the same count as the number beside it, drawn so
// you can read it from the corner of your eye.

export const TICK_RING_CENTER = 8;
export const TICK_RING_RADIUS = 5.75;
/** The faint ring under the arc, currentColor at this opacity. */
export const TICK_RING_TRACK_OPACITY = 0.35;

export type TickArc =
  /** The faint ring alone. At 0 seconds, while the interval is unknown,
   *  and while the arc is too short to end apart from the top. */
  | { kind: 'empty' }
  /** The whole circle on top, at the interval and past it, and in the
   *  last moment before it once the arc's end meets the top again. */
  | { kind: 'whole' }
  /** A clockwise arc covering `share` of the ring, between the top and
   *  `end`, where the count stands. Counting up it runs from the top to
   *  `end`, and counting down from `end` to the top. */
  | { kind: 'part'; share: number; end: { x: number; y: number }; path: string };

function num(n: number): string {
  return String(Math.round(n * 100) / 100);
}

/** `secs` as a share of `interval`, clamped to 0..1, or null while the
 *  interval is unknown or not positive or the count is not a number. */
function shareOf(secs: number, interval: number | null): number | null {
  if (interval === null || !Number.isFinite(interval) || interval <= 0) return null;
  if (Number.isNaN(secs)) return null;
  return Math.min(Math.max(secs / interval, 0), 1);
}

/** The arc covering `share` of the ring. It starts at the top counting
 *  up, and ends at the top counting down (`left`).
 *
 *  The edge is written to two decimals. On a long interval, such as one
 *  set with `#tick interval 10000`, the edge in the first and last
 *  seconds rounds back onto the top of the ring. SVG drops an arc whose
 *  ends meet, so that path would draw nothing and the nearly closed
 *  ring would read empty. Past the half it closes the circle instead,
 *  and before it the faint ring stands alone. */
function arcOf(share: number | null, left: boolean): TickArc {
  if (share === null || share <= 0) return { kind: 'empty' };
  if (share >= 1) return { kind: 'whole' };
  // Where the count stands, as a share of the way round from the top.
  const angle = (left ? 1 - share : share) * 2 * Math.PI;
  const c = TICK_RING_CENTER;
  const r = TICK_RING_RADIUS;
  const end = { x: c + r * Math.sin(angle), y: c - r * Math.cos(angle) };
  const top = `${num(c)} ${num(c - r)}`;
  const edge = `${num(end.x)} ${num(end.y)}`;
  if (edge === top) return share > 0.5 ? { kind: 'whole' } : { kind: 'empty' };
  const large = share > 0.5 ? 1 : 0;
  const radii = `${num(r)} ${num(r)}`;
  const path = left
    ? `M${edge}A${radii} 0 ${large} 1 ${top}`
    : `M${top}A${radii} 0 ${large} 1 ${edge}`;
  return { kind: 'part', share, end, path };
}

/** The arc counting up, for `secs` since the last tick against an
 *  interval of `interval` seconds. The share clamps to 0..1, so it stays
 *  whole while the tick is late. An interval that is unknown or not
 *  positive leaves the faint ring alone. */
export function tickArc(secs: number, interval: number | null): TickArc {
  return arcOf(shareOf(secs, interval), false);
}

/** The arc counting down, for `secs` left until the expected tick. It
 *  is whole right after a tick and empties toward the top, and at 0 or
 *  below zero, while the tick is late, the faint ring stands alone. */
export function tickArcLeft(secs: number, interval: number | null): TickArc {
  return arcOf(shareOf(secs, interval), true);
}

import type { StatusStyle, TickCount } from '../ipc/uiConfig';

// How the Strip, Dashboard and Meters bars give way as they run short,
// and the share of the tick their bars fill. Compact keeps its own
// measured order in statusLineFit.ts.
//
// Each style lists the parts it lets go, first to last. The bar draws
// with none gone, and while its items run past its edge it lets one
// more go, before the frame paints, so you never see it overflow. A new
// width or a new set of items starts it over from none. A part goes
// whole: nothing ends in an ellipsis while a part is left to go. Then
// the bar squeezes, and the room, your opponent's name and your vitals
// text end in an ellipsis to fit, each keeping room for a few words. A
// bar still short of room after that lets your opponent's name go, so
// the name reads or goes whole and never shrinks to a letter.

/** What a bar can let go. */
export type BarPart =
  /** Your character's name. */
  | 'name'
  /** The area after the room name. */
  | 'area'
  /** The word day or night after the game time. */
  | 'dayWord'
  /** The gauges narrow from 44 to 28 px. */
  | 'narrow'
  | 'moons'
  /** Your target, name and all. */
  | 'target'
  /** The labels before your vitals and the tick. */
  | 'labels'
  /** The gauges and the tick bar go, and the values stay. */
  | 'gauges'
  /** A round trip under 300 ms. A slow one stays. */
  | 'roundTrip'
  | 'time'
  /** A slow round trip's reading. Its dot or bars stay, in their tone. */
  | 'rttText'
  /** Each vital reads its current value without its max. */
  | 'maxes'
  /** The items draw closer together. */
  | 'tight'
  /** The room, your opponent's name and your vitals text may end in an
   *  ellipsis. */
  | 'squeeze'
  /** Your opponent's name. Its health stays, in the warn tone. */
  | 'foeName';

export type BarStyle = Exclude<StatusStyle, 'compact'>;

/** Each style's give way order. */
export const BAR_GIVE_WAY: Readonly<Record<BarStyle, readonly BarPart[]>> = {
  strip: [
    'name',
    'area',
    'dayWord',
    'moons',
    'target',
    'narrow',
    'roundTrip',
    'labels',
    'time',
    'gauges',
    'maxes',
    'tight',
    'rttText',
    'squeeze',
    'foeName',
  ],
  dashboard: [
    'name',
    'moons',
    'area',
    'target',
    'roundTrip',
    'time',
    'tight',
    'maxes',
    'rttText',
    'squeeze',
    'foeName',
  ],
  meters: [
    'moons',
    'roundTrip',
    'target',
    'labels',
    'time',
    'maxes',
    'tight',
    'rttText',
    'squeeze',
    'foeName',
  ],
};

/** The parts gone at `level` in `style`. */
export function partsGone(style: BarStyle, level: number): ReadonlySet<BarPart> {
  return new Set(BAR_GIVE_WAY[style].slice(0, level));
}

/** The share of the interval gone since the last tick, 0 to 1, for a
 *  count of `secs` in the `count` direction. Null while the interval is
 *  unknown. Counting down, the count is the seconds left. */
export function tickShare(secs: number, interval: number | null, count: TickCount): number | null {
  if (interval === null || !Number.isFinite(interval) || interval <= 0) return null;
  const gone = count === 'up' ? secs : interval - secs;
  return Math.min(Math.max(gone / interval, 0), 1);
}

import { useLayoutEffect, useRef, useState, type RefObject } from 'react';
import type { StatusStyle, TickCount } from '../ipc/uiConfig';

// How the Strip, Dashboard and Meters bars give way as they run short,
// and the share of the tick their bars fill. Compact keeps its own
// measured order in statusLineFit.ts.
//
// Each style lists the parts it lets go, first to last. The bar draws
// with none gone, and while its items run past its edge it lets one
// more go, before the frame paints, so you never see it overflow. A new
// width or a new set of items starts it over from none.

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
  /** The labels before your vitals and the tick. */
  | 'labels'
  /** The gauges and the tick bar go, and the values stay. */
  | 'gauges'
  /** A round trip under 300 ms. A slow one stays. */
  | 'roundTrip'
  | 'time'
  /** Your opponent's name. Its health stays. */
  | 'foeName';

export type BarStyle = Exclude<StatusStyle, 'compact'>;

/** Each style's give way order. */
export const BAR_GIVE_WAY: Readonly<Record<BarStyle, readonly BarPart[]>> = {
  strip: [
    'name',
    'area',
    'dayWord',
    'narrow',
    'moons',
    'foeName',
    'labels',
    'roundTrip',
    'gauges',
    'time',
  ],
  dashboard: ['name', 'moons', 'area', 'roundTrip', 'time', 'foeName'],
  meters: ['moons', 'roundTrip', 'labels', 'time', 'foeName'],
};

/** The parts gone at `level` in `style`. */
export function partsGone(style: BarStyle, level: number): ReadonlySet<BarPart> {
  return new Set(BAR_GIVE_WAY[style].slice(0, level));
}

/** How many of its parts a bar lets go: none while its items fit, and
 *  one more for each render they run past its edge. `key` names the
 *  items it draws, so a fight, a font or a style change starts over. */
export function useGiveWay(ref: RefObject<HTMLElement | null>, steps: number, key: string): number {
  const [level, setLevel] = useState(0);
  const [width, setWidth] = useState<number | null>(null);
  const seen = useRef<string | null>(null);
  useLayoutEffect(() => {
    const node = ref.current;
    if (!node) return;
    const measure = () => setWidth(node.clientWidth);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    return () => observer.disconnect();
  }, [ref]);
  useLayoutEffect(() => {
    const node = ref.current;
    if (!node || width === null) return;
    const now = `${key}|${width}`;
    if (seen.current !== now) {
      seen.current = now;
      if (level !== 0) {
        setLevel(0);
        return;
      }
    }
    if (level < steps && node.scrollWidth > node.clientWidth + 1) setLevel(level + 1);
  }, [ref, width, key, level, steps]);
  return Math.min(level, steps);
}

/** The share of the interval gone since the last tick, 0 to 1, for a
 *  count of `secs` in the `count` direction. Null while the interval is
 *  unknown. Counting down, the count is the seconds left. */
export function tickShare(secs: number, interval: number | null, count: TickCount): number | null {
  if (interval === null || !Number.isFinite(interval) || interval <= 0) return null;
  const gone = count === 'up' ? secs : interval - secs;
  return Math.min(Math.max(gone / interval, 0), 1);
}

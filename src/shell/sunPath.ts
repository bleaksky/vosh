import { DAY_FIRST_HOUR, NIGHT_FIRST_HOUR } from './daylight';

// The sun on its path before the game time in the Icon style, on the
// 16 unit grid of the One Window icons. A horizon, a faint half circle
// over it for the sun's path, and a dot for the sun. While the sun is
// up the dot sits on the path for the game hour. It rises on the left
// after 6:00, stands highest at midday, and sets on the right by 19:00.
// While the sun is down the dot drops under the horizon and opens.
//
// Whether the sun is up comes from isDaytime, so World.Time sunlight
// decides when the server sends it and the hour decides otherwise. The
// hour only places the dot.

export const SUN_CENTER_X = 8;
export const SUN_HORIZON_Y = 10.5;
export const SUN_ARC_RADIUS = 5.5;
/** The faint path over the horizon, currentColor at this opacity. */
export const SUN_TRACK_OPACITY = 0.35;
/** The horizon, 13 units wide under the path. */
export const SUN_HORIZON_PATH = 'M1.5 10.5h13';
/** The sun's path, a half circle standing on the horizon. */
export const SUN_ARC_PATH = 'M2.5 10.5A5.5 5.5 0 0 1 13.5 10.5';
/** The filled sun on its path. */
export const SUN_UP_RADIUS = 1.75;
/** The open sun under the horizon. */
export const SUN_DOWN = { x: 8, y: 13.4, r: 1.35 } as const;

export type SunDot =
  /** A filled dot on the path at x, y. */
  | { kind: 'up'; x: number; y: number }
  /** An open dot under the horizon. */
  | { kind: 'down' }
  /** The horizon and the path alone. */
  | { kind: 'none' };

/** Where the sun sits for a 0..23 game hour and whether it is up. A
 *  sun that is up with no hour stands at the top of the path. A server
 *  hour outside the day clamps to the ends of the path. Null for
 *  `daytime` draws no dot. */
export function sunDot(hour: number | null, daytime: boolean | null): SunDot {
  if (daytime === null) return { kind: 'none' };
  if (!daytime) return { kind: 'down' };
  const known = hour !== null && Number.isFinite(hour);
  const span = NIGHT_FIRST_HOUR - DAY_FIRST_HOUR;
  // Half an hour in, so 6:00 sits just over the horizon and 18:00
  // just before it sets.
  const t = known ? Math.min(Math.max((hour - DAY_FIRST_HOUR + 0.5) / span, 0), 1) : 0.5;
  const angle = Math.PI * (1 - t);
  return {
    kind: 'up',
    x: SUN_CENTER_X + SUN_ARC_RADIUS * Math.cos(angle),
    y: SUN_HORIZON_Y - SUN_ARC_RADIUS * Math.sin(angle),
  };
}

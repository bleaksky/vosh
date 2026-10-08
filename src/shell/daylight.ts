import { liftToContrast, STATUS_TEXT_CONTRAST, type ChromeTokens } from '../theme/chrome';
import { parseHex, toHex } from '../theme/color';
import type { WorldTime } from '../stores/gmcp/worldStore';
import type { XtermPalette } from '../theme/themes';

// The game time's daylight tint and whether its sun is up. The old
// input row chip tinted the time with a fixed color per part of the
// day. The status line keeps those parts of the day but takes each
// color from an ANSI slot of your theme, lifted until it reads as words
// on the status line ground, so a light theme keeps it readable too.

export type DaylightPhase =
  | 'late-night'
  | 'dawn'
  | 'morning'
  | 'midday'
  | 'afternoon'
  | 'dusk'
  | 'evening';

/** The ANSI slots the parts of the day read from. */
export type DaylightSlot = keyof Pick<
  XtermPalette,
  'blue' | 'brightRed' | 'yellow' | 'brightYellow' | 'red' | 'magenta'
>;

/** Each part of the day and the slot that stands in for the old fixed
 *  color. Cool blue at night, coral at dawn, warm yellow in the
 *  morning, gold at midday, amber in the afternoon, orange red at
 *  dusk, violet in the evening. */
export const DAYLIGHT_SLOTS: Readonly<Record<DaylightPhase, DaylightSlot>> = {
  'late-night': 'blue',
  dawn: 'brightRed',
  morning: 'yellow',
  midday: 'brightYellow',
  afternoon: 'yellow',
  dusk: 'red',
  evening: 'magenta',
};

/** The part of the day for a 0..23 game hour, on the old chip's
 *  boundaries. Null while the hour is unknown. */
export function daylightPhase(hour: number | null): DaylightPhase | null {
  if (hour === null || !Number.isFinite(hour) || hour < 0 || hour > 23) return null;
  if (hour >= 22 || hour < 5) return 'late-night';
  if (hour < 7) return 'dawn';
  if (hour < 11) return 'morning';
  if (hour < 14) return 'midday';
  if (hour < 17) return 'afternoon';
  if (hour < 19) return 'dusk';
  return 'evening';
}

/** The time tint for `hour` in a theme. The slot's color, moved in
 *  lightness away from the ground until it reaches the contrast the
 *  chrome gives words drawn in a status tone. Null while the hour is
 *  unknown or the slot does not parse, so the time keeps its plain
 *  color. */
export function daylightTint(
  hour: number | null,
  palette: XtermPalette,
  ground: Pick<ChromeTokens, 'bg' | 'appearance'>,
): string | null {
  const phase = daylightPhase(hour);
  if (phase === null) return null;
  const slot = parseHex(palette[DAYLIGHT_SLOTS[phase]]);
  if (!slot) return null;
  const bg = parseHex(ground.bg);
  if (!bg) return toHex(slot);
  const dir = ground.appearance === 'dark' ? 1 : -1;
  return toHex(liftToContrast(slot, bg, STATUS_TEXT_CONTRAST, dir));
}

/** The first game hour of the day where World.Time names no sunlight.
 *  Aabahran always names it, and day begins with the hour 6 line, `The
 *  day has begun.`, so the fallback agrees with it, as tick.rs does
 *  (Alerts Q17). */
export const DAY_FIRST_HOUR = 6;
/** The first game hour of the night, the hour 19 line, `The night has
 *  begun.`. */
export const NIGHT_FIRST_HOUR = 19;

/** Whether the sun is up, for the sun on its path beside the time.
 *  World.Time sunlight decides when the server sends it. Aabahran
 *  sends dark, rise, light, or set, and only dark is night. Otherwise
 *  the hour decides. Null while neither is known. */
export function isDaytime(time: WorldTime | null): boolean | null {
  if (!time) return null;
  const sun = time.sunlight?.toLowerCase();
  if (sun === 'dark' || sun === 'night') return false;
  if (sun === 'rise' || sun === 'light' || sun === 'set' || sun === 'day') return true;
  if (time.hour === null) return null;
  return time.hour >= DAY_FIRST_HOUR && time.hour < NIGHT_FIRST_HOUR;
}

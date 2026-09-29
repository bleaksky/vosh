import type { WorldTime } from '../../lib/stores/worldStore';

/** Game time from World.Time on a 24 hour clock, like `8:42` or
 *  `20:05`. A 24 hour clock needs no AM or PM to stay unambiguous. A
 *  server that sends only the hour (Aabahran does) reads as the top of
 *  that hour, `8:00`, since its clock moves one hour per tick. Null
 *  while the hour is unknown. */
export function formatGameTime(time: WorldTime | null): string | null {
  if (!time || time.hour === null) return null;
  return `${time.hour}:${String(time.minute ?? 0).padStart(2, '0')}`;
}

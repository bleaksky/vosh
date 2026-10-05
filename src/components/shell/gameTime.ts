import type { GameTime } from '../../ipc/uiConfig';
import type { WorldTime } from '../../lib/stores/worldStore';

/** Game time from World.Time on the clock you pick in Settings under
 *  Layout, then Status line. The 24 hour clock, the default, reads like
 *  `8:42` or `20:05` and needs no AM or PM to stay unambiguous. The 12
 *  hour clock reads like `8:42 AM` or `8:05 PM`, with `12:00 AM` at
 *  midnight and `12:00 PM` at noon. A server that sends only the hour
 *  (Aabahran does) reads as the top of that hour, `8:00`, since its
 *  clock moves one hour per tick. Null while the hour is unknown. */
export function formatGameTime(time: WorldTime | null, clock: GameTime = '24h'): string | null {
  if (!time || time.hour === null) return null;
  const minutes = String(time.minute ?? 0).padStart(2, '0');
  if (clock === '24h') return `${time.hour}:${minutes}`;
  const hour = time.hour % 12 || 12;
  const half = time.hour % 24 < 12 ? 'AM' : 'PM';
  return `${hour}:${minutes} ${half}`;
}

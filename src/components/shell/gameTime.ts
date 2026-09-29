import type { WorldTime } from '../../lib/stores/worldStore';

/** Game time from World.Time, like `8 AM`, or `8:42 AM` from servers
 *  that send minutes. Null while the hour is unknown. */
export function formatGameTime(time: WorldTime | null): string | null {
  if (!time || time.hour === null) return null;
  const hour12 = time.hour % 12 === 0 ? 12 : time.hour % 12;
  const suffix = time.hour < 12 ? 'AM' : 'PM';
  if (time.minute === null) return `${hour12} ${suffix}`;
  return `${hour12}:${String(time.minute).padStart(2, '0')} ${suffix}`;
}

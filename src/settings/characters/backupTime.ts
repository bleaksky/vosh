// How Restore from a backup… names when each backup was set aside.

const DAY = 86_400_000;

/** "Today at 14:02", "Yesterday at 22:15" or "Oct 24 at 09:41". */
export function backupTime(ms: number, now: number = Date.now()): string {
  const at = new Date(ms);
  const time = at.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
  const start = new Date(now);
  start.setHours(0, 0, 0, 0);
  if (ms >= start.getTime()) return `Today at ${time}`;
  if (ms >= start.getTime() - DAY) return `Yesterday at ${time}`;
  const day = at.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  return `${day} at ${time}`;
}

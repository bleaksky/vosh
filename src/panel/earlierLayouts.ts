import type { EarlierLayout } from '../ipc/panes';

// The words each row of Earlier layouts shows: when the layout gave way,
// and for one a step of yours or an update replaced, which step.

const TIME = { hour: 'numeric', minute: '2-digit' } as const;
const DAY = { month: 'short', day: 'numeric' } as const;

function sameDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

/** When an earlier layout gave way: Until 9:41 PM today, Until 11:15 PM
 *  yesterday, or Until Oct 12 at 8:03 PM. */
export function untilLabel(until: number, now: number = Date.now()): string {
  const at = new Date(until);
  const today = new Date(now);
  const yesterday = new Date(now);
  yesterday.setDate(yesterday.getDate() - 1);
  const time = at.toLocaleTimeString('en-US', TIME);
  if (sameDay(at, today)) return `Until ${time} today`;
  if (sameDay(at, yesterday)) return `Until ${time} yesterday`;
  return `Until ${at.toLocaleDateString('en-US', DAY)} at ${time}`;
}

/** The step that replaced an earlier layout, when it was more than a
 *  change: an update, Reset to default, or putting back another one. */
export function earlierNote(earlier: Pick<EarlierLayout, 'why' | 'version'>): string | null {
  switch (earlier.why) {
    case 'update':
      return earlier.version ? `Before ${earlier.version}` : 'Before an update';
    case 'reset':
      return 'Before Reset';
    case 'restore':
      return 'Before a restore';
    default:
      return null;
  }
}

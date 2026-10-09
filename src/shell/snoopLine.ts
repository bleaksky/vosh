import type { SnoopTab } from '../ipc/snoop';
import { howLong } from './sessionLine';

// What the snoop split says of a tab, in the words the sessions sidebar
// uses for how long something has lasted.

/** What a tab says when you point at it: how long a live snoop has been
 *  quiet, in the sidebar's words, or when one ended. */
export function tabTitle(tab: SnoopTab, now: number): string {
  if (!tab.live) return `${tab.name}, ${endedLine(tab, now).toLowerCase()}`;
  const quiet = tab.last_output_at === null ? null : howLong(now - tab.last_output_at);
  return quiet ? `${tab.name}, quiet ${quiet}` : tab.name;
}

/** When an ended snoop ended, in the words the sidebar uses for a
 *  session that dropped. */
export function endedLine(tab: SnoopTab, now: number): string {
  const ago = tab.ended_at === null ? null : howLong(now - tab.ended_at);
  return ago ? `Ended ${ago} ago` : 'Ended just now';
}

// Small text helpers for the panes, kept pure for the unit tests.

/** chatStore joins a Comm.Channel speaker onto the text as
 *  `Speaker: text`. Split one leading single word name back off so the
 *  Chat pane can set it in bold. Lines that triggers route keep their
 *  own wording, since their first word rarely ends in a colon. */
export function splitSpeaker(text: string): { speaker: string | null; text: string } {
  const m = /^([^\s:]{1,32}): ([\s\S]*)$/.exec(text);
  return m ? { speaker: m[1], text: m[2] } : { speaker: null, text };
}

/** Arrival time as the Chat pane shows it, `8:41` on a 24 hour
 *  clock. */
export function chatTime(ts: number): string {
  const at = new Date(ts);
  return `${at.getHours()}:${String(at.getMinutes()).padStart(2, '0')}`;
}

/** The value an Affects row shows: `missing`, the ticks left, or
 *  `permanent`. Empty when the server sent no duration. */
export function ticksLabel(state: string, ticks: number | null): string {
  if (state === 'missing') return 'missing';
  if (ticks === null) return '';
  if (ticks < 0) return 'permanent';
  return String(ticks);
}

/** A vitals value, `1020 / 1020`. */
export function vitalValue(current: number, max: number): string {
  return `${current} / ${max}`;
}

/** Exits in the Map pane's room row, `north east south`. */
export function exitsLabel(exits: readonly string[]): string {
  return exits.join(' ');
}

// Small text helpers for the panes, kept pure for the unit tests.

/** chatStore joins a Comm.Channel speaker onto the text as
 *  `Speaker: text`. Split one leading single word name back off so the
 *  Chat pane can set it in bold. Lines that triggers route keep their
 *  own wording, since their first word rarely ends in a colon. */
export function splitSpeaker(text: string): { speaker: string | null; text: string } {
  const m = /^([^\s:]{1,32}): ([\s\S]*)$/.exec(text);
  return m ? { speaker: m[1], text: m[2] } : { speaker: null, text };
}

const clocks = new Map<string, Intl.DateTimeFormat>();

function clockFor(locale: string | undefined): Intl.DateTimeFormat {
  const key = locale ?? '';
  let clock = clocks.get(key);
  if (!clock) {
    clock = new Intl.DateTimeFormat(locale, { hour: 'numeric', minute: '2-digit' });
    // A 24 hour clock pads the hour, so just after midnight reads
    // 00:55 and never like a duration.
    const cycle = clock.resolvedOptions().hourCycle;
    if (cycle === 'h23' || cycle === 'h24') {
      clock = new Intl.DateTimeFormat(locale, { hour: '2-digit', minute: '2-digit' });
    }
    clocks.set(key, clock);
  }
  return clock;
}

/** Arrival time as the Chat pane shows it, on the wall clock in your
 *  locale's 12 or 24 hour form: `8:41` or `08:41`. A 12 hour clock
 *  drops AM and PM, as the approved boards do, since the messages
 *  around it place it in the day. `locale` is for tests. */
export function chatTime(ts: number, locale?: string): string {
  return clockFor(locale)
    .formatToParts(new Date(ts))
    .filter((part) => part.type !== 'dayPeriod')
    .map((part) => part.value)
    .join('')
    .trim();
}

/** The value an Affects row shows: `missing`, the ticks left, or
 *  `permanent`. Empty when the server sent no duration. */
export function ticksLabel(state: string, ticks: number | null): string {
  if (state === 'missing') return 'missing';
  if (ticks === null) return '';
  if (ticks < 0) return 'permanent';
  return String(ticks);
}

/** The state an Affects row shows only through its marker color, as
 *  words for a screen reader: `expiring` or `harmful`. Null for rows
 *  whose visible text already says it (missing) or that need nothing. */
export function affectStateWord(state: string): string | null {
  if (state === 'expiring' || state === 'harmful') return state;
  return null;
}

/** A vitals value, `1020 / 1020`. */
export function vitalValue(current: number, max: number): string {
  return `${current} / ${max}`;
}

/** Exits in the Map pane's room row, `north east south`. */
export function exitsLabel(exits: readonly string[]): string {
  return exits.join(' ');
}

// Small text helpers for the panes, kept pure for the unit tests.

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
 *  drops AM and PM, since the messages
 *  around it place it in the day. `locale` is for tests. */
export function chatTime(ts: number, locale?: string): string {
  return clockFor(locale)
    .formatToParts(new Date(ts))
    .filter((part) => part.type !== 'dayPeriod')
    .map((part) => part.value)
    .join('')
    .trim();
}

/** The hours an Affects cell shows, with the marks of the game's own
 *  affects bar: `-` for a tracked affect you are missing, `+` for a
 *  permanent one, else the ticks left. Empty when the server sent no
 *  duration. */
export function affectHours(state: string, ticks: number | null): string {
  if (state === 'missing') return '-';
  if (ticks === null) return '';
  if (ticks < 0) return '+';
  return String(ticks);
}

/** What an Affects cell shows only in its hours column and its mark,
 *  as words after the name for a screen reader: `, 31 hours`,
 *  `, 1 hour, running out`, `, permanent, harmful`, `, missing`. */
export function affectWords(state: string, ticks: number | null): string {
  if (state === 'missing') return ', missing';
  const words: string[] = [];
  if (ticks !== null)
    words.push(ticks < 0 ? 'permanent' : `${ticks} hour${ticks === 1 ? '' : 's'}`);
  if (state === 'expiring') words.push('running out');
  if (state === 'harmful') words.push('harmful');
  return words.map((w) => `, ${w}`).join('');
}

/** The sentence the Affects pane shows in place of its rows, or null
 *  when it has rows to draw: before the first list since you
 *  connected, while the game hides your affects, and with nothing on
 *  you and nothing tracked. */
export function affectsEmptyText(
  current: readonly unknown[] | null,
  hidden: boolean,
  rows: readonly unknown[],
): string | null {
  if (hidden) return 'The game hides your affects right now.';
  if (current === null) return 'Affects appear when you log in.';
  if (rows.length === 0) return 'Nothing affects you right now.';
  return null;
}

/** Exits in the Map pane's room row, `north east south`. */
export function exitsLabel(exits: readonly string[]): string {
  return exits.join(' ');
}

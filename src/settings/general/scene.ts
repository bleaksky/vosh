import type { LogSession, SceneFilter, SceneRange } from '../../ipc/logs';
import { formatCount, isLocalHost, logTime } from './logView';

// Save a scene (board 5 of the Alerts and Scenes review), the words and
// numbers its page shows and the range it starts on. The page itself is
// ScenePage.tsx.

/** The channels a scene can leave out, as Comm.Channel names them. */
export const SCENE_CHANNELS: readonly string[] = [
  'tell',
  'newbie',
  'pray',
  'immortal',
  'imp',
  'say',
  'yell',
  'gtell',
  'cabal',
  'clan',
  'faction',
];

/** What a scene leaves out at first: your prompt and your commands, and
 *  the tells, newbie, prayers and the staff channels (Q10). */
export const FIRST_FILTER: SceneFilter = {
  prompts: false,
  commands: false,
  leftOut: ['tell', 'newbie', 'pray', 'immortal', 'imp'],
};

/** How far back a scene reaches when it opens. */
const FIRST_SPAN_MS = 15 * 60_000;
const MINUTE_MS = 60_000;
const DAY_MS = 86_400_000;

/** The start of the minute `ms` falls in. */
function minuteOf(ms: number): number {
  const d = new Date(ms);
  d.setSeconds(0, 0);
  return d.getTime();
}

/** The range a scene opens on: the last 15 minutes of `log`, or less
 *  when the log is shorter. `now` ends a log that still runs. */
export function firstRange(log: LogSession, now: number = Date.now()): SceneRange {
  const end = log.ended_at_ms ?? now;
  const from = Math.max(log.started_at_ms, end - FIRST_SPAN_MS);
  return { log: log.id, fromMs: minuteOf(from), toMs: minuteOf(end) + MINUTE_MS - 1 };
}

/** A time you typed on the 24 hour clock, `21:14` or `9:05`, as hours
 *  and minutes, or null when it is not one. */
export function parseClock(text: string): { hours: number; minutes: number } | null {
  const m = /^\s*(\d{1,2})[:.](\d{2})\s*$/.exec(text);
  if (!m) return null;
  const hours = Number(m[1]);
  const minutes = Number(m[2]);
  if (hours > 23 || minutes > 59) return null;
  return { hours, minutes };
}

/** The first moment at or after the minute of `after` that reads
 *  `hours:minutes` on the local clock, so a time past midnight in a log
 *  that runs into the next day lands on that day. */
export function clockAfter(after: number, hours: number, minutes: number): number {
  const d = new Date(after);
  d.setHours(hours, minutes, 0, 0);
  let at = d.getTime();
  if (at < minuteOf(after)) at += DAY_MS;
  return at;
}

/** The range with From set to the time you typed, or null when the text
 *  is no time. From starts on that minute of the log's own clock, and To
 *  moves with it when From passes it. */
export function withFrom(range: SceneRange, log: LogSession, text: string): SceneRange | null {
  const clock = parseClock(text);
  if (!clock) return null;
  const fromMs = clockAfter(log.started_at_ms, clock.hours, clock.minutes);
  const toMs = Math.max(range.toMs, fromMs + MINUTE_MS - 1);
  return { log: range.log, fromMs, toMs };
}

/** The range with To set to the time you typed, the end of that minute
 *  at or after From, or null when the text is no time. */
export function withTo(range: SceneRange, text: string): SceneRange | null {
  const clock = parseClock(text);
  if (!clock) return null;
  const toMs = clockAfter(range.fromMs, clock.hours, clock.minutes) + MINUTE_MS - 1;
  return { ...range, toMs, toId: null };
}

/** The range that starts on the line `id` at `ts`, for a click on its
 *  time in the preview. */
export function startingAt(range: SceneRange, id: number, ts: number): SceneRange {
  return { ...range, fromMs: minuteOf(ts), fromId: id };
}

/** The range that ends on the line `id` at `ts`, for a Shift click. */
export function endingAt(range: SceneRange, id: number, ts: number): SceneRange {
  return { ...range, toMs: minuteOf(ts) + MINUTE_MS - 1, toId: id };
}

/** The clock a From or To field shows. */
export function clockText(ms: number): string {
  return logTime(ms);
}

/** The count over the preview, `12 of 20 lines`. */
export function keptText(kept: number, total: number): string {
  if (total === 0) return 'No lines';
  return `${formatCount(kept)} of ${formatCount(total)} ${total === 1 ? 'line' : 'lines'}`;
}

/** The channels Add offers: the ones not left out yet, in the list's
 *  order. */
export function addable(leftOut: readonly string[]): string[] {
  return SCENE_CHANNELS.filter((name) => !leftOut.includes(name));
}

/** True when the selected session's profile logs the world it dials:
 *  your Log sessions choice, or until you choose, every world but this
 *  computer (D34). */
export function logsWorld(logSessions: boolean | null, host: string): boolean {
  return logSessions ?? !isLocalHost(host);
}

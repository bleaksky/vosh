import { normalizeAffectName } from '../../lib/affects';
import { CRITICAL_TICKS, EXPIRING_TICKS, type AffectFulls } from '../../ipc/affects';
import { HARMFUL_AFFECTS, harmfulSet } from './harmfulAffects';

// View model for the Affects pane, the at a glance checklist, timers
// first. Pure so the ordering rules are unit tested
// without a pane or a server.
//
// Rows come out in two runs:
//   1. every tracked affect in the order you set in Characters, each in
//      its slot whether you have it or not, marked expiring at the hours
//      you set it runs out at, two unless you change them;
//   2. affects you have but do not track, harmful ones first, each run
//      fewest ticks first, permanent after every timed one, ties broken
//      by name.
// Durations are server ticks, which the game prints as hours. -1 (any
// negative) means permanent and null means the server sent no usable
// duration. Each row carries the tone its hours take by the thresholds
// you set (Settings, Layout, Affects), so every style reads one rule.

export type AffectRowState = 'missing' | 'present' | 'expiring' | 'untracked' | 'harmful';

export interface AffectRow {
  /** Normalized affect name. Stable across pushes, so it doubles as
   *  the React key. */
  key: string;
  /** Display name. The tracked label when you set one, else the name
   *  exactly as the game sends it, or as you track it while missing. */
  name: string;
  state: AffectRowState;
  /** Ticks left. -1 is permanent. null for a missing affect or an
   *  unknown duration. */
  ticks: number | null;
  /** The color its hours take, by the thresholds the view was built
   *  with. Null for a missing affect, a permanent one, one with no
   *  duration, and one with hours to spare. */
  tone: HoursTone | null;
}

/** The slice of a current affect the view reads. The affects store's
 *  rows satisfy it. */
export interface AffectInput {
  name: string;
  duration: number | null;
}

/** The slice of a tracked entry the view reads. */
export interface TrackedInput {
  name: string;
  label?: string | null;
}

/** When an affect's hours change color, in whole ticks. The profile
 *  keeps both (affects_running_out_hours and affects_almost_gone_hours
 *  under [ui]), and the backend holds almost gone to at most running
 *  out. */
export interface AffectThresholds {
  /** At or under this many, a tracked affect counts as running out and
   *  its hours turn yellow. */
  runningOut: number;
  /** At or under this many, the hours turn bold red. */
  almostGone: number;
}

/** Today's thresholds: yellow at two hours, red at one or none. */
export const DEFAULT_AFFECT_THRESHOLDS: AffectThresholds = {
  runningOut: EXPIRING_TICKS,
  almostGone: CRITICAL_TICKS,
};

export function isTrackedRow(row: AffectRow): boolean {
  return row.state === 'missing' || row.state === 'present' || row.state === 'expiring';
}

export function affectsView(
  current: readonly AffectInput[],
  tracked: readonly TrackedInput[],
  harmfulNames: Iterable<string> = HARMFUL_AFFECTS,
  thresholds: AffectThresholds = DEFAULT_AFFECT_THRESHOLDS,
): AffectRow[] {
  const harmful = harmfulSet(harmfulNames);

  const live = new Map<string, AffectInput>();
  for (const affect of current) {
    const key = normalizeAffectName(affect.name);
    if (key.length === 0) continue;
    const prev = live.get(key);
    // Two rows that normalize alike keep the one that lasts longer, so
    // the pane never warns about an affect that is still up.
    if (!prev || lasting(ticksOf(affect.duration)) > lasting(ticksOf(prev.duration))) {
      live.set(key, affect);
    }
  }

  const slots: AffectRow[] = [];
  const trackedKeys = new Set<string>();
  for (const entry of tracked) {
    const key = normalizeAffectName(entry.name);
    if (key.length === 0 || trackedKeys.has(key)) continue;
    trackedKeys.add(key);
    const label = entry.label?.trim();
    const affect = live.get(key);
    if (!affect) {
      slots.push({ key, name: label || entry.name, state: 'missing', ticks: null, tone: null });
      continue;
    }
    const ticks = ticksOf(affect.duration);
    // Any tone means the hours are at or under running out.
    const tone = hoursTone(ticks, thresholds);
    slots.push({
      key,
      name: label || affect.name,
      state: tone ? 'expiring' : 'present',
      ticks,
      tone,
    });
  }

  const others: AffectRow[] = [];
  for (const [key, affect] of live) {
    if (trackedKeys.has(key)) continue;
    const ticks = ticksOf(affect.duration);
    others.push({
      key,
      name: affect.name,
      state: harmful.has(key) ? 'harmful' : 'untracked',
      ticks,
      tone: hoursTone(ticks, thresholds),
    });
  }
  others.sort((a, b) => {
    const byHarm = Number(b.state === 'harmful') - Number(a.state === 'harmful');
    if (byHarm !== 0) return byHarm;
    const byTicks = rank(a.ticks) - rank(b.ticks);
    if (byTicks !== 0) return byTicks;
    return a.key.localeCompare(b.key);
  });

  return [...slots, ...others];
}

export type HoursTone = 'danger' | 'warn';

/** The color of an affect's hours: bold red at almost gone or under,
 *  yellow at running out or under. Unless you set others, that is the
 *  game's rule, one hour or none in red, and Vosh warns a tick earlier,
 *  at two, in yellow. The same rule holds for every affect, tracked or
 *  not. Almost gone never reaches past running out, so equal values
 *  leave no yellow stage. */
export function hoursTone(
  ticks: number | null,
  thresholds: AffectThresholds = DEFAULT_AFFECT_THRESHOLDS,
): HoursTone | null {
  if (ticks === null || ticks < 0) return null;
  const { runningOut, almostGone } = thresholds;
  if (ticks <= Math.min(almostGone, runningOut)) return 'danger';
  if (ticks <= runningOut) return 'warn';
  return null;
}

/** The mark before an affect. A tracked slot takes a dot that agrees
 *  with its hours (`up`, `warn`, `danger`) or a hollow ring while you
 *  are `missing` it. A harmful affect you do not track takes the
 *  `harmful` diamond. Anything else has no mark. */
export type AffectMark = 'up' | 'warn' | 'danger' | 'missing' | 'harmful';

export function affectMark(row: AffectRow): AffectMark | null {
  if (row.state === 'missing') return 'missing';
  if (row.state === 'harmful') return 'harmful';
  if (row.state === 'untracked') return null;
  return row.tone ?? 'up';
}

/** How full an affect's gauge is, 0 to 1: the hours left over the
 *  hours at full. Missing is empty. Permanent is full and never drains.
 *  An affect with no full yet reads as full. Null when the server sent
 *  no duration, which draws no gauge. */
export function gaugeFraction(row: AffectRow, full: AffectFulls): number | null {
  if (row.state === 'missing') return 0;
  if (row.ticks === null) return null;
  if (row.ticks < 0) return 1;
  const top = full[row.key];
  if (top === undefined || !(top > 0)) return 1;
  return Math.min(1, Math.max(0, row.ticks / Math.max(top, row.ticks)));
}

/** What the pane header counts: tracked affects you are missing, and
 *  tracked affects running out, at or under the hours you set, two
 *  unless you change them. */
export function affectsSummary(rows: readonly AffectRow[]): {
  missing: number;
  runningOut: number;
} {
  let missing = 0;
  let runningOut = 0;
  for (const row of rows) {
    if (row.state === 'missing') missing += 1;
    else if (row.state === 'expiring') runningOut += 1;
  }
  return { missing, runningOut };
}

/** The rows the Affects pane draws. None before the first list since you
 *  connected, and none while the game hides your affects (lamented
 *  tears sends an empty list with the hidden flag), so no tracked
 *  affect reads missing then. */
export function affectsPaneRows(
  current: readonly AffectInput[] | null,
  tracked: readonly TrackedInput[],
  hidden: boolean,
  thresholds: AffectThresholds = DEFAULT_AFFECT_THRESHOLDS,
): AffectRow[] {
  if (current === null || hidden) return [];
  return affectsView(current, tracked, HARMFUL_AFFECTS, thresholds);
}

/** Coerce a duration into ticks: a whole number, -1 for any negative
 *  (permanent), null when unknown. */
function ticksOf(duration: number | null | undefined): number | null {
  if (typeof duration !== 'number' || !Number.isFinite(duration)) return null;
  if (duration < 0) return -1;
  return Math.floor(duration);
}

/** Sort key: timed affects by ticks, then permanent, then unknown. */
function rank(ticks: number | null): number {
  if (ticks === null) return Number.MAX_SAFE_INTEGER;
  if (ticks < 0) return Number.MAX_SAFE_INTEGER - 1;
  return ticks;
}

/** How long an affect stays up: permanent beats any timed one, and an
 *  unknown duration loses to both. */
function lasting(ticks: number | null): number {
  if (ticks === null) return -1;
  if (ticks < 0) return Number.MAX_SAFE_INTEGER;
  return ticks;
}

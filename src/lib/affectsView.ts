import { normalizeAffectName } from './affects';
import { HARMFUL_AFFECTS, harmfulSet } from './harmfulAffects';

// View model for the Affects pane, the at a glance checklist. Pure so
// the ordering rules are unit tested without a pane or a server.
//
// Rows come out in three runs:
//   1. tracked affects you do not have, in the order you track them;
//   2. tracked affects you have, fewest ticks first, permanent after
//      every timed one, marked expiring at two ticks or fewer;
//   3. affects you have but do not track, harmful ones first, each run
//      fewest ticks first, ties broken by name.
// Durations are server ticks. -1 (any negative) means permanent and
// null means the server sent no usable duration.

export type AffectRowState = 'missing' | 'present' | 'expiring' | 'untracked' | 'harmful';

export interface AffectRow {
  /** Normalized affect name. Stable across pushes, so it doubles as
   *  the React key. */
  key: string;
  /** Display name. The tracked label when you set one, else the
   *  server's name in sentence case. */
  name: string;
  state: AffectRowState;
  /** Ticks left. -1 is permanent. null for a missing affect or an
   *  unknown duration. */
  ticks: number | null;
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

/** A tracked affect at or under this many ticks reads as about to
 *  drop. An affect at 0 goes on the next tick. */
export const EXPIRING_TICKS = 2;

export function isTrackedRow(row: AffectRow): boolean {
  return row.state === 'missing' || row.state === 'present' || row.state === 'expiring';
}

export function affectsView(
  current: readonly AffectInput[],
  tracked: readonly TrackedInput[],
  harmfulNames: Iterable<string> = HARMFUL_AFFECTS,
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

  const missing: AffectRow[] = [];
  const present: AffectRow[] = [];
  const trackedKeys = new Set<string>();
  for (const entry of tracked) {
    const key = normalizeAffectName(entry.name);
    if (key.length === 0 || trackedKeys.has(key)) continue;
    trackedKeys.add(key);
    const label = entry.label?.trim();
    const affect = live.get(key);
    if (!affect) {
      missing.push({ key, name: label || sentenceCase(entry.name), state: 'missing', ticks: null });
      continue;
    }
    const ticks = ticksOf(affect.duration);
    present.push({
      key,
      name: label || sentenceCase(affect.name),
      state: ticks !== null && ticks >= 0 && ticks <= EXPIRING_TICKS ? 'expiring' : 'present',
      ticks,
    });
  }
  // Array.prototype.sort is stable, so equal ticks keep tracked order.
  present.sort((a, b) => rank(a.ticks) - rank(b.ticks));

  const others: AffectRow[] = [];
  for (const [key, affect] of live) {
    if (trackedKeys.has(key)) continue;
    others.push({
      key,
      name: sentenceCase(affect.name),
      state: harmful.has(key) ? 'harmful' : 'untracked',
      ticks: ticksOf(affect.duration),
    });
  }
  others.sort((a, b) => {
    const byHarm = Number(b.state === 'harmful') - Number(a.state === 'harmful');
    if (byHarm !== 0) return byHarm;
    const byTicks = rank(a.ticks) - rank(b.ticks);
    if (byTicks !== 0) return byTicks;
    return a.key.localeCompare(b.key);
  });

  return [...missing, ...present, ...others];
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

/** Collapse runs of spaces and upper case the first letter of an all
 *  lower case server name ("giant strength" reads "Giant strength").
 *  Names that already carry capitals keep the case the server or you
 *  wrote them in. */
function sentenceCase(raw: string): string {
  const name = raw.replace(/\s+/g, ' ').trim();
  if (name.length === 0 || name !== name.toLowerCase()) return name;
  return name.charAt(0).toUpperCase() + name.slice(1);
}

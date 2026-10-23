import type { KindNoun } from './automationDraft';
import { compareGroups, groupKeyOf } from './automationList';

// Moving several items of one list to a group at once, from the card
// for the rows you picked or from a row's menu.

/** A name more than one item would share in one group, and how many. */
export interface MoveClash {
  name: string;
  count: number;
}

/** The names the move of the items at `moved` to `group` would leave
 *  more than one item holding in one group, in list order, each once
 *  with how many would hold it. `keyOf` is the list's own key, so a kind that knows an
 *  item by its group and name clashes and one that knows it by a key
 *  never does. */
export function moveClashes<T>(
  values: readonly T[],
  moved: ReadonlySet<number>,
  group: string,
  regroup: (value: T, group: string) => T,
  keyOf: (value: T) => string,
  nameOf: (value: T) => string,
): MoveClash[] {
  const after = values.map((v, i) => (moved.has(i) ? regroup(v, group) : v));
  const seen = new Map<string, number>();
  for (const v of after) {
    const key = keyOf(v);
    seen.set(key, (seen.get(key) ?? 0) + 1);
  }
  const clashes: MoveClash[] = [];
  after.forEach((v, i) => {
    const count = seen.get(keyOf(v)) ?? 0;
    // An item whose key the move leaves alone clashes as it did before,
    // which Save already names, so only a new key counts.
    if (!moved.has(i) || count < 2 || keyOf(v) === keyOf(values[i])) return;
    const name = nameOf(v);
    if (!clashes.some((c) => c.name === name)) clashes.push({ name, count });
  });
  return clashes;
}

const COUNT_WORDS = ['', 'one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight', 'nine'];

/** What the card says before a move that would leave two items of one
 *  name in a group, or null when none would. Save refuses that list,
 *  so the card says it first. */
export function moveClashNote(
  noun: KindNoun,
  clashes: readonly MoveClash[],
  group: string,
): string | null {
  if (clashes.length === 0) return null;
  const where = groupKeyOf(group);
  // The first pair names the kind after a group, as in `two aliases
  // named ds`, and the sentence about no group names it first.
  const pairs = clashes.map(({ name, count }, i) => {
    const n = COUNT_WORDS[count] ?? String(count);
    return i === 0 && where ? `${n} ${noun.many} named ${name}` : `${n} named ${name}`;
  });
  const what =
    pairs.length === 1
      ? pairs[0]
      : `${pairs.slice(0, -1).join(', ')} and ${pairs[pairs.length - 1]}`;
  const fix = clashes.length === 1 ? 'Rename one' : 'Rename one of each';
  return where
    ? `${where} would then have ${what}. ${fix} before you save.`
    : `Your ${noun.many} with no group would then include ${what}. ${fix} before you save.`;
}

/** Every group name in `lists`, once each, in the order the list shows
 *  its headings. A blank name is no group and never shows. */
export function groupChoices(...lists: readonly (readonly string[])[]): string[] {
  const names = new Set<string>();
  for (const list of lists) {
    for (const name of list) {
      const group = groupKeyOf(name);
      if (group) names.add(group);
    }
  }
  return [...names].sort(compareGroups);
}

/** The group every one of `groups` shares, blank for none, or null when
 *  they differ. The Move row starts from it. */
export function sharedGroup(groups: readonly string[]): string | null {
  if (groups.length === 0) return null;
  const first = groupKeyOf(groups[0]);
  return groups.every((g) => groupKeyOf(g) === first) ? first : null;
}

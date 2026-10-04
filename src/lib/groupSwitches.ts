// The on and off switch on each group heading in Settings, Automation.
// It turns the whole group at once, the way #group does, and keeps the
// items' own switches as they are. While the loadouts decide a group,
// its switch shows what they set and waits, with a note that names them.

import type { GroupSwitch, LoadoutHold } from './session';

/** The switches of a list by group name. A reply that is not a list
 *  holds none. */
export function switchesByName(list: unknown): ReadonlyMap<string, GroupSwitch> {
  const out = new Map<string, GroupSwitch>();
  if (!Array.isArray(list)) return out;
  for (const raw of list as unknown[]) {
    if (!raw || typeof raw !== 'object') continue;
    const r = raw as Record<string, unknown>;
    if (typeof r.name !== 'string' || r.name === '') continue;
    const sw: GroupSwitch = { name: r.name, enabled: r.enabled !== false };
    const hold = r.loadouts as Record<string, unknown> | null | undefined;
    if (hold && typeof hold === 'object') {
      sw.loadouts = {
        on: hold.on === true,
        by: Array.isArray(hold.by) ? hold.by.filter((n): n is string => typeof n === 'string') : [],
      };
    }
    out.set(sw.name, sw);
  }
  return out;
}

/** `switches` with the group `name` turned, the way the page shows a
 *  switch you flipped before the answer comes back. */
export function withSwitch(
  switches: ReadonlyMap<string, GroupSwitch>,
  name: string,
  enabled: boolean,
): ReadonlyMap<string, GroupSwitch> {
  const current = switches.get(name);
  if (!current || current.enabled === enabled) return switches;
  const next = new Map(switches);
  next.set(name, { ...current, enabled });
  return next;
}

/** Names in a sentence, with a serial comma before the last of three
 *  or more. */
function nameList(names: readonly string[]): string {
  if (names.length <= 2) return names.join(' and ');
  return `${names.slice(0, -1).join(', ')}, and ${names[names.length - 1]}`;
}

/** The note under a heading whose group the loadouts decide. */
export function loadoutHoldNote(hold: LoadoutHold): string {
  if (hold.by.length === 0) return 'Every loadout is off, so this group stays off.';
  const one = hold.by.length === 1;
  const who = `The ${nameList(hold.by)} ${one ? 'loadout' : 'loadouts'}`;
  if (hold.on) return `${who} ${one ? 'turns' : 'turn'} this group on.`;
  return `${who} ${one ? 'leaves' : 'leave'} this group off.`;
}

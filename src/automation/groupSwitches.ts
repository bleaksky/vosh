// The on and off switch on each group heading in Settings, Automation.
// It turns the whole group at once, the way #group does, and keeps the
// items' own switches as they are. While the loadouts decide a group,
// its switch still turns it, with a note that names them and says when
// they turn back a group the switch or #group turned.

import type { GroupSwitch, LoadoutHold } from '../ipc/automation';
import { listJoin } from '../lib/text';

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

/** What Settings says when the switches do not load. The list still
 *  shows, with the switches it had. */
export const groupSwitchesLoadError =
  "Vosh couldn't load your group switches. Close Settings and open it again.";

/** When the loadouts lay their state over every group again. */
const UNTIL = 'when you next launch Vosh, switch profiles, or save Loadouts';

/** The note under a heading whose group the loadouts decide. `enabled`
 *  is the group's switch. The switch, `#group` and Lua all turn a group
 *  the loadouts decide, so the switch can differ from the loadouts until
 *  they lay their state over the group again, and the note says when. */
export function loadoutHoldNote(hold: LoadoutHold, enabled: boolean): string {
  const turned = enabled !== hold.on;
  if (hold.by.length === 0) {
    return turned
      ? `Every loadout is off, so this group goes off again ${UNTIL}.`
      : 'Every loadout is off, so this group stays off.';
  }
  const one = hold.by.length === 1;
  const who = `The ${listJoin(hold.by)} ${one ? 'loadout' : 'loadouts'}`;
  const verb = one ? 'turns' : 'turn';
  if (turned) return `${who} ${verb} this group ${hold.on ? 'on' : 'off'} again ${UNTIL}.`;
  if (hold.on) return `${who} ${verb} this group on.`;
  return `${who} ${one ? 'leaves' : 'leave'} this group off.`;
}

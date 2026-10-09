import { useCallback, useEffect, useRef, useState } from 'react';
import { automationSaveError } from '../../automation/automationRecords';
import { groupSwitchesLoadError, switchesByName, withSwitch } from '../../automation/groupSwitches';
import {
  listGroupSwitches,
  setGroupEnabled,
  subscribeGroupsChanged,
  type GroupList,
  type GroupSwitch,
} from '../../ipc/automation';
import { subscribeLoadoutsChanged } from '../../ipc/loadouts';
import { getShownProfile } from '../shownProfile';

/** The switches on one list's group headings, and how to turn one. */
export interface GroupSwitches {
  /** The switch of each group the store knows, by name. A group only
   *  the draft has gets one once you save it. */
  byName: ReadonlyMap<string, GroupSwitch>;
  /** Turn a whole group on or off at once. The switch moves now, and
   *  the store's answer replaces it. */
  turn: (group: string, enabled: boolean) => void;
}

const NONE: ReadonlyMap<string, GroupSwitch> = new Map();

/** The group switches of `list` in the profile Settings shows, or null
 *  for a list with none. They load again each time `loaded` changes, the
 *  list the editor last loaded or saved, and whenever a group turns or
 *  the loadouts change anywhere. A switch acts at once and never waits
 *  for Save, since a group's state lives apart from its items. A load
 *  that fails says so through `onError`. */
export function useGroupSwitches(
  list: GroupList | null,
  loaded: unknown,
  onError: (message: string | null) => void,
): GroupSwitches | null {
  const [byName, setByName] = useState<ReadonlyMap<string, GroupSwitch>>(NONE);
  // Only the newest answer lands, so a slow load never undoes a switch.
  const seq = useRef(0);

  const reload = useCallback(() => {
    if (!list) return;
    const mine = ++seq.current;
    listGroupSwitches(list, getShownProfile())
      .then((next) => {
        if (seq.current === mine) setByName(switchesByName(next));
      })
      .catch(() => {
        // Keep the switches it had. A load that works never clears the
        // message, since that would hide a save error.
        if (seq.current === mine) onError(groupSwitchesLoadError);
      });
  }, [list, onError]);

  useEffect(() => {
    reload();
  }, [reload, loaded]);

  useEffect(() => {
    if (!list) return;
    let cancelled = false;
    const unsubs: (() => void)[] = [];
    for (const subscribe of [subscribeGroupsChanged, subscribeLoadoutsChanged]) {
      void subscribe(() => {
        if (!cancelled) reload();
      })
        .then((fn) => {
          if (cancelled) fn();
          else unsubs.push(fn);
        })
        .catch(() => {});
    }
    return () => {
      cancelled = true;
      for (const fn of unsubs) fn();
    };
  }, [list, reload]);

  const turn = useCallback(
    (group: string, enabled: boolean) => {
      if (!list) return;
      const mine = ++seq.current;
      setByName((now) => withSwitch(now, group, enabled));
      setGroupEnabled(list, group, enabled, getShownProfile())
        .then((next) => {
          if (seq.current === mine) setByName(switchesByName(next));
        })
        .catch((e) => {
          onError(automationSaveError(e));
          reload();
        });
    },
    [list, onError, reload],
  );

  return list ? { byName, turn } : null;
}

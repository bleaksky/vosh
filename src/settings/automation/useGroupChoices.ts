import { useEffect, useMemo, useState } from 'react';
import { groupChoices } from '../../automation/groupMoves';
import { listGroupSwitches, type GroupList } from '../../ipc/automation';
import { getShownProfile } from '../shownProfile';

const LISTS: readonly GroupList[] = ['triggers', 'aliases', 'macros', 'timers'];
const NONE: readonly string[] = [];

/** The groups the Group fields offer: every group the four lists hold
 *  in the profile Settings shows, and `own`, the ones this list's draft
 *  names before you save. #group and the loadouts treat a group of one
 *  name as one group in every list, so a field offers them all. They
 *  load again each time `loaded` changes, the list the editor last
 *  loaded or saved. A list that fails to load offers none of its own. */
export function useGroupChoices(loaded: unknown, own: readonly string[]): string[] {
  const [saved, setSaved] = useState<readonly string[]>(NONE);
  useEffect(() => {
    let cancelled = false;
    const profile = getShownProfile();
    void Promise.all(
      LISTS.map((list) =>
        listGroupSwitches(list, profile)
          .then((rows) => rows.map((row) => row.name))
          .catch(() => [] as string[]),
      ),
    ).then((lists) => {
      if (!cancelled) setSaved(lists.flat());
    });
    return () => {
      cancelled = true;
    };
  }, [loaded]);
  return useMemo(() => groupChoices(saved, own), [saved, own]);
}

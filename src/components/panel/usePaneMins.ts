import { useMemo, useSyncExternalStore } from 'react';
import { affectsPaneRows } from '../../lib/affectsView';
import { getGroupState, subscribeGroupState } from '../../lib/groupStore';
import { useAffectsDisplay } from '../../lib/stores/affectsDisplayStore';
import { useAffects, useAffectsHidden } from '../../lib/stores/affectsStore';
import { useTrackedAffects } from '../../lib/stores/trackedAffectsStore';
import type { PaneSplit } from '../../lib/paneLayout';
import { affectsMinIn, affectsStyleMinH, groupMinH, type PaneMins } from './paneGeometry';

// The minimum heights that follow what the Affects and Group panes
// show right now, for PanelHost to lay `root` out `width` wide with.
// The panes order their own rows the same way, so the minimum covers
// the rows they draw in the style you picked. The Affects pane draws
// one column or two by its own width, which affectsMinIn reads from the
// tree.

function subscribeMembers(cb: () => void): () => void {
  return subscribeGroupState(() => cb());
}

// A number, so it reads the same between pushes.
function memberCount(): number {
  const { group } = getGroupState();
  return group.leader && Array.isArray(group.members) ? group.members.length : 0;
}

export function usePaneMins(root: PaneSplit | null, width: number): PaneMins {
  const current = useAffects();
  const tracked = useTrackedAffects();
  const hidden = useAffectsHidden();
  const { style } = useAffectsDisplay();
  const members = useSyncExternalStore(subscribeMembers, memberCount);
  const rows = useMemo(() => affectsPaneRows(current, tracked, hidden), [current, tracked, hidden]);
  const affects = useMemo(
    () => (root ? affectsMinIn(root, width, rows, style) : affectsStyleMinH(rows, 2, style)),
    [root, width, rows, style],
  );
  const group = groupMinH(members);
  return useMemo(() => ({ affects, group }), [affects, group]);
}

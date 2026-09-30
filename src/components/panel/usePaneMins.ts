import { useMemo, useSyncExternalStore } from 'react';
import { affectsPaneRows } from '../../lib/affectsView';
import { getGroupState, subscribeGroupState } from '../../lib/groupStore';
import { useAffects, useAffectsHidden } from '../../lib/stores/affectsStore';
import { useTrackedAffects } from '../../lib/stores/trackedAffectsStore';
import { affectsColumns } from './affectsGrid';
import { affectsMinH, groupMinH, type PaneMins } from './paneGeometry';

// The minimum heights that follow what the Affects and Group panes
// show right now, for PanelHost to lay the tree out with. The panes
// order their own rows the same way, so the minimum covers the rows
// they draw. The Affects pane draws one column or two by its width,
// which the panel's `width` stands in for.

function subscribeMembers(cb: () => void): () => void {
  return subscribeGroupState(() => cb());
}

// A number, so it reads the same between pushes.
function memberCount(): number {
  const { group } = getGroupState();
  return group.leader && Array.isArray(group.members) ? group.members.length : 0;
}

export function usePaneMins(width: number): PaneMins {
  const current = useAffects();
  const tracked = useTrackedAffects();
  const hidden = useAffectsHidden();
  const members = useSyncExternalStore(subscribeMembers, memberCount);
  const columns = affectsColumns(width);
  const affects = useMemo(
    () => affectsMinH(affectsPaneRows(current, tracked, hidden), columns),
    [current, tracked, hidden, columns],
  );
  const group = groupMinH(members);
  return useMemo(() => ({ affects, group }), [affects, group]);
}

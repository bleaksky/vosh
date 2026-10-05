import { useMemo, useSyncExternalStore } from 'react';
import { affectThresholdsOf } from '../../lib/affectsDisplay';
import { affectsPaneRows } from '../../lib/affectsView';
import { getGroupState, subscribeGroupState } from '../../stores/gmcp/groupStore';
import { useAffectsDisplay } from '../../stores/config/affectsDisplayStore';
import { useAffects, useAffectsHidden } from '../../stores/gmcp/affectsStore';
import { useTrackedAffects } from '../../stores/config/trackedAffectsStore';
import type { PaneSplit } from '../../lib/paneLayout';
import { affectsTwoColumnsW } from './affectsGrid';
import { useChipMeasure } from './chipMeasure';
import { affectsMinIn, affectsStyleMinH, groupMinH, type PaneMins } from './paneGeometry';

// The minimum heights that follow what the Affects and Group panes
// show right now, for PanelHost to lay `root` out `width` wide with.
// The panes order their own rows the same way, so the minimum covers
// the rows they draw in the style you picked. The Affects pane draws
// one column or two by its own width, which affectsMinIn reads from the
// tree. Every pane draws at your panel `size`, so each counts its rows
// at that size, and layoutPanes gives the rest their stock minimum at
// it.

function subscribeMembers(cb: () => void): () => void {
  return subscribeGroupState(() => cb());
}

// A number, so it reads the same between pushes.
function memberCount(): number {
  const { group } = getGroupState();
  return group.leader && Array.isArray(group.members) ? group.members.length : 0;
}

export function usePaneMins(root: PaneSplit | null, width: number, size: number): PaneMins {
  const current = useAffects();
  const tracked = useTrackedAffects();
  const hidden = useAffectsHidden();
  const display = useAffectsDisplay();
  const { style } = display;
  // The rows running out are the ones the pane must hold, so the
  // minimum reads the same hours the pane draws by.
  const thresholds = useMemo(() => affectThresholdsOf(display), [display]);
  // The chips pack with the pane's own measure, so the minimum holds
  // the lines the pane draws.
  const measure = useChipMeasure(size);
  const members = useSyncExternalStore(subscribeMembers, memberCount);
  const rows = useMemo(
    () => affectsPaneRows(current, tracked, hidden, thresholds),
    [current, tracked, hidden, thresholds],
  );
  const affects = useMemo(
    () =>
      root
        ? affectsMinIn(root, width, rows, style, measure, size)
        : affectsStyleMinH(rows, affectsTwoColumnsW(size), style, measure, size),
    [root, width, rows, style, measure, size],
  );
  const group = groupMinH(members, size);
  return useMemo(() => ({ affects, group }), [affects, group]);
}

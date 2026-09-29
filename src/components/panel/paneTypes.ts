import { getImmState } from '../../lib/immStore';
import { PANE_TYPES, allPanes, type PaneSplit, type PaneType } from '../../lib/paneLayout';

// Names for each pane type, shared by the pane headers, the pane menu,
// and the title band's Add a pane menu so all three say the same thing.

export const PANE_LABELS: Record<PaneType, string> = {
  map: 'Map',
  affects: 'Affects',
  group: 'Group',
  chat: 'Chat',
  imm: 'Staff queues',
};

/** Pane types this session offers. The staff queues pane only shows
 *  up once the server has sent Imm.Queues, which it does for
 *  immortals alone. */
export function offeredPaneTypes(): PaneType[] {
  const staff = getImmState().received;
  return PANE_TYPES.filter((t) => t !== 'imm' || staff);
}

/** Offered pane types the tree does not show yet, in menu order. */
export function paneTypesToAdd(tree: PaneSplit | null): PaneType[] {
  const shown = new Set(tree ? allPanes(tree) : []);
  return offeredPaneTypes().filter((t) => !shown.has(t));
}

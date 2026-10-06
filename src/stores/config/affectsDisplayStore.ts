import {
  affectsDisplayOf,
  DEFAULT_AFFECTS_DISPLAY,
  sameAffectsDisplay,
  subscribeAffectsDisplayChanged,
  type AffectsDisplay,
} from '../../ipc/affects';
import { getUiConfig } from '../../ipc/uiConfig';
import { createConfigStore } from './configStore';

// The active profile's affects display for the Affects pane: Style,
// Marker, Tint what to recast, and the hours at which an affect runs out
// and is almost gone, from Settings, Layout, Affects, or a pick in the
// pane's own menu.

const store = createConfigStore<AffectsDisplay>({
  initial: DEFAULT_AFFECTS_DISPLAY,
  read: () => getUiConfig().then(affectsDisplayOf),
  follow: subscribeAffectsDisplayChanged,
  same: sameAffectsDisplay,
});

export const startAffectsDisplayStore = store.start;
export const getAffectsDisplay = store.get;
export const useAffectsDisplay = store.use;

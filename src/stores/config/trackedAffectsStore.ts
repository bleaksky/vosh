import { subscribeTrackedAffectsChanged, type TrackedAffect } from '../../ipc/affects';
import { getUiConfig } from '../../ipc/uiConfig';
import { createConfigStore } from './configStore';

// The active profile's tracked affects, which you list under Tracked
// affects in Settings, Characters.

const store = createConfigStore<TrackedAffect[]>({
  initial: [],
  read: () => getUiConfig().then((cfg) => cfg.tracked_affects ?? []),
  follow: subscribeTrackedAffectsChanged,
});

export const startTrackedAffectsStore = store.start;
export const getTrackedAffects = store.get;
export const useTrackedAffects = store.use;

import { useSyncExternalStore } from 'react';
import {
  getUiConfig,
  subscribeProfileSwitched,
  subscribeTrackedAffectsChanged,
  type TrackedAffect,
} from '../session';
import { createStore } from './store';

// The active profile's tracked affects. Seeded from ui_get_config,
// then kept live by vosh://tracked-affects-changed (Settings saves,
// the backend's profile switch broadcast) and refetched on
// vosh://profile-switched in case the switch lands without a list
// broadcast. Lifted from AffectsBar.

const store = createStore<TrackedAffect[]>([]);
let started = false;
// Bumped by every event. A config fetch applies only when no event
// arrived after it started, so a slow fetch for the old profile cannot
// overwrite the list the switch just delivered.
let generation = 0;

function refetch(): void {
  const mine = ++generation;
  getUiConfig()
    .then((cfg) => {
      if (mine === generation) store.set(cfg.tracked_affects ?? []);
    })
    .catch(() => undefined);
}

export function startTrackedAffectsStore(): void {
  if (started) return;
  started = true;
  refetch();
  void subscribeTrackedAffectsChanged((list) => {
    generation += 1;
    store.set(list);
  });
  void subscribeProfileSwitched(() => refetch());
}

export function getTrackedAffects(): TrackedAffect[] {
  return store.get();
}

export function subscribeTrackedAffects(cb: () => void): () => void {
  startTrackedAffectsStore();
  return store.subscribe(cb);
}

export function useTrackedAffects(): TrackedAffect[] {
  return useSyncExternalStore(subscribeTrackedAffects, getTrackedAffects);
}

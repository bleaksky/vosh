import { useSyncExternalStore } from 'react';
import { subscribeProfileSwitched } from '../../ipc/profiles';
import { getUiConfig, subscribeVitalsDensityChanged, type VitalsDensity } from '../../ipc/uiConfig';
import { createStore } from '../store';

// The active profile's vitals density for the panel footer. Seeded
// from ui_get_config, kept live by vosh://vitals-density-changed (a
// save from Settings, the broadcast after a profile switch), and
// refetched on vosh://profile-switched in case the switch lands
// without one.

const store = createStore<VitalsDensity>('rows');
let started = false;
// Bumped by every event. A config fetch applies only when no event
// arrived after it started, so a slow fetch for the old profile cannot
// overwrite the density the switch just delivered.
let generation = 0;

function refetch(): void {
  const mine = ++generation;
  getUiConfig()
    .then((cfg) => {
      if (mine === generation) store.set(cfg.vitals_density);
    })
    .catch(() => undefined);
}

export function startVitalsDensityStore(): void {
  if (started) return;
  started = true;
  refetch();
  void subscribeVitalsDensityChanged((density) => {
    generation += 1;
    store.set(density);
  });
  void subscribeProfileSwitched(() => refetch());
}

export function getVitalsDensity(): VitalsDensity {
  return store.get();
}

export function subscribeVitalsDensity(cb: () => void): () => void {
  startVitalsDensityStore();
  return store.subscribe(cb);
}

export function useVitalsDensity(): VitalsDensity {
  return useSyncExternalStore(subscribeVitalsDensity, getVitalsDensity);
}

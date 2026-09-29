import { useSyncExternalStore } from 'react';
import {
  DEFAULT_VITALS_OPTIONS,
  getUiConfig,
  subscribeProfileSwitched,
  subscribeVitalsOptionsChanged,
  vitalsOptionsOf,
  type VitalsOptions,
} from '../session';
import { createStore } from './store';

// The active profile's vitals options for the panel footer and the
// status line: Values, Meter, and Warn before you run low from
// Settings, Layout, Vitals. Seeded from ui_get_config, kept live by
// vosh://vitals-options-changed (a save from Settings, the broadcast
// after a profile switch), and refetched on vosh://profile-switched in
// case the switch lands without one. Density keeps its own store.

const store = createStore<VitalsOptions>(DEFAULT_VITALS_OPTIONS);
let started = false;
// Bumped by every event. A config fetch applies only when no event
// arrived after it started, so a slow fetch for the old profile cannot
// overwrite the options the switch just delivered.
let generation = 0;

/** Keep the current snapshot when nothing in it moved, so the footer
 *  and the status line do not render again. */
function put(next: VitalsOptions): void {
  const prev = store.get();
  if (
    prev.values === next.values &&
    prev.meter === next.meter &&
    prev.warn_thirds === next.warn_thirds
  ) {
    return;
  }
  store.set(next);
}

function refetch(): void {
  const mine = ++generation;
  getUiConfig()
    .then((cfg) => {
      if (mine === generation) put(vitalsOptionsOf(cfg));
    })
    .catch(() => undefined);
}

export function startVitalsOptionsStore(): void {
  if (started) return;
  started = true;
  refetch();
  void subscribeVitalsOptionsChanged((options) => {
    generation += 1;
    put(options);
  });
  void subscribeProfileSwitched(() => refetch());
}

export function getVitalsOptions(): VitalsOptions {
  return store.get();
}

export function subscribeVitalsOptions(cb: () => void): () => void {
  startVitalsOptionsStore();
  return store.subscribe(cb);
}

export function useVitalsOptions(): VitalsOptions {
  return useSyncExternalStore(subscribeVitalsOptions, getVitalsOptions);
}

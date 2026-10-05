import { useSyncExternalStore } from 'react';
import { subscribeProfileSwitched } from '../../ipc/profiles';
import {
  DEFAULT_VITALS_OPTIONS,
  getUiConfig,
  subscribeVitalsOptionsChanged,
  vitalsOptionsOf,
  type VitalsOptions,
} from '../../ipc/uiConfig';
import { createStore } from './store';

// The active profile's vitals options for the panel footer and the
// status line: Values, Meter, Warn before you run low, and Hide vitals
// while your prompt is pinned from Settings, Layout, Vitals. Seeded from ui_get_config, kept live by
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
    prev.warn_thirds === next.warn_thirds &&
    prev.hide_when_pinned === next.hide_when_pinned
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

import { useSyncExternalStore } from 'react';
import {
  affectsDisplayOf,
  DEFAULT_AFFECTS_DISPLAY,
  getUiConfig,
  subscribeAffectsDisplayChanged,
  subscribeProfileSwitched,
  type AffectsDisplay,
} from '../session';
import { createStore } from './store';

// The active profile's affects display for the Affects pane: Style,
// Marker, and Tint what to recast from Settings, Layout, Affects, or a
// pick in the pane's own menu. Seeded from ui_get_config, kept live by
// vosh://affects-display-changed (a save from Settings, a menu pick, the
// broadcast after a profile switch), and refetched on
// vosh://profile-switched in case the switch lands without one.

const store = createStore<AffectsDisplay>(DEFAULT_AFFECTS_DISPLAY);
let started = false;
// Bumped by every event. A config fetch applies only when no event
// arrived after it started, so a slow fetch for the old profile cannot
// overwrite a pick made since.
let generation = 0;

/** Keep the current snapshot when nothing in it moved, so the pane does
 *  not render again. */
function put(next: AffectsDisplay): void {
  const prev = store.get();
  if (prev.style === next.style && prev.marker === next.marker && prev.tint === next.tint) {
    return;
  }
  store.set(next);
}

function refetch(): void {
  const mine = ++generation;
  getUiConfig()
    .then((cfg) => {
      if (mine === generation) put(affectsDisplayOf(cfg));
    })
    .catch(() => undefined);
}

export function startAffectsDisplayStore(): void {
  if (started) return;
  started = true;
  refetch();
  void subscribeAffectsDisplayChanged((display) => {
    generation += 1;
    put(display);
  });
  void subscribeProfileSwitched(() => refetch());
}

export function getAffectsDisplay(): AffectsDisplay {
  return store.get();
}

export function subscribeAffectsDisplay(cb: () => void): () => void {
  startAffectsDisplayStore();
  return store.subscribe(cb);
}

export function useAffectsDisplay(): AffectsDisplay {
  return useSyncExternalStore(subscribeAffectsDisplay, getAffectsDisplay);
}

import { useSyncExternalStore } from 'react';
import { subscribeProfileSwitched } from '../../ipc/profiles';
import { getUiConfig, subscribeChipStyleChanged, type ChipStyle } from '../../ipc/uiConfig';
import { createStore } from './store';

// How the status line draws the tick, the game time, and the moons,
// from UiConfig chip_style. Seeded from ui_get_config, then kept live by
// vosh://chip-style-changed, which setUiConfig emits to every window
// when Settings saves and the backend sends after a switch, a #profile
// load or reset, or an import, and refetched on vosh://profile-switched
// since the style is per profile. Lifted from the old useChipStyle hook.

const store = createStore<ChipStyle>('value_only');
let started = false;
// Bumped by every event. A config fetch applies only when no event
// arrived after it started, so a slow fetch cannot put back the style
// a save just replaced.
let generation = 0;

function refetch(): void {
  const mine = ++generation;
  getUiConfig()
    .then((cfg) => {
      if (mine === generation) store.set(cfg.chip_style);
    })
    .catch(() => undefined);
}

export function startChipStyleStore(): void {
  if (started) return;
  started = true;
  refetch();
  void subscribeChipStyleChanged((style) => {
    generation += 1;
    store.set(style);
  });
  void subscribeProfileSwitched(() => refetch());
}

export function getChipStyle(): ChipStyle {
  return store.get();
}

export function subscribeChipStyle(cb: () => void): () => void {
  startChipStyleStore();
  return store.subscribe(cb);
}

export function useChipStyle(): ChipStyle {
  return useSyncExternalStore(subscribeChipStyle, getChipStyle);
}

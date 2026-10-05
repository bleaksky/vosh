import { useSyncExternalStore } from 'react';
import { subscribeProfileSwitched } from '../ipc/profiles';
import { getUiConfig, subscribeTickCountChanged, type TickCount } from '../ipc/uiConfig';
import { createStore } from './store';

// Which way the status line tick counts, from UiConfig tick_count. The
// same wiring as the chip style store: seeded from ui_get_config, kept
// live by vosh://tick-count-changed, which setUiConfig emits to every
// window when Settings saves, and refetched on vosh://profile-switched
// since each character keeps its own.

const store = createStore<TickCount>('up');
let started = false;
// Bumped by every event. A config fetch applies only when no event
// arrived after it started, so a slow fetch cannot put back the count
// a save just replaced.
let generation = 0;

function refetch(): void {
  const mine = ++generation;
  getUiConfig()
    .then((cfg) => {
      if (mine === generation) store.set(cfg.tick_count);
    })
    .catch(() => undefined);
}

export function startTickCountStore(): void {
  if (started) return;
  started = true;
  refetch();
  void subscribeTickCountChanged((count) => {
    generation += 1;
    store.set(count);
  });
  void subscribeProfileSwitched(() => refetch());
}

export function getTickCount(): TickCount {
  return store.get();
}

export function subscribeTickCount(cb: () => void): () => void {
  startTickCountStore();
  return store.subscribe(cb);
}

export function useTickCount(): TickCount {
  return useSyncExternalStore(subscribeTickCount, getTickCount);
}

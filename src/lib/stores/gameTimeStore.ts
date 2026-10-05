import { useSyncExternalStore } from 'react';
import { subscribeProfileSwitched } from '../../ipc/profiles';
import { getUiConfig, subscribeGameTimeChanged, type GameTime } from '../../ipc/uiConfig';
import { createStore } from './store';

// The clock the status line reads the game time on, from UiConfig
// game_time. The same wiring as the tick count store: seeded from
// ui_get_config, kept live by vosh://game-time-changed, which
// setUiConfig emits to every window when Settings saves and the backend
// sends after a switch, a #profile load or reset, or an import, and
// refetched on vosh://profile-switched since each character keeps its
// own.

const store = createStore<GameTime>('24h');
let started = false;
// Bumped by every event. A config fetch applies only when no event
// arrived after it started, so a slow fetch cannot put back the clock
// a save just replaced.
let generation = 0;

function refetch(): void {
  const mine = ++generation;
  getUiConfig()
    .then((cfg) => {
      if (mine === generation) store.set(cfg.game_time);
    })
    .catch(() => undefined);
}

export function startGameTimeStore(): void {
  if (started) return;
  started = true;
  refetch();
  void subscribeGameTimeChanged((clock) => {
    generation += 1;
    store.set(clock);
  });
  void subscribeProfileSwitched(() => refetch());
}

export function getGameTime(): GameTime {
  return store.get();
}

export function subscribeGameTime(cb: () => void): () => void {
  startGameTimeStore();
  return store.subscribe(cb);
}

export function useGameTime(): GameTime {
  return useSyncExternalStore(subscribeGameTime, getGameTime);
}

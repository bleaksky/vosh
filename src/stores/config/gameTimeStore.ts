import { getUiConfig, type GameTime } from '../../ipc/uiConfig';
import { subscribeGameTimeChanged } from '../../ipc/uiConfigEvents';
import { createConfigStore } from './configStore';

// The clock the status line reads the game time on, from UiConfig
// game_time. You pick it under Game time in Settings, Layout, Status
// line, and each character keeps its own.

const store = createConfigStore<GameTime>({
  initial: '24h',
  read: () => getUiConfig().then((cfg) => cfg.game_time),
  follow: subscribeGameTimeChanged,
});

export const startGameTimeStore = store.start;
export const getGameTime = store.get;
export const useGameTime = store.use;

import { getUiConfig, type TickCount } from '../../ipc/uiConfig';
import { subscribeTickCountChanged } from '../../ipc/uiConfigEvents';
import { createConfigStore } from './configStore';

// Which way the status line tick counts, from UiConfig tick_count. You
// pick it under Tick counts in Settings, Layout, Status line, and each
// character keeps its own.

const store = createConfigStore<TickCount>({
  initial: 'up',
  read: () => getUiConfig().then((cfg) => cfg.tick_count),
  follow: subscribeTickCountChanged,
});

export const startTickCountStore = store.start;
export const getTickCount = store.get;
export const useTickCount = store.use;

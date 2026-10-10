import { getUiConfig, type StatusStyle } from '../../ipc/uiConfig';
import { subscribeStatusStyleChanged } from '../../ipc/uiConfigEvents';
import { createConfigStore } from './configStore';

// How the status bar draws, from UiConfig status_style. You pick it
// under Style in Settings, Layout, Status bar, and each profile keeps
// its own.

const store = createConfigStore<StatusStyle>({
  initial: 'meters',
  read: () => getUiConfig().then((cfg) => cfg.status_style),
  follow: subscribeStatusStyleChanged,
});

export const startStatusStyleStore = store.start;
export const getStatusStyle = store.get;
export const useStatusStyle = store.use;

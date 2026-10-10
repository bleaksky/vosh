import { getUiConfig, type PanelSide } from '../../ipc/uiConfig';
import { subscribePanelSideChanged } from '../../ipc/uiConfigEvents';
import { createConfigStore } from './configStore';

// The side the panel sits on, from UiConfig panel_side. You pick it
// under Panel side in Settings, Layout, and each profile keeps its own.

const store = createConfigStore<PanelSide>({
  initial: 'right',
  read: () => getUiConfig().then((cfg) => cfg.panel_side),
  follow: subscribePanelSideChanged,
});

export const startPanelSideStore = store.start;
export const usePanelSide = store.use;

import { getUiConfig, type ChipStyle } from '../../ipc/uiConfig';
import { subscribeChipStyleChanged } from '../../ipc/uiConfigEvents';
import { createConfigStore } from './configStore';

// How the status line draws the tick, the game time, and the moons,
// from UiConfig chip_style. You pick it under Tick and time in
// Settings, Layout, Status line, and each profile keeps its own.

const store = createConfigStore<ChipStyle>({
  initial: 'value_only',
  read: () => getUiConfig().then((cfg) => cfg.chip_style),
  follow: subscribeChipStyleChanged,
});

export const startChipStyleStore = store.start;
export const getChipStyle = store.get;
export const useChipStyle = store.use;

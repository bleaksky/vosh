import { getUiConfig, type StatsStyle } from '../../ipc/uiConfig';
import { subscribeStatsStyleChanged } from '../../ipc/uiConfigEvents';
import { createConfigStore } from './configStore';

// How the Stats pane draws, from UiConfig stats_style. You pick it from
// the pane's own menu or under Style in Settings, Layout, Stats, and
// each profile keeps its own.

const store = createConfigStore<StatsStyle>({
  initial: 'sheet',
  read: () => getUiConfig().then((cfg) => cfg.stats_style),
  follow: subscribeStatsStyleChanged,
});

export const startStatsStyleStore = store.start;
export const getStatsStyle = store.get;
export const useStatsStyle = store.use;

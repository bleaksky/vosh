import { DEFAULT_SNOOP_SHARE, getUiConfig, subscribeUiConfigReplaced } from '../../ipc/uiConfig';
import { createConfigStore } from './configStore';

// The share of the terminal column the snoop split opens at, from
// UiConfig snoop_share, 40 percent until you drag it (Snoop SN7). Each
// profile keeps its own, and a #profile load, reset or import reads it
// again.

const readShare = () => getUiConfig().then((cfg) => cfg.snoop_share);

const store = createConfigStore<number>({
  initial: DEFAULT_SNOOP_SHARE,
  read: readShare,
  follow: (cb) => subscribeUiConfigReplaced(() => void readShare().then(cb, () => undefined)),
});

export const useSnoopShare = store.use;

import {
  DEFAULT_SNOOP_SHARE,
  getUiConfig,
  setUiFields,
  subscribeUiConfigReplaced,
  type UiConfig,
} from '../../ipc/uiConfig';
import { createConfigStore } from './configStore';

// How much of the terminal column the snoop split takes, from UiConfig
// snoop_share, 40 percent until you drag it, and whether it is folded
// to its strip, from snoop_folded (Snoop SN7). Each profile keeps its
// own, and a #profile load, reset or import reads them again. A drag or
// a fold saves both fields alone and the split follows at once.

export interface SnoopSize {
  share: number;
  folded: boolean;
}

const sizeOf = (cfg: UiConfig): SnoopSize => ({
  share: cfg.snoop_share,
  folded: cfg.snoop_folded,
});
const readSize = () => getUiConfig().then(sizeOf);

const store = createConfigStore<SnoopSize>({
  initial: { share: DEFAULT_SNOOP_SHARE, folded: false },
  read: readSize,
  follow: (cb) => subscribeUiConfigReplaced(() => void readSize().then(cb, () => undefined)),
  same: (a, b) => a.share === b.share && a.folded === b.folded,
});

export const useSnoopSize = store.use;

/** Keep `size` for the profile in front. */
export function saveSnoopSize(size: SnoopSize): Promise<void> {
  store.set(size);
  return setUiFields({ snoop_share: size.share, snoop_folded: size.folded });
}

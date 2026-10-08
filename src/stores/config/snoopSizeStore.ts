import {
  DEFAULT_SNOOP_SHARE,
  getUiConfig,
  setUiFields,
  subscribeUiConfigReplaced,
  type UiConfig,
} from '../../ipc/uiConfig';
import { profileInFront, subscribeSessions } from '../session/sessionsStore';
import { createConfigStore } from './configStore';

// How much of the terminal column the snoop split takes, from UiConfig
// snoop_share, 40 percent until you drag it, and whether it is folded
// to its strip, from snoop_folded. Each profile keeps its own, and a
// #profile load, reset or import reads them again. A drag or a fold
// saves both fields alone and the split follows at once.
//
// The store names the profile it read. A session switch moves the
// profile in front at once, but the read of the new one lands later, so
// until the names match the size is still the old profile's.

export interface SnoopSize {
  share: number;
  folded: boolean;
}

/** A size and the profile it is the size of, undefined before the
 *  first read and null for a session that plays none. */
export interface ProfileSnoopSize extends SnoopSize {
  profile: string | null | undefined;
}

const sizeOf = (cfg: UiConfig, profile: string | null): ProfileSnoopSize => ({
  share: cfg.snoop_share,
  folded: cfg.snoop_folded,
  profile,
});
const readSize = () => {
  const profile = profileInFront();
  return getUiConfig(profile).then((cfg) => sizeOf(cfg, profile));
};

/** Hear the profile in front move, as a session switch moves it. */
function followProfile(cb: () => void): void {
  let last = profileInFront();
  subscribeSessions(() => {
    const now = profileInFront();
    if (now === last) return;
    last = now;
    cb();
  });
}

const store = createConfigStore<ProfileSnoopSize>({
  initial: { share: DEFAULT_SNOOP_SHARE, folded: false, profile: undefined },
  read: readSize,
  follow: (cb) => subscribeUiConfigReplaced(() => void readSize().then(cb, () => undefined)),
  same: (a, b) => a.share === b.share && a.folded === b.folded && a.profile === b.profile,
  reread: followProfile,
});

export const useSnoopSize = store.use;

/** Keep `size` for the profile in front. */
export function saveSnoopSize(size: SnoopSize): Promise<void> {
  store.set({ ...size, profile: profileInFront() });
  return setUiFields({ snoop_share: size.share, snoop_folded: size.folded });
}

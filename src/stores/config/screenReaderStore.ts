import {
  DEFAULT_SCREEN_READER,
  screenReaderOf,
  subscribeScreenReaderChanged,
  type ScreenReaderOptions,
} from '../../ipc/screenReader';
import { getUiConfig, subscribeUiConfigReplaced } from '../../ipc/uiConfig';
import { createConfigStore } from './configStore';

// The active profile's screen reader choices for the page that reads
// game lines aloud: Read new game lines, Read in the background, Read
// your prompt and the burst, from Settings, Accessibility, Screen
// reader. A save in Settings sends all four as one, and a profile
// switch, a #profile load or reset and an import read them again.

const store = createConfigStore<ScreenReaderOptions>({
  initial: DEFAULT_SCREEN_READER,
  read: () => getUiConfig().then(screenReaderOf),
  follow: subscribeScreenReaderChanged,
  same: (a, b) =>
    a.screen_reader === b.screen_reader &&
    a.screen_reader_background === b.screen_reader_background &&
    a.screen_reader_prompt === b.screen_reader_prompt &&
    a.screen_reader_burst === b.screen_reader_burst,
  reread: (cb) => void subscribeUiConfigReplaced(cb),
});

export const startScreenReaderStore = store.start;
export const getScreenReader = store.get;
export const useScreenReader = store.use;

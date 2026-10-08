import {
  DEFAULT_LINE_LOOK,
  getUiConfig,
  lineLookOf,
  subscribeInputLineLookChanged,
  type LineLook,
} from '../../ipc/uiConfig';
import { createConfigStore } from './configStore';

// How the command line looks, from Settings, Input: whether the caret
// blinks, its color, the color of what you type, the background and
// the size.

function same(a: LineLook, b: LineLook): boolean {
  return (
    a.blink === b.blink &&
    a.caretColor === b.caretColor &&
    a.textColor === b.textColor &&
    a.background === b.background &&
    a.backgroundColor === b.backgroundColor &&
    a.size === b.size
  );
}

const store = createConfigStore<LineLook>({
  initial: DEFAULT_LINE_LOOK,
  read: () => getUiConfig().then(lineLookOf),
  follow: subscribeInputLineLookChanged,
  same,
});

export const startLineLookStore = store.start;
export const getLineLook = store.get;
export const useLineLook = store.use;

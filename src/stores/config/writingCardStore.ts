import {
  getUiConfig,
  setUiFields,
  subscribeUiConfigReplaced,
  type UiConfig,
} from '../../ipc/uiConfig';
import { createConfigStore } from './configStore';

// Where you put the writing card, how tall you made its box, and whether
// it lives in its pane in the panel, from UiConfig writing_card_left,
// writing_card_top, writing_card_rows and writing_card_pinned. Each
// profile keeps its own, and a #profile load, reset or import reads them
// again. A move, a resize or a pin saves the fields it changes alone and
// the card follows at once.

export interface WritingCardPrefs {
  left: number | null;
  top: number | null;
  rows: number | null;
  pinned: boolean;
  /** False until the first read lands, so the card waits to learn
   *  where it opens. */
  loaded: boolean;
}

const prefsOf = (cfg: UiConfig): WritingCardPrefs => ({
  left: cfg.writing_card_left,
  top: cfg.writing_card_top,
  rows: cfg.writing_card_rows,
  pinned: cfg.writing_card_pinned,
  loaded: true,
});
const readPrefs = () => getUiConfig().then(prefsOf);

const store = createConfigStore<WritingCardPrefs>({
  initial: { left: null, top: null, rows: null, pinned: false, loaded: false },
  read: readPrefs,
  follow: (cb) => subscribeUiConfigReplaced(() => void readPrefs().then(cb, () => undefined)),
  same: (a, b) =>
    a.left === b.left &&
    a.top === b.top &&
    a.rows === b.rows &&
    a.pinned === b.pinned &&
    a.loaded === b.loaded,
});

export const useWritingCardPrefs = store.use;

/** Keep `patch` for the profile in front, and save only what it names. */
export function saveWritingCardPrefs(
  patch: Partial<Omit<WritingCardPrefs, 'loaded'>>,
): Promise<void> {
  store.set({ ...store.get(), ...patch });
  return setUiFields({
    ...('left' in patch ? { writing_card_left: patch.left ?? null } : {}),
    ...('top' in patch ? { writing_card_top: patch.top ?? null } : {}),
    ...('rows' in patch ? { writing_card_rows: patch.rows ?? null } : {}),
    ...('pinned' in patch ? { writing_card_pinned: patch.pinned === true } : {}),
  });
}

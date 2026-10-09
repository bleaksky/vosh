import { getUiConfig } from '../../ipc/uiConfig';
import {
  DEFAULT_TYPE_COLORS,
  subscribeInputTypeColorsChanged,
  typeColorsOf,
  type TypeColors,
} from '../../ipc/uiConfigInput';
import { createConfigStore } from './configStore';

// Color commands as you type and its four colors, from Settings, Input.
// While it is on, the command line colors a line by its first word, and
// the known words store reads what Vosh knows.

function same(a: TypeColors, b: TypeColors): boolean {
  return (
    a.on === b.on &&
    a.alias === b.alias &&
    a.hash === b.hash &&
    a.chat === b.chat &&
    a.unknown === b.unknown
  );
}

const store = createConfigStore<TypeColors>({
  initial: DEFAULT_TYPE_COLORS,
  read: () => getUiConfig().then(typeColorsOf),
  follow: subscribeInputTypeColorsChanged,
  same,
});

export const startTypeColorsStore = store.start;
export const getTypeColors = store.get;
export const subscribeTypeColors = store.subscribe;
export const useTypeColors = store.use;

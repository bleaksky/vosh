import { getUiConfig, subscribeInputLineMarkChanged } from '../../ipc/uiConfig';
import { createConfigStore } from './configStore';

// Use the same mark in the command line, from Settings, Input. While it
// is on, the line you type in starts with the mark your commands echo
// with. Each profile keeps its own.

const store = createConfigStore<boolean>({
  initial: true,
  read: () => getUiConfig().then((cfg) => cfg.input_line_mark),
  follow: subscribeInputLineMarkChanged,
});

export const startLineMarkStore = store.start;
export const getLineMark = store.get;
export const useLineMark = store.use;

import { getUiConfig } from '../../ipc/uiConfig';
import { subscribeInputLineMarkChanged } from '../../ipc/uiConfigEvents';
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

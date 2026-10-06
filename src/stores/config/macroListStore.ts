import { listMacros, subscribeMacrosChanged, type Macro } from '../../ipc/automation';
import { createConfigStore } from './configStore';

// Every macro the active profile holds, yours and the ones presets add.
// The card of a macro preset under Presets reads it to mark the keys one
// of your macros keeps. It loads on first use, since only Settings reads
// it, and follows each list the backend sends after a change.

const store = createConfigStore<Macro[]>({
  initial: [],
  read: listMacros,
  follow: subscribeMacrosChanged,
});

export const useMacroList = store.use;

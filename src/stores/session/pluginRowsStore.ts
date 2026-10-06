import { useSyncExternalStore } from 'react';
import { pluginsList, subscribePluginsChanged, type PluginRow } from '../../ipc/scripts';
import { createStore } from '../store';
import { getSelected, subscribeSelected } from './sessionsStore';

// Your plugins as the session in front sees them, on, off or stopped,
// which tells a Lua pane why it draws nothing. The store reads
// plugins_list for that session as it starts, on each selection and on
// each vosh://plugins-changed. A profile switch sends that event too,
// once the plugins of the next profile run. Null until the first answer,
// and a read applies only when nothing asked again after it, so a slow
// answer for the session behind never lands over the one in front.

const store = createStore<PluginRow[] | null>(null);
let started = false;
/** Counts each read, so only the newest answer applies. */
let asked = 0;

function read(): void {
  const mine = ++asked;
  pluginsList(getSelected())
    .then((rows) => {
      if (mine === asked) store.set(rows);
    })
    .catch(() => undefined);
}

export function startPluginRowsStore(): void {
  if (started) return;
  started = true;
  void subscribePluginsChanged(read);
  subscribeSelected(read);
  read();
}

function subscribe(cb: () => void): () => void {
  startPluginRowsStore();
  return store.subscribe(cb);
}

export const getPluginRows = store.get;

/** Your plugins as the session in front sees them, null until read. */
export function usePluginRows(): PluginRow[] | null {
  return useSyncExternalStore(subscribe, store.get);
}

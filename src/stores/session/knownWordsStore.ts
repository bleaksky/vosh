import { useSyncExternalStore } from 'react';
import { subscribeAliasesChanged, subscribeGroupsChanged } from '../../ipc/automation';
import { inputKnownWords, type KnownWords } from '../../ipc/input';
import { subscribeProfileSwitched } from '../../ipc/profiles';
import { subscribePluginsChanged } from '../../ipc/scripts';
import { getTypeColors, subscribeTypeColors } from '../config/typeColorsStore';
import { createStore } from '../store';
import { getSelected, subscribeSelected } from './sessionsStore';

// The aliases and # commands Vosh knows in the session in front, which
// color what you type. The store reads input_known_words only while
// Color commands as you type is on, so a player who never turns it on
// pays nothing. It reads as coloring turns on, on each selection, and
// on each change to your aliases, groups, plugins or profile, since
// each can add or drop an alias. Null while coloring is off and until
// the first answer, and a read applies only when nothing asked again
// after it, so a slow answer for the session behind never lands over
// the one in front.

const store = createStore<KnownWords | null>(null);
let started = false;
/** Counts each read and each turn off, so only the newest answer
 *  applies. */
let asked = 0;
let on = false;

function read(): void {
  if (!on) return;
  const mine = ++asked;
  inputKnownWords(getSelected())
    .then((words) => {
      if (mine === asked) store.set(words);
    })
    .catch(() => undefined);
}

function follow(): void {
  const now = getTypeColors().on;
  if (now === on) return;
  on = now;
  if (on) {
    read();
  } else {
    asked += 1;
    store.set(null);
  }
}

export function startKnownWordsStore(): void {
  if (started) return;
  started = true;
  void subscribeAliasesChanged(read);
  void subscribeGroupsChanged(read);
  void subscribePluginsChanged(read);
  void subscribeProfileSwitched(() => read());
  subscribeSelected(read);
  subscribeTypeColors(follow);
  follow();
}

function subscribe(cb: () => void): () => void {
  startKnownWordsStore();
  return store.subscribe(cb);
}

export const getKnownWords = store.get;

/** The words Vosh knows in the session in front, null while coloring
 *  is off or until read. */
export function useKnownWords(): KnownWords | null {
  return useSyncExternalStore(subscribe, store.get);
}

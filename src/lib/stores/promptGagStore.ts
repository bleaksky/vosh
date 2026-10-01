import { useSyncExternalStore } from 'react';
import {
  onPromptGagWithoutReader,
  onState,
  promptGagsWithoutReader,
  subscribeProfileSwitched,
  subscribePromptConfigChanged,
  subscribeUiConfigReplaced,
} from '../session';
import { createStore } from './store';

// The triggers that hid your prompt this session while the profile reads
// no prompt, so Vosh drew nothing in its place. The Triggers editor marks
// them. The session names each one once on
// session://prompt-gag-without-reader, and Settings may open later, so
// the store also asks for the list when it starts. A connection that
// opens or closes starts the list over, as the session does. Once the
// profile reads your prompt, or another profile takes over, the session
// forgets them, so the store asks for the list again then and takes it
// whole.

const NONE: ReadonlySet<string> = new Set();

const store = createStore<ReadonlySet<string>>(NONE);
let started = false;
// Bumped by every connect and disconnect. The answer to the ask applies
// only when neither came after it was asked for.
let resets = 0;

function add(names: readonly string[]): void {
  const now = store.get();
  const missing = names.filter((name) => !now.has(name));
  if (missing.length > 0) store.set(new Set([...now, ...missing]));
}

/** Ask the session for the list and take it whole, unless a connection
 *  opened or closed or another ask began meanwhile. */
function reread(): void {
  resets += 1;
  const mine = resets;
  void promptGagsWithoutReader()
    .then((names) => {
      if (mine !== resets) return;
      const now = store.get();
      const same = names.length === now.size && names.every((name) => now.has(name));
      if (!same) store.set(names.length > 0 ? new Set(names) : NONE);
    })
    .catch(() => undefined);
}

export function startPromptGagStore(): void {
  if (started) return;
  started = true;
  const named = onPromptGagWithoutReader(({ trigger }) => {
    if (typeof trigger === 'string' && trigger.length > 0) add([trigger]);
  });
  const states = onState((payload) => {
    if (payload.kind === 'connected') return;
    resets += 1;
    store.set(NONE);
  });
  const changes = [
    subscribePromptConfigChanged(reread),
    subscribeProfileSwitched(reread),
    subscribeUiConfigReplaced(reread),
  ];
  void Promise.all([named, states, ...changes])
    .then(() => {
      const mine = resets;
      return promptGagsWithoutReader().then((names) => {
        if (mine === resets) add(names);
      });
    })
    .catch(() => undefined);
}

export function getPromptGags(): ReadonlySet<string> {
  return store.get();
}

export function subscribePromptGags(cb: () => void): () => void {
  startPromptGagStore();
  return store.subscribe(cb);
}

/** The triggers that hid your prompt this session with nothing drawn in
 *  its place. */
export function usePromptGags(): ReadonlySet<string> {
  return useSyncExternalStore(subscribePromptGags, getPromptGags);
}

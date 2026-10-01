import { useSyncExternalStore } from 'react';
import { onPromptGagWithoutReader, onState, promptGagsWithoutReader } from '../session';
import { createStore } from './store';

// The triggers that hid your prompt this session while the profile reads
// no prompt, so Vosh drew nothing in its place. The Triggers editor marks
// them. The session names each one once on
// session://prompt-gag-without-reader, and Settings may open later, so
// the store also asks for the list when it starts. A connection that
// opens or closes starts the list over, as the session does.

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
  void Promise.all([named, states])
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

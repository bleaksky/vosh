import { useSyncExternalStore } from 'react';
import { onOutput, onState } from '../session';
import { createStore } from './store';

// The prompt the session pinned above the command line, as the text the
// band draws: the output's pin field, decoded. It listens from launch, so
// the band shows the latest prompt even when it mounts after the output
// that carried it, as it does when you choose Pinned. An empty pin and a
// disconnect clear it.

const store = createStore<string | null>(null);
let started = false;

export function startPinnedPromptStore(): void {
  if (started) return;
  started = true;
  const decoder = new TextDecoder('utf-8', { fatal: false });
  void onOutput((out) => {
    if (out.pin) store.set(out.pin.length > 0 ? decoder.decode(out.pin) : null);
  });
  void onState((state) => {
    if (state.kind === 'disconnected') store.set(null);
  });
}

export function getPinnedPrompt(): string | null {
  return store.get();
}

export function subscribePinnedPrompt(cb: () => void): () => void {
  startPinnedPromptStore();
  return store.subscribe(cb);
}

/** The latest pinned prompt, or null before one comes and after you
 *  disconnect. */
export function usePinnedPrompt(): string | null {
  return useSyncExternalStore(subscribePinnedPrompt, getPinnedPrompt);
}

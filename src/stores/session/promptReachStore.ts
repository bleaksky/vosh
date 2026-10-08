import { useSyncExternalStore } from 'react';
import { createStore } from '../store';

// How far past your prompt's widest row the pinned band reaches while the
// prompt card is open, in CSS px. The card's ↵ adds two cells after a row
// that a line break ends, and its caret reaches past the last cell, and
// the band grows to hold both, so neither spills past it. The card
// sets it and the dock reads it. 0 while the card is closed.

const store = createStore(0);

export function setPromptReach(px: number): void {
  store.set(Math.max(0, Math.round(px * 100) / 100));
}

export function getPromptReach(): number {
  return store.get();
}

export function subscribePromptReach(cb: () => void): () => void {
  return store.subscribe(cb);
}

export function usePromptReach(): number {
  return useSyncExternalStore(store.subscribe, store.get);
}

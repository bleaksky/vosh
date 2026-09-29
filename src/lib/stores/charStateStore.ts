import { useSyncExternalStore } from 'react';
import { onGmcpPackage, onState } from '../session';
import { asText, createStore } from './store';

// Your position and spoken language, from Char.State. Aabahran sends
// `{position, language}` each time a prompt would print, the prompt's
// %S and %s. Positions are dead, mortally wounded, incapacitated,
// stunned, meditate, sleeping, resting, sitting, fighting and standing.
// Nothing shows it yet. The prompt editor reads it later.

export interface CharState {
  position: string | null;
  language: string | null;
}

/** Parse a Char.State payload. null when it is not an object. */
export function parseCharState(data: unknown): CharState | null {
  if (!data || typeof data !== 'object' || Array.isArray(data)) return null;
  const d = data as Record<string, unknown>;
  return { position: asText(d.position), language: asText(d.language) };
}

const store = createStore<CharState | null>(null);
let started = false;

export function startCharStateStore(): void {
  if (started) return;
  started = true;
  void onGmcpPackage<unknown>('Char.State', (data) => {
    const next = parseCharState(data);
    if (!next) return;
    const prev = store.get();
    // It rides every prompt, so skip the ones that repeat.
    if (prev && prev.position === next.position && prev.language === next.language) return;
    store.set(next);
  });
  void onState((payload) => {
    if (payload.kind === 'disconnected') store.set(null);
  });
}

export function getCharState(): CharState | null {
  return store.get();
}

export function subscribeCharState(cb: () => void): () => void {
  startCharStateStore();
  return store.subscribe(cb);
}

/** Your position and language, or null until the game sends them. */
export function useCharState(): CharState | null {
  return useSyncExternalStore(subscribeCharState, getCharState);
}

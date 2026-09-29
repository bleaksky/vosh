import { useSyncExternalStore } from 'react';
import { onGmcpPackage, onState } from '../session';
import { createStore } from './store';

// Your prompt settings in the game, from Char.Prompt. Aabahran sends
// `{enabled, prompt, fprompt}` at login and whenever you change or show
// your prompt or fight prompt. The text is the raw format string with
// its colour codes, so Vosh can recognise the prompt line without you
// pasting it. Nothing shows it yet. The prompt editor reads it later.

export interface GamePrompt {
  /** False after `prompt off`. The game still sends the prompt time
   *  packages then. */
  enabled: boolean;
  /** The format string, like `%n%P%C<%hhp %mm %vmv> `. */
  prompt: string;
  /** The fight prompt, used in place of `prompt` while you fight.
   *  Empty when unset. */
  fprompt: string;
}

/** Parse a Char.Prompt payload. null when it is not an object. The
 *  strings stay exactly as sent, trailing spaces included. */
export function parseGamePrompt(data: unknown): GamePrompt | null {
  if (!data || typeof data !== 'object' || Array.isArray(data)) return null;
  const d = data as Record<string, unknown>;
  return {
    enabled: typeof d.enabled === 'boolean' ? d.enabled : true,
    prompt: typeof d.prompt === 'string' ? d.prompt : '',
    fprompt: typeof d.fprompt === 'string' ? d.fprompt : '',
  };
}

const store = createStore<GamePrompt | null>(null);
let started = false;

export function startGamePromptStore(): void {
  if (started) return;
  started = true;
  void onGmcpPackage<unknown>('Char.Prompt', (data) => {
    const next = parseGamePrompt(data);
    if (next) store.set(next);
  });
  void onState((payload) => {
    if (payload.kind === 'disconnected') store.set(null);
  });
}

export function getGamePrompt(): GamePrompt | null {
  return store.get();
}

export function subscribeGamePrompt(cb: () => void): () => void {
  startGamePromptStore();
  return store.subscribe(cb);
}

/** Your prompt settings in the game, or null until it sends them. */
export function useGamePrompt(): GamePrompt | null {
  return useSyncExternalStore(subscribeGamePrompt, getGamePrompt);
}

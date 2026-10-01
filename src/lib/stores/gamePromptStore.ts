import { useSyncExternalStore } from 'react';
import { onGmcpPackage, onState } from '../session';
import { createStore } from './store';

// Your prompt settings in the game, from Char.Prompt. Aabahran sends
// `{enabled, prompt, fprompt}` at login and whenever you change or show
// your prompt or fight prompt. The text is the raw format string with
// its colour codes, so Vosh can recognise the prompt line without you
// pasting it. The prompt engine in the backend keeps its own copy to read
// your prompt. The prompt card and Settings read this one for the codes,
// `enabled`, and when the game sent them, which their source line names
// (the game sent them when you logged in, or at 12:58). While it holds
// nothing they ask the backend with prompt_last_seen.

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

/** The latest Char.Prompt, with when it came. */
export interface GamePromptSeen extends GamePrompt {
  /** When it came, in milliseconds since the epoch. */
  receivedAt: number;
  /** It is the first one since you connected, which the game sends at
   *  login. False for any later one, and for one that came before this
   *  window heard you connect. */
  atLogin: boolean;
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

const store = createStore<GamePromptSeen | null>(null);
let started = false;
// No Char.Prompt has come since this window heard you connect. It starts
// false, so a window that loads while you play never calls a packet the
// login one.
let awaitingLogin = false;

export function startGamePromptStore(): void {
  if (started) return;
  started = true;
  void onGmcpPackage<unknown>('Char.Prompt', (data) => {
    const next = parseGamePrompt(data);
    if (!next) return;
    store.set({ ...next, receivedAt: Date.now(), atLogin: awaitingLogin });
    awaitingLogin = false;
  });
  void onState((payload) => {
    if (payload.kind === 'disconnected') {
      store.set(null);
      awaitingLogin = false;
    } else {
      // The session says it is connecting before it reads anything, so
      // the next packet is the one the game sends at login.
      awaitingLogin = true;
    }
  });
}

export function getGamePrompt(): GamePromptSeen | null {
  return store.get();
}

export function subscribeGamePrompt(cb: () => void): () => void {
  startGamePromptStore();
  return store.subscribe(cb);
}

/** Your prompt settings in the game with when they came, or null until it
 *  sends them. */
export function useGamePrompt(): GamePromptSeen | null {
  return useSyncExternalStore(subscribeGamePrompt, getGamePrompt);
}

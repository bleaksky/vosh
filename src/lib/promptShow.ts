// Where your prompt shows, as the active profile's [prompt] table says,
// and whether the profile reads a prompt at all. The Settings row, the
// palette and the main window read it from prompt_show_get, and read it
// again whenever something could have changed it: a command or a save
// that changed the table, a profile switch, the game sending your
// prompt settings, a change in whether Vosh reads your prompt, and a
// connect or disconnect.

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  normalizePromptShow,
  onGamePromptSeen,
  onState,
  subscribeProfileSwitched,
  subscribePromptConfigChanged,
  type PromptShow,
} from './session';

export interface PromptShowState {
  show: PromptShow;
  /** The profile has a capture that reads a prompt. */
  capture: boolean;
  /** The game sent Char.Prompt this session. */
  gameSent: boolean;
  /** The rows the pinned band keeps, the most any prompt the capture
   *  reads can take. */
  zone: number;
  /** You turned prompts off in the game. */
  promptsOff: boolean;
}

interface RawPromptShowState {
  show?: unknown;
  capture?: unknown;
  game_sent?: unknown;
  zone?: unknown;
  prompts_off?: unknown;
}

/** A state from what prompt_show_get returned, the text and nothing
 *  read for anything it did not say. */
export function normalizePromptShowState(raw: RawPromptShowState | null): PromptShowState {
  const zone = typeof raw?.zone === 'number' && Number.isFinite(raw.zone) ? raw.zone : 1;
  return {
    show: normalizePromptShow(raw?.show),
    capture: raw?.capture === true,
    gameSent: raw?.game_sent === true,
    zone: Math.min(6, Math.max(1, Math.round(zone))),
    promptsOff: raw?.prompts_off === true,
  };
}

export async function promptShowGet(): Promise<PromptShowState> {
  return normalizePromptShowState(await invoke<RawPromptShowState | null>('prompt_show_get'));
}

/** Call `cb` on everything that can change where your prompt shows or
 *  whether the profile reads one. Returns the unsubscribe. */
export function subscribePromptShowChanges(cb: () => void): () => void {
  const unlisteners: (() => void)[] = [];
  let closed = false;
  const keep = (p: Promise<() => void>) => {
    void p.then((un) => {
      if (closed) un();
      else unlisteners.push(un);
    });
  };
  keep(subscribePromptConfigChanged(cb));
  keep(subscribeProfileSwitched(() => cb()));
  keep(onGamePromptSeen(() => cb()));
  keep(onState(() => cb()));
  keep(listen<unknown>('session://prompt-status', () => cb()));
  return () => {
    closed = true;
    for (const un of unlisteners) un();
  };
}

/** Where your prompt shows, or null until the first read lands. */
export function usePromptShow(): PromptShowState | null {
  const [state, setState] = useState<PromptShowState | null>(null);
  useEffect(() => {
    let alive = true;
    const read = () => {
      promptShowGet()
        .then((next) => {
          if (alive) setState(next);
        })
        .catch(() => {});
    };
    read();
    const unsubscribe = subscribePromptShowChanges(read);
    return () => {
      alive = false;
      unsubscribe();
    };
  }, []);
  return state;
}

/** The sentence under the Settings row for each place. */
export const PROMPT_SHOW_HELP: Record<PromptShow, string> = {
  text: 'Your prompt shows in the text, where the game sends it.',
  lifted:
    'Each prompt stays in the text on a raised band, so your prompts stand apart from the game.',
  pinned:
    'Only your latest prompt shows, on a band above the command line. Earlier prompts leave the text, and Prompts triggers still see every one.',
};

/** Why the row is off while the profile reads no prompt: the Draw row's
 *  sentence in the same state. */
export function promptShowDisabledHelp(gameSent: boolean): string {
  return gameSent ? 'Customize your prompt first.' : "Tell Vosh your game's prompt first.";
}

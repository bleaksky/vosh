// Where your prompt shows, as the active profile's [prompt] table says,
// and whether the profile reads a prompt at all. The Settings row, the
// palette and the main window read it from prompt_show_get, and read it
// again whenever something could have changed it: a command or a save
// that changed the table, a profile switch, the game sending your
// prompt settings, a change in whether Vosh reads your prompt, and a
// connect or disconnect.

import { useEffect, useState } from 'react';
import { subscribeProfileSwitched } from '../ipc/profiles';
import {
  onGamePromptSeen,
  onPromptStatus,
  promptShowGet,
  subscribePromptConfigChanged,
  type PromptShow,
  type PromptShowState,
} from '../ipc/prompt';
import { onState } from '../ipc/session';

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
  keep(onPromptStatus(() => cb()));
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

/** What each place reads as, in the Settings row and in the menu of
 *  Customize prompt. */
export const PROMPT_SHOW_LABELS: Record<PromptShow, string> = {
  text: 'In the text',
  lifted: 'Lifted',
  pinned: 'Pinned',
};

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

/** Whether you can pick a place now. Not until Vosh knows whether the
 *  profile reads a prompt, and not while it reads none, which `why`
 *  explains. The Settings row and the card's button both follow it. */
export function promptShowLock(state: PromptShowState | null): {
  locked: boolean;
  why: string | null;
} {
  if (state === null) return { locked: true, why: null };
  if (!state.capture) return { locked: true, why: promptShowDisabledHelp(state.gameSent) };
  return { locked: false, why: null };
}

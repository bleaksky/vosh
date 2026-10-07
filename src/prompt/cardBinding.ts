// What the prompt card edits and where it saves it: your prompt's
// `[prompt]` table.

import type { UnlistenFn } from '@tauri-apps/api/event';
import {
  promptCardOpen,
  promptConfigGet,
  promptConfigSet,
  subscribePromptConfigChanged,
  type PromptConfig,
} from '../ipc/prompt';

/** The table the card works on, for one session. */
export interface CardBinding {
  /** Read the table as the card opens. */
  open: (session: number) => Promise<PromptConfig>;
  /** Hear the table change outside the card, and hand `take` the table
   *  as it now stands. */
  follow: (session: number, take: (next: PromptConfig) => void) => Promise<UnlistenFn>;
  /** Save a table the card changed. `asIs` keeps a design exactly as
   *  sent, as Start empty does. */
  write: (next: PromptConfig, options: { asIs: boolean; session: number }) => Promise<void>;
}

/** Your prompt. Opening keeps the design the card found among the
 *  earlier designs, and a `#prompt`, a Settings save or another window
 *  changes the table. */
export const PROMPT_BINDING: CardBinding = {
  open: promptCardOpen,
  follow: (session, take) =>
    subscribePromptConfigChanged(() => {
      void promptConfigGet(session)
        .then(take)
        .catch(() => {});
    }),
  write: (next, options) => promptConfigSet(next, options),
};

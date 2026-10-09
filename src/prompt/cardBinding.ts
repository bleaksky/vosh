// What the prompt card edits and where it saves it: your prompt's
// `[prompt]` table, or your vitals text, which the
// card titled Your vitals text edits as a design of its own.

import type { UnlistenFn } from '@tauri-apps/api/event';
import {
  normalizePromptShow,
  promptCardOpen,
  promptConfigGet,
  promptConfigSet,
  subscribePromptConfigChanged,
  type PromptConfig,
} from '../ipc/prompt';
import { fetchUiConfig, setUiFields } from '../ipc/uiConfig';
import {
  normalizeVitalsTextPrevious,
  subscribeVitalsTextChanged,
  type VitalsTextChange,
} from '../ipc/uiConfigVitals';
import { broadcastVitalsText } from '../ipc/uiConfigBroadcast';
import { startVitalsText } from '../ipc/vitals';

/** The table the card works on, for one session. */
export interface CardBinding {
  /** Your prompt, or your vitals text. */
  kind: 'prompt' | 'vitals';
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
  kind: 'prompt',
  open: promptCardOpen,
  follow: (session, take) =>
    subscribePromptConfigChanged(() => {
      void promptConfigGet(session)
        .then(take)
        .catch(() => {});
    }),
  write: (next, options) => promptConfigSet(next, options),
};

/** Your vitals text as the card's table: the text, or while you have
 *  none the one Text starts from, your 0.7 template `legacy` or Vosh's,
 *  drawn and yours, reading no prompt. `earlier` are the earlier texts,
 *  newest first, which its Presets offer as Yours and Your text before
 *  that. */
export function vitalsTable(
  text: string,
  earlier: readonly string[],
  legacy: string | null,
): PromptConfig {
  return {
    draw: true,
    template: text || startVitalsText(legacy),
    previous_templates: normalizeVitalsTextPrevious(earlier),
    capture: { kind: 'none' },
    show: normalizePromptShow(null),
    mirror: false,
  };
}

/** What the card saves of `table`: the text, empty for the one Text
 *  starts from so it keeps following that one, and the earlier texts it opened with. The setter
 *  puts the text it replaces among the earlier ones, so the earlier
 *  texts go after it and keep the text you opened with as Yours, as the
 *  prompt card keeps the design it opened with, rather than each step
 *  of an edit. */
export function vitalsTextSave(table: PromptConfig, legacy: string | null): VitalsTextChange {
  return {
    vitals_text: table.template === startVitalsText(legacy) ? '' : table.template,
    vitals_text_previous: table.previous_templates,
  };
}

/** Your vitals text, for the selected session's profile. Opening puts
 *  the text you open with first among the earlier ones, and Reset to
 *  default in Settings or another profile changes it. Each save tells
 *  every window, so Customize vitals shows it. */
export const VITALS_TEXT_BINDING: CardBinding = {
  kind: 'vitals',
  open: async () => {
    const config = await fetchUiConfig();
    return vitalsTable(
      config.vitals_text,
      [config.vitals_text, ...config.vitals_text_previous],
      config.vitals_legacy_text,
    );
  },
  follow: (_session, take) =>
    subscribeVitalsTextChanged(() => {
      void fetchUiConfig()
        .then((config) =>
          take(
            vitalsTable(config.vitals_text, config.vitals_text_previous, config.vitals_legacy_text),
          ),
        )
        .catch(() => {});
    }),
  write: async (next) => {
    const change = vitalsTextSave(next, await vitalsLegacyText());
    await setUiFields(change);
    await broadcastVitalsText(change);
  },
};

/** Your 0.7 template in today's codes, while it was on, for the vitals
 *  text card's Presets, or null. */
export async function vitalsLegacyText(): Promise<string | null> {
  return (await fetchUiConfig()).vitals_legacy_text;
}

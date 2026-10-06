import type { UnlistenFn } from '@tauri-apps/api/event';
import { subscribeTickConfigChanged, type TickConfig } from '../ipc/tick';
import { subscribeUiConfigReplaced } from '../ipc/uiConfig';

/** What the Settings Tick card does when the tick settings change
 *  outside it. */
export interface TickDraftFollow {
  /** Read the tick settings again, dropping any unsaved changes. */
  reload: () => void;
  /** Take settings a #tick command or another window changed. */
  adopt: (config: TickConfig) => void;
  /** Whether the card keeps what it shows: it holds unsaved changes, or
   *  Settings holds another profile than the one in front. */
  keeps: () => boolean;
}

/** Keep the Tick card on the tick settings of the profile in front. A
 *  profile switch, #profile load, #profile reset, or an import replaces
 *  the whole profile, and a change from elsewhere, like a #tick command
 *  or a save from another window, changes the tick. The card reads or
 *  takes either only while it keeps nothing, so unsaved changes stay
 *  with the profile they were made on, which a login switch can replace
 *  before Settings hears that it moved. */
export async function followTickDraft(follow: TickDraftFollow): Promise<UnlistenFn> {
  const stops = await Promise.all([
    subscribeUiConfigReplaced(() => {
      if (!follow.keeps()) follow.reload();
    }),
    subscribeTickConfigChanged((config) => {
      if (!follow.keeps()) follow.adopt(config);
    }),
  ]);
  return () => {
    for (const stop of stops) stop();
  };
}

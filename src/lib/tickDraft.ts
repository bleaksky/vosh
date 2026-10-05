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
  /** Whether the card holds unsaved changes. */
  isDirty: () => boolean;
}

/** Keep the Tick card on the live profile's tick settings. A profile
 *  switch, #profile load, #profile reset, or an import replaces the
 *  whole profile, so the card reads the new settings even over unsaved
 *  changes, since its Save sends every setting and would write the old
 *  profile's tick back. A change from elsewhere, like a #tick command
 *  or a save from another window, lands only while the card is clean. */
export async function followTickDraft(follow: TickDraftFollow): Promise<UnlistenFn> {
  const stops = await Promise.all([
    subscribeUiConfigReplaced(() => follow.reload()),
    subscribeTickConfigChanged((config) => {
      if (!follow.isDirty()) follow.adopt(config);
    }),
  ]);
  return () => {
    for (const stop of stops) stop();
  };
}

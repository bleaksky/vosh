// The active profile's pane layout, which the backend saves and sends
// when it changes outside this window, and the reset of any profile's
// panes. lib/paneLayout.ts sanitizes every tree it reads. Each call
// returns the Tauri promise as it is.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { PaneLayout } from '../lib/paneLayout';
import { PANE_LAYOUT_CHANGED } from './events';

/** The active profile's pane layout, as the backend sends it. */
export function paneLayoutGet(): Promise<unknown> {
  return invoke<unknown>('pane_layout_get');
}

/** Save the active profile's pane layout, with the generation the
 *  edited tree was read at. False when the backend refused it, since the
 *  profile it came from has been swapped out since. */
export function paneLayoutSet(save: {
  layout: PaneLayout;
  generation: number | null;
}): Promise<boolean> {
  return invoke<boolean>('pane_layout_set', { ...save });
}

/** Put a profile's panes back to the stock map over affects tree,
 *  keeping whether its panel shows and how wide it is. Returns the new
 *  layout as the backend sends it. The live profile saves it at once and
 *  every window hears it through vosh://pane-layout-changed. */
export function paneLayoutReset(profile?: string | null): Promise<unknown> {
  return invoke<unknown>('pane_layout_reset', { profile: profile ?? null });
}

/** Hear a pane layout saved outside this window. */
export function subscribePaneLayoutChanged(cb: (payload: unknown) => void): Promise<UnlistenFn> {
  return listen<unknown>(PANE_LAYOUT_CHANGED, (event) => cb(event.payload));
}

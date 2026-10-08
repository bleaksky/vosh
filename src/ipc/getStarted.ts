// Get started, the short list a new install opens on. profiles.toml keeps
// where you are in it once for the whole install, and Help opens it
// again in the main window.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { GET_STARTED_OPEN } from './events';

/** Where you are in Get started. */
export interface GetStartedState {
  /** The card opens at launch. */
  atLaunch: boolean;
  /** The steps you finished, by id. */
  done: string[];
}

/** Where you are in Get started, or null when it never opened. */
export function getStartedGet(): Promise<GetStartedState | null> {
  return invoke('get_started_get');
}

/** Keep whether the card opens at launch and the steps you finished. */
export function getStartedSet(state: GetStartedState): Promise<void> {
  return invoke('get_started_set', { atLaunch: state.atLaunch, done: state.done });
}

/** Bring the main window forward and open Get started in it. */
export function openGetStarted(): Promise<void> {
  return invoke('open_get_started');
}

/** Hear Help open Get started. Main window only. */
export function subscribeGetStartedOpen(cb: () => void): Promise<UnlistenFn> {
  return listen(GET_STARTED_OPEN, () => cb());
}

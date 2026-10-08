// The loadouts and which of them are active.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { LOADOUTS_CHANGED } from './events';

// Path B loadout state for the Settings UI. `path_b_active` is the
// flag the frontend reads to decide whether to render the Loadouts
// tab at all; in legacy mode it returns false and empty lists.
export interface LoadoutSummary {
  name: string;
  description?: string | null;
  enabled_groups: string[];
}

export interface LoadoutsState {
  path_b_active: boolean;
  active: string[];
  loadouts: LoadoutSummary[];
}

/** Loadout mode and a profile's stack of active loadouts, the selected
 *  session's profile's when it names none. */
export async function loadoutsGetState(profile?: string | null): Promise<LoadoutsState> {
  return invoke('loadouts_get_state', { profile });
}

// Replace a profile's active-loadouts list. The backend recomputes every
// store's disabled_groups, persists the loadout set, and emits
// vosh://loadouts-changed so other consumers see the update.
export async function loadoutsSetActive(active: string[], profile?: string | null): Promise<void> {
  return invoke('loadouts_set_active', { active, profile });
}

export async function subscribeLoadoutsChanged(cb: () => void): Promise<UnlistenFn> {
  return listen(LOADOUTS_CHANGED, () => cb());
}

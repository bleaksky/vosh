// The loadouts and which of them are active.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

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

export async function loadoutsGetState(): Promise<LoadoutsState> {
  return invoke('loadouts_get_state');
}

// Replace the active-loadouts list. The backend recomputes every
// store's disabled_groups, persists the loadout set, and emits
// vosh://loadouts-changed so other consumers see the update.
export async function loadoutsSetActive(active: string[]): Promise<void> {
  return invoke('loadouts_set_active', { active });
}

export async function subscribeLoadoutsChanged(cb: () => void): Promise<UnlistenFn> {
  return listen('vosh://loadouts-changed', () => cb());
}

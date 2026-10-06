// The profile list, creating, renaming and switching profiles, and the
// scope of each setting.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { PROFILES_CHANGED, PROFILE_SWITCHED } from './events';

// Named profile catalog.
export interface ProfileAutoMatch {
  host?: string | null;
  port?: number | null;
  /** Character names the profile claims. Any-of matching: a connect
   *  call carrying any one of these names triggers a profile switch.
   *  Empty list means the profile matches on host/port alone.
   *  The legacy single-string `character` field is accepted by the
   *  backend on load and promoted to a one-element list, so older
   *  profiles keep working without manual migration. */
  characters?: string[];
  /** The login toggle. False keeps the world and the names but stops
   *  the profile from matching at connect or login. The backend leaves
   *  it out while it is on, so absent means on. */
  enabled?: boolean;
}

export interface ProfileEntry {
  name: string;
  description?: string | null;
  auto_match?: ProfileAutoMatch | null;
}

export interface ProfilesList {
  active: string;
  profiles: ProfileEntry[];
}

export async function profilesList(): Promise<ProfilesList> {
  return invoke('profiles_list');
}

export async function profileDelete(name: string): Promise<void> {
  return invoke('profile_delete', { name });
}

export async function profileRename(oldName: string, newName: string): Promise<void> {
  return invoke('profile_rename', { old: oldName, new: newName });
}

export async function profileDuplicate(source: string, newName: string): Promise<void> {
  return invoke('profile_duplicate', { source, new: newName });
}

/** Keep the profile `profile` names open while a Settings page holds
 *  unsaved edits on it, or let go with null after Save or Discard. */
export async function profileHoldEdits(profile: string | null): Promise<void> {
  return invoke('profile_hold_edits', { profile });
}

/** Switch a session to the profile `name`, the selected session when it
 *  names none. */
export async function profileSwitch(name: string, session?: number): Promise<void> {
  return invoke('profile_switch', { name, session });
}

export async function profileResolveMatch(
  host: string,
  port: number,
  character: string | null,
): Promise<string | null> {
  return invoke('profile_resolve_match', { host, port, character });
}

/** The profile a new session on `host` and `port` starts on before
 *  anyone logs in, one pinned to that host and port, then one that
 *  claims the host on any port, or null to keep the profile in front. A
 *  claim that names characters counts too (Sessions Q2). */
export async function profileBeforeLogin(host: string, port: number): Promise<string | null> {
  return invoke('profile_resolve_match', { host, port, character: null, anyCharacter: true });
}

/** Create a profile, starting as a copy of `copyFrom` when given, with
 *  `autoMatch` as its login claim. The claim takes nothing from other
 *  profiles, so follow with profileSetLogin to own the character. Does
 *  not switch. */
export async function profileCreate(
  name: string,
  copyFrom?: string | null,
  autoMatch?: ProfileAutoMatch | null,
): Promise<ProfileEntry> {
  return invoke('profile_create', {
    name,
    copyFrom: copyFrom ?? null,
    autoMatch: autoMatch ?? null,
  });
}

// Per-category scope toggle (Profile vs Global).
export type ProfileScope = 'profile' | 'global';

export interface ScopeConfig {
  theme: ProfileScope;
  font: ProfileScope;
  dock_layout: ProfileScope;
  keep_last_command: ProfileScope;
  auto_update: ProfileScope;
}

export async function profileGetScope(): Promise<ScopeConfig> {
  return invoke('profile_get_scope');
}

export async function profileSetScope(scope: ScopeConfig): Promise<void> {
  return invoke('profile_set_scope', { scope });
}

export async function subscribeProfilesChanged(
  cb: (changedName: string) => void,
): Promise<UnlistenFn> {
  return listen<string>(PROFILES_CHANGED, (event) => {
    cb(event.payload);
  });
}

export async function subscribeProfileSwitched(
  cb: (newActive: string) => void,
): Promise<UnlistenFn> {
  return listen<string>(PROFILE_SWITCHED, (event) => {
    cb(event.payload);
  });
}
